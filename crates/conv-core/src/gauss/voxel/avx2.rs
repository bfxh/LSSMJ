//! AVX2 x 行内核（density）：一次算 8 个体素的 Mahalanobis。
//!
//! 只向量化 `max_maha`；exp 与累加仍按 x 升序标量执行。
//! 运算顺序与标量版逐项一致（无 FMA/无重排）⇒ 输出逐位相同。
//! surfel 的 A/B 没有收益，保持标量。

#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]

use super::*;
use std::arch::x86_64::*;

pub(super) fn available() -> bool {
    static AVX2: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVX2.get_or_init(|| std::is_x86_feature_detected!("avx2"))
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn maha8(g: &Prepared, dx: __m256, dy: __m256, dz: __m256) -> __m256 {
    let mut m = _mm256_setzero_ps();
    for (v, &l) in g.eigen.vectors.iter().zip(g.lam.iter()) {
        let y = _mm256_add_ps(
            _mm256_add_ps(
                _mm256_mul_ps(_mm256_set1_ps(v[0]), dx),
                _mm256_mul_ps(_mm256_set1_ps(v[1]), dy),
            ),
            _mm256_mul_ps(_mm256_set1_ps(v[2]), dz),
        );
        m = _mm256_add_ps(m, _mm256_div_ps(_mm256_mul_ps(y, y), _mm256_set1_ps(l)));
    }
    m
}

#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn density_row(
    g: &Prepared,
    grid: &Grid,
    y: u32,
    z: u32,
    x0: u32,
    x1: u32,
    x_base: u32,
    op: f32,
    out: &mut [f32],
    row: usize,
) {
    let h = grid.h;
    let dy = _mm256_set1_ps(grid.origin[1] + y as f32 * h - g.mean[1]);
    let dz = _mm256_set1_ps(grid.origin[2] + z as f32 * h - g.mean[2]);
    let org = _mm256_set1_ps(grid.origin[0]);
    let hh = _mm256_set1_ps(h);
    let mx = _mm256_set1_ps(g.mean[0]);
    let mut x = x0;
    while x + 7 <= x1 {
        let xs = _mm256_setr_ps(
            x as f32,
            (x + 1) as f32,
            (x + 2) as f32,
            (x + 3) as f32,
            (x + 4) as f32,
            (x + 5) as f32,
            (x + 6) as f32,
            (x + 7) as f32,
        );
        let dx = _mm256_sub_ps(_mm256_add_ps(org, _mm256_mul_ps(xs, hh)), mx);
        let m = maha8(g, dx, dy, dz);
        let mut ms = [0f32; 8];
        _mm256_storeu_ps(ms.as_mut_ptr(), m);
        for (l, &m) in ms.iter().enumerate() {
            if m > 18.0 {
                continue;
            }
            let idx = row + (x - x_base) as usize + l;
            out[idx] += op * (-0.5 * m).exp();
        }
        x += 8;
    }
    while x <= x1 {
        let p = grid.world_of(x, y, z);
        let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
        let m = max_maha(&g.eigen, g.lam, d);
        if m <= 18.0 {
            out[row + (x - x_base) as usize] += op * (-0.5 * m).exp();
        }
        x += 1;
    }
}

