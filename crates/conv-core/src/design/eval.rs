//! 建造态求值：把 `Sketch + Extrude` 变成真几何（水密三角网格）。
//!
//! 只做基础态（草图 + 拉伸）——布尔/倒角/旋转明确返回"未实现"，不假装成功。
//! 判据用解析量而不是"看起来对"：
//! - 体积 = 轮廓面积 × 拉伸距离（有向体积，等于朴素四面体累加）；
//! - 水密：每条有向边恰好被一个相反方向的边配对；
//! - 凹多边形必须靠耳切三角化（扇形三角化在 L 形上会算出错的体积——测试里就是这么卡住的）。
//!
//! 平面基约定：2D 坐标 `(u, v)` 映射到平面基，拉伸方向是 `n = u × v`。
//! `Xy: u=X, v=Y, n=Z`；`Xz: u=Z, v=X, n=Y`；`Yz: u=Y, v=Z, n=X`；`OffsetZ(z)` 同 `Xy` 但原点 z=z。

use super::ir::{Contour, Plane};

/// 三角网格（索引三连）。顶点是唯一化的，不含法线。
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub verts: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn signed_volume(&self) -> f32 {
        let mut v = 0.0f64;
        for t in &self.tris {
            let a = self.verts[t[0] as usize];
            let b = self.verts[t[1] as usize];
            let c = self.verts[t[2] as usize];
            v += dot(a, cross(b, c)) as f64;
        }
        (v / 6.0) as f32
    }

    /// 水密判据：每条有向边恰好配一条反向边。
    pub fn is_closed(&self) -> bool {
        let mut edges: std::collections::BTreeMap<(u32, u32), i32> = Default::default();
        for t in &self.tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a == b {
                    return false;
                }
                *edges.entry((a, b)).or_default() += 1;
            }
        }
        edges.iter().all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
    }

    /// 欧拉示性数 V − E + F。
    pub fn euler(&self) -> i64 {
        let mut edges: std::collections::BTreeSet<(u32, u32)> = Default::default();
        for t in &self.tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                edges.insert((a.min(b), a.max(b)));
            }
        }
        self.verts.len() as i64 - edges.len() as i64 + self.tris.len() as i64
    }
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

