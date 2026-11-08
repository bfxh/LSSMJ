//! 粒子腿各向异性核（T-GC-03）：每粒子一枚各向异性高斯核（中心 + 朝向四元数 + 三轴缩放），
//! 核贡献按 3σ 支撑窗口 splat 进体素场（`field[v] = Σ 核权重`，未归一）。
//!
//! 口径：核参数可显式给定（第一片），或由**邻域 PCA 自动定向**（第二片，锚 `W15A-024/025`：
//! "用局部邻域重建各向异性方向，PCA 主轴正交形式"）——kNN 协方差 → 对称 3×3 Jacobi
//! 特征分解 → 主轴正交基。特征值**升序** ⇒ 核 X 轴 = 最薄方向（薄片场景即法向）。
//! 缩放由特征值映射：σ_i = max(gain × √λ_i, σ_floor)（覆盖局部采样的口径，本片钉死）。
//! 累积 = **Q16 定点 + `atomicAdd<u32>`**——WGSL 无 `atomic<f32>`；整数加法可交换 ⇒
//! 与执行序无关、**逐位确定**（本仓"顺序走 scan/规范"纪律在 splat 上的落地）。
//! 坐标与库内一致：world = idx × h − 1，h = 2/(n−1)。

use crate::jfa::{Headless, readback_u32, storage_entry, uniform_entry};
use wgpu::util::DeviceExt;

/// 支撑窗口（σ 倍数；截断在 3σ）。
pub const KERNEL_CUTOFF_SIGMA: f32 = 3.0;
/// 定点标度（Q16）。
pub const FIXED_POINT_SCALE: f32 = 65536.0;

/// 各向异性核集合（长度须一致）。
pub struct AnisoKernels {
    /// 中心（世界坐标，与库内 [-1,1]³ 口径一致）
    pub centers: Vec<[f32; 3]>,
    /// 朝向四元数 (x,y,z,w)，不要求归一（按原样使用）
    pub rotations: Vec<[f32; 4]>,
    /// 三轴缩放（世界单位，标准差）
    pub scales: Vec<[f32; 3]>,
}

impl AnisoKernels {
    pub fn new(centers: Vec<[f32; 3]>, rotations: Vec<[f32; 4]>, scales: Vec<[f32; 3]>) -> Self {
        assert_eq!(rotations.len(), centers.len(), "rotations 长度不一致");
        assert_eq!(scales.len(), centers.len(), "scales 长度不一致");
        Self {
            centers,
            rotations,
            scales,
        }
    }

