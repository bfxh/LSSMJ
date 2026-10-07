//! GPU Surface Nets（T-GC-02；分块/全 GPU 遍历 = T-GC-05）：SDF→mesh，逐语义复刻 fast-surface-nets 0.2.1。
//!
//! 设计：顶点稀疏存储（槽位 = cell 线性索引，n³ 布局），判据用 CPU 侧 `surface_points`
//! 做 cell 映射对拍。
//! 分块契约（T-GC-05 第一片，锚 `W15A-034/035`）：cell 域 [0, n−1) 按 `chunk_cells` 分块；
//!   每块在"cell 运行区间 [origin, hi)（origin = 块起−1 钳 0，即 1-voxel halo）"上执行，
//!   quad 归属过滤 cell ≥ 块起（halo 层的 quad 属邻块）——顶点/索引槽位保持全局 n³ 布局
//!   ⇒ 分块与整块输出**全缓冲逐位可对拍**（接缝零缝判据）。
//! 遍历口径（T-GC-05 第二/五片）：块表 = 活跃块列表；**占据检测、活跃表、indirect 参数、
//!   gen/emit 全在 GPU**（occupancy → scan → build_active → indirect_args →
//!   dispatch_workgroups_indirect）；每块参数由块号在 shader 内推导，sdf 全局索引直读
//!   （旧"CPU 双符号扫描 + 值切片打包 + params 数组"机械已随第五片移除）。
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
struct RP {
    n: u32,
    per_axis: u32,
    chunk_cells: u32,
    wg_axis: u32,
    occ_wg: u32,
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

/// 分块运行统计（空块跳过可观察；活跃数由 GPU 占据检测读出）。
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

/// 全 GPU 遍历的运行前半（T-GC-05 第五片）：occupancy → scan → build_active →
/// indirect_args → gen/emit（dispatch_workgroups_indirect）。返回值全部 GPU 驻留。
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

    let cell_axis = n - 1;
    let per_axis = cell_axis.div_ceil(chunk_cells);
    let wg_axis = (chunk_cells + 1).div_ceil(4);
    let total_blocks = (per_axis * per_axis * per_axis) as usize;
    assert!(
        total_blocks <= (1 << 16),
        "块数超 scan 上限（total_blocks={total_blocks}）"
    );
    // 占据检测：逐 cell 一线程（grid-stride；workgroup 数封顶 65535）
    let cells_axis = (n - 1) as u64;
    let total_cells = cells_axis * cells_axis * cells_axis;
    let occ_wg = (total_cells.div_ceil(64)).min(65535) as u32;

    let sdf_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-sdf"),
        contents: bytemuck::cast_slice(sdf),
        usage: wgpu::BufferUsages::STORAGE,
    });
    // 共享输出缓冲（全局 n³ 槽位；各块写入互不相交或同值重叠——halo cell 被两块重写同值）
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
    let block_flag_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-block-flag"),
        contents: &vec![0u8; total_blocks * 4],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    // scan 输出缓冲先建（绑定组需要它存在），数据在 occupancy 之后由 scan 写入
    let blockmap_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gsn-blockmap"),
        size: (total_blocks * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let active_list_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gsn-active-list"),
        size: (total_blocks * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    // indirect 参数：(x, y, z) u32——只在"写者"绑定组出现（gen/emit 的组不含它：
    // indirect 源缓冲不得与该 dispatch 的 rw 绑定同作用域）
    let args_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gsn-indirect-args"),
        size: 12,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        mapped_at_creation: false,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("gsn.wgsl"));
    // 两组布局：写者（occupancy/build_active/build_indirect_args）与 gen/emit——
    // gen/emit 的组不含 indirect_args（indirect 源不得与其 rw 同作用域）；各自 storage 数 ≤ 8
    let bgl_a = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gsn-bgl-writers"),
        entries: &[
            storage_entry(0, true),
            storage_entry(3, false),
            storage_entry(4, true),
            storage_entry(5, false),
            storage_entry(6, false),
            uniform_entry(9),
        ],
    });
    let bgl_b = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gsn-bgl-gen"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, false),
            storage_entry(2, false),
            storage_entry(5, false),
            storage_entry(7, false),
            storage_entry(8, false),
            uniform_entry(9),
        ],
    });
    let pl_a = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gsn-pl-writers"),
        bind_group_layouts: &[Some(&bgl_a)],
        immediate_size: 0,
    });
    let pl_b = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gsn-pl-gen"),
        bind_group_layouts: &[Some(&bgl_b)],
        immediate_size: 0,
    });
    let mk = |layout: &wgpu::PipelineLayout, label: &str, entry: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    };
    let occupancy_pipe = mk(&pl_a, "gsn-occupancy", "occupancy");
    let build_active_pipe = mk(&pl_a, "gsn-build-active", "build_active");
    let args_pipe = mk(&pl_a, "gsn-indirect-args", "build_indirect_args");
    let gen_pipe = mk(&pl_b, "gsn-gen", "gen_vertices");
    let emit_pipe = mk(&pl_b, "gsn-emit", "emit_quads");

    let rp_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gsn-rp"),
        contents: bytemuck::bytes_of(&RP {
            n,
            per_axis,
            chunk_cells,
            wg_axis,
            occ_wg,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bg_a = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gsn-bg-writers"),
        layout: &bgl_a,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: sdf_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: block_flag_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: blockmap_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: active_list_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: args_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: rp_buf.as_entire_binding(),
            },
        ],
    });
    let bg_b = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gsn-bg-gen"),
        layout: &bgl_b,
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
                binding: 5,
                resource: active_list_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: counter_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: idx_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: rp_buf.as_entire_binding(),
            },
        ],
    });

    // ① 占据检测（逐 cell 并行，原子置块标志）
    let blocks_wg = (total_blocks as u32).div_ceil(64);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let tw = timer.as_deref_mut().and_then(|t| t.writes());
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: tw,
        });
        pass.set_pipeline(&occupancy_pipe);
        pass.set_bind_group(0, &bg_a, &[]);
        pass.dispatch_workgroups(occ_wg, 1, 1);
    }
    queue.submit([enc.finish()]);

    // ② 块扫描（scan 积木：block_flag → blockmap 排他前缀和，写入既有缓冲）
    crate::scan::exclusive_prefix_sum_into(
        hd,
        &block_flag_buf,
        &blockmap_buf,
        total_blocks as u32,
        timer.as_deref_mut(),
    );
    let last = ((total_blocks - 1) as u64) * 4;
    let stats = ChunkStats {
        chunks: total_blocks as u32,
        active: readback_u32_slice(hd, &blockmap_buf, last, 4)[0]
            + readback_u32_slice(hd, &block_flag_buf, last, 4)[0],
    };

    // ③ 活跃表 + indirect 参数 + gen/emit（indirect dispatch）
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    for (pipe, wgs) in [(&build_active_pipe, blocks_wg), (&args_pipe, 1)] {
        let tw = timer.as_deref_mut().and_then(|t| t.writes());
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: tw,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg_a, &[]);
        pass.dispatch_workgroups(wgs, 1, 1);
    }
    for pipe in [&gen_pipe, &emit_pipe] {
        let tw = timer.as_deref_mut().and_then(|t| t.writes());
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: tw,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg_b, &[]);
        pass.dispatch_workgroups_indirect(&args_buf, 0);
    }
    queue.submit([enc.finish()]);

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
/// 1-voxel halo 契约 + GPU 侧空块跳过 + 单趟 indirect 遍历；输出槽位与整块同布局（全局 n³）。
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
