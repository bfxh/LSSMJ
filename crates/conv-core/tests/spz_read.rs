//! SPZ 读入判据（T-GC-04 第五片）：自建夹具（raw/gzip 双容器）逐位 + 上游真文件对公式核对
//! + 金样哈希（读入契约的机器锚）+ 错误路径。
//!
//! 自建夹具的量化字段与期望值**按格式规范手写**（不从被测代码反推）；上游真文件
//! `fixtures/model.spz` 由 wgpu-3dgs-core @f64a247a 的 `given::gaussian_with_seed(42/123)`
//! 生成（MIT OR Apache-2.0，139 B），其解码值与生成公式按量化容差核对。

use conv_core::{
    gaussian::{cloud_hash, identity_roundtrip},
    jfa::headless_device,
    spz::{SPZ_MAGIC, SpzError, load_spz},
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

/// 自建夹具单点（量化域字段）。
struct QP {
    pos_i24: [i32; 3],
    alpha: u8,
    color: [u8; 3],
    scale: [u8; 3],
    rot: [u8; 3],
    sh: Vec<u8>,
}

fn build_spz(version: u32, fractional_bits: u8, sh_degree: u8, pts: &[QP], gzip: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&SPZ_MAGIC.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(pts.len() as u32).to_le_bytes());
    out.push(sh_degree);
    out.push(fractional_bits);
    out.push(0);
    out.push(0);
    for p in pts {
        for c in p.pos_i24 {
            out.extend_from_slice(&c.to_le_bytes()[..3]);
        }
    }
    for p in pts {
        out.push(p.alpha);
    }
    for p in pts {
        out.extend_from_slice(&p.color);
    }
    for p in pts {
        out.extend_from_slice(&p.scale);
    }
    for p in pts {
        out.extend_from_slice(&p.rot);
    }
    for p in pts {
        out.extend_from_slice(&p.sh);
    }
    if gzip { gzip_wrap(&out) } else { out }
}

fn gzip_wrap(raw: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::GzEncoder};
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut e, raw).unwrap();
    e.finish().unwrap()
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

fn flat3(p: &[[f32; 3]]) -> Vec<f32> {
    p.iter().flat_map(|v| v.iter().copied()).collect()
}

fn flat4(p: &[[f32; 4]]) -> Vec<f32> {
    p.iter().flat_map(|v| v.iter().copied()).collect()
}

/// 三枚边界语料点（含 −0 位置、裁切边界、w 导出退化）。
fn boundary_points(sh_stride: usize) -> Vec<QP> {
    let mk_sh =
        |base: i32| -> Vec<u8> { (0..sh_stride).map(|k| (base + k as i32) as u8).collect() };
    vec![
        QP {
            pos_i24: [1234, -5678, 91011],
            alpha: 0,             // 下界：a = 0.5/256
            color: [0, 128, 255], // 上下界 + 中点
            scale: [0, 160, 255], // log = −10 / 0 / 5.9375
            rot: [0, 128, 255],   // x=−1、y≈0.0039、z=1 ⇒ |xyz|²>1 ⇒ w 恰 0（退化）
            sh: mk_sh(0),
        },
        QP {
            pos_i24: [0, 0, 0],
            alpha: 255, // 上界：a = 255.5/256
            color: [255, 0, 128],
            scale: [255, 128, 0],
            rot: [127, 128, 129],
            sh: mk_sh(64),
        },
        QP {
            pos_i24: [-8388608, 8388607, -1], // i24 下界/上界/−1（符号扩展边界）
            alpha: 128,
            color: [7, 200, 63],
            scale: [64, 192, 8],
            rot: [64, 192, 32],
            sh: mk_sh(200),
        },
    ]
}

/// 期望云（**独立手写**：按格式规范逐字段重算，不调用被测函数）。
fn expected_cloud(
    pts: &[QP],
    fractional_bits: u8,
    sh_degree: u8,
) -> conv_core::gaussian::GaussianCloud {
    let pos_scale = 1.0f32 / (1u64 << fractional_bits) as f32;
    let positions: Vec<[f32; 3]> = pts
        .iter()
        .map(|p| {
            [
                p.pos_i24[0] as f32 * pos_scale,
                p.pos_i24[1] as f32 * pos_scale,
                p.pos_i24[2] as f32 * pos_scale,
            ]
        })
        .collect();
    let log_scales: Vec<[f32; 3]> = pts
        .iter()
        .map(|p| {
            [
                p.scale[0] as f32 / 16.0 - 10.0,
                p.scale[1] as f32 / 16.0 - 10.0,
                p.scale[2] as f32 / 16.0 - 10.0,
            ]
        })
        .collect();
    let rotations: Vec<[f32; 4]> = pts
        .iter()
        .map(|p| {
            let x = p.rot[0] as f32 / 127.5 - 1.0;
            let y = p.rot[1] as f32 / 127.5 - 1.0;
            let z = p.rot[2] as f32 / 127.5 - 1.0;
            [x, y, z, (1.0 - (x * x + y * y + z * z)).max(0.0).sqrt()]
        })
        .collect();
    let opacities: Vec<f32> = pts
        .iter()
        .map(|p| {
            let a = (p.alpha as f32 + 0.5) / 256.0;
            (a / (1.0 - a)).ln()
        })
        .collect();
    let colors: Vec<[f32; 3]> = pts
        .iter()
        .map(|p| {
            let d = |b: u8| {
                let f_dc = (b as f32 / 255.0 - 0.5) / 0.15;
                0.5 + conv_core::ply::SH_C0 * f_dc
            };
            [d(p.color[0]), d(p.color[1]), d(p.color[2])]
        })
        .collect();
    let cloud = conv_core::gaussian::GaussianCloud::new(
        positions, log_scales, rotations, opacities, colors,
    );
    if sh_degree > 0 {
        let stride = pts.first().map(|p| p.sh.len()).unwrap_or(0) as u32;
        let mut sh = Vec::new();
        for p in pts {
            for &b in &p.sh {
                sh.push((b as f32 - 128.0) / 128.0);
            }
        }
        cloud.with_sh_rest(sh, stride)
    } else {
        cloud
    }
}

