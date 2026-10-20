//! 候选块稀疏密度场 + halo Surface Nets。
//!
//! 每个候选块存 (B+2)³ 的 halo 场（细格角点 x0−1..x0+B）；块内细 splat 只处理与 halo
//! 相交的 Gaussian。当前未知角点一律写 +outside（hollow 语义）；稀疏内部密封是下一步。
//! 网格：每块用 fast-surface-nets 对 local cells 0..B（覆盖 global x0−1..x0+B−1），
//! 顶点按全局 cell key 去重、四边形按 4 cell 规范键去重 ⇒ 跨块无缝。

use super::{
    Grid, Prepared, VoxelMode, VoxelizeOptions, max_maha, prepare_density, splat_range,
    surface_blocks::SurfaceBlocks,
};
use crate::gauss::mesh::{Mesh, mesh_from_parts};
use crate::gauss::{Gaussian, normalize_vec};
use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SparseDensity {
    grid: Grid,
    block: u32,
    per: u32,
    pub iso: f32,
    pub outside: f32,
    data: Vec<Option<Vec<f32>>>,
}

impl SparseDensity {
    pub fn build(
        gaussians: &[Gaussian],
        opts: &VoxelizeOptions,
        grid: Grid,
        blocks: &SurfaceBlocks,
        iso: f32,
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
        let mut bins: Vec<Vec<u32>> = (0..per * per * per).map(|_| Vec::new()).collect();
        for (i, g) in prepared.iter().enumerate() {
            let r = splat_range(&grid, g.mean, g.radius);
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

        let active = blocks.active().to_vec();
        let next = AtomicUsize::new(0);
        let parts: Vec<(usize, Vec<f32>)> = std::thread::scope(|s| {
            let workers = if opts.threads == 0 {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            } else {
                opts.threads
            }
            .clamp(1, 24)
            .min(active.len().max(1));
            let mut handles = Vec::new();
            for _ in 0..workers {
                let next = &next;
                let active = &active;
                let bins = &bins;
                let prepared = &prepared;
                handles.push(s.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        if k >= active.len() {
                            break;
                        }
                        let id = active[k] as usize;
                        let (bx, by, bz) = (
                            id % per as usize,
                            (id / per as usize) % per as usize,
                            id / (per * per) as usize,
                        );
                        let x0 = bx as u32 * block;
                        let y0 = by as u32 * block;
                        let z0 = bz as u32 * block;
                        let mut local = vec![0f32; l * l * l];
                        splat_block(
                            prepared, &bins[id], &grid, x0, y0, z0, block, n, l, &mut local,
                        );
                        post_block(&mut local, &grid, iso, outside, x0, y0, z0, l);
                        out.push((id, local));
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
        });
        let mut data: Vec<Option<Vec<f32>>> = (0..per * per * per).map(|_| None).collect();
        for (id, local) in parts {
            data[id] = Some(local);
        }
        Ok(Self {
            grid,
            block,
            per,
            iso,
            outside,
            data,
        })
    }

    pub fn block_values(&self, id: usize) -> Option<&[f32]> {
        self.data.get(id).and_then(|v| v.as_deref())
    }

    pub fn mesh(&self) -> Result<Mesh, String> {
        let n = self.grid.n;
        let l = self.block + 2;
        let shape = RuntimeShape::<u32, 3>::new([l, l, l]);
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut vert_map: HashMap<u32, u32> = HashMap::new();
        let mut seen_quads: HashSet<[u32; 4]> = HashSet::new();
        for (id, slot) in self.data.iter().enumerate() {
            let Some(local) = slot else {
                continue;
            };
            let per = self.per as usize;
            let bx = (id % per) as u32;
            let by = ((id / per) % per) as u32;
            let bz = (id / (per * per)) as u32;
            let x0 = bx * self.block;
            let y0 = by * self.block;
            let z0 = bz * self.block;
            let mut buf = SurfaceNetsBuffer::default();
            surface_nets(local, &shape, [0; 3], [self.block + 1; 3], &mut buf);
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
                        self.grid.origin[0] + (x0 as f32 - 1.0 + p[0]) * self.grid.h,
                        self.grid.origin[1] + (y0 as f32 - 1.0 + p[1]) * self.grid.h,
                        self.grid.origin[2] + (z0 as f32 - 1.0 + p[2]) * self.grid.h,
                    ]
                })
                .collect();
            let norm: Vec<[f32; 3]> = buf.normals.iter().map(|v| normalize_vec(*v)).collect();
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
                if !seen_quads.insert(quad) {
                    continue;
                }
                for (j, &vi) in q.iter().enumerate() {
                    let key = keys[j];
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
        Ok(mesh_from_parts(positions, normals, indices, &self.grid))
    }
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
    local: &mut [f32],
) {
    let gx0 = x0 as i32 - 1;
    let gy0 = y0 as i32 - 1;
    let gz0 = z0 as i32 - 1;
    for &gi in indices {
        let g = &prepared[gi as usize];
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
                    local[row + lx] += g.opacity * (-0.5 * m).exp();
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
    l: usize,
) {
    let n = grid.n;
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
                    if d > iso { iso - d } else { outside }
                };
                local[idx] = v;
            }
        }
    }
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
        let sparse =
            SparseDensity::build(&gs, &opts, grid, &blocks, dense.iso).expect("sparse build");
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
        let sparse =
            SparseDensity::build(&gs, &opts, grid, &blocks, dense.iso).expect("sparse build");
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
