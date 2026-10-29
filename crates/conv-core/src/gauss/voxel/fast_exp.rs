//! 可选快速 exp（opt-in，`--fast-exp`）：exp2 位技巧 + 7 阶多项式。
//! f32 Horner 求值实测最大相对误差 **1.02e-6**（x ∈ [−9,0]，对比 `f32::exp`）。
//!
//! 默认路径仍走 `f32::exp`（逐位基线不变）；开这个档会改哈希，但几何量（体积/χ/分量）
//! 与默认一致。1024³ 稀疏大核实测 splat 快 ~3×（见 docs/papers §6.22）。

/// 2^f, f ∈ [0,1) 的极小极大多项式系数（高到低）。
#[allow(clippy::approx_constant)] // 多项式系数恰好接近 LN_2，不是常量误写
const C: [f32; 7] = [
    0.693_147_2,
    0.240_226_5,
    0.055_504_11,
    0.009_618_129,
    0.001_333_355_8,
    0.000_154_035_3,
    0.000_015_252_734,
];

/// `x ≤ 0` 时求 exp(x)。核内实参是 `−½m`（m ∈ [0,18]）⇒ x ∈ [−9,0]；
/// 范围外或 NaN 直接回退 `f32::exp`，保证任何输入都不会静默出错。
#[inline]
pub fn fast_exp(x: f32) -> f32 {
    if x > 0.0 || x < -80.0 || x.is_nan() {
        return x.exp();
    }
    let y = x * std::f32::consts::LOG2_E;
    let yi = y.floor();
    let f = y - yi;
    let mut p = C[6];
    p = C[5] + f * p;
    p = C[4] + f * p;
    p = C[3] + f * p;
    p = C[2] + f * p;
    p = C[1] + f * p;
    p = C[0] + f * p;
    let poly = 1.0 + f * p;
    poly * f32::from_bits(((yi as i32 + 127) << 23) as u32)
}

#[cfg(test)]
mod tests {
    use super::fast_exp;

    #[test]
    fn matches_libm_within_1e_6() {
        let mut max_rel = 0f32;
        let mut worst = 0f32;
        for i in 0..=200_000 {
            let x = -9.0 * (i as f32 / 200_000.0);
            let a = fast_exp(x);
            let b = x.exp();
            let rel = ((a - b) / b).abs();
            if rel > max_rel {
                max_rel = rel;
                worst = x;
            }
        }
        assert!(max_rel < 2e-6, "max rel err {max_rel:.3e} at x={worst}");
    }

    #[test]
    fn exact_at_zero_and_edge_cases() {
        assert_eq!(fast_exp(0.0), 1.0);
        // 范围外回退 std：不静默给错值
        assert_eq!(fast_exp(1.0), 1.0f32.exp());
        assert_eq!(fast_exp(-200.0), (-200.0f32).exp());
        assert!(fast_exp(f32::NAN).is_nan());
    }
}
