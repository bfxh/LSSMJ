//! 设计层的装配 IR：把"复杂设计"拆成系统 → 总成 → 子总成 → 零件的树，
//! 每个零件带**有序建造态**（2D 草图 → 拉伸/旋转成实体 → 布尔/倒角/贴图），
//! 校验后展开成可重放的建造计划。
//!
//! 设计目标：这是神经头要吐出的**目标格式**——所以它必须纯数据、可确定性重放、
//! 报错带零件路径（能指回是哪一颗零件、第几步）。

pub mod csg;
pub mod eval;
pub mod export;
pub mod ir;
pub mod material;
pub mod plan;
pub mod splat;
pub mod uv;
pub mod uv_lscm;

pub use csg::{boolean, fillet};
pub use export::{compose_design, totals, write_obj};
pub use eval::{build_part, compose, extrude, transform, Mesh};
pub use splat::surfels;
pub use uv::{unwrap as unwrap_uv, write_obj_uv, Axis as UvAxis, Island as UvIsland, TriState as UvTriState, UvLayout};
pub use ir::*;
pub use material::{bindings, validate as validate_materials, write_obj_mtl, Material, MaterialLib};
pub use plan::{validate, Diag, Plan, State, Step, Summary, MAX_ASM_DEPTH};