#[test]
fn spz_raw_and_gzip_bitwise_and_golden() {
    let fb = 12u8;
    let degree = 3u8;
    let stride = 3 * ((degree as usize + 1).pow(2) - 1);
    let pts = boundary_points(stride);
    let expected = expected_cloud(&pts, fb, degree);

    for gzip in [false, true] {
        let bytes = build_spz(2, fb, degree, &pts, gzip);
        let (cloud, info) = load_spz(&bytes).expect("自建夹具应可读入");
        assert_eq!(info.version, 2);
        assert_eq!(info.point_count, 3);
        assert_eq!(info.sh_degree, degree);
        assert_eq!(info.fractional_bits, fb);
        assert!(!info.antialiased);
        assert_eq!(info.gzip, gzip);

        // 五平面 + SH 平面逐位
        assert_f32_slice_bits_eq(
            &flat3(&cloud.positions),
            &flat3(&expected.positions),
            "position",
        );
        assert_f32_slice_bits_eq(
            &flat3(&cloud.log_scales),
            &flat3(&expected.log_scales),
            "log_scale",
        );
        assert_f32_slice_bits_eq(
            &flat4(&cloud.rotations),
            &flat4(&expected.rotations),
            "rotation",
        );
        assert_f32_slice_bits_eq(&cloud.opacities, &expected.opacities, "opacity");
        assert_f32_slice_bits_eq(&flat3(&cloud.colors), &flat3(&expected.colors), "color");
        assert_f32_slice_bits_eq(&cloud.sh_rest, &expected.sh_rest, "SH 平面");

        // 手写常量抽查（独立于两侧公式）：1234/4096 恰 = 0.30126953…；i24 上界/2^12
        assert_eq!(cloud.positions[0][0], 1234.0 / 4096.0);
        assert_eq!(cloud.positions[2][0], -8388608.0 / 4096.0);
        assert_eq!(cloud.positions[2][1], 8388607.0 / 4096.0);
        // α 下界：logit(0.5/256) = ln(0.5/255.5)
        assert_eq!(cloud.opacities[0], (0.5f32 / 255.5).ln());
        // 旋转退化：x=−1, z=1 ⇒ |xyz|²=2 ⇒ w 恰 0
        assert_eq!(cloud.rotations[0][3], 0.0);
    }

    // raw 与 gzip 双容器互拍（结果逐位一致）——上面已各自对期望云逐位 ⇒ 自然一致

    // 金样哈希（读入契约的机器锚）
    let bytes = build_spz(2, fb, degree, &pts, true);
    let (cloud, _) = load_spz(&bytes).unwrap();
    let h = cloud_hash(&cloud);
    println!("spz golden hash: {h:#018x}");
    assert_eq!(
        h, 0x84fc312a1b531423,
        "金样哈希漂移 —— 读入映射规则变更？（量化/域/公式）"
    );
}

