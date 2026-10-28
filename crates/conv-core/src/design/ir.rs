//! 设计装配 IR 的数据层：系统 → 总成 → 子总成 → 零件，零件带**有序建造态**。
//!
//! 只描述"要建什么、按什么顺序建"，不含几何求值——求值留给渲染/建模后端。
//! 校验与建造计划在 [`super::plan`]。
//!
//! 一条设计是纯数据：没有任何 HashMap / 时间戳 / 浮点累积，序列化与重放都按声明顺序，
//! 这样"同一份设计 → 同一份建造计划"是可断言的（神经头吐出 IR 后必须先过校验再用）。

/// 标识符：全局唯一，只允许字母数字与 `_-.`，≤64 字符。
///
/// 全局一个命名空间（系统/总成/零件共用），这样"引用一个 id"永远不歧义。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(String);

impl Id {
    pub fn new(s: &str) -> Result<Self, String> {
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if ok {
            Ok(Self(s.to_string()))
        } else {
            Err(format!("非法 id {s:?}：只允许字母数字与 _-. ，长度 1..=64"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    pub name: String,
    pub systems: Vec<System>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct System {
    pub id: Id,
    pub assemblies: Vec<Assembly>,
}

/// 总成与子总成是同一个递归结构，叶子是零件。
#[derive(Clone, Debug, PartialEq)]
pub struct Assembly {
    pub id: Id,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Assembly(Box<Assembly>),
    Part(Part),
}

impl Node {
    pub fn assembly(id: Id, children: Vec<Node>) -> Self {
        Node::Assembly(Box::new(Assembly { id, children }))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub id: Id,
    /// 在本总成里的定位方式：第一个零件必须是 [`Attach::Base`]，
    /// 其余必须挂到**本总成里已声明**的零件上（顺序即装配顺序）。
    pub attach: Attach,
    /// 有序建造态：先 2D 草图，再拉伸成实体，再布尔/倒角/贴图。
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attach {
    Base,
    To(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// 草图所在基准面。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Plane {
    Xy,
    Xz,
    Yz,
    /// 平行于 Xy、沿 Z 偏移 `at`。
    OffsetZ(f32),
}

/// 闭合轮廓（折线，首尾自动闭合），2D 坐标单位与后端一致。
#[derive(Clone, Debug, PartialEq)]
pub struct Contour(pub Vec<[f32; 2]>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolKind {
    Union,
    Difference,
    Intersect,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// 在基准面上画闭合轮廓（一个零件同一时刻只允许一个未拉伸的草图）。
    Sketch { plane: Plane, profile: Contour },
    /// 沿草图法向拉伸 `distance`（>0），消耗当前草图并产出一个实体。
    Extrude { distance: f32 },
    /// 草图绕轴旋转 `angle_deg`（0,360]，消耗当前草图并产出一个实体。
    Revolve { angle_deg: f32 },
    /// 与第 `with` 个已产出实体做布尔（下标按产出顺序，从 0 起）。
    Boolean { kind: BoolKind, with: usize },
    /// 对所有实体倒圆角（半径 >0）。
    Fillet { radius: f32 },
    /// 贴材质/纹理（名字非空）。
    Texture { name: String },
}

impl Op {
    /// 建造计划里用的短标签（不带参数，便于比对顺序）。
    pub fn kind(&self) -> &'static str {
        match self {
            Op::Sketch { .. } => "sketch",
            Op::Extrude { .. } => "extrude",
            Op::Revolve { .. } => "revolve",
            Op::Boolean { .. } => "boolean",
            Op::Fillet { .. } => "fillet",
            Op::Texture { .. } => "texture",
        }
    }
}

impl Part {
    /// 便捷构造：草图 → 拉伸 → 贴图。
    pub fn slab(id: Id, attach: Attach, plane: Plane, contour: Vec<[f32; 2]>, dist: f32) -> Self {
        Self {
            id,
            attach,
            ops: vec![
                Op::Sketch {
                    plane,
                    profile: Contour(contour),
                },
                Op::Extrude { distance: dist },
            ],
        }
    }
}
