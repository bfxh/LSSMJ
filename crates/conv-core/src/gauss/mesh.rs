//! 标量场 → 网格：Surface Nets（fast-surface-nets 0.2.1）+ 统计 + OBJ 导出。
//!
//! 顶点位置由插值给出（数组局部坐标，单位为格），这里统一换算回世界坐标。
//! 绕序以有符号体积校正为朝外；法线取库的 SDF 中心差分结果并按同一符号翻转。

use super::Aabb;
use super::voxel::{Field, Grid};
use crate::mesh_hash;
use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

/// 乘-旋散列：整数键比默认 SipHash 快一个量级（集合只做 insert/len，不迭代 ⇒ 顺序无关）。
#[derive(Default)]
pub(crate) struct FxHasher {
    hash: u64,
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        // 数组键（[u32;4]）走 Hash::hash_slice ⇒ 全是字节流，按 8 字节混一次即可。
        let (chunks, rem) = bytes.as_chunks::<8>();
        for c in chunks {
            self.write_u64(u64::from_le_bytes(*c));
        }
        if !rem.is_empty() {
            let mut b = [0u8; 8];
            b[..rem.len()].copy_from_slice(rem);
            self.write_u64(u64::from_le_bytes(b));
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.hash = (self.hash.rotate_left(5) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64);
    }
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub(crate) type FxSet<T> = HashSet<T, BuildHasherDefault<FxHasher>>;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default)]
pub struct MeshStats {
    pub vertices: usize,
    pub triangles: usize,
    pub components: usize,
    pub chi: i64,
    pub volume: f32,
    pub boundary_touch: bool,
    pub hash: u64,
    pub bounds: Option<Aabb>,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub stats: MeshStats,
}

/// GPU Surface Nets（`crate::gsn`）网格化。四边形槽位在 GPU 上按 atomicAdd 写入、顺序不定，
/// 这里按三角形规范序排序后再算统计/哈希 ⇒ 同输入逐位一致。
pub fn field_to_mesh_gpu(hd: &crate::jfa::Headless, field: &Field) -> Result<Mesh, String> {
    let gsn = crate::gsn::surface_nets_gpu(hd, &field.values, field.grid.n, None)?;
    Ok(mesh_from_gsn(&gsn, &field.grid))
}

fn mesh_from_gsn(gsn: &crate::gsn::GsnMesh, grid: &Grid) -> Mesh {
    let mut used = vec![false; gsn.positions.len()];
    for &slot in &gsn.indices {
        used[slot as usize] = true;
    }
    let mut remap = vec![u32::MAX; gsn.positions.len()];
    let mut positions = Vec::new();
    for (slot, is_used) in used.iter().enumerate() {
        if *is_used {
            remap[slot] = positions.len() as u32;
            let p = gsn.positions[slot];
            positions.push([
                grid.origin[0] + p[0] * grid.h,
                grid.origin[1] + p[1] * grid.h,
                grid.origin[2] + p[2] * grid.h,
            ]);
        }
    }
    // 压缩顺序 = 槽位升序（与 atomic 写入序无关）。
    let mut indices: Vec<u32> = gsn
        .indices
        .iter()
        .map(|&slot| remap[slot as usize])
        .collect();
    // 绕序保留，只旋转到最小索引开头；再按字典序排三角形。
    for t in indices.as_chunks_mut::<3>().0 {
        if t[1] < t[0] && t[1] <= t[2] {
            t.rotate_left(1);
        } else if t[2] < t[0] && t[2] < t[1] {
            t.rotate_left(2);
        }
    }
    indices.as_chunks_mut::<3>().0.sort_unstable();

    let mut normals = vec![[0.0f32; 3]; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let n = face_normal(
            positions[t[0] as usize],
            positions[t[1] as usize],
            positions[t[2] as usize],
        );
        for &v in t {
            let acc = &mut normals[v as usize];
            acc[0] += n[0];
            acc[1] += n[1];
            acc[2] += n[2];
        }
    }
    for n in &mut normals {
        *n = super::normalize_vec(*n);
    }
    finish_mesh(positions, normals, indices, grid)
}

