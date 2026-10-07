//! PLY 读入（T-GC-04 第二片）：3DGS 标准 PLY（ascii / binary_little_endian）→ planar `GaussianCloud`。
//!
//! 映射规则（钉死；改规则 = 改契约，由金样哈希 `tests/ply_read.rs` 强制披露）：
//!   position  = (x, y, z) 原样
//!   log_scale = (scale_0, scale_1, scale_2) 原样（3DGS 惯例本就是 log 域）
//!   rotation  = (rot_1, rot_2, rot_3, rot_0)——PLY 惯例 rot_0 为实部 w，本仓平面为 (x,y,z,w)
//!   opacity   = opacity 原样（logit 域）
//!   color     = 0.5 + SH_C0 × (f_dc_0..2)（SH DC → 线性 RGB 的 3DGS 约定）
//! `f_rest_*`（高阶 SH）**显式忽略**并记入 `PlyInfo.ignored_f_rest`；其余未知属性、非 float
//! 类型、非 vertex 元素（count>0）一律 `Err`——不静默吞。量化变体属后续片。

use std::collections::HashMap;

use crate::gaussian::GaussianCloud;

/// SH 零阶系数（3DGS 约定：color = 0.5 + SH_C0 × f_dc；= 1/(2√π)，经 f64 舍入到 f32）。
pub const SH_C0: f32 = 0.28209479177387814_f64 as f32;

