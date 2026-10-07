//! C22 判据的 CPU 腿测试（迁移自 scratch/conv-proto，读数锚 W15C-001..009）。

use conv_core::{
    GRID, euler_char, fibonacci_sphere, field_to_voxels, icosphere, mesh_hash, mesh_to_sdf_band,
    mesh_volume, naive_particle_field, sphere_sdf,
};
use fast_surface_nets::ndshape::ConstShape3u32;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};

const R: f32 = 0.75;
const H: f32 = 2.0 / (GRID as f32 - 1.0);
type Shape = ConstShape3u32<64, 64, 64>;

#[test]
fn mesh_sdf_matches_analytic_band() {
    // J2：最坏误差应恰为弦差上界（面心量法——顶点量法恒 0 是仪器错，W15C-006）
    let (verts, faces) = icosphere(2, R);
    let tess_bound = faces
        .iter()
        .map(|f| {
            let cen = [
                (verts[f[0] as usize][0] + verts[f[1] as usize][0] + verts[f[2] as usize][0]) / 3.0,
                (verts[f[0] as usize][1] + verts[f[1] as usize][1] + verts[f[2] as usize][1]) / 3.0,
                (verts[f[0] as usize][2] + verts[f[1] as usize][2] + verts[f[2] as usize][2]) / 3.0,
            ];
            sphere_sdf(cen, R).abs()
        })
        .fold(0.0f32, f32::max);
    let (sdf, max_err) = mesh_to_sdf_band(GRID, R, 3.0 * H, &verts, &faces);
    // 诊断：最坏 5 点明细
    let mut worst: Vec<(f32, u32, [f32; 3], f32)> = Vec::new();
    for (i, d) in sdf.iter().enumerate() {
        if !d.is_finite() {
            continue;
        }
        let i = i as u32;
        let (x, y, z) = (i % GRID, (i / GRID) % GRID, i / (GRID * GRID));
        let p = [x as f32 * H - 1.0, y as f32 * H - 1.0, z as f32 * H - 1.0];
        let a = sphere_sdf(p, R);
        let err = (d - a).abs();
        if err > 0.02 {
            worst.push((err, i, p, *d));
        }
    }
    worst.sort_by(|u, v| v.0.total_cmp(&u.0));
    for (err, i, p, d) in worst.iter().take(5) {
        println!(
            "WORST err={err:.5} i={i} p=({:.4},{:.4},{:.4}) a={:.5} d={d:.5}",
            p[0], p[1], p[2], d
        );
    }
    println!("J2 max_err={max_err:.5} tess_bound={tess_bound:.5}");
    assert!(max_err <= tess_bound + 1.5 * H, "max_err={max_err}");
}

#[test]
fn surface_nets_volume_genus_determinism() {
    // J3a/J3b/J3c：体积守恒 + 亏格 0 + 逐位确定性
    let sdf = field_to_voxels(GRID, R);
    let mut buf = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &Shape {}, [0; 3], [GRID - 1; 3], &mut buf);
    let world = |p: [f32; 3]| [p[0] * H - 1.0, p[1] * H - 1.0, p[2] * H - 1.0];
    let positions: Vec<[f32; 3]> = buf.positions.iter().map(|p| world(*p)).collect();
    let vol = mesh_volume(&positions, &buf.indices);
    let vol_ref = 4.0 / 3.0 * std::f32::consts::PI * R * R * R;
    let rel = ((vol - vol_ref) / vol_ref).abs();
    println!("J3a rel_err={rel:.5} tris={}", buf.indices.len() / 3);
    assert!(rel <= 0.03, "rel_err={rel}");

    let (nv, ne, nf) = euler_char(&positions, &buf.indices);
    let chi = nv as i64 - ne as i64 + nf as i64;
    println!("J3b V={nv} E={ne} F={nf} chi={chi}");
    assert_eq!(chi, 2, "genus-0 sphere expected");

    let mut buf2 = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &Shape {}, [0; 3], [GRID - 1; 3], &mut buf2);
    let h1 = mesh_hash(&positions, &buf.indices);
    let positions2: Vec<[f32; 3]> = buf2.positions.iter().map(|p| world(*p)).collect();
    let h2 = mesh_hash(&positions2, &buf2.indices);
    println!("J3c hash1={h1:016x} hash2={h2:016x}");
    assert_eq!(h1, h2, "bitwise determinism");
}

#[test]
fn particles_unsigned_field_matches_analytic() {
    // J4：naive 最近粒子基线（JFA 的 CPU 参照口径）
    let k = 4096;
    let particles = fibonacci_sphere(k, R);
    let (_, mean_err, max_err) = naive_particle_field(GRID, R, 2.0 * H, &particles);
    println!("J4 mean_err={mean_err:.5} max_err={max_err:.5}");
    assert!(mean_err <= 0.05, "mean_err={mean_err}");
}
