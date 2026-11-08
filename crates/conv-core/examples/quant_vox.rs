//! f32 vs u8 定点密度：能否用 1 字节拿到同一张等值面。

use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::ply::{PlyReadOptions, parse_ply_with};
use conv_core::gauss::voxel::{VoxelizeOptions, scale_for_iso, voxelize, voxelize_quant};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("用法: quant_vox <in.ply> [grid]");
        std::process::exit(2);
    });
    let grid: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let threads = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(8);
    let bytes = std::fs::read(&path).expect("读 PLY");
    let gs = parse_ply_with(&bytes, &PlyReadOptions::default())
        .expect("解析")
        .gaussians;
    let opts = VoxelizeOptions {
        n: grid,
        threads,
        ..VoxelizeOptions::default()
    };

    let t0 = Instant::now();
    let f = voxelize(&gs, &opts).expect("f32");
    let t_f32 = t0.elapsed();
    let iso = f.iso;
    let nv = f.grid.n as usize;
    let n = nv * nv * nv;
    let idx = |x: usize, y: usize, z: usize| z * nv * nv + y * nv + x;
    let surface = |vals: &dyn Fn(usize) -> f32, iso: f32| -> Vec<bool> {
        let mut s = vec![false; n];
        for z in 0..nv {
            for y in 0..nv {
                for x in 0..nv {
                    let i = idx(x, y, z);
                    let v = vals(i);
                    let mut c = false;
                    {
                        let mut p = |xx: usize, yy: usize, zz: usize| {
                            if (v > iso) != (vals(idx(xx, yy, zz)) > iso) {
                                c = true;
                            }
                        };
                        if x > 0 {
                            p(x - 1, y, z);
                        }
                        if x + 1 < nv {
                            p(x + 1, y, z);
                        }
                        if y > 0 {
                            p(x, y - 1, z);
                        }
                        if y + 1 < nv {
                            p(x, y + 1, z);
                        }
                        if z > 0 {
                            p(x, y, z - 1);
                        }
                        if z + 1 < nv {
                            p(x, y, z + 1);
                        }
                    }
                    s[i] = c;
                }
            }
        }
        s
    };
    let sf = surface(&|i| f.values[i], iso);
    let occ_f = sf.iter().filter(|b| **b).count();
    println!(
        "{} grid={} gauss={} | f32 {:.1} ms iso={:.3} surf={} mem={:.1}MB",
        std::path::Path::new(&path)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(&path),
        grid,
        gs.len(),
        t_f32.as_secs_f64() * 1e3,
        iso,
        occ_f,
        (n * 4) as f64 / 1e6
    );
    // 端到端：两边都提网格，比顶点/面数/体积
    let mesh_f = field_to_mesh(&f);
    println!(
        "f32  mesh: v={} t={} vol={:.4} observed={} sealed={}",
        mesh_f.positions.len(),
        mesh_f.stats.triangles,
        mesh_f.stats.volume,
        f.observed,
        f.sealed_interior
    );
    println!(
        "{:>5} {:>8} {:>10} {:>8} {:>7} {:>7} {:>9}",
        "kind", "ms", "surf", "IoU", "verts", "tris", "vol"
    );

    // u8：scale 由 iso 反推；u16：quantize_field_auto（探针定标，不饱和）
    let opts_iso = VoxelizeOptions {
        iso_abs: Some(iso),
        ..opts.clone()
    };
    for &(tag, scale) in &[("u8", scale_for_iso(iso)), ("u16", 0.0f32)] {
        let t = Instant::now();
        let qf: conv_core::gauss::voxel::Field = if tag == "u16" {
            conv_core::gauss::voxel::quantize_field_auto(&gs, &opts_iso)
                .expect("u16")
                .0
        } else {
            voxelize_quant::<u8>(&gs, &opts, scale)
                .expect("u8")
                .to_field(iso, opts.seal)
        };
        let ms = t.elapsed().as_secs_f64() * 1e3;
        // 纯 splat 耗时（不含 to_field / 网格），用于 SIMD A/B
        let t2 = Instant::now();
        if tag == "u16" {
            let _ = voxelize_quant::<u16>(&gs, &opts, 1000.0).expect("u16 splat");
        } else {
            let _ = voxelize_quant::<u8>(&gs, &opts, scale).expect("u8 splat");
        }
        let splat_ms = t2.elapsed().as_secs_f64() * 1e3;
        eprintln!("  {tag} splat_only={splat_ms:.1}ms");
        let sq = surface(&|i| qf.values[i], iso);
        let occ_q = sq.iter().filter(|b| **b).count();
        let mut inter = 0usize;
        for i in 0..n {
            if sq[i] && sf[i] {
                inter += 1;
            }
        }
        let union = occ_f + occ_q - inter;
        let mq = field_to_mesh(&qf);
        println!(
            "{:>5} {:>8.1} {:>10} {:>8.3} {:>7} {:>7} {:>9.4} obs={}",
            tag,
            ms,
            occ_q,
            inter as f64 / union.max(1) as f64,
            mq.positions.len(),
            mq.stats.triangles,
            mq.stats.volume,
            qf.observed
        );
    }
}
