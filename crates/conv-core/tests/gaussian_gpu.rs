//! 高斯腿判据（T-GC-04 第一片）：恒等直通逐位（无转换=无漂移）+ 金样哈希（格式契约）
//! + 先验红金丝雀（判据必须能判红）。

use conv_core::{
    Lcg,
    gaussian::{GaussianCloud, cloud_hash, gaussians_to_field, identity_roundtrip},
    jfa::headless_device,
    kernels::FIXED_POINT_SCALE,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const N: usize = 1000;

fn sample_cloud() -> GaussianCloud {
    let mut rng = Lcg::new(0x9c40);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(N);
    let mut log_scales: Vec<[f32; 3]> = Vec::with_capacity(N);
    let mut rotations: Vec<[f32; 4]> = Vec::with_capacity(N);
    let mut opacities: Vec<f32> = Vec::with_capacity(N);
    let mut colors: Vec<[f32; 3]> = Vec::with_capacity(N);
    for _ in 0..N {
        positions.push([
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
        ]);
        let s = |r: &mut Lcg| r.next01() * 4.0 - 4.0; // log 域
        log_scales.push([s(&mut rng), s(&mut rng), s(&mut rng)]);
        rotations.push([s(&mut rng), s(&mut rng), s(&mut rng), s(&mut rng)]);
        opacities.push(rng.next01() * 8.0 - 4.0); // logit 域
        colors.push([rng.next01(), rng.next01(), rng.next01()]);
    }
    // 边界语料：±0 / 次正规 / ±inf / NaN——逐位保真须全覆盖
    positions[0] = [0.0, -0.0, f32::MIN_POSITIVE / 2.0];
    positions[1] = [f32::INFINITY, f32::NEG_INFINITY, f32::NAN];
    log_scales[0] = [-0.0, f32::MIN_POSITIVE / 2.0, 0.0];
    rotations[0] = [f32::NAN, -0.0, f32::INFINITY, 1.0];
    opacities[0] = -0.0;
    opacities[1] = f32::NAN;
    colors[0] = [f32::MIN_POSITIVE / 2.0, -0.0, f32::NAN];
    // SH 高阶平面（stride 3）：并入恒等判据（含位边界语料）
    let mut sh_rest: Vec<f32> = (0..N * 3).map(|_| rng.next01() * 2.0 - 1.0).collect();
    sh_rest[0] = -0.0;
    sh_rest[1] = f32::INFINITY;
    sh_rest[2] = f32::MIN_POSITIVE / 2.0;
    GaussianCloud::new(positions, log_scales, rotations, opacities, colors).with_sh_rest(sh_rest, 3)
}

fn flat3(p: &[[f32; 3]]) -> Vec<f32> {
    p.iter().flat_map(|v| v.iter().copied()).collect()
}

fn flat4(p: &[[f32; 4]]) -> Vec<f32> {
    p.iter().flat_map(|v| v.iter().copied()).collect()
}

fn assert_f32_slice_bits_eq(a: &[f32], b: &[f32], what: &str) {
    assert_eq!(a.len(), b.len(), "{what} 长度不一致");
    let diff = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count();
    assert_eq!(diff, 0, "{what} 逐位不一致（{diff} 项）");
}

#[test]
fn identity_roundtrip_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let cloud = sample_cloud();
    let rt = identity_roundtrip(&hd, &cloud, None);
    assert_eq!(rt.count, cloud.count);
    assert_f32_slice_bits_eq(
        &flat3(&rt.positions),
        &flat3(&cloud.positions),
        "position 平面",
    );
    assert_f32_slice_bits_eq(
        &flat3(&rt.log_scales),
        &flat3(&cloud.log_scales),
        "log_scale 平面",
    );
    assert_f32_slice_bits_eq(
        &flat4(&rt.rotations),
        &flat4(&cloud.rotations),
        "rotation 平面",
    );
    assert_f32_slice_bits_eq(&rt.opacities, &cloud.opacities, "opacity 平面");
    assert_f32_slice_bits_eq(&flat3(&rt.colors), &flat3(&cloud.colors), "color 平面");
    assert_eq!(rt.sh_rest_stride, 3);
    assert_f32_slice_bits_eq(&rt.sh_rest, &cloud.sh_rest, "SH 平面（stride 3）");
    assert_eq!(cloud_hash(&rt), cloud_hash(&cloud), "金样哈希不一致");
}

