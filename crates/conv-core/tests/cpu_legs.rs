//! C22 判据的 CPU 腿测试（迁移自 scratch/conv-proto，读数锚 W15C-001..009）。
//! 末四针为 mesh→SDF 精度档（T-GC-01）的绕数判据：球/盒解析对拍 + 实时档错例 + 法向翻转不变性。

use conv_core::{
    GRID, box_mesh, box_sdf, euler_char, fibonacci_sphere, field_to_voxels, icosphere, mesh_hash,
    mesh_to_sdf_band, mesh_volume, naive_particle_field, radial_sign, sphere_sdf, winding_number,
    winding_sign,
};
use fast_surface_nets::ndshape::ConstShape3u32;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};

const R: f32 = 0.75;
const H: f32 = 2.0 / (GRID as f32 - 1.0);
type Shape = ConstShape3u32<64, 64, 64>;

/// 弦差上界：面心到解析球面的最远距离（三角化球整体内缩的这个壳层里，"解析内外"无意义）。
/// J2 用它钉误差上界，绕数判据用它作排除带。
fn chord_bound(verts: &[[f32; 3]], faces: &[[u32; 3]]) -> f32 {
    faces
        .iter()
        .map(|f| {
            let cen = [
                (verts[f[0] as usize][0] + verts[f[1] as usize][0] + verts[f[2] as usize][0]) / 3.0,
                (verts[f[0] as usize][1] + verts[f[1] as usize][1] + verts[f[2] as usize][1]) / 3.0,
                (verts[f[0] as usize][2] + verts[f[1] as usize][2] + verts[f[2] as usize][2]) / 3.0,
            ];
            sphere_sdf(cen, R).abs()
        })
        .fold(0.0f32, f32::max)
}

