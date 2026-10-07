//! PLY 读入判据（T-GC-04 第二片）：金样哈希（ascii/binary 双格式）+ 逐位映射 + 错误路径显式。
//! 映射规则见 `src/ply.rs` 头注（契约变更须在此披露——金样哈希会判红）。

use conv_core::gaussian::{GaussianCloud, cloud_hash};
use conv_core::ply::{PlyError, PlyFormat, SH_C0, load_ply};

const ASCII: &[u8] = include_bytes!("fixtures/gauss-mini-ascii.ply");
const BINARY: &[u8] = include_bytes!("fixtures/gauss-mini-binary.ply");

/// 与夹具逐字对应的期望云（rotation 走 (x,y,z,w) 平面、color 走 SH-DC 映射规则）。
fn expected_cloud() -> GaussianCloud {
    let positions = vec![
        [1.5, -2.25, 0.125],
        [-0.0, 4.0, -1.0],
        [2.0, 0.0, -3.0],
        [-1.0, -2.0, 3.0],
    ];
    let log_scales = vec![
        [-1.25, 0.5, 2.0],
        [-3.5, -4.25, -5.125],
        [0.0, 0.0, 0.0],
        [1.0, -1.0, 0.5],
    ];
    // PLY (w,x,y,z) → 平面 (x,y,z,w)
    let rotations = vec![
        [0.25, -0.5, 0.75, 0.5],
        [0.0, -0.0, 0.25, 1.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
    ];
    let opacities = vec![0.9, -2.5, 3.5, 0.0];
    let dc = [
        [0.25f32, -0.5, 0.75],
        [-1.0, 1.0, 0.0],
        [-0.125, 0.375, -0.625],
        [0.0, 0.0, 0.0],
    ];
    let colors = dc.iter().map(|c| c.map(|v| 0.5 + SH_C0 * v)).collect();
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
fn ply_ascii_and_binary_match_expected_bitwise() {
    let (ca, ia) = load_ply(ASCII).expect("ascii 读入失败");
    let (cb, ib) = load_ply(BINARY).expect("binary 读入失败");
    assert_eq!(ia.format, PlyFormat::Ascii);
    assert_eq!(ib.format, PlyFormat::BinaryLittleEndian);
    assert_eq!(ia.vertex_count, 4);
    assert_eq!(ib.vertex_count, 4);
    assert_eq!(ia.ignored_f_rest, 3, "f_rest 忽略计数（显式披露）");
    assert_eq!(ib.ignored_f_rest, 3);

    // 双格式互拍（f_rest 不同值、被忽略，不影响）
    assert_eq!(
        cloud_hash(&ca),
        cloud_hash(&cb),
        "ascii 与 binary 读入结果不一致"
    );

    // 对逐字期望云逐位（五平面）
    let exp = expected_cloud();
    assert_f32_slice_bits_eq(
        &flat3(&ca.positions),
        &flat3(&exp.positions),
        "position 平面",
    );
    assert_f32_slice_bits_eq(
        &flat3(&ca.log_scales),
        &flat3(&exp.log_scales),
        "log_scale 平面",
    );
    assert_f32_slice_bits_eq(
        &flat4(&ca.rotations),
        &flat4(&exp.rotations),
        "rotation 平面",
    );
    assert_f32_slice_bits_eq(&ca.opacities, &exp.opacities, "opacity 平面");
    assert_f32_slice_bits_eq(&flat3(&ca.colors), &flat3(&exp.colors), "color 平面");

    // 金样（读入契约的机器锚）
    let h = cloud_hash(&ca);
    println!("ply golden hash: {h:#018x}");
    assert_eq!(
        h, 0xcb1f8830bc27ba46,
        "金样哈希漂移 —— 读入映射规则变更？（重排/域/公式）"
    );
}

#[test]
fn ply_error_paths_are_explicit() {
    // 魔数不对
    assert!(matches!(load_ply(b"not a ply\n"), Err(PlyError::BadMagic)));
    // 大端不接受
    let be = b"ply\nformat binary_big_endian 1.0\nelement vertex 0\nend_header\n";
    assert!(matches!(load_ply(be), Err(PlyError::UnsupportedFormat(_))));
    // 未知属性（不静默吞）
    let unknown = b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float nx\nend_header\n0 0\n";
    assert!(matches!(
        load_ply(unknown),
        Err(PlyError::UnsupportedProperty { .. })
    ));
    // 非 vertex 元素（count>0）
    let face =
        b"ply\nformat ascii 1.0\nelement vertex 0\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n";
    assert!(matches!(
        load_ply(face),
        Err(PlyError::UnsupportedElement(_))
    ));
    // 缺失必需属性
    let missing = b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nend_header\n0\n";
    assert!(matches!(
        load_ply(missing),
        Err(PlyError::MissingProperty(_))
    ));
    // 截断（binary 少 10 字节）
    let trunc = &BINARY[..BINARY.len() - 10];
    assert!(matches!(load_ply(trunc), Err(PlyError::Truncated)));
    // 内容阶段坏浮点（ascii）
    let bad_float = b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nproperty float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\nproperty float opacity\nproperty float scale_0\nproperty float scale_1\nproperty float scale_2\nproperty float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\nend_header\n0 0 0 0 0 0 0 0 0 0 0 0 0 oops\n";
    assert!(matches!(load_ply(bad_float), Err(PlyError::BadFloat(_))));
}
