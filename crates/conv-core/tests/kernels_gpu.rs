//! 各向异性核 splat 判据（T-GC-03 第一片）：CPU 参照对拍（容差按定点量化读数钉）+
//! 逐位确定性 + 薄片质量判据（各向异性 vs 各向同性基线）+ 单核中心恰点读数。

use conv_core::{
    Lcg,
    jfa::headless_device,
    kernels::{AnisoKernels, FIXED_POINT_SCALE, pca_kernels, splat_field},
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const CUTOFF2: f32 = 9.0;

fn rot_inv(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let s = q[3];
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

/// CPU 参照（与被测同公式的暴力评估；浮点求和，容差含定点量子）。
fn cpu_reference(k: &AnisoKernels, n: u32) -> Vec<f32> {
    let h = 2.0 / (n as f32 - 1.0);
    let nn = n as i32;
    let mut field = vec![0f32; (n * n * n) as usize];
    let max_s = k
        .scales
        .iter()
        .flat_map(|s| s.iter())
        .fold(0f32, |a, &b| a.max(b));
    let r = ((3.0 * max_s) / h).ceil() as i32 + 1;
    for p in 0..k.centers.len() {
        let c = k.centers[p];
        let q = k.rotations[p];
        let s = k.scales[p];
        let ci = [
            ((c[0] + 1.0) / h).floor() as i32,
            ((c[1] + 1.0) / h).floor() as i32,
            ((c[2] + 1.0) / h).floor() as i32,
        ];
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    let v = [ci[0] + dx, ci[1] + dy, ci[2] + dz];
                    if v.iter().any(|&x| x < 0 || x >= nn) {
                        continue;
                    }
                    let vw = [
                        v[0] as f32 * h - 1.0,
                        v[1] as f32 * h - 1.0,
                        v[2] as f32 * h - 1.0,
                    ];
                    let u = rot_inv(q, [vw[0] - c[0], vw[1] - c[1], vw[2] - c[2]]);
                    let m = [u[0] / s[0], u[1] / s[1], u[2] / s[2]];
                    let nd2 = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
                    if nd2 > CUTOFF2 {
                        continue;
                    }
                    let l = (v[0] + v[1] * nn + v[2] * nn * nn) as usize;
                    field[l] += (-0.5 * nd2).exp();
                }
            }
        }
    }
    field
}

fn norm4(q: [f32; 4]) -> [f32; 4] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}

#[test]
fn splat_matches_cpu_reference() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let mut rng = Lcg::new(0x6b33);
    let count = 256usize;
    let mut centers = Vec::with_capacity(count);
    let mut rotations = Vec::with_capacity(count);
    let mut scales = Vec::with_capacity(count);
    for _ in 0..count {
        centers.push([
            rng.next01() * 1.4 - 0.7,
            rng.next01() * 1.4 - 0.7,
            rng.next01() * 1.4 - 0.7,
        ]);
        rotations.push(norm4([
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
            rng.next01() * 2.0 - 1.0,
        ]));
        let s = |r: &mut Lcg| 0.03 + r.next01() * 0.09;
        scales.push([s(&mut rng), s(&mut rng), s(&mut rng)]);
    }
    let k = AnisoKernels::new(centers, rotations, scales);
    let gpu = splat_field(&hd, &k, n, None);
    let cpu = cpu_reference(&k, n);
    let mut max_d = 0f32;
    for (g, c) in gpu.iter().zip(&cpu) {
        max_d = max_d.max((g - c).abs());
    }
    println!(
        "splat cpu-ref max_err={max_d:.3e}（定点量子={:.3e}）",
        1.0 / FIXED_POINT_SCALE
    );
    // 先量后钉：实测 4.9e-5（≈3.2 量子：累积量化 + exp 尾差）；上限 = 实测 ×2
    assert!(max_d <= 1e-4, "对拍超容忍：{max_d}");
}

