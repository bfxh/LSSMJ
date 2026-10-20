//! 高斯腿判据（C22）：球/环面解析对拍、亏格、逐位确定性、剪枝、PLY 读入。

use conv_core::gauss::mesh::field_to_mesh;
use conv_core::gauss::ply::parse_ply;
use conv_core::gauss::voxel::{VoxelMode, VoxelizeOptions, voxelize};
use conv_core::gauss::{
    Gaussian, PruneOptions, add_floaters, blob_sphere_gaussians, jacobi_eigen, median_flatness,
    prune, sphere_gaussians, torus_gaussians,
};

const R: f32 = 0.75;

fn volume_ref(r: f32) -> f32 {
    4.0 / 3.0 * std::f32::consts::PI * r * r * r
}

fn opts(mode: VoxelMode, n: u32) -> VoxelizeOptions {
    VoxelizeOptions {
        mode,
        n,
        ..VoxelizeOptions::default()
    }
}

#[test]
fn surfel_sphere_volume_genus_determinism() {
    let gs = sphere_gaussians(20_000, R, 0.99);
    let field = voxelize(&gs, &opts(VoxelMode::Surfel, 64)).expect("voxelize");
    let mesh = field_to_mesh(&field);
    let rel = ((mesh.stats.volume - volume_ref(R)) / volume_ref(R)).abs();
    println!(
        "surfel sphere: v={} t={} vol={:.5} rel={:.5} chi={} comp={} touch={} obs={} iso={:.5} hash={:016x}",
        mesh.stats.vertices,
        mesh.stats.triangles,
        mesh.stats.volume,
        rel,
        mesh.stats.chi,
        mesh.stats.components,
        mesh.stats.boundary_touch,
        field.observed,
        field.iso,
        mesh.stats.hash
    );
    assert!(rel <= 0.03, "volume rel err {rel}");
    assert_eq!(mesh.stats.chi, 2, "genus 0 sphere");
    assert_eq!(mesh.stats.components, 1);
    assert!(!mesh.stats.boundary_touch);

    let field2 = voxelize(&gs, &opts(VoxelMode::Surfel, 64)).expect("voxelize 2");
    let mesh2 = field_to_mesh(&field2);
    assert_eq!(mesh.stats.hash, mesh2.stats.hash, "bitwise determinism");
}

#[test]
fn surfel_torus_genus_one() {
    let gs = torus_gaussians(180, 110, 0.55, 0.22, 0.99);
    let field = voxelize(&gs, &opts(VoxelMode::Surfel, 72)).expect("voxelize");
    let mesh = field_to_mesh(&field);
    let vol_ref = 2.0 * std::f32::consts::PI.powi(2) * 0.55 * 0.22 * 0.22;
    let rel = ((mesh.stats.volume - vol_ref) / vol_ref).abs();
    println!(
        "surfel torus: v={} t={} vol={:.5} rel={:.5} chi={} comp={} obs={}",
        mesh.stats.vertices,
        mesh.stats.triangles,
        mesh.stats.volume,
        rel,
        mesh.stats.chi,
        mesh.stats.components,
        field.observed
    );
    assert!(rel <= 0.05, "torus volume rel err {rel}");
    assert_eq!(mesh.stats.chi, 0, "genus 1 torus");
    assert_eq!(mesh.stats.components, 1);
}

#[test]
fn density_blob_is_closed_and_bounded() {
    let gs = blob_sphere_gaussians(12_000, R, 0.05, 0.95, 7);
    let field = voxelize(&gs, &opts(VoxelMode::Density, 64)).expect("voxelize");
    let mesh = field_to_mesh(&field);
    let rel = ((mesh.stats.volume - volume_ref(R)) / volume_ref(R)).abs();
    println!(
        "density blob: v={} t={} vol={:.5} rel={:.5} chi={} comp={} iso={:.5}",
        mesh.stats.vertices,
        mesh.stats.triangles,
        mesh.stats.volume,
        rel,
        mesh.stats.chi,
        mesh.stats.components,
        field.iso
    );
    assert!(mesh.stats.triangles > 0);
    assert!(
        rel <= 0.25,
        "density volume rel err {rel}（厚壳会外扩，预算 25%）"
    );
    assert_eq!(mesh.stats.components, 1, "floaters 不存在时应为单连通");
}

