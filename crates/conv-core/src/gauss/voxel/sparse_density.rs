//! 候选块稀疏密度场 + halo Surface Nets。
//!
//! 每个候选块存 (B+2)³ 的 halo 场（细格角点 x0−1..x0+B）；块内细 splat 只处理与 halo
//! 相交的 Gaussian。未知角点默认写 +outside（hollow 语义）；`seal=true` 时走块级粗分类
//! 加细 BFS，把被围住的未知区写成 −outside。
//!
//! 网格：每块用 fast-surface-nets 对 local cells 0..B（覆盖 global x0−1..x0+B−1），
//! 顶点按全局 cell key 去重、四边形按 4 cell 规范键去重 ⇒ 跨块无缝。

use super::{
    Grid, Prepared, VoxelMode, VoxelizeOptions, density_weight_scale, max_maha, prepare_density,
    splat_range, surface_blocks::SurfaceBlocks,
};
use crate::gauss::mesh::{Mesh, mesh_from_parts};
use crate::gauss::{Gaussian, normalize_vec};
use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 2-bit 三态标签（0 未定 / 1 外 / 2 内）。256³ 下 labels 从 16.8 MB 降到 4.2 MB，
/// 且 4.2 MB 能留在缓存里（合成探针 256³：u8 110.7 ms → 2-bit 87.6 ms）；
/// 5-trit/byte 反而 142.1 ms，故不用基-3。
struct PackedLabels {
    data: Vec<u8>,
}

impl PackedLabels {
    fn new(count: usize) -> Self {
        Self {
            data: vec![0u8; count.div_ceil(4)],
        }
    }

    #[inline]
    fn get(&self, i: usize) -> u8 {
        (self.data[i / 4] >> ((i % 4) * 2)) & 3
    }

    #[inline]
    fn set(&mut self, i: usize, v: u8) {
        let sh = (i % 4) * 2;
        self.data[i / 4] = (self.data[i / 4] & !(3 << sh)) | ((v & 3) << sh);
    }
}

pub struct SparseDensity {
    grid: Grid,
    block: u32,
    per: u32,
    pub iso: f32,
    pub outside: f32,
    pub observed: usize,
    pub sealed_interior: usize,
    data: Vec<Option<Vec<f32>>>,
}

impl SparseDensity {
    pub fn build(
        gaussians: &[Gaussian],
        opts: &VoxelizeOptions,
        grid: Grid,
        blocks: &SurfaceBlocks,
        iso: f32,
        seal: bool,
    ) -> Result<Self, String> {
        if opts.mode != VoxelMode::Density {
            return Err("SparseDensity 目前只支持 density".into());
        }
        if blocks.fine_n != grid.n || blocks.block == 0 {
            return Err("SurfaceBlocks 与细网格不匹配".into());
        }
        let prepared = prepare_density(gaussians, &grid, opts);
        if prepared.is_empty() {
            return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
        }
        let n = grid.n;
        let block = blocks.block;
        let per = blocks.per;
        let l = (block + 2) as usize;
        let outside = if opts.iso_frac > 0.0 {
            (iso / opts.iso_frac).max(iso * 1.5)
        } else {
            iso * 2.0
        };
        let bins = block_bins(&prepared, &grid, blocks);
        let w_scale = density_weight_scale(&grid, opts);
        let parts = parallel_blocks(blocks, opts.threads, |id, bx, by, bz| {
            let (x0, y0, z0) = (bx * block, by * block, bz * block);
            let mut local = vec![0f32; l * l * l];
            splat_block(
                &prepared, &bins[id], &grid, x0, y0, z0, block, n, l, w_scale, &mut local,
            );
            let observed = post_block(&mut local, &grid, iso, outside, x0, y0, z0, block, l);
            (local, observed)
        });
        let mut data: Vec<Option<Vec<f32>>> = (0..per * per * per).map(|_| None).collect();
        let mut observed = 0usize;
        for (id, (local, obs)) in parts {
            observed += obs;
            data[id] = Some(local);
        }
        let sealed_interior = if seal {
            let t = std::time::Instant::now();
            let sealed = seal_blocks(&mut data, None, &grid, block, per, blocks, outside);
            if std::env::var_os("LSSMJ_STAGE_DEBUG").is_some() {
                eprintln!(
                    "[sparse] density seal {:.1} ms",
                    t.elapsed().as_secs_f64() * 1e3
                );
            }
            sealed
        } else {
            0
        };
        Ok(Self {
            grid,
            block,
            per,
            iso,
            outside,
            observed,
            sealed_interior,
            data,
        })
    }

