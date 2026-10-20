//! 可选 SIMD prep：固定旋转序列的 cyclic Jacobi（3×3 对称阵），8 路 AVX2 批处理。
//!
//! 与默认自适应 Jacobi 的区别：旋转对按固定顺序 (0,1)(0,2)(1,2) 循环，不按最大非对角元
//! 选支——这样 8 个高斯可以在同一 AVX 指令里用相同的 p/q 更新，无需逐 lane 分支。
//! 代价：数值路径不同，eigen 的末位/基可能与默认路径不同 ⇒ 仅作为 opt-in，默认路径保持逐位。

use super::Eigen;

const SWEEPS: usize = 5;
const PAIRS: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

#[inline]
fn identity3() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

fn sort_eigen(a: [[f32; 3]; 3], v: [[f32; 3]; 3]) -> Eigen {
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let mut values = [0f32; 3];
    let mut vectors = [[0f32; 3]; 3];
    for (dst, &src) in order.iter().enumerate() {
        values[dst] = a[src][src];
        vectors[dst] = [v[0][src], v[1][src], v[2][src]];
    }
    Eigen { values, vectors }
}

/// 标量 cyclic Jacobi（无 AVX2 时的回退；也是 AVX2 的语义参照）。
#[allow(clippy::needless_range_loop)]
pub(crate) fn cyclic_eigen(cov: [[f32; 3]; 3]) -> Eigen {
    let mut a = cov;
    let mut v = identity3();
    for _ in 0..SWEEPS {
        for &(p, q) in &PAIRS {
            let apq = a[p][q];
            if apq.abs() < 1e-20 {
                continue;
            }
            let app = a[p][p];
            let aqq = a[q][q];
            let tau = (aqq - app) / (2.0 * apq);
            let t = if tau >= 0.0 {
                1.0 / (tau + (1.0 + tau * tau).sqrt())
            } else {
                -1.0 / (-tau + (1.0 + tau * tau).sqrt())
            };
            let c = 1.0 / (1.0 + t * t).sqrt();
            let s = t * c;
            a[p][p] = app - t * apq;
            a[q][q] = aqq + t * apq;
            a[p][q] = 0.0;
            a[q][p] = 0.0;
            for k in 0..3 {
                if k != p && k != q {
                    let akp = a[k][p];
                    let akq = a[k][q];
                    a[k][p] = c * akp - s * akq;
                    a[p][k] = a[k][p];
                    a[k][q] = s * akp + c * akq;
                    a[q][k] = a[k][q];
                }
            }
            for k in 0..3 {
                let vkp = v[k][p];
                let vkq = v[k][q];
                v[k][p] = c * vkp - s * vkq;
                v[k][q] = s * vkp + c * vkq;
            }
        }
    }
    sort_eigen(a, v)
}

pub(crate) fn eigen_batch(covs: &[[[f32; 3]; 3]]) -> Vec<Eigen> {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: runtime feature detection above.
            return unsafe { eigen_avx2(covs) };
        }
    }
    covs.iter().map(|c| cyclic_eigen(*c)).collect()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[allow(unsafe_op_in_unsafe_fn, clippy::needless_range_loop)]
