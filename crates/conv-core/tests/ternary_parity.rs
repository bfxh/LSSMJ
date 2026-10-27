//! 三值体素场判据：并行 = 串行（逐位），且是 density 场的超集。

use conv_core::gauss::voxel::{VoxelizeOptions, voxelize_ternary};
use conv_core::gauss::{Gaussian, sphere_gaussians};

fn opts(n: u32, threads: usize) -> VoxelizeOptions {
    // seal=false：不把未知区填成内部，否则 `values > iso` 会把「未观测的外部」也算进来，
    // 那不是 density 覆盖集。
    VoxelizeOptions {
        n,
        threads,
        seal: false,
        ..VoxelizeOptions::default()
    }
}

fn gs() -> Vec<Gaussian> {
    let mut v = sphere_gaussians(600, 1.0, 0.9);
    v.extend(conv_core::gauss::torus_gaussians(32, 16, 0.8, 0.25, 0.9));
    v
}

#[test]
fn parallel_matches_single_thread_bitwise() {
    let g = gs();
    let a = voxelize_ternary(&g, &opts(64, 1)).expect("单线程");
    let b = voxelize_ternary(&g, &opts(64, 8)).expect("多线程");
    assert_eq!(a.words.len(), b.words.len());
    assert_eq!(a.words, b.words, "并行与串行的 trit 缓冲必须逐位相同");
    assert_eq!(a.occupied, b.occupied);
}

#[test]
fn single_gaussian_marks_its_own_voxel() {
    // 局部性：一个高斯必须至少把它自己所在体素标成 trit>=1。
    let g = vec![Gaussian {
        mean: [0.0, 0.0, 0.0],
        scale: [0.2, 0.2, 0.2],
        rot: [1.0, 0.0, 0.0, 0.0],
        opacity: 0.9,
        color: [0.5, 0.5, 0.5],
        normal: None,
    }];
    let t = voxelize_ternary(&g, &opts(64, 4)).expect("ternary");
    let n = t.grid.n;
    let c = n / 2;
    assert!(t.trit_at(c, c, c) >= 1, "中心体素必须被覆盖");
    assert!(
        t.observed >= 1 && t.occupied == 0,
        "单个高斯不可能达到 trit=2"
    );
}
