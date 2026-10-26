//! 高斯 → 标量场。两种口径：
//! - `Density`：α·exp(-½ Mahalanobis) 的核密度累加，阈值化后取等值面。
//! - `Surfel`：以最薄轴为法线，把每个高斯当作带权平面样本做 TSDF 融合（2DGS/带法线资产）。
//!
//! 共同约束：采样核在**法线/最薄轴上至少有一格宽**（`kernel_clamp_cells` /
//! `normal_band_cells`）。否则薄于体素的核会在格点上采样成空场，Surface Nets 无面可提。

use super::{Aabb, Gaussian, bounds_of, dot, normalize_vec};

#[cfg(target_arch = "x86_64")]
mod avx2;
pub mod sparse_density;
pub mod sparse_surfel;
pub mod quant;
pub mod surface_blocks;
pub mod ternary;
mod tiles;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Gpu,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Gpu => "gpu",
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
    pub backend: Backend,
    /// CPU splat 的 3D tile 可选档（默认 z-slab；GPU 忽略）。
    pub bins3d: bool,
    /// 可选 SIMD prep（cyclic Jacobi + AVX2；默认标量逐位不变，opt-in 改哈希）。
    pub simd_prep: bool,
    pub n: u32,
    pub padding: u32,
    /// 密度场：阈值 = iso_frac × 正密度的 99.9 分位；0 表示使用 `iso_abs`。
    pub iso_frac: f32,
    pub iso_abs: Option<f32>,
    /// 密度核在最薄轴上的标准差下限（格）。
    pub kernel_clamp_cells: f32,
    /// 可选：用**世界单位**给核标准差下限（覆盖 `kernel_clamp_cells × h`）。
    /// 这样同一 σ 在不同 grid 下含义一致，密度场才可能跨分辨率对齐。
    pub kernel_clamp_world: Option<f32>,
    /// 可选：按**参考网格**折算核下限（`0.75 × h_ref`）。同一参考值在任何 grid 上
    /// 得到同一世界 σ，等价于「把核宽固定成 N³ 时的样子」，无需按场景手算 σ。
    pub kernel_clamp_ref: Option<u32>,
    /// 面元融合在最薄轴（法线方向）上的标准差下限（格）。
    pub normal_band_cells: f32,
    /// 面元 TSDF 累积的最小权重；低于它的体素视为未观测。
    pub min_weight: f32,
    /// 未知区封闭：从域边界泛洪标记外部，被围住的未知区填成内部。
    /// 关掉会得到"壳体"（内外两张面），打开得到实心体（默认）。
    pub seal: bool,
    /// splat 工作线程数；1 = 单线程参照（逐位基线）。实际值会夹到 `[1, n]`。
    pub threads: usize,
    /// 手动取景（世界坐标）；None = 自动按 3σ 包围盒取景。
    pub aabb: Option<Aabb>,
    /// 仅 `gauss2mesh` 使用：走 u16 定点密度（同面、1.1×、内存 1/2）。库侧 `voxelize` 忽略。
    pub quant16: bool,
    /// 显式 u16 scale；None = 由探针标定（多一趟 splat）。
    pub quant_scale: Option<f32>,
}