unsafe fn eigen_avx2(covs: &[[[f32; 3]; 3]]) -> Vec<Eigen> {
    use std::arch::x86_64::*;

    let n = covs.len();
    let mut out = Vec::with_capacity(n);
    let zero = _mm256_setzero_ps();
    let one = _mm256_set1_ps(1.0);
    let two = _mm256_set1_ps(2.0);
    let eps = _mm256_set1_ps(1e-20);
    let sign = _mm256_set1_ps(-0.0);

    let mut base = 0usize;
    while base < n {
        let m = (n - base).min(8);
        let mut a = [[zero; 3]; 3];
        let mut v = [[zero; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                let mut arr = [0f32; 8];
                for l in 0..m {
                    arr[l] = covs[base + l][i][j];
                }
                a[i][j] = _mm256_loadu_ps(arr.as_ptr());
            }
        }
        for i in 0..3 {
            for j in 0..3 {
                v[i][j] = if i == j { one } else { zero };
            }
        }

        for _ in 0..SWEEPS {
            for &(p, q) in &PAIRS {
                let app = a[p][p];
                let aqq = a[q][q];
                let apq = a[p][q];
                let abs_apq = _mm256_andnot_ps(sign, apq);
                let small = _mm256_cmp_ps(abs_apq, eps, _CMP_LT_OQ);
                let apq_safe = _mm256_blendv_ps(apq, one, small);
                let tau = _mm256_div_ps(_mm256_sub_ps(aqq, app), _mm256_mul_ps(two, apq_safe));
                let abs_tau = _mm256_andnot_ps(sign, tau);
                let denom = _mm256_add_ps(
                    abs_tau,
                    _mm256_sqrt_ps(_mm256_add_ps(one, _mm256_mul_ps(tau, tau))),
                );
                let sgn = _mm256_or_ps(_mm256_and_ps(tau, sign), one);
                let mut t = _mm256_div_ps(sgn, denom);
                t = _mm256_blendv_ps(t, zero, small);
                let c = _mm256_div_ps(one, _mm256_sqrt_ps(_mm256_add_ps(one, _mm256_mul_ps(t, t))));
                let s = _mm256_mul_ps(t, c);
                let tapq = _mm256_mul_ps(t, apq);
                a[p][p] = _mm256_sub_ps(app, tapq);
                a[q][q] = _mm256_add_ps(aqq, tapq);
                a[p][q] = zero;
                a[q][p] = zero;
                for k in 0..3 {
                    if k != p && k != q {
                        let akp = a[k][p];
                        let akq = a[k][q];
                        let nkp = _mm256_sub_ps(_mm256_mul_ps(c, akp), _mm256_mul_ps(s, akq));
                        let nkq = _mm256_add_ps(_mm256_mul_ps(s, akp), _mm256_mul_ps(c, akq));
                        a[k][p] = nkp;
                        a[p][k] = nkp;
                        a[k][q] = nkq;
                        a[q][k] = nkq;
                    }
                }
                for k in 0..3 {
                    let vkp = v[k][p];
                    let vkq = v[k][q];
                    v[k][p] = _mm256_sub_ps(_mm256_mul_ps(c, vkp), _mm256_mul_ps(s, vkq));
                    v[k][q] = _mm256_add_ps(_mm256_mul_ps(s, vkp), _mm256_mul_ps(c, vkq));
                }
            }
        }

        let mut diag = [[0f32; 8]; 3];
        for i in 0..3 {
            _mm256_storeu_ps(diag[i].as_mut_ptr(), a[i][i]);
        }
        let mut cols = [[[0f32; 8]; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                _mm256_storeu_ps(cols[i][j].as_mut_ptr(), v[i][j]);
            }
        }
        for l in 0..m {
            let aa = [
                [diag[0][l], 0.0, 0.0],
                [0.0, diag[1][l], 0.0],
                [0.0, 0.0, diag[2][l]],
            ];
            let vv = [
                [cols[0][0][l], cols[0][1][l], cols[0][2][l]],
                [cols[1][0][l], cols[1][1][l], cols[1][2][l]],
                [cols[2][0][l], cols[2][1][l], cols[2][2][l]],
            ];
            out.push(sort_eigen(aa, vv));
        }
        base += 8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gauss::sphere_gaussians;

    #[test]
    fn cyclic_eigenvalues_match_adaptive() {
        let gs = sphere_gaussians(2_000, 0.75, 0.99);
        let covs: Vec<_> = gs.iter().map(|g| g.covariance()).collect();
        let ours = eigen_batch(&covs);
        assert_eq!(ours.len(), gs.len());
        for (g, e) in gs.iter().zip(&ours) {
            let a = g.eigen();
            for k in 0..3 {
                let scale = a.values[k].abs().max(1.0);
                let err = (a.values[k] - e.values[k]).abs();
                assert!(
                    err <= 1e-6 * scale,
                    "eigenvalue mismatch: adaptive {:?} cyclic {:?} err={err}",
                    a.values,
                    e.values
                );
            }
        }
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::gauss::sphere_gaussians;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    #[ignore = "diagnostic SIMD eigen probe"]
    fn bench_eigen_batch() {
        #[cfg(target_arch = "x86_64")]
        println!("avx2 detected={}", std::is_x86_feature_detected!("avx2"));
        let gs = sphere_gaussians(100_000, 0.75, 0.99);
        let covs: Vec<_> = gs.iter().map(|g| g.covariance()).collect();
        let t = Instant::now();
        let e = eigen_batch(&covs);
        let batch_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        for g in &gs {
            black_box(g.eigen());
        }
        let adaptive_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "eigen 100k: adaptive={adaptive_ms:.1}ms batch={batch_ms:.1}ms speedup={:.2}x",
            adaptive_ms / batch_ms
        );
        assert_eq!(e.len(), gs.len());
    }
}
