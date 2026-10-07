//! GPU 排他前缀和积木（T-GC-05 第三片）：u32 数组 → 排他前缀和 + 总数。
//!
//! 三段式、无原子、全确定（"需要 GPU 顺序时走 GPU scan（规范保证）"）——见 `scan.wgsl`。
//! 消费者：稠密顶点 compaction（`gsn::compact_mesh`）；后续稀疏体素格写侧的扫描/压缩也用它。

use crate::jfa::{Headless, readback_u32, storage_entry, uniform_entry};
use wgpu::util::DeviceExt;

const WG: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    nb: u32,
    _p0: u32,
    _p1: u32,
}

/// 排他前缀和（返回 (每元素排他和, 总数)；空输入 → ([], 0)）。
pub fn exclusive_prefix_sum_u32(
    hd: &Headless,
    data: &[u32],
    timer: Option<&mut crate::timer::GpuTimer>,
) -> (Vec<u32>, u32) {
    if data.is_empty() {
        return (Vec::new(), 0);
    }
    let out = exclusive_prefix_sum_buf(hd, data, timer);
    let vals = readback_u32(hd, &out);
    let total = vals[data.len() - 1] + data[data.len() - 1];
    (vals, total)
}

/// 只跑三段式扫描、返回输出缓冲（上传输入；供 GPU 驻留消费方，不回读）。
pub(crate) fn exclusive_prefix_sum_buf(
    hd: &Headless,
    data: &[u32],
    timer: Option<&mut crate::timer::GpuTimer>,
) -> wgpu::Buffer {
    let in_buf = hd
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scan-in"),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE,
        });
    scan_buffers(hd, &in_buf, data.len() as u32, timer)
}

/// 对既有输入缓冲（长度 n 个 u32）跑三段式扫描——免上传往返（runner 直出链路用）。
pub(crate) fn exclusive_prefix_sum_buf_from(
    hd: &Headless,
    in_buf: &wgpu::Buffer,
    n: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> wgpu::Buffer {
    assert!(n > 0, "扫描长度须 ≥ 1");
    scan_buffers(hd, in_buf, n, timer)
}

fn scan_buffers(
    hd: &Headless,
    in_buf: &wgpu::Buffer,
    n: u32,
    mut timer: Option<&mut crate::timer::GpuTimer>,
) -> wgpu::Buffer {
    let nb = n.div_ceil(WG);
    assert!(nb > 0 && (nb as u64) <= 1 << 16, "块数超上限（nb={nb}）");
    let device = &hd.device;
    let queue = &hd.queue;

    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scan-out"),
        size: (n * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let sums_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scan-sums"),
        size: (nb * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let off_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scan-offsets"),
        size: (nb * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scan-params"),
        contents: bytemuck::bytes_of(&Params {
            n,
            nb,
            _p0: 0,
            _p1: 0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("scan.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scan-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, false),
            storage_entry(2, false),
            storage_entry(3, false),
            uniform_entry(4),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scan-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let mk = |label: &str, entry: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pl),
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    };
    let block_scan = mk("scan-block", "block_scan");
    let block_carry = mk("scan-carry", "block_carry");
    let scan_final = mk("scan-final", "scan_final");

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scan-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: in_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: sums_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: off_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });

    // 三段同 encoder（pass 间隐式 barrier）：block → carry → final
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    for (pipe, wgs) in [(&block_scan, nb), (&block_carry, 1), (&scan_final, nb)] {
        let tw = timer.as_deref_mut().and_then(|t| t.writes());
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: tw,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(wgs, 1, 1);
    }
    queue.submit([enc.finish()]);

    out_buf
}
