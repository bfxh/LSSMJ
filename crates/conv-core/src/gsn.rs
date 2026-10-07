//! GPU Surface Nets（T-GC-02）：SDF→mesh，逐语义复刻 fast-surface-nets 0.2.1。
//!
//! 设计：顶点稀疏存储（槽位 = cell 线性索引，n³ 布局），判据用 CPU 侧 `surface_points`
//! 做 cell 映射对拍——无需 GPU 压缩（compaction 属后续片）。
//! 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"
//! 排序后对拍——四边形集合与绕序确定 ⇒ 排序流逐位确定。
//! 教训留档：base/前缀 + write_buffer 路线曾出现"输入逐位一致、输出随机"的未解非确定性
//! （counts/base 读回一致、纯 kernel、identity-stub 稳定——证据在 w15c 判据档），
//! 故本片弃用该机械；若后续需要 GPU 顺序，走 GPU scan（规范保证）而非 write_buffer。

use crate::jfa::{Headless, readback_f32, storage_entry, uniform_entry};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct P {
    n: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

pub struct GsnMesh {
    /// 稀疏顶点（槽位 = cell 线性索引 x + y*n + z*n²；非表面槽位为零）
    pub positions: Vec<[f32; 3]>,
    /// 表面 cell 标记（1=有顶点）
    pub flags: Vec<u32>,
    /// 三角索引（引用 positions 槽位；执行序——判据侧按四顶点规范键排序后对拍）
    pub indices: Vec<u32>,
    /// 四边形数
    pub quad_count: u32,
    pub n: u32,
}

/// GPU Surface Nets：`sdf` 为 index 单位（[0,n)³），cell 范围与 CPU 版一致（[0,n−2]³）。
pub fn surface_nets_gpu(hd: &Headless, sdf: &[f32], n: u32) -> GsnMesh {
    let device = &hd.device;
    let queue = &hd.queue;
    let count = (n * n * n) as usize;

    let sdf_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-sdf"),
        contents: bytemuck::cast_slice(sdf),
        usage: wgpu::BufferUsages::STORAGE,
    });
    // 零初始化：非表面槽位不写，但逐位确定性判据要哈希全缓冲
    let vtx_pos_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-vtx-pos"),
        contents: &vec![0u8; count * 16],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let vtx_flag_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-vtx-flag"),
        contents: &vec![0u8; count * 4],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-params"),
        contents: bytemuck::bytes_of(&P {
            n,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let counter_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-counter"),
        contents: &[0u8; 4],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    // 上限 = cell 数 × 3 四边形 × 6 索引
    let idx_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gsn-idx"),
        size: ((count as u64) * 3 * 6 * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("gsn.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gsn-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, false),
            storage_entry(2, false),
            uniform_entry(3),
            storage_entry(4, false),
            storage_entry(5, false),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gsn-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let gen_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gsn-gen"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("gen_vertices"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let emit_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gsn-emit"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("emit_quads"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gsn-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: sdf_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: vtx_pos_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: vtx_flag_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: counter_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: idx_buf.as_entire_binding(),
            },
        ],
    });

    let dispatch_3d = |pipe: &wgpu::ComputePipeline| {
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            let wg = n.div_ceil(4);
            pass.dispatch_workgroups(wg, wg, wg);
        }
        queue.submit([enc.finish()]);
    };

    // ① 顶点生成 ② 四边发射（atomicAdd 槽位）
    dispatch_3d(&gen_pipe);
    dispatch_3d(&emit_pipe);

    // ③ 回读
    let quad_count = readback_f32(hd, &counter_buf)
        .into_iter()
        .map(f32::to_bits)
        .next()
        .unwrap_or(0);
    let mut indices = readback_f32(hd, &idx_buf)
        .into_iter()
        .map(f32::to_bits)
        .collect::<Vec<u32>>();
    indices.truncate((quad_count as usize) * 6);
    let mut positions = Vec::with_capacity(count);
    let pos_raw = readback_f32(hd, &vtx_pos_buf);
    for chunk in pos_raw.as_chunks::<4>().0 {
        positions.push([chunk[0], chunk[1], chunk[2]]);
    }
    let flags: Vec<u32> = readback_f32(hd, &vtx_flag_buf)
        .into_iter()
        .map(f32::to_bits)
        .collect();

    GsnMesh {
        positions,
        flags,
        indices,
        quad_count,
        n,
    }
}
