//! 3DGS PLY 读入（ASCII / binary_little_endian）。
//!
//! 只依赖顶点元素；元素顺序可以任意，其它元素按行跳过。标准 3DGS 语义：
//! `scale_*` 是 log 标准差、`opacity` 是 logit、`rot_0..3` 是 [w,x,y,z]。
//! 这两个激活可用 [`PlyReadOptions`] 关掉，兼容已激活的导出。

use super::{Gaussian, normalize_quat, sh_dc_to_rgb, sigmoid};

#[derive(Clone, Copy, Debug)]
pub struct PlyReadOptions {
    pub scales_are_log: bool,
    pub opacity_is_logit: bool,
}

impl Default for PlyReadOptions {
    fn default() -> Self {
        Self {
            scales_are_log: true,
            opacity_is_logit: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlyData {
    pub gaussians: Vec<Gaussian>,
    pub has_normals: bool,
    pub format: &'static str,
    pub source_vertices: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Ty {
    fn parse(s: &str) -> Result<Self, String> {
        Ok(match s {
            "char" | "int8" => Ty::I8,
            "uchar" | "uint8" => Ty::U8,
            "short" | "int16" => Ty::I16,
            "ushort" | "uint16" => Ty::U16,
            "int" | "int32" => Ty::I32,
            "uint" | "uint32" => Ty::U32,
            "float" | "float32" => Ty::F32,
            "double" | "float64" => Ty::F64,
            _ => return Err(format!("未知 PLY 类型：{s}")),
        })
    }

    fn size(self) -> usize {
        match self {
            Ty::I8 | Ty::U8 => 1,
            Ty::I16 | Ty::U16 => 2,
            Ty::I32 | Ty::U32 | Ty::F32 => 4,
            Ty::F64 => 8,
        }
    }

    fn text(self, s: &str) -> Result<f64, String> {
        Ok(match self {
            Ty::F32 | Ty::F64 => s.parse::<f64>().map_err(|_| format!("不是数字：{s}"))?,
            Ty::I8 | Ty::I16 | Ty::I32 => {
                s.parse::<i64>().map_err(|_| format!("不是整数：{s}"))? as f64
            }
            Ty::U8 | Ty::U16 | Ty::U32 => {
                s.parse::<u64>()
                    .map_err(|_| format!("不是无符号整数：{s}"))? as f64
            }
        })
    }
}

#[derive(Clone, Debug)]
struct Prop {
    name: String,
    ty: Ty,
    list_count: Option<Ty>,
    list_item: Option<Ty>,
}

impl Prop {
    fn is_list(&self) -> bool {
        self.list_count.is_some()
    }
}

#[derive(Clone, Debug)]
struct Element {
    name: String,
    count: usize,
    props: Vec<Prop>,
}

pub fn read_ply(path: &std::path::Path) -> Result<PlyData, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读不了 {}：{e}", path.display()))?;
    parse_ply(&bytes)
}

pub fn parse_ply(bytes: &[u8]) -> Result<PlyData, String> {
    parse_ply_with(bytes, &PlyReadOptions::default())
}

pub fn parse_ply_with(bytes: &[u8], opts: &PlyReadOptions) -> Result<PlyData, String> {
    let (format, elements, data_start) = parse_header(bytes)?;
    let mut gaussians = Vec::new();
    let mut has_normals = false;
    match format {
        Format::Ascii => {
            let text = std::str::from_utf8(&bytes[data_start..])
                .map_err(|_| "ASCII PLY 的正文不是 UTF-8".to_string())?;
            let mut lines = text.lines();
            for elem in &elements {
                let ix = Indexes::new(elem);
                for row in 0..elem.count {
                    let line = loop {
                        match lines.next() {
                            Some(l) if l.trim().is_empty() => continue,
                            Some(l) => break l,
                            None => {
                                return Err(format!(
                                    "PLY 截断：元素 {} 需要 {} 行，缺少第 {} 行",
                                    elem.name, elem.count, row
                                ));
                            }
                        }
                    };
                    if elem.name == "vertex" {
                        let toks: Vec<&str> = line.split_whitespace().collect();
                        if toks.len() < elem.props.len() {
                            return Err(format!("vertex 第 {row} 行列数不足"));
                        }
                        let mut vals = Vec::with_capacity(elem.props.len());
                        for (k, p) in elem.props.iter().enumerate() {
                            if p.is_list() {
                                return Err("vertex 元素不支持 list 属性".into());
                            }
                            vals.push(p.ty.text(toks[k])?);
                        }
                        let (g, has_n) = build_gaussian(&vals, &ix, row, opts)?;
                        has_normals |= has_n;
                        gaussians.push(g);
                    }
                }
            }
        }
        Format::BinaryLittle => {
            let mut pos = data_start;
            for elem in &elements {
                let ix = Indexes::new(elem);
                let stride: usize = elem.props.iter().map(|p| p.ty.size()).sum();
                if elem.name != "vertex" {
                    for _ in 0..elem.count {
                        for p in &elem.props {
                            if let Some(ct) = p.list_count {
                                let n = read_bin(bytes, &mut pos, ct)? as usize;
                                pos = pos
                                    .checked_add(n * p.list_item.unwrap().size())
                                    .ok_or("PLY 偏移溢出")?;
                            } else {
                                pos = pos.checked_add(p.ty.size()).ok_or("PLY 偏移溢出")?;
                            }
                        }
                    }
                    if pos > bytes.len() {
                        return Err("PLY 截断（跳过非顶点元素时越界）".into());
                    }
                    continue;
                }
                if elem.props.iter().any(Prop::is_list) {
                    return Err("vertex 元素不支持 list 属性".into());
                }
                let end = pos
                    .checked_add(elem.count * stride)
                    .ok_or("PLY 数据长度溢出")?;
                if end > bytes.len() {
                    return Err(format!(
                        "PLY 截断：vertex {} 行 × {} 字节，只剩 {} 字节",
                        elem.count,
                        stride,
                        bytes.len().saturating_sub(pos)
                    ));
                }
                let mut offsets = Vec::with_capacity(elem.props.len());
                let mut off = 0usize;
                for p in &elem.props {
                    offsets.push(off);
                    off += p.ty.size();
                }
                for row in 0..elem.count {
                    let base = pos + row * stride;
                    let mut vals = Vec::with_capacity(elem.props.len());
                    for (k, p) in elem.props.iter().enumerate() {
                        let mut cursor = base + offsets[k];
                        vals.push(read_bin(bytes, &mut cursor, p.ty)?);
                    }
                    let (g, has_n) = build_gaussian(&vals, &ix, row, opts)?;
                    has_normals |= has_n;
                    gaussians.push(g);
                }
                pos = end;
            }
        }
    }
    if gaussians.is_empty() {
        return Err("PLY 里没有 vertex 高斯".into());
    }
    Ok(PlyData {
        source_vertices: gaussians.len(),
        gaussians,
        has_normals,
        format: match format {
            Format::Ascii => "ascii",
            Format::BinaryLittle => "binary_little_endian",
        },
    })
}

enum Format {
    Ascii,
    BinaryLittle,
}

fn parse_header(bytes: &[u8]) -> Result<(Format, Vec<Element>, usize), String> {
    let end = bytes
        .windows(10)
        .position(|w| w == b"end_header")
        .ok_or("不是 PLY：缺 end_header")?;
    let line_end = bytes[end..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|p| end + p + 1)
        .ok_or("PLY 头没有结束换行")?;
    let header = std::str::from_utf8(&bytes[..line_end]).map_err(|_| "PLY 头不是 ASCII")?;
    let mut lines = header.lines();
    if lines.next().map(str::trim) != Some("ply") {
        return Err("不是 PLY：缺 magic".into());
    }
    let mut format = None;
    let mut elements: Vec<Element> = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("comment") || line.starts_with("obj_info") {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        match toks.first().copied() {
            Some("format") => {
                if toks.len() < 2 {
                    return Err("format 行缺格式".into());
                }
                format = Some(match toks[1] {
                    "ascii" => Format::Ascii,
                    "binary_little_endian" => Format::BinaryLittle,
                    "binary_big_endian" => {
                        return Err("不支持 binary_big_endian（请导出为 little endian）".into());
                    }
                    other => return Err(format!("未知 PLY format：{other}")),
                });
            }
            Some("element") => {
                if toks.len() < 3 {
                    return Err("element 行格式错误".into());
                }
                let count = toks[2]
                    .parse::<usize>()
                    .map_err(|_| format!("element 行计数无效：{}", toks[2]))?;
                elements.push(Element {
                    name: toks[1].to_string(),
                    count,
                    props: Vec::new(),
                });
            }
            Some("property") => {
                let elem = elements.last_mut().ok_or("property 出现在 element 之前")?;
                if toks.len() >= 5 && toks[1] == "list" {
                    elem.props.push(Prop {
                        name: toks[4].to_string(),
                        ty: Ty::U8,
                        list_count: Some(Ty::parse(toks[2])?),
                        list_item: Some(Ty::parse(toks[3])?),
                    });
                } else if toks.len() >= 3 {
                    elem.props.push(Prop {
                        name: toks[2].to_string(),
                        ty: Ty::parse(toks[1])?,
                        list_count: None,
                        list_item: None,
                    });
                } else {
                    return Err(format!("property 行格式错误：{line}"));
                }
            }
            Some("end_header") => break,
            Some(other) => return Err(format!("未知 PLY 头指令：{other}")),
            None => {}
        }
    }
    let format = format.ok_or("PLY 头缺 format")?;
    Ok((format, elements, line_end))
}

fn read_bin(bytes: &[u8], pos: &mut usize, ty: Ty) -> Result<f64, String> {
    let n = ty.size();
    let s = bytes.get(*pos..*pos + n).ok_or("PLY 正文越界")?;
    *pos += n;
    Ok(match ty {
        Ty::I8 => i8::from_le_bytes([s[0]]) as f64,
        Ty::U8 => s[0] as f64,
        Ty::I16 => i16::from_le_bytes([s[0], s[1]]) as f64,
        Ty::U16 => u16::from_le_bytes([s[0], s[1]]) as f64,
        Ty::I32 => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        Ty::U32 => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        Ty::F32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        Ty::F64 => f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]),
    })
}

struct Indexes {
    x: Option<usize>,
    y: Option<usize>,
    z: Option<usize>,
    nx: Option<usize>,
    ny: Option<usize>,
    nz: Option<usize>,
    scale: [Option<usize>; 3],
    rot: [Option<usize>; 4],
    opacity: Option<usize>,
    color: [Option<usize>; 3],
}

impl Indexes {
    fn new(elem: &Element) -> Self {
        let find = |name: &str| elem.props.iter().position(|p| p.name == name);
        Self {
            x: find("x"),
            y: find("y"),
            z: find("z"),
            nx: find("nx"),
            ny: find("ny"),
            nz: find("nz"),
            scale: [find("scale_0"), find("scale_1"), find("scale_2")],
            rot: [find("rot_0"), find("rot_1"), find("rot_2"), find("rot_3")],
            opacity: find("opacity"),
            color: [find("f_dc_0"), find("f_dc_1"), find("f_dc_2")],
        }
    }
}

fn build_gaussian(
    vals: &[f64],
    ix: &Indexes,
    row: usize,
    opts: &PlyReadOptions,
) -> Result<(Gaussian, bool), String> {
    let get = |i: Option<usize>| i.map(|k| vals[k]);
    let (x, y, z) = match (ix.x, ix.y, ix.z) {
        (Some(x), Some(y), Some(z)) => (vals[x], vals[y], vals[z]),
        _ => return Err(format!("vertex 第 {row} 行缺 x/y/z")),
    };
    let mut scale = [0.01f32; 3];
    for (out, idx) in scale.iter_mut().zip(ix.scale) {
        if let Some(v) = get(idx) {
            let s = if opts.scales_are_log { v.exp() } else { v };
            *out = s as f32;
        }
    }
    let mut rot = [1.0f32, 0.0, 0.0, 0.0];
    if ix.rot.iter().all(Option::is_some) {
        for k in 0..4 {
            rot[k] = vals[ix.rot[k].unwrap()] as f32;
        }
        rot = normalize_quat(rot);
    }
    let opacity = match get(ix.opacity) {
        Some(v) => {
            let a = if opts.opacity_is_logit {
                sigmoid(v as f32)
            } else {
                v as f32
            };
            a.clamp(0.0, 1.0)
        }
        None => 1.0,
    };
    let color = if ix.color.iter().all(Option::is_some) {
        sh_dc_to_rgb([
            vals[ix.color[0].unwrap()] as f32,
            vals[ix.color[1].unwrap()] as f32,
            vals[ix.color[2].unwrap()] as f32,
        ])
    } else {
        [0.82, 0.84, 0.90]
    };
    let normal = match (get(ix.nx), get(ix.ny), get(ix.nz)) {
        (Some(a), Some(b), Some(c)) => {
            let n = [a as f32, b as f32, c as f32];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-8 && len.is_finite() {
                Some([n[0] / len, n[1] / len, n[2] / len])
            } else {
                None
            }
        }
        _ => None,
    };
    let g = Gaussian {
        mean: [x as f32, y as f32, z as f32],
        scale,
        rot,
        opacity,
        color,
        normal,
    };
    if !g.is_finite() {
        return Err(format!("vertex 第 {row} 行含 NaN/Inf 或非正 scale"));
    }
    Ok((g, normal.is_some()))
}