impl Default for VoxelizeOptions {
    fn default() -> Self {
        Self {
            mode: VoxelMode::Density,
            backend: Backend::Cpu,
            bins3d: false,
            simd_prep: false,
            n: 64,
            padding: 3,
            iso_frac: 0.65,
            iso_abs: None,
            kernel_clamp_cells: 0.75,
            kernel_clamp_world: None,
            kernel_clamp_ref: None,
            normal_band_cells: 1.0,
            min_weight: 0.02,
            seal: true,
            threads: 1,
            aabb: None,
            quant16: false,
            quant_scale: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    /// 标量场，负值=内部（两种模式同口径）。
    pub values: Vec<f32>,
    pub grid: Grid,
    pub mode: VoxelMode,
    pub backend: Backend,
    /// 本次是否走了 CPU 3D tile 可选档。
    pub bins3d: bool,
    /// 本次 prep 是否走了可选 SIMD/cyclic 路径。
    pub simd_prep: bool,
    /// 密度模式=实际阈值；面元模式=0。
    pub iso: f32,
    /// 被场覆盖（密度 > iso / TSDF 权重足够）的体素数。
    pub observed: usize,
    /// 封闭填充的体素数（未知且被围住 → 内部）；0 = 壳体或开放面。
    pub sealed_interior: usize,
    /// 实际使用的 splat 工作线程数。
    pub workers: usize,
    /// 自动/手动取景实际使用的包围盒。
    pub bounds: Aabb,
}

fn fit_inputs(gaussians: &[Gaussian], opts: &VoxelizeOptions) -> Result<(Grid, Aabb), String> {
    if gaussians.is_empty() {
        return Err("没有高斯可体素化".into());
    }
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    Ok((grid, bounds))
}

pub use quant::{
    QuantField, quantize_auto_quantfield, quantize_field_auto, scale_for_iso, voxelize_quant,
};
pub use ternary::{TritField, voxelize_ternary, voxelize_ternary_with};

pub fn voxelize(gaussians: &[Gaussian], opts: &VoxelizeOptions) -> Result<Field, String> {
    let (grid, bounds) = fit_inputs(gaussians, opts)?;
    match opts.backend {
        Backend::Cpu => match opts.mode {
            VoxelMode::Density => voxelize_density(gaussians, opts, grid, bounds),
            VoxelMode::Surfel => voxelize_surfel(gaussians, opts, grid, bounds),
        },
        Backend::Gpu => super::gpu::voxelize_gpu(gaussians, opts, grid, bounds),
    }
}

/// 复用调用方提供的 GPU 设备（批处理/判据用；一次转换请用 [`voxelize`]）。
pub fn voxelize_gpu_with(
    headless: &crate::jfa::Headless,
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<Field, String> {
    let (grid, bounds) = fit_inputs(gaussians, opts)?;
    let mut opts = opts.clone();
    opts.backend = Backend::Gpu;
    super::gpu::voxelize_gpu_with(headless, gaussians, &opts, grid, bounds)
}

pub(crate) struct SplatRange {
    pub(crate) lo: [u32; 3],
    pub(crate) hi: [u32; 3],
    pub(crate) empty: bool,
}

pub(crate) fn splat_range(grid: &Grid, center: [f32; 3], radius: [f32; 3]) -> SplatRange {
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

/// 第 k 顺序统计量（O(n)），不排序整个数组；结果与排序后取同一下标逐位一致。
fn quantile_select(values: &mut [f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    let idx = idx.min(values.len() - 1);
    let (_, pivot, _) = values.select_nth_unstable_by(idx, f32::total_cmp);
    *pivot
}

#[derive(Clone, Copy)]
pub(crate) struct Prepared {
    pub(crate) mean: [f32; 3],
    pub(crate) opacity: f32,
    pub(crate) eigen: super::Eigen,
    pub(crate) lam: [f32; 3],
    pub(crate) radius: [f32; 3],
    pub(crate) normal: [f32; 3],
}

#[derive(Clone, Copy)]
struct Chunk {
    /// [z0, z1)
    z0: u32,
    z1: u32,
}

fn chunk_ranges(n: u32, chunks: usize) -> Vec<Chunk> {
    let chunks = chunks.clamp(1, n as usize);
    let base = n as usize / chunks;
    let rem = n as usize % chunks;
    let mut out = Vec::with_capacity(chunks);
    let mut z = 0u32;
    for i in 0..chunks {
        let h = (base + usize::from(i < rem)) as u32;
        out.push(Chunk { z0: z, z1: z + h });
        z += h;
    }
    out
}

/// z 层分块并行：每块输出独占，无原子/无锁；返回 (块号, 块结果)。
/// 分块只改变写入位置，不改变每个体素的 Gaussian 累加顺序。
fn parallel_map_chunks<T, F>(chunks: &[Chunk], workers: usize, f: F) -> Vec<(usize, T)>
where
    T: Send,
    F: Fn(usize, &Chunk) -> T + Sync,
{
    let workers = workers.min(chunks.len()).max(1);
    if workers <= 1 || chunks.len() <= 1 {
        return chunks
            .iter()
            .enumerate()
            .map(|(i, c)| (i, f(i, c)))
            .collect();
    }
    // 动态取块：各 z 块成本不均时避免静态轮转的尾部等待；输出按块号回收，顺序不变。
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let next = &next;
            let f = &f;
            handles.push(s.spawn(move || {
                let mut out = Vec::new();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= chunks.len() {
                        break;
                    }
                    out.push((i, f(i, &chunks[i])));
                }
                out
            }));
        }
        let mut all = Vec::new();
        for h in handles {
            match h.join() {
                Ok(v) => all.extend(v),
                Err(_) => panic!("splat worker panicked"),
            }
        }
        all
    })
}

fn prepare_density_one(g: &Gaussian, min_var: f32) -> Prepared {
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
    Prepared {
        mean: g.mean,
        opacity: g.opacity.clamp(0.0, 1.0),
        eigen,
        lam,
        radius,
        normal: [0.0; 3],
    }
}

fn surfel_normal(g: &Gaussian, eigen: &super::Eigen, centroid: [f32; 3]) -> [f32; 3] {
    let normal = match g.normal {
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
    normalize_vec(normal)
}

fn prepare_surfel_one(
    g: &Gaussian,
    centroid: [f32; 3],
    normal_sigma: f32,
    tangent_floor: f32,
) -> Prepared {
    let eigen = g.eigen();
    let normal = surfel_normal(g, &eigen, centroid);
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
    Prepared {
        mean: g.mean,
        opacity: g.opacity.clamp(0.0, 1.0),
        eigen,
        lam,
        radius,
        normal,
    }
}

/// 按输入顺序分块并行 prepare；每块内保持 Gaussian 顺序，拼回后与单线程逐位一致。
fn parallel_prepare<F>(gaussians: &[Gaussian], workers: usize, f: F) -> Vec<Prepared>
where
    F: Fn(&Gaussian) -> Prepared + Sync,
{
    let filtered: Vec<&Gaussian> = gaussians
        .iter()
        .filter(|g| g.is_finite() && g.opacity > 0.0)
        .collect();
    let n = filtered.len();
    let workers = workers.min(n).max(1);
    if workers <= 1 || n < 2048 {
        return filtered.iter().map(|g| f(g)).collect();
    }
    let chunk_size = n.div_ceil(workers);
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in filtered.chunks(chunk_size) {
            let f = &f;
            handles.push(s.spawn(move || chunk.iter().map(|g| f(g)).collect::<Vec<_>>()));
        }
        let mut out = Vec::with_capacity(n);
        for h in handles {
            match h.join() {
                Ok(v) => out.extend(v),
                Err(_) => panic!("prepare worker panicked"),
            }
        }
        out
    })
}

