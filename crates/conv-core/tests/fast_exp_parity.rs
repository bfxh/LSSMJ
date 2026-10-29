//! `--fast-exp`（opt-in 快速 exp）：与默认 `f32::exp` 相比，**几何量必须一致**。
//!
//! 默认路径仍走 libm exp（逐位基线）；本测试锁的是 opt-in 档的语义：
//! 体积（`field_to_mesh` 的 stats.volume）、χ、分量、三角数全部与默认档相等，
//! 只允许浮点位模式不同（哈希会变）。

use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::voxel::{VoxelizeOptions, voxelize};
use conv_core::gauss::{blob_sphere_gaussians, sphere_gaussians, torus_gaussians};

fn opts(n: u32, fast: bool) -> VoxelizeOptions {
    VoxelizeOptions {
        n,
        threads: 8,
        seal: true,
        fast_exp: fast,
        ..VoxelizeOptions::default()
    }
}

fn check(name: &str, gs: &[conv_core::gauss::Gaussian], n: u32) {
    let a = voxelize(gs, &opts(n, false)).expect("默认档");
    let b = voxelize(gs, &opts(n, true)).expect("fast-exp 档");
    let ma = field_to_mesh(&a);
    let mb = field_to_mesh(&b);

    assert_eq!(
        ma.stats.triangles, mb.stats.triangles,
        "{name}: 三角数必须相同（快速 exp 只允许浮点位模式变化）"
    );
    assert_eq!(ma.stats.chi, mb.stats.chi, "{name}: χ 必须相同");
    assert_eq!(
        ma.stats.components, mb.stats.components,
        "{name}: 连通分量必须相同"
    );
    let rel = ((ma.stats.volume - mb.stats.volume) / ma.stats.volume).abs();
    assert!(rel < 1e-4, "{name}: 体积相对差 {rel:.3e} 超限");
}

#[test]
fn fast_exp_keeps_geometry_on_sphere() {
    check("sphere", &sphere_gaussians(2_000, 1.0, 0.9), 96);
}

#[test]
fn fast_exp_keeps_geometry_on_torus() {
    check("torus", &torus_gaussians(32, 16, 0.8, 0.25, 0.9), 96);
}

#[test]
fn fast_exp_keeps_geometry_on_blobs() {
    check(
        "blobs",
        &blob_sphere_gaussians(3_000, 0.75, 0.05, 0.95, 7),
        96,
    );
}
