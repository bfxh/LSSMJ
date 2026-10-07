//! 稠密 compaction 判据（T-GC-05 第三片）：稠密流 vs 稀疏 n³ 槽位——
//! 顶点序 = cell 序且逐位、索引重映射后规范四边形集合一致、计数一致、确定性、空输入零路径。

use conv_core::{
    GRID, field_to_voxels,
    gsn::{compact_mesh, surface_nets_gpu, surface_nets_gpu_chunked, surface_nets_gpu_compact},
    jfa::headless_device,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;

fn cell_of(slot: u32, n: u32) -> [u32; 3] {
    [slot % n, (slot / n) % n, slot / (n * n)]
}

fn cell_lin(c: [u32; 3]) -> u32 {
    c[0] + c[1] * GRID + c[2] * GRID * GRID
}

/// 稠密顶点号 → cell 线性号（第 i 个稠密顶点 = 第 i 个 flagged cell）。
fn dense_of(flags: &[u32]) -> Vec<u32> {
    (0..flags.len() as u32)
        .filter(|&i| flags[i as usize] == 1)
        .map(|c| cell_lin(cell_of(c, GRID)))
        .collect()
}

/// 把三角形两端顶点经 `m` 映射成 cell 线性号后规范化（两个三角各自升序，quad 内字典序）。
fn canonical_quads_map<F: Fn(u32) -> u32>(indices: &[u32], m: F) -> Vec<([u32; 3], [u32; 3])> {
    let mut quads: Vec<([u32; 3], [u32; 3])> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| {
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
fn compact_mesh_dense_matches_sparse_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let whole = surface_nets_gpu(&hd, &sdf, GRID, None);
    let compact = compact_mesh(&hd, &whole, None);

    // ① 计数：顶点数 = flags 计数；索引数不变
    let flagged: Vec<u32> = (0..whole.flags.len() as u32)
        .filter(|&i| whole.flags[i as usize] == 1)
        .collect();
    assert_eq!(
        compact.vertex_count as usize,
        flagged.len(),
        "顶点数 ≠ flags 计数"
    );
    assert_eq!(compact.positions.len(), flagged.len());
    assert_eq!(compact.indices.len(), whole.indices.len(), "索引数变化");

    // ② 稠密顶点序 = cell 序，且逐位等于对应稀疏槽位
    for (i, &cell) in flagged.iter().enumerate() {
        let sp = whole.positions[cell as usize];
        let dp = compact.positions[i];
        assert!(
            sp[0].to_bits() == dp[0].to_bits()
                && sp[1].to_bits() == dp[1].to_bits()
                && sp[2].to_bits() == dp[2].to_bits(),
            "第 {i} 个稠密顶点 ≠ cell {cell} 的稀疏顶点"
        );
    }

    // ③ 索引重映射：稠密索引 → cell（第 i 个稠密顶点 = 第 i 个 flagged cell）后与稀疏规范一致
    let dense_cells = dense_of(&whole.flags);
    assert!(
        compact
            .indices
            .iter()
            .all(|&v| (v as usize) < compact.positions.len()),
        "重映射索引越界"
    );
    let q_sparse = canonical_quads_map(&whole.indices, |v| v);
    let q_dense = canonical_quads_map(&compact.indices, |v| dense_cells[v as usize]);
    assert_eq!(q_sparse, q_dense, "重映射后规范四边形集合不一致");
    println!(
        "compact: {} 顶点 / {} 四边形（稀疏 {} 槽位）",
        compact.vertex_count,
        compact.indices.len() / 6,
        whole.flags.len()
    );
}

#[test]
fn compact_mesh_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let whole = surface_nets_gpu(&hd, &sdf, GRID, None);
    let a = compact_mesh(&hd, &whole, None);
    let b = compact_mesh(&hd, &whole, None);
    assert_eq!(a.vertex_count, b.vertex_count);
    let diff = a
        .positions
        .iter()
        .zip(&b.positions)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "稠密顶点非逐位确定（差异 {diff} 项）");
    assert_eq!(a.indices, b.indices, "稠密索引非确定");
}

#[test]
fn compact_mesh_empty_input() {
    let _gpu = GPU_LOCK.lock().unwrap();
    // 全正场：无顶点、无四边形（scan/散射的零路径）
    let sdf = vec![1.0f32; (GRID * GRID * GRID) as usize];
    let hd = headless_device();
    let mesh = surface_nets_gpu(&hd, &sdf, GRID, None);
    assert_eq!(mesh.quad_count, 0);
    let compact = compact_mesh(&hd, &mesh, None);
    assert_eq!(compact.vertex_count, 0);
    assert!(compact.positions.is_empty());
    assert!(compact.indices.is_empty());
}

#[test]
fn direct_compact_matches_two_step_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    // 直出（runner 免稀疏回写）vs 两步法（稀疏回读 + compact_mesh）
    let (direct, stats) = surface_nets_gpu_compact(&hd, &sdf, GRID, 16, None);
    let (sparse, _) = surface_nets_gpu_chunked(&hd, &sdf, GRID, 16, None);
    let two_step = compact_mesh(&hd, &sparse, None);
    println!(
        "direct: {} 顶点 / {} 四边形（活跃块 {}/{}）",
        direct.vertex_count,
        direct.indices.len() / 6,
        stats.active,
        stats.chunks
    );

    // ① 计数
    assert_eq!(direct.vertex_count, two_step.vertex_count, "顶点数不一致");
    assert_eq!(direct.positions.len(), two_step.positions.len());
    assert_eq!(direct.indices.len(), two_step.indices.len());

    // ② 稠密顶点逐位（两侧均为 cell 序）
    let diff = direct
        .positions
        .iter()
        .zip(&two_step.positions)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "稠密顶点非逐位一致（{diff} 项）");

    // ③ 规范四边形（同一 dense→cell 映射）
    let dense_cells = dense_of(&sparse.flags);
    let q_direct = canonical_quads_map(&direct.indices, |v| dense_cells[v as usize]);
    let q_two = canonical_quads_map(&two_step.indices, |v| dense_cells[v as usize]);
    assert_eq!(q_direct, q_two, "规范四边形集合不一致");
}

#[test]
fn direct_compact_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let (a, _) = surface_nets_gpu_compact(&hd, &sdf, GRID, 16, None);
    let (b, _) = surface_nets_gpu_compact(&hd, &sdf, GRID, 16, None);
    assert_eq!(a.vertex_count, b.vertex_count);
    let diff = a
        .positions
        .iter()
        .zip(&b.positions)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "直出稠密顶点非确定（差异 {diff} 项）");
    // 索引：原子序非确定 ⇒ 规范比较（映射由 flags 重建）
    let flags = surface_nets_gpu(&hd, &sdf, GRID, None).flags;
    let dense_cells = dense_of(&flags);
    assert_eq!(
        canonical_quads_map(&a.indices, |v| dense_cells[v as usize]),
        canonical_quads_map(&b.indices, |v| dense_cells[v as usize]),
        "直出索引规范集合非确定"
    );
}