fn simd_prepared_density(filtered: &[&Gaussian], min_var: f32) -> Vec<Prepared> {
    let covs: Vec<[[f32; 3]; 3]> = filtered.iter().map(|g| g.covariance()).collect();
    let eigs = super::simd::eigen_batch(&covs);
    let mut out = Vec::with_capacity(filtered.len());
    for (g, eigen) in filtered.iter().zip(eigs) {
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
        out.push(Prepared {
            mean: g.mean,
            opacity: g.opacity.clamp(0.0, 1.0),
            eigen,
            lam,
            radius,
            normal: [0.0; 3],
        });
    }
    out
}

fn prepare_density_simd(gaussians: &[Gaussian], min_var: f32, workers: usize) -> Vec<Prepared> {
    let filtered: Vec<&Gaussian> = gaussians
        .iter()
        .filter(|g| g.is_finite() && g.opacity > 0.0)
        .collect();
    let n = filtered.len();
    let workers = workers.min(n).max(1);
    if workers <= 1 || n < 2048 {
        return simd_prepared_density(&filtered, min_var);
    }
    let chunk_size = n.div_ceil(workers);
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in filtered.chunks(chunk_size) {
            handles.push(s.spawn(move || simd_prepared_density(chunk, min_var)));
        }
        let mut out = Vec::with_capacity(n);
        for h in handles {
            match h.join() {
                Ok(v) => out.extend(v),
                Err(_) => panic!("simd prepare worker panicked"),
            }
        }
        out
    })
}