    pub fn block_values(&self, id: usize) -> Option<&[f32]> {
        self.data.get(id).and_then(|v| v.as_deref())
    }

    pub fn mesh(&self) -> Result<Mesh, String> {
        mesh_blocks(&self.data, &self.grid, self.block, self.per).map_err(|e| {
            if e.contains("没有生成三角形") {
                // 已知缺陷：>256³ 的 auto iso 取自半分辨率探针；密度是「每体素质量」，
                // 换分辨率后阈值不等价（scene01：256³ iso=29.39，512³ 等效约 6）。
                format!(
                    "{e}（iso={:.4}，观测 {} 体素）。>256³ 的 auto iso 来自半分辨率探针、未按分辨率换算，请显式传 --iso",
                    self.iso, self.observed
                )
            } else {
                e
            }
        })
    }
}

/// 每个候选块只保留与块（含 1 格 halo）相交的 Gaussian 下标。
pub(super) fn block_bins(
    prepared: &[Prepared],
    grid: &Grid,
    blocks: &SurfaceBlocks,
) -> Vec<Vec<u32>> {
    let n = grid.n;
    let block = blocks.block;
    let per = blocks.per;
    let mut bins: Vec<Vec<u32>> = (0..per * per * per).map(|_| Vec::new()).collect();
    for (i, g) in prepared.iter().enumerate() {
        let r = splat_range(grid, g.mean, g.radius);
        if r.empty {
            continue;
        }
        let lo = [
            r.lo[0].saturating_sub(1),
            r.lo[1].saturating_sub(1),
            r.lo[2].saturating_sub(1),
        ];
        let hi = [
            (r.hi[0] + 1).min(n - 1),
            (r.hi[1] + 1).min(n - 1),
            (r.hi[2] + 1).min(n - 1),
        ];
        let b0 = [lo[0] / block, lo[1] / block, lo[2] / block];
        let b1 = [hi[0] / block, hi[1] / block, hi[2] / block];
        for bz in b0[2]..=b1[2] {
            for by in b0[1]..=b1[1] {
                for bx in b0[0]..=b1[0] {
                    if blocks.contains(bx, by, bz) {
                        bins[(bx + by * per + bz * per * per) as usize].push(i as u32);
                    }
                }
            }
        }
    }
    bins
}

/// 按候选块并行执行 `f(id, bx, by, bz)`，结果与块顺序无关（调用方按 id 落位）。
pub(super) fn parallel_blocks<T, F>(blocks: &SurfaceBlocks, threads: usize, f: F) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize, u32, u32, u32) -> T + Sync,
{
    let active = blocks.active();
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let workers = if threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            threads
        }
        .clamp(1, 24)
        .min(active.len().max(1));
        let mut handles = Vec::new();
        for _ in 0..workers {
            let next = &next;
            let f = &f;
            handles.push(s.spawn(move || {
                let mut out = Vec::new();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= active.len() {
                        break;
                    }
                    let id = active[k] as usize;
                    let (bx, by, bz) = blocks.xyz(id as u32);
                    out.push((id, f(id, bx, by, bz)));
                }
                out
            }));
        }
        let mut all = Vec::new();
        for h in handles {
            match h.join() {
                Ok(v) => all.extend(v),
                Err(_) => panic!("sparse block worker panicked"),
            }
        }
        all
    })
}

