//! 判据器械自检（v5 阶段 0）：比较器负例注入 + Lcg 契约。纯 CPU、无 GPU——
//! 质量门自身在普通 runner 可验证；GPU 测试的严格化都委托给 `common`，本文件是它的验收面。
//!
//! 负例矩阵（v5 §F03 验收：注入后各自失败且失败原因明确）：
//! 全 NaN / 单点 Inf / 缩短输出 / 空域 ⇒ `band_err_strict` 红（正例走 `common` 内嵌测试）；
//! 翻面 / 残缺索引 / 越界 ⇒ `directed_quads` / `assert_index_bounds` 红；
//! F11：`Lcg::next01` 的 [0,1) 契约以已知序列（公式复算）+ 全域覆盖钉死。

mod common;

use common::{assert_index_bounds, band_err_strict, directed_quads};
use conv_core::Lcg;

// ---- F11 · Lcg::next01 契约 ----

#[test]
fn lcg_next01_known_sequence() {
    // 已知序列：测试内独立复算 LCG（不调库），逐位钉住「高 24 位 × 2^-24」口径。
    // 更改采样序列必须换约（gaussian 金样已随本次修复披露换约）。
    const A: u64 = 6364136223846793005;
    const C: u64 = 1442695040888963407;
    let mut s = 0x5eed_2026_1007u64;
    let mut rng = Lcg::new(0x5eed_2026_1007);
    for k in 0..8 {
        s = s.wrapping_mul(A).wrapping_add(C);
        let expect = ((s >> 40) as f32) * (1.0 / (1u64 << 24) as f32);
        let got = rng.next01();
        assert_eq!(
            got.to_bits(),
            expect.to_bits(),
            "Lcg 序列第 {k} 步漂移：got={got} expect={expect}"
        );
    }
}

#[test]
fn lcg_next01_covers_domain() {
    // 全域覆盖 + 人工边界：旧实现（>>33 除 u32::MAX）值域实为 [0, 0.5]，
    // max > 0.999 与 mean∈[0.48,0.52) 两条都会红——本判据对旧实现是真红。
    let mut rng = Lcg::new(0xfeed_beef);
    let (mut min, mut max, mut sum) = (1.0f32, 0.0f32, 0.0f64);
    for _ in 0..65536 {
        let v = rng.next01();
        assert!((0.0..1.0).contains(&v), "越界值 {v}（契约 [0,1)）");
        min = min.min(v);
        max = max.max(v);
        sum += v as f64;
    }
    let mean = sum / 65536.0;
    println!("Lcg 64K 采样：min={min} max={max} mean={mean:.4}");
    assert!(min < 0.001, "下端未覆盖：min={min}");
    assert!(
        max > 0.999,
        "上端未覆盖：max={max}（旧缺陷的签名是 max≈0.5）"
    );
    assert!((0.48..0.52).contains(&mean), "均值偏出全域半区：{mean}");
}

// ---- F03 · band_err_strict 负例矩阵（正例与同族负例在 common::tests）----

#[test]
#[should_panic(expected = "非有限")]
fn injection_all_nan_gpu_fails() {
    let cpu = vec![0.5f32; 64];
    let gpu = vec![f32::NAN; 64]; // 域内全 NaN：旧路径 cnt=0/max=0 仍绿
    let _ = band_err_strict(&cpu, &gpu, "注入:全NaN", 0.05);
}

#[test]
#[should_panic(expected = "非有限")]
fn injection_single_inf_fails() {
    let cpu = vec![0.5f32; 64];
    let mut gpu = vec![0.5f32; 64];
    gpu[31] = f32::INFINITY;
    let _ = band_err_strict(&cpu, &gpu, "注入:单点Inf", 0.05);
}

#[test]
#[should_panic(expected = "长度")]
fn injection_truncated_output_fails() {
    let cpu = vec![0.5f32; 64];
    let gpu = vec![0.5f32; 48]; // 缩短输出：zip 口径会静默截断
    let _ = band_err_strict(&cpu, &gpu, "注入:缩短输出", 0.05);
}

#[test]
#[should_panic(expected = "比较域为空")]
fn injection_empty_domain_fails() {
    let cpu = vec![f32::INFINITY; 64];
    let gpu = vec![0.5f32; 64];
    let _ = band_err_strict(&cpu, &gpu, "注入:空域", 0.05);
}

// ---- F04 · 有向规范化负例 ----

#[test]
fn directed_quads_separates_flipped_triangle() {
    // F04 验收「单个三角翻面被检出」：排序版规范化下 (1,2,3) 与 (1,3,2) 同键（漏检）；
    // 有向版下两者不同键。
    let fwd = directed_quads(&[1u32, 2, 3, 4, 5, 6], |v| v);
    let flip = directed_quads(&[1u32, 3, 2, 4, 5, 6], |v| v);
    assert_ne!(fwd, flip, "翻面三角未被绕序判据检出");
    // 整体翻面 = 每枚三角都翻 ⇒ 全部键都变 ⇒ 与正向集合不相等
    let global_flip = directed_quads(&[2u32, 1, 3, 5, 4, 6], |v| v);
    assert_ne!(fwd, global_flip);
}

#[test]
#[should_panic(expected = "残缺")]
fn injection_residual_index_stream_fails() {
    let _ = directed_quads(&[1u32, 2, 3, 4, 5], |v| v); // 5 索引：6 不尽
}

#[test]
#[should_panic(expected = "越界")]
fn injection_oob_index_fails() {
    assert_index_bounds(&[0u32, 1, 2, 9_999], 4, "注入:越界索引");
}

#[test]
fn duplicate_faces_stay_counted() {
    // F04 验收「重复面保留」：排序多重集长度不变
    let dup = vec![1u32, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6];
    let uniq = vec![1u32, 2, 3, 4, 5, 6];
    assert_eq!(directed_quads(&dup, |v| v).len(), 2);
    assert_eq!(directed_quads(&uniq, |v| v).len(), 1);
}