fn simd_prepared_surfel(
    filtered: &[&Gaussian],
    centroid: [f32; 3],
    normal_sigma: f32,
    tangent_floor: f32,
) -> Vec<Prepared> {
    let covs: Vec<[[f32; 3]; 3]> = filtered.iter().map(|g| g.covariance()).collect();
    let eigs = super::simd::eigen_batch(&covs);
    let mut out = Vec::with_capacity(filtered.len());
    for (g, eigen) in filtered.iter().zip(eigs) {
        let normal = surfel_normal(g, &eigen, centroid);
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
        out.push(Prepared {
            mean: g.mean,
            opacity: g.opacity.clamp(0.0, 1.0),
            eigen,
            lam,
            radius,
            normal,
        });
    }
    out
}

fn prepare_surfel_simd(
    gaussians: &[Gaussian],
    centroid: [f32; 3],
    normal_sigma: f32,
    tangent_floor: f32,
    workers: usize,
) -> Vec<Prepared> {
    let filtered: Vec<&Gaussian> = gaussians
        .iter()
        .filter(|g| g.is_finite() && g.opacity > 0.0)
        .collect();
    let n = filtered.len();
    let workers = workers.min(n).max(1);
    if workers <= 1 || n < 2048 {
        return simd_prepared_surfel(&filtered, centroid, normal_sigma, tangent_floor);
    }
    let chunk_size = n.div_ceil(workers);
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in filtered.chunks(chunk_size) {
            handles.push(
                s.spawn(move || simd_prepared_surfel(chunk, centroid, normal_sigma, tangent_floor)),
            );
        }
        let mut out = Vec::with_capacity(n);
        for h in handles {
            match h.join() {
                Ok(v) => out.extend(v),
                Err(_) => panic!("simd prepare worker panicked"),
            }
        }
        out
    })
}

pub(crate) fn prepare_density(
    gaussians: &[Gaussian],
    grid: &Grid,
    opts: &VoxelizeOptions,
) -> Vec<Prepared> {
    let min_var = if let Some(w) = opts.kernel_clamp_world {
        w.max(1e-6).powi(2)
    } else if let Some(r) = opts.kernel_clamp_ref {
        // 由当前 grid 反推参考网格的 h：h = h_cur × (n−1−2p) / (ref−1−2p)
        let inner_cur = (grid.n as f32 - 1.0 - 2.0 * opts.padding as f32).max(1.0);
        let inner_ref = (r as f32 - 1.0 - 2.0 * opts.padding as f32).max(1.0);
        let h_ref = grid.h * inner_cur / inner_ref;
        (opts.kernel_clamp_cells.max(0.001) * h_ref).powi(2)
    } else {
        (opts.kernel_clamp_cells.max(0.001) * grid.h).powi(2)
    };
    if opts.simd_prep {
        return prepare_density_simd(gaussians, min_var, opts.threads);
    }
    parallel_prepare(gaussians, opts.threads, |g| prepare_density_one(g, min_var))
}