#[test]
fn splat_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let mut rng = Lcg::new(0x6b34);
    let count = 512usize;
    let mut centers = Vec::with_capacity(count);
    for _ in 0..count {
        centers.push([
            rng.next01() * 1.4 - 0.7,
            rng.next01() * 1.4 - 0.7,
            rng.next01() * 1.4 - 0.7,
        ]);
    }
    let k = AnisoKernels::isotropic(centers, 0.08);
    let a = splat_field(&hd, &k, n, None);
    let b = splat_field(&hd, &k, n, None);
    let diff = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count();
    assert_eq!(diff, 0, "splat 非逐位确定（差异 {diff} 项）");
}

#[test]
fn single_kernel_center_exact() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    // 核中心恰在体素 (16,16,16) 的世界坐标上 ⇒ 该体素权重恰为 1（Q16=65536）
    let cw = 16.0 * h - 1.0;
    let k = AnisoKernels::isotropic(vec![[cw, cw, cw]], 0.1);
    let f = splat_field(&hd, &k, n, None);
    let center = f[(16 + 16 * n + 16 * n * n) as usize];
    assert_eq!(center, 1.0, "单核中心读数应恰为 1.0（见 {center}）");
}

#[test]
fn thin_sheet_anisotropy_preserves_thickness() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    // 16×16 粒子铺在 z=0 平面（间距 0.08 世界单位）
    let mut centers = Vec::with_capacity(256);
    for iy in 0..16 {
        for ix in 0..16 {
            centers.push([-0.6 + ix as f32 * 0.08, -0.6 + iy as f32 * 0.08, 0.0]);
        }
    }
    let (wide, thin) = (0.1f32, 0.02f32);
    let aniso = AnisoKernels::new(
        centers.clone(),
        vec![[0.0, 0.0, 0.0, 1.0]; 256],
        vec![[wide, wide, thin]; 256],
    );
    let iso = AnisoKernels::isotropic(centers, wide);
    let fa = splat_field(&hd, &aniso, n, None);
    let fi = splat_field(&hd, &iso, n, None);
    let thickness = |f: &[f32]| -> usize {
        (0..n)
            .filter(|&z| f[(16 + 16 * n + z * n * n) as usize] >= 0.5)
            .count()
    };
    let ta = thickness(&fa);
    let ti = thickness(&fi);
    println!("薄片厚度（≥0.5）：各向异性 {ta} 层 / 各向同性 {ti} 层（h={h:.4}）");
    // 先量后钉（定点累积 ⇒ 逐位确定，读数稳定）：实测 2 vs 8
    assert!(ta < ti, "各向异性未保住薄片：{ta} vs {ti}");
    assert!(ta <= 2, "各向异性厚度超实测基线：{ta} 层（实测 2）");
    assert!(ti >= 4, "各向同性基线异常变好：{ti} 层（实测 8）");
}

// ---- 邻域 PCA 自动定向（T-GC-03 第二片）----

fn fwd_rot(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    // 前向旋转 = 逆旋转取 w 取反
    rot_inv([q[0], q[1], q[2], -q[3]], v)
}

#[test]
fn pca_orientation_aligns_normal() {
    let _gpu = GPU_LOCK.lock().unwrap();
    // 斜置平板：法向 (1,1,1)/√3，面上 16×16 粒子 + 面内抖动
    let inv = 1.0 / 3.0f32.sqrt();
    let n = [inv, inv, inv];
    let u = [1.0 / 2.0f32.sqrt(), -1.0 / 2.0f32.sqrt(), 0.0];
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    let mut rng = Lcg::new(0x9c3a);
    let mut centers = Vec::with_capacity(256);
    for iy in 0..16 {
        for ix in 0..16 {
            let (a, b) = (ix as f32 * 0.08 - 0.6, iy as f32 * 0.08 - 0.6);
            let ja = (rng.next01() - 0.5) * 0.008;
            let jb = (rng.next01() - 0.5) * 0.008;
            let jn = (rng.next01() - 0.5) * 0.001;
            centers.push([
                u[0] * (a + ja) + v[0] * (b + jb) + n[0] * jn,
                u[1] * (a + ja) + v[1] * (b + jb) + n[1] * jn,
                u[2] * (a + ja) + v[2] * (b + jb) + n[2] * jn,
            ]);
        }
    }
    let k = pca_kernels(&centers, 16, 4.0, 0.02);
    // 最薄轴 = 核 X 轴（特征值升序）——前向旋转单位 X 轴应平行于法向
    let mut worst = 0f32;
    for q in &k.rotations {
        let e0 = fwd_rot(*q, [1.0, 0.0, 0.0]);
        let d = (e0[0] * n[0] + e0[1] * n[1] + e0[2] * n[2]).abs();
        worst = worst.max(1.0 - d);
    }
    println!("PCA 轴向与法向最大偏差 1-|dot| = {worst:.4}（阈值先量后钉）");
    // 先量后钉：实测 1-|dot| ≈ 0.0000（逐位确定，读数稳定）
    assert!(worst <= 0.001, "最薄轴未对齐法向：1-|dot| = {worst}");
}

