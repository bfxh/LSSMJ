//! 机器可读报告：转换统计 + 显式降级记账。

use conv_core::gauss::PruneStats;
use conv_core::gauss::mesh::Mesh;
use conv_core::gauss::voxel::Field;
use std::fmt::Write as _;

#[derive(Clone, Copy)]
pub(crate) struct Timings {
    pub(crate) load: f64,
    pub(crate) prune: f64,
    pub(crate) voxel: f64,
    pub(crate) mesh: f64,
    pub(crate) write: f64,
    pub(crate) gpu_init: f64,
}
impl Timings {
    pub(crate) fn new() -> Self {
        Self {
            load: 0.0,
            prune: 0.0,
            voxel: 0.0,
            mesh: 0.0,
            write: 0.0,
            gpu_init: 0.0,
        }
    }
}

pub(crate) struct ReportData<'a> {
    pub(crate) title: &'a str,
    pub(crate) subject: &'a str,
    pub(crate) format: &'a str,
    pub(crate) mode_requested: &'a str,
    pub(crate) mode_used: &'a str,
    pub(crate) auto_reason: &'a str,
    pub(crate) fallback: Option<&'a str>,
    pub(crate) gpu_fallback: Option<&'a str>,
    pub(crate) iso_retry: Option<&'a str>,
    pub(crate) timings: Timings,
    pub(crate) prune_stats: PruneStats,
    pub(crate) field: &'a Field,
    pub(crate) mesh: &'a Mesh,
    pub(crate) warnings: &'a [String],
    pub(crate) has_normals: bool,
    pub(crate) flatness: f32,
    pub(crate) hollow: bool,
}

pub(crate) fn report_json(d: &ReportData<'_>) -> String {
    let mut s = String::with_capacity(4096);
    let bounds = d.mesh.stats.bounds.map(|b| {
        format!(
            "{{\"min\":[{:.6},{:.6},{:.6}],\"max\":[{:.6},{:.6},{:.6}]}}",
            b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]
        )
    });
    let warnings = d
        .warnings
        .iter()
        .map(|w| jstr(w))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = write!(
        s,
        concat!(
            "{{\n",
            "  \"title\": {},\n",
            "  \"subject\": {},\n",
            "  \"format\": {},\n",
            "  \"mode_requested\": {},\n",
            "  \"mode_used\": {},\n",
            "  \"auto_reason\": {},\n",
            "  \"fallback\": {},\n",
            "  \"gpu_fallback\": {},\n",
            "  \"iso_retry\": {},\n",
            "  \"has_normals\": {},\n",
            "  \"flatness_p50\": {:.5},\n",
            "  \"hollow\": {},\n",
            "  \"gaussians\": {{\"input\": {}, \"kept\": {}, \"dropped_opacity\": {}, \"dropped_radius\": {}, \"dropped_invalid\": {}}},\n",
            "  \"grid\": {{\"n\": {}, \"h\": {:.8}}},\n",
            "  \"field\": {{\"mode\": {}, \"backend\": {}, \"iso\": {:.8}, \"threads\": {}, \"observed\": {}, \"sealed_interior\": {}}},\n",
            "  \"mesh\": {{\"vertices\": {}, \"triangles\": {}, \"components\": {}, \"chi\": {}, \"volume\": {:.8}, \"boundary_touch\": {}, \"hash\": \"{:016x}\", \"bounds\": {}}},\n",
            "  \"timings_ms\": {{\"load\": {:.3}, \"prune\": {:.3}, \"voxel\": {:.3}, \"mesh\": {:.3}, \"write\": {:.3}, \"gpu_init\": {:.3}}},\n",
            "  \"warnings\": [{}]\n",
            "}}\n"
        ),
        jstr(d.title),
        jstr(d.subject),
        jstr(d.format),
        jstr(d.mode_requested),
        jstr(d.mode_used),
        jstr(d.auto_reason),
        d.fallback.map(jstr).unwrap_or_else(|| "null".into()),
        d.gpu_fallback.map(jstr).unwrap_or_else(|| "null".into()),
        d.iso_retry.map(jstr).unwrap_or_else(|| "null".into()),
        d.has_normals,
        d.flatness,
        d.hollow,
        d.prune_stats.input,
        d.prune_stats.kept,
        d.prune_stats.dropped_opacity,
        d.prune_stats.dropped_radius,
        d.prune_stats.dropped_invalid,
        d.field.grid.n,
        d.field.grid.h,
        jstr(d.field.mode.name()),
        jstr(d.field.backend.name()),
        d.field.iso,
        d.field.workers,
        d.field.observed,
        d.field.sealed_interior,
        d.mesh.stats.vertices,
        d.mesh.stats.triangles,
        d.mesh.stats.components,
        d.mesh.stats.chi,
        d.mesh.stats.volume,
        d.mesh.stats.boundary_touch,
        d.mesh.stats.hash,
        bounds.unwrap_or_else(|| "null".into()),
        d.timings.load,
        d.timings.prune,
        d.timings.voxel,
        d.timings.mesh,
        d.timings.write,
        d.timings.gpu_init,
        warnings
    );
    s
}

pub(crate) fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