pub(crate) fn prepare_surfel(
    gaussians: &[Gaussian],
    grid: &Grid,
    opts: &VoxelizeOptions,
) -> (Vec<Prepared>, f32) {
    let centroid = {
        let mut c = [0f32; 3];
        let mut wsum = 0f32;
        for g in gaussians {
            if !g.is_finite() {
                continue;
            }
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
    if opts.simd_prep {
        return (
            prepare_surfel_simd(
                gaussians,
                centroid,
                normal_sigma,
                tangent_floor,
                opts.threads,
            ),
            clamp_abs,
        );
    }
    let out = parallel_prepare(gaussians, opts.threads, |g| {
        prepare_surfel_one(g, centroid, normal_sigma, tangent_floor)
    });
    (out, clamp_abs)
}

/// 每个 z 块只保留与它相交的 Gaussian 下标（按 prepared 顺序，故累加顺序不变）。
fn build_bins(prepared: &[Prepared], grid: &Grid, chunks: &[Chunk]) -> Vec<Vec<u32>> {
    let n = grid.n as usize;
    let mut z_to_chunk = vec![0u32; n];
    for (ci, c) in chunks.iter().enumerate() {
        for z in c.z0..c.z1 {
            z_to_chunk[z as usize] = ci as u32;
        }
    }
    let mut bins: Vec<Vec<u32>> = (0..chunks.len()).map(|_| Vec::new()).collect();
    for (i, g) in prepared.iter().enumerate() {
        let range = splat_range(grid, g.mean, g.radius);
        if range.empty {
            continue;
        }
        let c0 = z_to_chunk[range.lo[2] as usize] as usize;
        let c1 = z_to_chunk[range.hi[2] as usize] as usize;
        for bin in &mut bins[c0..=c1] {
            bin.push(i as u32);
        }
    }
    bins
}

fn avx2_splat() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        if std::env::var_os("LSSMJ_NO_AVX2").is_some() {
            return false;
        }
        avx2::available()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn splat_density_chunk(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    chunk: Chunk,
    out: &mut [f32],
) {
    let n = grid.n as usize;
    let n2 = n * n;
    #[cfg(target_arch = "x86_64")]
    let simd = avx2_splat();
    for &gi in indices {
        let g = &prepared[gi as usize];
        let range = splat_range(grid, g.mean, g.radius);
        if range.empty {
            continue;
        }
        let z_lo = range.lo[2].max(chunk.z0);
        let z_hi = range.hi[2].min(chunk.z1 - 1);
        if z_lo > z_hi {
            continue;
        }
        for z in z_lo..=z_hi {
            let lz = (z - chunk.z0) as usize;
            for y in range.lo[1]..=range.hi[1] {
                let row = lz * n2 + y as usize * n;
                #[cfg(target_arch = "x86_64")]
                if simd {
                    unsafe {
                        avx2::density_row(g, grid, y, z, range.lo[0], range.hi[0], 0, out, row);
                    }
                    continue;
                }
                for x in range.lo[0]..=range.hi[0] {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    out[row + x as usize] += g.opacity * (-0.5 * m).exp();
                }
            }
        }
    }
}

fn splat_surfel_chunk(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    chunk: Chunk,
    clamp_abs: f32,
    num: &mut [f32],
    den: &mut [f32],
) {
    let n = grid.n as usize;
    let n2 = n * n;
    for &gi in indices {
        let g = &prepared[gi as usize];
        let range = splat_range(grid, g.mean, g.radius);
        if range.empty {
            continue;
        }
        let z_lo = range.lo[2].max(chunk.z0);
        let z_hi = range.hi[2].min(chunk.z1 - 1);
        if z_lo > z_hi {
            continue;
        }
        for z in z_lo..=z_hi {
            let lz = (z - chunk.z0) as usize;
            for y in range.lo[1]..=range.hi[1] {
                let row = lz * n2 + y as usize * n;
                for x in range.lo[0]..=range.hi[0] {
                    let p = grid.world_of(x, y, z);
                    let d = [p[0] - g.mean[0], p[1] - g.mean[1], p[2] - g.mean[2]];
                    let m = max_maha(&g.eigen, g.lam, d);
                    if m > 18.0 {
                        continue;
                    }
                    let dist = dot(d, g.normal).clamp(-clamp_abs, clamp_abs);
                    let w = g.opacity * (-0.5 * m).exp();
                    let idx = row + x as usize;
                    num[idx] += w * dist;
                    den[idx] += w;
                }
            }
        }
    }
}

fn parallel_splat_density(prepared: &[Prepared], grid: &Grid, workers: usize) -> (Vec<f32>, usize) {
    let n = grid.n as usize;
    let n2 = n * n;
    let chunk_count = if workers <= 1 {
        1
    } else {
        (workers * 2).min(n).max(1)
    };
    let chunks = chunk_ranges(grid.n, chunk_count);
    let workers = workers.min(chunks.len()).max(1);
    let bins = build_bins(prepared, grid, &chunks);
    let mut parts = parallel_map_chunks(&chunks, workers, |i, c| {
        let mut local = vec![0f32; n2 * (c.z1 - c.z0) as usize];
        splat_density_chunk(prepared, &bins[i], grid, *c, &mut local);
        local
    });
    parts.sort_by_key(|(i, _)| *i);
    let mut out = vec![0f32; n * n * n];
    for (i, local) in parts {
        let c = &chunks[i];
        let start = c.z0 as usize * n2;
        out[start..start + local.len()].copy_from_slice(&local);
    }
    (out, workers)
}

fn parallel_splat_surfel(
    prepared: &[Prepared],
    grid: &Grid,
    clamp_abs: f32,
    workers: usize,
) -> (Vec<f32>, Vec<f32>, usize) {
    let n = grid.n as usize;
    let n2 = n * n;
    let chunk_count = if workers <= 1 {
        1
    } else {
        (workers * 2).min(n).max(1)
    };
    let chunks = chunk_ranges(grid.n, chunk_count);
    let workers = workers.min(chunks.len()).max(1);
    let bins = build_bins(prepared, grid, &chunks);
    let mut parts = parallel_map_chunks(&chunks, workers, |i, c| {
        let len = n2 * (c.z1 - c.z0) as usize;
        let mut num = vec![0f32; len];
        let mut den = vec![0f32; len];
        splat_surfel_chunk(prepared, &bins[i], grid, *c, clamp_abs, &mut num, &mut den);
        (num, den)
    });
    parts.sort_by_key(|(i, _)| *i);
    let mut num_all = vec![0f32; n * n * n];
    let mut den_all = vec![0f32; n * n * n];
    for (i, (num, den)) in parts {
        let c = &chunks[i];
        let start = c.z0 as usize * n2;
        num_all[start..start + num.len()].copy_from_slice(&num);
        den_all[start..start + den.len()].copy_from_slice(&den);
    }
    (num_all, den_all, workers)
}

fn voxelize_density(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    let prepared = prepare_density(gaussians, &grid, opts);
    if prepared.is_empty() {
        return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
    }
    let (density, workers) = if opts.bins3d {
        tiles::splat_density_tiles(&prepared, &grid, opts.threads)
    } else {
        parallel_splat_density(&prepared, &grid, opts.threads)
    };
    density_post(density, opts, grid, bounds, workers, Backend::Cpu)
}

pub(crate) fn density_post(
    density: Vec<f32>,
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
    workers: usize,
    backend: Backend,
) -> Result<Field, String> {
    let mut positive: Vec<f32> = density.iter().copied().filter(|v| *v > 0.0).collect();
    if positive.is_empty() {
        return Err("密度场全为 0（高斯是否落在域外？）".into());
    }
    let q999 = quantile_select(&mut positive, 0.999);
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
        backend,
        bins3d: backend == Backend::Cpu && opts.bins3d,
        simd_prep: opts.simd_prep,
        iso,
        observed,
        sealed_interior,
        workers,
        bounds,
    })
}