#[test]
fn pca_thin_sheet_auto_oriented() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 32u32;
    let mut rng = Lcg::new(0x9c3b);
    let mut centers = Vec::with_capacity(256);
    for iy in 0..16 {
        for ix in 0..16 {
            let jz = (rng.next01() - 0.5) * 0.004;
            centers.push([ix as f32 * 0.08 - 0.6, iy as f32 * 0.08 - 0.6, jz]);
        }
    }
    // 端到端：粒子 →（PCA 自动）→ 核 → splat
    let k = pca_kernels(&centers, 16, 4.0, 0.02);
    let f = splat_field(&hd, &k, n, None);
    let iso = AnisoKernels::isotropic(centers.clone(), 0.1);
    let fi = splat_field(&hd, &iso, n, None);
    let thickness = |f: &[f32]| -> usize {
        (0..n)
            .filter(|&z| f[(16 + 16 * n + z * n * n) as usize] >= 0.5)
            .count()
    };
    let (ta, ti) = (thickness(&f), thickness(&fi));
    println!("PCA 自动定向薄片厚度（≥0.5）：{ta} 层（各向同性基线 {ti} 层）");
    // 先量后钉（定点累积 ⇒ 读数稳定）：实测 2 vs 8——自动定向与手调核持平
    assert!(ta < ti, "PCA 定向未保住薄片：{ta} vs {ti}");
    assert!(ta == 2, "PCA 定向厚度偏离实测基线：{ta} 层（实测 2）");
    assert!(ti >= 4, "各向同性基线异常变好：{ti} 层（实测 8）");
}

#[test]
fn pca_deterministic_bitwise() {
    let mut rng = Lcg::new(0x9c3c);
    let mut centers = Vec::with_capacity(128);
    for _ in 0..128 {
        centers.push([
            rng.next01() * 1.6 - 0.8,
            rng.next01() * 1.6 - 0.8,
            rng.next01() * 1.6 - 0.8,
        ]);
    }
    let a = pca_kernels(&centers, 12, 3.0, 0.01);
    let b = pca_kernels(&centers, 12, 3.0, 0.01);
    for (x, y) in a.rotations.iter().zip(&b.rotations) {
        for c in 0..4 {
            assert_eq!(x[c].to_bits(), y[c].to_bits(), "rotations 非确定");
        }
    }
    for (x, y) in a.scales.iter().zip(&b.scales) {
        for c in 0..3 {
            assert_eq!(x[c].to_bits(), y[c].to_bits(), "scales 非确定");
        }
    }
}

#[test]
fn pca_degenerate_coincident_points() {
    // 共点退化：协方差为零 ⇒ 主轴=单位阵、缩放=floor、全部有限（无 NaN）
    let centers = vec![[0.3, -0.2, 0.5]; 8];
    let k = pca_kernels(&centers, 4, 2.0, 0.015);
    for q in &k.rotations {
        assert!(q.iter().all(|v| v.is_finite()));
    }
    for s in &k.scales {
        for &v in s {
            assert_eq!(v, 0.015, "退化点缩放应为 floor");
        }
    }
}
