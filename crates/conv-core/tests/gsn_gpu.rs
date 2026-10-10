//! GPU Surface Nets 判据：与 CPU fast-surface-nets 的四边形集合对拍（cell 空间）
//! + 体积/欧拉特征 + 确定性（规范键排序后逐位）。

use std::collections::HashSet;

use conv_core::{GRID, field_to_voxels, gsn::surface_nets_gpu, jfa::headless_device, mesh_volume};
use fast_surface_nets::ndshape::ConstShape3u32;
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};
use std::sync::Mutex;

mod common;
use common::{assert_index_bounds, directed_quads, signed_volume};

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

/// 四边形 → cell 空间**有向**规范形（两个三角各自循环旋转最小化，三角对字典序）。
/// F04：规范化只消除循环移位与三角对序，保留绕序——翻面三角给出不同键（旧排序版会并键）。
/// GPU 侧槽位=cell 线性索引（已是 cell 空间）；CPU 侧经 surface_points 映射。
fn canonical_quads(
    indices: &[u32],
    compact_to_cell: Option<&[(u32, u32, u32)]>,
) -> Vec<([u32; 3], [u32; 3])> {
    match compact_to_cell {
        Some(map) => directed_quads(indices, |v| {
            let c = map[v as usize];
            c.0 + c.1 * GRID + c.2 * GRID * GRID
        }),
        None => directed_quads(indices, |v| v),
    }
}

#[test]
fn gsn_matches_cpu_per_cell() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let gpu = surface_nets_gpu(&hd, &sdf, GRID, None);

    let mut cpu = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &Shape {}, [0; 3], [GRID - 1; 3], &mut cpu);

    // J1 四边形数一致
    assert_eq!(
        gpu.quad_count as usize,
        cpu.indices.len() / 6,
        "quad count mismatch"
    );

    // J2 逐 cell 位置对拍（cpu.surface_points 给出 cell 坐标 → gpu 槽位）
    // F05：先断言有限，再聚合误差——f32::max 遇 NaN 会静默吞点，误差门不能依赖偶然覆盖
    let mut max_err = 0f32;
    for (i, sp) in cpu.surface_points.iter().enumerate() {
        let slot = (sp[0] + sp[1] * GRID + sp[2] * GRID * GRID) as usize;
        assert!(slot < gpu.flags.len(), "cell {sp:?} 槽位越界 slot={slot}");
        assert_eq!(gpu.flags[slot], 1, "cell {sp:?} 缺顶点");
        let gp = gpu.positions[slot];
        let cp = cpu.positions[i];
        assert!(
            gp[0].is_finite() && gp[1].is_finite() && gp[2].is_finite(),
            "cell {sp:?} GPU 位置非有限 {gp:?}"
        );
        assert!(
            cp[0].is_finite() && cp[1].is_finite() && cp[2].is_finite(),
            "cell {sp:?} CPU 参照位置非有限 {cp:?}"
        );
        let e =
            ((gp[0] - cp[0]).powi(2) + (gp[1] - cp[1]).powi(2) + (gp[2] - cp[2]).powi(2)).sqrt();
        assert!(e.is_finite(), "cell {sp:?} 位置误差非有限");
        max_err = max_err.max(e);
    }
    assert!(!cpu.surface_points.is_empty(), "CPU 表面点为空——反空跑");
    println!("GSN per-cell max_err={max_err:.6}");
    assert!(max_err <= 1e-4, "per-cell position max_err={max_err}");

    // J3 四边形集合一致（cell 空间**有向**规范形：成员 + 绕序；残缺索引/越界在 common 内硬红）
    let cell_map: Vec<(u32, u32, u32)> = cpu
        .surface_points
        .iter()
        .map(|sp| (sp[0], sp[1], sp[2]))
        .collect();
    assert_index_bounds(&gpu.indices, gpu.positions.len(), "GSN gpu 索引");
    assert_index_bounds(&cpu.indices, cpu.positions.len(), "GSN cpu 索引");
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

    // J4b 绕序方向（F04）：有符号体积与 CPU 参照同号——排序规范化证明不了朝向，体积方向单独验
    let sv_gpu = signed_volume(&gpu.positions, &gpu.indices);
    let sv_cpu = signed_volume(&cpu.positions, &cpu.indices);
    println!("GSN signed vol_gpu={sv_gpu:.5} vol_cpu={sv_cpu:.5}");
    assert!(
        sv_gpu * sv_cpu > 0.0 && sv_cpu.abs() > 1e-6,
        "GSN 绕序方向与 CPU 参照不一致：signed vol_gpu={sv_gpu} vol_cpu={sv_cpu}"
    );

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
    let a = surface_nets_gpu(&hd, &sdf, GRID, None);
    let b = surface_nets_gpu(&hd, &sdf, GRID, None);
    // 顶点逐位确定（全缓冲哈希）；索引执行序非确定 ⇒ 规范键排序后逐位对拍
    assert_eq!(fnv_mesh(&a.positions, &[]), fnv_mesh(&b.positions, &[]));
    let qa = canonical_quads(&a.indices, None);
    let qb = canonical_quads(&b.indices, None);
    assert_eq!(qa, qb, "canonical quad stream varies");
}
