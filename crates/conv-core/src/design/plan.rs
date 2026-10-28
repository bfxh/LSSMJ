//! 校验 + 建造计划：把一份 [`Design`] 拆成"按顺序、可视觉重放"的步骤序列。
//!
//! 校验规则（每条都对应一个诊断，带零件路径，方便回修神经头的输出）：
//! - 结构：id 全局唯一；系统/总成/零件都不许空；总成嵌套 ≤ [`MAX_ASM_DEPTH`] 层；
//! - 装配顺序：一个总成里第一个零件必须 `Base`，其余必须 `To(本总成里更早声明的零件)`；
//! - 建造态：草图必须被拉伸/旋转消费；布尔只能引用已产出的实体；倒角/贴图要先有实体；
//!   零件结束时必须 ≥1 实体、且没有悬空草图。
//!
//! 计划按声明顺序深度优先展开，**同一份设计必得同一份计划**（无 HashMap、无浮点累积）。

use super::ir::*;

/// 系统之下允许的总成嵌套层数（总成 → 子总成 → 再下一级）。
pub const MAX_ASM_DEPTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    /// 出问题的零件/总成路径，如 `frame/leg_l/knee/bolt`。
    pub path: String,
    pub msg: String,
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.msg)
    }
}

/// 某一步之后的零件状态（"视觉建造状态"）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub bodies: usize,
    pub open_sketch: bool,
    pub textured: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub seq: usize,
    pub path: String,
    pub kind: &'static str,
    pub state: State,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub systems: usize,
    pub assemblies: usize,
    pub parts: usize,
    pub bodies: usize,
    pub steps: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub summary: Summary,
}

struct Ctx {
    /// 已见过的 id → 首次出现路径（查重）
    seen: std::collections::BTreeMap<String, String>,
    steps: Vec<Step>,
    diags: Vec<Diag>,
    bodies: usize,
}

impl Ctx {
    fn err(&mut self, path: &str, msg: impl Into<String>) {
        self.diags.push(Diag {
            path: path.to_string(),
            msg: msg.into(),
        });
    }
}

/// 校验并生成建造计划；有任何诊断就整体拒绝（不做"部分可用"的降级）。
pub fn validate(d: &Design) -> Result<Plan, Vec<Diag>> {
    let mut c = Ctx {
        seen: Default::default(),
        steps: Vec::new(),
        diags: Vec::new(),
        bodies: 0,
    };
    if d.name.trim().is_empty() {
        c.err("<design>", "方案名为空");
    }
    if d.systems.is_empty() {
        c.err("<design>", "方案里没有任何系统");
    }
    for s in &d.systems {
        let p = c.claim(&s.id, "");
        if s.assemblies.is_empty() {
            c.err(&p, "系统里没有任何总成");
        }
        for a in &s.assemblies {
            c.assembly(a, &p, 1);
        }
    }
    if !c.diags.is_empty() {
        return Err(c.diags);
    }
    let summary = Summary {
        systems: d.systems.len(),
        assemblies: count_assemblies(d),
        parts: count_parts(d),
        bodies: c.bodies,
        steps: c.steps.len(),
    };
    Ok(Plan {
        steps: c.steps,
        summary,
    })
}

impl Ctx {
    /// 登记 id 并返回它的路径；重名记为诊断（返回带 `#dup` 的路径以继续收集其它问题）。
    fn claim(&mut self, id: &Id, prefix: &str) -> String {
        let path = if prefix.is_empty() {
            id.to_string()
        } else {
            format!("{prefix}/{id}")
        };
        if let Some(first) = self.seen.get(id.as_str()) {
            let first = first.clone();
            self.err(&path, format!("id 重复（首次出现在 {first}）"));
            return format!("{path}#dup");
        }
        self.seen.insert(id.as_str().to_string(), path.clone());
        path
    }

