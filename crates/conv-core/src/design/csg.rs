//! 布尔与倒角：**在体素 SDF 域做**，再抽等值面。
//!
//! 为什么不直接做三角网 CSG：共面/退化三角/数值谓词这三件事一旦不精确，输出就不再水密，
//! 而"水密"是后面所有环节（体积、体素化、面元采样）的前提。这里换成离散路线：
//! 1. 网格 → 网格点上的有符号距离（最近三角距离 + 沿倾斜射线的奇偶判内外）；
//! 2. 布尔 = SDF 的 min（并）/ max（交）/ max(a, −b)（差）；
//! 3. 倒角 r = **形态学开运算**：先做半径 r 的 max 滤波（腐蚀），再做半径 r 的 min 滤波（膨胀）。
//!    对凸棱就是"内缩 r 的体 ⊕ 半径 r 的球"，圆角体积有闭式解可用于对账；
//! 4. Surface Nets 抽等值面，绕序按总体积符号统一。
//!
//! 代价与边界：精度受网格步长 h 限制（体积误差 ~1–3%），尖棱会被格点抹平；
//! 这是**表示选型**不是实现缺陷——要精确 CSG 得换 B-rep 内核。

use super::eval::Mesh;
use super::ir::BoolKind;
use crate::gauss::voxel::Grid;
use crate::point_tri_dist;

/// 自动取景：所有网格包围盒的并集 + `pad` 格边距。
pub fn grid_fit(meshes: &[&Mesh], n: u32, pad: u32) -> Grid {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for m in meshes {
        for v in &m.verts {
            for a in 0..3 {
                lo[a] = lo[a].min(v[a]);
                hi[a] = hi[a].max(v[a]);
            }
        }
    }
    let ext = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max).max(1e-6);
    let h = ext / (n as f32 - 1.0 - 2.0 * pad as f32);
    let span = h * (n as f32 - 1.0);
    let center = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    Grid {
        n,
        origin: [
            center[0] - span * 0.5,
            center[1] - span * 0.5,
            center[2] - span * 0.5,
        ],
        h,
    }
}

fn at(g: &Grid, i: usize) -> [f32; 3] {
    let n = g.n as usize;
    let x = i % n;
    let y = (i / n) % n;
    let z = i / (n * n);
    [
        g.origin[0] + x as f32 * g.h,
        g.origin[1] + y as f32 * g.h,
        g.origin[2] + z as f32 * g.h,
    ]
}

/// 网格 → 有符号距离场（负=内部）。
///
/// 内外用**倾斜射线**的奇偶判定：正轴射线在顶点/棱上会漏判，倾斜方向把这类退化
/// 变成"几乎不可能"，且方向固定 ⇒ 结果确定。
pub fn sdf(m: &Mesh, g: &Grid) -> Vec<f32> {
    let dir = normalize([1.0, 0.0013, 0.0007]);
    let mut out = Vec::with_capacity((g.n * g.n * g.n) as usize);
    for i in 0..(g.n * g.n * g.n) as usize {
        let p = at(g, i);
        let mut d = f32::INFINITY;
        for t in &m.tris {
            let (a, b, c) = (
                m.verts[t[0] as usize],
                m.verts[t[1] as usize],
                m.verts[t[2] as usize],
            );
            d = d.min(point_tri_dist(p, a, b, c));
        }
        let inside = odd_crossings(p, dir, m);
        out.push(if inside { -d } else { d });
    }
    out
}

fn odd_crossings(p: [f32; 3], dir: [f32; 3], m: &Mesh) -> bool {
    let mut n = 0usize;
    for t in &m.tris {
        let (a, b, c) = (
            m.verts[t[0] as usize],
            m.verts[t[1] as usize],
            m.verts[t[2] as usize],
        );
        if hit(p, dir, a, b, c) {
            n += 1;
        }
    }
    n % 2 == 1
}

/// Möller–Trumbore（沿正方向）。
fn hit(o: [f32; 3], d: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let s = sub(o, a);
    let u = dot(s, h) * inv;
    if !(-1e-7..=1.0 + 1e-7).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < -1e-7 || u + v > 1.0 + 1e-7 {
        return false;
    }
    dot(e2, q) * inv > 1e-7
}