fn voxelize_surfel(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    let (prepared, clamp_abs) = prepare_surfel(gaussians, &grid, opts);
    if prepared.is_empty() {
        return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
    }
    let (num, den, workers) = if opts.bins3d {
        tiles::splat_surfel_tiles(&prepared, &grid, clamp_abs, opts.threads)
    } else {
        parallel_splat_surfel(&prepared, &grid, clamp_abs, opts.threads)
    };
    surfel_post(num, den, opts, grid, bounds, workers, Backend::Cpu)
}

pub(crate) fn surfel_post(
    num: Vec<f32>,
    den: Vec<f32>,
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
    workers: usize,
    backend: Backend,
) -> Result<Field, String> {
    let count = (grid.n * grid.n * grid.n) as usize;
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
        backend,
        bins3d: backend == Backend::Cpu && opts.bins3d,
        simd_prep: opts.simd_prep,
        iso: 0.0,
        observed,
        sealed_interior,
        workers,
        bounds,
    })
}

/// 把未观测体素分成"连通到边界的外区"与"被围住的内区"：
/// 外区写 +outside，内区写 −outside。返回被填成内部的体素数。
///
/// 这是 TSDF 体融合的符号传播；开放面会让泛洪漏进内部（sealed_interior≈0），
/// 上层应据此告警而不是静默出一个破面。
pub(crate) fn seal_unknown(values: &mut [f32], known: &[bool], grid: &Grid, outside: f32) -> usize {
    let n = grid.n as usize;
    let n2 = n * n;
    // 扫描线填充：每个种子沿 x 扩成一段，再把上下左右 4 个邻行里与之重叠的未知段入栈。
    // 语义与逐体素 BFS 完全相同（同一连通分量），只是入栈次数从 O(体素) 降到 O(段)。
    let mut outside_mask = vec![false; n * n * n];
    let mut stack: Vec<(usize, usize, usize)> = Vec::with_capacity(n * 2);

    #[inline]
    fn push_row(
        mask: &mut [bool],
        known: &[bool],
        stack: &mut Vec<(usize, usize, usize)>,
        n: usize,
        y: usize,
        z: usize,
        x0: usize,
        x1: usize,
    ) {
        let row = z * n * n + y * n;
        let mut x = x0;
        while x <= x1 {
            while x <= x1 && (known[row + x] || mask[row + x]) {
                x += 1;
            }
            if x > x1 {
                break;
            }
            let start = x;
            while x <= x1 && !known[row + x] && !mask[row + x] {
                x += 1;
            }
            stack.push((y, z, start));
        }
    }

    // 种子：域边界的未知体素
    for z in 0..n {
        for y in 0..n {
            if z == 0 || z + 1 == n || y == 0 || y + 1 == n {
                push_row(&mut outside_mask, known, &mut stack, n, y, z, 0, n - 1);
            } else {
                push_row(&mut outside_mask, known, &mut stack, n, y, z, 0, 0);
                push_row(&mut outside_mask, known, &mut stack, n, y, z, n - 1, n - 1);
            }
        }
    }

    while let Some((y, z, sx)) = stack.pop() {
        let row = z * n2 + y * n;
        if known[row + sx] || outside_mask[row + sx] {
            continue;
        }
        let mut x0 = sx;
        while x0 > 0 && !known[row + x0 - 1] && !outside_mask[row + x0 - 1] {
            x0 -= 1;
        }
        let mut x1 = sx;
        while x1 + 1 < n && !known[row + x1 + 1] && !outside_mask[row + x1 + 1] {
            x1 += 1;
        }
        for x in x0..=x1 {
            outside_mask[row + x] = true;
        }
        if y > 0 {
            push_row(&mut outside_mask, known, &mut stack, n, y - 1, z, x0, x1);
        }
        if y + 1 < n {
            push_row(&mut outside_mask, known, &mut stack, n, y + 1, z, x0, x1);
        }
        if z > 0 {
            push_row(&mut outside_mask, known, &mut stack, n, y, z - 1, x0, x1);
        }
        if z + 1 < n {
            push_row(&mut outside_mask, known, &mut stack, n, y, z + 1, x0, x1);
        }
    }

    let mut interior = 0usize;
    for i in 0..n * n * n {
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

#[cfg(test)]
mod bench_post {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "diagnostic timing probe"]
    fn bench_quantile_select() {
        let n = 2_000_000usize;
        let mut lcg = crate::Lcg::new(1);
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(lcg.next01() * 100.0);
        }
        let mut sorted = v.clone();
        let t = Instant::now();
        sorted.sort_by(f32::total_cmp);
        let q_sort = quantile(&sorted, 0.999);
        let sort_ms = t.elapsed().as_secs_f64() * 1e3;

        let mut selected = v;
        let t = Instant::now();
        let q_sel = quantile_select(&mut selected, 0.999);
        let sel_ms = t.elapsed().as_secs_f64() * 1e3;

        assert_eq!(q_sort.to_bits(), q_sel.to_bits());
        println!(
            "n={n} sort={sort_ms:.2} ms select={sel_ms:.2} ms ratio={:.1}x",
            sort_ms / sel_ms
        );
    }
}

