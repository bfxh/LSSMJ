//! 高斯腿判据（T-GC-04 第一片）：恒等直通逐位（无转换=无漂移）+ 金样哈希（格式契约）
//! + 先验红金丝雀（判据必须能判红）。

use conv_core::{
    Lcg,
    gaussian::{GaussianCloud, cloud_hash, identity_roundtrip},
    jfa::headless_device,
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
    GaussianCloud::new(positions, log_scales, rotations, opacities, colors)
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
    assert_eq!(
        h, 0x9b5164015597b571,
        "金样哈希漂移 —— 格式契约变更？（平面集/次序/域）"
    );
}