const REQUIRED_PROPS: [&str; 14] = [
    "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1", "scale_2",
    "rot_0", "rot_1", "rot_2", "rot_3",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlyFormat {
    Ascii,
    BinaryLittleEndian,
}

/// 读入报告（显式披露忽略/格式，不静默）。
#[derive(Debug, Clone, Copy)]
pub struct PlyInfo {
    pub format: PlyFormat,
    pub vertex_count: u32,
    pub ignored_f_rest: u32,
}

#[derive(Debug)]
pub enum PlyError {
    BadMagic,
    BadHeader(&'static str),
    UnsupportedFormat(String),
    UnsupportedElement(String),
    UnsupportedProperty { ty: String, name: String },
    MissingProperty(&'static str),
    Truncated,
    BadFloat(String),
}

/// 读入 3DGS PLY → (planar 高斯云, 读入报告)。
pub fn load_ply(bytes: &[u8]) -> Result<(GaussianCloud, PlyInfo), PlyError> {
    // ---- 头部（首行即验魔数）----
    let mut pos = 0usize;
    let mut lines: Vec<String> = Vec::new();
    let mut body_start = None;
    while pos < bytes.len() {
        let nl = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or(PlyError::BadHeader("缺少 end_header"))?;
        let mut line = &bytes[pos..pos + nl];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        let s = std::str::from_utf8(line)
            .map_err(|_| PlyError::BadHeader("头部非 UTF-8"))?
            .to_string();
        pos += nl + 1;
        if lines.is_empty() {
            if s != "ply" {
                return Err(PlyError::BadMagic);
            }
            lines.push(s);
            continue;
        }
        if s == "end_header" {
            body_start = Some(pos);
            break;
        }
        lines.push(s);
    }
    let body_start = body_start.ok_or(PlyError::BadHeader("缺少 end_header"))?;

    let mut format: Option<PlyFormat> = None;
    let mut vertex_count: Option<usize> = None;
    let mut in_vertex = false;
    let mut props: Vec<(String, String)> = Vec::new(); // (ty, name)
    for l in &lines[1..] {
        let mut it = l.split_whitespace();
        match it.next() {
            Some("format") => {
                let f = it.next().unwrap_or("");
                format = Some(match f {
                    "ascii" => PlyFormat::Ascii,
                    "binary_little_endian" => PlyFormat::BinaryLittleEndian,
                    other => return Err(PlyError::UnsupportedFormat(other.to_string())),
                });
            }
            Some("comment") | Some("obj_info") => {}
            Some("element") => {
                let name = it.next().unwrap_or("");
                let n: usize = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or(PlyError::BadHeader("element 计数非法"))?;
                if name == "vertex" {
                    in_vertex = true;
                    vertex_count = Some(n);
                } else if n > 0 {
                    return Err(PlyError::UnsupportedElement(name.to_string()));
                } else {
                    in_vertex = false;
                }
            }
            Some("property") => {
                let ty = it.next().unwrap_or("");
                let name = it.next().unwrap_or("");
                if in_vertex {
                    props.push((ty.to_string(), name.to_string()));
                }
            }
            Some(_) => return Err(PlyError::BadHeader("未知头部关键字")),
            None => {}
        }
    }
    let format = format.ok_or(PlyError::BadHeader("缺 format 行"))?;
    let count = vertex_count.ok_or(PlyError::BadHeader("缺 vertex 元素"))?;

    // ---- 属性分类：必需 / f_rest_*（忽略）/ 其余未知一律拒 ----
    let mut idx_of: HashMap<String, usize> = HashMap::new();
    let mut ignored_f_rest = 0u32;
    for (i, (ty, name)) in props.iter().enumerate() {
        if name.starts_with("f_rest_") {
            if ty != "float" {
                return Err(PlyError::UnsupportedProperty {
                    ty: ty.clone(),
                    name: name.clone(),
                });
            }
            ignored_f_rest += 1;
            continue;
        }
        if !REQUIRED_PROPS.contains(&name.as_str()) || ty != "float" {
            return Err(PlyError::UnsupportedProperty {
                ty: ty.clone(),
                name: name.clone(),
            });
        }
        idx_of.insert(name.clone(), i);
    }
    for r in REQUIRED_PROPS {
        if !idx_of.contains_key(r) {
            return Err(PlyError::MissingProperty(r));
        }
    }

    // ---- 顶点体 ----
    let n_props = props.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(count);
    let mut log_scales: Vec<[f32; 3]> = Vec::with_capacity(count);
    let mut rotations: Vec<[f32; 4]> = Vec::with_capacity(count);
    let mut opacities: Vec<f32> = Vec::with_capacity(count);
    let mut colors: Vec<[f32; 3]> = Vec::with_capacity(count);

    let mut take = |vals: &[f32]| {
        let g = |n: &str| vals[idx_of[n]];
        positions.push([g("x"), g("y"), g("z")]);
        log_scales.push([g("scale_0"), g("scale_1"), g("scale_2")]);
        rotations.push([g("rot_1"), g("rot_2"), g("rot_3"), g("rot_0")]);
        opacities.push(g("opacity"));
        let d = |n: &str| 0.5 + SH_C0 * g(n);
        colors.push([d("f_dc_0"), d("f_dc_1"), d("f_dc_2")]);
    };

    match format {
        PlyFormat::Ascii => {
            let text = std::str::from_utf8(&bytes[body_start..])
                .map_err(|_| PlyError::BadHeader("ascii 体非 UTF-8"))?;
            let mut tok = text.split_whitespace();
            let mut vals: Vec<f32> = Vec::with_capacity(n_props);
            for _ in 0..count {
                vals.clear();
                for _ in 0..n_props {
                    let t = tok.next().ok_or(PlyError::Truncated)?;
                    vals.push(t.parse().map_err(|_| PlyError::BadFloat(t.to_string()))?);
                }
                take(&vals);
            }
        }
        PlyFormat::BinaryLittleEndian => {
            let stride = n_props * 4;
            let need = count.checked_mul(stride).ok_or(PlyError::Truncated)?;
            let body = bytes
                .get(body_start..body_start + need)
                .ok_or(PlyError::Truncated)?;
            let mut vals: Vec<f32> = Vec::with_capacity(n_props);
            for v in 0..count {
                vals.clear();
                let base = v * stride;
                for i in 0..n_props {
                    let b = &body[base + 4 * i..base + 4 * i + 4];
                    vals.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                }
                take(&vals);
            }
        }
    }

    let cloud = GaussianCloud::new(positions, log_scales, rotations, opacities, colors);
    Ok((
        cloud,
        PlyInfo {
            format,
            vertex_count: count as u32,
            ignored_f_rest,
        },
    ))
}
