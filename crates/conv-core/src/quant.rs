//! planar 量化变体（T-GC-04 第三片；锚 `W15A-041/042` 的 planar_quantized 家族）：
//! 五平面 → 紧凑整数表示（有损；误差预算钉死）。
//!
//! 方案（钉死；改方案 = 改契约，由字节金样强制披露）：
//! - position：每轴 **u16 相对云 AABB**（`lo + q/65535 × range`；量子 = range/65535）
//! - log_scale：**i16 Q8**（步 1/256，覆盖 ±128——log 域本身窄）
//! - rotation：**i8 逐分量**（`q = round(c × 127)`，c ∈ [−1,1]；解量化不重归一）
//! - opacity：**u8**，logit 域 [−16, 16] 线性（步 32/255 ≈ 0.125），**越界钳制**
//! - color：**u8** 通道，[0,1] 线性（步 1/255），**越界钳制**
//!
//! 理论误差上界（= 半量子；判据按此断言，而非经验值）：position ≤ range/131070、
//! log_scale ≤ 1/512、rotation ≤ 1/254、opacity ≤ 16/255、color ≤ 1/510。
//! 幂等：`quant(dequant(quant(x)))` 字节级 == `quant(x)`（解量化落量子格心，重量化回同格）。

use crate::gaussian::GaussianCloud;

/// 量化云（平面序与 `GaussianCloud` 一致）。
pub struct QuantizedCloud {
    pub count: u32,
    /// position：u16 相对 AABB
    pub pos_q: Vec<[u16; 3]>,
    /// AABB 下界 / 每轴跨度（解量化所需；range == 0 ⇒ 整轴退化为 lo）
    pub pos_lo: [f32; 3],
    pub pos_range: [f32; 3],
    /// log_scale：i16 Q8
    pub log_scale_q: Vec<[i16; 3]>,
    /// rotation：i8 逐分量（按 ×127 量化）
    pub rot_q: Vec<[i8; 4]>,
    /// opacity：u8，logit ∈ [−16, 16] 线性
    pub opacity_q: Vec<u8>,
    /// color：u8 通道，[0,1] 线性
    pub color_q: Vec<[u8; 3]>,
}

const OP_LO: f32 = -16.0;
const OP_HI: f32 = 16.0;

fn quant_u16(v: f32, lo: f32, range: f32) -> u16 {
    if range <= 0.0 {
        return 0;
    }
    ((v - lo) / range * 65535.0).round().clamp(0.0, 65535.0) as u16
}

fn quant_i16_q8(v: f32) -> i16 {
    (v * 256.0).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn quant_i8(v: f32) -> i8 {
    (v * 127.0).round().clamp(-128.0, 127.0) as i8
}

fn quant_u8_lin(v: f32, lo: f32, hi: f32) -> u8 {
    (((v - lo) / (hi - lo)) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// 量化（见模块头注的钉死方案）。
pub fn quantize(c: &GaussianCloud) -> QuantizedCloud {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in &c.positions {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    if c.positions.is_empty() {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let range = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];

    QuantizedCloud {
        count: c.count,
        pos_q: c
            .positions
            .iter()
            .map(|p| {
                [
                    quant_u16(p[0], lo[0], range[0]),
                    quant_u16(p[1], lo[1], range[1]),
                    quant_u16(p[2], lo[2], range[2]),
                ]
            })
            .collect(),
        pos_lo: lo,
        pos_range: range,
        log_scale_q: c
            .log_scales
            .iter()
            .map(|s| [quant_i16_q8(s[0]), quant_i16_q8(s[1]), quant_i16_q8(s[2])])
            .collect(),
        rot_q: c
            .rotations
            .iter()
            .map(|q| {
                [
                    quant_i8(q[0]),
                    quant_i8(q[1]),
                    quant_i8(q[2]),
                    quant_i8(q[3]),
                ]
            })
            .collect(),
        opacity_q: c
            .opacities
            .iter()
            .map(|&o| quant_u8_lin(o, OP_LO, OP_HI))
            .collect(),
        color_q: c
            .colors
            .iter()
            .map(|col| {
                [
                    quant_u8_lin(col[0], 0.0, 1.0),
                    quant_u8_lin(col[1], 0.0, 1.0),
                    quant_u8_lin(col[2], 0.0, 1.0),
                ]
            })
            .collect(),
    }
}

/// 解量化（planar 五平面；position 用云级 AABB 还原）。
pub fn dequantize(q: &QuantizedCloud) -> GaussianCloud {
    let positions = q
        .pos_q
        .iter()
        .map(|p| {
            [
                q.pos_lo[0] + p[0] as f32 / 65535.0 * q.pos_range[0],
                q.pos_lo[1] + p[1] as f32 / 65535.0 * q.pos_range[1],
                q.pos_lo[2] + p[2] as f32 / 65535.0 * q.pos_range[2],
            ]
        })
        .collect();
    let log_scales = q
        .log_scale_q
        .iter()
        .map(|s| {
            [
                s[0] as f32 / 256.0,
                s[1] as f32 / 256.0,
                s[2] as f32 / 256.0,
            ]
        })
        .collect();
    let rotations = q
        .rot_q
        .iter()
        .map(|r| {
            [
                r[0] as f32 / 127.0,
                r[1] as f32 / 127.0,
                r[2] as f32 / 127.0,
                r[3] as f32 / 127.0,
            ]
        })
        .collect();
    let opacities = q
        .opacity_q
        .iter()
        .map(|&o| OP_LO + o as f32 / 255.0 * (OP_HI - OP_LO))
        .collect();
    let colors = q
        .color_q
        .iter()
        .map(|c| {
            [
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
            ]
        })
        .collect();
    GaussianCloud::new(positions, log_scales, rotations, opacities, colors)
}

/// 字节金样哈希（平面序：pos(AABB+u16) → log_scale(i16) → rot(i8) → opacity(u8) → color(u8)；
/// 逐位 FNV-1a）——量化格式契约的机器锚。
pub fn quant_hash(q: &QuantizedCloud) -> u64 {
    fn eat(h: &mut u64, b: u8) {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in q.count.to_le_bytes() {
        eat(&mut h, b);
    }
    for v in q.pos_lo.iter().chain(q.pos_range.iter()) {
        for b in v.to_bits().to_le_bytes() {
            eat(&mut h, b);
        }
    }
    for p in &q.pos_q {
        for v in p {
            for b in v.to_le_bytes() {
                eat(&mut h, b);
            }
        }
    }
    for s in &q.log_scale_q {
        for v in s {
            for b in v.to_le_bytes() {
                eat(&mut h, b);
            }
        }
    }
    for r in &q.rot_q {
        for v in r {
            eat(&mut h, *v as u8);
        }
    }
    for o in &q.opacity_q {
        eat(&mut h, *o);
    }
    for c in &q.color_q {
        for v in c {
            eat(&mut h, *v);
        }
    }
    h
}
