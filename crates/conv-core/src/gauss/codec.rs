//! 高斯压缩码：每个高斯 **16 字节**（本仓 `Gaussian` 是 14 个 f32 = 56 B，3.5×；
//! 满 SH-3 资产是 59 个 f32 = 236 B，若再加 48 B 的 SH 平面才谈得上 14.75×，本码**不含** SH 高阶）。
//!
//! 3DGS 的内存大头从来不是网络权重而是高斯本身：100 万颗 f32 高斯 ≈ 236 MB，
//! 显存/带宽都死在这儿。这里给的是「推理与查看」用的定点码，字段按感知重要性分配位宽：
//!
//! | 字 | 内容 |
//! |---|---|
//! | w0 | mean.x / mean.y：**块内**相对中心的 i16 定点 |
//! | w1 | mean.z i16 ｜ log2(scale) 三个 5 bit ｜ 1 位保留 |
//! | w2 | 旋转：最大分量的 2 bit 下标 + 其余三个各 10 bit（四元数符号可翻转，故只存三者） |
//! | w3 | SH DC 三通道 u8 定点 ｜ opacity u8 |
//!
//! 块（默认 4096 颗）头 32 B：中心 f32×3、位置步长、log 尺度下限与步长、实际数量。
//! 位置是块内定点，精度随场景尺度自适应：10 m 场景步长 ≈ 0.15 mm。
//!
//! 只做**存储/回读**，不改语义：回读得到仍是 [`Gaussian`]（`scale` 为激活值、`color` 为 SH DC）。

use super::Gaussian;

/// 每颗高斯的字节数（4 个 u32）。
pub const BYTES_PER_GAUSS: usize = 16;
/// 每块的高斯数上限（块头决定位置/尺度的定点范围）。
pub const CHUNK: usize = 4096;
/// 块头字节数。
pub const CHUNK_HEADER: usize = 32;
/// SH DC 的定点范围（`0.5 + C0*dc` 要覆盖 [0,1] ⇒ |dc| ≤ 1.772）。
const DC_RANGE: f32 = 1.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packed {
    pub n: usize,
    pub chunks: usize,
    /// `chunks*CHUNK_HEADER + n*BYTES_PER_GAUSS`
    pub bytes: usize,
}

/// 打这些高斯需要多少字节（与 [`pack`] 实际写出的一致）。
pub fn bytes_for(n: usize) -> usize {
    n.div_ceil(CHUNK) * CHUNK_HEADER + n * BYTES_PER_GAUSS
}

#[derive(Debug, PartialEq, Eq)]
pub enum PackError {
    Truncated { need: usize, got: usize },
    BadChunkCount { header: u32, bytes: usize },
}

struct ChunkHead {
    center: [f32; 3],
    step: f32,
    ls_min: f32,
    ls_step: f32,
    n: u32,
}

pub fn pack(gs: &[Gaussian]) -> (Vec<u8>, Packed) {
    let chunks = gs.len().div_ceil(CHUNK);
    // 块内 bbox 决定定点步长 ⇒ 有限值一定落在 [−32767, 32767] 里；非有限值直接拦下，
    // 不让 NaN 静默变成 0 号点（`Gaussian::is_finite` 是调用方的门）。
    debug_assert!(gs.iter().all(Gaussian::is_finite), "pack 收到非有限高斯");
    let mut out = Vec::with_capacity(bytes_for(gs.len()));
    for chunk in gs.chunks(CHUNK) {
        let head = chunk_head(chunk);
        for v in [
            head.center[0],
            head.center[1],
            head.center[2],
            head.step,
            head.ls_min,
            head.ls_step,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // 保留，凑 32 B
        for g in chunk {
            let q = quantize(g, &head);
            for w in q {
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
    }
    let stats = Packed {
        n: gs.len(),
        chunks,
        bytes: out.len(),
    };
    (out, stats)
}

pub fn unpack(bytes: &[u8]) -> Result<Vec<Gaussian>, PackError> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p < bytes.len() {
        if p + CHUNK_HEADER > bytes.len() {
            return Err(PackError::Truncated {
                need: p + CHUNK_HEADER,
                got: bytes.len(),
            });
        }
        let f = |i: usize| {
            let at = p + i * 4;
            f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
        };
        let head = ChunkHead {
            center: [f(0), f(1), f(2)],
            step: f(3),
            ls_min: f(4),
            ls_step: f(5),
            n: {
                let at = p + 24;
                u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
            },
        };
        p += CHUNK_HEADER;
        let need = p + head.n as usize * BYTES_PER_GAUSS;
        if need > bytes.len() {
            return Err(PackError::BadChunkCount {
                header: head.n,
                bytes: bytes.len() - p,
            });
        }
        for _ in 0..head.n {
            let w: [u32; 4] = [
                u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()),
                u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap()),
                u32::from_le_bytes(bytes[p + 8..p + 12].try_into().unwrap()),
                u32::from_le_bytes(bytes[p + 12..p + 16].try_into().unwrap()),
            ];
            out.push(dequantize(w, &head));
            p += BYTES_PER_GAUSS;
        }
    }
    Ok(out)
}