pub(super) fn mesh_blocks(
    data: &[Option<Vec<f32>>],
    grid: &Grid,
    block: u32,
    per: u32,
) -> Result<Mesh, String> {
    let n = grid.n;
    let l = block + 2;
    let shape = RuntimeShape::<u32, 3>::new([l, l, l]);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vert_map: HashMap<u32, u32> = HashMap::new();
    let mut seen_quads: HashSet<[u32; 4]> = HashSet::new();
    // 块内格（到块边界都留 1 格余量）只有本块看得见 ⇒ 用块级小表去重，
    // 避掉全局 486 万条目表的随机访问；跨块共享的边界格仍走 vert_map。
    let mut local_verts: HashMap<u32, u32> = HashMap::new();
    let mut local_quads: HashSet<[u32; 4]> = HashSet::new();
    for (id, slot) in data.iter().enumerate() {
        let Some(local) = slot else {
            continue;
        };
        let per = per as usize;
        let bx = (id % per) as u32;
        let by = ((id / per) % per) as u32;
        let bz = (id / (per * per)) as u32;
        let x0 = bx * block;
        let y0 = by * block;
        let z0 = bz * block;
        let mut buf = SurfaceNetsBuffer::default();
        surface_nets(local, &shape, [0; 3], [block + 1; 3], &mut buf);
        if buf.indices.is_empty() {
            continue;
        }
        let cells: Vec<[i32; 3]> = buf
            .surface_points
            .iter()
            .map(|p| {
                [
                    x0 as i32 - 1 + p[0] as i32,
                    y0 as i32 - 1 + p[1] as i32,
                    z0 as i32 - 1 + p[2] as i32,
                ]
            })
            .collect();
        let posg: Vec<[f32; 3]> = buf
            .positions
            .iter()
            .map(|p| {
                [
                    grid.origin[0] + (x0 as f32 - 1.0 + p[0]) * grid.h,
                    grid.origin[1] + (y0 as f32 - 1.0 + p[1]) * grid.h,
                    grid.origin[2] + (z0 as f32 - 1.0 + p[2]) * grid.h,
                ]
            })
            .collect();
        let norm: Vec<[f32; 3]> = buf.normals.iter().map(|v| normalize_vec(*v)).collect();
        local_verts.clear();
        local_quads.clear();
        for q in buf.indices.as_chunks::<6>().0 {
            let mut keys = [0u32; 6];
            let mut valid = true;
            for (j, &vi) in q.iter().enumerate() {
                let c = cells[vi as usize];
                if c[0] < 0
                    || c[1] < 0
                    || c[2] < 0
                    || c[0] >= n as i32 - 1
                    || c[1] >= n as i32 - 1
                    || c[2] >= n as i32 - 1
                {
                    valid = false;
                    break;
                }
                keys[j] = c[0] as u32 + c[1] as u32 * n + c[2] as u32 * n * n;
            }
            if !valid {
                continue;
            }
            let mut quad = [0u32; 4];
            let mut count = 0;
            for &k in &keys {
                if !quad[..count].contains(&k) {
                    if count == 4 {
                        count = 5;
                        break;
                    }
                    quad[count] = k;
                    count += 1;
                }
            }
            if count != 4 {
                continue;
            }
            quad.sort_unstable();
            // 四个格都在本块内部时，四边形不可能被邻块重复发出 ⇒ 块级小表即可。
            let all_inner = q.iter().all(|&vi| {
                let c = cells[vi as usize];
                let (lx, ly, lz) = (c[0] - x0 as i32, c[1] - y0 as i32, c[2] - z0 as i32);
                lx >= 1
                    && ly >= 1
                    && lz >= 1
                    && lx < block as i32 - 1
                    && ly < block as i32 - 1
                    && lz < block as i32 - 1
            });
            let dup = if all_inner {
                !local_quads.insert(quad)
            } else {
                !seen_quads.insert(quad)
            };
            if dup {
                continue;
            }
            for (j, &vi) in q.iter().enumerate() {
                let key = keys[j];
                let c = cells[vi as usize];
                let (lx, ly, lz) = (c[0] - x0 as i32, c[1] - y0 as i32, c[2] - z0 as i32);
                // 到块边界都留 1 格余量：邻块的 surface_nets 范围最多伸进本块 1 格，
                // 所以 inner 格只有本块看得见，可以走块级小表。
                if lx >= 1
                    && ly >= 1
                    && lz >= 1
                    && lx < block as i32 - 1
                    && ly < block as i32 - 1
                    && lz < block as i32 - 1
                {
                    let lk = (lz as u32 * block + ly as u32) * block + lx as u32;
                    let gid = match local_verts.get(&lk) {
                        Some(&g) => g,
                        None => {
                            let g = positions.len() as u32;
                            local_verts.insert(lk, g);
                            positions.push(posg[vi as usize]);
                            normals.push(norm[vi as usize]);
                            g
                        }
                    };
                    indices.push(gid);
                    continue;
                }
                let gid = match vert_map.get(&key) {
                    Some(&g) => {
                        let d = {
                            let a = positions[g as usize];
                            let b = posg[vi as usize];
                            (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
                        };
                        if d > 1e-3 {
                            return Err(format!(
                                "相邻块共享 cell 顶点不一致：δ={d:.6}（cell={key}）"
                            ));
                        }
                        g
                    }
                    None => {
                        let g = positions.len() as u32;
                        vert_map.insert(key, g);
                        positions.push(posg[vi as usize]);
                        normals.push(norm[vi as usize]);
                        g
                    }
                };
                indices.push(gid);
            }
        }
    }
    if indices.is_empty() {
        return Err("稀疏网格没有生成三角形".into());
    }
    Ok(mesh_from_parts(positions, normals, indices, grid))
}

#[allow(clippy::too_many_arguments)]
fn splat_block(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    x0: u32,
    y0: u32,
    z0: u32,
    block: u32,
    n: u32,
    l: usize,
    w_scale: f32,
    local: &mut [f32],
) {
    let gx0 = x0 as i32 - 1;
    let gy0 = y0 as i32 - 1;
    let gz0 = z0 as i32 - 1;
    for &gi in indices {
        let g = &prepared[gi as usize];
        let op = g.opacity * w_scale;
        let r = splat_range(grid, g.mean, g.radius);
        if r.empty {
            continue;
        }
        let xa = r.lo[0].max(x0.saturating_sub(1));
        let xb = r.hi[0].min((x0 + block).min(n - 1));
        let ya = r.lo[1].max(y0.saturating_sub(1));
        let yb = r.hi[1].min((y0 + block).min(n - 1));
        let za = r.lo[2].max(z0.saturating_sub(1));
        let zb = r.hi[2].min((z0 + block).min(n - 1));
        if xa > xb || ya > yb || za > zb {
            continue;
        }
        for z in za..=zb {
            let lz = (z as i32 - gz0) as usize;
            for y in ya..=yb {
                let ly = (y as i32 - gy0) as usize;
                let row = (lz * l + ly) * l;
                for x in xa..=xb {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    let lx = (x as i32 - gx0) as usize;
                    local[row + lx] += op * (-0.5 * m).exp();
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn post_block(
    local: &mut [f32],
    grid: &Grid,
    iso: f32,
    outside: f32,
    x0: u32,
    y0: u32,
    z0: u32,
    block: u32,
    l: usize,
) -> usize {
    let n = grid.n;
    let mut observed = 0usize;
    for lz in 0..l {
        let gz = z0 as i32 - 1 + lz as i32;
        for ly in 0..l {
            let gy = y0 as i32 - 1 + ly as i32;
            let row = (lz * l + ly) * l;
            for lx in 0..l {
                let gx = x0 as i32 - 1 + lx as i32;
                let idx = row + lx;
                let v = if gx < 0
                    || gy < 0
                    || gz < 0
                    || gx >= n as i32
                    || gy >= n as i32
                    || gz >= n as i32
                {
                    outside
                } else {
                    let d = local[idx];
                    if d > iso {
                        // 只统计属于本块的角点，避免块间 halo 重复计数。
                        if gx as u32 / block == x0 / block
                            && gy as u32 / block == y0 / block
                            && gz as u32 / block == z0 / block
                        {
                            observed += 1;
                        }
                        iso - d
                    } else {
                        outside
                    }
                };
                local[idx] = v;
            }
        }
    }
    observed
}

/// 稀疏内部密封：块级粗分类 + 活跃块内未知细格的 BFS。
/// 已知（density: value<0；surfel: known mask）是屏障；外部种子来自域边界/外部空块，
/// 内部种子来自被围住的空块。
pub(super) fn seal_blocks(
    data: &mut [Option<Vec<f32>>],
    knowns: Option<&[Option<Vec<u8>>]>,
    grid: &Grid,
    block: u32,
    per: u32,
    blocks: &SurfaceBlocks,
    outside: f32,
) -> usize {
    let n = grid.n;
    let l = (block + 2) as usize;
    let per_i = per as i32;
    let count = (per * per * per) as usize;
    let mut class = vec![0u8; count]; // 0 未知空块, 1 内部屏障, 2 活跃
    for &id in blocks.active() {
        class[id as usize] = 2;
    }
    let coarse = blocks.coarse();
    let cn = coarse.grid.n;
    let cm = cn - 1;
    let ratio = (cn - 1) as f32 / (n - 1) as f32;
    for bz in 0..per {
        for by in 0..per {
            for bx in 0..per {
                let id = (bx + by * per + bz * per * per) as usize;
                if class[id] != 0 {
                    continue;
                }
                let x0 = bx * block;
                let y0 = by * block;
                let z0 = bz * block;
                let x1 = (x0 + block).min(n - 1);
                let y1 = (y0 + block).min(n - 1);
                let z1 = (z0 + block).min(n - 1);
                let c0 = (x0 as f32 * ratio).floor() as u32;
                let c1 = (x1 as f32 * ratio).floor() as u32;
                let d0 = (y0 as f32 * ratio).floor() as u32;
                let d1 = (y1 as f32 * ratio).floor() as u32;
                let e0 = (z0 as f32 * ratio).floor() as u32;
                let e1 = (z1 as f32 * ratio).floor() as u32;
                let c1 = c1.min(cm - 1);
                let d1 = d1.min(cm - 1);
                let e1 = e1.min(cm - 1);
                let mut all_neg = true;
                'outer: for cz in e0..=e1 {
                    for cy in d0..=d1 {
                        for cx in c0..=c1 {
                            if coarse.values[(cx + cy * cn + cz * cn * cn) as usize] >= 0.0 {
                                all_neg = false;
                                break 'outer;
                            }
                        }
                    }
                }
                if all_neg {
                    class[id] = 1;
                }
            }
        }
    }
    let mut out_block = vec![false; count];
    let mut bq = VecDeque::new();
    for bz in 0..per {
        for by in 0..per {
            for bx in 0..per {
                if bx != 0 && by != 0 && bz != 0 && bx + 1 != per && by + 1 != per && bz + 1 != per
                {
                    continue;
                }
                let id = (bx + by * per + bz * per * per) as usize;
                if class[id] == 0 {
                    out_block[id] = true;
                    bq.push_back(id as u32);
                }
            }
        }
    }
    while let Some(id) = bq.pop_front() {
        let id = id as usize;
        let bx = (id % per as usize) as i32;
        let by = ((id / per as usize) % per as usize) as i32;
        let bz = (id / (per * per) as usize) as i32;
        for (dx, dy, dz) in [
            (-1i32, 0i32, 0i32),
            (1, 0, 0),
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
        ] {
            let x = bx + dx;
            let y = by + dy;
            let z = bz + dz;
            if x < 0 || y < 0 || z < 0 || x >= per_i || y >= per_i || z >= per_i {
                continue;
            }
            let nid = (x as u32 + y as u32 * per + z as u32 * per * per) as usize;
            if class[nid] == 0 && !out_block[nid] {
                out_block[nid] = true;
                bq.push_back(nid as u32);
            }
        }
    }

    let bid = |x: u32, y: u32, z: u32| -> usize {
        if block.is_power_of_two() {
            let s = block.trailing_zeros();
            ((x >> s) + (y >> s) * per + (z >> s) * per * per) as usize
        } else {
            ((x / block) + (y / block) * per + (z / block) * per * per) as usize
        }
    };
    let mut labels = PackedLabels::new((n * n * n) as usize); // 0 未定, 1 外, 2 内
    let mut queue = VecDeque::new();
    let bmask = block - 1;
    let pow2_block = block.is_power_of_two();
    let nsh = n.trailing_zeros();
    let nmask = n - 1;
    let pow2_n = n.is_power_of_two();
    for &id in blocks.active() {
        let id = id as usize;
        let (bx, by, bz) = blocks.xyz(id as u32);
        let x0 = bx * block;
        let y0 = by * block;
        let z0 = bz * block;
        let vals = data[id].as_ref().unwrap();
        let kn = knowns.map(|k| k[id].as_ref().unwrap());
        for dz in 0..block {
            let gz = z0 + dz;
            if gz >= n {
                break;
            }
            for dy in 0..block {
                let gy = y0 + dy;
                if gy >= n {
                    break;
                }
                for dx in 0..block {
                    let gx = x0 + dx;
                    if gx >= n {
                        break;
                    }
                    let g = (gx + gy * n + gz * n * n) as usize;
                    if labels.get(g) != 0 {
                        continue;
                    }
                    let li = (((dz + 1) as usize * l) + (dy + 1) as usize) * l + (dx + 1) as usize;
                    let known_here = match kn {
                        Some(k) => k[li] != 0,
                        None => vals[li] < 0.0,
                    };
                    if known_here {
                        continue;
                    }
                    let mut lab = 0u8;
                    for (dx, dy, dz) in [
                        (-1i32, 0i32, 0i32),
                        (1, 0, 0),
                        (0, -1, 0),
                        (0, 1, 0),
                        (0, 0, -1),
                        (0, 0, 1),
                    ] {
                        let x = gx as i32 + dx;
                        let y = gy as i32 + dy;
                        let z = gz as i32 + dz;
                        if x < 0
                            || y < 0
                            || z < 0
                            || x >= n as i32
                            || y >= n as i32
                            || z >= n as i32
                        {
                            lab = 1;
                            break;
                        }
                        let nb = bid(x as u32, y as u32, z as u32);
                        if nb == id || class[nb] == 2 {
                            continue;
                        }
                        if out_block[nb] {
                            lab = 1;
                            break;
                        }
                        lab = 2;
                    }
                    if lab != 0 {
                        labels.set(g, lab);
                        queue.push_back(g as u32);
                    }
                }
            }
        }
    }
    let mut c_nb = usize::MAX;
    let mut c_vals: &[f32] = &[];
    let mut c_kn: Option<&[u8]> = None;
    while let Some(g) = queue.pop_front() {
        let g = g as usize;
        let (gx, gy, gz) = if pow2_n {
            (
                (g as u32) & nmask,
                ((g as u32) >> nsh) & nmask,
                (g as u32) >> (2 * nsh),
            )
        } else {
            ((g as u32) % n, ((g as u32) / n) % n, (g as u32) / (n * n))
        };
        let lab = labels.get(g);
        for (dx, dy, dz) in [
            (-1i32, 0i32, 0i32),
            (1, 0, 0),
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
        ] {
            let x = gx as i32 + dx;
            let y = gy as i32 + dy;
            let z = gz as i32 + dz;
            if x < 0 || y < 0 || z < 0 || x >= n as i32 || y >= n as i32 || z >= n as i32 {
                continue;
            }
            let (x, y, z) = (x as u32, y as u32, z as u32);
            let nb = bid(x, y, z);
            if class[nb] != 2 {
                continue;
            }
            let ng = (x + y * n + z * n * n) as usize;
            if labels.get(ng) != 0 {
                continue;
            }
            let (lx, ly, lz) = if pow2_block {
                (
                    (x & bmask) as usize + 1,
                    (y & bmask) as usize + 1,
                    (z & bmask) as usize + 1,
                )
            } else {
                let bx = x / block;
                let by = y / block;
                let bz = z / block;
                (
                    (x - bx * block + 1) as usize,
                    (y - by * block + 1) as usize,
                    (z - bz * block + 1) as usize,
                )
            };
            let li = (lz * l + ly) * l + lx;
            if nb != c_nb {
                c_nb = nb;
                c_vals = data[nb].as_ref().unwrap();
                c_kn = knowns.map(|k| k[nb].as_ref().unwrap().as_slice());
            }
            let known_nb = match c_kn {
                Some(k) => k[li] != 0,
                None => c_vals[li] < 0.0,
            };
            if known_nb {
                continue;
            }
            labels.set(ng, lab);
            queue.push_back(ng as u32);
        }
    }

    let mut inside = 0usize;
    for &id in blocks.active() {
        let id = id as usize;
        let (bx, by, bz) = blocks.xyz(id as u32);
        let x0 = bx * block;
        let y0 = by * block;
        let z0 = bz * block;
        let kn = knowns.map(|k| k[id].as_ref().unwrap());
        let vals = data[id].as_mut().unwrap();
        for lz in 0..l {
            let gz = z0 as i64 - 1 + lz as i64;
            for ly in 0..l {
                let gy = y0 as i64 - 1 + ly as i64;
                for lx in 0..l {
                    let gx = x0 as i64 - 1 + lx as i64;
                    let idx = (lz * l + ly) * l + lx;
                    let v = &mut vals[idx];
                    if gx < 0
                        || gy < 0
                        || gz < 0
                        || gx >= n as i64
                        || gy >= n as i64
                        || gz >= n as i64
                    {
                        *v = outside;
                        continue;
                    }
                    let known_here = match kn {
                        Some(k) => k[idx] != 0,
                        None => *v < 0.0,
                    };
                    if known_here {
                        continue;
                    }
                    let (gx, gy, gz) = (gx as u32, gy as u32, gz as u32);
                    let nb = bid(gx, gy, gz);
                    let lab = if class[nb] == 2 {
                        labels.get((gx + gy * n + gz * n * n) as usize)
                    } else if out_block[nb] {
                        1
                    } else {
                        2
                    };
                    let owned = gx >= x0
                        && gx < x0 + block
                        && gy >= y0
                        && gy < y0 + block
                        && gz >= z0
                        && gz < z0 + block;
                    *v = if lab == 1 {
                        outside
                    } else {
                        if owned {
                            inside += 1;
                        }
                        -outside
                    };
                }
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::super::{fit_inputs, voxelize};
    use super::*;
    use crate::gauss::mesh::field_to_mesh;
    use crate::gauss::{add_floaters, blob_sphere_gaussians};
    use std::time::Instant;

    #[test]
    fn sparse_density_matches_dense_128() {
        let mut gs = blob_sphere_gaussians(5_000, 0.75, 0.05, 0.95, 7);
        add_floaters(&mut gs, 300, 7 ^ 0x9e37_79b9);
        let opts = VoxelizeOptions {
            mode: VoxelMode::Density,
            n: 128,
            threads: 24,
            seal: false,
            ..VoxelizeOptions::default()
        };
        let dense = voxelize(&gs, &opts).expect("dense");
        let dm = field_to_mesh(&dense);
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let blocks = SurfaceBlocks::detect(&gs, &opts, grid, 16, 0, 4).expect("blocks");
        let sparse = SparseDensity::build(&gs, &opts, grid, &blocks, dense.iso, false)
            .expect("sparse build");
        let sm = sparse.mesh().expect("sparse mesh");
        let rel = ((dm.stats.volume - sm.stats.volume) / dm.stats.volume).abs();
        println!(
            "dense tri={} vol={:.5} chi={}; sparse tri={} vol={:.5} chi={} rel={rel:.5} active={} ({:.1}%)",
            dm.stats.triangles,
            dm.stats.volume,
            dm.stats.chi,
            sm.stats.triangles,
            sm.stats.volume,
            sm.stats.chi,
            blocks.active_count(),
            blocks.fraction() * 100.0
        );
        assert_eq!(dm.stats.triangles, sm.stats.triangles, "triangles");
        assert!(rel < 1e-3, "volume rel {rel}");
        assert_eq!(dm.stats.chi, sm.stats.chi, "chi");
        assert_eq!(dm.stats.components, sm.stats.components, "components");
    }

    #[test]
    fn sparse_density_solid_matches_dense_128() {
        let mut gs = blob_sphere_gaussians(5_000, 0.75, 0.05, 0.95, 7);
        add_floaters(&mut gs, 300, 7 ^ 0x9e37_79b9);
        let opts = VoxelizeOptions {
            mode: VoxelMode::Density,
            n: 128,
            threads: 24,
            ..VoxelizeOptions::default()
        };
        let dense = voxelize(&gs, &opts).expect("dense");
        let dm = field_to_mesh(&dense);
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let blocks = SurfaceBlocks::detect(&gs, &opts, grid, 16, 0, 4).expect("blocks");
        let sparse =
            SparseDensity::build(&gs, &opts, grid, &blocks, dense.iso, true).expect("sparse build");
        let sm = sparse.mesh().expect("sparse mesh");
        let rel = ((dm.stats.volume - sm.stats.volume) / dm.stats.volume).abs();
        println!(
            "solid dense tri={} vol={:.5} chi={}; sparse tri={} vol={:.5} chi={} rel={rel:.5} comp={}/{}",
            dm.stats.triangles,
            dm.stats.volume,
            dm.stats.chi,
            sm.stats.triangles,
            sm.stats.volume,
            sm.stats.chi,
            dm.stats.components,
            sm.stats.components
        );
        assert!(rel < 0.05, "volume rel {rel}");
        assert_eq!(dm.stats.chi, sm.stats.chi, "chi");
        assert_eq!(dm.stats.components, sm.stats.components);
    }

    #[test]
    #[ignore = "diagnostic sparse density 256"]
    fn sparse_density_256_bench() {
        let mut gs = blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
        add_floaters(&mut gs, 300, 7 ^ 0x9e37_79b9);
        let opts = VoxelizeOptions {
            mode: VoxelMode::Density,
            n: 256,
            threads: 24,
            seal: false,
            ..VoxelizeOptions::default()
        };
        let t = Instant::now();
        let dense = voxelize(&gs, &opts).expect("dense");
        let dm = field_to_mesh(&dense);
        let dense_ms = t.elapsed().as_secs_f64() * 1e3;
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let t = Instant::now();
        let blocks = SurfaceBlocks::detect(&gs, &opts, grid, 16, 0, 4).expect("blocks");
        let detect_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let sparse = SparseDensity::build(&gs, &opts, grid, &blocks, dense.iso, false)
            .expect("sparse build");
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let sm = sparse.mesh().expect("sparse mesh");
        let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
        let rel = ((dm.stats.volume - sm.stats.volume) / dm.stats.volume).abs();
        println!(
            "dense={dense_ms:.1}ms (tri={} vol={:.5} chi={}); sparse detect={detect_ms:.1} build={build_ms:.1} mesh={mesh_ms:.1}ms (tri={} vol={:.5} chi={} rel={rel:.5}) active={} ({:.1}%)",
            dm.stats.triangles,
            dm.stats.volume,
            dm.stats.chi,
            sm.stats.triangles,
            sm.stats.volume,
            sm.stats.chi,
            blocks.active_count(),
            blocks.fraction() * 100.0
        );
        assert_eq!(dm.stats.triangles, sm.stats.triangles);
        assert!(rel < 1e-3, "volume rel {rel}");
        assert_eq!(dm.stats.chi, sm.stats.chi);
    }
}
