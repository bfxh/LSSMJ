//! GPU mesh→SDF 判据：对 CPU mesh_to_sdf_band 对拍带 + 两次运行逐位确定性。
//! 符号两档（T-GC-01）：Radial=径向出射射线（凸体口径，与 CPU radial_sign 同源）、
//! Winding=广义绕数（精度档，与 CPU winding_number 同源）。

use conv_core::{
    GRID, icosphere, mesh_sdf_gpu::SignMode, mesh_sdf_gpu::mesh_to_sdf_gpu, mesh_to_sdf_band,
    mesh_to_sdf_band_winding, sphere_sdf,
};
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

/// 带内逐点对拍读数（mean/max，跳过 INF 带外槽）+ 最坏 5 点明细。
fn band_err(cpu: &[f32], gpu: &[f32]) -> (f64, f32) {
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
        println!(
            "WORST err={e:.5} i={i} analytic={a:.5} cpu={c:.5} gpu={:.5}",
            gpu[*i]
        );
    }
    (sum / cnt as f64, max_err)
}

#[test]
fn gpu_mesh_sdf_matches_cpu_band() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let (cpu, _cpu_max) = mesh_to_sdf_band(GRID, R, 3.0 * H, &verts, &faces);
    let gpu = mesh_to_sdf_gpu(
        &conv_core::jfa::headless_device(),
        &verts,
        &faces,
        GRID,
        SignMode::Radial,
        None,
    );
    let (mean, max_err) = band_err(&cpu, &gpu);
    println!("GPU mesh→SDF（实时档）mean={mean:.6} max={max_err:.5}");
    // 对拍带 = 2 体素（h_world）：JFA 近似 + 样本密度误差 + 径向符号同源
    assert!(max_err <= 2.0 * H, "gpu vs cpu max_err={max_err}");
}

#[test]
fn gpu_mesh_sdf_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let hd = conv_core::jfa::headless_device();
    let a = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Radial, None);
    let b = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Radial, None);
    assert_eq!(fnv(&a), fnv(&b), "bitwise determinism across runs");
}

#[test]
fn gpu_mesh_sdf_sphere_sign_sane() {
    // 符号抽查（窄带口径）：带内球面附近内负外正——径向符号正确性的最小抽查。
    // 探针取 |距面| ≤ 3H（符号带宽 4.5 体素 > 比较带宽 3，深内部按设计保持无符号）。
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let hd = conv_core::jfa::headless_device();
    let g = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Radial, None);
    let at = probe_at(&g);
    let inside = at([0.66, 0.0, 0.0]); // |p|−R ≈ −0.09（带内）
    let outside = at([0.83, 0.0, 0.0]); // |p|−R ≈ +0.08（带内）
    println!("sign probe（实时档）: inside={inside:.4} outside={outside:.4}");
    assert!(inside < 0.0, "带内球内侧应为负（径向符号失准）");
    assert!(outside > 0.0, "带内球外侧应为正");
}

#[test]
fn gpu_mesh_sdf_winding_matches_cpu_band() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let (cpu, _cpu_max) = mesh_to_sdf_band_winding(GRID, R, 3.0 * H, &verts, &faces);
    let gpu = mesh_to_sdf_gpu(
        &conv_core::jfa::headless_device(),
        &verts,
        &faces,
        GRID,
        SignMode::Winding,
        None,
    );
    let (mean, max_err) = band_err(&cpu, &gpu);
    println!("GPU mesh→SDF（精度档/绕数）mean={mean:.6} max={max_err:.5}");
    // 同一距离腿 ⇒ 对拍带沿用 2 体素；CPU↔GPU 的 atan2 只保证同序求和（跨端逐位不主张）
    assert!(max_err <= 2.0 * H, "gpu vs cpu max_err={max_err}");
}

#[test]
fn gpu_mesh_sdf_winding_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = icosphere(2, R);
    let hd = conv_core::jfa::headless_device();
    let a = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Winding, None);
    let b = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Winding, None);
    assert_eq!(fnv(&a), fnv(&b), "bitwise determinism across runs");
}

