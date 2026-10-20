//! 高斯腿（T-GC-04 扩展片）：3DGS 原始数据 → 标量场 → 网格。
//!
//! 口径：高斯本身没有拓扑，转网格必须先选一个可度量的标量场再等值面提取。
//! 本模块提供两种场：`Density`（不透明度核密度，原始 3DGS 的保守路线）与
//! `Surfel`（以最薄轴为法线做加权 TSDF 融合，2DGS/带法线资产的路线）。
//! 两者都输出"负值=内部"的场，交给 [`crate::gauss::mesh`] 做 Surface Nets。

pub mod gpu;
pub mod mesh;
pub mod ply;
mod simd;
pub mod voxel;

use std::f32::consts::PI;

/// 3DGS 基元。`scale` 为已激活的标准差（PLY 里的 log-scale 已取 exp），
/// `rot` 为 [w, x, y, z]（PLY 的 rot_0..3 顺序），`opacity` 为已激活值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gaussian {
    pub mean: [f32; 3],
    pub scale: [f32; 3],
    pub rot: [f32; 4],
    pub opacity: f32,
    pub color: [f32; 3],
    /// 资产自带法线（2DGS/SuGaR 一类导出可能有）；原始 3DGS 没有。
    pub normal: Option<[f32; 3]>,
}

