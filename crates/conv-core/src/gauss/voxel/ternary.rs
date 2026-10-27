//! 三值（trit）体素场：把 density 的 f32 累加缓冲换成 2-bit 打包饱和计数。
//!
//! 语义差异（必须记住）：
//! - `Field`（density）= Σ opacity·exp(-½m)，阈值取分位数 ⇒ 连续密度。
//! - `TritField` = 每个体素被多少个高斯覆盖的**饱和计数**（0/1/2），
//!   `trit >= 2` 只表示「至少两个高斯碰到过这里」，不是密度。
//!
//! 收益：写入量降到 f32 的 1/16，实测 4–30×（见 README ⑦）。

use super::*;

/// 2-bit 三元场：0 = 未被覆盖，1 = 被 1 个高斯覆盖，2 = 被 ≥2 个覆盖（饱和）。
#[derive(Clone, Debug)]
pub struct TritField {
    pub grid: Grid,
    /// 16 个体素 / u32，z-major 线性索引。
    pub words: Vec<u32>,
    /// trit >= 2 的体素数。
    pub occupied: usize,
    /// trit >= 1 的体素数（观测到的体素）。
    pub observed: usize,
    pub workers: usize,
    pub bounds: Aabb,
}

impl TritField {
    #[inline]
    pub fn trit_at(&self, x: u32, y: u32, z: u32) -> u8 {
        let n = self.grid.n as usize;
        let idx = z as usize * n * n + y as usize * n + x as usize;
        ((self.words[idx / 16] >> ((idx % 16) * 2)) & 0b11) as u8
    }

    /// 把 trit 场转成 Surface Nets 可用的标量场：trit>=iso 记为内部（负值）。
    pub fn to_field(&self, iso_trit: u8) -> Field {
        let n = self.grid.n as usize;
        let mut values = vec![1.0f32; n * n * n];
        for (i, v) in values.iter_mut().enumerate() {
            let t = ((self.words[i / 16] >> ((i % 16) * 2)) & 0b11) as u8;
            if t >= iso_trit {
                *v = -1.0;
            }
        }
        Field {
            values,
            grid: self.grid,
            mode: VoxelMode::Density,
            backend: Backend::Cpu,
            bins3d: false,
            simd_prep: false,
            iso: 0.0,
            observed: self.observed,
            sealed_interior: 0,
            workers: self.workers,
            bounds: self.bounds,
        }
    }
}

#[inline]
fn trit_add(words: &mut [u32], idx: usize) {
    let w = idx / 16;
    let shift = (idx % 16) * 2;
    let cur = (words[w] >> shift) & 0b11;
    if cur < 2 {
        words[w] = (words[w] & !(0b11 << shift)) | ((cur + 1) << shift);
    }
}

fn splat_trit_chunk(
    prepared: &[Prepared],
    indices: &[u32],
    grid: &Grid,
    chunk: Chunk,
    m_cut: f32,
    out: &mut [u32],
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
                    if m > m_cut {
                        continue;
                    }
                    trit_add(out, row + x as usize);
                }
            }
        }
    }
}

/// 三值体素化：与 [`voxelize`] 同样的取景/准备/裁剪，只换累加缓冲。
///
/// 分块按 4 个 z 层对齐（n² 可被 16 整除时），保证每块落在整字边界上；
/// `n % 4 != 0` 时退化为单块（仍正确，只是不分块）。
/// `m_cut` = Mahalanobis² 截断：18 = 3σ（默认，粗壳），1.0 = 1σ（薄壳）。
/// 收紧它能显著减少「多占」，用来逼近 f32 密度场的等值面。
pub fn voxelize_ternary_with(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    m_cut: f32,
) -> Result<TritField, String> {
    if gaussians.is_empty() {
        return Err("没有高斯可体素化".into());
    }
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    let n = grid.n as usize;
    let prepared = prepare_density(gaussians, &grid, opts);
    let workers = opts.threads.clamp(1, n.max(1));

    let aligned = n.is_multiple_of(4);
    let mut words: Vec<u32>;

    if !aligned {
        let mut buf = vec![0u32; (n * n * n).div_ceil(16)];
        let indices: Vec<u32> = (0..prepared.len() as u32).collect();
        splat_trit_chunk(
            &prepared,
            &indices,
            &grid,
            Chunk {
                z0: 0,
                z1: n as u32,
            },
            m_cut,
            &mut buf,
        );
        words = buf;
    } else {
        // 每块 4 个 z 层 = 4·n² 个体素 = n²/4 个 word（n%4==0 ⇒ 整字）。
        let blocks: Vec<Chunk> = (0..n / 4)
            .map(|i| Chunk {
                z0: (i * 4) as u32,
                z1: (i * 4 + 4) as u32,
            })
            .collect();
        let mut parts = parallel_map_chunks(&blocks, workers, |_, c| {
            let wps = (c.z1 - c.z0) as usize * n * n / 16;
            let mut buf = vec![0u32; wps];
            // 只保留与这块相交的高斯（与 build_bins 同思路，保持顺序）
            let indices: Vec<u32> = prepared
                .iter()
                .enumerate()
                .filter(|(_, g)| {
                    let r = splat_range(&grid, g.mean, g.radius);
                    !r.empty && r.lo[2] < c.z1 && r.hi[2] >= c.z0
                })
                .map(|(i, _)| i as u32)
                .collect();
            splat_trit_chunk(&prepared, &indices, &grid, *c, m_cut, &mut buf);
            buf
        });
        // parallel_map_chunks 按**完成顺序**返回，必须按块号重排后再拼接
        parts.sort_by_key(|(i, _)| *i);
        let mut merged = Vec::with_capacity((n * n * n).div_ceil(16));
        for (_, part) in parts {
            merged.extend_from_slice(&part);
        }
        words = merged;
    }
    words.resize((n * n * n).div_ceil(16), 0);

    let mut occupied = 0usize;
    let mut observed = 0usize;
    for (i, w) in words.iter().enumerate() {
        let mut w = *w;
        let limit = if i == words.len() - 1 {
            n * n * n - i * 16
        } else {
            16
        };
        for _ in 0..limit {
            match w & 0b11 {
                0 => {}
                1 => observed += 1,
                _ => {
                    observed += 1;
                    occupied += 1;
                }
            }
            w >>= 2;
        }
    }

    Ok(TritField {
        grid,
        words,
        occupied,
        observed,
        workers,
        bounds,
    })
}

/// 默认 3σ 截断（等价于旧的 18.0）。
pub fn voxelize_ternary(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<TritField, String> {
    voxelize_ternary_with(gaussians, opts, 18.0)
}