#[cfg(test)]
mod bench_prepare {
    use super::*;
    use crate::gauss::{bounds_of, sphere_gaussians};
    use std::time::Instant;

    fn assert_same(a: &[Prepared], b: &[Prepared]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.mean.map(f32::to_bits), y.mean.map(f32::to_bits));
            assert_eq!(x.opacity.to_bits(), y.opacity.to_bits());
            assert_eq!(x.lam.map(f32::to_bits), y.lam.map(f32::to_bits));
            assert_eq!(x.radius.map(f32::to_bits), y.radius.map(f32::to_bits));
            assert_eq!(x.normal.map(f32::to_bits), y.normal.map(f32::to_bits));
            assert_eq!(
                x.eigen.values.map(f32::to_bits),
                y.eigen.values.map(f32::to_bits)
            );
            for k in 0..3 {
                assert_eq!(
                    x.eigen.vectors[k].map(f32::to_bits),
                    y.eigen.vectors[k].map(f32::to_bits)
                );
            }
        }
    }

    #[test]
    #[ignore = "diagnostic timing probe"]
    fn bench_prepare_and_bins() {
        let gs = sphere_gaussians(100_000, 0.75, 0.99);
        let bounds = bounds_of(&gs);
        let grid = Grid::fit(bounds, 128, 3).unwrap();
        let one = VoxelizeOptions {
            n: 128,
            threads: 1,
            ..VoxelizeOptions::default()
        };
        let many = VoxelizeOptions {
            threads: 24,
            ..one.clone()
        };

        let t = Instant::now();
        let d1 = prepare_density(&gs, &grid, &one);
        let t1 = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let d2 = prepare_density(&gs, &grid, &many);
        let t2 = t.elapsed().as_secs_f64() * 1e3;
        assert_same(&d1, &d2);
        println!(
            "prepare density n={} 1T={t1:.1}ms 24T={t2:.1}ms ratio={:.2}x",
            gs.len(),
            t1 / t2
        );

        let mut simd = many.clone();
        simd.simd_prep = true;
        let t = Instant::now();
        let d3 = prepare_density(&gs, &grid, &simd);
        let t3 = t.elapsed().as_secs_f64() * 1e3;
        assert_eq!(d3.len(), d1.len());
        for (x, y) in d1.iter().zip(&d3) {
            for k in 0..3 {
                let scale = x.eigen.values[k].abs().max(1.0);
                assert!((x.eigen.values[k] - y.eigen.values[k]).abs() <= 1e-6 * scale);
            }
        }
        println!(
            "prepare density SIMD n={} {t3:.1}ms vs 1T {t1:.1}ms / 24T {t2:.1}ms speedup={:.2}x",
            gs.len(),
            t2 / t3
        );

        let t = Instant::now();
        let (s1, _) = prepare_surfel(&gs, &grid, &one);
        let t1 = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (s2, _) = prepare_surfel(&gs, &grid, &many);
        let t2 = t.elapsed().as_secs_f64() * 1e3;
        assert_same(&s1, &s2);
        println!(
            "prepare surfel  n={} 1T={t1:.1}ms 24T={t2:.1}ms ratio={:.2}x",
            gs.len(),
            t1 / t2
        );

        let mut simd = many.clone();
        simd.simd_prep = true;
        let t = Instant::now();
        let (s3, _) = prepare_surfel(&gs, &grid, &simd);
        let t3 = t.elapsed().as_secs_f64() * 1e3;
        assert_eq!(s3.len(), s1.len());
        for (x, y) in s1.iter().zip(&s3) {
            for k in 0..3 {
                let scale = x.eigen.values[k].abs().max(1.0);
                assert!((x.eigen.values[k] - y.eigen.values[k]).abs() <= 1e-6 * scale);
            }
        }
        println!(
            "prepare surfel  SIMD n={} {t3:.1}ms vs 1T {t1:.1}ms / 24T {t2:.1}ms speedup={:.2}x",
            gs.len(),
            t2 / t3
        );

        let chunks = chunk_ranges(128, 48);
        let t = Instant::now();
        let mut checks = 0usize;
        for c in &chunks {
            for g in &d1 {
                let r = splat_range(&grid, g.mean, g.radius);
                if !r.empty {
                    let z_lo = r.lo[2].max(c.z0);
                    let z_hi = r.hi[2].min(c.z1 - 1);
                    if z_lo <= z_hi {
                        checks += 1;
                    }
                }
            }
        }
        let old_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let bins = build_bins(&d1, &grid, &chunks);
        let new_ms = t.elapsed().as_secs_f64() * 1e3;
        let entries: usize = bins.iter().map(Vec::len).sum();
        println!(
            "bins n=1e5 chunks={} old_scan={old_ms:.1}ms checks={checks} build={new_ms:.1}ms entries={entries}",
            chunks.len()
        );
    }
}

#[cfg(test)]
mod bench_tiles;
