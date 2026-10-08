//! SPZ 读入（T-GC-04 第五片）：Niantic SPZ（v2）→ planar `GaussianCloud`。
//! 锚 `W15A-043`（wgpu-3dgs-core 0.8.0 参照）/`W15A-040..044`。
//!
//! 映射规则（钉死；改规则 = 改契约，由金样哈希 `tests/spz_read.rs` 强制披露）：
//!   position  = i24 定点（3 字节 LE、符号扩展）× 2^(−fractional_bits)（乘 2 的幂：精确）
//!   log_scale = b/16 − 10（SPZ 以 u8 存 log 域；本仓平面本就是 log 域 ⇒ 直落，无 exp/ln 往返）
//!   rotation  = xyz = b/127.5 − 1，w = √(max(0, 1−|xyz|²))（SPZ 惯例导出 w；与参照同口径，
//!               不二次归一化——量化越界交给 max(0)，如实承载）
//!   opacity   = logit((b+0.5)/256)（SPZ 存线性 α；半格夹逼保证 logit 有限；本仓平面为 logit 域）
//!   color     = 0.5 + SH_C0 × ((b/255 − 0.5)/0.15)（SPZ 以 f_dc×0.15+0.5 量化；折算回本仓 SH-DC 约定）
//!   sh_rest   = (b − 128)/128，**系数主序**（每系数 (r,g,b)）；stride = 每顶点值数
//!               （3×系数个数，与 PLY 属性数同口径）；无高阶（degree 0）则无平面
//! 容器：gzip 包装（1f 8b 起手）自动解压（flate2/miniz_oxide，纯 Rust 后端）；尾部多余字节忽略。
//! 版本口径（fail-closed）：只支持 v2（v1 = f16 位置、v3 = smallest-three 四元数，均为不同编码，
//! 显式 `UnsupportedVersion` 拒收——不静默错读）；sh_degree ≤ 3、fractional_bits ≤ 30 同拒越界。

use std::borrow::Cow;

use crate::gaussian::GaussianCloud;

/// SPZ 魔数（"NGSP" 按小端读作 u32）。
pub const SPZ_MAGIC: u32 = 0x5053474e;

/// 读入报告（显式披露容器与编码形态）。
#[derive(Debug, Clone, Copy)]
pub struct SpzInfo {
    pub version: u32,
    pub point_count: u32,
    pub sh_degree: u8,
    pub fractional_bits: u8,
    /// flags bit0：antialiased 元数据（不影响解码）
    pub antialiased: bool,
    /// 是否 gzip 包装
    pub gzip: bool,
}

#[derive(Debug)]
pub enum SpzError {
    BadMagic,
    UnsupportedVersion(u32),
    UnsupportedShDegree(u8),
    BadFractionalBits(u8),
    Truncated,
    BadGzip,
}

const HEADER_LEN: usize = 16;

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// i24（3 字节 LE）→ i32（符号扩展）。
fn i24_le(b: &[u8]) -> i32 {
    let v = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
    (v << 8) >> 8
}

/// SPZ 颜色 u8（f_dc×0.15+0.5 量化）→ 本仓 SH-DC 约定 0.5 + SH_C0×f_dc。
fn decode_color(b: u8) -> f32 {
    let f_dc = (b as f32 / 255.0 - 0.5) / 0.15;
    0.5 + crate::ply::SH_C0 * f_dc
}

