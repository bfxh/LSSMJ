//! 稀疏块尺寸扫描（ignored）：256³ 下 B=8/16/32 的候选块、处理角点数与耗时。
//! 判据同时校验每个 B 的 tri/vol/χ 与稠密 hollow 一致，避免“快但错”。

use conv_core::gauss::Gaussian;
use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::voxel::{
    VoxelMode, VoxelizeOptions, sparse_density::SparseDensity, sparse_surfel::SparseSurfel,
    surface_blocks::SurfaceBlocks, voxelize,
};
use conv_core::gauss::{add_floaters, blob_sphere_gaussians, sphere_gaussians};
use std::time::Instant;

fn truth_blocks(field: &conv_core::gauss::voxel::Field, block: u32) -> usize {
    let n = field.grid.n;
    let per = n.div_ceil(block);
    let mut out = 0usize;
    for bz in 0..per {
        for by in 0..per {
            for bx in 0..per {
                let x1 = ((bx + 1) * block).min(n);
                let y1 = ((by + 1) * block).min(n);
                let z1 = ((bz + 1) * block).min(n);
                let mut neg = false;
                let mut pos = false;
                'scan: for z in bz * block..z1 {
                    for y in by * block..y1 {
                        let row = (y + z * n) as usize * n as usize;
                        for x in bx * block..x1 {
                            let v = field.values[row + x as usize];
                            if v < 0.0 {
                                neg = true;
                            } else if v > 0.0 {
                                pos = true;
                            }
                            if neg && pos {
                                out += 1;
                                break 'scan;
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn sweep(gs: &[Gaussian], opts: &VoxelizeOptions, label: &str) {
    let t = Instant::now();
    let dense = voxelize(gs, opts).expect("dense");
    let dm = field_to_mesh(&dense);
    let dense_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "{label} dense={dense_ms:.1}ms tri={} vol={:.5} chi={}",
        dm.stats.triangles, dm.stats.volume, dm.stats.chi
    );
    for block in [8u32, 16, 32] {
        let t = Instant::now();
        let blocks = SurfaceBlocks::detect(gs, opts, dense.grid, block, 0, 4).expect("blocks");
        let detect_ms = t.elapsed().as_secs_f64() * 1e3;
        let truth = truth_blocks(&dense, block);
        println!(
            "{label} B={block}: truth={truth} active={} overshoot={:.1}%",
            blocks.active_count(),
            (blocks.active_count() as f64 / truth as f64 - 1.0) * 100.0
        );
        let t = Instant::now();
        let mesh = match opts.mode {
            VoxelMode::Density => {
                let sparse = SparseDensity::build(gs, opts, dense.grid, &blocks, dense.iso, false)
                    .expect("sparse density");
                let build_ms = t.elapsed().as_secs_f64() * 1e3;
                let t = Instant::now();
                let mesh = sparse.mesh().expect("mesh");
                let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
                println!(
                    "{label} B={block}: active={} ({:.1}%) corners={:.2}M detect={detect_ms:.1} build={build_ms:.1} mesh={mesh_ms:.1} total={:.1}",
                    blocks.active_count(),
                    blocks.fraction() * 100.0,
                    blocks.active_count() as f64 * (block as f64 + 2.0).powi(3) / 1e6,
                    detect_ms + build_ms + mesh_ms
                );
                mesh
            }
            VoxelMode::Surfel => {
                let sparse = SparseSurfel::build(gs, opts, dense.grid, &blocks, false)
                    .expect("sparse surfel");
                let build_ms = t.elapsed().as_secs_f64() * 1e3;
                let t = Instant::now();
                let mesh = sparse.mesh().expect("mesh");
                let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
                println!(
                    "{label} B={block}: active={} ({:.1}%) corners={:.2}M detect={detect_ms:.1} build={build_ms:.1} mesh={mesh_ms:.1} total={:.1}",
                    blocks.active_count(),
                    blocks.fraction() * 100.0,
                    blocks.active_count() as f64 * (block as f64 + 2.0).powi(3) / 1e6,
                    detect_ms + build_ms + mesh_ms
                );
                mesh
            }
        };
        let s = &mesh.stats;
        let rel = ((dm.stats.volume - s.volume) / dm.stats.volume).abs();
        println!(
            "{label} B={block}: tri={} vol={:.5} chi={} comp={} rel={rel:.6}",
            s.triangles, s.volume, s.chi, s.components
        );
        assert_eq!(dm.stats.triangles, s.triangles, "{label} B={block} tri");
        assert!(rel < 1e-4, "{label} B={block} vol rel {rel}");
        assert_eq!(dm.stats.chi, s.chi, "{label} B={block} chi");
        assert_eq!(dm.stats.components, s.components, "{label} B={block} comp");
    }
}

#[test]
#[ignore = "diagnostic sparse block size sweep 256"]
fn sparse_block_size_sweep_256() {
    let mut blobs = blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
    add_floaters(&mut blobs, 300, 7 ^ 0x9e37_79b9);
    let density_opts = VoxelizeOptions {
        mode: VoxelMode::Density,
        n: 256,
        threads: 24,
        seal: false,
        ..VoxelizeOptions::default()
    };
    sweep(&blobs, &density_opts, "density");

    let sphere = sphere_gaussians(60_000, 0.75, 0.99);
    let surfel_opts = VoxelizeOptions {
        mode: VoxelMode::Surfel,
        n: 256,
        threads: 24,
        seal: false,
        ..VoxelizeOptions::default()
    };
    sweep(&sphere, &surfel_opts, "surfel");
}
