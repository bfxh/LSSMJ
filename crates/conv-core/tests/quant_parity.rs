//! u8 定点密度：并行与单线程必须逐字节相同。

use conv_core::gauss::voxel::{VoxelizeOptions, voxelize_quant};
use conv_core::gauss::{sphere_gaussians, torus_gaussians};

fn opts(threads: usize) -> VoxelizeOptions {
    VoxelizeOptions {
        n: 64,
        threads,
        seal: false,
        ..VoxelizeOptions::default()
    }
}

#[test]
fn parallel_matches_single_thread_bitwise() {
    let mut g = sphere_gaussians(600, 1.0, 0.9);
    g.extend(torus_gaussians(32, 16, 0.8, 0.25, 0.9));
    let a = voxelize_quant::<u8>(&g, &opts(1), 8.0).expect("1T");
    let b = voxelize_quant::<u8>(&g, &opts(8), 8.0).expect("8T");
    assert_eq!(a.values, b.values, "u8 定点场必须逐字节相同");

    // u16 也要逐位一致
    let c = voxelize_quant::<u16>(&g, &opts(1), 2000.0).expect("u16 1T");
    let d = voxelize_quant::<u16>(&g, &opts(8), 2000.0).expect("u16 8T");
    assert_eq!(c.values, d.values, "u16 定点场必须逐位相同");
}
