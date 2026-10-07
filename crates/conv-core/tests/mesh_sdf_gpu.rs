//! GPU mesh→SDF 判据：对 CPU mesh_to_sdf_band 对拍带 + 两次运行逐位确定性。
//! （符号口径：径向出射射线，凸体测试对象与 CPU 同源。）

use conv_core::{GRID, icosphere, mesh_sdf_gpu::mesh_to_sdf_gpu, mesh_to_sdf_band, sphere_sdf};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;
const H: f32 = 2.0 / (GRID as f32 - 1.0);

fn fnv(data: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

#[test]
fn gpu_mesh_sdf_matches_cpu_band() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let (cpu, _cpu_max) = mesh_to_sdf_band(GRID, R, 3.0 * H, &verts, &faces);
    let gpu = mesh_to_sdf_gpu(&conv_core::jfa::headless_device(), &verts, &faces, GRID);
    let mut max_err = 0f32;
    let mut sum = 0f64;
    let mut cnt = 0usize;
    let mut worst: Vec<(f32, usize, f32, f32)> = Vec::new();
    for (i, c) in cpu.iter().enumerate() {
        if !c.is_finite() {
            continue;
        }
        let g = gpu[i];
        if !g.is_finite() {
            continue;
        }
        let e = (g - c).abs();
        if e > 0.05 {
            let iu = i as u32;
            let x = iu % GRID;
            let y = (iu / GRID) % GRID;
            let z = iu / (GRID * GRID);
            let p = [x as f32 * H - 1.0, y as f32 * H - 1.0, z as f32 * H - 1.0];
            worst.push((e, i, sphere_sdf(p, R), *c));
        }
        max_err = max_err.max(e);
        sum += e as f64;
        cnt += 1;
    }
    worst.sort_by(|u, v| v.0.total_cmp(&u.0));
    for (e, i, a, c) in worst.iter().take(5) {
        println!("WORST err={e:.5} i={i} analytic={a:.5} cpu={c:.5} gpu=");
        // gpu 值单独取（借用的 gpu 在上面循环里）
        println!("   gpu[{}]={:.5}", i, gpu[*i]);
    }
    let mean = sum / cnt as f64;
    println!("GPU mesh→SDF mean={mean:.6} max={max_err:.5} / {cnt} 带内体素");
    // 对拍带 = 2 体素（h_world）：JFA 近似 + 样本密度误差 + 径向符号同源
    assert!(max_err <= 2.0 * H, "gpu vs cpu max_err={max_err}");
}

#[test]
fn gpu_mesh_sdf_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let hd = conv_core::jfa::headless_device();
    let a = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID);
    let b = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID);
    assert_eq!(fnv(&a), fnv(&b), "bitwise determinism across runs");
}

#[test]
fn gpu_mesh_sdf_sphere_sign_sane() {
    // 符号抽查（窄带口径）：带内球面附近内负外正——径向符号正确性的最小抽查。
    // 探针取 |距面| ≤ 3H（符号带宽 4.5 体素 > 比较带宽 3，深内部按设计保持无符号）。
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let hd = conv_core::jfa::headless_device();
    let g = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID);
    let at = |p: [f32; 3]| -> f32 {
        let n = GRID as f32;
        let x = ((p[0] + 1.0) / 2.0 * (n - 1.0)).round() as usize;
        let y = ((p[1] + 1.0) / 2.0 * (n - 1.0)).round() as usize;
        let z = ((p[2] + 1.0) / 2.0 * (n - 1.0)).round() as usize;
        g[x + y * GRID as usize + z * (GRID * GRID) as usize]
    };
    let inside = at([0.66, 0.0, 0.0]); // |p|−R ≈ −0.09（带内）
    let outside = at([0.83, 0.0, 0.0]); // |p|−R ≈ +0.08（带内）
    println!("sign probe: inside={inside:.4} outside={outside:.4}");
    assert!(inside < 0.0, "带内球内侧应为负（径向符号失准）");
    assert!(outside > 0.0, "带内球外侧应为正");
    let _ = sphere_sdf([0.0; 3], R);
}