#[test]
fn identity_canary_detects_corruption() {
    // 先验红：单 bit 篡改必须被哈希与逐位判据同时抓住
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let cloud = sample_cloud();
    let mut rt = identity_roundtrip(&hd, &cloud, None);
    rt.positions[7][1] = f32::from_bits(rt.positions[7][1].to_bits() ^ 1);
    assert_ne!(
        cloud_hash(&rt),
        cloud_hash(&cloud),
        "金丝雀失败：单 bit 篡改未被哈希捕捉"
    );
    let diff = flat3(&rt.positions)
        .iter()
        .zip(flat3(&cloud.positions))
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count();
    assert_eq!(diff, 1, "金丝雀失败：逐位判据未定位到单 bit 篡改");
}

#[test]
fn golden_hash_pinned() {
    // 金样：格式契约的机器锚——平面集/次序/域约定任何改动都必须在此处披露
    let cloud = sample_cloud();
    let h = cloud_hash(&cloud);
    println!("golden hash: {h:#018x}");
    // 金样换约（2026-10-10，F11）：旧值 0x48f5cf547ce2a737 钉在 Lcg 修复前的采样序列上
    // ——next01 值域曾是 [0, 0.5]（31 位截断），修复后序列变更 ⇒ 云变更 ⇒ 换约。
    // 格式契约（平面集/次序/域/SH）本身未动。
    assert_eq!(
        h, 0x2dddec981ec291dd,
        "金样哈希漂移 —— 格式契约变更？（平面集/次序/域/SH）"
    );
}

// ---- 高斯→体素（矩阵边 11 第一片，锚 W15A-016/017：概率占据）----

#[test]
fn gauss_to_voxel_single_analytic() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    let cw = 16.0 * h - 1.0; // 体素 (16,16,16) 世界坐标
    let sig = 0.1f32;
    let cloud = GaussianCloud::new(
        vec![[cw, cw, cw]],
        vec![[sig.ln(), sig.ln(), sig.ln()]],
        vec![[0.0, 0.0, 0.0, 1.0]],
        vec![0.0], // logit 0 ⇒ 概率 0.5
        vec![[0.5, 0.5, 0.5]],
    );
    let f = gaussians_to_field(&hd, &cloud, n, None);
    let center = f[(16 + 16 * n + 16 * n * n) as usize];
    assert_eq!(center, 0.5, "中心读数应恰为 p=0.5（Q16 恰 32768）");
    // 相邻体素（沿 x 一格）：解析 0.5·exp(−½(h/σ)²)
    let v17 = f[(17 + 16 * n + 16 * n * n) as usize];
    let expect17 = 0.5 * (-0.5 * (h / sig).powi(2)).exp();
    println!("单高斯：中心 {center} / 邻格 {v17}（解析 {expect17:.6}）");
    assert!(
        (v17 - expect17).abs() <= 1.0 / FIXED_POINT_SCALE + 1e-6,
        "邻格读数偏离解析：{v17} vs {expect17}"
    );
    // 支撑外恰零
    assert_eq!(f[0], 0.0, "角点应在 3σ 支撑外（恰零）");
}

#[test]
fn gauss_to_voxel_opacity_semantics() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    let cw = 16.0 * h - 1.0;
    let sig = 0.1f32;
    let mk = |opacity: f32| {
        GaussianCloud::new(
            vec![[cw, cw, cw]],
            vec![[sig.ln(), sig.ln(), sig.ln()]],
            vec![[0.0, 0.0, 0.0, 1.0]],
            vec![opacity],
            vec![[0.5, 0.5, 0.5]],
        )
    };
    let a = gaussians_to_field(&hd, &mk(0.0), n, None); // p = 0.5
    let b = gaussians_to_field(&hd, &mk(3.0f32.ln()), n, None); // p = 0.75
    let (ca, cb) = (
        a[(16 + 16 * n + 16 * n * n) as usize],
        b[(16 + 16 * n + 16 * n * n) as usize],
    );
    assert_eq!(ca, 0.5);
    assert_eq!(cb, 0.75, "logit ln3 ⇒ 概率 0.75（Q16 舍入后恰 49152）");
    assert_eq!(cb / ca, 1.5, "同几何场值比应恰为概率比");
}

