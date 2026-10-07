//! GPU Surface Nets（T-GC-02；分块 = T-GC-05）：SDF→mesh，逐语义复刻 fast-surface-nets 0.2.1。
//!
//! 设计：顶点稀疏存储（槽位 = cell 线性索引，n³ 布局），判据用 CPU 侧 `surface_points`
//! 做 cell 映射对拍。
//! 分块契约（T-GC-05 第一片，锚 `W15A-034/035`）：cell 域 [0, n−1) 按 `chunk_cells` 分块；
//!   每块在"值切片 [origin, b]（含 1-voxel halo：origin = 块起−1，钳 0）+ cell 运行区间
//!   [origin, b)"上执行，quad 归属过滤 cell ≥ 块起（halo 层的 quad 属邻块）——
//!   顶点/索引槽位保持全局 n³ 布局 ⇒ 分块与整块输出**全缓冲逐位可对拍**（接缝零缝判据）。
//! 遍历口径（T-GC-05 第二片）：块表 = 活跃块列表（值块 [a, b]³ 双符号并存的保守占据检测）；
//!   每块 params 与值切片打包进数组/单缓冲，**单趟 dispatch 跑全部活跃块**；块哈希结构
//!   （无界场景）与 GPU 侧占据检测/indirect dispatch 属后续片。
//! 稠密化（T-GC-05 第三/四片）：`compact_mesh`（吃回读产物）与 `surface_nets_gpu_compact`
//!   （runner 直出，gen/emit 后全程 GPU 驻留，免稀疏 n³ 回写往返）共用同一条后链
//!   `compact_chain`（scan → 散射 + 重映射）。
//! 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"
//! 排序后对拍——四边形集合与绕序确定 ⇒ 排序流逐位确定。
//! 教训留档：base/前缀 + write_buffer 路线曾出现"输入逐位一致、输出随机"的未解非确定性
//! （证据在 w15c 判据档），故弃用该机械；本片全程不 write_buffer，需要 GPU 顺序时走 GPU scan。

