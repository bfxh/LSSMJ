//! 候选块稀疏 surfel 场 + halo Surface Nets。
//!
//! 每块存 (B+2)³ 的 TSDF 与 known 掩码：`den >= min_weight` 的角点已知，值取
//! `clamp(num/den, ±3h)`；其余未知角点先写 +outside。seal 时复用块级粗分类 +
//! 块内 BFS，把未知角点分成外部 / 内部并写 ±outside。网格复用 density 的
//! `mesh_blocks`（全局 cell key 去重 + 四边形规范键去重）。

use super::{
    Grid, Prepared, VoxelMode, VoxelizeOptions, max_maha, prepare_surfel,
    sparse_density::{block_bins, mesh_blocks, parallel_blocks, seal_blocks},
    splat_range,
    surface_blocks::SurfaceBlocks,
};
use crate::gauss::mesh::Mesh;
use crate::gauss::{Gaussian, dot};

pub struct SparseSurfel {
    grid: Grid,
    block: u32,
    per: u32,
    pub outside: f32,
    pub observed: usize,
    pub sealed_interior: usize,
    data: Vec<Option<Vec<f32>>>,
    knowns: Vec<Option<Vec<u8>>>,
}

impl SparseSurfel {
    pub fn build(
        gaussians: &[Gaussian],
        opts: &VoxelizeOptions,
        grid: Grid,
        blocks: &SurfaceBlocks,
        seal: bool,
    ) -> Result<Self, String> {
        if opts.mode != VoxelMode::Surfel {
            return Err("SparseSurfel 只支持 surfel".into());
        }
        if blocks.fine_n != grid.n || blocks.block == 0 {
            return Err("SurfaceBlocks 与细网格不匹配".into());
        }
        let (prepared, clamp_abs) = prepare_surfel(gaussians, &grid, opts);
        if prepared.is_empty() {
            return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
        }
        let block = blocks.block;
        let per = blocks.per;
        let l = (block + 2) as usize;
        let outside = 3.0 * grid.h;
        let n = grid.n;
        let bins = block_bins(&prepared, &grid, blocks);
        let parts = parallel_blocks(blocks, opts.threads, |id, bx, by, bz| {
            let (x0, y0, z0) = (bx * block, by * block, bz * block);
            let mut num = vec![0f32; l * l * l];
            let mut den = vec![0f32; l * l * l];
            splat_surfel_block(
                &prepared,
                &bins[id],
                &grid,
                x0,
                y0,
                z0,
                block,
                n,
                l,
                clamp_abs,
                opts.fast_exp,
                &mut num,
                &mut den,
            );
            post_surfel_block(
                &num,
                &den,
                &grid,
                opts.min_weight,
                outside,
                x0,
                y0,
                z0,
                block,
                l,
            )
        });
        let mut data: Vec<Option<Vec<f32>>> = (0..per * per * per).map(|_| None).collect();
        let mut knowns: Vec<Option<Vec<u8>>> = (0..per * per * per).map(|_| None).collect();
        let mut observed = 0usize;
        for (id, (values, known, obs)) in parts {
            observed += obs;
            data[id] = Some(values);
            knowns[id] = Some(known);
        }
        if observed == 0 {
            return Err("面元融合没有观测到任何体素（法线不可信或带宽太小）".into());
        }
        let sealed_interior = if seal {
            let t = std::time::Instant::now();
            let sealed = seal_blocks(&mut data, Some(&knowns), &grid, block, per, blocks, outside);
            if std::env::var_os("LSSMJ_STAGE_DEBUG").is_some() {
                eprintln!(
                    "[sparse] surfel seal {:.1} ms",
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
            outside,
            observed,
            sealed_interior,
            data,
            knowns,
        })
    }

    pub fn block_values(&self, id: usize) -> Option<&[f32]> {
        self.data.get(id).and_then(|v| v.as_deref())
    }

    pub fn block_known(&self, id: usize) -> Option<&[u8]> {
        self.knowns.get(id).and_then(|v| v.as_deref())
    }

    pub fn mesh(&self) -> Result<Mesh, String> {
        mesh_blocks(&self.data, &self.grid, self.block, self.per)
    }
}

#[allow(clippy::too_many_arguments)]
fn splat_surfel_block(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    x0: u32,
    y0: u32,
    z0: u32,
    block: u32,
    n: u32,
    l: usize,
    clamp_abs: f32,
    fast: bool,
    num: &mut [f32],
    den: &mut [f32],
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
                    let dist = dot(d, g.normal).clamp(-clamp_abs, clamp_abs);
                    let w = g.opacity
                        * if fast {
                            super::fast_exp::fast_exp(-0.5 * m)
                        } else {
                            (-0.5 * m).exp()
                        };
                    let idx = row + (x as i32 - gx0) as usize;
                    num[idx] += w * dist;
                    den[idx] += w;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn post_surfel_block(
    num: &[f32],
    den: &[f32],
    grid: &Grid,
    min_weight: f32,
    outside: f32,
    x0: u32,
    y0: u32,
    z0: u32,
    block: u32,
    l: usize,
) -> (Vec<f32>, Vec<u8>, usize) {
    let n = grid.n;
    let mut observed = 0usize;
    let mut values = vec![0f32; l * l * l];
    let mut known = vec![0u8; l * l * l];
    for lz in 0..l {
        let gz = z0 as i32 - 1 + lz as i32;
        for ly in 0..l {
            let gy = y0 as i32 - 1 + ly as i32;
            for lx in 0..l {
                let gx = x0 as i32 - 1 + lx as i32;
                let idx = (lz * l + ly) * l + lx;
                if gx < 0 || gy < 0 || gz < 0 || gx >= n as i32 || gy >= n as i32 || gz >= n as i32
                {
                    values[idx] = outside;
                    continue;
                }
                let d = den[idx];
                if d >= min_weight && d > 0.0 {
                    values[idx] = (num[idx] / d).clamp(-outside, outside);
                    known[idx] = 1;
                    if gx as u32 / block == x0 / block
                        && gy as u32 / block == y0 / block
                        && gz as u32 / block == z0 / block
                    {
                        observed += 1;
                    }
                } else {
                    values[idx] = outside;
                }
            }
        }
    }
    (values, known, observed)
}

#[cfg(test)]
mod tests {
    use super::super::{fit_inputs, voxelize};
    use super::*;
    use crate::gauss::mesh::field_to_mesh;
    use crate::gauss::{sphere_gaussians, torus_gaussians};
    use std::time::Instant;

    fn surfel_opts(n: u32, seal: bool) -> VoxelizeOptions {
        VoxelizeOptions {
            mode: VoxelMode::Surfel,
            n,
            threads: 24,
            seal,
            ..VoxelizeOptions::default()
        }
    }

    fn compare(gs: &[Gaussian], n: u32, seal: bool, label: &str) {
        let opts = surfel_opts(n, seal);
        let dense = voxelize(gs, &opts).expect("dense");
        let dm = field_to_mesh(&dense);
        let (grid, _) = fit_inputs(gs, &opts).expect("fit");
        let blocks = SurfaceBlocks::detect(gs, &opts, grid, 16, 0, 4).expect("blocks");
        let sparse = SparseSurfel::build(gs, &opts, grid, &blocks, seal).expect("sparse build");
        let sm = sparse.mesh().expect("sparse mesh");
        let rel = ((dm.stats.volume - sm.stats.volume) / dm.stats.volume).abs();
        println!(
            "{label} seal={seal}: dense tri={} vol={:.5} chi={} comp={}; sparse tri={} vol={:.5} chi={} comp={} rel={rel:.5} active={} ({:.1}%) obs={}",
            dm.stats.triangles,
            dm.stats.volume,
            dm.stats.chi,
            dm.stats.components,
            sm.stats.triangles,
            sm.stats.volume,
            sm.stats.chi,
            sm.stats.components,
            blocks.active_count(),
            blocks.fraction() * 100.0,
            sparse.observed
        );
        if !seal {
            assert_eq!(dm.stats.triangles, sm.stats.triangles, "triangles");
            assert!(rel < 1e-3, "volume rel {rel}");
        } else {
            assert!(rel < 0.05, "volume rel {rel}");
        }
        assert_eq!(dm.stats.chi, sm.stats.chi, "chi");
        assert_eq!(dm.stats.components, sm.stats.components, "components");
    }

    #[test]
    fn sparse_surfel_sphere_hollow_matches_dense_128() {
        let gs = sphere_gaussians(20_000, 0.75, 0.99);
        compare(&gs, 128, false, "sphere");
    }

    #[test]
    fn sparse_surfel_sphere_solid_matches_dense_128() {
        let gs = sphere_gaussians(20_000, 0.75, 0.99);
        compare(&gs, 128, true, "sphere");
    }

    #[test]
    fn sparse_surfel_torus_hollow_matches_dense_128() {
        let gs = torus_gaussians(180, 110, 0.55, 0.22, 0.99);
        compare(&gs, 128, false, "torus");
    }

    #[test]
    #[ignore = "diagnostic sparse surfel 512 sealed (block 16/32)"]
    fn sparse_surfel_512_bench() {
        let gs = sphere_gaussians(60_000, 0.75, 0.99);
        let opts = surfel_opts(512, true);
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        for block in [16u32, 32] {
            let t = Instant::now();
            let blocks = SurfaceBlocks::detect(&gs, &opts, grid, block, 0, 4).expect("blocks");
            let detect_ms = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let sparse =
                SparseSurfel::build(&gs, &opts, grid, &blocks, true).expect("sparse build");
            let build_ms = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let sm = sparse.mesh().expect("sparse mesh");
            let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "surfel 512 sealed B={block}: detect={detect_ms:.1} build={build_ms:.1} mesh={mesh_ms:.1} ms active={} ({:.1}%) corners={:.1}M tri={} vol={:.5} chi={} comp={} obs={} sealed={}",
                blocks.active_count(),
                blocks.fraction() * 100.0,
                blocks.active_count() as f64 * (block as f64 + 2.0).powi(3) / 1e6,
                sm.stats.triangles,
                sm.stats.volume,
                sm.stats.chi,
                sm.stats.components,
                sparse.observed,
                sparse.sealed_interior
            );
        }
    }

    #[test]
    #[ignore = "diagnostic sparse surfel 256 sealed"]
    fn sparse_surfel_256_seal_bench() {
        let gs = sphere_gaussians(60_000, 0.75, 0.99);
        let opts = surfel_opts(256, true);
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let t = Instant::now();
        let blocks = SurfaceBlocks::detect(&gs, &opts, grid, 16, 0, 4).expect("blocks");
        let detect_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let sparse = SparseSurfel::build(&gs, &opts, grid, &blocks, true).expect("sparse build");
        let build_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let sm = sparse.mesh().expect("sparse mesh");
        let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "surfel 256 sealed: detect={detect_ms:.1} build={build_ms:.1} mesh={mesh_ms:.1} ms tri={} vol={:.5} chi={} comp={} active={} ({:.1}%) obs={}",
            sm.stats.triangles,
            sm.stats.volume,
            sm.stats.chi,
            sm.stats.components,
            blocks.active_count(),
            blocks.fraction() * 100.0,
            sparse.observed
        );
    }

    #[test]
    #[ignore = "diagnostic sparse surfel 256"]
    fn sparse_surfel_256_bench() {
        let gs = sphere_gaussians(60_000, 0.75, 0.99);
        let opts = surfel_opts(256, false);
        let t = Instant::now();
        let dense = voxelize(&gs, &opts).expect("dense");
        let dm = field_to_mesh(&dense);
        let dense_ms = t.elapsed().as_secs_f64() * 1e3;
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let t = Instant::now();
        let blocks = SurfaceBlocks::detect(&gs, &opts, grid, 16, 0, 4).expect("blocks");
        let detect_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let sparse = SparseSurfel::build(&gs, &opts, grid, &blocks, false).expect("sparse build");
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
