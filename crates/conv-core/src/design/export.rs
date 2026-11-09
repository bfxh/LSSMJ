//! 整份方案的合成与导出：系统 → 各总成 → 各零件的世界坐标实体，可选写成一个 OBJ。
//!
//! 合成是**声明顺序**的深度优先（与 [`super::plan`] 的建造计划同序），
//! 所以"第几步之后再合成一次"总能得到确定的结果，方便做分步预览。

use std::io::Write;
use std::path::Path;

use super::eval::{CSG_N, Mesh, build_part};
use super::ir::{Design, Node, Place};

/// 一份方案在世界坐标下的全部实体：`(零件路径, 实体列表)`。
pub fn compose_design(d: &Design) -> Result<Vec<(String, Vec<Mesh>)>, String> {
    compose_design_at(d, CSG_N)
}

/// 指定体素分辨率（布尔/倒角用）；倒角半径小于格点会报错，所以细特征要调大 `n`。
pub fn compose_design_at(d: &Design, n: u32) -> Result<Vec<(String, Vec<Mesh>)>, String> {
    let mut out = Vec::new();
    for s in &d.systems {
        for a in &s.assemblies {
            walk(a, Place::IDENTITY, &format!("{}/{}", s.id, a.id), n, &mut out)?;
        }
    }
    Ok(out)
}

fn walk(
    asm: &super::ir::Assembly,
    world: Place,
    path: &str,
    n: u32,
    out: &mut Vec<(String, Vec<Mesh>)>,
) -> Result<(), String> {
    use super::ir::Attach;
    use super::eval::transform;

    let here = world.then(&asm.place);
    let mut placed: Vec<(String, Place)> = Vec::new();
    for child in &asm.children {
        match child {
            Node::Assembly(sub) => walk(sub, here, &format!("{path}/{}", sub.id), n, out)?,
            Node::Part(p) => {
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
                let bodies = build_part_at_checked(p, n)?;
                out.push((
                    format!("{path}/{}", p.id),
                    bodies.iter().map(|m| transform(m, &w)).collect(),
                ));
                placed.push((p.id.to_string(), w));
            }
        }
    }
    Ok(())
}

fn build_part_at_checked(p: &super::ir::Part, n: u32) -> Result<Vec<Mesh>, String> {
    super::eval::build_part_at(p, n)
}

/// 写成一个 OBJ：每个零件一个 `o <路径>` 组，顶点/面全局编号。
///
/// 没用第三方写盘库——格式就这么点东西，自己写反而能保证字节级确定（测试直接比对）。
pub fn write_obj(parts: &[(String, Vec<Mesh>)], path: &Path) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "# trit design export")?;
    let mut base = 1u32; // OBJ 顶点从 1 开始
    for (name, bodies) in parts {
        for (bi, m) in bodies.iter().enumerate() {
            writeln!(f, "o {}#{}", name, bi)?;
            for v in &m.verts {
                writeln!(f, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2])?;
            }
            for t in &m.tris {
                writeln!(
                    f,
                    "f {} {} {}",
                    t[0] + base,
                    t[1] + base,
                    t[2] + base
                )?;
            }
            base += m.verts.len() as u32;
        }
    }
    f.flush()
}

/// 方案总览（给日志/界面的"这一步要建什么"用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub parts: usize,
    pub bodies: usize,
    pub tris: usize,
    pub verts: usize,
}

pub fn totals(parts: &[(String, Vec<Mesh>)]) -> Totals {
    let mut t = Totals {
        parts: parts.len(),
        ..Default::default()
    };
    for (_, bodies) in parts {
        t.bodies += bodies.len();
        for m in bodies {
            t.tris += m.tris.len();
            t.verts += m.verts.len();
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::super::ir::*;
    use super::*;

    fn id(s: &str) -> Id {
        Id::new(s).unwrap()
    }

    fn sq() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    fn design() -> Design {
        let base = Part::slab(id("base"), Attach::Base(Place::IDENTITY), Plane::Xy, sq(), 0.5);
        let post = Part::slab(
            id("post"),
            Attach::To {
                id: id("base"),
                place: Place::at([0.0, 0.0, 0.5]),
            },
            Plane::Xy,
            sq(),
            1.0,
        );
        let plate = Part::slab(id("plate"), Attach::Base(Place::IDENTITY), Plane::Xy, sq(), 0.25);
        Design {
            name: "two".into(),
            systems: vec![System {
                id: id("sys"),
                assemblies: vec![
                    Assembly {
                        id: id("left"),
                        place: Place::IDENTITY,
                        children: vec![Node::Part(base), Node::Part(post)],
                    },
                    Assembly {
                        id: id("right"),
                        place: Place::at([3.0, 0.0, 0.0]),
                        children: vec![Node::Part(plate)],
                    },
                ],
            }],
        }
    }

    #[test]
    fn composes_all_parts_with_assembly_offset() {
        let parts = compose_design(&design()).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].0, "sys/left/base");
        assert_eq!(parts[1].0, "sys/left/post");
        assert_eq!(parts[2].0, "sys/right/plate");
        // 右总成整体 +3 m；左总成的 post 挂在 base 顶上（z 从 0.5 起）
        let xs: Vec<f32> = parts[2].1[0].verts.iter().map(|v| v[0]).collect();
        assert!(xs.iter().cloned().fold(f32::INFINITY, f32::min) >= 3.0 - 1e-6);
        let zs: Vec<f32> = parts[1].1[0].verts.iter().map(|v| v[2]).collect();
        assert!(zs.iter().cloned().fold(f32::INFINITY, f32::min) >= 0.5 - 1e-6);
        let t = totals(&parts);
        assert_eq!((t.parts, t.bodies), (3, 3));
        assert_eq!(t.tris, 36, "三个拉伸体各 12 三角");
    }

    #[test]
    fn validate_and_compose_agree_on_paths() {
        // 校验器给的路径必须与合成后的路径一一对应，否则"第几步建哪颗零件"就对不上
        let d = design();
        let plan = super::super::plan::validate(&d).unwrap();
        let composed = compose_design(&d).unwrap();
        let mut plan_parts: Vec<String> = plan.steps.iter().map(|s| s.path.clone()).collect();
        plan_parts.dedup();
        let mut comp: Vec<String> = composed.iter().map(|(p, _)| p.clone()).collect();
        comp.sort();
        plan_parts.sort();
        assert_eq!(plan_parts, comp);
    }

    #[test]
    fn obj_export_is_byte_deterministic() {
        let parts = compose_design(&design()).unwrap();
        let p1 = std::env::temp_dir().join("trit_design_a.obj");
        let p2 = std::env::temp_dir().join("trit_design_b.obj");
        write_obj(&parts, &p1).unwrap();
        write_obj(&parts, &p2).unwrap();
        let a = std::fs::read(&p1).unwrap();
        let b = std::fs::read(&p2).unwrap();
        assert_eq!(a, b);
        let text = String::from_utf8(a).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("v ")).count(), totals(&parts).verts);
        assert_eq!(text.lines().filter(|l| l.starts_with("f ")).count(), totals(&parts).tris);
        assert!(text.contains("o sys/left/base#0"));
    }
}
