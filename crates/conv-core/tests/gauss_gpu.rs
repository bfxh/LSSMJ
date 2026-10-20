//! GPU splat 判据：整数原子累加的逐位确定性 + 与 CPU f32 场的容差对拍 + 拓扑一致。

use conv_core::gauss::mesh::{field_to_mesh, field_to_mesh_gpu};
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

    let gm1 = field_to_mesh_gpu(&hd, &g1).expect("gpu mesh surfel 1");
    let gm2 = field_to_mesh_gpu(&hd, &g1).expect("gpu mesh surfel 2");
    assert_eq!(gm1.stats.hash, gm2.stats.hash, "gpu mesher determinism");
    let mrel = ((mg.stats.volume - gm1.stats.volume) / mg.stats.volume).abs();
    println!(
        "gpu mesher surfel: cpu_vol={:.5} gpu_vol={:.5} rel={mrel:.6} tri={}/{} chi={}/{} verts={}/{}",
        mg.stats.volume,
        gm1.stats.volume,
        mg.stats.triangles,
        gm1.stats.triangles,
        mg.stats.chi,
        gm1.stats.chi,
        mg.stats.vertices,
        gm1.stats.vertices
    );
    assert!(mrel < 0.01, "gpu mesher surfel volume rel {mrel}");
    assert_eq!(
        mg.stats.triangles, gm1.stats.triangles,
        "gpu mesher triangles"
    );
    assert_eq!(mg.stats.chi, gm1.stats.chi, "gpu mesher chi");
    assert_eq!(
        mg.stats.components, gm1.stats.components,
        "gpu mesher components"
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

    let dm1 = field_to_mesh_gpu(&hd, &d1).expect("gpu mesh density 1");
    let dm2 = field_to_mesh_gpu(&hd, &d1).expect("gpu mesh density 2");
    assert_eq!(
        dm1.stats.hash, dm2.stats.hash,
        "gpu mesher density determinism"
    );
    let mrel = ((md.stats.volume - dm1.stats.volume) / md.stats.volume).abs();
    println!(
        "gpu mesher density: cpu_vol={:.5} gpu_vol={:.5} rel={mrel:.6} tri={}/{} chi={}/{} verts={}/{}",
        md.stats.volume,
        dm1.stats.volume,
        md.stats.triangles,
        dm1.stats.triangles,
        md.stats.chi,
        dm1.stats.chi,
        md.stats.vertices,
        dm1.stats.vertices
    );
    assert!(mrel < 0.01, "gpu mesher density volume rel {mrel}");
    assert_eq!(
        md.stats.triangles, dm1.stats.triangles,
        "gpu mesher triangles"
    );
    assert_eq!(md.stats.chi, dm1.stats.chi, "gpu mesher chi");
    assert_eq!(
        md.stats.components, dm1.stats.components,
        "gpu mesher components"
    );

    let mut hollow_opts = cpu_opts.clone();
    hollow_opts.seal = false;
    let cpu_hollow = voxelize(&gs, &hollow_opts).expect("cpu density hollow");
    let gpu_hollow = voxelize_gpu_with(
        &hd,
        &gs,
        &VoxelizeOptions {
            backend: Backend::Gpu,
            ..hollow_opts
        },
    )
    .expect("gpu density hollow");
    let mh = field_to_mesh(&cpu_hollow);
    let gh = field_to_mesh(&gpu_hollow);
    let rel = ((mh.stats.volume - gh.stats.volume) / mh.stats.volume).abs();
    println!(
        "gpu density hollow: cpu_sealed={} gpu_sealed={} cpu_vol={:.5} gpu_vol={:.5} rel={rel:.5} chi={}/{}",
        cpu_hollow.sealed_interior,
        gpu_hollow.sealed_interior,
        mh.stats.volume,
        gh.stats.volume,
        mh.stats.chi,
        gh.stats.chi
    );
    assert_eq!(cpu_hollow.sealed_interior, 0);
    assert_eq!(gpu_hollow.sealed_interior, 0);
    assert!(rel < 0.05, "density hollow volume rel {rel}");
    assert_eq!(mh.stats.chi, gh.stats.chi, "density hollow chi");
}

#[test]
#[ignore = "diagnostic GPU timings (64/128, density/surfel)"]
fn bench_gpu_scenes() {
    let _g = GPU_LOCK.lock().unwrap();
    let hd = try_headless_device().expect("headless gpu");
    let gs = sphere_gaussians(20_000, 0.75, 0.99);
    for n in [64u32, 128] {
        for mode in [VoxelMode::Density, VoxelMode::Surfel] {
            let cpu_opts = opts(mode, n);
            let t = Instant::now();
            let cpu = voxelize(&gs, &cpu_opts).expect("cpu");
            let cpu_ms = t.elapsed().as_secs_f64() * 1e3;
            let gpu_opts = VoxelizeOptions {
                backend: Backend::Gpu,
                ..cpu_opts
            };
            let _ = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu warmup");
            let t = Instant::now();
            let gpu = voxelize_gpu_with(&hd, &gs, &gpu_opts).expect("gpu");
            let gpu_ms = t.elapsed().as_secs_f64() * 1e3;
            let hollow_opts = VoxelizeOptions {
                seal: false,
                ..gpu_opts
            };
            let t = Instant::now();
            let gpu_hollow = voxelize_gpu_with(&hd, &gs, &hollow_opts).expect("gpu hollow");
            let hollow_ms = t.elapsed().as_secs_f64() * 1e3;
            let mc = field_to_mesh(&cpu);
            let t = Instant::now();
            let mg = field_to_mesh(&gpu);
            let mesh_cpu_ms = t.elapsed().as_secs_f64() * 1e3;
            let rel = ((mc.stats.volume - mg.stats.volume) / mc.stats.volume).abs();
            println!(
                "n={n} {:<7} vox cpu={cpu_ms:7.1}ms gpu={gpu_ms:7.1}ms (hollow {hollow_ms:.1}) iso={:.4}/{:.4} vol_rel={rel:.5} chi={}/{}",
                mode.name(),
                cpu.iso,
                gpu.iso,
                mc.stats.chi,
                mg.stats.chi
            );
            println!("  {:<7} mesh cpu={mesh_cpu_ms:6.1}ms", mode.name());
            let t = Instant::now();
            match field_to_mesh_gpu(&hd, &gpu) {
                Ok(mgg) => {
                    let mesh_gpu_ms = t.elapsed().as_secs_f64() * 1e3;
                    let mesh_rel = ((mg.stats.volume - mgg.stats.volume) / mg.stats.volume).abs();
                    println!(
                        "  {:<7} mesh gpu={mesh_gpu_ms:6.1}ms rel={mesh_rel:.6} tri={}/{} chi={}/{}",
                        mode.name(),
                        mg.stats.triangles,
                        mgg.stats.triangles,
                        mg.stats.chi,
                        mgg.stats.chi
                    );
                    assert_eq!(
                        mg.stats.triangles, mgg.stats.triangles,
                        "mesh tris at n={n}"
                    );
                    assert_eq!(mg.stats.chi, mgg.stats.chi, "mesh chi at n={n}");
                }
                Err(e) => println!("  {:<7} mesh gpu=skipped: {e}", mode.name()),
            }
            // hollow 是开放壳，χ 与实心体不可比；只验"没有内部填充"且仍出网格。
            let mgh = field_to_mesh(&gpu_hollow);
            assert_eq!(gpu_hollow.sealed_interior, 0);
            assert!(mgh.stats.triangles > 0, "hollow triangles at n={n}");
        }
    }
}
