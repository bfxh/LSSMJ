//! 高斯 → 标量场。两种口径：
//! - `Density`：α·exp(-½ Mahalanobis) 的核密度累加，阈值化后取等值面。
//! - `Surfel`：以最薄轴为法线，把每个高斯当作带权平面样本做 TSDF 融合（2DGS/带法线资产）。
//!
//! 共同约束：采样核在**法线/最薄轴上至少有一格宽**（`kernel_clamp_cells` /
//! `normal_band_cells`）。否则薄于体素的核会在格点上采样成空场，Surface Nets 无面可提。

use super::{Aabb, Gaussian, bounds_of, dot, normalize_vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelMode {
    Density,
    Surfel,
}

impl VoxelMode {
    pub fn name(self) -> &'static str {
        match self {
            VoxelMode::Density => "density",
            VoxelMode::Surfel => "surfel",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub n: u32,
    pub origin: [f32; 3],
    pub h: f32,
}

impl Grid {
    /// 自动取景：数据包围盒居中放进 n³ 立方域，四周留 `padding` 格。
    pub fn fit(bounds: Aabb, n: u32, padding: u32) -> Result<Self, String> {
        if bounds.is_empty() {
            return Err("包围盒为空（没有可用的高斯）".into());
        }
        if n < 8 {
            return Err(format!("grid 太小：{n}（最小 8）"));
        }
        let inner = (n - 1) as f32 - 2.0 * padding as f32;
        if inner <= 1.0 {
            return Err(format!("padding={padding} 对 grid={n} 太大"));
        }
        let extent = bounds.extent();
        let max_ext = extent.into_iter().fold(0f32, f32::max);
        if max_ext <= 0.0 || !max_ext.is_finite() {
            return Err("包围盒退化（边长为 0）".into());
        }
        let h = max_ext / inner;
        let center = bounds.center();
        let mut origin = [0f32; 3];
        for i in 0..3 {
            origin[i] = center[i] - 0.5 * (n - 1) as f32 * h;
        }
        Ok(Self { n, origin, h })
    }

    pub fn domain(&self) -> Aabb {
        let e = (self.n - 1) as f32 * self.h;
        Aabb {
            min: self.origin,
            max: [self.origin[0] + e, self.origin[1] + e, self.origin[2] + e],
        }
    }

    pub fn world_of(&self, x: u32, y: u32, z: u32) -> [f32; 3] {
        [
            self.origin[0] + x as f32 * self.h,
            self.origin[1] + y as f32 * self.h,
            self.origin[2] + z as f32 * self.h,
        ]
    }

    pub fn linear(&self, x: u32, y: u32, z: u32) -> usize {
        (x + y * self.n + z * self.n * self.n) as usize
    }
}

#[derive(Clone, Debug)]
pub struct VoxelizeOptions {
    pub mode: VoxelMode,
    pub n: u32,
    pub padding: u32,
    /// 密度场：阈值 = iso_frac × 正密度的 99.9 分位；0 表示使用 `iso_abs`。
    pub iso_frac: f32,
    pub iso_abs: Option<f32>,
    /// 密度核在最薄轴上的标准差下限（格）。
    pub kernel_clamp_cells: f32,
    /// 面元融合在最薄轴（法线方向）上的标准差下限（格）。
    pub normal_band_cells: f32,
    /// 面元 TSDF 累积的最小权重；低于它的体素视为未观测。
    pub min_weight: f32,
    /// 未知区封闭：从域边界泛洪标记外部，被围住的未知区填成内部。
    /// 关掉会得到"壳体"（内外两张面），打开得到实心体（默认）。
    pub seal: bool,
    /// 手动取景（世界坐标）；None = 自动按 3σ 包围盒取景。
    pub aabb: Option<Aabb>,
}

impl Default for VoxelizeOptions {
    fn default() -> Self {
        Self {
            mode: VoxelMode::Density,
            n: 64,
            padding: 3,
            iso_frac: 0.65,
            iso_abs: None,
            kernel_clamp_cells: 0.75,
            normal_band_cells: 1.0,
            min_weight: 0.02,
            seal: true,
            aabb: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    /// 标量场，负值=内部（两种模式同口径）。
    pub values: Vec<f32>,
    pub grid: Grid,
    pub mode: VoxelMode,
    /// 密度模式=实际阈值；面元模式=0。
    pub iso: f32,
    /// 被场覆盖（密度 > iso / TSDF 权重足够）的体素数。
    pub observed: usize,
    /// 封闭填充的体素数（未知且被围住 → 内部）；0 = 壳体或开放面。
    pub sealed_interior: usize,
    /// 自动/手动取景实际使用的包围盒。
    pub bounds: Aabb,
}

pub fn voxelize(gaussians: &[Gaussian], opts: &VoxelizeOptions) -> Result<Field, String> {
    if gaussians.is_empty() {
        return Err("没有高斯可体素化".into());
    }
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    match opts.mode {
        VoxelMode::Density => voxelize_density(gaussians, opts, grid, bounds),
        VoxelMode::Surfel => voxelize_surfel(gaussians, opts, grid, bounds),
    }
}

struct SplatRange {
    lo: [u32; 3],
    hi: [u32; 3],
    empty: bool,
}

fn splat_range(grid: &Grid, center: [f32; 3], radius: [f32; 3]) -> SplatRange {
    let n = grid.n as i64;
    let mut lo = [0u32; 3];
    let mut hi = [0u32; 3];
    let mut empty = false;
    for i in 0..3 {
        let a = ((center[i] - radius[i] - grid.origin[i]) / grid.h).floor() as i64;
        let b = ((center[i] + radius[i] - grid.origin[i]) / grid.h).ceil() as i64;
        if b < 0 || a > n - 1 {
            empty = true;
            break;
        }
        lo[i] = a.max(0) as u32;
        hi[i] = b.min(n - 1) as u32;
    }
    SplatRange { lo, hi, empty }
}

fn max_maha(eigen: &super::Eigen, lam: [f32; 3], d: [f32; 3]) -> f32 {
    let mut m = 0f32;
    for (v, l) in eigen.vectors.iter().zip(lam) {
        let y = dot(*v, d);
        m += y * y / l;
    }
    m
}

fn world_sigma(eigen: &super::Eigen, lam: [f32; 3], axis: usize) -> f32 {
    let mut v = 0f32;
    for (vec, l) in eigen.vectors.iter().zip(lam) {
        let c = vec[axis];
        v += c * c * l;
    }
    3.0 * v.max(0.0).sqrt()
}

fn quantile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn voxelize_density(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    let count = (grid.n * grid.n * grid.n) as usize;
    let mut density = vec![0f32; count];
    let min_var = (opts.kernel_clamp_cells.max(0.001) * grid.h).powi(2);
    for g in gaussians {
        if !g.is_finite() || g.opacity <= 0.0 {
            continue;
        }
        let eigen = g.eigen();
        let lam = [
            eigen.values[0].max(min_var),
            eigen.values[1].max(min_var),
            eigen.values[2].max(min_var),
        ];
        let radius = [
            world_sigma(&eigen, lam, 0),
            world_sigma(&eigen, lam, 1),
            world_sigma(&eigen, lam, 2),
        ];
        let range = splat_range(&grid, g.mean, radius);
        if range.empty {
            continue;
        }
        let opacity = g.opacity.clamp(0.0, 1.0);
        for z in range.lo[2]..=range.hi[2] {
            for y in range.lo[1]..=range.hi[1] {
                for x in range.lo[0]..=range.hi[0] {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&eigen, lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    density[grid.linear(x, y, z)] += opacity * (-0.5 * m).exp();
                }
            }
        }
    }
    let mut positive: Vec<f32> = density.iter().copied().filter(|v| *v > 0.0).collect();
    if positive.is_empty() {
        return Err("密度场全为 0（高斯是否落在域外？）".into());
    }
    positive.sort_by(f32::total_cmp);
    let q999 = quantile(&positive, 0.999);
    let iso = match opts.iso_abs {
        Some(v) => v,
        None => {
            if opts.iso_frac <= 0.0 {
                return Err("iso_frac 必须 > 0（或用 iso_abs 指定绝对阈值）".into());
            }
            opts.iso_frac * q999
        }
    };
    if iso <= 0.0 || !iso.is_finite() {
        return Err(format!("无效阈值 iso={iso}"));
    }
    let observed = density.iter().filter(|v| **v > iso).count();
    if observed == 0 {
        return Err(format!("阈值 iso={iso} 过高：没有任何体素被占据"));
    }
    let known: Vec<bool> = density.iter().map(|v| *v > iso).collect();
    let outside = q999.max(1e-6);
    let mut values: Vec<f32> = density.into_iter().map(|v| iso - v).collect();
    let sealed_interior = if opts.seal {
        seal_unknown(&mut values, &known, &grid, outside)
    } else {
        for (v, k) in values.iter_mut().zip(&known) {
            if !*k {
                *v = outside;
            }
        }
        0
    };
    Ok(Field {
        values,
        grid,
        mode: VoxelMode::Density,
        iso,
        observed,
        sealed_interior,
        bounds,
    })
}

fn voxelize_surfel(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    let count = (grid.n * grid.n * grid.n) as usize;
    let mut num = vec![0f32; count];
    let mut den = vec![0f32; count];
    let centroid = {
        let mut c = [0f32; 3];
        let mut wsum = 0f32;
        for g in gaussians {
            let w = g.opacity.max(0.0);
            for (ci, mi) in c.iter_mut().zip(g.mean) {
                *ci += mi * w;
            }
            wsum += w;
        }
        if wsum > 0.0 {
            [c[0] / wsum, c[1] / wsum, c[2] / wsum]
        } else {
            [0f32; 3]
        }
    };
    let normal_sigma = (opts.normal_band_cells.max(0.25) * grid.h).powi(2);
    let tangent_floor = (0.25 * grid.h).powi(2);
    let clamp_abs = 3.0 * grid.h;
    for g in gaussians {
        if !g.is_finite() || g.opacity <= 0.0 {
            continue;
        }
        let eigen = g.eigen();
        let mut normal = match g.normal {
            Some(n) => normalize_vec(n),
            None => {
                let n = eigen.vectors[0];
                let away = [
                    g.mean[0] - centroid[0],
                    g.mean[1] - centroid[1],
                    g.mean[2] - centroid[2],
                ];
                if dot(n, away) < 0.0 {
                    [-n[0], -n[1], -n[2]]
                } else {
                    n
                }
            }
        };
        normal = normalize_vec(normal);
        // 最薄轴就是法线轴；资产给出的法线可能与最薄轴有偏差，不强制改轴。
        let lam = [
            eigen.values[0].max(normal_sigma),
            eigen.values[1].max(tangent_floor),
            eigen.values[2].max(tangent_floor),
        ];
        let radius = [
            world_sigma(&eigen, lam, 0),
            world_sigma(&eigen, lam, 1),
            world_sigma(&eigen, lam, 2),
        ];
        let range = splat_range(&grid, g.mean, radius);
        if range.empty {
            continue;
        }
        let opacity = g.opacity.clamp(0.0, 1.0);
        for z in range.lo[2]..=range.hi[2] {
            for y in range.lo[1]..=range.hi[1] {
                for x in range.lo[0]..=range.hi[0] {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&eigen, lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    let dist = dot(d, normal).clamp(-clamp_abs, clamp_abs);
                    let w = opacity * (-0.5 * m).exp();
                    let idx = grid.linear(x, y, z);
                    num[idx] += w * dist;
                    den[idx] += w;
                }
            }
        }
    }
    let outside = 3.0 * grid.h;
    let mut values = Vec::with_capacity(count);
    let mut known = vec![false; count];
    let mut observed = 0usize;
    for i in 0..count {
        if den[i] >= opts.min_weight && den[i] > 0.0 {
            values.push((num[i] / den[i]).clamp(-outside, outside));
            known[i] = true;
            observed += 1;
        } else {
            values.push(outside);
        }
    }
    if observed == 0 {
        return Err("面元融合没有观测到任何体素（法线不可信或带宽太小）".into());
    }
    let sealed_interior = if opts.seal {
        seal_unknown(&mut values, &known, &grid, outside)
    } else {
        0
    };
    Ok(Field {
        values,
        grid,
        mode: VoxelMode::Surfel,
        iso: 0.0,
        observed,
        sealed_interior,
        bounds,
    })
}

/// 把未观测体素分成"连通到边界的外区"与"被围住的内区"：
/// 外区写 +outside，内区写 −outside。返回被填成内部的体素数。
///
/// 这是 TSDF 体融合的符号传播；开放面会让泛洪漏进内部（sealed_interior≈0），
/// 上层应据此告警而不是静默出一个破面。
fn seal_unknown(values: &mut [f32], known: &[bool], grid: &Grid, outside: f32) -> usize {
    use std::collections::VecDeque;
    let n = grid.n;
    let n_usize = n as usize;
    let count = n_usize * n_usize * n_usize;
    let mut outside_mask = vec![false; count];
    let mut queue = VecDeque::new();
    let push = |idx: usize, mask: &mut [bool], q: &mut VecDeque<usize>| {
        if !known[idx] && !mask[idx] {
            mask[idx] = true;
            q.push_back(idx);
        }
    };
    for z in 0..n_usize {
        for y in 0..n_usize {
            for x in 0..n_usize {
                if x == 0
                    || y == 0
                    || z == 0
                    || x + 1 == n_usize
                    || y + 1 == n_usize
                    || z + 1 == n_usize
                {
                    let idx = x + y * n_usize + z * n_usize * n_usize;
                    push(idx, &mut outside_mask, &mut queue);
                }
            }
        }
    }
    while let Some(idx) = queue.pop_front() {
        let x = idx % n_usize;
        let y = (idx / n_usize) % n_usize;
        let z = idx / (n_usize * n_usize);
        if x > 0 {
            push(idx - 1, &mut outside_mask, &mut queue);
        }
        if x + 1 < n_usize {
            push(idx + 1, &mut outside_mask, &mut queue);
        }
        if y > 0 {
            push(idx - n_usize, &mut outside_mask, &mut queue);
        }
        if y + 1 < n_usize {
            push(idx + n_usize, &mut outside_mask, &mut queue);
        }
        if z > 0 {
            push(idx - n_usize * n_usize, &mut outside_mask, &mut queue);
        }
        if z + 1 < n_usize {
            push(idx + n_usize * n_usize, &mut outside_mask, &mut queue);
        }
    }
    let mut interior = 0usize;
    for i in 0..count {
        if !known[i] {
            if outside_mask[i] {
                values[i] = outside;
            } else {
                values[i] = -outside;
                interior += 1;
            }
        }
    }
    interior
}

/// 场的统计读数（给报告/UX 用）。
pub fn field_percentiles(field: &Field) -> [f32; 3] {
    let mut v: Vec<f32> = field
        .values
        .iter()
        .copied()
        .filter(|x| x.is_finite())
        .collect();
    if v.is_empty() {
        return [0.0; 3];
    }
    v.sort_by(f32::total_cmp);
    [quantile(&v, 0.01), quantile(&v, 0.5), quantile(&v, 0.99)]
}
