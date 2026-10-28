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
    /// 相对父级坐标系的位置；顶层总成即相对系统坐标系。
    pub place: Place,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Assembly(Box<Assembly>),
    Part(Part),
}

impl Node {
    pub fn assembly(id: Id, children: Vec<Node>) -> Self {
        Node::Assembly(Box::new(Assembly {
            id,
            place: Place::IDENTITY,
            children,
        }))
    }
}

/// 刚体放置：平移 + 四元数 `[w, x, y, z]`（不做缩放——缩放该写进草图尺寸，便于追溯）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub t: [f32; 3],
    pub q: [f32; 4],
}

impl Place {
    pub const IDENTITY: Place = Place {
        t: [0.0; 3],
        q: [1.0, 0.0, 0.0, 0.0],
    };

    pub fn at(t: [f32; 3]) -> Self {
        Self { t, ..Self::IDENTITY }
    }

    /// 绕 Z 轴旋转 `deg` 度。
    pub fn rot_z(deg: f32) -> Self {
        let h = deg.to_radians() * 0.5;
        Self {
            t: [0.0; 3],
            q: [h.cos(), 0.0, 0.0, h.sin()],
        }
    }

    pub fn is_finite(&self) -> bool {
        self.t.iter().all(|v| v.is_finite()) && self.q.iter().all(|v| v.is_finite())
    }

    pub fn norm_q(&self) -> f32 {
        (self.q[0] * self.q[0] + self.q[1] * self.q[1] + self.q[2] * self.q[2] + self.q[3] * self.q[3])
            .sqrt()
    }

    /// 复合：`self ∘ other`（先 other 再 self）。
    pub fn then(&self, other: &Place) -> Place {
        let (a, b) = (self.normalized(), other.normalized());
        let q = [
            a.q[0] * b.q[0] - a.q[1] * b.q[1] - a.q[2] * b.q[2] - a.q[3] * b.q[3],
            a.q[0] * b.q[1] + a.q[1] * b.q[0] + a.q[2] * b.q[3] - a.q[3] * b.q[2],
            a.q[0] * b.q[2] - a.q[1] * b.q[3] + a.q[2] * b.q[0] + a.q[3] * b.q[1],
            a.q[0] * b.q[3] + a.q[1] * b.q[2] - a.q[2] * b.q[1] + a.q[3] * b.q[0],
        ];
        let r = rotate(a.q, b.t);
        Place {
            t: [a.t[0] + r[0], a.t[1] + r[1], a.t[2] + r[2]],
            q,
        }
    }

    fn normalized(&self) -> Place {
        let n = self.norm_q();
        if n > 0.0 {
            Place {
                t: self.t,
                q: [
                    self.q[0] / n,
                    self.q[1] / n,
                    self.q[2] / n,
                    self.q[3] / n,
                ],
            }
        } else {
            Self::IDENTITY
        }
    }
}

/// 用单位四元数旋转向量。
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n <= 0.0 {
        return v;
    }
    let (w, x, y, z) = (q[0] / n, q[1] / n, q[2] / n, q[3] / n);
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
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

/// 零件落位的两种方式。两者都带 [`Place`]：`Base` 相对本总成坐标系，
/// `To` 相对被挂零件的坐标系。
#[derive(Clone, Debug, PartialEq)]
pub enum Attach {
    Base(Place),
    To { id: Id, place: Place },
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
