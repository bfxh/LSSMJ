//! 3D tile 对照（仅测试编译）：与现役 z-slab 逐位对拍 + 计时。

use super::tiles::{splat_density_tiles, splat_surfel_tiles};
use super::*;
use crate::gauss::{bounds_of, sphere_gaussians};
use std::time::Instant;

fn bits(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
#[ignore = "diagnostic 3D-tile experiment"]
fn compare_3d_tiles() {
    for count in [20_000usize, 100_000] {
        let gs = sphere_gaussians(count, 0.75, 0.99);
        let grid = Grid::fit(bounds_of(&gs), 128, 3).unwrap();
        let opts = VoxelizeOptions {
            n: 128,
            threads: 24,
            ..VoxelizeOptions::default()
        };
        let d = prepare_density(&gs, &grid, &opts);
        let t = Instant::now();
        let (z, _) = parallel_splat_density(&d, &grid, 24, 1.0, false);
        let z_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (tile, _) = splat_density_tiles(&d, &grid, 24, false);
        let tile_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "density n={count} z-slab={z_ms:.1}ms 3d-tile={tile_ms:.1}ms speedup={:.2}x equal={}",
            z_ms / tile_ms,
            bits(&z, &tile)
        );
        assert!(bits(&z, &tile));

        let (s, clamp_abs) = prepare_surfel(&gs, &grid, &opts);
        let t = Instant::now();
        let (zn, zd, _) = parallel_splat_surfel(&s, &grid, clamp_abs, 24, false);
        let z_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (tn, td, _) = splat_surfel_tiles(&s, &grid, clamp_abs, 24, false);
        let tile_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "surfel  n={count} z-slab={z_ms:.1}ms 3d-tile={tile_ms:.1}ms speedup={:.2}x equal={}",
            z_ms / tile_ms,
            bits(&zn, &tn) && bits(&zd, &td)
        );
        assert!(bits(&zn, &tn) && bits(&zd, &td));
    }
}
