//! T-GC-05 第一片判据：分块 GSN 与整块 GSN 的**无缝**对拍——
//! ① 全缓冲逐位（位置/标记）：分块不得有任何位差（接缝处的强形式）
//! ② 规范四边形集合：全局一致 + **逐块**一致（归属过滤精确划分四边形集）
//! ③ 空块跳过：active < chunks 且跳过不改变任何输出

use conv_core::{
    GRID, field_to_voxels,
    gsn::{surface_nets_gpu, surface_nets_gpu_chunked},
    jfa::headless_device,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;
const C: u32 = 16; // 块 = 16 cell（64³ ⇒ 4 轴块，末块 15）

fn cell_of(slot: u32, n: u32) -> [u32; 3] {
    [slot % n, (slot / n) % n, slot / (n * n)]
}

fn canonical(t1: [u32; 3], t2: [u32; 3]) -> ([u32; 3], [u32; 3]) {
    let mut a = t1;
    let mut b = t2;
    a.sort();
    b.sort();
    if a > b { (b, a) } else { (a, b) }
}

/// 全部四边形规范集合。
fn canonical_quads(indices: &[u32]) -> Vec<([u32; 3], [u32; 3])> {
    let mut quads: Vec<([u32; 3], [u32; 3])> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| canonical([q[0], q[1], q[2]], [q[3], q[4], q[5]]))
        .collect();
    quads.sort();
    quads
}

/// 块 [a, b) 归属的四边形规范集合。归属判定：四顶点 cell 全在 [块起−1, 块止)³ 内
/// **且至少一个在 [块起, 块止)³ 内**——等价于"发射 cell（p1）∈ 块"（quad 四顶点逐轴 ≤ p1
/// 且 ≥ p1−1），互不重不漏（缺"钉住"条件会把跨块 quad 重复计入相邻两块）。
fn quads_of_block(indices: &[u32], n: u32, a: [u32; 3], b: [u32; 3]) -> Vec<([u32; 3], [u32; 3])> {
    let mut quads: Vec<([u32; 3], [u32; 3])> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| {
            let mut cells: Vec<[u32; 3]> = Vec::with_capacity(4);
            for &v in q.iter() {
                let c = cell_of(v, n);
                if !cells.contains(&c) {
                    cells.push(c);
                }
            }
            let relaxed = cells
                .iter()
                .all(|c| (0..3).all(|k| c[k] + 1 >= a[k] && c[k] < b[k]));
            let pinned = cells
                .iter()
                .any(|c| (0..3).all(|k| c[k] >= a[k] && c[k] < b[k]));
            relaxed && pinned
        })
        .map(|q| canonical([q[0], q[1], q[2]], [q[3], q[4], q[5]]))
        .collect();
    quads.sort();
    quads
}

fn assert_positions_bitwise_equal(a: &[[f32; 3]], b: &[[f32; 3]]) {
    assert_eq!(a.len(), b.len(), "位置缓冲长度不一致");
    let diff = a
        .iter()
        .zip(b)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "位置缓冲逐位不一致槽位数={diff}");
}

#[test]
fn chunked_gsn_matches_whole_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let whole = surface_nets_gpu(&hd, &sdf, GRID, None);
    let (chunked, stats) = surface_nets_gpu_chunked(&hd, &sdf, GRID, C, None);
    println!(
        "chunk stats: {}/{} 活跃（C={C}）",
        stats.active, stats.chunks
    );

    // ① 全缓冲逐位：位置 + 标记（含所有边界/接缝 cell）
    assert_positions_bitwise_equal(&whole.positions, &chunked.positions);
    assert_eq!(whole.flags, chunked.flags, "标记缓冲不一致");

    // ② 规范四边形集合：全局一致
    assert_eq!(whole.quad_count, chunked.quad_count, "四边形数不一致");
    let qw = canonical_quads(&whole.indices);
    let qc = canonical_quads(&chunked.indices);
    assert_eq!(qw, qc, "规范四边形集合不一致");

    // ③ 逐块（接缝）：归属集合分别一致，且并集 = 全量（精确划分）
    let cell_axis = GRID - 1;
    let per_axis = cell_axis.div_ceil(C);
    let mut total = 0usize;
    for cz in 0..per_axis {
        for cy in 0..per_axis {
            for cx in 0..per_axis {
                let a = [cx * C, cy * C, cz * C];
                let b = [
                    (a[0] + C).min(cell_axis),
                    (a[1] + C).min(cell_axis),
                    (a[2] + C).min(cell_axis),
                ];
                let qw_blk = quads_of_block(&whole.indices, GRID, a, b);
                let qc_blk = quads_of_block(&chunked.indices, GRID, a, b);
                assert_eq!(qw_blk, qc_blk, "块 {a:?}..{b:?} 归属四边形集不一致");
                total += qc_blk.len();
            }
        }
    }
    assert_eq!(
        total, whole.quad_count as usize,
        "逐块归属未精确划分四边形集"
    );
}

#[test]
fn chunked_gsn_odd_chunk_size() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let sdf = field_to_voxels(GRID, R);
    let hd = headless_device();
    let whole = surface_nets_gpu(&hd, &sdf, GRID, None);
    // 非整除块（末块短）：63 = 20 + 20 + 20 + 3
    let (chunked, stats) = surface_nets_gpu_chunked(&hd, &sdf, GRID, 20, None);
    println!(
        "chunk stats: {}/{} 活跃（C=20）",
        stats.active, stats.chunks
    );
    assert_positions_bitwise_equal(&whole.positions, &chunked.positions);
    assert_eq!(whole.flags, chunked.flags);
    assert_eq!(whole.quad_count, chunked.quad_count);
    assert_eq!(
        canonical_quads(&whole.indices),
        canonical_quads(&chunked.indices)
    );
}

#[test]
fn chunked_gsn_skips_empty_blocks() {
    let _gpu = GPU_LOCK.lock().unwrap();
    // 小球（R=0.3）：球面只穿中段块，角落块全正 ⇒ 必须被跳过
    let sdf = field_to_voxels(GRID, 0.3);
    let hd = headless_device();
    let whole = surface_nets_gpu(&hd, &sdf, GRID, None);
    let (chunked, stats) = surface_nets_gpu_chunked(&hd, &sdf, GRID, C, None);
    println!(
        "skip check: {}/{} 活跃（R=0.3, C={C}）",
        stats.active, stats.chunks
    );
    assert!(
        stats.active >= 1 && stats.active < stats.chunks,
        "应有空块被跳过：active={}/{}",
        stats.active,
        stats.chunks
    );
    // 跳过不许改变任何输出
    assert_positions_bitwise_equal(&whole.positions, &chunked.positions);
    assert_eq!(whole.quad_count, chunked.quad_count);
    assert_eq!(
        canonical_quads(&whole.indices),
        canonical_quads(&chunked.indices)
    );
}
