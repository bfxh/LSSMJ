//! 粗场定位：在同域低分辨率场上找表面粗块，膨胀后映射为细分辨率候选块。
//!
//! 用途是 T-GC-05 的第一步：稀疏路径只对候选块做细 splat/网格。
//! 判定保守优先（宁可多选，不漏选）：粗块取 8 角点有正有负，再按 `margin` 膨胀。

use super::{
    Backend, Field, Grid, VoxelMode, VoxelizeOptions, bounds_of, voxelize_density, voxelize_surfel,
};
use crate::gauss::Gaussian;

pub struct SurfaceBlocks {
    pub fine_n: u32,
    pub block: u32,
    pub per: u32,
    pub coarse_n: u32,
    pub margin: u32,
    coarse: Field,
    /// 升序的块线性 id（x + y*per + z*per²）
    active: Vec<u32>,
}

impl SurfaceBlocks {
    /// `coarse_factor`：粗分辨率 = fine_n / factor（至少 8）。`margin` 以粗块为单位。
    pub fn detect(
        gaussians: &[Gaussian],
        opts: &VoxelizeOptions,
        fine_grid: Grid,
        block: u32,
        margin: u32,
        coarse_factor: u32,
    ) -> Result<Self, String> {
        if block == 0 {
            return Err("block 必须 > 0".into());
        }
        if coarse_factor < 2 {
            return Err("coarse_factor 必须 ≥ 2".into());
        }
        let coarse_n = (fine_grid.n / coarse_factor).max(8);
        if coarse_n >= fine_grid.n {
            return Err(format!(
                "粗分辨率 {coarse_n} 不小于细分辨率 {}",
                fine_grid.n
            ));
        }
        let hc = (fine_grid.n - 1) as f32 * fine_grid.h / (coarse_n - 1) as f32;
        let coarse_grid = Grid {
            n: coarse_n,
            origin: fine_grid.origin,
            h: hc,
        };
        let mut co = opts.clone();
        co.n = coarse_n;
        co.backend = Backend::Cpu;
        co.bins3d = false;
        let data_bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
        let coarse = match opts.mode {
            VoxelMode::Density => voxelize_density(gaussians, &co, coarse_grid, data_bounds)?,
            VoxelMode::Surfel => voxelize_surfel(gaussians, &co, coarse_grid, data_bounds)?,
        };
        let mask = surface_mask(&coarse, coarse_n);
        let mask = dilate(&mask, coarse_n - 1, margin);
        let per = fine_grid.n.div_ceil(block);
        let ratio = (coarse_n - 1) as f32 / (fine_grid.n - 1) as f32;
        let mut active = Vec::new();
        for bz in 0..per {
            for by in 0..per {
                for bx in 0..per {
                    if block_hits(&mask, coarse_n - 1, bx, by, bz, block, fine_grid.n, ratio) {
                        active.push(bx + by * per + bz * per * per);
                    }
                }
            }
        }
        Ok(Self {
            fine_n: fine_grid.n,
            block,
            per,
            coarse_n,
            margin,
            coarse,
            active,
        })
    }

    pub fn coarse(&self) -> &Field {
        &self.coarse
    }

    pub fn active(&self) -> &[u32] {
        &self.active
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn total(&self) -> usize {
        (self.per * self.per * self.per) as usize
    }

    pub fn fraction(&self) -> f32 {
        self.active.len() as f32 / self.total() as f32
    }

    pub fn contains(&self, bx: u32, by: u32, bz: u32) -> bool {
        if bx >= self.per || by >= self.per || bz >= self.per {
            return false;
        }
        let id = bx + by * self.per + bz * self.per * self.per;
        self.active.binary_search(&id).is_ok()
    }

    pub fn xyz(&self, id: u32) -> (u32, u32, u32) {
        let per = self.per;
        (id % per, (id / per) % per, id / (per * per))
    }
}

fn surface_mask(field: &Field, n: u32) -> Vec<bool> {
    let m = n - 1;
    let mut mask = vec![false; (m * m * m) as usize];
    let at = |x: u32, y: u32, z: u32| field.values[(x + y * n + z * n * n) as usize];
    for z in 0..m {
        for y in 0..m {
            for x in 0..m {
                let mut neg = false;
                let mut pos = false;
                for dz in 0..2 {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let v = at(x + dx, y + dy, z + dz);
                            if v < 0.0 {
                                neg = true;
                            } else if v > 0.0 {
                                pos = true;
                            }
                        }
                    }
                }
                mask[(x + y * m + z * m * m) as usize] = neg && pos;
            }
        }
    }
    mask
}