/// 布尔：并 = min、交 = max、差 = max(a, −b)。
pub fn combine(a: &[f32], b: &[f32], kind: BoolKind) -> Vec<f32> {
    assert_eq!(a.len(), b.len());
    match kind {
        BoolKind::Union => a.iter().zip(b).map(|(x, y)| x.min(*y)).collect(),
        BoolKind::Intersect => a.iter().zip(b).map(|(x, y)| x.max(*y)).collect(),
        BoolKind::Difference => a.iter().zip(b).map(|(x, y)| x.max(-y)).collect(),
    }
}

/// 布尔运算（自动取景到两者并集）。
pub fn boolean(a: &Mesh, b: &Mesh, kind: BoolKind, n: u32) -> Result<Mesh, String> {
    if n < 16 {
        return Err(format!("体素分辨率过小：{n}"));
    }
    let g = grid_fit(&[a, b], n, 2);
    let d = combine(&sdf(a, &g), &sdf(b, &g), kind);
    Ok(extract(&d, &g))
}

/// 倒角：半径 r 的形态学开运算（腐蚀 → 膨胀），再抽面。
///
/// 对凸棱等价于"内缩 r 的体 ⊕ 半径 r 的球"。`r` 小于一个格时不做（没有可分辨的效果）。
pub fn fillet(m: &Mesh, r: f32, n: u32) -> Result<Mesh, String> {
    if !(r.is_finite() && r > 0.0) {
        return Err(format!("圆角半径必须 >0 且有限，实得 {r}"));
    }
    let g = grid_fit(&[m], n, 2);
    let d = sdf(m, &g);
    let k = (r / g.h).round() as i32;
    if k < 1 {
        return Err(format!("半径 {r} 小于一个格点 {:.4}，无效果", g.h));
    }
    let e = morph(&d, &g, k, true); // 腐蚀
    let o = morph(&e, &g, k, false); // 膨胀
    Ok(extract(&o, &g))
}

/// 球邻域上的 max（`erode`）或 min（`dilate`）滤波。
fn morph(d: &[f32], g: &Grid, k: i32, erode: bool) -> Vec<f32> {
    let n = g.n as i32;
    let idx = |x: i32, y: i32, z: i32| ((z * n + y) * n + x) as usize;
    // 只收球内的偏移，棱角处不做"立方体"滤波（那会把角切平）
    let mut offs = Vec::new();
    for dz in -k..=k {
        for dy in -k..=k {
            for dx in -k..=k {
                if dx * dx + dy * dy + dz * dz <= k * k {
                    offs.push((dx, dy, dz));
                }
            }
        }
    }
    let mut out = vec![0f32; d.len()];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let mut acc = if erode { f32::NEG_INFINITY } else { f32::INFINITY };
                for &(dx, dy, dz) in &offs {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    let v = if (0..n).contains(&nx) && (0..n).contains(&ny) && (0..n).contains(&nz) {
                        d[idx(nx, ny, nz)]
                    } else {
                        // 越界一律当"外部"（正距离）：腐蚀时不会被越界值撑住
                        1.0
                    };
                    acc = if erode { acc.max(v) } else { acc.min(v) };
                }
                out[idx(x, y, z)] = acc;
            }
        }
    }
    out
}

/// 等值面抽取；绕序按总体积符号统一为朝外。
pub fn extract(values: &[f32], g: &Grid) -> Mesh {
    use fast_surface_nets::ndshape::RuntimeShape;
    use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};

    let n = g.n;
    let shape = RuntimeShape::<u32, 3>::new([n, n, n]);
    let mut buf = SurfaceNetsBuffer::default();
    surface_nets(values, &shape, [0; 3], [n - 1; 3], &mut buf);
    let verts: Vec<[f32; 3]> = buf
        .positions
        .iter()
        .map(|p| {
            [
                g.origin[0] + p[0] * g.h,
                g.origin[1] + p[1] * g.h,
                g.origin[2] + p[2] * g.h,
            ]
        })
        .collect();
    let tris: Vec<[u32; 3]> = buf
        .indices
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let mut m = Mesh { verts, tris };
    if m.signed_volume() < 0.0 {
        for t in &mut m.tris {
            t.swap(1, 2);
        }
    }
    m
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

