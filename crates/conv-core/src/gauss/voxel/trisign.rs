//! 三态符号场（3PSDF 风格）：由高斯面元朝向投票生成 Inside / Null / Outside。
//!
//! 与二值 TSDF 的差别不在数值精度，而在**语义**：每个面元只沿自己的法线投票——
//! 法线正侧（`d > +band`）投 Outside、负侧（`d < -band`）投 Inside、
//! 带内（`|d| <= band`）投 Null。逐体素取多数票，无票保持 Null。
//!
//! 这样薄壳（壁厚小于带宽）不会被强行解释成「有体积的薄实体」，
//! 壁带里的 Null 充当断路开关：两侧都是 Outside，但中间不是 Inside。
//! 提取交给 [`trit_core::signfield::extract_surface`] 的三态 Dual Contouring。

use super::{Grid, VoxelizeOptions, splat_range};
use crate::gauss::{Gaussian, bounds_of};
use trit_core::signfield::{SignField3D, TriSign};

/// 由高斯面元朝向投票生成三态符号场。
///
/// `band` 是「薄壳带」的半宽（世界单位）：
/// - `band` 越小，Null 带越薄，越容易得到单层界面（但采样不足时会破洞）；
/// - `band` 越大，Null 带越厚，界面会分成两层——这正是 3PSDF「需要更密采样」的体现。
pub fn tri_sign_field(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    band: Option<f32>,
) -> Result<(SignField3D, Grid), String> {
    if gaussians.is_empty() {
        return Err("没有高斯可体素化".into());
    }
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    // band 缺省 = 1 个体素边长：让 Null 带刚好一层体素厚（薄壳才可能单层）。
    let band = band.unwrap_or(grid.h).max(1e-9);
    let n = grid.n as usize;
    let n2 = n * n;
    let mut votes = vec![[0u32; 3]; n * n * n];

    for g in gaussians {
        let eig = g.eigen();
        let raw = g.normal.unwrap_or(eig.vectors[0]);
        let ln = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2])
            .sqrt()
            .max(1e-9);
        let nrm = [raw[0] / ln, raw[1] / ln, raw[2] / ln];
        let r = g.axis_radius();
        let range = splat_range(
            &grid,
            g.mean,
            [r[0] + band, r[1] + band, r[2] + band],
        );
        if range.empty {
            continue;
        }
        for z in range.lo[2]..=range.hi[2] {
            for y in range.lo[1]..=range.hi[1] {
                for x in range.lo[0]..=range.hi[0] {
                    let p = grid.world_of(x, y, z);
                    let d = (p[0] - g.mean[0]) * nrm[0]
                        + (p[1] - g.mean[1]) * nrm[1]
                        + (p[2] - g.mean[2]) * nrm[2];
                    let k = if d > band {
                        2 // outside
                    } else if d < -band {
                        0 // inside
                    } else {
                        1 // null：薄壳带
                    };
                    votes[z as usize * n2 + y as usize * n + x as usize][k] += 1;
                }
            }
        }
    }

    // 初值按 Outside（开放场景：未观测 = 视野外 = 外部）。
    // 若用 Null 作初值，「未观测 Null」会与「薄壳带 Null」混为一谈，
    // 导致数据覆盖边界（观测区 vs 视野外）被当成表面提取，面积暴涨。
    let mut field = SignField3D::filled(grid.n, TriSign::Outside);
    for (i, v) in votes.iter().enumerate() {
        if v[0] + v[1] + v[2] == 0 {
            continue; // 无票 ⇒ 保持 Null（未观测）
        }
        let max = v[0].max(v[1]).max(v[2]);
        // 平票优先 Null：宁可不断言，这是三态的核心纪律。
        let s = if v[1] == max {
            TriSign::Null
        } else if v[2] == max {
            TriSign::Outside
        } else {
            TriSign::Inside
        };
        let x = (i % n) as u32;
        let y = ((i / n) % n) as u32;
        let z = (i / n2) as u32;
        field.set(x, y, z, s);
    }

    Ok((field, grid))
}

/// 三态场的体素统计（验收用）：`[inside, null, outside]`。
pub fn tri_sign_counts(field: &SignField3D) -> [usize; 3] {
    field.counts()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gauss::Gaussian;
    use crate::gauss::voxel::VoxelizeOptions;

    fn plane_gaussians() -> Vec<Gaussian> {
        // 一块近似平面：法线全朝 +z，中心分布在 z=0 的平面上。
        let mut out = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                out.push(Gaussian {
                    mean: [(i as f32 - 1.0) * 0.25, (j as f32 - 1.0) * 0.25, 0.0],
                    scale: [0.06, 0.06, 0.01],
                    rot: [1.0, 0.0, 0.0, 0.0],
                    opacity: 1.0,
                    color: [1.0, 1.0, 1.0],
                    normal: Some([0.0, 0.0, 1.0]),
                });
            }
        }
        out
    }

    #[test]
    fn vote_produces_all_three_states() {
        let gs = plane_gaussians();
        let opts = VoxelizeOptions {
            n: 16,
            ..VoxelizeOptions::default()
        };
        // band 需覆盖至少一层体素（h≈0.06），否则没有体素落在薄壳带内。
        let (f, _g) = tri_sign_field(&gs, &opts, Some(0.06)).expect("三态体素化");
        let c = tri_sign_counts(&f);
        assert!(c[0] > 0, "应有 inside 票：{c:?}");
        assert!(c[1] > 0, "应有 null 带：{c:?}");
        assert!(c[2] > 0, "应有 outside 票：{c:?}");
    }

    #[test]
    fn narrow_band_gives_thinner_null_layer() {
        let gs = plane_gaussians();
        let base = VoxelizeOptions {
            n: 16,
            ..VoxelizeOptions::default()
        };
        let (wide, _) = tri_sign_field(&gs, &base, Some(0.05)).expect("宽带");
        let (narrow, _) = tri_sign_field(&gs, &base, Some(0.01)).expect("窄带");
        assert!(
            narrow.counts()[1] <= wide.counts()[1],
            "窄带的 Null 体素不应多于宽带：narrow={} wide={}",
            narrow.counts()[1],
            wide.counts()[1]
        );
    }
}
