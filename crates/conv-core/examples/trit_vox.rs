//! f32 density vs 2-bit 三元：二维扫描（f32 阈值 × 三元核截断），找能否对上同一张表面。

use conv_core::gauss::ply::{PlyReadOptions, parse_ply_with};
use conv_core::gauss::voxel::{VoxelizeOptions, voxelize, voxelize_ternary_with};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        eprintln!("用法: trit_vox <input.ply> [grid]");
        std::process::exit(2);
    });
    let grid: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let threads = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(8);

    let bytes = std::fs::read(&path).expect("读 PLY 失败");
    let ply = parse_ply_with(&bytes, &PlyReadOptions::default()).expect("解析 PLY 失败");
    let gs = ply.gaussians;
    let opts = VoxelizeOptions {
        n: grid,
        threads,
        ..VoxelizeOptions::default()
    };

    let t0 = Instant::now();
    let field = voxelize(&gs, &opts).expect("f32");
    let t_f32 = t0.elapsed();
    let nv = field.grid.n as usize;
    let n = nv * nv * nv;
    let idx = |x: usize, y: usize, z: usize| z * nv * nv + y * nv + x;
    println!(
        "{} grid={} gauss={} | f32 {:.1} ms auto_iso={:.3}",
        std::path::Path::new(&path)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(&path),
        grid,
        gs.len(),
        t_f32.as_secs_f64() * 1e3,
        field.iso
    );

    let cuts = [18.0f32, 9.0, 4.0, 2.25, 1.0, 0.5];
    let mut trits = Vec::new();
    for &mc in &cuts {
        let t = Instant::now();
        let tri = voxelize_ternary_with(&gs, &opts, mc).expect("ternary");
        eprintln!(
            "m_cut={mc:.2} trit {:.1} ms occ={}",
            t.elapsed().as_secs_f64() * 1e3,
            tri.occupied
        );
        trits.push(tri);
    }

    let isos = [1.0f32, 2.0, 5.0, 10.0, 20.0, field.iso];
    print!("{:>8} {:>8}", "iso", "surf");
    for &mc in &cuts {
        print!("{:>9.2}", mc);
    }
    println!();
    for &iso in &isos {
        // f32 等值面（6 邻域跨阈值）
        let mut surf = vec![false; n];
        for z in 0..nv {
            for y in 0..nv {
                for x in 0..nv {
                    let i = idx(x, y, z);
                    let v = field.values[i];
                    let mut cross = false;
                    {
                        let mut probe = |xx: usize, yy: usize, zz: usize| {
                            if (v > iso) != (field.values[idx(xx, yy, zz)] > iso) {
                                cross = true;
                            }
                        };
                        if x > 0 {
                            probe(x - 1, y, z);
                        }
                        if x + 1 < nv {
                            probe(x + 1, y, z);
                        }
                        if y > 0 {
                            probe(x, y - 1, z);
                        }
                        if y + 1 < nv {
                            probe(x, y + 1, z);
                        }
                        if z > 0 {
                            probe(x, y, z - 1);
                        }
                        if z + 1 < nv {
                            probe(x, y, z + 1);
                        }
                    }
                    surf[i] = cross;
                }
            }
        }
        let occ_surf = surf.iter().filter(|b| **b).count();
        print!("{:>8.2} {:>8}", iso, occ_surf);
        for tri in &trits {
            let mut inter = 0usize;
            let mut occ = 0usize;
            for i in 0..n {
                let t = (tri.words[i / 16] >> ((i % 16) * 2)) & 0b11;
                if t >= 1 {
                    occ += 1;
                    if surf[i] {
                        inter += 1;
                    }
                }
            }
            let union = occ + occ_surf - inter;
            print!("{:>9.3}", inter as f64 / union.max(1) as f64);
        }
        println!();
    }
}
