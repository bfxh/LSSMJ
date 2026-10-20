//! GPU 侧 iso + 密封：density 直方图求 q99.9、frontier BFS 标 outside、写密封场。

use super::*;
use crate::gauss::voxel::Backend;
use crate::jfa::{readback_f32, storage_entry, uniform_entry};
use wgpu::util::DeviceExt;

fn readback_u32_range(hd: &Headless, buf: &wgpu::Buffer, offset: u64, len: usize) -> Vec<u32> {
    let size = (len * 4) as u64;
    let download = hd.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("post-readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = hd
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(buf, offset, &download, 0, size);
    hd.queue.submit([enc.finish()]);
    let slice = download.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    hd.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = slice.get_mapped_range().unwrap();
    bytemuck::allocation::pod_collect_to_vec(&data)
}

fn make_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    pl: &wgpu::PipelineLayout,
    entry: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: Some(pl),
        module,
        entry_point: Some(entry),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn dispatch(
    hd: &Headless,
    pipe: &wgpu::ComputePipeline,
    bg: &wgpu::BindGroup,
    params_buf: &wgpu::Buffer,
    params: &mut PostParams,
    total: u32,
    label: &str,
) -> Result<(), String> {
    let (x, y) = dispatch_dims(total)?;
    params.dispatch_x = x;
    hd.queue
        .write_buffer(params_buf, 0, bytemuck::bytes_of(params));
    let mut enc = hd
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, bg, &[]);
        pass.dispatch_workgroups(x, y, 1);
    }
    hd.queue.submit([enc.finish()]);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn post_gpu(
    hd: &Headless,
    density: Option<&wgpu::Buffer>,
    num: Option<&wgpu::Buffer>,
    den: Option<&wgpu::Buffer>,
    grid: Grid,
    opts: &VoxelizeOptions,
    bounds: Aabb,
    mode: VoxelMode,
) -> Result<Field, String> {
    let device = &hd.device;
    let queue = &hd.queue;
    let n = grid.n;
    let count = (n * n * n) as usize;
    let dummy = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("post-dummy"),
        contents: &[0u8; 4],
        usage: wgpu::BufferUsages::STORAGE,
    });
    let density = density.unwrap_or(&dummy);
    let num = num.unwrap_or(&dummy);
    let den = den.unwrap_or(&dummy);

    let mask_a = buffer_uninit(device, "post-mask-a", count * 4);
    let mask_b = buffer_cleared(hd, "post-mask-b", count * 4);
    let counters_hist = buffer_cleared(hd, "post-counters-hist", (8 + 65536) * 4);
    let field_buf = buffer_uninit(device, "post-field", count * 4);
    let mut params = PostParams {
        n,
        mode: if mode == VoxelMode::Density { 0 } else { 1 },
        count: 0,
        dispatch_x: 0,
        step: 0,
        seal: u32::from(opts.seal),
        pad1: 0,
        pad2: 0,
        iso: 0.0,
        outside: 0.0,
        min_weight: opts.min_weight,
        max_density: 0.0,
        dens_scale: DENS_SCALE,
        den_scale: DEN_SCALE,
        dist_scale: DIST_SCALE,
        pad3: 0.0,
    };
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("post-params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("../post.wgsl"));
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("post-bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, true),
            storage_entry(5, false),
            storage_entry(6, false),
            storage_entry(8, false),
            uniform_entry(9),
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("post-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let stats_pipe = make_pipeline(device, &module, &pl, "stats");
    let hist_pipe = make_pipeline(device, &module, &pl, "histogram");
    let init_pipe = make_pipeline(device, &module, &pl, "seal_init");
    let dilate_pipe = make_pipeline(device, &module, &pl, "dilate");
    let field_density_pipe = make_pipeline(device, &module, &pl, "field_density");
    let field_surfel_pipe = make_pipeline(device, &module, &pl, "field_surfel");

    let make_bg = |fin: &wgpu::Buffer, fout: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: density.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: num.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: den.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: fin.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: fout.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: counters_hist.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: field_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        })
    };
    let bg_ab = make_bg(&mask_a, &mask_b);
    let bg_ba = make_bg(&mask_b, &mask_a);

    let (iso, outside_val) = if mode == VoxelMode::Density {
        dispatch(
            hd,
            &stats_pipe,
            &bg_ab,
            &params_buf,
            &mut params,
            count as u32,
            "post-stats",
        )?;
        let c = readback_u32_range(hd, &counters_hist, 0, 6);
        let max = c[4];
        let positive = c[5];
        if positive == 0 {
            return Err("GPU 密度场全为 0".into());
        }
        let step = (max / 65536) + 1;
        params.step = step;
        dispatch(
            hd,
            &hist_pipe,
            &bg_ab,
            &params_buf,
            &mut params,
            count as u32,
            "post-hist",
        )?;
        let hist = readback_u32_range(hd, &counters_hist, 8 * 4, 65536);
        let rank = (((positive - 1) as f32) * 0.999).round() as u32;
        let mut acc = 0u32;
        let mut target = None;
        for (i, &v) in hist.iter().enumerate() {
            if v == 0 {
                continue;
            }
            if acc.saturating_add(v) > rank {
                target = Some(i);
                break;
            }
            acc = acc.saturating_add(v);
        }
        let bin = target.ok_or("GPU iso 直方图未命中")?;
        let q999 = (bin as u32).saturating_mul(step) as f32 / DENS_SCALE;
        let iso = opts.iso_abs.unwrap_or(opts.iso_frac * q999);
        if iso <= 0.0 || !iso.is_finite() {
            return Err(format!("无效阈值 iso={iso}"));
        }
        (iso, q999)
    } else {
        (0.0, 3.0 * grid.h)
    };
    params.iso = iso;
    params.outside = outside_val;

    // seal_init 写 mask_b；之后每趟 in/out 互换（全格 ping-pong），每趟只读一份、只写另一份。
    let mut current_is_a = false;
    if opts.seal {
        dispatch(
            hd,
            &init_pipe,
            &bg_ab,
            &params_buf,
            &mut params,
            count as u32,
            "post-seal-init",
        )?;
        let (wg_x, wg_y) = dispatch_dims(count as u32)?;
        params.dispatch_x = wg_x;
        queue.write_buffer(&params_buf, 0, bytemuck::bytes_of(&params));
        // 每批只在批末回读一次；回读（poll）比泛洪趟数贵，批量取 64 封顶。
        let batch = n.min(64);
        let max_passes = 2 * n;
        let mut passes = 0u32;
        let mut converged = false;
        while passes < max_passes {
            let this_batch = batch.min(max_passes - passes);
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("post-seal-dilate"),
            });
            for _ in 0..this_batch {
                // 每趟清零"有变化"标志：批次最后一趟仍无变化 ⇒ 泛洪已到不动点。
                enc.clear_buffer(&counters_hist, 4, Some(4));
                let bg = if current_is_a { &bg_ab } else { &bg_ba };
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("post-seal-dilate"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&dilate_pipe);
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(wg_x, wg_y, 1);
                current_is_a = !current_is_a;
            }
            queue.submit([enc.finish()]);
            passes += this_batch;
            let c = readback_u32_range(hd, &counters_hist, 0, 6);
            if c[1] == 0 {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(format!("GPU 密封未收敛（{max_passes} 趟泛洪后仍有变化）"));
        }
    }

    queue.write_buffer(&counters_hist, 8, &[0u8; 8]);
    params.count = 0;
    let field_pipe = if mode == VoxelMode::Density {
        &field_density_pipe
    } else {
        &field_surfel_pipe
    };
    let field_bg = if current_is_a { &bg_ab } else { &bg_ba };
    dispatch(
        hd,
        field_pipe,
        field_bg,
        &params_buf,
        &mut params,
        count as u32,
        "post-field",
    )?;
    let values = readback_f32(hd, &field_buf);
    let c = readback_u32_range(hd, &counters_hist, 0, 6);
    let observed = c[2] as usize;
    let sealed = c[3] as usize;
    if observed == 0 {
        return Err("GPU 后处理没有观测到任何体素".into());
    }
    Ok(Field {
        values,
        grid,
        mode,
        backend: Backend::Gpu,
        bins3d: false,
        simd_prep: opts.simd_prep,
        iso,
        observed,
        sealed_interior: sealed,
        workers: 0,
        bounds,
    })
}