/// 两分量网格（精度档主判据场景）：icosphere(sub, r) 心移到 (±cx, 0, 0) 后拼接。
fn two_spheres(sub: u32, r: f32, cx: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let (v, f) = icosphere(sub, r);
    let off = v.len() as u32;
    let shift = |p: [f32; 3], s: f32| [p[0] + s, p[1], p[2]];
    let mut verts: Vec<[f32; 3]> = v.iter().map(|p| shift(*p, -cx)).collect();
    verts.extend(v.iter().map(|p| shift(*p, cx)));
    let mut faces = f.clone();
    faces.extend(f.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    (verts, faces)
}

#[test]
fn gpu_mesh_sdf_winding_fixes_multi_component() {
    // 精度档存在的理由搬到 GPU 上验：两个分离球（原点不在实体内）之间的凹湾，
    // 实时档（径向出射）整片判成内部，精度档（绕数）判外部。
    // 末条断言钉的是"实时档在此处确实判错"的错例证据，不是认可其行为。
    let _gpu = GPU_LOCK.lock().unwrap();
    let (verts, faces) = two_spheres(2, 0.35, 0.5); // x ∈ [−0.85,−0.15] ∪ [0.15,0.85]
    let hd = conv_core::jfa::headless_device();
    let w = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Winding, None);
    let r = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Radial, None);
    let at_w = probe_at(&w);
    let at_r = probe_at(&r);

    let bay = [0.11f32, 0.0, 0.0]; // 凹湾（实体外，距右球面 ≈0.04 ⇒ 带内）
    let deep = [0.20f32, 0.0, 0.0]; // 右球内（距面 ≈0.05 ⇒ 带内）
    let (bw, br) = (at_w(bay), at_r(bay));
    let (dw, dr) = (at_w(deep), at_r(deep));
    println!(
        "多分量（两分离球）：凹湾 绕数={bw:.4} / 径向={br:.4}；球内 绕数={dw:.4} / 径向={dr:.4}"
    );
    assert!(bw > 0.0, "精度档把凹湾判成了内（绕数失准）");
    assert!(dw < 0.0, "精度档把球内判成了外");
    assert!(dr < 0.0, "实时档球内应判负（口径变了需复核）");
    assert!(
        br < 0.0,
        "错例证据失效：实时档凹湾不再判错（radial 行为变了，需复核口径）"
    );
}

/// 世界坐标 → 最近体素取值（[-1,1]³ 上的 GRID³ 场）。
fn probe_at(field: &[f32]) -> impl Fn([f32; 3]) -> f32 + '_ {
    move |p: [f32; 3]| {
        let idx = |c: f32| ((c + 1.0) / 2.0 * (GRID as f32 - 1.0)).round() as usize;
        field[idx(p[0]) + idx(p[1]) * GRID as usize + idx(p[2]) * (GRID * GRID) as usize]
    }
}