#[test]
fn pruning_removes_floaters_exactly() {
    let clean = sphere_gaussians(8_000, R, 0.9);
    let mut dirty = clean.clone();
    add_floaters(&mut dirty, 300, 42);
    let (kept, stats) = prune(&dirty, &PruneOptions::default());
    println!(
        "prune: in={} kept={} op={} rad={} inv={}",
        stats.input, stats.kept, stats.dropped_opacity, stats.dropped_radius, stats.dropped_invalid
    );
    assert_eq!(kept.len(), clean.len());
    assert_eq!(stats.dropped_opacity, 300);

    let a = field_to_mesh(&voxelize(&clean, &opts(VoxelMode::Surfel, 64)).unwrap());
    let b = field_to_mesh(&voxelize(&kept, &opts(VoxelMode::Surfel, 64)).unwrap());
    assert_eq!(a.stats.hash, b.stats.hash, "剪枝后与干净输入逐位一致");
}

#[test]
fn ply_binary_activations_roundtrip() {
    let gs = [
        Gaussian {
            mean: [0.1, -0.2, 0.3],
            scale: [0.05, 0.06, 0.007],
            rot: [1.0, 0.0, 0.0, 0.0],
            opacity: 0.8,
            color: [0.2, 0.4, 0.6],
            normal: Some([0.0, 0.0, 1.0]),
        },
        Gaussian {
            mean: [-0.4, 0.5, -0.6],
            scale: [0.02, 0.03, 0.004],
            rot: [1.0, 0.0, 0.0, 0.0],
            opacity: 0.3,
            color: [0.9, 0.1, 0.1],
            normal: None,
        },
    ];
    let names = [
        "x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0",
        "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
    ];
    let mut header = String::from("ply\nformat binary_little_endian 1.0\n");
    header.push_str(&format!("element vertex {}\n", gs.len()));
    for n in names {
        header.push_str(&format!("property float {n}\n"));
    }
    header.push_str("element face 0\nproperty list uchar int vertex_indices\nend_header\n");
    let mut bytes = header.into_bytes();
    for g in &gs {
        let mut row = Vec::with_capacity(17 * 4);
        let push = |row: &mut Vec<u8>, v: f32| row.extend_from_slice(&v.to_le_bytes());
        push(&mut row, g.mean[0]);
        push(&mut row, g.mean[1]);
        push(&mut row, g.mean[2]);
        let n = g.normal.unwrap_or([0.0, 0.0, 0.0]);
        push(&mut row, n[0]);
        push(&mut row, n[1]);
        push(&mut row, n[2]);
        for c in g.color {
            let dc = (c - 0.5) / 0.282_094_8;
            push(&mut row, dc);
        }
        let logit = (g.opacity / (1.0 - g.opacity)).ln();
        push(&mut row, logit);
        for s in g.scale {
            push(&mut row, s.ln());
        }
        for q in g.rot {
            push(&mut row, q);
        }
        bytes.extend_from_slice(&row);
    }
    let data = parse_ply(&bytes).expect("parse ply");
    assert_eq!(data.gaussians.len(), 2);
    assert!(data.has_normals);
    assert_eq!(data.format, "binary_little_endian");
    for (a, b) in data.gaussians.iter().zip(gs.iter()) {
        for k in 0..3 {
            assert!((a.mean[k] - b.mean[k]).abs() < 1e-6);
            assert!((a.scale[k] - b.scale[k]).abs() < 1e-6);
        }
        assert!((a.opacity - b.opacity).abs() < 1e-6);
        assert!((a.color[0] - b.color[0]).abs() < 1e-5);
    }
    assert!(data.gaussians[1].normal.is_none());
}

