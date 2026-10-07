//! GPU JFA（Jump Flooding）距离场积木——粒子→隐式场腿的实时核心（T-GC-03 前置）。
//!
//! 语义：N³ index 网格，点种子（index 坐标），输出每体素到最近种子的欧氏距离。
//! 判据：JFA 对点种子给出精确最近种子距离（label 平局不影响距离值），
//! `tests/jfa_gpu.rs` 用 CPU 暴力参照对拍 + 两次运行逐位确定性。
//! 注意：JFA 是近似算法（Rong-Tan 2006 口径），对拍带见判据测试。

use wgpu::util::DeviceExt;

pub(crate) const INVALID: u32 = u32::MAX;
const WG: u32 = 4; // workgroup_size(4,4,4)

pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// 无头设备（判据/离线管线用；显示面走引擎侧 T-PH-01）。
pub fn headless_device() -> Headless {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("no wgpu adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("conv-core-headless"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .expect("request_device");
    Headless { device, queue }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    step: u32,
    n: u32,
}

/// JFA 距离场（CPU 种子便捷入口）：`seeds` 为 index 坐标，返回每体素最近种子距离（index 单位）。
pub fn jfa_distance_field(headless: &Headless, seeds: &[[f32; 3]], n: u32) -> Vec<f32> {
    let device = &headless.device;
    let count = (n * n * n) as usize;

    // label 初始化：种子所在体素写自身索引，其余 INVALID
    let mut labels = vec![INVALID; count];
    for (si, s) in seeds.iter().enumerate() {
        let (x, y, z) = (s[0] as u32, s[1] as u32, s[2] as u32);
        assert!(x < n && y < n && z < n, "seed out of grid");
        labels[(x + y * n + z * n * n) as usize] = si as u32;
    }
    let label_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("jfa-label-a"),
        contents: bytemuck::cast_slice(&labels),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let seeds_buf = {
        // WGSL array<vec4<f32>> 步长 16——Rust 侧必须按 16 字节填充（vec3 步长 12 会错位）
        let padded: Vec<[f32; 4]> = seeds.iter().map(|s| [s[0], s[1], s[2], 0.0]).collect();
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jfa-seeds"),
            contents: bytemuck::cast_slice(&padded),
            usage: wgpu::BufferUsages::STORAGE,
        })
    };
    let dist_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("jfa-dist"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    jfa_run(headless, &label_a, &seeds_buf, n, &dist_buf);
    readback_f32(headless, &dist_buf)
}

/// JFA 泛洪（步长 n/2..1）+ 终距离趟。label 缓冲须已按 INVALID/种子索引初始化；
/// seeds 缓冲为 array<vec4<f32>>（index 坐标）；距离写入调用方提供的 dist 缓冲（index 单位），
/// 符号与回读由调用方决定——mesh→SDF 链路因此不中断 GPU 驻留。
pub(crate) fn jfa_run(
    headless: &Headless,
    label_a: &wgpu::Buffer,
    seeds_buf: &wgpu::Buffer,
    n: u32,
    dist_buf: &wgpu::Buffer,
) {
    let device = &headless.device;
    let queue = &headless.queue;
    let count = (n * n * n) as usize;

    let label_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("jfa-label-b"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("jfa-params"),
        size: 8,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("jfa.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("jfa-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, false),
            storage_entry(2, true),
            uniform_entry(3),
            storage_entry(4, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("jfa-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pass_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("jfa-pass"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("jfa_pass"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let dist_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("jfa-dist"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("to_distance"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let make_bg = |label: &str, src: &wgpu::Buffer, dst: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: seeds_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: dist_buf.as_entire_binding(),
                },
            ],
        })
    };

    let mut label_src = label_a;
    let mut label_dst = &label_b;
    let wg = n.div_ceil(WG);
    let mut step = n / 2;
    while step > 0 {
        queue.write_buffer(&params, 0, bytemuck::bytes_of(&Params { step, n }));
        let bg = make_bg("jfa-pass-bg", label_src, label_dst);
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pass_pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(wg, wg, wg);
        }
        queue.submit([enc.finish()]);
        std::mem::swap(&mut label_src, &mut label_dst);
        step /= 2;
    }

    // 最终距离腿：label_src 现在持有最新标签
    queue.write_buffer(&params, 0, bytemuck::bytes_of(&Params { step: 0, n }));
    let dist_bg = make_bg("jfa-dist-bg", label_src, label_dst);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&dist_pipe);
        pass.set_bind_group(0, &dist_bg, &[]);
        pass.dispatch_workgroups(wg, wg, wg);
    }
    queue.submit([enc.finish()]);
}

/// 缓冲回读（f32）。
pub(crate) fn readback_f32(headless: &Headless, buf: &wgpu::Buffer) -> Vec<f32> {
    let device = &headless.device;
    let download = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: buf.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(buf, 0, &download, 0, buf.size());
    headless.queue.submit([enc.finish()]);
    let slice = download.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = slice.get_mapped_range().unwrap();
    bytemuck::allocation::pod_collect_to_vec(&data)
}

pub(crate) fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            min_binding_size: None,
            has_dynamic_offset: false,
        },
        count: None,
    }
}

pub(crate) fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            min_binding_size: None,
            has_dynamic_offset: false,
        },
        count: None,
    }
}