use crate::jfa::{
    Headless, readback_f32, readback_u32, readback_u32_slice, storage_entry, uniform_entry,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct P {
    n: u32,
    wg_axis: u32,
    slice_offset: u32,
    _pad0: u32,
    nv: [u32; 3],
    _pad1: u32,
    origin: [u32; 3],
    _pad2: u32,
    hi: [u32; 3],
    _pad3: u32,
    chunk_min: [u32; 3],
    _pad4: u32,
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

/// 分块运行统计（空块跳过可观察）。
pub struct ChunkStats {
    pub chunks: u32,
    pub active: u32,
}

/// 稠密网格（T-GC-05 第三片）：顶点按 cell 序打包 + 索引重映射。
pub struct CompactMesh {
    /// 稠密顶点（顺序 = cell 线性索引序，逐位保真）
    pub positions: Vec<[f32; 3]>,
    /// 重映射后的三角索引（引用 `positions`）
    pub indices: Vec<u32>,
    pub vertex_count: u32,
}

/// 一次分块运行的 GPU 驻留产物（稀疏缓冲未回读）。
struct BlocksRun {
    vtx_pos_buf: wgpu::Buffer,
    vtx_flag_buf: wgpu::Buffer,
    idx_buf: wgpu::Buffer,
    counter_buf: wgpu::Buffer,
    stats: ChunkStats,
}

/// 块表 + gen/emit 两个 pass（T-GC-05 第一/二片的前半，各入口共用）。
fn run_blocks(
    hd: &Headless,
    sdf: &[f32],
    n: u32,
    chunk_cells: u32,
    mut timer: Option<&mut crate::timer::GpuTimer>,
) -> BlocksRun {
    assert!(chunk_cells >= 1, "chunk_cells 必须 ≥ 1");
    let device = &hd.device;
    let queue = &hd.queue;
    let count = (n * n * n) as usize;

    // ① 块表：枚举块 → 占据检测（保守：值块 [a, b]³ 双符号才活跃）→ 打包 params + 值切片
    let cell_axis = n - 1;
    let per_axis = cell_axis.div_ceil(chunk_cells);
    // 每块每轴 workgroup 数（覆盖所有块的最大运行区域 = chunk_cells + 1 个 cell；多余由 hi 守卫剪掉）
    let wg_axis = (chunk_cells + 1).div_ceil(4);
    let mut stats = ChunkStats {
        chunks: 0,
        active: 0,
    };
    let mut params: Vec<P> = Vec::new();
    let mut packed: Vec<f32> = Vec::new();
    for cz in 0..per_axis {
        for cy in 0..per_axis {
            for cx in 0..per_axis {
                let a = [cx * chunk_cells, cy * chunk_cells, cz * chunk_cells];
                let b = [
                    (a[0] + chunk_cells).min(cell_axis),
                    (a[1] + chunk_cells).min(cell_axis),
                    (a[2] + chunk_cells).min(cell_axis),
                ];
                stats.chunks += 1;
                if !block_has_both_signs(sdf, n, a, b) {
                    continue;
                }
                stats.active += 1;
                // 1-voxel halo：运行区间与值切片同起（块起−1，钳 0）；切片上界 = cell 区间上界
                let origin = [
                    a[0].saturating_sub(1),
                    a[1].saturating_sub(1),
                    a[2].saturating_sub(1),
                ];
                let nv = [
                    b[0] - origin[0] + 1,
                    b[1] - origin[1] + 1,
                    b[2] - origin[2] + 1,
                ];
                params.push(P {
                    n,
                    wg_axis,
                    slice_offset: packed.len() as u32,
                    _pad0: 0,
                    nv,
                    _pad1: 0,
                    origin,
                    _pad2: 0,
                    hi: b,
                    _pad3: 0,
                    chunk_min: a,
                    _pad4: 0,
                });
                packed.extend(extract_region(sdf, n, origin, b));
            }
        }
    }

    // ② 共享输出缓冲（全局 n³ 槽位；各块写入互不相交或同值重叠——halo cell 被两块重写同值）
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

    // ③ 单趟遍历：块表（params 数组 + 打包切片）→ 2 个 pass 跑全部活跃块
    if !params.is_empty() {
        let sdf_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gsn-sdf-packed"),
            contents: bytemuck::cast_slice(&packed),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gsn-params"),
            contents: bytemuck::cast_slice(&params),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let module = device.create_shader_module(wgpu::include_wgsl!("gsn.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gsn-bgl"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, false),
                storage_entry(2, false),
                storage_entry(3, true),
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

        for pipe in [&gen_pipe, &emit_pipe] {
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
                // x 轴展平 (活跃块 × wg_axis)，块内坐标 = (余数, y, z)
                pass.dispatch_workgroups(stats.active * wg_axis, wg_axis, wg_axis);
            }
            queue.submit([enc.finish()]);
        }
    }

    BlocksRun {
        vtx_pos_buf,
        vtx_flag_buf,
        idx_buf,
        counter_buf,
        stats,
    }
}

/// GPU Surface Nets（整块 = 单块运行器的退化情形）。
pub fn surface_nets_gpu(
    hd: &Headless,
    sdf: &[f32],
    n: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> GsnMesh {
    surface_nets_gpu_chunked(hd, sdf, n, n - 1, timer).0
}

/// 分块 GPU Surface Nets：cell 域 [0, n−1) 按 `chunk_cells` 均分（末块可短）。
/// 1-voxel halo 契约 + 空块跳过 + 单趟遍历；输出槽位与整块同布局（全局 n³）。
pub fn surface_nets_gpu_chunked(
    hd: &Headless,
    sdf: &[f32],
    n: u32,
    chunk_cells: u32,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> (GsnMesh, ChunkStats) {
    let run = run_blocks(hd, sdf, n, chunk_cells, timer);
    let count = (n * n * n) as usize;

    // 回读（与整块同布局；无活跃块时零初始化缓冲即空输出）
    let quad_count = readback_f32(hd, &run.counter_buf)
        .into_iter()
        .map(f32::to_bits)
        .next()
        .unwrap_or(0);
    let mut indices = readback_f32(hd, &run.idx_buf)
        .into_iter()
        .map(f32::to_bits)
        .collect::<Vec<u32>>();
    indices.truncate((quad_count as usize) * 6);
    let mut positions = Vec::with_capacity(count);
    let pos_raw = readback_f32(hd, &run.vtx_pos_buf);
    for chunk in pos_raw.as_chunks::<4>().0 {
        positions.push([chunk[0], chunk[1], chunk[2]]);
    }
    let flags: Vec<u32> = readback_f32(hd, &run.vtx_flag_buf)
        .into_iter()
        .map(f32::to_bits)
        .collect();

    (
        GsnMesh {
            positions,
            flags,
            indices,
            quad_count,
            n,
        },
        run.stats,
    )
}

/// 分块 GSN **直出稠密流**（T-GC-05 第四片）：gen/emit 之后全程 GPU 驻留——
/// scan(flags) → 散射顶点 + 重映射索引 → 只回读稠密流（免稀疏 n³ 回写往返）。
/// 语义与 `surface_nets_gpu_chunked` + `compact_mesh` 两步法等价（判据对拍两者）。
pub fn surface_nets_gpu_compact(
    hd: &Headless,
    sdf: &[f32],
    n: u32,
    chunk_cells: u32,
    mut timer: Option<&mut crate::timer::GpuTimer>,
) -> (CompactMesh, ChunkStats) {
    let run = run_blocks(hd, sdf, n, chunk_cells, timer.as_deref_mut());
    let quad_count = readback_u32(hd, &run.counter_buf)
        .first()
        .copied()
        .unwrap_or(0);
    let count = quad_count * 6;
    let n3 = n * n * n;
    let (pos_out_buf, idx_out_buf, total) = compact_chain(
        hd,
        &run.vtx_flag_buf,
        &run.vtx_pos_buf,
        &run.idx_buf,
        n3,
        count,
        timer,
    );
    (
        readback_dense(hd, &pos_out_buf, &idx_out_buf, total, count),
        run.stats,
    )
}

/// 稀疏 `GsnMesh`（n³ 槽位）→ 稠密流：上传稀疏三件套 → 同一后链 → 回读稠密流。
/// （runner 直出免往返见 `surface_nets_gpu_compact`。）
pub fn compact_mesh(
    hd: &Headless,
    mesh: &GsnMesh,
    timer: Option<&mut crate::timer::GpuTimer>,
) -> CompactMesh {
    let device = &hd.device;
    let n3 = mesh.flags.len();
    assert_eq!(n3, (mesh.n * mesh.n * mesh.n) as usize, "flags 长度非 n³");
    assert_eq!(
        mesh.indices.len(),
        mesh.quad_count as usize * 6,
        "索引长度与四边形数不符"
    );
    let count = mesh.indices.len() as u32;
    let pos4: Vec<[f32; 4]> = mesh
        .positions
        .iter()
        .map(|p| [p[0], p[1], p[2], 1.0])
        .collect();
    let flags_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cp-flags"),
        contents: bytemuck::cast_slice(&mesh.flags),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let pos_in_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cp-pos-in"),
        contents: bytemuck::cast_slice(&pos4),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let idx_in_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cp-idx-in"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let (pos_out_buf, idx_out_buf, total) = compact_chain(
        hd,
        &flags_buf,
        &pos_in_buf,
        &idx_in_buf,
        n3 as u32,
        count,
        timer,
    );
    readback_dense(hd, &pos_out_buf, &idx_out_buf, total, count)
}

/// 稠密化后链（GPU 驻留）：scan(flags) → 散射 + 重映射 → 稠密输出缓冲。
/// 返回 (稠密位置缓冲, 稠密索引缓冲, 顶点总数)；总数 = scan 尾元素 + 尾 flag。
fn compact_chain(
    hd: &Headless,
    flags_buf: &wgpu::Buffer,
    pos_in_buf: &wgpu::Buffer,
    idx_in_buf: &wgpu::Buffer,
    n3: u32,
    count: u32,
    mut timer: Option<&mut crate::timer::GpuTimer>,
) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    let device = &hd.device;
    let queue = &hd.queue;
    let n3u = n3 as usize;
    let last = ((n3u - 1) as u64) * 4;
    let cellmap_buf =
        crate::scan::exclusive_prefix_sum_buf_from(hd, flags_buf, n3, timer.as_deref_mut());
    let scan_last = readback_u32_slice(hd, &cellmap_buf, last, 4)[0];
    let flags_last = readback_u32_slice(hd, flags_buf, last, 4)[0];
    let total = scan_last + flags_last;

    let pos_out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cp-pos-out"),
        size: ((total as u64) * 16).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let idx_out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cp-idx-out"),
        size: ((count as u64) * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cp-params"),
        contents: bytemuck::bytes_of(&CP {
            n: n3,
            count,
            _p0: 0,
            _p1: 0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    if total > 0 || count > 0 {
        let module = device.create_shader_module(wgpu::include_wgsl!("compact.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cp-bgl"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, true),
                storage_entry(2, true),
                storage_entry(3, false),
                storage_entry(4, true),
                storage_entry(5, false),
                uniform_entry(6),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cp-pl"),
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
        let scatter = mk("cp-scatter", "scatter_vertices");
        let remap = mk("cp-remap", "remap_indices");
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cp-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: flags_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellmap_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pos_in_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pos_out_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: idx_in_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: idx_out_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        if total > 0 {
            let tw = timer.as_mut().and_then(|t| t.writes());
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: tw,
            });
            pass.set_pipeline(&scatter);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(n3.div_ceil(64), 1, 1);
        }
        if count > 0 {
            let tw = timer.as_mut().and_then(|t| t.writes());
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: tw,
            });
            pass.set_pipeline(&remap);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        queue.submit([enc.finish()]);
    }

    (pos_out_buf, idx_out_buf, total)
}

/// 回读稠密输出（位置 total 个 / 索引截到 count）。
fn readback_dense(
    hd: &Headless,
    pos_out_buf: &wgpu::Buffer,
    idx_out_buf: &wgpu::Buffer,
    total: u32,
    count: u32,
) -> CompactMesh {
    let mut positions = Vec::with_capacity(total as usize);
    for c in readback_f32(hd, pos_out_buf).as_chunks::<4>().0 {
        positions.push([c[0], c[1], c[2]]);
    }
    let mut indices = readback_u32(hd, idx_out_buf);
    indices.truncate(count as usize);
    CompactMesh {
        positions,
        indices,
        vertex_count: total,
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CP {
    n: u32,
    count: u32,
    _p0: u32,
    _p1: u32,
}

/// 值块 [a, b]³（每轴闭区间）双符号并存 ⇒ 可能存在表面 cell（保守占据检测，空块跳过用）。
fn block_has_both_signs(sdf: &[f32], n: u32, a: [u32; 3], b: [u32; 3]) -> bool {
    let (mut pos, mut neg) = (false, false);
    for z in a[2]..=b[2] {
        for y in a[1]..=b[1] {
            let base = (y * n + z * n * n) as usize;
            for x in a[0]..=b[0] {
                if sdf[base + x as usize] < 0.0 {
                    neg = true;
                } else {
                    pos = true;
                }
                if pos && neg {
                    return true;
                }
            }
        }
    }
    false
}

/// 值切片提取：每轴闭区间 [lo, hi]（lo = 块起−1 的 halo，hi = cell 区间上界）。
fn extract_region(sdf: &[f32], n: u32, lo: [u32; 3], hi: [u32; 3]) -> Vec<f32> {
    let w = [
        (hi[0] - lo[0] + 1) as usize,
        (hi[1] - lo[1] + 1) as usize,
        (hi[2] - lo[2] + 1) as usize,
    ];
    let mut out = Vec::with_capacity(w[0] * w[1] * w[2]);
    for z in lo[2]..=hi[2] {
        for y in lo[1]..=hi[1] {
            let base = (y * n + z * n * n) as usize;
            for x in lo[0]..=hi[0] {
                out.push(sdf[base + x as usize]);
            }
        }
    }
    out
}