    fn assembly(&mut self, a: &Assembly, prefix: &str, depth: usize) {
        let path = self.claim(&a.id, prefix);
        if depth > MAX_ASM_DEPTH {
            self.err(
                &path,
                format!("总成嵌套超过 {MAX_ASM_DEPTH} 层（系统→总成→子总成→零件）"),
            );
        }
        if a.children.is_empty() {
            self.err(&path, "总成里没有子总成或零件");
            return;
        }
        // 本总成作用域内的零件：第一个必须 Base，其余挂到更早的零件上。
        let mut declared: Vec<String> = Vec::new();
        let mut first_part = true;
        for child in &a.children {
            match child {
                Node::Assembly(sub) => self.assembly(sub, &path, depth + 1),
                Node::Part(p) => {
                    let pp = self.part(p, &path, &declared, first_part);
                    declared.push(pp);
                    first_part = false;
                }
            }
        }
    }

    fn part(&mut self, p: &Part, prefix: &str, earlier: &[String], first: bool) -> String {
        let path = self.claim(&p.id, prefix);
        let place = match &p.attach {
            Attach::Base(pl) if !first => {
                self.err(&path, "只有本总成的第一个零件可以用 Base");
                pl
            }
            Attach::Base(pl) => pl,
            Attach::To { id: t, place } => {
                if first {
                    self.err(&path, "第一个零件必须用 Base（没有可挂的对象）");
                } else if !earlier.iter().any(|e| e.ends_with(&format!("/{t}"))) {
                    self.err(
                        &path,
                        format!("挂接目标 {t} 不是本总成里更早声明的零件（装配顺序不成立）"),
                    );
                }
                place
            }
        };
        if !place.is_finite() {
            self.err(&path, "放置里有非有限值");
        } else if place.norm_q() < 1e-6 {
            self.err(&path, "放置的四元数为零");
        }
        let mut st = State {
            bodies: 0,
            open_sketch: false,
            textured: false,
        };
        for (i, op) in p.ops.iter().enumerate() {
            let at = format!("{path}@{i}");
            // 一步出错就不改变建造态：否则"缺草图"的拉伸还会被算成一个实体，
            // 后面的诊断与计划都会跟着错。
            let before = self.diags.len();
            let prev = st;
            match op {
                Op::Sketch { profile, .. } => {
                    if st.open_sketch {
                        self.err(&at, "上一个草图还没拉伸/旋转");
                    }
                    if profile.0.len() < 3 {
                        self.err(&at, format!("闭合轮廓至少 3 点，实得 {}", profile.0.len()));
                    }
                    if profile.0.iter().flatten().any(|v| !v.is_finite()) {
                        self.err(&at, "轮廓里有非有限坐标");
                    }
                    st.open_sketch = true;
                }
                Op::Extrude { distance } => {
                    if !st.open_sketch {
                        self.err(&at, "拉伸前没有草图");
                    }
                    if !(distance.is_finite() && *distance > 0.0) {
                        self.err(&at, format!("拉伸距离必须 >0 且有限，实得 {distance}"));
                    }
                    st.open_sketch = false;
                    st.bodies += 1;
                }
                Op::Revolve { angle_deg } => {
                    if !st.open_sketch {
                        self.err(&at, "旋转前没有草图");
                    }
                    if !(angle_deg.is_finite() && *angle_deg > 0.0 && *angle_deg <= 360.0) {
                        self.err(&at, format!("旋转角必须在 (0,360]，实得 {angle_deg}"));
                    }
                    st.open_sketch = false;
                    st.bodies += 1;
                }
                Op::Boolean { with, kind } => {
                    if *with >= st.bodies {
                        self.err(
                            &at,
                            format!("布尔引用了第 {with} 个实体，但当前只产出 {} 个", st.bodies),
                        );
                    }
                    if st.bodies == 0 {
                        self.err(&at, "布尔前没有任何实体");
                    }
                    let _ = kind;
                }
                Op::Fillet { radius } => {
                    if st.bodies == 0 {
                        self.err(&at, "倒角前没有实体");
                    }
                    if !(radius.is_finite() && *radius > 0.0) {
                        self.err(&at, format!("圆角半径必须 >0，实得 {radius}"));
                    }
                }
                Op::Texture { name } => {
                    if st.bodies == 0 {
                        self.err(&at, "贴图前没有实体");
                    }
                    if name.trim().is_empty() {
                        self.err(&at, "材质名为空");
                    }
                    st.textured = true;
                }
            }
            if self.diags.len() != before {
                st = prev;
            }
            self.steps.push(Step {
                seq: self.steps.len(),
                path: path.clone(),
                kind: op.kind(),
                state: st,
            });
        }
        if st.open_sketch {
            self.err(&path, "零件结束时还有未拉伸的草图");
        }
        if st.bodies == 0 {
            self.err(&path, "零件没有产出任何实体");
        }
        self.bodies += st.bodies;
        path
    }
}