#[test]
fn mesh_sdf_matches_analytic_band() {
    // J2：最坏误差应恰为弦差上界（面心量法——顶点量法恒 0 是仪器错，W15C-006）
    let (verts, faces) = icosphere(2, R);
    let tess_bound = chord_bound(&verts, &faces);
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

// ---- mesh→SDF 精度档：广义绕数（T-GC-01）----

/// 两分量网格：icosphere(sub, r) 心移到 (±cx, 0, 0)，第二分量索引偏移后拼接。
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
fn winding_sign_is_exact_on_closed_sphere() {
    // 精度档第一判据：闭合定向网格上 w 在实体内恒 ±1、外恒 0（与三角化粗细无关），
    // 故内外判定应零错判——这是径向口径拿不到保证的性质（它只对凸体成立）。
    // 排除弦差壳：壳内"解析球内"而"网格面外"，两套口径本就相反（同 J2 的弦差上界）。
    let (verts, faces) = icosphere(2, R);
    let shell = chord_bound(&verts, &faces);
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    let mut probes = 0usize;
    let mut mismatch = 0usize;
    let mut w_in_min = f32::MAX;
    let mut w_out_max = 0f32;
    for i in 0..n * n * n {
        let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        let a = sphere_sdf(p, R);
        if a.abs() <= shell {
            continue; // 弦差壳：网格面与解析球面的内外相反，不作判据面
        }
        let w = winding_number(p, &verts, &faces);
        let inside = a < 0.0;
        if inside {
            w_in_min = w_in_min.min(w.abs());
        } else {
            w_out_max = w_out_max.max(w.abs());
        }
        if (w.abs() >= 0.5) != inside {
            mismatch += 1;
        }
        probes += 1;
    }
    println!(
        "绕数内外判定：{probes} 点（弦差壳 {shell:.5} 外），错判 {mismatch}；\
         |w| 内最小 {w_in_min:.6} / 外最大 {w_out_max:.6}"
    );
    assert!(
        probes >= 1000,
        "反空跑：判据点 {probes} 异常少（壳掩码吞掉了全域？）"
    );
    assert_eq!(mismatch, 0, "闭合球网格绕数判内外出现错判");
    assert!(w_in_min > 0.99, "内侧 |w| 应恒 ≈1，实测最小 {w_in_min}");
    assert!(w_out_max < 0.01, "外侧 |w| 应恒 ≈0，实测最大 {w_out_max}");
}

#[test]
fn winding_sign_matches_analytic_box() {
    // 验收口径"球/盒解析对拍"的盒侧：轴对齐盒是精确三角化 ⇒ 没有弦差带要排，
    // 内外判定应在整域零错判（顺带验 `box_mesh` 的绕序一致——不一致则 |w| 不为 1）。
    let half = 0.5f32;
    let (verts, faces) = box_mesh(half);
    let n = 32u32;
    let h = 2.0 / (n as f32 - 1.0);
    let mut probes = 0usize;
    let mut mismatch = 0usize;
    let mut w_in_min = f32::MAX;
    let mut w_out_max = 0f32;
    for i in 0..n * n * n {
        let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        let a = box_sdf(p, half);
        if a.abs() < 1e-6 {
            continue; // 与面重合的体素：内外无定义（本网格与格点不重合，走不到）
        }
        let w = winding_number(p, &verts, &faces);
        let inside = a < 0.0;
        if inside {
            w_in_min = w_in_min.min(w.abs());
        } else {
            w_out_max = w_out_max.max(w.abs());
        }
        if (w.abs() >= 0.5) != inside {
            mismatch += 1;
        }
        probes += 1;
    }
    println!(
        "绕数内外判定（盒）：{probes} 点，错判 {mismatch}；|w| 内最小 {w_in_min:.6} / 外最大 {w_out_max:.6}"
    );
    assert!(probes >= 1000, "反空跑：判据点 {probes} 异常少");
    assert_eq!(mismatch, 0, "闭合盒网格绕数判内外出现错判");
    assert!(w_in_min > 0.99, "内侧 |w| 应恒 ≈1，实测最小 {w_in_min}");
    assert!(w_out_max < 0.01, "外侧 |w| 应恒 ≈0，实测最大 {w_out_max}");
}

#[test]
fn winding_fixes_cases_where_radial_breaks() {
    // 精度档存在理由：原点不在实体内（多分量/非星形）时，径向出射口径把两球之间的凹湾整片判为内。
    // 本测试同时钉住"实时档在此处确实判错"作为错例证据——若哪天 radial 被改掉，此针会红并提醒复核。
    let (verts, faces) = two_spheres(2, 0.35, 0.5); // 分离：x ∈ [−0.85,−0.15] ∪ [0.15,0.85]
    let bay = [0.11f32, 0.0, 0.0]; // 两球凹湾（实体外）
    let deep = [0.30f32, 0.0, 0.0]; // 右球内
    assert!(
        winding_sign(bay, &verts, &faces) > 0.0,
        "精度档把凹湾判成了内"
    );
    assert!(
        radial_sign(bay, &verts, &faces) < 0.0,
        "错例证据失效：实时档此处不再判错（radial 行为变了，需复核口径）"
    );
    assert!(winding_sign(deep, &verts, &faces) < 0.0);
    assert!(radial_sign(deep, &verts, &faces) < 0.0);

    // 重叠处的重数：两球交叠 ⇒ w ≈ 2（径向无此概念，只会给 0/1）
    let (ov_v, ov_f) = two_spheres(2, 0.35, 0.25);
    let w0 = winding_number([0.0; 3], &ov_v, &ov_f);
    println!("重叠区绕数 w(原点)={w0:.5}（期望 ≈2）");
    assert!((1.8..2.2).contains(&w0), "重数叠加失准：w={w0}");
}

#[test]
fn winding_sign_ignores_global_orientation_flip() {
    // 判定用 |w|：整体翻绕序只翻 w 的符号，内外判定不变（法向不一致的网格口径记录）。
    let (verts, faces) = icosphere(2, R);
    let shell = chord_bound(&verts, &faces);
    let flipped: Vec<[u32; 3]> = faces.iter().map(|t| [t[0], t[2], t[1]]).collect();
    let n = 24u32;
    let h = 2.0 / (n as f32 - 1.0);
    let mut probes = 0usize;
    let mut diff = 0usize;
    let mut w_in_min = f32::MAX;
    for i in 0..n * n * n {
        let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        let a = sphere_sdf(p, R);
        if a.abs() <= shell {
            continue;
        }
        let sign0 = winding_sign(p, &verts, &faces);
        let w0 = winding_number(p, &verts, &faces);
        let w_flip = winding_number(p, &verts, &flipped);
        if a < 0.0 {
            w_in_min = w_in_min.min(w0.abs().min(w_flip.abs()));
            assert!(
                w0 * w_flip < 0.0,
                "内侧翻转后 w 未变号（{p:?}：w={w0}，翻转后 {w_flip}）"
            );
        }
        if sign0 != winding_sign(p, &verts, &flipped) {
            diff += 1;
        }
        probes += 1;
    }
    println!("翻绕序不变性：{probes} 点，判定差异 {diff}；内侧翻转后 |w| 最小 {w_in_min:.5}");
    assert!(probes >= 100, "反空跑：判据点 {probes} 异常少");
    assert_eq!(diff, 0, "整体翻绕序改变了内外判定（应只改 w 符号）");
    assert!(
        w_in_min > 0.99,
        "翻转前后内侧 |w| 都应 ≈1，实测最小 {w_in_min}"
    );
}