    /// 各向同性基线（单位朝向、全轴等宽）——质量判据的对照臂。
    pub fn isotropic(centers: Vec<[f32; 3]>, scale: f32) -> Self {
        let n = centers.len();
        Self {
            centers,
            rotations: vec![[0.0, 0.0, 0.0, 1.0]; n],
            scales: vec![[scale, scale, scale]; n],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct KP {
    n: u32,
    count: u32,
    side: u32,
    total: u32,
    stride: u32,
}

/// splat 进 n³ 体素场：返回每体素核权重和（= Q16 整数累积 / 65536）。
pub fn splat_field(
    hd: &Headless,
    kernels: &AnisoKernels,
    n: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> Vec<f32> {
    let ones = vec![1.0f32; kernels.centers.len()];
    splat_field_weighted(hd, kernels, &ones, n, timer)
}

/// splat（按核加权）：每核贡献 = weight_i × exp(−½ Mahalanobis²)——「概率占据」
/// 等语义（opacity 等标量）由权重承载；`weights` 长度须与核数一致。
pub fn splat_field_weighted(
    hd: &Headless,
    kernels: &AnisoKernels,
    weights: &[f32],
    n: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> Vec<f32> {
    let device = &hd.device;
    let queue = &hd.queue;
    let count = kernels.centers.len() as u32;
    assert_eq!(
        weights.len(),
        kernels.centers.len(),
        "weights 长度须与核数一致"
    );
    let h = 2.0 / (n as f32 - 1.0);
    let max_s = kernels
        .scales
        .iter()
        .flat_map(|s| s.iter())
        .fold(0f32, |a, &b| a.max(b));
    // 支撑窗（cell）：R = ceil(3σ_max / h) + 1（+1 边距覆盖 floor 取整误差）
    let r = ((KERNEL_CUTOFF_SIGMA * max_s) / h).ceil() as u32 + 1;
    let side = 2 * r + 1;
    let total = (count as u64) * (side as u64).pow(3);
    assert!(
        total <= u32::MAX as u64,
        "splat 线程数超上界（count×side³={total}）"
    );
    // grid-stride：派发封顶 65535 workgroup，余量由循环消化
    let dispatch_wg = (total.div_ceil(64)).min(65535) as u32;
    let stride = dispatch_wg * 64;

    let flat = |v: &[[f32; 3]]| -> Vec<f32> { v.iter().flat_map(|x| x.iter().copied()).collect() };
    let centers_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-centers"),
        contents: bytemuck::cast_slice(&flat(&kernels.centers)),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let rots_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-rots"),
        contents: bytemuck::cast_slice(&kernels.rotations.concat()),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let scales_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-scales"),
        contents: bytemuck::cast_slice(&flat(&kernels.scales)),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let weights_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-weights"),
        contents: bytemuck::cast_slice(weights),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let field_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-field"),
        contents: &vec![0u8; (n as usize) * (n as usize) * (n as usize) * 4],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("k-params"),
        contents: bytemuck::bytes_of(&KP {
            n,
            count,
            side,
            total: total as u32,
            stride,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("kernels.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("k-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, false),
            uniform_entry(4),
            storage_entry(5, true),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("k-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("k-splat"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("splat"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("k-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: centers_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: rots_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: scales_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: field_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: weights_buf.as_entire_binding(),
            },
        ],
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let tw = timer.and_then(|t| t.writes());
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: tw,
        });
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(dispatch_wg, 1, 1);
    }
    queue.submit([enc.finish()]);

    readback_u32(hd, &field_buf)
        .into_iter()
        .map(|q| q as f32 / FIXED_POINT_SCALE)
        .collect()
}

// ---- 邻域 PCA 自动定向（T-GC-03 第二片，锚 W15A-024/025）----

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// 对称 3×3 Jacobi 特征分解：返回（特征值升序, 特征向量——`cols[i]` = 第 i 个
/// 特征值对应的特征向量）。固定扫描序 + 固定迭代上限 ⇒ 逐位确定。
fn jacobi_eigen(mut a: [[f32; 3]; 3]) -> ([f32; 3], [[f32; 3]; 3]) {
    let mut v = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..64 {
        let (mut p, mut q, mut best) = (0usize, 1usize, 0f32);
        for (i, j) in [(0usize, 1usize), (0, 2), (1, 2)] {
            if a[i][j].abs() > best {
                best = a[i][j].abs();
                p = i;
                q = j;
            }
        }
        if best < 1e-12 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        let apq = a[p][q];
        let k = 3 - p - q; // p,q 之外的第三索引（p≠q ⇒ 必在 0..3）
        let akp = a[k][p];
        let akq = a[k][q];
        a[k][p] = c * akp - s * akq;
        a[p][k] = a[k][p];
        a[k][q] = s * akp + c * akq;
        a[q][k] = a[k][q];
        a[p][p] -= t * apq;
        a[q][q] += t * apq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
        // V = V·J：全部三行都要更新（含 k=p、k=q——与 A 块不同，此处无跳过）
        for row in v.iter_mut() {
            let vkp = row[p];
            let vkq = row[q];
            row[p] = c * vkp - s * vkq;
            row[q] = s * vkp + c * vkq;
        }
    }
    let ev = [a[0][0], a[1][1], a[2][2]];
    let mut ord = [0usize, 1, 2];
    if ev[ord[0]] > ev[ord[1]] {
        ord.swap(0, 1);
    }
    if ev[ord[1]] > ev[ord[2]] {
        ord.swap(1, 2);
    }
    if ev[ord[0]] > ev[ord[1]] {
        ord.swap(0, 1);
    }
    let evs = [ev[ord[0]], ev[ord[1]], ev[ord[2]]];
    // cols[i] = 第 i 个特征向量（组件 = v 的行）
    let cols = [
        [v[0][ord[0]], v[1][ord[0]], v[2][ord[0]]],
        [v[0][ord[1]], v[1][ord[1]], v[2][ord[1]]],
        [v[0][ord[2]], v[1][ord[2]], v[2][ord[2]]],
    ];
    (evs, cols)
}

/// 由正交基（`cols[c]` = 第 c 轴）提取四元数 (x,y,z,w)（Shepperd；M 的列 = cols）。
pub(crate) fn quat_from_cols(cols: &[[f32; 3]; 3]) -> [f32; 4] {
    let (m00, m01, m02) = (cols[0][0], cols[1][0], cols[2][0]);
    let (m10, m11, m12) = (cols[0][1], cols[1][1], cols[2][1]);
    let (m20, m21, m22) = (cols[0][2], cols[1][2], cols[2][2]);
    let tr = m00 + m11 + m22;
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    }
}

/// 粒子 → 核参数（邻域 PCA 自动定向）。
///
/// 口径（钉死）：kNN（暴力、距离平方升序 + 索引平局序）；协方差按邻域质心；
/// Jacobi 特征分解**升序** ⇒ 核 X 轴 = 最薄方向；每轴符号钉死（最大绝对分量取正）、
/// 右手系（det < 0 翻第三轴）；缩放 σ_i = max(gain × √λ_i, σ_floor)。
pub fn pca_kernels(particles: &[[f32; 3]], k: usize, gain: f32, sigma_floor: f32) -> AnisoKernels {
    let count = particles.len();
    assert!(count > 0, "粒子数须 ≥ 1");
    let kk = k.clamp(2, count);
    let mut rotations = Vec::with_capacity(count);
    let mut scales = Vec::with_capacity(count);
    for i in 0..count {
        // ① kNN（含自身；确定性平局序）
        let mut dist: Vec<(f32, usize)> = (0..count)
            .map(|j| {
                let d = [
                    particles[j][0] - particles[i][0],
                    particles[j][1] - particles[i][1],
                    particles[j][2] - particles[i][2],
                ];
                (dot3(d, d), j)
            })
            .collect();
        dist.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let nb = &dist[..kk];
        // ② 邻域协方差
        let mut mean = [0f32; 3];
        for &(_, j) in nb {
            for c in 0..3 {
                mean[c] += particles[j][c];
            }
        }
        for c in mean.iter_mut() {
            *c /= kk as f32;
        }
        let mut cov = [[0f32; 3]; 3];
        for &(_, j) in nb {
            let d = [
                particles[j][0] - mean[0],
                particles[j][1] - mean[1],
                particles[j][2] - mean[2],
            ];
            for (r, row) in cov.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    *cell += d[r] * d[c];
                }
            }
        }
        for row in cov.iter_mut() {
            for cell in row.iter_mut() {
                *cell /= kk as f32;
            }
        }
        // ③ 主轴（升序）+ 符号/手性钉死
        let (ev, mut cols) = jacobi_eigen(cov);
        for e in cols.iter_mut() {
            let mut bi = 0usize;
            let mut bv = 0f32;
            for (c, &val) in e.iter().enumerate() {
                if val.abs() > bv {
                    bv = val.abs();
                    bi = c;
                }
            }
            if e[bi] < 0.0 {
                for val in e.iter_mut() {
                    *val = -*val;
                }
            }
        }
        if dot3(cols[0], cross3(cols[1], cols[2])) < 0.0 {
            for val in cols[2].iter_mut() {
                *val = -*val;
            }
        }
        rotations.push(quat_from_cols(&cols));
        scales.push([
            (gain * ev[0].max(0.0).sqrt()).max(sigma_floor),
            (gain * ev[1].max(0.0).sqrt()).max(sigma_floor),
            (gain * ev[2].max(0.0).sqrt()).max(sigma_floor),
        ]);
    }
    AnisoKernels {
        centers: particles.to_vec(),
        rotations,
        scales,
    }
}