#[test]
fn parallel_splat_matches_single_thread_bitwise() {
    // C22：线程数不是数值来源；z-slab 分块只改写入位置，不改累加顺序。
    let gs = sphere_gaussians(4_000, R, 0.99);
    let mut single = opts(VoxelMode::Surfel, 48);
    single.threads = 1;
    let f1 = voxelize(&gs, &single).expect("single surfel");
    let mut multi = single.clone();
    multi.threads = 8;
    let f2 = voxelize(&gs, &multi).expect("multi surfel");
    assert!(f2.workers > 1, "expected parallel workers");
    assert_eq!(f1.values.len(), f2.values.len());
    for (a, b) in f1.values.iter().zip(&f2.values) {
        assert_eq!(a.to_bits(), b.to_bits(), "surfel field bitwise");
    }
    assert_eq!(f1.observed, f2.observed);
    assert_eq!(f1.sealed_interior, f2.sealed_interior);
    let m1 = field_to_mesh(&f1);
    let m2 = field_to_mesh(&f2);
    assert_eq!(m1.stats.hash, m2.stats.hash, "surfel mesh hash");

    let gs = blob_sphere_gaussians(3_000, R, 0.05, 0.95, 7);
    let mut single = opts(VoxelMode::Density, 48);
    single.threads = 1;
    let d1 = voxelize(&gs, &single).expect("single density");
    let mut multi = single.clone();
    multi.threads = 8;
    let d2 = voxelize(&gs, &multi).expect("multi density");
    for (a, b) in d1.values.iter().zip(&d2.values) {
        assert_eq!(a.to_bits(), b.to_bits(), "density field bitwise");
    }
    let dm1 = field_to_mesh(&d1);
    let dm2 = field_to_mesh(&d2);
    println!(
        "parallel: surfel workers {} hash {:016x}; density workers {} hash {:016x}",
        f2.workers, m2.stats.hash, d2.workers, dm2.stats.hash
    );
    assert_eq!(dm1.stats.hash, dm2.stats.hash, "density mesh hash");
}

#[test]
fn bins3d_matches_zslab_bitwise() {
    let gs = sphere_gaussians(4_000, R, 0.99);
    let mut base = opts(VoxelMode::Surfel, 48);
    base.threads = 8;
    let a = voxelize(&gs, &base).expect("z-slab surfel");
    let mut b = base.clone();
    b.bins3d = true;
    let c = voxelize(&gs, &b).expect("3d tile surfel");
    assert!(!a.bins3d && c.bins3d);
    for (x, y) in a.values.iter().zip(&c.values) {
        assert_eq!(x.to_bits(), y.to_bits(), "surfel 3d tile bitwise");
    }
    assert_eq!(
        field_to_mesh(&a).stats.hash,
        field_to_mesh(&c).stats.hash,
        "surfel 3d tile mesh hash"
    );

    let gs = blob_sphere_gaussians(3_000, R, 0.05, 0.95, 7);
    let mut base = opts(VoxelMode::Density, 48);
    base.threads = 8;
    let a = voxelize(&gs, &base).expect("z-slab density");
    let mut b = base.clone();
    b.bins3d = true;
    let c = voxelize(&gs, &b).expect("3d tile density");
    for (x, y) in a.values.iter().zip(&c.values) {
        assert_eq!(x.to_bits(), y.to_bits(), "density 3d tile bitwise");
    }
    assert_eq!(
        field_to_mesh(&a).stats.hash,
        field_to_mesh(&c).stats.hash,
        "density 3d tile mesh hash"
    );
}

#[test]
fn eigen_and_flatness() {
    let e = jacobi_eigen([[3.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]]);
    assert!((e.values[0] - 1.0).abs() < 1e-5);
    assert!((e.values[1] - 2.0).abs() < 1e-5);
    assert!((e.values[2] - 3.0).abs() < 1e-5);
    assert!(median_flatness(&sphere_gaussians(100, R, 0.9)) > 2.0);
    let iso = blob_sphere_gaussians(100, R, 0.05, 0.9, 1);
    assert!(median_flatness(&iso) < 1.01);
}
