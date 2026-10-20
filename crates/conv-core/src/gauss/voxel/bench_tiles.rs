//! 3D tile 分桶实验（仅测试编译）：与现役 z-slab 逐位对拍 + 计时。

use super::*;
use crate::gauss::{bounds_of, sphere_gaussians};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const T: u32 = 8;
const TL: usize = (T * T * T) as usize;

#[derive(Clone, Copy)]
struct Tile {
    x0: u32,
    y0: u32,
    z0: u32,
}

fn tiles(n: u32) -> Vec<Tile> {
    let t = n.div_ceil(T);
    let mut out = Vec::with_capacity((t * t * t) as usize);
    for z in 0..t {
        for y in 0..t {
            for x in 0..t {
                out.push(Tile {
                    x0: x * T,
                    y0: y * T,
                    z0: z * T,
                });
            }
        }
    }
    out
}

fn build_tile_bins(prepared: &[Prepared], grid: &Grid, tiles: &[Tile]) -> Vec<Vec<u32>> {
    let per = grid.n.div_ceil(T);
    let mut bins: Vec<Vec<u32>> = (0..tiles.len()).map(|_| Vec::new()).collect();
    for (i, g) in prepared.iter().enumerate() {
        let r = splat_range(grid, g.mean, g.radius);
        if r.empty {
            continue;
        }
        let tx0 = r.lo[0] / T;
        let tx1 = r.hi[0] / T;
        let ty0 = r.lo[1] / T;
        let ty1 = r.hi[1] / T;
        let tz0 = r.lo[2] / T;
        let tz1 = r.hi[2] / T;
        for tz in tz0..=tz1 {
            for ty in ty0..=ty1 {
                for tx in tx0..=tx1 {
                    let ti = (tz * per * per + ty * per + tx) as usize;
                    bins[ti].push(i as u32);
                }
            }
        }
    }
    bins
}

fn splat_density_tile(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    tile: Tile,
    out: &mut [f32],
) {
    let n = grid.n;
    let x1 = (tile.x0 + T).min(n);
    let y1 = (tile.y0 + T).min(n);
    let z1 = (tile.z0 + T).min(n);
    for &gi in indices {
        let g = &prepared[gi as usize];
        let r = splat_range(grid, g.mean, g.radius);
        if r.empty {
            continue;
        }
        let x0 = r.lo[0].max(tile.x0);
        let xa = r.hi[0].min(x1 - 1);
        let y0 = r.lo[1].max(tile.y0);
        let ya = r.hi[1].min(y1 - 1);
        let z0 = r.lo[2].max(tile.z0);
        let za = r.hi[2].min(z1 - 1);
        if x0 > xa || y0 > ya || z0 > za {
            continue;
        }
        for z in z0..=za {
            let lz = (z - tile.z0) as usize;
            for y in y0..=ya {
                let ly = (y - tile.y0) as usize;
                let row = lz * (T as usize) * (T as usize) + ly * (T as usize);
                for x in x0..=xa {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    out[row + (x - tile.x0) as usize] += g.opacity * (-0.5 * m).exp();
                }
            }
        }
    }
}

fn splat_surfel_tile(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    tile: Tile,
    clamp_abs: f32,
    num: &mut [f32],
    den: &mut [f32],
) {
    let n = grid.n;
    let x1 = (tile.x0 + T).min(n);
    let y1 = (tile.y0 + T).min(n);
    let z1 = (tile.z0 + T).min(n);
    for &gi in indices {
        let g = &prepared[gi as usize];
        let r = splat_range(grid, g.mean, g.radius);
        if r.empty {
            continue;
        }
        let x0 = r.lo[0].max(tile.x0);
        let xa = r.hi[0].min(x1 - 1);
        let y0 = r.lo[1].max(tile.y0);
        let ya = r.hi[1].min(y1 - 1);
        let z0 = r.lo[2].max(tile.z0);
        let za = r.hi[2].min(z1 - 1);
        if x0 > xa || y0 > ya || z0 > za {
            continue;
        }
        for z in z0..=za {
            let lz = (z - tile.z0) as usize;
            for y in y0..=ya {
                let ly = (y - tile.y0) as usize;
                let row = lz * (T as usize) * (T as usize) + ly * (T as usize);
                for x in x0..=xa {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    let dist = dot(d, g.normal).clamp(-clamp_abs, clamp_abs);
                    let w = g.opacity * (-0.5 * m).exp();
                    let idx = row + (x - tile.x0) as usize;
                    num[idx] += w * dist;
                    den[idx] += w;
                }
            }
        }
    }
}

