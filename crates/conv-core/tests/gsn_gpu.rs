//! GPU Surface Nets 判据：与 CPU fast-surface-nets 的四边形集合对拍（cell 空间）
//! + 体积/欧拉特征 + 确定性（规范键排序后逐位）。

use std::collections::HashSet;

use conv_core::{GRID, field_to_voxels, gsn::surface_nets_gpu, jfa::headless_device, mesh_volume};
use fast_surface_nets::ndshape::ConstShape3u32;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;
type Shape = ConstShape3u32<64, 64, 64>;

fn fnv_mesh(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in positions {
        for c in p {
            for b in c.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    for i in indices {
        for b in i.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

/// 四边形 → cell 空间规范形（两个三角的 cell 三元组，各自升序，quad 内按字典序）。
/// GPU 侧槽位=cell 线性索引（已是 cell 空间）；CPU 侧经 surface_points 映射。
fn canonical_quads(
    indices: &[u32],
    compact_to_cell: Option<&[(u32, u32, u32)]>,
) -> Vec<([u32; 3], [u32; 3])> {
    let mut quads: Vec<([u32; 3], [u32; 3])> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| {
            let m = |v: u32| -> u32 {
                match compact_to_cell {
                    Some(map) => {
                        let c = map[v as usize];
                        c.0 + c.1 * GRID + c.2 * GRID * GRID
                    }
                    None => v,
                }
            };
            let mut t1 = [m(q[0]), m(q[1]), m(q[2])];
            let mut t2 = [m(q[3]), m(q[4]), m(q[5])];
            t1.sort();
            t2.sort();
            if t1 > t2 { (t2, t1) } else { (t1, t2) }
        })
        .collect();
    quads.sort();
    quads
}

#[test]
fn gsn_matches_cpu_per_cell() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let gpu = surface_nets_gpu(&hd, &sdf, GRID);

    let mut cpu = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &Shape {}, [0; 3], [GRID - 1; 3], &mut cpu);

    // J1 四边形数一致
    assert_eq!(
        gpu.quad_count as usize,
        cpu.indices.len() / 6,
        "quad count mismatch"
    );

    // J2 逐 cell 位置对拍（cpu.surface_points 给出 cell 坐标 → gpu 槽位）
    let mut max_err = 0f32;
    for (i, sp) in cpu.surface_points.iter().enumerate() {
        let slot = (sp[0] + sp[1] * GRID + sp[2] * GRID * GRID) as usize;
        assert_eq!(gpu.flags[slot], 1, "cell {sp:?} 缺顶点");
        let gp = gpu.positions[slot];
        let cp = cpu.positions[i];
        let e =
            ((gp[0] - cp[0]).powi(2) + (gp[1] - cp[1]).powi(2) + (gp[2] - cp[2]).powi(2)).sqrt();
        max_err = max_err.max(e);
    }
    println!("GSN per-cell max_err={max_err:.6}");
    assert!(max_err <= 1e-4, "per-cell position max_err={max_err}");

    // J3 四边形集合一致（cell 空间规范形：成员 + 绕序）
    let cell_map: Vec<(u32, u32, u32)> = cpu
        .surface_points
        .iter()
        .map(|sp| (sp[0], sp[1], sp[2]))
        .collect();
    let q_cpu = canonical_quads(&cpu.indices, Some(&cell_map));
    let q_gpu = canonical_quads(&gpu.indices, None);
    assert_eq!(q_cpu.len(), q_gpu.len());
    for (k, (a, b)) in q_cpu.iter().zip(q_gpu.iter()).enumerate() {
        assert_eq!(a, b, "quad #{k} mismatch: cpu={a:?} gpu={b:?}");
    }

    // J4 体积与欧拉特征
    let vol_gpu = mesh_volume(&gpu.positions, &gpu.indices);
    let vol_cpu = mesh_volume(&cpu.positions, &cpu.indices);
    let rel = ((vol_gpu - vol_cpu) / vol_cpu).abs();
    println!("GSN vol_gpu={vol_gpu:.5} vol_cpu={vol_cpu:.5} rel={rel:.5}");
    assert!(rel <= 0.03, "volume rel={rel}");

    let v_count = gpu.flags.iter().filter(|&&f| f == 1).count();
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for t in gpu.indices.as_chunks::<3>().0 {
        for e in 0..3 {
            let a = t[e];
            let b = t[(e + 1) % 3];
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let chi = v_count as i64 - edges.len() as i64 + (gpu.indices.len() / 3) as i64;
    println!("GSN chi={chi} (V={v_count} E={})", edges.len());
    assert_eq!(chi, 2, "genus-0 sphere expected");
}

#[test]
fn gsn_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let a = surface_nets_gpu(&hd, &sdf, GRID);
    let b = surface_nets_gpu(&hd, &sdf, GRID);
    // 顶点逐位确定（全缓冲哈希）；索引执行序非确定 ⇒ 规范键排序后逐位对拍
    assert_eq!(fnv_mesh(&a.positions, &[]), fnv_mesh(&b.positions, &[]));
    let qa = canonical_quads(&a.indices, None);
    let qb = canonical_quads(&b.indices, None);
    assert_eq!(qa, qb, "canonical quad stream varies");
}
