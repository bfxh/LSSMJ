//! GPU mesh→SDF（T-GC-01）：三角网→表面采样→JFA 种子→窄带距离场→符号（两档）。
//!
//! 单位：全程 index 单位（顶点 CPU 侧缩放到 [0,n)³），返回前缩放回世界单位 [-1,1]³。
//! 确定性：样本槽位来自 CPU 前缀和（无原子序竞争），散布用 atomicMin（最小样本索引胜出），
//! 全链逐位确定。符号两档（`SignMode`）：
//! - `Radial` 实时档 = 逐体素径向出射射线（凸体判据口径，与 CPU radial_sign 同源）；
//! - `Winding` 精度档 = 逐体素广义绕数（与 CPU winding_number 同源），对非凸/多分量/自交稳健，
//!   代价是每带内体素 O(n_tris) 暴力求和（树加速属后续片）。
//!
//! 样本缓冲按每三角上限 64 预分配（n_tris×64×16B）——压缩属后续片（GPU scan）。
//! 该上限的实测代价（精度档粗盒判据的读数）：少数大三角的网格（12 面粗盒 @64³）样本间距可达
//! ~4.9 体素 > 符号带宽 4.5 ⇒ 棱/角内侧窄带出现未投符号空洞（符号本身零假内）。拆分大三角属后续片。

use crate::jfa::{Headless, INVALID, jfa_run, readback_f32, storage_entry, uniform_entry};
use crate::timer::GpuTimer;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshParams {
    n_tris: u32,
    n: u32,
    density: f32,
    band: f32,
}

const MAX_PER_TRI: u32 = 64;
const WG1: u32 = 64;

/// 符号档位：实时档（径向出射射线，凸体口径）/ 精度档（广义绕数，一般网格）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignMode {
    Radial,
    Winding,
}

impl SignMode {
    fn entry(self) -> &'static str {
        match self {
            SignMode::Radial => "sign_radial",
            SignMode::Winding => "sign_winding",
        }
    }
}

/// GPU mesh→SDF：返回世界单位窄带有符号距离场（带外为无符号距离）。
/// `timer`：C22 逐边 GPU 计时（None = 不计时）。
pub fn mesh_to_sdf_gpu(
    hd: &Headless,
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
    n: u32,
    mode: SignMode,
    mut timer: Option<&mut GpuTimer>,
) -> Vec<f32> {
    let h_world = 2.0 / (n as f32 - 1.0);
    let device = &hd.device;
    let queue = &hd.queue;
    let count = (n * n * n) as usize;
    let n_tris = faces.len() as u32;

    // 顶点缩放到 index 单位并按 vec4 填充（WGSL array<vec4> 步长 16）
    let verts_idx: Vec<[f32; 4]> = verts
        .iter()
        .map(|v| {
            [
                (v[0] + 1.0) / h_world,
                (v[1] + 1.0) / h_world,
                (v[2] + 1.0) / h_world,
                0.0,
            ]
        })
        .collect();
    let faces_pad: Vec<[u32; 4]> = faces.iter().map(|f| [f[0], f[1], f[2], 0]).collect();

    let verts_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ms-verts"),
        contents: bytemuck::cast_slice(&verts_idx),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let faces_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ms-faces"),
        contents: bytemuck::cast_slice(&faces_pad),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ms-params"),
        contents: bytemuck::bytes_of(&MeshParams {
            n_tris,
            n,
            density: 1.5,
            // 符号带宽 > 比较带宽（3）：样本稀疏导致"到样本距离"略大于"到表面距离"，
            // 带内体素必须全部投符号，否则带缘出现正负错配（判据首跑抓到）
            band: 4.5,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let counts_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ms-counts"),
        size: (u64::from(n_tris) * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let base_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ms-base"),
        size: (u64::from(n_tris) + 1) * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let samples_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ms-samples"),
        size: (u64::from(n_tris) * u64::from(MAX_PER_TRI) * 16).max(16),
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let invalid = vec![INVALID; count];
    let label_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ms-label"),
        contents: bytemuck::cast_slice(&invalid),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let dist_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ms-dist"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("mesh_sdf.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ms-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            uniform_entry(2),
            storage_entry(3, false),
            storage_entry(4, true),
            storage_entry(5, false),
            storage_entry(6, false),
            storage_entry(7, false),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ms-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let mk_pipe = |label: &str, entry: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pl),
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    };
    let count_pipe = mk_pipe("ms-count", "count_samples");
    let emit_pipe = mk_pipe("ms-emit", "emit_samples");
    let scatter_pipe = mk_pipe("ms-scatter", "scatter_seeds");
    let sign_pipe = mk_pipe("ms-sign", mode.entry());

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ms-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: verts_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: faces_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: counts_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: base_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: samples_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: label_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: dist_buf.as_entire_binding(),
            },
        ],
    });

    let dispatch_linear =
        |pipe: &wgpu::ComputePipeline, threads: u32, timer: &mut Option<&mut GpuTimer>| {
            let mut enc =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            let tw = timer.as_deref_mut().and_then(|t| t.writes());
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: tw,
                });
                pass.set_pipeline(pipe);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(threads.div_ceil(WG1), 1, 1);
            }
            queue.submit([enc.finish()]);
        };
    let dispatch_3d = |pipe: &wgpu::ComputePipeline, timer: &mut Option<&mut GpuTimer>| {
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let tw = timer.as_deref_mut().and_then(|t| t.writes());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: tw,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            let wg = n.div_ceil(4);
            pass.dispatch_workgroups(wg, wg, wg);
        }
        queue.submit([enc.finish()]);
    };

    // ① 计数 + 回读（小缓冲回读；GPU 前缀和属后续片）
    dispatch_linear(&count_pipe, n_tris, &mut timer);
    let counts: Vec<u32> = readback_f32(hd, &counts_buf)
        .into_iter()
        .map(f32::to_bits)
        .collect();

    // ② CPU 前缀和（确定性槽位）
    let mut base = Vec::with_capacity(counts.len() + 1);
    base.push(0u32);
    for c in &counts {
        base.push(base.last().unwrap() + c);
    }
    let total = *base.last().unwrap();
    queue.write_buffer(&base_buf, 0, bytemuck::cast_slice(&base));

    // ③ 发射样本 + atomicMin 散布
    dispatch_linear(&emit_pipe, n_tris, &mut timer);
    dispatch_linear(&scatter_pipe, total, &mut timer);

    // ④ JFA 泛洪 + 终距离（GPU 驻留；距离写入 dist_buf）
    jfa_run(
        hd,
        &label_buf,
        &samples_buf,
        n,
        &dist_buf,
        timer.as_deref_mut(),
    );

    // ⑤ 符号（窄带内逐体素，带外保持无符号）：Radial=径向射线 / Winding=广义绕数
    dispatch_3d(&sign_pipe, &mut timer);

    let mut out = readback_f32(hd, &dist_buf);
    for d in &mut out {
        *d *= h_world;
    }
    out
}
