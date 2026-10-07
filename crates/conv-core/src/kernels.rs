//! 粒子腿各向异性核（T-GC-03 第一片）：每粒子一枚各向异性高斯核（中心 + 朝向四元数 + 三轴缩放），
//! 核贡献按 3σ 支撑窗口 splat 进体素场（`field[v] = Σ 核权重`，未归一）。
//!
//! 口径：核参数**显式给定**；邻域 PCA 自动定向（TOG 2013 口径，锚 `W15A-024/025`）属后续片。
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
    _pad: u32,
}

/// splat 进 n³ 体素场：返回每体素核权重和（= Q16 整数累积 / 65536）。
pub fn splat_field(
    hd: &Headless,
    kernels: &AnisoKernels,
    n: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> Vec<f32> {
    let device = &hd.device;
    let queue = &hd.queue;
    let count = kernels.centers.len() as u32;
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
        total <= 65535 * 64,
        "splat 线程数超上限（count×side³={total}）——网格或核窗口过大"
    );

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
            _pad: 0,
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
        pass.dispatch_workgroups((total as u32).div_ceil(64), 1, 1);
    }
    queue.submit([enc.finish()]);

    readback_u32(hd, &field_buf)
        .into_iter()
        .map(|q| q as f32 / FIXED_POINT_SCALE)
        .collect()
}
