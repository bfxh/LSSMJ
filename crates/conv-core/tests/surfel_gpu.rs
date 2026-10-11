//! mesh→高斯判据（自研缺口④第一片）：surfel 结构（法向/尺度/占位属性）+ 采样随面积标度与
//! 逐位确定性 + 端到端水平集（mesh → surfels → splat → 球面半径读数）。

use conv_core::{
    icosphere,
    jfa::headless_device,
    kernels::{AnisoKernels, splat_field},
    surfel::{SurfelParams, mesh_to_gaussians},
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn flat_quad(half: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    // z = 0 平面正方形（绕序使法向 = +z）
    let v = vec![
        [-half, -half, 0.0],
        [half, -half, 0.0],
        [half, half, 0.0],
        [-half, half, 0.0],
    ];
    let f = vec![[0u32, 1, 2], [0, 2, 3]];
    (v, f)
}

fn fwd_rot(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let s = -q[3]; // 前向 = 逆旋转取 w 取反再取反 ⇒ 直接展开
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] - 2.0 * s * uv[0] + 2.0 * uuv[0],
        v[1] - 2.0 * s * uv[1] + 2.0 * uuv[1],
        v[2] - 2.0 * s * uv[2] + 2.0 * uuv[2],
    ]
}

#[test]
fn surfel_structure_flat_quad() {
    let (v, f) = flat_quad(0.5); // 面积 1（两三角各 0.5）
    let p = SurfelParams {
        spacing: 0.5,
        thickness: 0.05,
    };
    let c = mesh_to_gaussians(&v, &f, &p);
    // 每三角 n = ceil(0.5 / 0.25) = 2 ⇒ 共 4
    assert_eq!(c.count, 4);
    let (ln_t, ln_n) = (0.25f32.ln(), 0.05f32.ln());
    for i in 0..c.count as usize {
        let pos = c.positions[i];
        assert_eq!(pos[2], 0.0, "采样点应恰在平面 z=0 上");
        let z_axis = fwd_rot(c.rotations[i], [0.0, 0.0, 1.0]);
        assert!(z_axis[2].abs() > 0.999, "面法向未对齐 z：{z_axis:?}");
        assert_eq!(c.log_scales[i], [ln_t, ln_t, ln_n], "退化 surfel 尺度口径");
        assert_eq!(c.opacities[i], 1.0);
        assert_eq!(c.colors[i], [0.5, 0.5, 0.5]);
    }
}

#[test]
fn sampling_scales_with_area_and_deterministic() {
    let p = SurfelParams {
        spacing: 0.25,
        thickness: 0.02,
    };
    let (v1, f1) = flat_quad(0.5); // 面积 1：每三角 0.5/0.0625 = 8
    let (v2, f2) = flat_quad(1.0); // 面积 4：每三角 2.0/0.0625 = 32
    let a = mesh_to_gaussians(&v1, &f1, &p);
    let b = mesh_to_gaussians(&v2, &f2, &p);
    assert_eq!(a.count, 16);
    assert_eq!(b.count, 64);
    // 逐位确定性（两跑）
    let c = mesh_to_gaussians(&v2, &f2, &p);
    for i in 0..c.count as usize {
        for j in 0..3 {
            assert_eq!(
                b.positions[i][j].to_bits(),
                c.positions[i][j].to_bits(),
                "position 非确定"
            );
            assert_eq!(
                b.log_scales[i][j].to_bits(),
                c.log_scales[i][j].to_bits(),
                "log_scale 非确定"
            );
        }
        for j in 0..4 {
            assert_eq!(
                b.rotations[i][j].to_bits(),
                c.rotations[i][j].to_bits(),
                "rotation 非确定"
            );
        }
    }
}

#[test]
fn sphere_level_set_end_to_end() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let r = 0.75f32;
    let (verts, faces) = icosphere(2, r);
    let p = SurfelParams {
        spacing: 0.2,
        thickness: 0.08,
    };
    let cloud = mesh_to_gaussians(&verts, &faces, &p);
    println!("surfel 采样 {} 点（{} 面）", cloud.count, faces.len());
    let scales: Vec<[f32; 3]> = cloud.log_scales.iter().map(|s| s.map(f32::exp)).collect();
    let k = AnisoKernels::new(cloud.positions.clone(), cloud.rotations.clone(), scales);
    let n = 32u32;
    let f = splat_field(&hd, &k, n, None);
    let h = 2.0 / (n as f32 - 1.0);
    // 沿 ±x/±y/±z 在 y=z=15 行上找 field ≥ 0.5 的最远半径
    let at = |i: u32, j: u32, kk: u32| f[(i + j * n + kk * n * n) as usize];
    let mut radii = Vec::new();
    for axis in 0..3usize {
        let mut r_neg = f32::INFINITY;
        let mut r_pos = f32::NEG_INFINITY;
        for i in 0..n {
            let val = match axis {
                0 => at(i, 15, 15),
                1 => at(15, i, 15),
                _ => at(15, 15, i),
            };
            if val >= 0.5 {
                let w = i as f32 * h - 1.0;
                r_neg = r_neg.min(w);
                r_pos = r_pos.max(w);
            }
        }
        radii.push(r_neg.abs());
        radii.push(r_pos.abs());
    }
    let r_min = radii.iter().cloned().fold(f32::INFINITY, f32::min);
    let r_max = radii.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    println!("水平集半径（≥0.5）：6 向 = {radii:?}，min={r_min:.4} max={r_max:.4}（球 R={r}）");
    assert!(r_min.is_finite() && r_max.is_finite(), "水平集为空");
    // 先量后钉（定点累积 ⇒ 读数稳定）：六向均 0.87097（外扩 ≈1.5σ_n，场衰减预期；离散 ~1e-6）
    assert!((r_min - r).abs() <= 0.15, "最小半径偏离过大：{r_min}");
    assert!((r_max - r).abs() <= 0.15, "最大半径偏离过大：{r_max}");
    assert!(r_max - r_min < 0.01, "六向半径离散过大：{r_max} - {r_min}");
}
