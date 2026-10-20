//! 稀疏分块可行性探针（ignored）：256³ 下按 16/32³ 块统计“有符号变化”的表面块比例。
//! 注意：这里统计的是 post-seal 后的 Field；未知区被写成 ±outside，所以“非零块”恒为 100%，
//! 不能当原始 splat 分配率。表面块（min<0<max）才是稀疏带需要保留的部分。

use conv_core::gauss::voxel::{Field, VoxelMode, VoxelizeOptions, voxelize};
use conv_core::gauss::{add_floaters, blob_sphere_gaussians, torus_gaussians};

fn block_stats(field: &Field, bs: u32) {
    let n = field.grid.n;
    let per = n.div_ceil(bs);
    let mut surface = 0usize;
    for bz in 0..per {
        for by in 0..per {
            for bx in 0..per {
                let x0 = bx * bs;
                let y0 = by * bs;
                let z0 = bz * bs;
                let x1 = (x0 + bs).min(n);
                let y1 = (y0 + bs).min(n);
                let z1 = (z0 + bs).min(n);
                let mut neg = false;
                let mut pos = false;
                for z in z0..z1 {
                    for y in y0..y1 {
                        let base = (y * n + z * n * n) as usize;
                        for x in x0..x1 {
                            let v = field.values[base + x as usize];
                            if v < 0.0 {
                                neg = true;
                            } else if v > 0.0 {
                                pos = true;
                            }
                        }
                    }
                }
                if neg && pos {
                    surface += 1;
                }
            }
        }
    }
    let total = (per * per * per) as usize;
    println!(
        "n={n} bs={bs}: blocks={total} surface-blocks={surface} ({:.1}%)",
        surface as f64 / total as f64 * 100.0
    );
}

#[test]
#[ignore = "diagnostic sparse block occupancy"]
fn sparse_block_occupancy_256() {
    let opts = VoxelizeOptions {
        n: 256,
        threads: 24,
        ..VoxelizeOptions::default()
    };
    let torus = torus_gaussians(180, 110, 0.55, 0.22, 0.99);
    let mut o = opts.clone();
    o.mode = VoxelMode::Surfel;
    let f = voxelize(&torus, &o).expect("torus");
    for bs in [16, 32] {
        block_stats(&f, bs);
    }
    drop(f);

    let mut blobs = blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, 7);
    add_floaters(&mut blobs, 300, 7 ^ 0x9e37_79b9);
    let mut o = opts;
    o.mode = VoxelMode::Density;
    let f = voxelize(&blobs, &o).expect("blobs");
    for bs in [16, 32] {
        block_stats(&f, bs);
    }
}
