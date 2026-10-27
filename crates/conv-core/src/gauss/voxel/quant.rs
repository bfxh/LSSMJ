//! 1 字节定点密度场：把 f32 的核质量累加压到 u8（饱和），保留「质量」语义。
//!
//! 与 `ternary` 的区别：三元记「被几个高斯覆盖」，这里记「累加质量」，
//! 所以阈值化后仍能还原同一张等值面（量化误差由 `scale` 决定）。

use super::*;

/// 累加单元：u8（4× 省内存）或 u16（2× 省内存，舍入损失可忽略）。
pub trait Acc: Copy + Default + Send + 'static {
    const MAX: f32;
    /// 档位数（直方图用）。
    const LEVELS: usize;
    fn add_sat(self, q: u32) -> Self;
    fn to_f32(self) -> f32;
    fn raw(self) -> usize;
}

impl Acc for u8 {
    const MAX: f32 = 255.0;
    const LEVELS: usize = 256;
    fn add_sat(self, q: u32) -> Self {
        self.saturating_add(q.min(255) as u8)
    }
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn raw(self) -> usize {
        self as usize
    }
}

impl Acc for u16 {
    const MAX: f32 = 65535.0;
    const LEVELS: usize = 65536;
    fn add_sat(self, q: u32) -> Self {
        self.saturating_add(q.min(65535) as u16)
    }
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn raw(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
pub struct QuantField<T = u8> {
    pub grid: Grid,
    /// 定点质量，`value / scale` = 原始密度。
    pub values: Vec<T>,
    pub scale: f32,
    pub workers: usize,
    pub bounds: Aabb,
}

fn splat_quant_chunk<T: Acc>(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    chunk: Chunk,
    scale: f32,
    out: &mut [T],
) {
    let n = grid.n as usize;
    let n2 = n * n;
    #[cfg(target_arch = "x86_64")]
    let simd = super::avx2_splat();
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
                // 只在 x 跨度足够长时走 SIMD：3σ 核在 256³ 下只有 6–10 格，
                // 向量体几乎不触发，反而白付每次建常量的开销（实测慢 5–15%）。
                #[cfg(target_arch = "x86_64")]
                if simd && range.hi[0] - range.lo[0] >= 15 {
                    unsafe {
                        super::avx2::quant_row(
                            g,
                            grid,
                            y,
                            z,
                            range.lo[0],
                            range.hi[0],
                            scale,
                            out,
                            row,
                        );
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
                    let q = (g.opacity * (-0.5 * m).exp() * scale).round();
                    if q < 1.0 {
                        continue;
                    }
                    let i = row + x as usize;
                    out[i] = out[i].add_sat(q as u32);
                }
            }
        }
    }
}

/// 由 iso 反推 scale：让 iso 落在 204/255，留 20% 头部空间，保证不饱和。
///
/// 这是唯一需要的规则——不要按场景手调 scale。
#[inline]
pub fn scale_for_iso(iso: f32) -> f32 {
    204.0 / iso.max(1e-6)
}

impl<T: Acc> QuantField<T> {
    /// 还原成与 `voxelize` 同口径的标量场：`values = iso - density`（负值=内部），
    /// 并按 `seal` 做未知区封闭，保证 Surface Nets 得到同一张面。
    pub fn to_field(&self, iso: f32, seal: bool) -> Field {
        let inv = 1.0 / self.scale;
        // 直方图求 p99.9（u16 只有 65536 档），避免对 16.7M 个值排序。
        let mut hist = vec![0u32; T::LEVELS];
        let mut observed = 0usize;
        for &v in &self.values {
            hist[v.raw()] += 1;
            if v.to_f32() * inv > iso {
                observed += 1;
            }
        }
        let nonzero: u64 = hist[1..].iter().map(|c| *c as u64).sum();
        let want = ((nonzero as f64) * 0.999) as u64;
        let mut acc = 0u64;
        let mut q999 = iso.max(1e-6);
        for (q, c) in hist.iter().enumerate().skip(1) {
            acc += *c as u64;
            if acc >= want {
                q999 = (q as f32 * inv).max(1e-6);
                break;
            }
        }

        // values 与 known 一趟算完（省一次 67 MB 往返）
        let mut values: Vec<f32> = Vec::with_capacity(self.values.len());
        let mut known: Vec<bool> = Vec::with_capacity(self.values.len());
        for v in &self.values {
            let d = v.to_f32() * inv;
            values.push(iso - d);
            known.push(d > iso);
        }
        let sealed_interior = if seal {
            super::seal_unknown(&mut values, &known, &self.grid, q999)
        } else {
            for (v, k) in values.iter_mut().zip(&known) {
                if !*k {
                    *v = q999;
                }
            }
            0
        };

        Field {
            values,
            grid: self.grid,
            mode: VoxelMode::Density,
            backend: Backend::Cpu,
            bins3d: false,
            simd_prep: false,
            iso,
            observed,
            sealed_interior,
            workers: self.workers,
            bounds: self.bounds,
        }
    }
}