fn chunk_head(chunk: &[Gaussian]) -> ChunkHead {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    let mut ls_lo = f32::INFINITY;
    let mut ls_hi = f32::NEG_INFINITY;
    for g in chunk {
        for a in 0..3 {
            lo[a] = lo[a].min(g.mean[a]);
            hi[a] = hi[a].max(g.mean[a]);
        }
        for a in 0..3 {
            let v = g.scale[a].max(1e-12).log2();
            ls_lo = ls_lo.min(v);
            ls_hi = ls_hi.max(v);
        }
    }
    let center = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    let half = (0..3)
        .map(|a| (hi[a] - lo[a]) * 0.5)
        .fold(0.0f32, f32::max);
    ChunkHead {
        center,
        // 空块给 1 m，避免 0 步长
        step: if half.is_finite() && half > 0.0 {
            half / 32767.0
        } else {
            1.0 / 32767.0
        },
        ls_min: if ls_lo.is_finite() { ls_lo } else { 0.0 },
        // 31 档覆盖 [ls_lo, ls_hi]，退化时给最小步长
        ls_step: if ls_hi.is_finite() && ls_hi > ls_lo {
            (ls_hi - ls_lo) / 31.0
        } else {
            1e-4
        },
        n: chunk.len() as u32,
    }
}

