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
    // gsn 的索引缓冲按"每 cell 3 个四边形"预留；超过绑定上限时 wgpu 会 panic，
    // 这里先拒绝，让调用方走显式 CPU 回退。
    let idx_bytes = (field.grid.n as u64).pow(3) * 3 * 6 * 4;
    let limit = hd.device.limits().max_storage_buffer_binding_size;
    if idx_bytes > limit {
        return Err(format!(
            "GPU 网格索引缓冲 {:.1} MB 超过设备绑定上限 {:.1} MB",
            idx_bytes as f64 / (1 << 20) as f64,
            limit as f64 / (1 << 20) as f64
        ));
    }
    let gsn = crate::gsn::surface_nets_gpu(hd, &field.values, field.grid.n);
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

    let mut vol = signed_volume(&positions, &indices);
    if vol < 0.0 {
        for t in indices.as_chunks_mut::<3>().0 {
            t.swap(1, 2);
        }
        vol = -vol;
    }
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
    let mut normals: Vec<[f32; 3]> = buf
        .normals
        .iter()
        .map(|v| super::normalize_vec(*v))
        .collect();
    let mut indices = buf.indices;

    let vol = signed_volume(&positions, &indices);
    if vol < 0.0 {
        for t in indices.as_chunks_mut::<3>().0 {
            t.swap(1, 2);
        }
        for v in &mut normals {
            *v = [-v[0], -v[1], -v[2]];
        }
    }
    if normals.len() < positions.len() {
        normals.resize(positions.len(), [0.0, 1.0, 0.0]);
    }

    let (components, chi, boundary_touch, bounds) = topo_stats(&positions, &indices, grid);
    let hash = mesh_hash(&positions, &indices);
    let stats = MeshStats {
        vertices: positions.len(),
        triangles: indices.len() / 3,
        components,
        chi,
        volume: vol.abs(),
        boundary_touch,
        hash,
        bounds,
    };
    // 顶点的法线数量应与位置一致；Surface Nets 保证这一点，防御性截断。
    normals.truncate(positions.len());
    if positions.is_empty() {
        indices.clear();
    }
    Mesh {
        positions,
        normals,
        indices,
        stats,
    }
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
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
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
            edges.insert(if a < b { (a, b) } else { (b, a) });
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
    for p in &mesh.positions {
        writeln!(w, "v {:.6} {:.6} {:.6}", p[0], p[1], p[2])?;
    }
    for n in &mesh.normals {
        writeln!(w, "vn {:.6} {:.6} {:.6}", n[0], n[1], n[2])?;
    }
    for t in mesh.indices.as_chunks::<3>().0 {
        writeln!(
            w,
            "f {}//{} {}//{} {}//{}",
            t[0] + 1,
            t[0] + 1,
            t[1] + 1,
            t[1] + 1,
            t[2] + 1,
            t[2] + 1
        )?;
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