/// 盒场读数：(比较体素数, 假内, 未投符号空洞, 幅值最坏)。
/// 比较带 = 精确点-三角距离 ≤ 2H；符号参照 = 解析盒（L∞ 在棱/角处不是欧氏距离，故只用于内外）。
fn box_readings(
    g: &[f32],
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
    half: f32,
) -> (usize, usize, usize, f32) {
    let mut probes = 0usize;
    let mut false_inside = 0usize;
    let mut unsigned_hole = 0usize;
    let mut mag_max = 0f32;
    for (i, &d) in g.iter().enumerate() {
        let iu = i as u32;
        let (x, y, z) = (iu % GRID, (iu / GRID) % GRID, iu / (GRID * GRID));
        let p = [x as f32 * H - 1.0, y as f32 * H - 1.0, z as f32 * H - 1.0];
        let mut exact = f32::MAX;
        for t in faces {
            let dd = conv_core::point_tri_dist(
                p,
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            if dd < exact {
                exact = dd;
            }
        }
        if exact > 2.0 * H {
            continue;
        }
        let inside = conv_core::box_sdf(p, half) < 0.0;
        probes += 1;
        match (d < 0.0, inside) {
            (true, false) => false_inside += 1,
            (false, true) => unsigned_hole += 1,
            _ => {}
        }
        mag_max = mag_max.max((d.abs() - exact).abs());
    }
    (probes, false_inside, unsigned_hole, mag_max)
}

/// 每面 k×k 网格的盒（k=1 即 12 三角的粗盒）：u×v = 外法向，故 6 面绕序一致朝外。
fn box_grid(half: f32, k: u32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let defs: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([-1.0, -1.0, 1.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]), // +z
        ([-1.0, 1.0, -1.0], [2.0, 0.0, 0.0], [0.0, -2.0, 0.0]), // −z
        ([1.0, -1.0, -1.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]), // +x
        ([-1.0, -1.0, -1.0], [0.0, 0.0, 2.0], [0.0, 2.0, 0.0]), // −x
        ([-1.0, 1.0, -1.0], [0.0, 0.0, 2.0], [2.0, 0.0, 0.0]), // +y
        ([-1.0, -1.0, -1.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]), // −y
    ];
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    let w = k + 1;
    for (o, u, v) in defs {
        let base = verts.len() as u32;
        for jj in 0..=k {
            for ii in 0..=k {
                let s = ii as f32 / k as f32;
                let t = jj as f32 / k as f32;
                verts.push([
                    (o[0] + u[0] * s + v[0] * t) * half,
                    (o[1] + u[1] * s + v[1] * t) * half,
                    (o[2] + u[2] * s + v[2] * t) * half,
                ]);
            }
        }
        for jj in 0..k {
            for ii in 0..k {
                let a = base + jj * w + ii;
                tris.push([a, a + 1, a + w + 1]);
                tris.push([a, a + w + 1, a + w]);
            }
        }
    }
    (verts, tris)
}

#[test]
fn gpu_mesh_sdf_winding_box_grid_matches_analytic() {
    // 验收口径"球/盒解析对拍"的 GPU 侧（每面 8×8 网格，768 三角）：盒是精确三角化 ⇒ 无弦差，
    // 距离腿误差只来自样本间距 + JFA 近似；绕数逐体素给精确内外 ⇒ 假内与空洞都应为 0。
    let _gpu = GPU_LOCK.lock().unwrap();
    let half = 0.5f32;
    let (verts, faces) = box_grid(half, 8);
    let hd = conv_core::jfa::headless_device();
    let g = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Winding, None);
    let (probes, false_inside, hole, mag) = box_readings(&g, &verts, &faces, half);
    println!(
        "GPU 精度档（盒 8×8/面，{} 三角）：比较 {probes} 体素，假内 {false_inside}，空洞 {hole}，幅值最坏 {mag:.5}",
        faces.len()
    );
    assert_eq!(
        false_inside, 0,
        "GPU 精度档在盒上把外部判成了内部（绕数失准）"
    );
    assert_eq!(
        hole, 0,
        "GPU 精度档在盒内侧窄带漏投符号（绕数带宽/样本密度失配）"
    );
    assert!(mag <= 2.0 * H, "幅值最坏 {mag} > 2 体素");
}

#[test]
fn gpu_mesh_sdf_winding_coarse_box_keeps_sign_correct() {
    // 同一场景的粗盒（12 个大三角）实测：绕数符号零假内，但**距离腿**在棱/角内侧出现未投符号空洞
    // ——每三角样本数上限 64 让大三角的样本间距涨到 ~4.9 体素 > 符号带宽 4.5。
    // 这是实时档采样密度（样本压缩/大三角拆分属后续片）的限制，不是精度档符号的缺陷；空洞数只记档不钉死。
    let _gpu = GPU_LOCK.lock().unwrap();
    let half = 0.5f32;
    let (verts, faces) = conv_core::box_mesh(half);
    let hd = conv_core::jfa::headless_device();
    let g = mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, SignMode::Winding, None);
    let (probes, false_inside, hole, mag) = box_readings(&g, &verts, &faces, half);
    println!(
        "GPU 精度档（粗盒 12 三角）：比较 {probes} 体素，假内 {false_inside}，空洞 {hole}（样本上限所致），幅值最坏 {mag:.5}"
    );
    assert_eq!(
        false_inside, 0,
        "GPU 精度档在粗盒上把外部判成了内部（绕数失准）"
    );
}