fn dilate(mask: &[bool], m: u32, margin: u32) -> Vec<bool> {
    if margin == 0 {
        return mask.to_vec();
    }
    let mut out = vec![false; mask.len()];
    for z in 0..m {
        for y in 0..m {
            for x in 0..m {
                if !mask[(x + y * m + z * m * m) as usize] {
                    continue;
                }
                let r = margin as i32;
                for dz in -r..=r {
                    let zz = z as i32 + dz;
                    if !(0..m as i32).contains(&zz) {
                        continue;
                    }
                    for dy in -r..=r {
                        let yy = y as i32 + dy;
                        if !(0..m as i32).contains(&yy) {
                            continue;
                        }
                        for dx in -r..=r {
                            let xx = x as i32 + dx;
                            if (0..m as i32).contains(&xx) {
                                let i = xx as u32 + yy as u32 * m + zz as u32 * m * m;
                                out[i as usize] = true;
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn block_hits(
    mask: &[bool],
    m: u32,
    bx: u32,
    by: u32,
    bz: u32,
    block: u32,
    fine_n: u32,
    ratio: f32,
) -> bool {
    let lo = |b: u32| b * block;
    let hi = |b: u32| ((b + 1) * block).min(fine_n - 1);
    let c0 = (lo(bx) as f32 * ratio).floor() as i32;
    let c1 = (hi(bx) as f32 * ratio).floor() as i32;
    let d0 = (lo(by) as f32 * ratio).floor() as i32;
    let d1 = (hi(by) as f32 * ratio).floor() as i32;
    let e0 = (lo(bz) as f32 * ratio).floor() as i32;
    let e1 = (hi(bz) as f32 * ratio).floor() as i32;
    for z in e0..=e1 {
        if !(0..m as i32).contains(&z) {
            continue;
        }
        for y in d0..=d1 {
            if !(0..m as i32).contains(&y) {
                continue;
            }
            for x in c0..=c1 {
                if (0..m as i32).contains(&x) {
                    let i = x as u32 + y as u32 * m + z as u32 * m * m;
                    if mask[i as usize] {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::{fit_inputs, voxelize};
    use super::*;
    use crate::gauss::{add_floaters, blob_sphere_gaussians, sphere_gaussians, torus_gaussians};

    fn fine_surface_blocks(field: &Field, block: u32) -> Vec<u32> {
        let n = field.grid.n;
        let per = n.div_ceil(block);
        let mut out = Vec::new();
        for bz in 0..per {
            for by in 0..per {
                for bx in 0..per {
                    let x1 = ((bx + 1) * block).min(n);
                    let y1 = ((by + 1) * block).min(n);
                    let z1 = ((bz + 1) * block).min(n);
                    let mut neg = false;
                    let mut pos = false;
                    for z in bz * block..z1 {
                        for y in by * block..y1 {
                            let base = (y * n + z * n * n) as usize;
                            for x in bx * block..x1 {
                                let v = field.values[base + x as usize];
                                if v < 0.0 {
                                    neg = true;
                                } else if v > 0.0 {
                                    pos = true;
                                }
                            }
                        }
                    }
                    if neg && pos {
                        out.push(bx + by * per + bz * per * per);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn coarse_surface_blocks_cover_fine_surface() {
        let cases: Vec<(&str, Vec<crate::gauss::Gaussian>, VoxelMode)> = vec![
            (
                "sphere",
                sphere_gaussians(20_000, 0.75, 0.99),
                VoxelMode::Surfel,
            ),
            (
                "torus",
                torus_gaussians(180, 110, 0.55, 0.22, 0.99),
                VoxelMode::Surfel,
            ),
            (
                "blobs",
                {
                    let mut g = blob_sphere_gaussians(5_000, 0.75, 0.05, 0.95, 7);
                    add_floaters(&mut g, 300, 7 ^ 0x9e37_79b9);
                    g
                },
                VoxelMode::Density,
            ),
        ];
        for (name, gs, mode) in cases {
            let opts = VoxelizeOptions {
                mode,
                n: 128,
                threads: 24,
                ..VoxelizeOptions::default()
            };
            let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
            let fine = voxelize(&gs, &opts).expect("fine field");
            let truth = fine_surface_blocks(&fine, 16);
            for margin in [0, 1] {
                let blocks =
                    SurfaceBlocks::detect(&gs, &opts, grid, 16, margin, 4).expect("detect");
                let missed = truth
                    .iter()
                    .filter(|&&id| {
                        let (x, y, z) = blocks.xyz(id);
                        !blocks.contains(x, y, z)
                    })
                    .count();
                println!(
                    "{name} margin={margin}: truth={} active={} ({:.1}%) missed={missed}",
                    truth.len(),
                    blocks.active_count(),
                    blocks.fraction() * 100.0
                );
                assert_eq!(missed, 0, "{name} margin={margin} misses {missed} blocks");
                assert!(
                    blocks.fraction() < 0.95,
                    "{name} margin={margin} selects too much"
                );
            }
        }
    }

    #[test]
    #[ignore = "diagnostic 256 coarse block recall"]
    fn coarse_surface_blocks_256_recall() {
        let cases: Vec<(&str, Vec<Gaussian>, VoxelMode)> = vec![
            (
                "torus",
                torus_gaussians(180, 110, 0.55, 0.22, 0.99),
                VoxelMode::Surfel,
            ),
            (
                "blobs",
                {
                    let mut g = blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
                    add_floaters(&mut g, 300, 7 ^ 0x9e37_79b9);
                    g
                },
                VoxelMode::Density,
            ),
        ];
        for (name, gs, mode) in cases {
            let opts = VoxelizeOptions {
                mode,
                n: 256,
                threads: 24,
                ..VoxelizeOptions::default()
            };
            let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
            let fine = voxelize(&gs, &opts).expect("fine");
            for block in [16, 32] {
                let truth = fine_surface_blocks(&fine, block);
                for margin in [0, 1] {
                    let b =
                        SurfaceBlocks::detect(&gs, &opts, grid, block, margin, 4).expect("detect");
                    let missed = truth
                        .iter()
                        .filter(|&&id| {
                            let (x, y, z) = b.xyz(id);
                            !b.contains(x, y, z)
                        })
                        .count();
                    println!(
                        "{name} b={block} m={margin}: truth={} active={} ({:.1}%) missed={missed}",
                        truth.len(),
                        b.active_count(),
                        b.fraction() * 100.0
                    );
                    assert_eq!(missed, 0, "{name} b={block} m={margin}");
                }
            }
        }
    }

    #[test]
    fn coarse_grid_covers_same_domain() {
        let gs = sphere_gaussians(1_000, 0.75, 0.99);
        let opts = VoxelizeOptions {
            n: 128,
            ..VoxelizeOptions::default()
        };
        let (grid, _) = fit_inputs(&gs, &opts).expect("fit");
        let b = SurfaceBlocks::detect(&gs, &opts, grid, 16, 1, 4).expect("detect");
        let c = b.coarse();
        assert_eq!(c.grid.n, 32);
        assert_eq!(c.grid.origin, grid.origin);
        let fine_extent = (grid.n - 1) as f32 * grid.h;
        let coarse_extent = (c.grid.n - 1) as f32 * c.grid.h;
        assert!((fine_extent - coarse_extent).abs() < 1e-4);
    }
}