/// 只保留三角形数最多的连通分量（同数取最小根，确定性），压缩顶点并重算统计/哈希。
pub fn keep_largest_component(mesh: &Mesh, grid: &Grid) -> Mesh {
    let n = mesh.positions.len();
    if n == 0 || mesh.indices.is_empty() {
        return finish_mesh(Vec::new(), Vec::new(), Vec::new(), grid);
    }
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    for t in mesh.indices.as_chunks::<3>().0 {
        for e in 0..3 {
            let a = find(&mut parent, t[e]);
            let b = find(&mut parent, t[(e + 1) % 3]);
            if a != b {
                parent[a as usize] = b;
            }
        }
    }
    let mut tri_count = vec![0usize; n];
    for t in mesh.indices.as_chunks::<3>().0 {
        let r = find(&mut parent, t[0]) as usize;
        tri_count[r] += 1;
    }
    let mut best = 0u32;
    let mut best_count = 0usize;
    for (i, &c) in tri_count.iter().enumerate() {
        if c > best_count {
            best_count = c;
            best = i as u32;
        }
    }
    if best_count == 0 {
        return finish_mesh(Vec::new(), Vec::new(), Vec::new(), grid);
    }
    let mut remap = vec![u32::MAX; n];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::with_capacity(mesh.indices.len());
    for t in mesh.indices.as_chunks::<3>().0 {
        if find(&mut parent, t[0]) != best {
            continue;
        }
        for &v in t {
            let mut k = remap[v as usize];
            if k == u32::MAX {
                k = positions.len() as u32;
                remap[v as usize] = k;
                positions.push(mesh.positions[v as usize]);
                normals.push(
                    mesh.normals
                        .get(v as usize)
                        .copied()
                        .unwrap_or([0.0, 1.0, 0.0]),
                );
            }
            indices.push(k);
        }
    }
    finish_mesh(positions, normals, indices, grid)
}

/// 供稀疏分块等外部网格器使用：统一走同一套绕序/统计/哈希口径。
pub(crate) fn mesh_from_parts(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    grid: &Grid,
) -> Mesh {
    finish_mesh(positions, normals, indices, grid)
}

fn finish_mesh(
    positions: Vec<[f32; 3]>,
    mut normals: Vec<[f32; 3]>,
    mut indices: Vec<u32>,
    grid: &Grid,
) -> Mesh {
    if positions.is_empty() {
        indices.clear();
    }
    let mut vol = signed_volume(&positions, &indices);
    if vol < 0.0 {
        for t in indices.as_chunks_mut::<3>().0 {
            t.swap(1, 2);
        }
        for v in &mut normals {
            *v = [-v[0], -v[1], -v[2]];
        }
        vol = -vol;
    }
    if normals.len() < positions.len() {
        normals.resize(positions.len(), [0.0, 1.0, 0.0]);
    }
    normals.truncate(positions.len());
    let (components, chi, boundary_touch, bounds) = topo_stats(&positions, &indices, grid);
    let stats = MeshStats {
        vertices: positions.len(),
        triangles: indices.len() / 3,
        components,
        chi,
        volume: vol.abs(),
        boundary_touch,
        hash: mesh_hash(&positions, &indices),
        bounds,
    };
    Mesh {
        positions,
        normals,
        indices,
        stats,
    }
}

pub fn field_to_mesh(field: &Field) -> Mesh {
    let n = field.grid.n;
    let shape = RuntimeShape::<u32, 3>::new([n, n, n]);
    let mut buf = SurfaceNetsBuffer::default();
    surface_nets(&field.values, &shape, [0; 3], [n - 1; 3], &mut buf);

    let grid = &field.grid;
    let positions: Vec<[f32; 3]> = buf
        .positions
        .iter()
        .map(|p| {
            [
                grid.origin[0] + p[0] * grid.h,
                grid.origin[1] + p[1] * grid.h,
                grid.origin[2] + p[2] * grid.h,
            ]
        })
        .collect();
    let normals: Vec<[f32; 3]> = buf
        .normals
        .iter()
        .map(|v| super::normalize_vec(*v))
        .collect();
    finish_mesh(positions, normals, buf.indices, grid)
}

