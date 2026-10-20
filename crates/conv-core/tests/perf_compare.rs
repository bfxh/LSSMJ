//! 完整性能对比（ignored 诊断）：同一场景比较
//! CPU 1T / CPU 24T / GPU 复用 / GPU + GPU 网格。
//!
//! 运行：
//! CARGO_INCREMENTAL=0 cargo test --release --test perf_compare -- \
//!   --test-threads=1 --ignored --nocapture

use conv_core::gauss::mesh::{field_to_mesh, field_to_mesh_gpu};
use conv_core::gauss::voxel::{Backend, VoxelMode, VoxelizeOptions, voxelize, voxelize_gpu_with};
use conv_core::gauss::{
    PruneOptions, add_floaters, blob_sphere_gaussians, prune, sphere_gaussians,
};
use conv_core::jfa::{Headless, try_headless_device};
use std::sync::Mutex;
use std::time::Instant;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn measure_cpu(
    gs: &[conv_core::gauss::Gaussian],
    opts: &VoxelizeOptions,
    reps: usize,
) -> (f64, f64) {
    let mut vox = Vec::with_capacity(reps);
    let mut mesh = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        let field = voxelize(gs, opts).expect("cpu voxelize");
        vox.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        let _ = field_to_mesh(&field);
        mesh.push(t.elapsed().as_secs_f64() * 1e3);
    }
    (median(vox), median(mesh))
}

fn measure_gpu(
    hd: &Headless,
    gs: &[conv_core::gauss::Gaussian],
    opts: &VoxelizeOptions,
    reps: usize,
    gpu_mesh: bool,
) -> (f64, f64, f64, f64) {
    let t = Instant::now();
    let field = voxelize_gpu_with(hd, gs, opts).expect("gpu warmup");
    let cold_vox = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    if gpu_mesh {
        let _ = field_to_mesh_gpu(hd, &field).expect("gpu mesh warmup");
    } else {
        let _ = field_to_mesh(&field);
    }
    let cold_mesh = t.elapsed().as_secs_f64() * 1e3;

    let mut vox = Vec::with_capacity(reps);
    let mut mesh = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        let field = voxelize_gpu_with(hd, gs, opts).expect("gpu voxelize");
        vox.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        if gpu_mesh {
            let _ = field_to_mesh_gpu(hd, &field).expect("gpu mesh");
        } else {
            let _ = field_to_mesh(&field);
        }
        mesh.push(t.elapsed().as_secs_f64() * 1e3);
    }
    (cold_vox, cold_mesh, median(vox), median(mesh))
}

#[test]
#[ignore = "diagnostic full performance comparison"]
fn perf_compare() {
    let _g = GPU_LOCK.lock().unwrap();
    let t = Instant::now();
    let hd = try_headless_device().expect("headless gpu");
    println!("PERF gpu_init={:.1}", t.elapsed().as_secs_f64() * 1e3);

    let sphere = sphere_gaussians(20_000, 0.75, 0.99);
    let mut blobs = blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
    add_floaters(&mut blobs, 300, 7 ^ 0x9e37_79b9);
    // 与 CLI 默认一致：先剪掉 300 个低不透明度 floaters，再拟合域。
    let (blobs, _) = prune(
        &blobs,
        &PruneOptions {
            min_opacity: 0.05,
            min_radius: 0.0,
            max_radius: 0.0,
        },
    );
    let scenes = [
        ("sphere", sphere.as_slice(), VoxelMode::Surfel),
        ("blobs", blobs.as_slice(), VoxelMode::Density),
    ];

    for n in [64u32, 128] {
        for (scene, gs, mode) in scenes {
            let base = VoxelizeOptions {
                mode,
                n,
                ..VoxelizeOptions::default()
            };

            let mut cpu1 = base.clone();
            cpu1.threads = 1;
            let (v, m) = measure_cpu(gs, &cpu1, 3);
            println!(
                "PERF n={n} scene={scene} variant=cpu1t vox={v:.1} mesh={m:.1} total={:.1}",
                v + m
            );

            let mut cpu24 = base.clone();
            cpu24.threads = 24;
            let (v, m) = measure_cpu(gs, &cpu24, 3);
            println!(
                "PERF n={n} scene={scene} variant=cpu24t vox={v:.1} mesh={m:.1} total={:.1}",
                v + m
            );

            let gpu = VoxelizeOptions {
                backend: Backend::Gpu,
                ..base.clone()
            };
            let (cv, cm, v, m) = measure_gpu(&hd, gs, &gpu, 3, false);
            println!(
                "PERF n={n} scene={scene} variant=gpu cold_vox={cv:.1} cold_mesh={cm:.1} vox={v:.1} mesh={m:.1} total={:.1}",
                v + m
            );

            let (cv, cm, v, m) = measure_gpu(&hd, gs, &gpu, 3, true);
            println!(
                "PERF n={n} scene={scene} variant=gpu_mesh cold_vox={cv:.1} cold_mesh={cm:.1} vox={v:.1} mesh={m:.1} total={:.1}",
                v + m
            );
        }
    }
}
