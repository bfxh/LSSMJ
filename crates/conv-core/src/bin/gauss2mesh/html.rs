//! 自包含 WebGL 预览：mesh + 高斯点 + 统计 + 警告。

use crate::report::{ReportData, jstr};
use conv_core::gauss::{Gaussian, bounds_of};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

pub(crate) fn write_html(
    path: &Path,
    d: &ReportData<'_>,
    gaussians: &[Gaussian],
) -> std::io::Result<()> {
    let template = include_str!("../../viewer.html");
    let mut data = String::with_capacity(1 << 22);
    let b = d.mesh.stats.bounds.unwrap_or_else(|| bounds_of(gaussians));
    let _ = write!(
        data,
        "{{\n\"title\":{},\n\"subject\":{},\n\"stats\":{{\"mode\":{},\"backend\":{},\"mesher\":{},\"bins3d\":{},\"simd_prep\":{},\"grid\":{},\"workers\":{},\"vertices\":{},\"triangles\":{},\"volume\":{:.6},\"chi\":{},\"components\":{},\"observed\":{},\"sealed\":{},\"iso\":{:.6},\"hash\":\"{:016x}\"}},\n",
        jstr(d.title),
        jstr(d.subject),
        jstr(d.mode_used),
        jstr(d.field.backend.name()),
        jstr(d.mesher),
        d.field.bins3d,
        d.field.simd_prep,
        d.field.grid.n,
        d.field.workers,
        d.mesh.stats.vertices,
        d.mesh.stats.triangles,
        d.mesh.stats.volume,
        d.mesh.stats.chi,
        d.mesh.stats.components,
        d.field.observed,
        d.field.sealed_interior,
        d.field.iso,
        d.mesh.stats.hash
    );
    data.push_str("\n\"warnings\":[");
    for (i, w) in d.warnings.iter().enumerate() {
        if i > 0 {
            data.push(',');
        }
        data.push_str(&jstr(w));
    }
    data.push_str("],\n");
    let _ = writeln!(
        data,
        "\"bounds\":{{\"min\":[{:.6},{:.6},{:.6}],\"max\":[{:.6},{:.6},{:.6}]}},",
        b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]
    );
    // 二进制数组用 base64 嵌入，避免逐浮点格式化：128³/68k 顶点时 HTML 写盘约减半。
    data.push_str("\"mesh\":{\"pos\":\"");
    data.push_str(&b64_f32x3(&d.mesh.positions));
    data.push_str("\",\"nrm\":\"");
    data.push_str(&b64_f32x3(&d.mesh.normals));
    data.push_str("\",\"idx\":\"");
    data.push_str(&b64_u32(&d.mesh.indices));
    let _ = writeln!(
        data,
        "\",\"vertices\":{},\"triangles\":{}}},",
        d.mesh.stats.vertices, d.mesh.stats.triangles
    );
    let stride = (gaussians.len() / 150_000).max(1);
    let count = gaussians.len().div_ceil(stride);
    let mut points = Vec::with_capacity(count);
    let mut colors = Vec::with_capacity(count);
    for g in gaussians.iter().step_by(stride) {
        points.push(g.mean);
        colors.push(g.color);
    }
    data.push_str("\"points\":{\"pos\":\"");
    data.push_str(&b64_f32x3(&points));
    data.push_str("\",\"col\":\"");
    data.push_str(&b64_f32x3(&colors));
    let _ = writeln!(data, "\",\"count\":{count}}}");
    data.push('}');
    let title = html_escape(d.title);
    let subject = html_escape(d.subject);
    let out = template
        .replace("__TITLE__", &title)
        .replace("__SUBJECT__", &subject)
        .replace("__DATA_JSON__", &data);
    fs::write(path, out)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b0 = c[0] as u32;
        let b1 = c.get(1).copied().unwrap_or(0) as u32;
        let b2 = c.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((n >> 18) & 63) as usize]);
        out.push(B64[((n >> 12) & 63) as usize]);
        out.push(if c.len() > 1 {
            B64[((n >> 6) & 63) as usize]
        } else {
            b'='
        });
        out.push(if c.len() > 2 {
            B64[(n & 63) as usize]
        } else {
            b'='
        });
    }
    String::from_utf8(out).expect("base64 is ASCII")
}

fn b64_f32x3(values: &[[f32; 3]]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 12);
    for v in values {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    b64(&bytes)
}

fn b64_u32(values: &[u32]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for v in values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    b64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors_and_padding() {
        assert_eq!(b64(b""), "");
        assert_eq!(b64(b"f"), "Zg==");
        assert_eq!(b64(b"fo"), "Zm8=");
        assert_eq!(b64(b"foo"), "Zm9v");
        assert_eq!(b64(&[0u8; 3]), "AAAA");
        assert_eq!(b64_f32x3(&[[0.0; 3]]).len(), 16);
        assert_eq!(b64_u32(&[1, 2, 3]).len(), 16);
    }
}