/// 平面基：原点 + 两个基向量，法向 = u × v。
fn frame(plane: Plane) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match plane {
        Plane::Xy => ([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        Plane::Xz => ([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        Plane::Yz => ([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        Plane::OffsetZ(z) => ([0.0, 0.0, z], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    }
}

/// 有向面积（(u,v) 平面，逆时针为正）。
pub fn signed_area(pts: &[[f32; 2]]) -> f32 {
    let n = pts.len();
    let mut a = 0.0f64;
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        a += (p[0] as f64) * (q[1] as f64) - (q[0] as f64) * (p[1] as f64);
    }
    (a * 0.5) as f32
}

fn seg_intersect(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> bool {
    let o = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (d1, d2) = (o(c, d, a), o(c, d, b));
    let (d3, d4) = (o(a, b, c), o(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// 轮廓合法性：≥3 点、面积非零、不自交（相邻边共享端点不算）。
fn check_contour(pts: &[[f32; 2]]) -> Result<f32, String> {
    if pts.len() < 3 {
        return Err(format!("轮廓至少 3 点，实得 {}", pts.len()));
    }
    if pts.iter().flatten().any(|v| !v.is_finite()) {
        return Err("轮廓里有非有限坐标".into());
    }
    let area = signed_area(pts);
    if area.abs() < 1e-9 {
        return Err(format!("轮廓面积为 0（退化），实得 {area:e}"));
    }
    let n = pts.len();
    for i in 0..n {
        for j in i + 1..n {
            // 相邻边（含首尾）共享端点，跳过
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if seg_intersect(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                return Err(format!("轮廓自交：边 {i} 与边 {j}"));
            }
        }
    }
    Ok(area)
}

/// 耳切三角化（CCW 输入），返回顶点下标三元组。
fn ear_clip(pts: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    let mut out = Vec::with_capacity(pts.len() - 2);
    let cross2 = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let inside = |p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        let s = cross2(a, b, p);
        let t = cross2(b, c, p);
        let u = cross2(c, a, p);
        s >= -1e-12 && t >= -1e-12 && u >= -1e-12
    };
    let mut guard = 0usize;
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (i0, i1, i2) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[i0], pts[i1], pts[i2]);
            if cross2(a, b, c) <= 1e-12 {
                continue; // 凹角或退化，不是耳
            }
            if idx.iter().any(|&j| {
                j != i0
                    && j != i1
                    && j != i2
                    && inside(pts[j], a, b, c)
                    && // 边界上的共线点不算"在耳内"，否则会漏切
                    !(pts[j] == a || pts[j] == b || pts[j] == c)
            }) {
                continue;
            }
            out.push([i0, i1, i2]);
            idx.remove(k);
            clipped = true;
            break;
        }
        guard += 1;
        if !clipped || guard > pts.len() * pts.len() + 8 {
            // 兜底：退化输入（重复点/共线）用扇形补齐，保证不无限循环
            let i0 = idx[0];
            for k in 1..idx.len() - 1 {
                out.push([i0, idx[k], idx[k + 1]]);
            }
            return out;
        }
    }
    out.push([idx[0], idx[1], idx[2]]);
    out
}

/// 草图 → 拉伸实体。轮廓按逆时针规范化（负面积就反转），保证体积为正、法向朝外。
pub fn extrude(contour: &Contour, plane: Plane, distance: f32) -> Result<Mesh, String> {
    if !(distance.is_finite() && distance > 0.0) {
        return Err(format!("拉伸距离必须 >0 且有限，实得 {distance}"));
    }
    let area = check_contour(&contour.0)?;
    let mut pts = contour.0.clone();
    if area < 0.0 {
        pts.reverse();
    }
    let (o, u, v) = frame(plane);
    let n = cross(u, v);
    let n_pts = pts.len();
    let mut verts = Vec::with_capacity(n_pts * 2);
    for p in &pts {
        verts.push([
            o[0] + u[0] * p[0] + v[0] * p[1],
            o[1] + u[1] * p[0] + v[1] * p[1],
            o[2] + u[2] * p[0] + v[2] * p[1],
        ]);
    }
    for p in &pts {
        verts.push([
            o[0] + u[0] * p[0] + v[0] * p[1] + n[0] * distance,
            o[1] + u[1] * p[0] + v[1] * p[1] + n[1] * distance,
            o[2] + u[2] * p[0] + v[2] * p[1] + n[2] * distance,
        ]);
    }
    let mut tris = Vec::with_capacity((n_pts - 2) * 2 + n_pts * 2);
    let ring = ear_clip(&pts);
    // 底面：外法向是 −n ⇒ 绕序反过来
    for t in &ring {
        tris.push([t[0] as u32, t[2] as u32, t[1] as u32]);
    }
    // 顶面：外法向 +n ⇒ 保持 CCW
    for t in &ring {
        let b = n_pts as u32;
        tris.push([t[0] as u32 + b, t[1] as u32 + b, t[2] as u32 + b]);
    }
    // 侧壁
    for i in 0..n_pts {
        let j = (i + 1) % n_pts;
        let (bi, bj) = (i as u32, j as u32);
        let (ti, tj) = (bi + n_pts as u32, bj + n_pts as u32);
        tris.push([bi, bj, tj]);
        tris.push([bi, tj, ti]);
    }
    Ok(Mesh { verts, tris })
}

/// 零件级求值：按顺序重放 ops，返回产出的实体列表。
///
/// 只支持 Sketch + Extrude；其余 op 明确报"未实现"——不静默跳过一个布尔或倒角，
/// 那会让调用方以为零件已经建对了。
pub fn build_part(part: &super::ir::Part) -> Result<Vec<Mesh>, String> {
    let mut bodies: Vec<Mesh> = Vec::new();
    let mut pending: Option<(super::ir::Contour, Plane)> = None;
    for (i, op) in part.ops.iter().enumerate() {
        match op {
            super::ir::Op::Sketch { plane, profile } => {
                pending = Some((profile.clone(), *plane));
            }
            super::ir::Op::Extrude { distance } => {
                let (c, plane) = pending
                    .take()
                    .ok_or_else(|| format!("{}@{}: 拉伸前没有草图", part.id, i))?;
                bodies.push(extrude(&c, plane, *distance)?);
            }
            other => {
                return Err(format!(
                    "{}@{}: {} 尚未实现（当前只支持 sketch+extrude）",
                    part.id,
                    i,
                    other.kind()
                ))
            }
        }
    }
    if let Some(_c) = pending {
        return Err(format!("{}: 结束时还有未拉伸的草图", part.id));
    }
    Ok(bodies)
}

/// 把总成组合到世界坐标：返回每个零件的路径与**世界坐标下**的实体。
///
/// 放置按树复合：`world = 父总成世界放置 ∘ 本总成放置 ∘ 零件放置`；
/// `To { id, place }` 的参照系是**同一总成里更早的零件**（与校验规则一致）。
pub fn compose(asm: &super::ir::Assembly, world: super::ir::Place) -> Result<Vec<(String, Vec<Mesh>)>, String> {
    let mut out = Vec::new();
    walk(asm, world, "", &mut out)?;
    Ok(out)
}

fn walk(
    asm: &super::ir::Assembly,
    world: super::ir::Place,
    prefix: &str,
    out: &mut Vec<(String, Vec<Mesh>)>,
) -> Result<(), String> {
    use super::ir::{Attach, Node, Place};
    let here = world.then(&asm.place);
    let path = if prefix.is_empty() {
        asm.id.to_string()
    } else {
        format!("{prefix}/{}", asm.id)
    };
    let mut placed: Vec<(String, Place)> = Vec::new();
    for child in &asm.children {
        match child {
            Node::Assembly(a) => walk(a, here, &path, out)?,
            Node::Part(p) => {
                // Base 相对本总成坐标系；To 的目标已经带着世界放置，不能再乘一次 here
                // （否则整组旋转会被应用两遍——这个 bug 就是 placement_chain 测试抓到的）。
                let w = match &p.attach {
                    Attach::Base(pl) => here.then(pl),
                    Attach::To { id, place } => {
                        let (_, parent) = placed
                            .iter()
                            .find(|(pid, _)| pid == id.as_str())
                            .ok_or_else(|| {
                                format!("{}: 挂接目标 {id} 不在本总成里（或声明在后面）", p.id)
                            })?;
                        parent.then(place)
                    }
                };
                let bodies = build_part(p)?;
                out.push((format!("{path}/{}", p.id), bodies.into_iter().map(|m| transform(&m, &w)).collect()));
                placed.push((p.id.to_string(), w));
            }
        }
    }
    Ok(())
}

/// 刚体变换网格：只转顶点，拓扑与体积不变。
pub fn transform(m: &Mesh, place: &super::ir::Place) -> Mesh {
    use super::ir::rotate;
    Mesh {
        verts: m.verts.iter().map(|v| {
            let r = rotate(place.q, *v);
            [r[0] + place.t[0], r[1] + place.t[1], r[2] + place.t[2]]
        }).collect(),
        tris: m.tris.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ccw_square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    #[test]
    fn square_extrudes_to_watertight_prism() {
        let m = extrude(&Contour(ccw_square()), Plane::Xy, 0.5).unwrap();
        assert_eq!(m.verts.len(), 8);
        assert_eq!(m.tris.len(), 12, "2 个盖各 2 三角 + 4 面侧壁各 2");
        assert!(m.is_closed(), "必须水密");
        assert_eq!(m.euler(), 2, "拓扑球");
        assert!((m.signed_volume() - 0.5).abs() < 1e-6, "体积 {}", m.signed_volume());
    }

    #[test]
    fn clockwise_input_is_normalized() {
        let mut pts = ccw_square();
        pts.reverse();
        let m = extrude(&Contour(pts), Plane::Xy, 0.25).unwrap();
        assert!((m.signed_volume() - 0.25).abs() < 1e-6, "体积 {}", m.signed_volume());
        assert!(m.is_closed());
    }

    #[test]
    fn concave_l_shape_needs_real_triangulation() {
        // L 形（凹）：面积 3 个单位方块。扇形三角化会切出多边形外的三角形 ⇒ 体积不对。
        let pts = vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ];
        assert!((signed_area(&pts) - 3.0).abs() < 1e-6);
        let m = extrude(&Contour(pts), Plane::Xz, 2.0).unwrap();
        assert!(m.is_closed());
        assert_eq!(m.euler(), 2);
        assert!(
            (m.signed_volume() - 6.0).abs() < 1e-5,
            "体积应为 3×2=6，实得 {}",
            m.signed_volume()
        );
    }

    #[test]
    fn planar_planes_put_geometry_where_expected() {
        for (plane, axis, expect) in [
            (Plane::Xy, 2usize, 0.0f32),
            (Plane::OffsetZ(3.0), 2usize, 3.0f32),
            (Plane::Xz, 1usize, 0.0f32),
            (Plane::Yz, 0usize, 0.0f32),
        ] {
            let m = extrude(&Contour(ccw_square()), plane, 1.0).unwrap();
            for v in &m.verts {
                assert!((v[axis] - expect).abs() <= 1.0 + 1e-6, "{plane:?} 的 {axis} 轴越界");
            }
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for v in &m.verts {
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
            assert!((hi[axis] - lo[axis] - 1.0).abs() < 1e-6, "{plane:?} 拉伸长度不对");
        }
    }

    #[test]
    fn degenerate_contours_are_rejected() {
        assert!(extrude(&Contour(ccw_square()), Plane::Xy, 0.0).is_err());
        assert!(extrude(&Contour(ccw_square()), Plane::Xy, f32::NAN).is_err());
        assert!(extrude(&Contour(vec![[0.0, 0.0], [1.0, 1.0]]), Plane::Xy, 1.0).is_err());
        // 共线：面积 0
        assert!(extrude(
            &Contour(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]),
            Plane::Xy,
            1.0
        )
        .is_err());
        // 自交：蝴蝶结
        assert!(extrude(
            &Contour(vec![[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]]),
            Plane::Xy,
            1.0
        )
        .is_err());
    }

    #[test]
    fn part_builds_bodies_in_order() {
        use super::super::ir::{Attach, Id, Op, Part, Place};
        let p = Part {
            id: Id::new("base").unwrap(),
            attach: Attach::Base(Place::IDENTITY),
            ops: vec![
                Op::Sketch {
                    plane: Plane::Xy,
                    profile: Contour(ccw_square()),
                },
                Op::Extrude { distance: 0.5 },
                Op::Sketch {
                    plane: Plane::OffsetZ(0.5),
                    profile: Contour(ccw_square()),
                },
                Op::Extrude { distance: 0.25 },
            ],
        };
        let bodies = build_part(&p).unwrap();
        assert_eq!(bodies.len(), 2);
        assert!((bodies[0].signed_volume() - 0.5).abs() < 1e-6);
        assert!((bodies[1].signed_volume() - 0.25).abs() < 1e-6);
        assert!(bodies.iter().all(Mesh::is_closed));
    }

    #[test]
    fn unsupported_op_is_reported_not_skipped() {
        use super::super::ir::{Attach, Id, Op, Part, Place};
        let p = Part {
            id: Id::new("p").unwrap(),
            attach: Attach::Base(Place::IDENTITY),
            ops: vec![
                Op::Sketch {
                    plane: Plane::Xy,
                    profile: Contour(ccw_square()),
                },
                Op::Extrude { distance: 0.5 },
                Op::Fillet { radius: 0.01 },
            ],
        };
        let e = build_part(&p).unwrap_err();
        assert!(e.contains("尚未实现") && e.contains("fillet"), "{e}");
    }

    #[test]
    fn compose_applies_placement_chain() {
        use super::super::ir::{Assembly, Attach, Id, Node, Op, Part, Place};
        let mk = |name: &str, attach: Attach| {
            Node::Part(Part {
                id: Id::new(name).unwrap(),
                attach,
                ops: vec![
                    Op::Sketch {
                        plane: Plane::Xy,
                        profile: Contour(ccw_square()),
                    },
                    Op::Extrude { distance: 0.5 },
                ],
            })
        };
        let asm = Assembly {
            id: Id::new("asm").unwrap(),
            place: Place::rot_z(90.0), // 整组绕 Z 转 90°
            children: vec![
                mk("base", Attach::Base(Place::IDENTITY)),
                mk(
                    "arm",
                    Attach::To {
                        id: Id::new("base").unwrap(),
                        place: Place::at([2.0, 0.0, 0.0]), // 母件坐标系的 +X 2 m
                    },
                ),
            ],
        };
        let out = compose(&asm, Place::IDENTITY).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "asm/base");
        assert_eq!(out[1].0, "asm/arm");
        // 整组绕 Z 转 90°：零件自身占 x∈[-1,0]（方块的 −y 落到 −x），
        // 第二件挂在母件坐标系 +X 2 m ⇒ 世界平移 (0,2,0)，故 y∈[2,3]。
        let xs: Vec<f32> = out[1].1[0].verts.iter().map(|v| v[0]).collect();
        let ys: Vec<f32> = out[1].1[0].verts.iter().map(|v| v[1]).collect();
        let (xmin, xmax) = (xs.iter().cloned().fold(f32::INFINITY, f32::min), xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max));
        let (ymin, ymax) = (ys.iter().cloned().fold(f32::INFINITY, f32::min), ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max));
        assert!((xmin + 1.0).abs() < 1e-5 && (xmax - 0.0).abs() < 1e-5, "x 范围 {xmin}..{xmax}");
        assert!((ymin - 2.0).abs() < 1e-5 && (ymax - 3.0).abs() < 1e-5, "y 范围 {ymin}..{ymax}");
        // 刚体变换不改变体积与水密性
        for (_, bodies) in &out {
            for b in bodies {
                assert!(b.is_closed());
                assert!((b.signed_volume() - 0.5).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn compose_rejects_dangling_attach() {
        use super::super::ir::{Assembly, Attach, Id, Node, Op, Part, Place};
        let asm = Assembly {
            id: Id::new("a").unwrap(),
            place: Place::IDENTITY,
            children: vec![Node::Part(Part {
                id: Id::new("p").unwrap(),
                attach: Attach::To {
                    id: Id::new("ghost").unwrap(),
                    place: Place::IDENTITY,
                },
                ops: vec![Op::Sketch {
                    plane: Plane::Xy,
                    profile: Contour(ccw_square()),
                }, Op::Extrude { distance: 1.0 }],
            })],
        };
        let e = compose(&asm, Place::IDENTITY).unwrap_err();
        assert!(e.contains("ghost"), "{e}");
    }

    #[test]
    fn output_is_deterministic() {
        let a = extrude(&Contour(ccw_square()), Plane::Xy, 0.7).unwrap();
        let b = extrude(&Contour(ccw_square()), Plane::Xy, 0.7).unwrap();
        assert_eq!(a, b);
    }
}