fn quantize(g: &Gaussian, h: &ChunkHead) -> [u32; 4] {
    let qi = |a: usize| -> i32 {
        (((g.mean[a] - h.center[a]) / h.step).round() as i32).clamp(-32768, 32767)
    };
    let (qx, qy, qz) = (qi(0), qi(1), qi(2));
    let s = g.scale.map(|v| {
        let q = ((v.max(1e-12).log2() - h.ls_min) / h.ls_step).round();
        q.clamp(0.0, 31.0) as u32
    });
    let w0 = (qx as u16 as u32) | ((qy as u16 as u32) << 16);
    let w1 = (qz as u16 as u32) | (s[0] << 16) | (s[1] << 21) | (s[2] << 26);
    let w2 = pack_rot(g.rot);
    let dc = |a: usize| -> u32 {
        (((g.color[a] + DC_RANGE) / (2.0 * DC_RANGE) * 255.0).round()).clamp(0.0, 255.0) as u32
    };
    let op = (g.opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let w3 = dc(0) | (dc(1) << 8) | (dc(2) << 16) | (op << 24);
    [w0, w1, w2, w3]
}

fn dequantize(w: [u32; 4], h: &ChunkHead) -> Gaussian {
    let sx = w[0] as u16 as i16 as f32;
    let sy = (w[0] >> 16) as u16 as i16 as f32;
    let sz = w[1] as u16 as i16 as f32;
    let scale = [
        (h.ls_min + (w[1] >> 16 & 31) as f32 * h.ls_step).exp2(),
        (h.ls_min + (w[1] >> 21 & 31) as f32 * h.ls_step).exp2(),
        (h.ls_min + (w[1] >> 26 & 31) as f32 * h.ls_step).exp2(),
    ];
    let dc = |shift: u32| -> f32 {
        ((w[3] >> shift) & 255) as f32 / 255.0 * 2.0 * DC_RANGE - DC_RANGE
    };
    Gaussian {
        mean: [
            h.center[0] + sx * h.step,
            h.center[1] + sy * h.step,
            h.center[2] + sz * h.step,
        ],
        scale,
        rot: unpack_rot(w[2]),
        opacity: ((w[3] >> 24) & 255) as f32 / 255.0,
        color: [dc(0), dc(8), dc(16)],
        normal: None,
    }
}

const SQ2: f32 = std::f32::consts::SQRT_2;
const INV_SQ2: f32 = 0.707_106_77;
const QMAX: f32 = 1023.0;

/// 最小三分量四元数：把最大分量的**符号规范为正**（q 与 −q 是同一旋转），
/// 其余三个分量落在 [−1/√2, 1/√2] 上，各 10 bit。
fn pack_rot(rot: [f32; 4]) -> u32 {
    let mut q = rot;
    let mut big = 0usize;
    for i in 1..4 {
        if q[i].abs() > q[big].abs() {
            big = i;
        }
    }
    let sign = if q[big] < 0.0 { -1.0 } else { 1.0 };
    for v in q.iter_mut() {
        *v *= sign;
    }
    let mut w = big as u32;
    let mut shift = 2;
    for (i, &v) in q.iter().enumerate() {
        if i == big {
            continue;
        }
        let t = ((v + INV_SQ2) / SQ2 * QMAX).round().clamp(0.0, QMAX);
        w |= (t as u32) << shift;
        shift += 10;
    }
    w
}

fn unpack_rot(w: u32) -> [f32; 4] {
    let big = (w & 3) as usize;
    let mut q = [0f32; 4];
    let mut shift = 2;
    for (i, slot) in q.iter_mut().enumerate() {
        if i == big {
            continue;
        }
        let u = ((w >> shift) & 1023) as f32;
        shift += 10;
        *slot = u / QMAX * SQ2 - INV_SQ2;
    }
    let rest: f32 = q.iter().map(|v| v * v).sum();
    q[big] = (1.0 - rest).max(0.0).sqrt();
    normalize(q)
}

fn normalize(q: [f32; 4]) -> [f32; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n > 0.0 {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    } else {
        [1.0, 0.0, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确定性伪随机（SplitMix64），测试不依赖 rand。
    fn lcg(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u32 << 24) as f32
    }

    fn sample(n: usize, seed: u64) -> Vec<Gaussian> {
        let mut s = seed;
        (0..n)
            .map(|i| {
                let r = |k: &mut u64| lcg(k);
                Gaussian {
                    mean: [
                        r(&mut s) * 10.0 - 5.0,
                        r(&mut s) * 10.0 - 5.0,
                        r(&mut s) * 10.0 - 5.0,
                    ],
                    scale: [
                        0.01 + r(&mut s) * 0.19,
                        0.01 + r(&mut s) * 0.19,
                        0.01 + r(&mut s) * 0.19,
                    ],
                    rot: normalize([r(&mut s) - 0.5, r(&mut s) - 0.5, r(&mut s) - 0.5, r(&mut s) - 0.5]),
                    opacity: (i as f32 / n as f32).clamp(0.0, 1.0),
                    color: [r(&mut s) * 3.0 - 1.5, r(&mut s) * 3.0 - 1.5, r(&mut s) * 3.0 - 1.5],
                    normal: None,
                }
            })
            .collect()
    }

    #[test]
    fn bytes_are_exact() {
        for (n, chunks) in [(0, 0), (1, 1), (CHUNK - 1, 1), (CHUNK, 1), (CHUNK + 1, 2), (10_000, 3)] {
            let gs = sample(n, 7);
            let (buf, st) = pack(&gs);
            assert_eq!(st.bytes, bytes_for(n), "n={n}");
            assert_eq!(buf.len(), st.bytes);
            assert_eq!(st.chunks, chunks, "n={n}");
            assert_eq!(st.bytes, chunks * CHUNK_HEADER + n * BYTES_PER_GAUSS);
        }
        // 16 B 是这颗码存在的理由：宽度写进断言，防止有人悄悄加字段
        assert_eq!(BYTES_PER_GAUSS, 4 * size_of::<u32>());
        assert!(56.0 / BYTES_PER_GAUSS as f32 > 3.0, "至少要比 f32 结构省 3×");
    }

    #[test]
    fn roundtrip_is_within_field_bounds() {
        let gs = sample(3000, 11);
        let (buf, _) = pack(&gs);
        let back = unpack(&buf).unwrap();
        assert_eq!(back.len(), gs.len());
        // 位置：10 m 场景、单块 ⇒ 步长 ≈ 10/65534 ≈ 0.15 mm
        let step = 10.0 / 65534.0;
        let mut rot_err = 0f32;
        for (a, b) in gs.iter().zip(&back) {
            for i in 0..3 {
                assert!((a.mean[i] - b.mean[i]).abs() <= step * 0.5 + 1e-6, "位置误差 {:?}", a.mean);
                // 5 bit log 尺度按块内范围定步长：本样本 ≈4.3 个倍频程 ⇒ 单侧误差 ≤ 5%
                let rel = (a.scale[i] / b.scale[i]).abs().max(b.scale[i] / a.scale[i]).ln();
                assert!(rel <= 0.06, "尺度相对误差 {rel} 超 6%");
                let dc = (a.color[i] - b.color[i]).abs();
                assert!(dc <= DC_RANGE / 255.0 + 1e-6, "SH DC 误差异常 {dc}");
            }
            assert!((a.opacity - b.opacity).abs() <= 1.0 / 510.0 + 1e-6);
            // 四元数：方向上等价（允许全局符号翻转）
            let dot: f32 = (0..4).map(|i| a.rot[i] * b.rot[i]).sum();
            rot_err = rot_err.max(1.0 - dot.abs());
        }
        // 10 bit 最小三分量：最坏角误差远小于 0.5°（1−|dot| ≈ θ²/8）
        assert!(rot_err < 1e-5, "旋转误差 {rot_err} 过大（≈{:.3}°）", (rot_err * 8.0).sqrt().to_degrees());
    }

    #[test]
    fn deterministic_and_order_preserving() {
        let gs = sample(500, 3);
        let (a, _) = pack(&gs);
        let (b, _) = pack(&gs);
        assert_eq!(a, b, "同样的输入必须打出同样的字节");
        let back = unpack(&a).unwrap();
        let first = &back[0].opacity;
        assert_eq!(*first, (gs[0].opacity * 255.0).round() / 255.0);
    }

    /// 单点离群不会把整块挤爆：定点步长按块 bbox 定，离群点自己也在边界内可表示。
    #[test]
    fn single_outlier_does_not_blow_up_the_chunk() {
        let mut gs = sample(10, 5);
        let mut far = gs[0];
        far.mean = [1e3, 0.0, 0.0];
        gs[0] = far;
        let (buf, _) = pack(&gs);
        let back = unpack(&buf).unwrap();
        assert!((back[0].mean[0] - 1e3).abs() < 0.05, "离群点自己也必须还原");
        for (a, b) in gs.iter().zip(&back) {
            assert!((a.mean[0] - b.mean[0]).abs() < 0.05, "整块步长被离群点拖粗后仍在 5 cm 内");
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn nan_is_rejected_by_debug_assert() {
        let mut g = sample(1, 1)[0];
        g.opacity = f32::NAN;
        let r = std::panic::catch_unwind(|| pack(&[g]));
        assert!(r.is_err(), "NaN 高斯必须在 debug 下当场拦下，不能静默变成 0 号点");
    }

    #[test]
    fn truncated_input_errors_instead_of_panicking() {
        let gs = sample(20, 9);
        let (buf, _) = pack(&gs);
        assert!(matches!(
            unpack(&buf[..CHUNK_HEADER - 1]),
            Err(PackError::Truncated { .. })
        ));
        assert!(matches!(
            unpack(&buf[..buf.len() - 3]),
            Err(PackError::BadChunkCount { .. })
        ));
    }

    #[test]
    fn covariance_survives_quantization() {
        // 真正影响渲染的是协方差（尺度×旋转），直接比它，别只比字段。
        let gs = sample(200, 17);
        let (buf, _) = pack(&gs);
        let back = unpack(&buf).unwrap();
        for (a, b) in gs.iter().zip(&back) {
            let (ca, cb) = (a.covariance(), b.covariance());
            let mut num = 0f32;
            let mut den = 0f32;
            for i in 0..3 {
                for j in 0..3 {
                    num += (ca[i][j] - cb[i][j]).powi(2);
                    den += ca[i][j] * ca[i][j];
                }
            }
            let rel = (num / den.max(1e-20)).sqrt();
            assert!(rel < 0.14, "协方差相对误差 {rel} 超 14%（尺度 5% + 旋转 10 bit）");
        }
    }
}

#[cfg(test)]
mod real_asset {
    use super::*;

    /// 真资产上量一遍：字节数、压缩比、协方差相对误差（缺文件就跳过，不假装通过）。
    #[test]
    fn sphere_ply_roundtrip() {
        let p = std::path::Path::new("out/ply_sphere.ply");
        if !p.exists() {
            eprintln!("跳过：{} 不存在", p.display());
            return;
        }
        let data = super::super::ply::read_ply(p).expect("read ply");
        let gs = data.gaussians;
        let (buf, st) = pack(&gs);
        let back = unpack(&buf).unwrap();
        let f32_bytes = gs.len() * 17 * 4; // 该 PLY 每颗 17 个 f32（含法线）
        let mut worst = 0f32;
        let mut sum = 0f64;
        for (a, b) in gs.iter().zip(&back) {
            let (ca, cb) = (a.covariance(), b.covariance());
            let (mut num, mut den) = (0f32, 0f32);
            for i in 0..3 {
                for j in 0..3 {
                    num += (ca[i][j] - cb[i][j]).powi(2);
                    den += ca[i][j] * ca[i][j];
                }
            }
            let rel = (num / den.max(1e-20)).sqrt();
            worst = worst.max(rel);
            sum += rel as f64;
        }
        let mean = sum / gs.len() as f64;
        println!(
            "n={} 打包 {} B vs f32 {} B = {:.2}×；协方差相对误差 均值 {:.4} 最差 {:.4}",
            st.n,
            st.bytes,
            f32_bytes,
            f32_bytes as f64 / st.bytes as f64,
            mean,
            worst
        );
        assert_eq!(st.bytes, bytes_for(gs.len()));
        assert!(worst < 0.5, "真资产上协方差误差 {worst} 已不可接受");
    }
}