fn concat_density(tiles: &[Tile], mut parts: Vec<(usize, Vec<f32>)>, n: u32) -> Vec<f32> {
    parts.sort_by_key(|(i, _)| *i);
    let mut out = vec![0f32; (n * n * n) as usize];
    for (ti, local) in &parts {
        let t = tiles[*ti];
        let x1 = (t.x0 + T).min(n);
        let y1 = (t.y0 + T).min(n);
        let z1 = (t.z0 + T).min(n);
        for z in t.z0..z1 {
            for y in t.y0..y1 {
                for x in t.x0..x1 {
                    let li = ((z - t.z0) * T * T + (y - t.y0) * T + (x - t.x0)) as usize;
                    let gi = (x + y * n + z * n * n) as usize;
                    out[gi] = local[li];
                }
            }
        }
    }
    out
}

fn concat_surfel(
    tiles: &[Tile],
    mut parts: Vec<(usize, Vec<f32>, Vec<f32>)>,
    n: u32,
) -> (Vec<f32>, Vec<f32>) {
    parts.sort_by_key(|(i, _, _)| *i);
    let mut num = vec![0f32; (n * n * n) as usize];
    let mut den = vec![0f32; (n * n * n) as usize];
    for (ti, ln, ld) in &parts {
        let t = tiles[*ti];
        let x1 = (t.x0 + T).min(n);
        let y1 = (t.y0 + T).min(n);
        let z1 = (t.z0 + T).min(n);
        for z in t.z0..z1 {
            for y in t.y0..y1 {
                for x in t.x0..x1 {
                    let li = ((z - t.z0) * T * T + (y - t.y0) * T + (x - t.x0)) as usize;
                    let gi = (x + y * n + z * n * n) as usize;
                    num[gi] = ln[li];
                    den[gi] = ld[li];
                }
            }
        }
    }
    (num, den)
}

fn splat_density_tiles(prepared: &[Prepared], grid: &Grid, workers: usize) -> Vec<f32> {
    let tiles = tiles(grid.n);
    let bins = build_tile_bins(prepared, grid, &tiles);
    let next = AtomicUsize::new(0);
    let parts: Vec<(usize, Vec<f32>)> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let next = &next;
            let bins = &bins;
            let tiles = &tiles;
            handles.push(s.spawn(move || {
                let mut out = Vec::new();
                loop {
                    let ti = next.fetch_add(1, Ordering::Relaxed);
                    if ti >= tiles.len() {
                        break;
                    }
                    let mut local = vec![0f32; TL];
                    splat_density_tile(prepared, &bins[ti], grid, tiles[ti], &mut local);
                    out.push((ti, local));
                }
                out
            }));
        }
        let mut all = Vec::new();
        for h in handles {
            match h.join() {
                Ok(v) => all.extend(v),
                Err(_) => panic!("tile worker panicked"),
            }
        }
        all
    });
    concat_density(&tiles, parts, grid.n)
}

fn splat_surfel_tiles(
    prepared: &[Prepared],
    grid: &Grid,
    clamp_abs: f32,
    workers: usize,
) -> (Vec<f32>, Vec<f32>) {
    let tiles = tiles(grid.n);
    let bins = build_tile_bins(prepared, grid, &tiles);
    let next = AtomicUsize::new(0);
    let parts: Vec<(usize, Vec<f32>, Vec<f32>)> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let next = &next;
            let bins = &bins;
            let tiles = &tiles;
            handles.push(s.spawn(move || {
                let mut out = Vec::new();
                loop {
                    let ti = next.fetch_add(1, Ordering::Relaxed);
                    if ti >= tiles.len() {
                        break;
                    }
                    let mut num = vec![0f32; TL];
                    let mut den = vec![0f32; TL];
                    splat_surfel_tile(
                        prepared, &bins[ti], grid, tiles[ti], clamp_abs, &mut num, &mut den,
                    );
                    out.push((ti, num, den));
                }
                out
            }));
        }
        let mut all = Vec::new();
        for h in handles {
            match h.join() {
                Ok(v) => all.extend(v),
                Err(_) => panic!("tile worker panicked"),
            }
        }
        all
    });
    concat_surfel(&tiles, parts, grid.n)
}

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
        let (z, _) = parallel_splat_density(&d, &grid, 24);
        let z_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let tile = splat_density_tiles(&d, &grid, 24);
        let tile_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "density n={count} z-slab={z_ms:.1}ms 3d-tile={tile_ms:.1}ms speedup={:.2}x equal={}",
            z_ms / tile_ms,
            bits(&z, &tile)
        );
        assert!(bits(&z, &tile));

        let (s, clamp_abs) = prepare_surfel(&gs, &grid, &opts);
        let t = Instant::now();
        let (zn, zd, _) = parallel_splat_surfel(&s, &grid, clamp_abs, 24);
        let z_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (tn, td) = splat_surfel_tiles(&s, &grid, clamp_abs, 24);
        let tile_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "surfel  n={count} z-slab={z_ms:.1}ms 3d-tile={tile_ms:.1}ms speedup={:.2}x equal={}",
            z_ms / tile_ms,
            bits(&zn, &tn) && bits(&zd, &td)
        );
        assert!(bits(&zn, &tn) && bits(&zd, &td));
    }
}
