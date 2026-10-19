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
        "{{\n\"title\":{},\n\"subject\":{},\n\"stats\":{{\"mode\":{},\"grid\":{},\"vertices\":{},\"triangles\":{},\"volume\":{:.6},\"chi\":{},\"components\":{},\"observed\":{},\"sealed\":{},\"iso\":{:.6},\"hash\":\"{:016x}\"}},\n",
        jstr(d.title),
        jstr(d.subject),
        jstr(d.mode_used),
        d.field.grid.n,
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
    data.push_str("\"mesh\":{\"positions\":[");
    for (i, p) in d.mesh.positions.iter().enumerate() {
        if i > 0 {
            data.push(',');
        }
        let _ = write!(data, "{:.6},{:.6},{:.6}", p[0], p[1], p[2]);
    }
    data.push_str("],\"normals\":[");
    for (i, n) in d.mesh.normals.iter().enumerate() {
        if i > 0 {
            data.push(',');
        }
        let _ = write!(data, "{:.4},{:.4},{:.4}", n[0], n[1], n[2]);
    }
    data.push_str("],\"indices\":[");
    for (i, idx) in d.mesh.indices.iter().enumerate() {
        if i > 0 {
            data.push(',');
        }
        let _ = write!(data, "{idx}");
    }
    data.push_str("]},\n\"points\":{\"positions\":[");
    let stride = (gaussians.len() / 150_000).max(1);
    let mut first = true;
    let mut colors = String::new();
    for g in gaussians.iter().step_by(stride) {
        if !first {
            data.push(',');
            colors.push(',');
        }
        first = false;
        let _ = write!(data, "{:.5},{:.5},{:.5}", g.mean[0], g.mean[1], g.mean[2]);
        let _ = write!(
            colors,
            "{:.4},{:.4},{:.4}",
            g.color[0], g.color[1], g.color[2]
        );
    }
    data.push_str("],\"colors\":[");
    data.push_str(&colors);
    data.push_str("]}\n}");
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
