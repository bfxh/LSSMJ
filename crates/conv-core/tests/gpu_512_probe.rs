//! GPU 512³ 探针（ignored）：adapter limits 抬高后，dense GPU 场 + CPU 网格能否跑通。
//! `cargo test --release --test gpu_512_probe -- --ignored --nocapture --test-threads=1`

use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::sphere_gaussians;
use conv_core::gauss::voxel::{Backend, VoxelMode, VoxelizeOptions, voxelize_gpu_with};
use conv_core::jfa::try_headless_device;
use std::sync::Mutex;
use std::time::Instant;

static GPU_LOCK: Mutex<()> = Mutex::new(());

#[test]
#[ignore = "diagnostic gpu 512 dense"]
fn gpu_512_dense_probe() {
    let _g = GPU_LOCK.lock().unwrap();
    let hd = match try_headless_device() {
        Ok(hd) => hd,
        Err(e) => {
            println!("no gpu: {e}");
            return;
        }
    };
    let gs = sphere_gaussians(60_000, 0.75, 0.99);
    let seal = true;
    let opts = VoxelizeOptions {
        mode: VoxelMode::Surfel,
        n: 512,
        threads: 24,
        seal,
        backend: Backend::Gpu,
        ..VoxelizeOptions::default()
    };
    let t = Instant::now();
    match voxelize_gpu_with(&hd, &gs, &opts) {
        Ok(f) => {
            let vox = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let m = field_to_mesh(&f);
            let mesh = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "gpu 512 surfel seal={seal}: vox={vox:.1} mesh={mesh:.1} ms tri={} vol={:.5} chi={} comp={} obs={} sealed={}",
                m.stats.triangles,
                m.stats.volume,
                m.stats.chi,
                m.stats.components,
                f.observed,
                f.sealed_interior
            );
            assert_eq!(m.stats.chi, 2, "surfel 512 chi");
            assert_eq!(m.stats.components, 1, "surfel 512 comp");
            assert!((m.stats.volume - 1.77).abs() < 0.01, "surfel 512 vol");
        }
        Err(e) => println!("gpu 512 surfel seal={seal} failed: {e}"),
    }

    // 与 CLI 剪枝后的 demo blobs 同场景（不含 floaters；取景更紧 ⇒ job 更多）。
    let blobs = conv_core::gauss::blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
    let opts = VoxelizeOptions {
        mode: VoxelMode::Density,
        n: 512,
        threads: 24,
        seal: true,
        backend: Backend::Gpu,
        ..VoxelizeOptions::default()
    };
    match voxelize_gpu_with(&hd, &blobs, &opts) {
        Ok(_) => panic!("density 512 GPU 不应在 CLI 场景下装进 u32 job 表"),
        Err(e) => {
            println!("gpu 512 density expected job overflow: {e}");
            assert!(e.contains("job 总数溢出"), "unexpected error: {e}");
        }
    }
}
