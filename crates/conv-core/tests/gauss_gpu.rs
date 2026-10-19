//! GPU splat 判据：整数原子累加的逐位确定性 + 与 CPU f32 场的容差对拍 + 拓扑一致。

use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::voxel::{Backend, VoxelMode, VoxelizeOptions, voxelize, voxelize_gpu_with};
use conv_core::gauss::{blob_sphere_gaussians, sphere_gaussians};
use conv_core::jfa::try_headless_device;
use std::sync::Mutex;
use std::time::Instant;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn opts(mode: VoxelMode, n: u32) -> VoxelizeOptions {
    VoxelizeOptions {
        mode,
        n,
        threads: 1,
        ..VoxelizeOptions::default()
    }
}

fn field_hash(values: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in values {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn max_field_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

#[test]
fn gpu_splat_matches_cpu_within_tolerance() {
    let _g = GPU_LOCK.lock().unwrap();
    let t = Instant::now();
    let hd = try_headless_device().expect("headless gpu");
    println!("gpu device init: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);

    let gs = sphere_gaussians(4_000, 0.75, 0.99);
    let cpu_opts = opts(VoxelMode::Surfel, 48);
    let cpu = voxelize(&gs, &cpu_opts).expect("cpu surfel");
    let gpu_opts = VoxelizeOptions {
        backend: Backend::Gpu,
        ..cpu_opts
    };
    let t = Instant::now();
    let g1 = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu surfel 1");
    let first_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let g2 = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu surfel 2");
    let second_ms = t.elapsed().as_secs_f64() * 1e3;
    println!("gpu surfel device-reuse: first={first_ms:.1} ms second={second_ms:.1} ms");
    assert_eq!(g1.backend, Backend::Gpu);
    assert_eq!(g1.workers, 0);
    assert_eq!(
        field_hash(&g1.values),
        field_hash(&g2.values),
        "gpu surfel determinism"
    );
    let diff = max_field_diff(&cpu.values, &g1.values);
    let mc = field_to_mesh(&cpu);
    let mg = field_to_mesh(&g1);
    let rel = ((mc.stats.volume - mg.stats.volume) / mc.stats.volume).abs();
    println!(
        "gpu surfel: max_diff={diff:.6} cpu_vol={:.5} gpu_vol={:.5} rel={rel:.5} cpu_chi={} gpu_chi={}",
        mc.stats.volume, mg.stats.volume, mc.stats.chi, mg.stats.chi
    );
    // 低权重体素的 den 在 min_weight 附近，定点量化可能翻转"观测/未观测"判定；
    // 拓扑与体积才是 C22 的几何判据，场值给 3e-2 带。
    assert!(diff < 0.03, "surfel field max diff {diff}");
    assert!(rel < 0.02, "surfel volume rel {rel}");
    assert_eq!(mc.stats.chi, mg.stats.chi, "surfel chi");
    assert_eq!(
        mc.stats.components, mg.stats.components,
        "surfel components"
    );

    let gs = blob_sphere_gaussians(3_000, 0.75, 0.05, 0.95, 7);
    let cpu_opts = opts(VoxelMode::Density, 48);
    let cpu = voxelize(&gs, &cpu_opts).expect("cpu density");
    let gpu_opts = VoxelizeOptions {
        backend: Backend::Gpu,
        ..cpu_opts
    };
    let t = Instant::now();
    let d1 = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu density 1");
    let first_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let d2 = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu density 2");
    let second_ms = t.elapsed().as_secs_f64() * 1e3;
    println!("gpu density device-reuse: first={first_ms:.1} ms second={second_ms:.1} ms");
    assert_eq!(
        field_hash(&d1.values),
        field_hash(&d2.values),
        "gpu density determinism"
    );
    let diff = max_field_diff(&cpu.values, &d1.values);
    let mc = field_to_mesh(&cpu);
    let md = field_to_mesh(&d1);
    let rel = ((mc.stats.volume - md.stats.volume) / mc.stats.volume).abs();
    println!(
        "gpu density: max_diff={diff:.6} cpu_iso={:.4} gpu_iso={:.4} cpu_vol={:.5} gpu_vol={:.5} rel={rel:.5} cpu_chi={} gpu_chi={}",
        cpu.iso, d1.iso, mc.stats.volume, md.stats.volume, mc.stats.chi, md.stats.chi
    );
    // 密度场在 iso 附近做二值密封：微小量化会让边界体素从 0 翻到 +outside；
    // 原始场最大差不作为判据——看 iso 差 + 网格体积/拓扑。
    assert!((cpu.iso - d1.iso).abs() < 0.01, "density iso");
    assert!(rel < 0.05, "density volume rel {rel}");
    assert_eq!(mc.stats.chi, md.stats.chi, "density chi");
    assert_eq!(
        mc.stats.components, md.stats.components,
        "density components"
    );
}