#[cfg(test)]
mod tests {
    use super::super::eval::extrude;
    use super::super::ir::{Contour, Plane};
    use super::*;

    fn square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    fn cube() -> Mesh {
        extrude(&Contour(square()), Plane::Xy, 1.0).unwrap()
    }

    fn shifted(m: &Mesh, d: [f32; 3]) -> Mesh {
        Mesh {
            verts: m.verts.iter().map(|v| [v[0] + d[0], v[1] + d[1], v[2] + d[2]]).collect(),
            tris: m.tris.clone(),
        }
    }

    fn rel(a: f32, b: f32) -> f32 {
        ((a - b) / b).abs()
    }

    #[test]
    fn union_of_half_overlapping_cubes() {
        let a = cube();
        let b = shifted(&cube(), [0.5, 0.0, 0.0]);
        let m = boolean(&a, &b, BoolKind::Union, 48).unwrap();
        assert!(m.is_closed(), "布尔结果必须水密");
        let v = m.signed_volume();
        assert!(rel(v, 1.5) < 0.04, "并集体积应≈1.5，实得 {v}");
    }

    #[test]
    fn difference_and_intersect() {
        let a = cube();
        let b = shifted(&cube(), [0.5, 0.0, 0.0]);
        let d = boolean(&a, &b, BoolKind::Difference, 48).unwrap();
        assert!(d.is_closed());
        assert!(rel(d.signed_volume(), 0.5) < 0.06, "差集体积应≈0.5，实得 {}", d.signed_volume());
        let i = boolean(&a, &b, BoolKind::Intersect, 48).unwrap();
        assert!(i.is_closed());
        assert!(rel(i.signed_volume(), 0.5) < 0.06, "交集体积应≈0.5，实得 {}", i.signed_volume());
    }

    #[test]
    fn subtract_removes_everything_when_contained() {
        let a = cube();
        let big = shifted(&extrude(&Contour(vec![[-1.0, -1.0], [2.0, -1.0], [2.0, 2.0], [-1.0, 2.0]]), Plane::Xy, 2.0).unwrap(), [0.0, 0.0, -0.5]);
        let d = boolean(&a, &big, BoolKind::Difference, 40).unwrap();
        assert!(d.tris.is_empty() || d.signed_volume() < 0.01, "完全包含时差集应为空，实得 {}", d.signed_volume());
    }

    #[test]
    fn fillet_rounds_a_cube_to_the_closed_form() {
        let m = cube();
        let r = 0.1f32;
        let f = fillet(&m, r, 64).unwrap();
        assert!(f.is_closed(), "倒角结果必须水密");
        // 内缩 (1-2r) 的立方体 ⊕ 半径 r 的球：V = a³ + 6a²r + 3πa·r² + (4/3)πr³
        let a = 1.0 - 2.0 * r;
        let want = a.powi(3) + 6.0 * a * a * r + 3.0 * std::f32::consts::PI * a * r * r
            + 4.0 / 3.0 * std::f32::consts::PI * r * r * r;
        let got = f.signed_volume();
        assert!(rel(got, want) < 0.06, "圆角体积应≈{want:.4}，实得 {got:.4}");
        // 圆角后体积必须小于原立方体（开运算只会削凸棱）
        assert!(got < 1.0, "开运算不该增体积");
    }

    #[test]
    fn too_small_radius_is_reported() {
        let e = fillet(&cube(), 1e-4, 32).unwrap_err();
        assert!(e.contains("小于一个格点"), "{e}");
    }

    #[test]
    fn results_are_deterministic() {
        let a = cube();
        let b = shifted(&cube(), [0.5, 0.0, 0.0]);
        let m1 = boolean(&a, &b, BoolKind::Union, 32).unwrap();
        let m2 = boolean(&a, &b, BoolKind::Union, 32).unwrap();
        assert_eq!(m1, m2);
    }
}
