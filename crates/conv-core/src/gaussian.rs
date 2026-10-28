//! 高斯腿（T-GC-04 第一片）：planar 布局 + 恒等映射（粒子↔高斯同表示，无转换=无漂移）。
//!
//! 布局契约（锚 `W15A-041/042`：格式家族以 planar 为主、成员布局二选一须编译期钉死）：
//! **planar（SoA）**——五个独立平面（position / log-scale / rotation 四元数 (x,y,z,w) /
//! logit-opacity / SH-DC 颜色），每平面一条连续 f32 流、独立缓冲。packed（AoS）不提供；
//! 改平面集或次序 = 改格式契约，由金样哈希（`tests/gaussian_gpu.rs`）强制披露。
//! 恒等边（锚 `W15A-014/015`：物理-渲染同表示）：粒子（JFA 种子口径）与高斯共享
//! position 平面——零拷贝视图；GPU 侧 `identity_roundtrip` 把各平面经 compute pass 直通
//! （未来量化/变换阶段的挂载点），回读**逐位**一致（含 ±0 / 次正规 / ±inf / NaN）。
//! SH 高阶系数、PLY/SPZ 读入、planar 量化变体属后续片。

use crate::jfa::{Headless, readback_f32, storage_entry, uniform_entry};
use wgpu::util::DeviceExt;

/// 高斯云（planar 布局，五平面）。
pub struct GaussianCloud {
    pub count: u32,
    /// 平面：位置（世界单位）
    pub positions: Vec<[f32; 3]>,
    /// 平面：缩放（log 域）
    pub log_scales: Vec<[f32; 3]>,
    /// 平面：旋转四元数（x,y,z,w——次序即契约，不在此校验归一）
    pub rotations: Vec<[f32; 4]>,
    /// 平面：不透明度（logit 域）
    pub opacities: Vec<f32>,
    /// 平面：SH-DC 颜色（线性 RGB）
    pub colors: Vec<[f32; 3]>,
}

impl GaussianCloud {
    pub fn new(
        positions: Vec<[f32; 3]>,
        log_scales: Vec<[f32; 3]>,
        rotations: Vec<[f32; 4]>,
        opacities: Vec<f32>,
        colors: Vec<[f32; 3]>,
    ) -> Self {
        let count = positions.len();
        assert_eq!(log_scales.len(), count, "log_scales 平面长度不一致");
        assert_eq!(rotations.len(), count, "rotations 平面长度不一致");
        assert_eq!(opacities.len(), count, "opacities 平面长度不一致");
        assert_eq!(colors.len(), count, "colors 平面长度不一致");
        Self {
            count: count as u32,
            positions,
            log_scales,
            rotations,
            opacities,
            colors,
        }
    }

    /// 粒子（JFA 种子口径）↔ 高斯共享 position 平面：零拷贝视图（恒等边，锚 W15A-014/015）。
    pub fn particle_positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
}

/// 金样哈希（平面序固定：position → log_scale → rotation → opacity → color，
/// count 先入；逐位 FNV-1a）——格式契约的机器锚。
pub fn cloud_hash(c: &GaussianCloud) -> u64 {
    fn eat(h: &mut u64, v: f32) {
        for b in v.to_bits().to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in c.count.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for p in &c.positions {
        p.iter().for_each(|&v| eat(&mut h, v));
    }
    for p in &c.log_scales {
        p.iter().for_each(|&v| eat(&mut h, v));
    }
    for p in &c.rotations {
        p.iter().for_each(|&v| eat(&mut h, v));
    }
    c.opacities.iter().for_each(|&v| eat(&mut h, v));
    for p in &c.colors {
        p.iter().for_each(|&v| eat(&mut h, v));
    }
    h
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PT {
    n: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
}

/// 单平面 GPU 直通：上传 → copy pass → 回读（逐位）。
fn plane_passthrough(
    hd: &Headless,
    src_data: &[f32],
    bgl: &wgpu::BindGroupLayout,
    pipe: &wgpu::ComputePipeline,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> Vec<f32> {
    let device = &hd.device;
    let n = src_data.len() as u32;
    let src_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-src"),
        contents: bytemuck::cast_slice(src_data),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let dst_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gauss-dst"),
        size: (n as u64 * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gauss-params"),
        contents: bytemuck::bytes_of(&PT {
            n,
            _p0: 0,
            _p1: 0,
            _p2: 0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gauss-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: src_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: dst_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
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
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(n.div_ceil(64), 1, 1);
    }
    hd.queue.submit([enc.finish()]);
    readback_f32(hd, &dst_buf)
}

/// GPU 恒等直通：五平面各自经 copy pass → 回读同构云（逐位一致，无转换=无漂移）。
pub fn identity_roundtrip(
    hd: &Headless,
    cloud: &GaussianCloud,
    mut timer: Option<&mut crate::timer::GpuTimer>,
) -> GaussianCloud {
    let device = &hd.device;
    let module = device.create_shader_module(wgpu::include_wgsl!("gaussian.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gauss-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, false),
            uniform_entry(2),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gauss-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("gauss-passthrough"),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("passthrough_f32"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let flat3 = |p: &[[f32; 3]]| -> Vec<f32> { p.iter().flat_map(|v| v.iter().copied()).collect() };
    let flat4 = |p: &[[f32; 4]]| -> Vec<f32> { p.iter().flat_map(|v| v.iter().copied()).collect() };

    let pos = plane_passthrough(
        hd,
        &flat3(&cloud.positions),
        &bgl,
        &pipe,
        timer.as_deref_mut(),
    );
    let scl = plane_passthrough(
        hd,
        &flat3(&cloud.log_scales),
        &bgl,
        &pipe,
        timer.as_deref_mut(),
    );
    let rot = plane_passthrough(
        hd,
        &flat4(&cloud.rotations),
        &bgl,
        &pipe,
        timer.as_deref_mut(),
    );
    let opa = plane_passthrough(hd, &cloud.opacities, &bgl, &pipe, timer.as_deref_mut());
    let col = plane_passthrough(hd, &flat3(&cloud.colors), &bgl, &pipe, timer);

    let unflat3 = |v: &[f32]| -> Vec<[f32; 3]> {
        v.as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect()
    };
    let unflat4 = |v: &[f32]| -> Vec<[f32; 4]> {
        v.as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect()
    };
    GaussianCloud::new(
        unflat3(&pos),
        unflat3(&scl),
        unflat4(&rot),
        opa,
        unflat3(&col),
    )
}
