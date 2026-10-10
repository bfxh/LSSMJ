//! 鲁棒性判据（v5 F12 收窄片）：空种子 / 重合种子 / 退化三角形 / 边界接触 /
//! n=2 最小网格 / GSN 重复面与边流形。
//!
//! 口径（v5 F12）：未支持输入不被宣称通过——空种子在 API 边界显式拒绝；
//! 其余场景给出确定性行为判据（逐位 / 流形 / 符号正确），而非只要求"不炸"。

use std::collections::HashMap;
use std::sync::Mutex;

use common::directed_quads;
use conv_core::{
    field_to_voxels, gsn::surface_nets_gpu, icosphere, jfa::headless_device,
    jfa::jfa_distance_field, mesh_sdf_gpu::SignMode, mesh_sdf_gpu::mesh_to_sdf_gpu,
    mesh_to_sdf_band_winding,
};

mod common;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn fnv_f32(data: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

// ---- 空种子：API 边界显式拒绝（不静默返回全 INF 假绿）----

#[test]
#[should_panic(expected = "种子表为空")]
fn jfa_empty_seeds_rejected() {
    let _gpu = gpu_lock();
    let hd = headless_device();
    let _ = jfa_distance_field(&hd, &[], 8, None);
}

// ---- 重合种子：与单种子场逐位一致（标签平局不影响距离）----

#[test]
fn jfa_coincident_seeds_match_single_bitwise() {
    let _gpu = gpu_lock();
    let hd = headless_device();
    let n = 16u32;
    let s = [5.0f32, 7.0, 9.0];
    let single = jfa_distance_field(&hd, &[s], n, None);
    let coincident = jfa_distance_field(&hd, &[s, s, s, s], n, None);
    assert_eq!(
        fnv_f32(&single),
        fnv_f32(&coincident),
        "重合种子场与单种子场不一致（标签平局泄漏进了距离）"
    );
}

// ---- n=2 最小网格：JFA / GSN 都要有确定性行为 ----

#[test]
fn jfa_n2_minimal_grid_deterministic() {
    let _gpu = gpu_lock();
    let hd = headless_device();
    let seeds = [[0.0f32, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let a = jfa_distance_field(&hd, &seeds, 2, None);
    let b = jfa_distance_field(&hd, &seeds, 2, None);
    assert_eq!(fnv_f32(&a), fnv_f32(&b), "n=2 两次运行应逐位一致");
    // 8 格逐格对暴力参照（种子在 index 角点，距离是精确可比的）
    for i in 0u32..8 {
        let p = [(i % 2) as f32, ((i / 2) % 2) as f32, (i / 4) as f32];
        let d0 = ((p[0] - 0.0).powi(2) + (p[1] - 0.0).powi(2) + (p[2] - 0.0).powi(2)).sqrt();
        let d1 = ((p[0] - 1.0).powi(2) + (p[1] - 1.0).powi(2) + (p[2] - 1.0).powi(2)).sqrt();
        let best = d0.min(d1);
        let e = (a[i as usize] - best).abs();
        assert!(
            e <= 1e-5,
            "n=2 cell {i}: jfa={} brute={best}",
            a[i as usize]
        );
    }
}

#[test]
fn gsn_n2_minimal_grid_deterministic_and_empty() {
    let _gpu = gpu_lock();
    // R=0.75 的球在 2³ 角点域上处处为正 ⇒ 无表面 ⇒ 空输出（有效而非崩溃）
    let sdf = field_to_voxels(2, 0.75);
    let hd = headless_device();
    let a = surface_nets_gpu(&hd, &sdf, 2, None);
    let b = surface_nets_gpu(&hd, &sdf, 2, None);
    assert_eq!(a.quad_count, 0, "n=2 全正场应得空网格");
    assert_eq!(a.indices, b.indices, "n=2 索引两次运行应一致");
    assert_eq!(
        fnv_f32(
            &a.positions
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<f32>>()
        ),
        fnv_f32(
            &b.positions
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<f32>>()
        ),
    );
}

// ---- 退化三角形：零面积三角的立体角贡献恒 0 ⇒ 绕数符号逐格不变；
// 距离腿把它当弦段几何（min over 更多三角只会收缩 |d|）——这不是缺陷，是几何变了。
// 钉的是：确定性 + 带内无 NaN + 符号逐格一致 + |d| 只能收缩。----

fn degenerate_mesh() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let (verts, faces) = icosphere(2, 0.75);
    let v0 = verts.len() as u32;
    let mut f = faces.clone();
    f.push([v0, v0 + 1, v0 + 1]); // 重复顶点 ⇒ b×c ≡ 0 ⇒ 立体角分子恒 0
    let mut v = verts.clone();
    v.push(verts[0]);
    v.push(verts[1]);
    (v, f)
}

#[test]
fn mesh_sdf_degenerate_tri_sign_neutral_cpu() {
    let (verts, faces) = icosphere(2, 0.75);
    let (deg_verts, deg_faces) = degenerate_mesh();
    let (base, _) = mesh_to_sdf_band_winding(32, 0.75, 0.2, &verts, &faces);
    let (with, _) = mesh_to_sdf_band_winding(32, 0.75, 0.2, &deg_verts, &deg_faces);
    let (mut sign_diff, mut shrink_violation, mut max_drop) = (0usize, 0usize, 0f32);
    let mut probes = 0usize;
    for (b, w) in base.iter().zip(&with) {
        if !b.is_finite() {
            continue; // 带掩码
        }
        probes += 1;
        assert!(w.is_finite(), "退化三角使带内出现非有限值");
        if (*b < 0.0) != (*w < 0.0) {
            sign_diff += 1;
        }
        if w.abs() > b.abs() + 1e-6 {
            shrink_violation += 1;
        }
        max_drop = max_drop.max(b.abs() - w.abs());
    }
    println!(
        "CPU 退化三角：{probes} 格，符号差 {sign_diff}，收缩违规 {shrink_violation}，最大收缩 {max_drop:.5}"
    );
    assert!(probes >= 100, "反空跑：{probes}");
    assert_eq!(
        sign_diff, 0,
        "退化三角改变了绕数符号判定（立体角约定被破坏）"
    );
    assert_eq!(shrink_violation, 0, "退化三角使 |d| 变大（min 语义被破坏）");
}

#[test]
fn mesh_sdf_degenerate_tri_sign_neutral_gpu() {
    let _gpu = gpu_lock();
    let (verts, faces) = icosphere(2, 0.75);
    let (deg_verts, deg_faces) = degenerate_mesh();
    let hd = headless_device();
    let base = mesh_to_sdf_gpu(&hd, &verts, &faces, 32, SignMode::Winding, None);
    let with_a = mesh_to_sdf_gpu(&hd, &deg_verts, &deg_faces, 32, SignMode::Winding, None);
    let with_b = mesh_to_sdf_gpu(&hd, &deg_verts, &deg_faces, 32, SignMode::Winding, None);
    assert_eq!(
        fnv_f32(&with_a),
        fnv_f32(&with_b),
        "含退化三角的场两次运行不逐位（确定性破坏）"
    );
    // 分类比较（带隶属口径）：符号腿只覆盖 |d| ≤ band(4.5 体素) 的格；退化三角改变了
    // 采样云 ⇒ JFA 距离在带缘重排 ⇒ 隶属可翻转（base 带内被投符号 / with 带外保持无符号）。
    // 绕数约定（退化三角贡献 ≡ 0）只在**两场都带内**的格上判符号；隶属翻转单独计数，
    // 要求两值都贴着带缘（±2 体素 JFA 近似带）；带外格比无符号距离 |Δ| ≤ 2 体素。
    let band_vox = 4.5f32; // 符号带宽（index 体素，mesh_sdf_gpu.rs band: 4.5）
    let h = 2.0f32 / 31.0; // n=32 世界步长
    let band = band_vox * h; // 换算成世界单位（场值的单位）
    let tol = 2.0f32 * h; // JFA 近似带（2 体素，同 #30 口径，世界单位）
    let (mut both_in, mut sign_diff, mut member_flips, mut max_delta) =
        (0usize, 0usize, 0usize, 0f32);
    let mut member_flip_vals: Vec<(usize, f32, f32)> = Vec::new();
    for (i, (b, w)) in base.iter().zip(&with_a).enumerate() {
        assert!(b.is_finite() && w.is_finite(), "非有限值 i={i}");
        let (b_in, w_in) = (b.abs() <= band, w.abs() <= band);
        if b_in && w_in {
            both_in += 1;
            if (*b < 0.0) != (*w < 0.0) {
                sign_diff += 1;
            }
            max_delta = max_delta.max((w.abs() - b.abs()).abs());
        } else if b_in != w_in {
            member_flips += 1;
            if member_flip_vals.len() < 5 {
                member_flip_vals.push((i, *b, *w));
            }
            assert!(
                b.abs() <= band + tol && w.abs() <= band + tol,
                "带隶属翻转但值远离带缘：i={i} base={b} with={w}"
            );
        } else {
            max_delta = max_delta.max((w.abs() - b.abs()).abs());
        }
    }
    println!(
        "GPU 退化三角：两场带内 {both_in}（符号差 {sign_diff}，|Δ| 最大 {max_delta:.5} 世界 = {:.2} 体素），隶属翻转 {member_flips} {member_flip_vals:?}",
        max_delta / h
    );
    assert!(both_in >= 100, "反空跑：{both_in}");
    assert_eq!(
        sign_diff, 0,
        "退化三角改变了带内绕数符号（WGSL 立体角约定与 CPU 不一致）"
    );
    assert!(
        max_delta <= tol,
        "GPU 距离差 {max_delta} 世界 = {:.2} 体素，超出 {tol} 体素近似带",
        max_delta / h
    );
}

// ---- 边界接触：网格面恰好落在域边界（±1）上，符号与有限性不崩 ----

#[test]
fn mesh_sdf_boundary_touching_box_stays_sane() {
    let _gpu = gpu_lock();
    let (verts, faces) = conv_core::box_mesh(1.0); // 六面贴着 [-1,1]³ 域边界
    let hd = headless_device();
    let g = mesh_to_sdf_gpu(&hd, &verts, &faces, 32, SignMode::Winding, None);
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    let (mut probes, mut false_inside, mut bad_finite) = (0usize, 0usize, 0usize);
    for (i, &d) in g.iter().enumerate() {
        assert!(
            d.is_finite() || d == f32::INFINITY,
            "边界接触场出现 NaN：i={i} d={d}"
        );
        let iu = i as u32;
        let p = [
            (iu % n) as f32 * h - 1.0,
            ((iu / n) % n) as f32 * h - 1.0,
            (iu / (n * n)) as f32 * h - 1.0,
        ];
        let sdf_analytic = conv_core::box_sdf(p, 1.0);
        let mut exact = f32::MAX;
        for t in &faces {
            let dd = conv_core::point_tri_dist(
                p,
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            exact = exact.min(dd);
        }
        if exact > 2.0 * h {
            continue;
        }
        probes += 1;
        if d.is_nan() || !d.is_finite() {
            bad_finite += 1;
        }
        if sdf_analytic.abs() <= 1e-6 {
            continue; // 贴面层：盒面恰在网格平面上 ⇒ 内外无定义（同 cpu_legs 盒判据口径）
        }
        let inside = sdf_analytic < 0.0;
        if d < 0.0 && !inside {
            false_inside += 1;
        }
    }
    println!("边界接触盒：比较 {probes} 体素，假内 {false_inside}，非有限 {bad_finite}");
    assert!(probes >= 100, "反空跑：比较体素数 {probes} 异常少");
    assert_eq!(bad_finite, 0, "带内出现非有限值");
    assert_eq!(false_inside, 0, "边界接触下把域外判成了内部");
    // 带内符号抽查：符号带宽 4.5 体素（≈0.29 世界），探针取距面 0.05（0.78 体素，带内）
    let idx = |c: f32| ((c + 1.0) / 2.0 * (n as f32 - 1.0)).round() as usize;
    let inband = g[idx(0.95) + idx(0.0) * n as usize + idx(0.0) * (n * n) as usize];
    assert!(inband < 0.0, "带内盒内侧应为负（符号失准）：{inband}");
    // 深内部（0.5,0,0）距面 0.5 = 8 体素 > 带宽 ⇒ 按设计保持无符号（正），记录性断言
    let deep = g[idx(0.5) + idx(0.0) * n as usize + idx(0.0) * (n * n) as usize];
    assert!(
        deep > 0.0,
        "带外深内部应保持无符号正（口径变了需复核）：{deep}"
    );
}

// ---- GSN 网格质量：无重复面 + 每条边恰由两面共享且方向相反（封闭定向流形）----

#[test]
fn gsn_sphere_is_oriented_edge_manifold() {
    let _gpu = gpu_lock();
    let sdf = field_to_voxels(64, 0.75);
    let hd = headless_device();
    let m = surface_nets_gpu(&hd, &sdf, 64, None);
    let quads = directed_quads(&m.indices, |v| v);
    assert!(!quads.is_empty(), "反空跑：球面网格为空");

    // 无重复面（多重集无重复键）
    let unique = quads.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique.len(),
        quads.len(),
        "存在重复四边形 {} / {}",
        quads.len() - unique.len(),
        quads.len()
    );

    // 边流形 + 定向一致：每条无序边恰被两面共享；有向边 (a→b) 与 (b→a) 各恰一次
    let mut undirected: HashMap<(u32, u32), usize> = HashMap::new();
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            *undirected.entry((lo, hi)).or_insert(0) += 1;
            *directed.entry((a, b)).or_insert(0) += 1;
        }
    }
    let (mut deg_bad, mut dir_bad) = (0usize, 0usize);
    for &c in undirected.values() {
        if c != 2 {
            deg_bad += 1;
        }
    }
    for (&(a, b), &c) in &directed {
        if c != 1 || directed.get(&(b, a)) != Some(&1) {
            dir_bad += 1;
        }
    }
    println!(
        "GSN 流形：{} 边，度≠2 {deg_bad}，定向不一致 {dir_bad}",
        undirected.len()
    );
    assert_eq!(deg_bad, 0, "存在非二度边（开边或非流形）");
    assert_eq!(dir_bad, 0, "共享边的两侧绕向相同（定向不一致）");
}