impl Gaussian {
    pub fn covariance(&self) -> [[f32; 3]; 3] {
        let r = quat_to_mat(normalize_quat(self.rot));
        let s2 = [
            self.scale[0] * self.scale[0],
            self.scale[1] * self.scale[1],
            self.scale[2] * self.scale[2],
        ];
        let mut cov = [[0f32; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] = r[i][0] * s2[0] * r[j][0]
                    + r[i][1] * s2[1] * r[j][1]
                    + r[i][2] * s2[2] * r[j][2];
            }
        }
        cov
    }

    /// 协方差特征分解（升序）。values[0] 对应最薄轴，即面元的法线方向。
    pub fn eigen(&self) -> Eigen {
        jacobi_eigen(self.covariance())
    }

    /// 场景世界轴上的 3σ 半径（用于包围盒与 splat 范围）。
    pub fn axis_radius(&self) -> [f32; 3] {
        let c = self.covariance();
        [
            3.0 * c[0][0].max(0.0).sqrt(),
            3.0 * c[1][1].max(0.0).sqrt(),
            3.0 * c[2][2].max(0.0).sqrt(),
        ]
    }

    pub fn is_finite(&self) -> bool {
        self.mean.iter().all(|v| v.is_finite())
            && self.scale.iter().all(|v| v.is_finite() && *v > 0.0)
            && self.rot.iter().all(|v| v.is_finite())
            && self.opacity.is_finite()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Eigen {
    /// 升序特征值。
    pub values: [f32; 3],
    /// vectors[i] 对应 values[i]，单位长度。
    pub vectors: [[f32; 3]; 3],
}

/// 对称 3×3 Jacobi 特征分解（确定性：固定扫描顺序与迭代上限）。
pub fn jacobi_eigen(a: [[f32; 3]; 3]) -> Eigen {
    let mut a = a;
    let mut v = [[1f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..32 {
        let mut p = 0usize;
        let mut q = 1usize;
        let mut max = a[0][1].abs();
        if a[0][2].abs() > max {
            p = 0;
            q = 2;
            max = a[0][2].abs();
        }
        if a[1][2].abs() > max {
            p = 1;
            q = 2;
            max = a[1][2].abs();
        }
        if max < 1e-10 {
            break;
        }
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        let theta = 0.5 * (aqq - app) / apq;
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        // a ← Jᵀ a J（只动 p/q 行列）
        let mut b = a;
        b[p][p] = app - t * apq;
        b[q][q] = aqq + t * apq;
        b[p][q] = 0.0;
        b[q][p] = 0.0;
        for k in 0..3 {
            if k != p && k != q {
                let akp = a[k][p];
                let akq = a[k][q];
                b[k][p] = c * akp - s * akq;
                b[p][k] = b[k][p];
                b[k][q] = s * akp + c * akq;
                b[q][k] = b[k][q];
            }
        }
        a = b;
        for row in &mut v {
            let vkp = row[p];
            let vkq = row[q];
            row[p] = c * vkp - s * vkq;
            row[q] = s * vkp + c * vkq;
        }
    }
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let mut values = [0f32; 3];
    let mut vectors = [[0f32; 3]; 3];
    for (dst, &src) in order.iter().enumerate() {
        values[dst] = a[src][src];
        vectors[dst] = normalize_vec([v[0][src], v[1][src], v[2][src]]);
    }
    Eigen { values, vectors }
}

/// [w, x, y, z] → 旋转矩阵。
pub fn quat_to_mat(q: [f32; 4]) -> [[f32; 3]; 3] {
    let [w, x, y, z] = q;
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

pub fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-12 || !n.is_finite() {
        [1.0, 0.0, 0.0, 0.0]
    } else {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    }
}

pub fn normalize_vec(v: [f32; 3]) -> [f32; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-12 || !n.is_finite() {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// 零阶球谐 → 线性颜色（3DGS 的 f_dc 口径）。
pub fn sh_dc_to_rgb(dc: [f32; 3]) -> [f32; 3] {
    const SH_C0: f32 = 0.282_094_8;
    [
        (0.5 + SH_C0 * dc[0]).clamp(0.0, 1.0),
        (0.5 + SH_C0 * dc[1]).clamp(0.0, 1.0),
        (0.5 + SH_C0 * dc[2]).clamp(0.0, 1.0),
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn empty() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    pub fn from_points(points: &[[f32; 3]]) -> Self {
        let mut b = Self::empty();
        for p in points {
            b.union_point(*p);
        }
        b
    }

    pub fn union_point(&mut self, p: [f32; 3]) {
        for ((mn, mx), c) in self.min.iter_mut().zip(self.max.iter_mut()).zip(p) {
            *mn = mn.min(c);
            *mx = mx.max(c);
        }
    }

    pub fn union(&mut self, other: &Aabb) {
        self.union_point(other.min);
        self.union_point(other.max);
    }

    pub fn is_empty(&self) -> bool {
        (0..3).any(|i| {
            !matches!(
                self.max[i].partial_cmp(&self.min[i]),
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            )
        })
    }

    pub fn center(&self) -> [f32; 3] {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
            0.5 * (self.min[2] + self.max[2]),
        ]
    }

    pub fn extent(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    pub fn expand(&mut self, d: f32) {
        for i in 0..3 {
            self.min[i] -= d;
            self.max[i] += d;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PruneOptions {
    /// 低于该不透明度的基元丢弃（floaters 的主要来源）。
    pub min_opacity: f32,
    /// 半径（最大轴 3σ）低于该值的基元丢弃；0 = 关闭。
    pub min_radius: f32,
    /// 半径高于该值的基元丢弃（背景板/雾团）；0 = 关闭。
    pub max_radius: f32,
}

impl Default for PruneOptions {
    fn default() -> Self {
        Self {
            min_opacity: 0.05,
            min_radius: 0.0,
            max_radius: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PruneStats {
    pub input: usize,
    pub kept: usize,
    pub dropped_opacity: usize,
    pub dropped_radius: usize,
    pub dropped_invalid: usize,
}

pub fn prune(gaussians: &[Gaussian], opts: &PruneOptions) -> (Vec<Gaussian>, PruneStats) {
    let mut stats = PruneStats {
        input: gaussians.len(),
        ..PruneStats::default()
    };
    let mut kept = Vec::with_capacity(gaussians.len());
    for g in gaussians {
        if !g.is_finite() {
            stats.dropped_invalid += 1;
            continue;
        }
        if g.opacity < opts.min_opacity {
            stats.dropped_opacity += 1;
            continue;
        }
        let r = g.axis_radius().into_iter().fold(0f32, f32::max);
        if (opts.min_radius > 0.0 && r < opts.min_radius)
            || (opts.max_radius > 0.0 && r > opts.max_radius)
        {
            stats.dropped_radius += 1;
            continue;
        }
        kept.push(*g);
    }
    stats.kept = kept.len();
    (kept, stats)
}

/// 3σ 包围盒并集（用于自动取景）。
pub fn bounds_of(gaussians: &[Gaussian]) -> Aabb {
    let mut b = Aabb::empty();
    for g in gaussians {
        let r = g.axis_radius();
        b.union_point([g.mean[0] - r[0], g.mean[1] - r[1], g.mean[2] - r[2]]);
        b.union_point([g.mean[0] + r[0], g.mean[1] + r[1], g.mean[2] + r[2]]);
    }
    b
}

/// 最薄轴与中间轴的标准差之比中位数：越小说明越像面元。
pub fn median_flatness(gaussians: &[Gaussian]) -> f32 {
    let mut ratios: Vec<f32> = gaussians
        .iter()
        .map(|g| {
            let e = g.eigen();
            let thin = e.values[0].max(1e-20).sqrt();
            let mid = e.values[1].max(1e-20).sqrt();
            mid / thin
        })
        .filter(|v| v.is_finite())
        .collect();
    if ratios.is_empty() {
        return 1.0;
    }
    ratios.sort_by(f32::total_cmp);
    ratios[ratios.len() / 2]
}

fn color_for_normal(n: [f32; 3]) -> [f32; 3] {
    [
        0.35 + 0.55 * (0.5 + 0.5 * n[0]),
        0.35 + 0.55 * (0.5 + 0.5 * n[1]),
        0.40 + 0.55 * (0.5 + 0.5 * n[2]),
    ]
}

/// 球面：Fibonacci 均匀采样 + 朝外法线 + 扁平面元。
pub fn sphere_gaussians(count: usize, radius: f32, opacity: f32) -> Vec<Gaussian> {
    let count = count.max(1);
    let golden = 0.5 * (5.0f32.sqrt() - 1.0);
    let spacing = (4.0 * PI * radius * radius / count as f32).sqrt();
    let tangent = (1.7 * spacing).max(radius * 0.004);
    (0..count)
        .map(|i| {
            let zz = 1.0 - 2.0 * (i as f32 + 0.5) / count as f32;
            let r = (1.0 - zz * zz).max(0.0).sqrt();
            let th = 2.0 * PI * golden * i as f32;
            let n = [r * th.cos(), r * th.sin(), zz];
            Gaussian {
                mean: [radius * n[0], radius * n[1], radius * n[2]],
                scale: [tangent, tangent, (radius * 0.0015).max(1e-4)],
                rot: quat_from_z_to(n),
                opacity,
                color: color_for_normal(n),
                normal: Some(n),
            }
        })
        .collect()
}

/// 规则采样的圆环面（可判亏格 1）；法线=管截面朝外方向。
pub fn torus_gaussians(
    nu: usize,
    nv: usize,
    major: f32,
    minor: f32,
    opacity: f32,
) -> Vec<Gaussian> {
    let nu = nu.max(3);
    let nv = nv.max(3);
    let spacing = (2.0 * PI * major / nu as f32)
        .min(2.0 * PI * minor / nv as f32)
        .max(minor * 0.01);
    let tangent = 1.7 * spacing;
    let mut out = Vec::with_capacity(nu * nv);
    for i in 0..nu {
        let u = 2.0 * PI * (i as f32 + 0.5) / nu as f32;
        for j in 0..nv {
            let v = 2.0 * PI * (j as f32 + 0.5) / nv as f32;
            let n = [u.cos() * v.cos(), u.sin() * v.cos(), v.sin()];
            let rr = major + minor * v.cos();
            let mean = [rr * u.cos(), rr * u.sin(), minor * v.sin()];
            out.push(Gaussian {
                mean,
                scale: [tangent, tangent, (minor * 0.004).max(1e-4)],
                rot: quat_from_z_to(n),
                opacity,
                color: color_for_normal(n),
                normal: Some(n),
            });
        }
    }
    out
}

/// 各向同性高斯组成的厚壳（原始 3DGS 的替身：没有可靠法线，只能走密度场）。
pub fn blob_sphere_gaussians(
    count: usize,
    radius: f32,
    sigma: f32,
    opacity: f32,
    seed: u64,
) -> Vec<Gaussian> {
    let mut lcg = crate::Lcg::new(seed);
    let base = sphere_gaussians(count.max(1), radius, opacity);
    base.into_iter()
        .map(|g| {
            let jitter = sigma * 1.2;
            let mean = [
                g.mean[0] + (lcg.next01() - 0.5) * 2.0 * jitter,
                g.mean[1] + (lcg.next01() - 0.5) * 2.0 * jitter,
                g.mean[2] + (lcg.next01() - 0.5) * 2.0 * jitter,
            ];
            let c = [
                0.3 + 0.6 * lcg.next01(),
                0.3 + 0.6 * lcg.next01(),
                0.4 + 0.5 * lcg.next01(),
            ];
            Gaussian {
                mean,
                scale: [sigma, sigma, sigma],
                rot: [1.0, 0.0, 0.0, 0.0],
                opacity,
                color: c,
                normal: None,
            }
        })
        .collect()
}

/// 在数据包围盒外撒低不透明度 floaters（测剪枝与鲁棒性）。
pub fn add_floaters(gaussians: &mut Vec<Gaussian>, count: usize, seed: u64) {
    let b = bounds_of(gaussians);
    let c = b.center();
    let ext = b.extent().into_iter().fold(0f32, f32::max).max(1e-6);
    let mut lcg = crate::Lcg::new(seed);
    for _ in 0..count {
        let dir = normalize_vec([
            lcg.next01() * 2.0 - 1.0,
            lcg.next01() * 2.0 - 1.0,
            lcg.next01() * 2.0 - 1.0,
        ]);
        let r = ext * (0.9 + 1.5 * lcg.next01());
        let sigma = ext * 0.01;
        gaussians.push(Gaussian {
            mean: [c[0] + dir[0] * r, c[1] + dir[1] * r, c[2] + dir[2] * r],
            scale: [sigma, sigma, sigma],
            rot: [1.0, 0.0, 0.0, 0.0],
            opacity: 0.01,
            color: [0.9, 0.2, 0.2],
            normal: None,
        });
    }
}

/// 把局部 +z 转到 dir 的四元数。
pub fn quat_from_z_to(dir: [f32; 3]) -> [f32; 4] {
    let n = normalize_vec(dir);
    let z = [0.0, 0.0, 1.0];
    let d = dot(z, n).clamp(-1.0, 1.0);
    if d > 0.999_999 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    if d < -0.999_999 {
        return [0.0, 1.0, 0.0, 0.0];
    }
    let axis = normalize_vec(cross(z, n));
    let angle = d.acos();
    let s = (angle * 0.5).sin();
    [(angle * 0.5).cos(), axis[0] * s, axis[1] * s, axis[2] * s]
}