fn signed_volume(positions: &[[f32; 3]], indices: &[u32]) -> f32 {
    let mut vol = 0f32;
    for t in indices.as_chunks::<3>().0 {
        let a = positions[t[0] as usize];
        let b = positions[t[1] as usize];
        let c = positions[t[2] as usize];
        vol += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    vol / 6.0
}

fn topo_stats(
    positions: &[[f32; 3]],
    indices: &[u32],
    grid: &Grid,
) -> (usize, i64, bool, Option<Aabb>) {
    if positions.is_empty() || indices.is_empty() {
        return (0, 0, false, None);
    }
    let mut parent: Vec<u32> = (0..positions.len() as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    // 闭合四边形网格每条边恰好被两个三角形共用 ⇒ 唯一边数 = 1.5 × 三角形数 = indices.len()/2。
    // 一次预分配到位，避免 1460 万条边走二十多次扩容重哈希（1024³ 实测该段 1525→886 ms）。
    let mut edges: FxSet<u64> =
        FxSet::with_capacity_and_hasher(indices.len() / 2, Default::default());
    let mut used = vec![false; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        for e in 0..3 {
            let a = t[e];
            let b = t[(e + 1) % 3];
            used[a as usize] = true;
            used[b as usize] = true;
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            if ra != rb {
                parent[ra as usize] = rb;
            }
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            edges.insert((lo as u64) << 32 | hi as u64);
        }
    }
    let mut roots = HashSet::new();
    for (v, is_used) in used.iter().enumerate() {
        if *is_used {
            let r = find(&mut parent, v as u32);
            roots.insert(r);
        }
    }
    let v = used.iter().filter(|x| **x).count() as i64;
    let e = edges.len() as i64;
    let f = (indices.len() / 3) as i64;
    let chi = v - e + f;

    let dom = grid.domain();
    let margin = 1.5 * grid.h;
    let mut b = Aabb::empty();
    let mut touch = false;
    for p in positions {
        b.union_point(*p);
        for (i, c) in p.iter().enumerate() {
            if *c <= dom.min[i] + margin || *c >= dom.max[i] - margin {
                touch = true;
            }
        }
    }
    (roots.len(), chi, touch, Some(b))
}

pub fn write_obj(mesh: &Mesh, path: &Path) -> std::io::Result<()> {
    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    writeln!(w, "# LSSMJ gauss2mesh (surface nets)")?;
    writeln!(
        w,
        "# vertices={} triangles={} volume={:.6} chi={} components={}",
        mesh.stats.vertices,
        mesh.stats.triangles,
        mesh.stats.volume,
        mesh.stats.chi,
        mesh.stats.components
    )?;
    // 写出是纯格式化（同一张 1024³ 网格到 C:/E: 两块盘都是 ~2.1 s ⇒ 不是 I/O）：分块多线程格式化、
    // 按原顺序落盘，逐行内容与串行版完全相同。
    write_lines(&mut w, mesh.positions.len(), |i, buf| {
        let p = mesh.positions[i];
        let _ = writeln!(buf, "v {:.6} {:.6} {:.6}", p[0], p[1], p[2]);
    })?;
    write_lines(&mut w, mesh.normals.len(), |i, buf| {
        let n = mesh.normals[i];
        let _ = writeln!(buf, "vn {:.6} {:.6} {:.6}", n[0], n[1], n[2]);
    })?;
    write_lines(&mut w, mesh.indices.len() / 3, |i, buf| {
        let t = [
            mesh.indices[3 * i],
            mesh.indices[3 * i + 1],
            mesh.indices[3 * i + 2],
        ];
        let _ = writeln!(
            buf,
            "f {}//{} {}//{} {}//{}",
            t[0] + 1,
            t[0] + 1,
            t[1] + 1,
            t[1] + 1,
            t[2] + 1,
            t[2] + 1
        );
    })?;
    Ok(())
}

/// 分块并行格式化 `[0, n)` 行，按顺序写盘（每行内容由 `f` 决定，输出顺序不变）。
fn write_lines<F>(w: &mut impl Write, n: usize, f: F) -> std::io::Result<()>
where
    F: Fn(usize, &mut Vec<u8>) + Sync,
{
    if n == 0 {
        return Ok(());
    }
    const CHUNK: usize = 1 << 18;
    let workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let mut start = 0;
    while start < n {
        let end = (start + CHUNK).min(n);
        if workers == 1 {
            let mut buf = Vec::with_capacity((end - start) * 48);
            for i in start..end {
                f(i, &mut buf);
            }
            w.write_all(&buf)?;
        } else {
            let per = (end - start).div_ceil(workers);
            let bufs: Vec<Vec<u8>> = std::thread::scope(|s| {
                let mut hs = Vec::new();
                for k in 0..workers {
                    let a = start + k * per;
                    let b = (a + per).min(end);
                    if a >= b {
                        continue;
                    }
                    let f = &f;
                    hs.push(s.spawn(move || {
                        let mut buf = Vec::with_capacity((b - a) * 48);
                        for i in a..b {
                            f(i, &mut buf);
                        }
                        buf
                    }));
                }
                hs.into_iter()
                    .map(|h| h.join().expect("obj writer panicked"))
                    .collect()
            });
            for b in &bufs {
                w.write_all(b)?;
            }
        }
        start = end;
    }
    Ok(())
}

/// 三点面法线（未归一化）。
pub fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    super::cross(
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    )
}