pub fn voxelize_quant<T: Acc>(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    scale: f32,
) -> Result<QuantField<T>, String> {
    if gaussians.is_empty() {
        return Err("没有高斯可体素化".into());
    }
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    let n = grid.n as usize;
    let prepared = prepare_density(gaussians, &grid, opts);
    let workers = opts.threads.clamp(1, n.max(1));
    let mut values = vec![T::default(); n * n * n];

    let chunk_count = if workers <= 1 {
        1
    } else {
        (workers * 2).min(n).max(1)
    };
    let chunks: Vec<Chunk> = (0..chunk_count)
        .map(|i| {
            let z0 = (i * n / chunk_count) as u32;
            let z1 = ((i + 1) * n / chunk_count) as u32;
            Chunk { z0, z1 }
        })
        .collect();
    let indices: Vec<u32> = (0..prepared.len() as u32).collect();

    // 每块独占 z 区间：直接切片并行写，无锁
    let n2 = n * n;
    std::thread::scope(|s| {
        let per = n.div_ceil(workers);
        for (ci, slice) in values.chunks_mut(per * n2).enumerate() {
            let z0 = (ci * per) as u32;
            let z1 = ((ci + 1) * per).min(n) as u32;
            if z0 >= z1 {
                break;
            }
            let c = Chunk { z0, z1 };
            let prepared = &prepared;
            let grid = &grid;
            let indices = &indices;
            let _ = &chunks;
            s.spawn(move || {
                splat_quant_chunk(prepared, indices, grid, c, scale, slice);
            });
        }
    });

    Ok(QuantField {
        grid,
        values,
        scale,
        workers,
        bounds,
    })
}

/// 自动定标：先 1/4 分辨率探针拿 iso 与峰值密度，再按 `25000/峰值` 定 scale；
/// 若检测到饱和（出现 u16::MAX）就缩 scale 重试，最多 3 次。
///
/// 返回 (与 f32 同口径的 Field, 最终 scale)。
pub fn quantize_field_auto(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<(Field, f32), String> {
    let (q, scale) = quantize_auto_quantfield(gaussians, opts)?;
    let iso = opts.iso_abs.ok_or("quant16 需要显式 iso")?;
    Ok((q.to_field(iso, opts.seal), scale))
}

/// 同 `quantize_field_auto`，但返回量化场本身（供 `to_biased_i16` 直读网格）。
pub fn quantize_auto_quantfield(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<(QuantField<u16>, f32), String> {
    // iso 必须由调用方给（同一分辨率的 f32 阈值）。密度是「每体素质量」，随体素尺寸变化，
    // 所以低分辨率探针得到的 iso 不能直接用于全分辨率。
    let iso = opts
        .iso_abs
        .ok_or("quant16 需要显式 iso（用 f32 跑一次拿 iso，再复用到批量）")?;

    // 定标：必须先知道峰值密度。饱和会把内部体素夹平，直接改变等值面附近的插值，
    // 体积能偏 ±11%~−26% —— 所以宁可多花一次 n/4 探针（约 1/64 成本），
    // 用「粗网格峰值」当保守上界（粗网格每体素质量更大 ⇒ 不会低估），按 60000/峰值 定标。
    // 显式 scale：跳过标定，直接一趟（批量复用时才是真正的 1.1×）
    if let Some(scale) = opts.quant_scale {
        let q = voxelize_quant::<u16>(gaussians, opts, scale)?;
        return Ok((q, scale));
    }

    let mut probe = opts.clone();
    probe.n = (opts.n / 2).max(16);
    probe.backend = Backend::Cpu;
    probe.mode = VoxelMode::Density;
    probe.iso_abs = Some(iso);
    let pf = voxelize(gaussians, &probe)?;
    // Field 存的是 `iso - density`，所以 density = iso - value，峰值 = iso - min(value)
    let dmin = pf.values.iter().cloned().fold(f32::INFINITY, f32::min);
    let dmax = (iso - dmin).max(iso).max(1e-6);

    let mut scale = 60000.0 / dmax;
    if std::env::var_os("LSSMJ_QUANT_DEBUG").is_some() {
        eprintln!(
            "  quant16 probe: n={} dmax={dmax:.2} iso={iso:.2} scale={scale:.1}",
            probe.n
        );
    }
    let mut last_sat = 0usize;
    for _ in 0..3 {
        let q = voxelize_quant::<u16>(gaussians, opts, scale)?;
        last_sat = q.values.iter().filter(|v| **v == u16::MAX).count();
        if last_sat == 0 {
            return Ok((q, scale));
        }
        // 探针低估了峰值：减半重来（很少触发）
        scale *= 0.5;
    }
    Err(format!("u16 定标失败：仍 {last_sat} 个体素饱和"))
}