#[test]
fn spz_upstream_model_fixture_against_independent_decode() {
    // 上游夹具：wgpu-3dgs-core @f64a247a examples/model.spz（139 B，MIT OR Apache-2.0，
    // 9 点 sh_degree=3 的模型示例）。核对口径 = **独立手工解码**（python/gzip 逐字段，
    // 与实现不同语言、不同代码路径）+ 结构不变量，杜绝"用被测代码反推期望"。
    // 独立解码记录：version=2 count=9 sh_deg=3 fb=12 flags=0；pos = 立方格点
    // (0/4/8)³；alpha 全 255；color[0]=(195,60,60)；scale[0]=(149,160,155)；
    // rot[0]=(149,138,149)。
    let bytes = include_bytes!("fixtures/model.spz");
    let (cloud, info) = load_spz(bytes).expect("上游真文件应可读入");
    assert_eq!(info.version, 2);
    assert_eq!(info.point_count, 9);
    assert_eq!(info.sh_degree, 3);
    assert_eq!(info.fractional_bits, 12);
    assert!(!info.antialiased);
    assert!(info.gzip, "上游夹具为 gzip 容器");

    // 格点位置（整数 × 4，fb=12 ⇒ 恰表示）
    assert_eq!(cloud.positions[0], [0.0, 0.0, 0.0]);
    assert_eq!(cloud.positions[1], [0.0, 8.0, 4.0]);
    assert_eq!(cloud.positions[3], [4.0, 4.0, 8.0]);
    assert_eq!(cloud.positions[6], [8.0, 8.0, 8.0]);
    // log_scale 手算：149/16−10 = −0.6875、160/16−10 = 0、155/16−10 = −0.3125（二进精确）
    assert_eq!(cloud.log_scales[0], [-0.6875, 0.0, -0.3125]);
    // 旋转按公式（149/127.5−1 等）密核对；w 导出 ⇒ |q|² 恒 1
    let xyz = [
        149.0f32 / 127.5 - 1.0,
        138.0 / 127.5 - 1.0,
        149.0 / 127.5 - 1.0,
    ];
    for (k, want) in xyz.iter().enumerate() {
        assert!(
            (cloud.rotations[0][k] - want).abs() <= 1e-6,
            "rot[{k}] 偏离"
        );
    }
    let norm2: f32 = cloud.rotations[0].iter().map(|v| v * v).sum();
    assert!(
        (norm2 - 1.0).abs() <= 1e-6,
        "w 导出应得单位四元数：|q|²={norm2}"
    );
    // 线性 α：255 ⇒ a = 255.5/256 ⇒ logit
    assert_eq!(
        cloud.opacities[0],
        (255.5f32 / 256.0f32 / (1.0 - 255.5 / 256.0)).ln()
    );

    // 不变量：全点有限、尺度在 SPZ 值域内、全部 |q|² ≈ 1
    for i in 0..9 {
        assert!(cloud.positions[i].iter().all(|v| v.is_finite()));
        assert!(cloud.log_scales[i].iter().all(|v| (-10.0..6.0).contains(v)));
        let n2: f32 = cloud.rotations[i].iter().map(|v| v * v).sum();
        assert!((n2 - 1.0).abs() <= 1e-5, "pt{i} |q|²={n2}");
    }
    assert_eq!(cloud.sh_rest.len(), 9 * 45);
    assert_eq!(cloud.sh_rest_stride, 45);

    let h = cloud_hash(&cloud);
    println!(
        "upstream model.spz decode: {} 点、sh=3；golden {h:#018x}",
        info.point_count
    );
    assert_eq!(
        h, 0xa01ac08a5359f27c,
        "上游夹具解码金样漂移 —— 读入映射规则变更？"
    );
}

#[test]
fn spz_gpu_identity_roundtrip() {
    // 读入产物直接喂恒等直通（跨片一致性：SPZ → planar 云 → GPU 往返逐位）
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let stride = 45;
    let pts = boundary_points(stride);
    let bytes = build_spz(2, 12, 3, &pts, false);
    let (cloud, _) = load_spz(&bytes).unwrap();
    let rt = identity_roundtrip(&hd, &cloud, None);
    assert_eq!(rt.count, cloud.count);
    assert_f32_slice_bits_eq(
        &flat3(&rt.positions),
        &flat3(&cloud.positions),
        "往返 position",
    );
    assert_f32_slice_bits_eq(&rt.sh_rest, &cloud.sh_rest, "往返 SH");
}

#[test]
fn spz_error_paths() {
    let stride = 3;
    let pts = boundary_points(stride);
    let good = build_spz(2, 12, 1, &pts, false);

    // 坏魔数
    let mut b = good.clone();
    b[0] ^= 0xff;
    assert!(matches!(load_spz(&b), Err(SpzError::BadMagic)));

    // 版本 1 / 3（f16 位置 / smallest-three 四元数——不同编码，显式拒收）
    let mut b = good.clone();
    b[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(load_spz(&b), Err(SpzError::UnsupportedVersion(1))));
    let mut b = good.clone();
    b[4..8].copy_from_slice(&3u32.to_le_bytes());
    assert!(matches!(load_spz(&b), Err(SpzError::UnsupportedVersion(3))));

    // sh_degree 越界
    let mut b = good.clone();
    b[12] = 4;
    assert!(matches!(
        load_spz(&b),
        Err(SpzError::UnsupportedShDegree(4))
    ));

    // fractional_bits 越界
    let mut b = good.clone();
    b[13] = 31;
    assert!(matches!(load_spz(&b), Err(SpzError::BadFractionalBits(31))));

    // 截断：仅头 / 数据中途断
    assert!(matches!(load_spz(&good[..10]), Err(SpzError::Truncated)));
    let mut b = good.clone();
    b.truncate(good.len() - 2);
    assert!(matches!(load_spz(&b), Err(SpzError::Truncated)));

    // 坏 gzip 流
    let bad_gz = vec![0x1f, 0x8b, 0x00, 0x00, 0xde, 0xad, 0xbe, 0xef];
    assert!(matches!(load_spz(&bad_gz), Err(SpzError::BadGzip)));
}
