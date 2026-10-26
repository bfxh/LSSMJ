//! GPU splat（density / surfel）：确定性定点整数原子累加。
//!
//! 反直觉点：GPU 上直接 `atomicAdd<f32>` 不存在，且浮点原子/归约顺序会让结果不可复现。
//! 这里把权重量化成 u32/i32 再用整数原子累加——整数加法可交换可结合，
//! 所以同一输入多次运行的输出**逐位一致**（与 dispatch/执行顺序无关）。
//! 代价是量化误差：CPU 的 f32 场与 GPU 场不是逐位相等，判据按容差对拍。
//!
//! job 表：CPU 侧对每个高斯算 3σ bbox，前缀和成 `offsets`；GPU 每个 thread
//! 处理一个 (Gaussian, voxel) job，二分查回 Gaussian 下标。无稀疏结构、无排序。

use super::voxel::{
    Field, Grid, Prepared, VoxelMode, VoxelizeOptions, prepare_density, prepare_surfel, splat_range,
};
use super::{Aabb, Gaussian};
use crate::jfa::{Headless, storage_entry, try_headless_device, uniform_entry};
mod gpu_post;

use wgpu::util::DeviceExt;

const DENS_SCALE: f32 = 65536.0;
const DIST_SCALE: f32 = 16384.0;
const DEN_SCALE: f32 = 65536.0;
const WG: u32 = 64;
const MAX_WG_X: u32 = 65535;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuGaussian {
    mean: [f32; 4],
    lam: [f32; 4],
    v0: [f32; 4],
    v1: [f32; 4],
    v2: [f32; 4],
    normal: [f32; 4],
    bbox_min: [u32; 4],
    bbox_size: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    count: u32,
    dispatch_x: u32,
    pad: u32,
    origin_h: [f32; 4],
    extra: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostParams {
    n: u32,
    mode: u32,
    count: u32,
    dispatch_x: u32,
    step: u32,
    seal: u32,
    pad1: u32,
    pad2: u32,
    iso: f32,
    outside: f32,
    min_weight: f32,
    max_density: f32,
    dens_scale: f32,
    den_scale: f32,
    dist_scale: f32,
    known_margin: f32,
}

struct JobTable {
    gaussians: Vec<GpuGaussian>,
    offsets: Vec<u32>,
    total: u32,
}

fn build_job_table(prepared: &[Prepared], grid: &Grid) -> Result<JobTable, String> {
    let mut gaussians = Vec::with_capacity(prepared.len());
    let mut offsets = Vec::with_capacity(prepared.len() + 1);
    offsets.push(0u32);
    let mut total = 0u32;
    for g in prepared {
        let range = splat_range(grid, g.mean, g.radius);
        let (bbox_min, bbox_size) = if range.empty {
            ([0u32; 4], [0u32; 4])
        } else {
            (
                [range.lo[0], range.lo[1], range.lo[2], 0],
                [
                    range.hi[0] - range.lo[0] + 1,
                    range.hi[1] - range.lo[1] + 1,
                    range.hi[2] - range.lo[2] + 1,
                    0,
                ],
            )
        };
        let count = bbox_size[0]
            .checked_mul(bbox_size[1])
            .and_then(|v| v.checked_mul(bbox_size[2]))
            .ok_or("GPU splat bbox 体素数溢出 u32")?;
        total = total.checked_add(count).ok_or_else(|| {
            format!(
                "GPU splat job 总数溢出 u32（累计 {} + 本高斯 {}，bbox {:?}）",
                total, count, bbox_size
            )
        })?;
        offsets.push(total);
        gaussians.push(GpuGaussian {
            mean: [g.mean[0], g.mean[1], g.mean[2], g.opacity],
            lam: [g.lam[0], g.lam[1], g.lam[2], 0.0],
            v0: [
                g.eigen.vectors[0][0],
                g.eigen.vectors[0][1],
                g.eigen.vectors[0][2],
                0.0,
            ],
            v1: [
                g.eigen.vectors[1][0],
                g.eigen.vectors[1][1],
                g.eigen.vectors[1][2],
                0.0,
            ],
            v2: [
                g.eigen.vectors[2][0],
                g.eigen.vectors[2][1],
                g.eigen.vectors[2][2],
                0.0,
            ],
            normal: [g.normal[0], g.normal[1], g.normal[2], 0.0],
            bbox_min,
            bbox_size,
        });
    }
    if total == 0 {
        return Err("GPU splat 没有可写体素（核宽/域外）".into());
    }
    Ok(JobTable {
        gaussians,
        offsets,
        total,
    })
}

fn dispatch_dims(total: u32) -> Result<(u32, u32), String> {
    let workgroups = total.div_ceil(WG);
    let x = workgroups.min(MAX_WG_X);
    let y = workgroups.div_ceil(x);
    if y > MAX_WG_X {
        return Err("GPU splat 派发维度超过 65535（job 表过大）".into());
    }
    Ok((x, y))
}

fn buffer_uninit(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes as u64,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// 不在 CPU 侧分配/上传零数组：GPU clear 后再用，128³ 可省 20–30 ms。
fn buffer_cleared(hd: &Headless, label: &str, bytes: usize) -> wgpu::Buffer {
    let buf = buffer_uninit(&hd.device, label, bytes);
    let mut enc = hd
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.clear_buffer(&buf, 0, Some(bytes as u64));
    hd.queue.submit([enc.finish()]);
    buf
}

fn splat_density_gpu(
    hd: &Headless,
    prepared: &[Prepared],
    grid: &Grid,
) -> Result<wgpu::Buffer, String> {
    let table = build_job_table(prepared, grid)?;
    let (wg_x, wg_y) = dispatch_dims(table.total)?;
    let device = &hd.device;
    let queue = &hd.queue;
    let count = (grid.n * grid.n * grid.n) as usize;

    let gs_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-gs"),
        contents: bytemuck::cast_slice(&table.gaussians),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let offset_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-offsets"),
        contents: bytemuck::cast_slice(&table.offsets),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let out_buf = buffer_cleared(hd, "gauss-splat-density", count * 4);
    let params = Params {
        n: grid.n,
        count: table.gaussians.len() as u32,
        dispatch_x: wg_x,
        pad: 0,
        origin_h: [grid.origin[0], grid.origin[1], grid.origin[2], grid.h],
        extra: [DENS_SCALE, 0.0, 0.0, 0.0],
    };
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("splat_density.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gauss-splat-density-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
            uniform_entry(4),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gauss-splat-density-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gauss-splat-density"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("splat_density"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gauss-splat-density-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: gs_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: offset_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gauss-splat-density-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(wg_x, wg_y, 1);
    }
    queue.submit([enc.finish()]);
    Ok(out_buf)
}

fn splat_surfel_gpu(
    hd: &Headless,
    prepared: &[Prepared],
    grid: &Grid,
    clamp_abs: f32,
) -> Result<(wgpu::Buffer, wgpu::Buffer), String> {
    let table = build_job_table(prepared, grid)?;
    let (wg_x, wg_y) = dispatch_dims(table.total)?;
    let device = &hd.device;
    let queue = &hd.queue;
    let count = (grid.n * grid.n * grid.n) as usize;

    let gs_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-gs"),
        contents: bytemuck::cast_slice(&table.gaussians),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let offset_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-offsets"),
        contents: bytemuck::cast_slice(&table.offsets),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let num_buf = buffer_cleared(hd, "gauss-splat-num", count * 4);
    let den_buf = buffer_cleared(hd, "gauss-splat-den", count * 4);
    let params = Params {
        n: grid.n,
        count: table.gaussians.len() as u32,
        dispatch_x: wg_x,
        pad: 0,
        origin_h: [grid.origin[0], grid.origin[1], grid.origin[2], grid.h],
        extra: [0.0, DIST_SCALE, DEN_SCALE, clamp_abs],
    };
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-splat-params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("splat_surfel.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gauss-splat-surfel-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
            storage_entry(3, false),
            uniform_entry(4),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gauss-splat-surfel-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gauss-splat-surfel"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("splat_surfel"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gauss-splat-surfel-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: gs_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: offset_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: num_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: den_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gauss-splat-surfel-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(wg_x, wg_y, 1);
    }
    queue.submit([enc.finish()]);
    Ok((num_buf, den_buf))
}

pub(crate) fn voxelize_gpu_with(
    hd: &Headless,
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    match opts.mode {
        VoxelMode::Density => {
            let prepared = prepare_density(gaussians, &grid, opts);
            if prepared.is_empty() {
                return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
            }
            let density = splat_density_gpu(hd, &prepared, &grid)?;
            gpu_post::post_gpu(
                hd,
                Some(&density),
                None,
                None,
                grid,
                opts,
                bounds,
                VoxelMode::Density,
            )
        }
        VoxelMode::Surfel => {
            let (prepared, clamp_abs) = prepare_surfel(gaussians, &grid, opts);
            if prepared.is_empty() {
                return Err("没有有效的高斯（NaN/Inf/非正不透明度）".into());
            }
            let (num, den) = splat_surfel_gpu(hd, &prepared, &grid, clamp_abs)?;
            gpu_post::post_gpu(
                hd,
                None,
                Some(&num),
                Some(&den),
                grid,
                opts,
                bounds,
                VoxelMode::Surfel,
            )
        }
    }
}

pub(crate) fn voxelize_gpu(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    grid: Grid,
    bounds: Aabb,
) -> Result<Field, String> {
    let hd = try_headless_device()?;
    voxelize_gpu_with(&hd, gaussians, opts, grid, bounds)
}
