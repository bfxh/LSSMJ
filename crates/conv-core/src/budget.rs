//! 预算治理阶梯（T-GC-06 第三片）：逐边超阈的响应纪律——**先降旋钮再降腿**。
//!
//! 旋钮集按"已有腿的实数面"接入：现阶段只有**体素分辨率**一枚（JFA / mesh→SDF /
//! GSN 三条腿都消费 `n`）；论文口径另两枚（高斯剪枝率 / 粒子间距）各自随
//! T-GC-04 / T-GC-03 采样腿落地时并入本表——不给不存在的腿建空转旋钮。
//!
//! 梯级的实效由 `tests/timing_probe.rs` 的 `budget_degrade_canary` 用真实 GPU
//! 测量钉住：降档后读数必须落回合成预算内，否则判红（降级不许是纸面动作）。

use crate::GRID;

/// 分辨率旋钮下限：与现有判据网格下限对齐（`tests/jfa_gpu.rs` 判据即跑 n=32）。
/// 比这更小的分辨率没有判据支撑——宁可直接降腿，也不进未验证区间。
pub const MIN_RES: u32 = 32;

/// 降级计划 = 当前生效的旋钮档位；`nominal()` = 全档（等于判据网格 `GRID`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetPlan {
    pub voxel_resolution: u32,
}

impl BudgetPlan {
    pub fn nominal() -> Self {
        Self {
            voxel_resolution: GRID,
        }
    }

    /// 降一档旋钮（分辨率减半，`MIN_RES` 为界）。`None` = 旋钮已到底。
    pub fn degrade(self) -> Option<Self> {
        let next = (self.voxel_resolution / 2).max(MIN_RES);
        if next == self.voxel_resolution {
            None
        } else {
            Some(Self {
                voxel_resolution: next,
            })
        }
    }
}

/// 一侧腿的超阈响应。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetResponse {
    /// 预算内：不动。
    Keep,
    /// 超阈：降一档旋钮后重试。
    Degrade(BudgetPlan),
    /// 超阈且旋钮到底：降腿（该腿默认档转换停用，由调用方记账）。
    DropLeg,
}

/// 超阈响应（序纪律钉死：先降旋钮再降腿）。
pub fn respond(over_budget: bool, plan: BudgetPlan) -> BudgetResponse {
    if !over_budget {
        return BudgetResponse::Keep;
    }
    match plan.degrade() {
        Some(next) => BudgetResponse::Degrade(next),
        None => BudgetResponse::DropLeg,
    }
}