/// 读入 SPZ（gzip 包装自动解压）→ (planar 高斯云, 读入报告)。
pub fn load_spz(bytes: &[u8]) -> Result<(GaussianCloud, SpzInfo), SpzError> {
    let (raw, gzip): (Cow<[u8]>, bool) = if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
    {
        let mut out = Vec::new();
        let mut dec = flate2::read::GzDecoder::new(bytes);
        std::io::Read::read_to_end(&mut dec, &mut out).map_err(|_| SpzError::BadGzip)?;
        (Cow::Owned(out), true)
    } else {
        (Cow::Borrowed(bytes), false)
    };
    let raw: &[u8] = &raw;

    if raw.len() < HEADER_LEN {
        return Err(SpzError::Truncated);
    }
    if le_u32(&raw[0..4]) != SPZ_MAGIC {
        return Err(SpzError::BadMagic);
    }
    let version = le_u32(&raw[4..8]);
    if version != 2 {
        return Err(SpzError::UnsupportedVersion(version));
    }
    let point_count = le_u32(&raw[8..12]);
    let sh_degree = raw[12];
    if sh_degree > 3 {
        return Err(SpzError::UnsupportedShDegree(sh_degree));
    }
    let fractional_bits = raw[13];
    if fractional_bits > 30 {
        return Err(SpzError::BadFractionalBits(fractional_bits));
    }
    let antialiased = raw[14] & 0x1 != 0;

    let count = point_count as usize;
    let sh_coeffs = match sh_degree {
        0 => 0usize,
        d => (d as usize + 1).pow(2) - 1,
    };
    let sh_stride = sh_coeffs * 3;
    let need = count * (9 + 1 + 3 + 3 + 3 + sh_stride);
    let body = &raw[HEADER_LEN..];
    if body.len() < need {
        return Err(SpzError::Truncated);
    }

    let pos_scale = 1.0f32 / (1u64 << fractional_bits) as f32;
    let mut positions = Vec::with_capacity(count);
    for i in 0..count {
        let p = &body[i * 9..i * 9 + 9];
        positions.push([
            i24_le(&p[0..3]) as f32 * pos_scale,
            i24_le(&p[3..6]) as f32 * pos_scale,
            i24_le(&p[6..9]) as f32 * pos_scale,
        ]);
    }
    let alpha_off = count * 9;
    let color_off = alpha_off + count;
    let scale_off = color_off + count * 3;
    let rot_off = scale_off + count * 3;
    let sh_off = rot_off + count * 3;

    let mut opacities = Vec::with_capacity(count);
    for i in 0..count {
        let a = (body[alpha_off + i] as f32 + 0.5) / 256.0;
        opacities.push((a / (1.0 - a)).ln());
    }

    let mut colors = Vec::with_capacity(count);
    for i in 0..count {
        let c = &body[color_off + i * 3..color_off + i * 3 + 3];
        colors.push([decode_color(c[0]), decode_color(c[1]), decode_color(c[2])]);
    }

    let mut log_scales = Vec::with_capacity(count);
    for i in 0..count {
        let s = &body[scale_off + i * 3..scale_off + i * 3 + 3];
        log_scales.push([
            s[0] as f32 / 16.0 - 10.0,
            s[1] as f32 / 16.0 - 10.0,
            s[2] as f32 / 16.0 - 10.0,
        ]);
    }

    let mut rotations = Vec::with_capacity(count);
    for i in 0..count {
        let q = &body[rot_off + i * 3..rot_off + i * 3 + 3];
        let x = q[0] as f32 / 127.5 - 1.0;
        let y = q[1] as f32 / 127.5 - 1.0;
        let z = q[2] as f32 / 127.5 - 1.0;
        let w = (1.0 - (x * x + y * y + z * z)).max(0.0).sqrt();
        rotations.push([x, y, z, w]);
    }

    let cloud = GaussianCloud::new(positions, log_scales, rotations, opacities, colors);
    let cloud = if sh_stride > 0 {
        let mut sh = Vec::with_capacity(count * sh_stride);
        for i in 0..count {
            for k in 0..sh_stride {
                sh.push((body[sh_off + i * sh_stride + k] as f32 - 128.0) / 128.0);
            }
        }
        cloud.with_sh_rest(sh, sh_stride as u32)
    } else {
        cloud
    };

    Ok((
        cloud,
        SpzInfo {
            version,
            point_count,
            sh_degree,
            fractional_bits,
            antialiased,
            gzip,
        },
    ))
}
