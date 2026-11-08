//! 共享判据模块（v5 阶段 0 / F03+F04+F05 修复）。
//!
//! 性质：纯 CPU、无 GPU 依赖——质量门自身可在普通 runner 上验证；
//! 负例注入见 `gate_selftest.rs`（全 NaN / 单点 Inf / 缩短输出 / 空域 / 翻面 / 残缺索引）。
//!
//! 三条修复的落点：
//! - F03：带内对拍不再静默跳过非有限值——比较域内 GPU 非有限 = 硬红；域空 = 硬红；
//!   长度必须一致（`zip` 静默截断是同族误绿路径）。
//! - F04：四边形规范化从「顶点排序」改为「仅循环旋转」——排序会抹掉绕序，
//!   翻面三角被并成同一键；重复面改由多重集（排序向量相等）自然保留。
//! - F05：位置对拍先断言有限、再聚合误差——`f32::max` 遇 NaN 会静默吞点。

/// 三角形有向规范化：三个循环旋转中取最小者。
/// 保留绕序：(a,b,c) 与 (a,c,b) 得到**不同**键；循环移位得到**相同**键。
pub fn rot_min3<T: Copy + Ord>(t: [T; 3]) -> [T; 3] {
    let r1 = [t[1], t[2], t[0]];
    let r2 = [t[2], t[0], t[1]];
    let m = if r1 < t { r1 } else { t };
    if r2 < m { r2 } else { m }
}

/// 6 索引四边形 → (三角1, 三角2) 有向键：各三角 `rot_min3`，三角对按字典序摆正。
/// 三角对摆正只消除「先报哪枚三角」的自由度；对角线剖分由发射方格式固定，不属于对称。
pub fn quad_key6<T: Copy + Ord>(q: &[T; 6]) -> ([T; 3], [T; 3]) {
    let t1 = rot_min3([q[0], q[1], q[2]]);
    let t2 = rot_min3([q[3], q[4], q[5]]);
    if t1 > t2 { (t2, t1) } else { (t1, t2) }
}

/// 四边形流（每 6 索引一枚）经 `map` 映射后取**有向**规范集合（升序多重集）。
/// `map`：顶点号 → 键单元（恒等 = 槽位号；或映射到 cell 坐标 / cell 线性号）。
/// F04：翻面三角与正向三角给出不同键（排序版会给出相同键，绕序判据因此失效）。
pub fn directed_quads<T: Copy + Ord>(
    indices: &[u32],
    map: impl Fn(u32) -> T,
) -> Vec<([T; 3], [T; 3])> {
    assert_eq!(
        indices.len() % 6,
        0,
        "四边形流残缺：len={} 非 6 的倍数（truncated chunk）",
        indices.len()
    );
    let mut quads: Vec<([T; 3], [T; 3])> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| {
            quad_key6(&[
                map(q[0]),
                map(q[1]),
                map(q[2]),
                map(q[3]),
                map(q[4]),
                map(q[5]),
            ])
        })
        .collect();
    quads.sort();
    quads
}

/// 索引范围断言：所有索引必须落在有效顶点数内（越界索引是网格完整性缺陷，不是比较细节）。
pub fn assert_index_bounds(indices: &[u32], vertex_count: usize, ctx: &str) {
    if let Some(&bad) = indices.iter().find(|&&v| v as usize >= vertex_count) {
        panic!("{ctx}: 索引越界 {bad} ≥ 顶点数 {vertex_count}");
    }
}

/// 超阈误差明细：(err, 线性索引, cpu, gpu)，按误差降序（调用方取前几条打印最坏点）。
pub type WorstDetail = Vec<(f32, usize, f32, f32)>;

/// 带内逐点严格对拍（F03）。
///
/// 口径：
/// - 比较域 = CPU 侧有限值的格（带外槽按约定写 INF，是掩码不是数据）；
/// - 长度必须一致（显式断言，不信 `zip` 的静默截断）；
/// - 域内 GPU 出现非有限值 = 硬红（不再 `continue` 吞掉——域内全非有限时
///   旧实现 cnt=0 / max_err=0 仍绿，即 F03 的误绿路径）；
/// - 域必须非空（反空跑：空域上任何阈值断言都空洞成立）；
/// - 返回 (mean, max, 域大小) + 超过 `report_at` 的误差明细（按误差降序），
///   供调用方打印最坏点（明细元素 = (err, 线性索引, cpu, gpu)）。
pub fn band_err_strict(
    cpu: &[f32],
    gpu: &[f32],
    ctx: &str,
    report_at: f32,
) -> (f64, f32, usize, WorstDetail) {
    assert_eq!(
        cpu.len(),
        gpu.len(),
        "{ctx}: CPU/GPU 输出长度不一致 cpu={} gpu={}",
        cpu.len(),
        gpu.len()
    );
    let mut max_err = 0f32;
    let mut sum = 0f64;
    let mut cnt = 0usize;
    let mut bad: Vec<(usize, f32)> = Vec::new();
    let mut worst: Vec<(f32, usize, f32, f32)> = Vec::new();
    for (i, &c) in cpu.iter().enumerate() {
        if !c.is_finite() {
            continue; // 带外槽 = 掩码（CPU 参照定义比较域）
        }
        let g = gpu[i];
        if !g.is_finite() {
            bad.push((i, g));
            continue;
        }
        let e = (g - c).abs();
        if e > report_at {
            worst.push((e, i, c, g));
        }
        max_err = max_err.max(e);
        sum += e as f64;
        cnt += 1;
    }
    assert!(
        bad.is_empty(),
        "{ctx}: 比较域内 GPU 输出非有限 {} 处（首处 i={} val={:?}）——对拍作废",
        bad.len(),
        bad[0].0,
        bad[0].1
    );
    assert!(cnt > 0, "{ctx}: 比较域为空（CPU 参照全带外）——反空跑");
    worst.sort_by(|u, v| v.0.total_cmp(&u.0));
    (sum / cnt as f64, max_err, cnt, worst)
}

