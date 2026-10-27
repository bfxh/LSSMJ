//! 3D tile splat（可选档）：8³ tile、每 tile 独占输出、按 prepared 顺序建桶。
//! 与 z-slab 只差任务划分与写入位置；每个体素的 Gaussian 累加顺序不变。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    #[cfg(target_arch = "x86_64")]
    let simd = super::avx2_splat();
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
                #[cfg(target_arch = "x86_64")]
                if simd {
                    unsafe {
                        super::avx2::density_row(
                            g, grid, y, z, x0, xa, tile.x0, g.opacity, out, row,
                        );
                    }
                    continue;
                }
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

pub(crate) fn splat_density_tiles(
    prepared: &[Prepared],
    grid: &Grid,
    workers: usize,
) -> (Vec<f32>, usize) {
    let tiles = tiles(grid.n);
    let workers = workers.min(tiles.len()).max(1);
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
    (concat_density(&tiles, parts, grid.n), workers)
}

pub(crate) fn splat_surfel_tiles(
    prepared: &[Prepared],
    grid: &Grid,
    clamp_abs: f32,
    workers: usize,
) -> (Vec<f32>, Vec<f32>, usize) {
    let tiles = tiles(grid.n);
    let workers = workers.min(tiles.len()).max(1);
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
    let (num, den) = concat_surfel(&tiles, parts, grid.n);
    (num, den, workers)
}