#[test]
fn gauss_to_voxel_deterministic_and_bounded() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let mut rng = Lcg::new(0x76c1);
    let mut positions = Vec::new();
    let mut rotations = Vec::new();
    let mut log_scales = Vec::new();
    let mut opacities = Vec::new();
    for _ in 0..8 {
        positions.push([
            rng.next01() * 0.5 - 0.25,
            rng.next01() * 0.5 - 0.25,
            rng.next01() * 0.5 - 0.25,
        ]);
        rotations.push([0.0, 0.0, 0.0, 1.0]);
        let s = 0.08 + rng.next01() * 0.04;
        log_scales.push([s.ln(), s.ln(), s.ln()]);
        opacities.push(rng.next01() * 4.0 - 2.0);
    }
    let cloud = GaussianCloud::new(
        positions,
        log_scales,
        rotations,
        opacities,
        vec![[0.5, 0.5, 0.5]; 8],
    );
    let a = gaussians_to_field(&hd, &cloud, n, None);
    let b = gaussians_to_field(&hd, &cloud, n, None);
    let diff = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count();
    assert_eq!(diff, 0, "场非逐位确定（{diff} 项）");
    assert!(a.iter().all(|v| v.is_finite() && *v >= 0.0), "场出现非法值");
    assert_eq!(a[0], 0.0, "角点应在全支撑窗外（恰零）");
}

/// 大规模恒等直通：1.5M 点（单平面 4.5M 元素）——派发 n/64 = 70,313 > 65,535，
/// 此前 1D 派发上限拒发；grid-stride 后逐位保真判据不变（各腿规模探针第一片）。
#[test]
fn identity_roundtrip_large_cloud_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 1_500_000usize;
    let mut rng = Lcg::new(0x1a2b3c);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut log_scales: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut rotations: Vec<[f32; 4]> = Vec::with_capacity(n);
    let mut opacities: Vec<f32> = Vec::with_capacity(n);
    let mut colors: Vec<[f32; 3]> = Vec::with_capacity(n);
    for _ in 0..n {
        positions.push([
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
        ]);
        let s = |r: &mut Lcg| r.next01() * 4.0 - 4.0;
        log_scales.push([s(&mut rng), s(&mut rng), s(&mut rng)]);
        rotations.push([s(&mut rng), s(&mut rng), s(&mut rng), s(&mut rng)]);
        opacities.push(rng.next01() * 8.0 - 4.0);
        colors.push([rng.next01(), rng.next01(), rng.next01()]);
    }
    // 位边界语料（尾点抽查 +0/−0/次正规）
    positions[n - 1] = [0.0, -0.0, f32::MIN_POSITIVE / 2.0];
    opacities[n - 1] = -0.0;
    let cloud = GaussianCloud::new(positions, log_scales, rotations, opacities, colors);
    assert!(n * 3 / 64 > 65_535, "本规模须超过 1D 派发上限");

    let t0 = std::time::Instant::now();
    let rt = identity_roundtrip(&hd, &cloud, None);
    let wall = t0.elapsed();
    assert_eq!(rt.count, cloud.count);
    assert_f32_slice_bits_eq(
        &flat3(&rt.positions),
        &flat3(&cloud.positions),
        "position 平面（1.5M 点）",
    );
    assert_f32_slice_bits_eq(
        &flat3(&rt.log_scales),
        &flat3(&cloud.log_scales),
        "log_scale 平面",
    );
    assert_f32_slice_bits_eq(
        &flat4(&rt.rotations),
        &flat4(&cloud.rotations),
        "rotation 平面",
    );
    assert_f32_slice_bits_eq(&rt.opacities, &cloud.opacities, "opacity 平面");
    assert_f32_slice_bits_eq(&flat3(&rt.colors), &flat3(&cloud.colors), "color 平面");
    println!("恒等 1.5M 点：5 平面逐位一致，端到端 {wall:?}（单平面 4.5M 元素）");
}