/// 带符号体积（F04 的方向判据；`mesh_volume` 取绝对值，方向须单独验）。
/// 对闭合定向网格：外法向（逆时针绕序）⇒ 正；翻全局绕序 ⇒ 变号。
pub fn signed_volume(positions: &[[f32; 3]], indices: &[u32]) -> f32 {
    let mut vol = 0f32;
    for t in indices.as_chunks::<3>().0 {
        let a = positions[t[0] as usize];
        let b = positions[t[1] as usize];
        let c = positions[t[2] as usize];
        vol += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    vol / 6.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rot_min3_preserves_winding() {
        // 循环移位 = 同一键；翻面 = 不同键（F04 的核心语义）
        assert_eq!(rot_min3([3u32, 1, 2]), [1, 2, 3]);
        assert_eq!(rot_min3([2u32, 3, 1]), [1, 2, 3]);
        assert_eq!(rot_min3([1u32, 2, 3]), [1, 2, 3]);
        assert_ne!(rot_min3([1u32, 3, 2]), [1, 2, 3]);
    }

    #[test]
    fn quad_key6_order_free() {
        let a = quad_key6(&[1u32, 2, 3, 4, 5, 6]);
        let b = quad_key6(&[4u32, 5, 6, 1, 2, 3]); // 三角对换序
        assert_eq!(a, b);
        let c = quad_key6(&[2u32, 3, 1, 4, 5, 6]); // 单三角循环移位
        assert_eq!(a, c);
    }

    #[test]
    fn directed_quads_keeps_multiplicity() {
        // 重复面保留两份（多重集）；残缺流直接红
        let dup = [1u32, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6];
        assert_eq!(directed_quads(&dup, |v| v).len(), 2);
    }

    #[test]
    #[should_panic(expected = "残缺")]
    fn directed_quads_rejects_residual() {
        let _ = directed_quads(&[1u32, 2, 3, 4, 5], |v| v);
    }

    #[test]
    #[should_panic(expected = "越界")]
    fn index_bounds_rejects_oob() {
        assert_index_bounds(&[0u32, 7], 7, "注入");
    }

    #[test]
    fn signed_volume_flips_with_winding() {
        // 单四面体：(0,0,0),(1,0,0),(0,1,0),(0,0,1) 有向体积 = 1/6；翻绕序 = −1/6
        let p = [
            [0.0f32; 3],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let fwd = [0u32, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3]; // 外法向绕序（和为正）
        let v = signed_volume(&p, &fwd);
        assert!((v - 1.0 / 6.0).abs() < 1e-6, "signed={v}");
        let flipped: Vec<u32> = fwd.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
        let v2 = signed_volume(&p, &flipped);
        assert!((v2 + 1.0 / 6.0).abs() < 1e-6, "flipped={v2}");
    }

    #[test]
    #[should_panic(expected = "非有限")]
    fn band_err_rejects_all_nan_gpu() {
        let cpu = vec![0.5f32; 4];
        let gpu = vec![f32::NAN; 4];
        let _ = band_err_strict(&cpu, &gpu, "注入:全NaN", 0.05);
    }

    #[test]
    #[should_panic(expected = "非有限")]
    fn band_err_rejects_single_inf() {
        let cpu = vec![0.5f32; 4];
        let mut gpu = vec![0.5f32; 4];
        gpu[2] = f32::INFINITY;
        let _ = band_err_strict(&cpu, &gpu, "注入:单点Inf", 0.05);
    }

    #[test]
    #[should_panic(expected = "长度")]
    fn band_err_rejects_short_output() {
        let cpu = vec![0.5f32; 4];
        let gpu = vec![0.5f32; 3]; // 缩短输出：zip 会静默截断，这里必须红
        let _ = band_err_strict(&cpu, &gpu, "注入:缩短输出", 0.05);
    }

    #[test]
    #[should_panic(expected = "比较域为空")]
    fn band_err_rejects_empty_domain() {
        let cpu = vec![f32::INFINITY; 4]; // 全带外
        let gpu = vec![0.5f32; 4];
        let _ = band_err_strict(&cpu, &gpu, "注入:空域", 0.05);
    }

    #[test]
    fn band_err_accepts_good_pair() {
        let cpu = vec![f32::INFINITY, 0.5, 0.5, 0.5];
        let gpu = vec![0.0, 0.5 + 1e-3, 0.5 - 1e-3, 0.5];
        let (mean, max, cnt, worst) = band_err_strict(&cpu, &gpu, "好对", 0.05);
        assert_eq!(cnt, 3);
        assert!(max <= 2e-3 && mean <= 2e-3);
        assert!(worst.is_empty());
    }
}