/// u8/u16 定点量化行内核：与标量 `splat_quant_chunk` 逐位一致（只向量化 maha，无 FMA/无重排）。
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn quant_row<T: super::quant::Acc>(
    g: &Prepared,
    grid: &Grid,
    y: u32,
    z: u32,
    x0: u32,
    x1: u32,
    scale: f32,
    out: &mut [T],
    row: usize,
) {
    let h = grid.h;
    let dy = _mm256_set1_ps(grid.origin[1] + y as f32 * h - g.mean[1]);
    let dz = _mm256_set1_ps(grid.origin[2] + z as f32 * h - g.mean[2]);
    let org = _mm256_set1_ps(grid.origin[0]);
    let hh = _mm256_set1_ps(h);
    let mx = _mm256_set1_ps(g.mean[0]);
    let mut x = x0;
    while x + 7 <= x1 {
        let xs = _mm256_setr_ps(
            x as f32,
            (x + 1) as f32,
            (x + 2) as f32,
            (x + 3) as f32,
            (x + 4) as f32,
            (x + 5) as f32,
            (x + 6) as f32,
            (x + 7) as f32,
        );
        let dx = _mm256_sub_ps(_mm256_add_ps(org, _mm256_mul_ps(xs, hh)), mx);
        let m = maha8(g, dx, dy, dz);
        let mut ms = [0f32; 8];
        _mm256_storeu_ps(ms.as_mut_ptr(), m);
        for (l, &m) in ms.iter().enumerate() {
            if m > 18.0 {
                continue;
            }
            let q = (g.opacity * (-0.5 * m).exp() * scale).round();
            if q < 1.0 {
                continue;
            }
            let i = row + x as usize + l;
            out[i] = out[i].add_sat(q as u32);
        }
        x += 8;
    }
    while x <= x1 {
        let p = grid.world_of(x, y, z);
        let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
        let m = max_maha(&g.eigen, g.lam, d);
        if m <= 18.0 {
            let q = (g.opacity * (-0.5 * m).exp() * scale).round();
            if q >= 1.0 {
                let i = row + x as usize;
                out[i] = out[i].add_sat(q as u32);
            }
        }
        x += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{available, density_row};
    use crate::gauss::sphere_gaussians;

    fn prepared(g: &crate::gauss::Gaussian) -> Prepared {
        let eigen = g.eigen();
        Prepared {
            mean: g.mean,
            opacity: g.opacity,
            eigen,
            lam: eigen.values.map(|v| v.max(1e-6)),
            radius: [0.1; 3],
            normal: eigen.vectors[0],
        }
    }

    fn scalar_density(g: &Prepared, grid: &Grid, out: &mut [f32], n: u32) {
        for z in 0..n {
            for y in 0..n {
                let row = (y * n + z * n * n) as usize;
                for x in 0..n {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m <= 18.0 {
                        out[row + x as usize] += g.opacity * (-0.5 * m).exp();
                    }
                }
            }
        }
    }

    #[test]
    fn avx2_density_row_matches_scalar_bitwise() {
        if !available() {
            eprintln!("AVX2 unavailable; skip");
            return;
        }
        let gs = sphere_gaussians(4, 0.75, 0.99);
        let grid = Grid {
            n: 24,
            origin: [-1.2, -1.2, -1.2],
            h: 0.1,
        };
        let n = grid.n;
        let count = (n * n * n) as usize;
        for g in &gs {
            let p = prepared(g);
            let mut a = vec![0f32; count];
            let mut b = vec![0f32; count];
            scalar_density(&p, &grid, &mut a, n);
            for z in 0..n {
                for y in 0..n {
                    let row = (y * n + z * n * n) as usize;
                    unsafe {
                        density_row(&p, &grid, y, z, 0, n - 1, 0, p.opacity, &mut b, row);
                    }
                }
            }
            assert!(
                a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()),
                "density AVX2 != scalar"
            );
        }
    }

    fn scalar_quant<T: super::super::quant::Acc>(
        g: &Prepared,
        grid: &Grid,
        scale: f32,
        out: &mut [T],
        n: u32,
    ) {
        for z in 0..n {
            for y in 0..n {
                let row = (y * n + z * n * n) as usize;
                for x in 0..n {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    let q = (g.opacity * (-0.5 * m).exp() * scale).round();
                    if q < 1.0 {
                        continue;
                    }
                    out[row + x as usize] = out[row + x as usize].add_sat(q as u32);
                }
            }
        }
    }

    #[test]
    fn avx2_quant_row_matches_scalar_bitwise() {
        use super::super::quant::Acc;
        if !available() {
            eprintln!("AVX2 unavailable; skip");
            return;
        }
        let gs = sphere_gaussians(4, 0.75, 0.99);
        let grid = Grid {
            n: 24,
            origin: [-1.2, -1.2, -1.2],
            h: 0.1,
        };
        let n = grid.n;
        let count = (n * n * n) as usize;
        for g in &gs {
            let p = prepared(g);
            let scale = 900.0f32;
            let mut a16 = vec![0u16; count];
            let mut b16 = vec![0u16; count];
            scalar_quant::<u16>(&p, &grid, scale, &mut a16, n);
            for z in 0..n {
                for y in 0..n {
                    let row = (y * n + z * n * n) as usize;
                    unsafe {
                        super::quant_row(&p, &grid, y, z, 0, n - 1, scale, &mut b16, row);
                    }
                }
            }
            assert_eq!(a16, b16, "quant u16 AVX2 != scalar");

            let mut a8 = vec![0u8; count];
            let mut b8 = vec![0u8; count];
            scalar_quant::<u8>(&p, &grid, scale, &mut a8, n);
            for z in 0..n {
                for y in 0..n {
                    let row = (y * n + z * n * n) as usize;
                    unsafe {
                        super::quant_row(&p, &grid, y, z, 0, n - 1, scale, &mut b8, row);
                    }
                }
            }
            assert_eq!(a8, b8, "quant u8 AVX2 != scalar");
        }
    }
}
