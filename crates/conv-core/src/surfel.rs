//! mesh→高斯（自研缺口④第一片）：**表面采样 + 面元协方差（退化 surfel）**——
//! 矩阵边 9（"工程直接；需自研（小）"，锚 `W15B-013` SuGaR binding 的表面采样思路）。
//!
//! 口径（钉死；改口径 = 改契约）：
//! - 采样：每三角 `n = max(1, ceil(area / spacing²))` 个；重心坐标走**黄金比低差异序列**：
//!   `u = frac((k+0.5)·α)`、`v = frac((k+0.5)·β)`（α = 0.6180339887498949，
//!   β = 0.7548776662466927），`u+v>1 ⇒ (u,v)=(1−u,1−v)`（折回三角内）——确定性、无随机源。
//!   退化三角（面积 < 1e-12）跳过。
//! - 朝向：n̂ = normalize((p₁−p₀)×(p₂−p₀))；切向基 t₁ = normalize(n̂×ê_r)
//!   （ê_r = |n̂| 分量最小的坐标轴，数值稳定 выбор），t₂ = n̂×t₁——右手系 (t₁, t₂, n̂)。
//! - 协方差（退化 surfel）：σ_t = spacing/2（切向两轴同宽）、σ_n = thickness（法向薄）；
//!   log_scales = ln σ（3DGS log 域）。
//! - 占位属性（无材质源）：opacity = 1.0、颜色 = 中性灰 0.5——材质贯通属后续片。

use crate::gaussian::GaussianCloud;
use crate::kernels::quat_from_cols;

const ALPHA: f32 = 0.618_033_988_749_894_9_f64 as f32;
const BETA: f32 = 0.754_877_666_246_692_7_f64 as f32;

/// 面元参数（世界单位）。
#[derive(Clone, Copy)]
pub struct SurfelParams {
    /// 面内目标采样间距（每三角采样数 = ceil(area / spacing²)）
    pub spacing: f32,
    /// 法向厚度（退化 surfel 的 σ_n）
    pub thickness: f32,
}

/// mesh → 高斯云（planar；逐顶点 = 一枚退化 surfel）。
pub fn mesh_to_gaussians(
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
    p: &SurfelParams,
) -> GaussianCloud {
    let mut positions = Vec::new();
    let mut log_scales = Vec::new();
    let mut rotations = Vec::new();
    let mut opacities = Vec::new();
    let mut colors = Vec::new();

    let sig_t = p.spacing * 0.5;
    let ln_t = sig_t.ln();
    let ln_n = p.thickness.ln();

    for f in faces {
        let p0 = verts[f[0] as usize];
        let p1 = verts[f[1] as usize];
        let p2 = verts[f[2] as usize];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let cr = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area = 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        if area < 1e-12 {
            continue;
        }
        let n = ((area / (p.spacing * p.spacing)).ceil() as usize).max(1);
        // 法向 + 数值稳定切向基
        let nl = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let nrm = [cr[0] / nl, cr[1] / nl, cr[2] / nl];
        let mut ref_i = 0usize;
        let mut ref_v = nrm[0].abs();
        for (i, &c) in nrm.iter().enumerate() {
            if c.abs() < ref_v {
                ref_v = c.abs();
                ref_i = i;
            }
        }
        let e_ref = match ref_i {
            0 => [1.0, 0.0, 0.0],
            1 => [0.0, 1.0, 0.0],
            _ => [0.0, 0.0, 1.0],
        };
        let t1r = [
            nrm[1] * e_ref[2] - nrm[2] * e_ref[1],
            nrm[2] * e_ref[0] - nrm[0] * e_ref[2],
            nrm[0] * e_ref[1] - nrm[1] * e_ref[0],
        ];
        let t1l = (t1r[0] * t1r[0] + t1r[1] * t1r[1] + t1r[2] * t1r[2]).sqrt();
        let t1 = [t1r[0] / t1l, t1r[1] / t1l, t1r[2] / t1l];
        let t2 = [
            nrm[1] * t1[2] - nrm[2] * t1[1],
            nrm[2] * t1[0] - nrm[0] * t1[2],
            nrm[0] * t1[1] - nrm[1] * t1[0],
        ];
        let q = quat_from_cols(&[t1, t2, nrm]);

        for k in 0..n {
            let s = k as f32 + 0.5;
            let mut u = (s * ALPHA).fract();
            let mut v = (s * BETA).fract();
            if u + v > 1.0 {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            positions.push([
                p0[0] + u * e1[0] + v * e2[0],
                p0[1] + u * e1[1] + v * e2[1],
                p0[2] + u * e1[2] + v * e2[2],
            ]);
            log_scales.push([ln_t, ln_t, ln_n]);
            rotations.push(q);
            opacities.push(1.0);
            colors.push([0.5, 0.5, 0.5]);
        }
    }

    GaussianCloud::new(positions, log_scales, rotations, opacities, colors)
}
