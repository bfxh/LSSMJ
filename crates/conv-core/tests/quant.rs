//! planar 量化判据（T-GC-04 第三片）：逐平面误差 ≤ **理论半量子**（非经验值）+ 幂等字节级 +
//! 确定性/金样 + 越界钳制语义。纯 CPU。

use conv_core::{
    gaussian::GaussianCloud,
    ply::load_ply,
    quant::{dequantize, quant_hash, quantize},
};

const ASCII: &[u8] = include_bytes!("fixtures/gauss-mini-ascii.ply");

#[test]
fn quant_error_within_half_quantum() {
    let (cloud, _) = load_ply(ASCII).expect("fixture 读入失败");
    let q = quantize(&cloud);
    let d = dequantize(&q);
    let (mut e_pos, mut e_ls, mut e_rot, mut e_op, mut e_col) = (0f32, 0f32, 0f32, 0f32, 0f32);
    for i in 0..cloud.count as usize {
        for a in 0..3 {
            e_pos = e_pos.max((cloud.positions[i][a] - d.positions[i][a]).abs());
            e_ls = e_ls.max((cloud.log_scales[i][a] - d.log_scales[i][a]).abs());
            e_col = e_col.max((cloud.colors[i][a] - d.colors[i][a]).abs());
        }
        for a in 0..4 {
            e_rot = e_rot.max((cloud.rotations[i][a] - d.rotations[i][a]).abs());
        }
        e_op = e_op.max((cloud.opacities[i] - d.opacities[i]).abs());
    }
    println!(
        "量化实测误差：pos {e_pos:.2e} / log_scale {e_ls:.2e} / rot {e_rot:.2e} / opacity {e_op:.2e} / color {e_col:.2e}"
    );
    let eps = 1e-6f32; // 解量化浮点尾差余量
    let bp = q.pos_range.iter().cloned().fold(0f32, f32::max) / 131070.0;
    assert!(e_pos <= bp + eps, "pos 超半量子：{e_pos} > {bp}");
    assert!(e_ls <= 1.0 / 512.0 + eps, "log_scale 超半量子：{e_ls}");
    assert!(e_rot <= 1.0 / 254.0 + eps, "rotation 超半量子：{e_rot}");
    assert!(e_op <= 16.0 / 255.0 + eps, "opacity 超半量子：{e_op}");
    assert!(e_col <= 1.0 / 510.0 + eps, "color 超半量子：{e_col}");
}

#[test]
fn quant_idempotent_bytes() {
    let (cloud, _) = load_ply(ASCII).expect("fixture 读入失败");
    let q1 = quantize(&cloud);
    let q2 = quantize(&dequantize(&q1));
    assert_eq!(quant_hash(&q1), quant_hash(&q2), "幂等哈希不一致");
    assert_eq!(q1.pos_q, q2.pos_q);
    assert_eq!(q1.log_scale_q, q2.log_scale_q);
    assert_eq!(q1.rot_q, q2.rot_q);
    assert_eq!(q1.opacity_q, q2.opacity_q);
    assert_eq!(q1.color_q, q2.color_q);
    assert_eq!(q1.pos_lo, q2.pos_lo, "重算 AABB 下界漂移");
    assert_eq!(q1.pos_range, q2.pos_range, "重算 AABB 跨度漂移");
}

#[test]
fn quant_deterministic_and_golden() {
    let (cloud, _) = load_ply(ASCII).expect("fixture 读入失败");
    let a = quant_hash(&quantize(&cloud));
    let b = quant_hash(&quantize(&cloud));
    assert_eq!(a, b, "量化非确定");
    println!("quant golden hash: {a:#018x}");
    assert_eq!(
        a, 0xbf47e4a79fedbf54,
        "字节金样漂移 —— 量化方案契约变更？（格/域/钳制）"
    );
}

#[test]
fn quant_clamps_out_of_range() {
    let cloud = GaussianCloud::new(
        vec![[0.25, -0.5, 0.75]],
        vec![[-200.0, 0.0, 1.5]],
        vec![[0.0, 0.0, 0.0, 1.0]],
        vec![100.0],
        vec![[0.0, 1.0, 2.0]],
    );
    let d = dequantize(&quantize(&cloud));
    // 单点 ⇒ AABB 退化 ⇒ 位置精确还原
    assert_eq!(d.positions[0], [0.25, -0.5, 0.75]);
    // log_scale −200 越界 ⇒ 钳到 i16 下界 −128
    assert_eq!(d.log_scales[0], [-128.0, 0.0, 1.5]);
    assert_eq!(d.rotations[0], [0.0, 0.0, 0.0, 1.0]);
    // opacity 100 越上界 ⇒ +16
    assert_eq!(d.opacities[0], 16.0);
    // color 0/1 恰点还原；2 越上界 ⇒ 1.0
    assert_eq!(d.colors[0], [0.0, 1.0, 1.0]);
}