fn count_assemblies(d: &Design) -> usize {
    fn walk(a: &Assembly, n: &mut usize) {
        *n += 1;
        for c in &a.children {
            if let Node::Assembly(s) = c {
                walk(s, n);
            }
        }
    }
    let mut n = 0;
    for s in &d.systems {
        for a in &s.assemblies {
            walk(a, &mut n);
        }
    }
    n
}

fn count_parts(d: &Design) -> usize {
    fn walk(a: &Assembly, n: &mut usize) {
        for c in &a.children {
            match c {
                Node::Assembly(s) => walk(s, n),
                Node::Part(_) => *n += 1,
            }
        }
    }
    let mut n = 0;
    for s in &d.systems {
        for a in &s.assemblies {
            walk(a, &mut n);
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Id {
        Id::new(s).unwrap()
    }

    fn square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    /// 一个能过的最小方案：1 系统 / 1 总成（含子总成）/ 3 零件。
    fn good() -> Design {
        let bolt = Part::slab(id("bolt"), Attach::Base(Place::IDENTITY), Plane::Xy, square(), 0.1);
        let plate = Part {
            id: id("plate"),
            attach: Attach::To {
                id: id("bolt"),
                place: Place::at([0.0, 0.0, 0.1]),
            },
            ops: vec![
                Op::Sketch {
                    plane: Plane::Xz,
                    profile: Contour(square()),
                },
                Op::Extrude { distance: 0.2 },
                Op::Fillet { radius: 0.01 },
                Op::Texture {
                    name: "brushed_steel".into(),
                },
            ],
        };
        let knee = Node::assembly(
            id("knee"),
            vec![Node::Part(Part::slab(
                id("bracket"),
                Attach::Base(Place::IDENTITY),
                Plane::Xz,
                square(),
                0.05,
            ))],
        );
        Design {
            name: "demo".into(),
            systems: vec![System {
                id: id("frame"),
                assemblies: vec![Assembly {
                    id: id("leg_l"),
                    place: Place::IDENTITY,
                    children: vec![Node::Part(bolt), Node::Part(plate), knee],
                }],
            }],
        }
    }

    #[test]
    fn valid_design_yields_ordered_plan() {
        let p = validate(&good()).expect("应通过");
        assert_eq!(
            p.summary,
            Summary {
                systems: 1,
                assemblies: 2,
                parts: 3,
                bodies: 3,
                steps: 8
            }
        );
        let kinds: Vec<_> = p.steps.iter().map(|s| (s.path.as_str(), s.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("frame/leg_l/bolt", "sketch"),
                ("frame/leg_l/bolt", "extrude"),
                ("frame/leg_l/plate", "sketch"),
                ("frame/leg_l/plate", "extrude"),
                ("frame/leg_l/plate", "fillet"),
                ("frame/leg_l/plate", "texture"),
                ("frame/leg_l/knee/bracket", "sketch"),
                ("frame/leg_l/knee/bracket", "extrude"),
            ]
        );
        // 视觉建造态：草图步之后没有实体，拉伸步之后有 1 个
        assert_eq!(p.steps[0].state.bodies, 0);
        assert!(p.steps[0].state.open_sketch);
        assert_eq!(p.steps[1].state.bodies, 1);
        assert!(!p.steps[1].state.open_sketch);
        assert!(p.steps[5].state.textured);
        assert_eq!(
            p.steps.iter().map(|s| s.seq).collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>(),
            "seq 必须是 0..n 的连续序号"
        );
    }

    #[test]
    fn plan_is_deterministic() {
        let a = validate(&good()).unwrap();
        let b = validate(&good()).unwrap();
        assert_eq!(a, b, "同一份设计必须得到同一份计划");
    }

    #[test]
    fn extrude_without_sketch_is_rejected_with_path() {
        let mut d = good();
        if let Node::Part(p) = &mut d.systems[0].assemblies[0].children[0] {
            p.ops.remove(0); // 去掉草图
        }
        let e = validate(&d).unwrap_err();
        assert!(
            e.iter().any(|x| x.path == "frame/leg_l/bolt@0" && x.msg.contains("没有草图")),
            "{e:?}"
        );
        assert!(e.iter().any(|x| x.path == "frame/leg_l/bolt" && x.msg.contains("没有产出")));
    }

    #[test]
    fn part_without_solid_is_rejected() {
        let mut d = good();
        if let Node::Part(p) = &mut d.systems[0].assemblies[0].children[0] {
            p.ops.truncate(1); // 只留草图
        }
        let e = validate(&d).unwrap_err();
        assert!(e.iter().any(|x| x.msg.contains("未拉伸")));
        assert!(e.iter().any(|x| x.msg.contains("没有产出")));
    }

    #[test]
    fn attach_must_point_at_earlier_sibling() {
        let mut d = good();
        if let Node::Part(p) = &mut d.systems[0].assemblies[0].children[0] {
            p.attach = Attach::To {
                id: id("bracket"),
                place: Place::IDENTITY,
            }; // 自相矛盾：第一个零件不能挂
        }
        let e = validate(&d).unwrap_err();
        assert!(e.iter().any(|x| x.msg.contains("第一个零件必须用 Base")), "{e:?}");
    }

    #[test]
    fn duplicate_id_across_levels_is_rejected() {
        let mut d = good();
        d.systems[0].assemblies[0].id = id("bolt");
        let e = validate(&d).unwrap_err();
        assert!(e.iter().any(|x| x.msg.contains("id 重复")), "{e:?}");
    }

    #[test]
    fn too_deep_nesting_is_rejected() {
        let mut d = good();
        let mut node = Node::Part(Part::slab(
            id("leaf"),
            Attach::Base(Place::IDENTITY),
            Plane::Xy,
            square(),
            0.1,
        ));
        for i in 0..MAX_ASM_DEPTH {
            node = Node::assembly(id(&format!("a{i}")), vec![node]);
        }
        d.systems[0].assemblies[0].children.push(node);
        let e = validate(&d).unwrap_err();
        assert!(
            e.iter().any(|x| x.msg.contains("嵌套超过")),
            "应拦下过深嵌套：{e:?}"
        );
    }

    #[test]
    fn boolean_forward_reference_is_rejected() {
        let mut d = good();
        if let Node::Part(p) = &mut d.systems[0].assemblies[0].children[0] {
            p.ops.push(Op::Boolean {
                kind: BoolKind::Difference,
                with: 1, // 只有 1 个实体（下标 0）
            });
        }
        let e = validate(&d).unwrap_err();
        assert!(e.iter().any(|x| x.msg.contains("布尔引用了第 1 个实体")), "{e:?}");
    }

    #[test]
    fn empty_design_is_rejected() {
        let d = Design {
            name: "x".into(),
            systems: vec![],
        };
        let e = validate(&d).unwrap_err();
        assert!(e.iter().any(|x| x.msg.contains("没有任何系统")));
    }

    #[test]
    fn illegal_id_is_rejected_at_construction() {
        assert!(Id::new("").is_err());
        assert!(Id::new("a b").is_err());
        assert!(Id::new("零件").is_err());
        assert!(Id::new(&"x".repeat(65)).is_err());
        assert!(Id::new("leg_l.knee-2").is_ok());
    }
}
