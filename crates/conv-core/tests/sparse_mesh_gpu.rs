//! T-GC-05 第六片判据：稀疏块单块 GPU 网格化（**跨块 halo 读取**；锚 `W15A-034/035`）——
//! ① 拼接零缝：逐块产物按本块所有权装配回全局 n³ 槽 ⇒ 与整块 GSN 位置/标记**逐位**一致，
//!    规范四边形集合一致；
//! ② 逐块归属：每块四边形集 = 密网格按"发射 cell ∈ 块"的归属集（互不重不漏、精确划分）；
//! ③ halo 实证：球面跨块处真实引用 −1 halo 层（局部 0 平面）顶点（三轴计数 > 0）；
//! ④ 微观夹具（对角面）：块边界 cell 恰产 8 四边形、halo 引用计数逐项可预测（[8,4,8]）；
//! ⑤ 负块坐标（i32 原点路径）：平移对（块 (−1,−1,−1) ↔ 块 (0,0,0)）结构逐槽位一致、
//!    位置在档（跨坐标平移不保证逐位：f32 加法量级不同 ⇒ ±1 ulp 注记）；
//! ⑥ 未分配块 ⇒ 空网格。

use conv_core::{
    field_to_voxels,
    gsn::{BlockMesh, mesh_block, surface_nets_gpu},
    jfa::headless_device,
    sparse::SparseGrid,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const C: i32 = 8; // 块 = 8 cell
const B: i32 = 4; // 每轴块数
const N: u32 = (C * B + 1) as u32; // 33（格点 [0, 33)）

fn cell_i32(lin: u32, n: u32) -> [i32; 3] {
    [
        (lin % n) as i32,
        ((lin / n) % n) as i32,
        (lin / (n * n)) as i32,
    ]
}

/// 规范四边形键：两枚三角（各自 3 个 cell 顶点排序后）。
type QuadKey = ([[i32; 3]; 3], [[i32; 3]; 3]);

fn canon_tri(t: [[i32; 3]; 3]) -> [[i32; 3]; 3] {
    let mut a = t;
    a.sort();
    a
}

fn canon_pair(t1: [[i32; 3]; 3], t2: [[i32; 3]; 3]) -> QuadKey {
    let (a, b) = (canon_tri(t1), canon_tri(t2));
    if a > b { (b, a) } else { (a, b) }
}

fn quad_key_i32(q: &[u32], n: u32) -> QuadKey {
    canon_pair(
        [cell_i32(q[0], n), cell_i32(q[1], n), cell_i32(q[2], n)],
        [cell_i32(q[3], n), cell_i32(q[4], n), cell_i32(q[5], n)],
    )
}

/// 全部四边形规范集合（顶点槽 → cell 坐标）。
fn canon_quads_i32(indices: &[u32], n: u32) -> Vec<QuadKey> {
    let mut v: Vec<_> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| quad_key_i32(q, n))
        .collect();
    v.sort();
    v
}

/// 块 [a, b) 归属的四边形规范集合（归属判定 = 四顶点 cell 全在 [块起−1, 块止)³
/// 且至少一个在 [块起, 块止)³——等价于"发射 cell ∈ 块"）。与 gsn_chunk_gpu 同判据。
fn quads_of_block_i32(indices: &[u32], n: u32, a: [i32; 3], b: [i32; 3]) -> Vec<QuadKey> {
    let mut v: Vec<_> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| {
            let mut cells: Vec<[i32; 3]> = Vec::with_capacity(4);
            for &s in q.iter() {
                let c = cell_i32(s, n);
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
        .map(|q| quad_key_i32(q, n))
        .collect();
    v.sort();
    v
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

/// 局部槽位（lc 布局 (C+1)³）→ 局部 cell 坐标。
fn decode_slot(slot: u32, s: u32) -> [i32; 3] {
    [
        (slot % s) as i32,
        ((slot / s) % s) as i32,
        (slot / (s * s)) as i32,
    ]
}

#[test]
fn sparse_block_meshes_stitch_to_whole() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let sdf = field_to_voxels(N, 0.5);
    let mut sp = SparseGrid::new(C);
    let nn = N as i32;
    for z in 0..nn {
        for y in 0..nn {
            for x in 0..nn {
                sp.set([x, y, z], sdf[(x + y * nn + z * nn * nn) as usize]);
            }
        }
    }
    assert_eq!(sp.allocated_blocks(), 125, "格点 [0,33)³ 应触 5³ 块");

    let dense = surface_nets_gpu(&hd, &sdf, N, None);

    let n3 = (N * N * N) as usize;
    let mut apos = vec![[0f32; 3]; n3];
    let mut aflags = vec![0u32; n3];
    let mut aidx: Vec<u32> = Vec::new();
    let mut halo_refs = [0usize; 3];
    let mut total_block_quads = 0usize;

    for bz in 0..B {
        for by in 0..B {
            for bx in 0..B {
                let blk = [bx, by, bz];
                let bm = mesh_block(&hd, &sp, blk, None);
                let s = bm.c + 1;
                assert_eq!(bm.positions.len(), (s * s * s) as usize);
                assert_eq!(bm.flags.len(), (s * s * s) as usize);

                // ① 顶点：仅本块所有权层（lc ∈ [1, C+1)）装配到全局 n³ 槽
                for lz in 1..s {
                    for ly in 1..s {
                        for lx in 1..s {
                            let g = [
                                blk[0] * C + lx as i32 - 1,
                                blk[1] * C + ly as i32 - 1,
                                blk[2] * C + lz as i32 - 1,
                            ];
                            let lin = (g[0] + g[1] * nn + g[2] * nn * nn) as usize;
                            let slot = lx + ly * s + lz * s * s;
                            apos[lin] = bm.positions[slot as usize];
                            aflags[lin] = bm.flags[slot as usize];
                        }
                    }
                }

                // ② 四边形：映射到全局槽 + halo 引用计数 + 逐块归属对拍
                let mut blk_lins: Vec<u32> = Vec::with_capacity(bm.indices.len());
                for q in bm.indices.as_chunks::<6>().0 {
                    for &v in q {
                        let lc = decode_slot(v, s);
                        for i in 0..3 {
                            if lc[i] == 0 {
                                halo_refs[i] += 1;
                            }
                        }
                        let g = [
                            blk[0] * C + lc[0] - 1,
                            blk[1] * C + lc[1] - 1,
                            blk[2] * C + lc[2] - 1,
                        ];
                        let lin = (g[0] + g[1] * nn + g[2] * nn * nn) as u32;
                        blk_lins.push(lin);
                        aidx.push(lin);
                    }
                }
                let blk_keys = canon_quads_i32(&blk_lins, N);
                let a = [bx * C, by * C, bz * C];
                let b_end = [a[0] + C, a[1] + C, a[2] + C];
                let dense_keys = quads_of_block_i32(&dense.indices, N, a, b_end);
                assert_eq!(
                    blk_keys,
                    dense_keys,
                    "块 {blk:?} 归属四边形集不一致（{}/{})",
                    blk_keys.len(),
                    dense_keys.len()
                );
                total_block_quads += blk_keys.len();
            }
        }
    }

    // 拼接零缝：位置/标记逐位 + 规范四边形集合一致
    assert_positions_bitwise_equal(&dense.positions, &apos);
    assert_eq!(dense.flags, aflags, "标记缓冲不一致");
    assert_eq!(dense.quad_count as usize, aidx.len() / 6);
    assert_eq!(
        total_block_quads, dense.quad_count as usize,
        "逐块归属未精确划分"
    );
    assert_eq!(
        canon_quads_i32(&dense.indices, N),
        canon_quads_i32(&aidx, N),
        "装配后规范四边形集合与整块不一致"
    );
    assert!(
        halo_refs.iter().all(|&h| h > 0),
        "halo 层顶点未被引用（三轴计数 {halo_refs:?}）"
    );
    println!(
        "sparse stitch: {} 四边形 / {} 活跃槽；halo 引用（x/y/z）={:?}",
        dense.quad_count,
        aflags.iter().filter(|&&f| f == 1).count(),
        halo_refs
    );
}

#[test]
fn diagonal_plane_halo_refs_deterministic() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    // 对角面 x + z − 0.5：穿面 cell = (0, y, 0)（x+z = 0），其 x/z 双边引用 −1 halo 层
    let mut g = SparseGrid::new(4);
    for z in -5..9i32 {
        for y in -5..9i32 {
            for x in -5..9i32 {
                g.set([x, y, z], (x + z) as f32 - 0.5);
            }
        }
    }
    let bm = mesh_block(&hd, &g, [0, 0, 0], None);
    let s = bm.c + 1;
    assert_eq!(
        bm.quad_count, 8,
        "cell (0,y,0) × 4 应各产 x/z 双边 = 8 个四边形"
    );

    let mut halo = [0usize; 3];
    for q in bm.indices.as_chunks::<6>().0 {
        // 每 quad 的 6 索引含重复顶点（两三角共享边）——按**去重后**的 4 cell 计引用
        let mut seen: Vec<[i32; 3]> = Vec::with_capacity(4);
        for &v in q {
            assert_eq!(bm.flags[v as usize], 1, "被引用顶点须为表面 cell");
            let lc = decode_slot(v, s);
            if !seen.contains(&lc) {
                seen.push(lc);
            }
        }
        for lc in seen {
            for i in 0..3 {
                if lc[i] == 0 {
                    halo[i] += 1;
                }
            }
        }
    }
    // 去重 cell 口径：x-quad ×4 各引 2 枚 z−1（lc 0 平面）+ z-quad ×4 各引 2 枚 x−1
    // + y=0 行（x/z 双边各 2 枚）的 y−1 → [8, 4, 8]
    assert_eq!(halo, [8, 4, 8], "halo 层引用计数（x/y/z）");

    // 位置在档：顶点落在其 cell 内（[0,1]³ 凸组合）
    for (slot, &f) in bm.flags.iter().enumerate() {
        if f != 1 {
            continue;
        }
        let lc = decode_slot(slot as u32, s);
        let cell = [lc[0] - 1, lc[1] - 1, lc[2] - 1]; // 块 0：全局 = lc − 1
        let p = bm.positions[slot];
        for k in 0..3 {
            let d = p[k] - cell[k] as f32;
            assert!(
                (-1e-6..=1.0 + 1e-6).contains(&d),
                "顶点越出 cell：slot={slot} p={p:?} cell={cell:?}"
            );
        }
    }
    println!(
        "diagonal plane: {} 四边形，halo 引用 {:?}",
        bm.quad_count, halo
    );
}

#[test]
fn negative_block_coords_shift_pair() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let h = 2.0f32 / 16.0; // n=17 口径
    let v = |q: [i32; 3]| {
        conv_core::sphere_sdf([q[0] as f32 * h, q[1] as f32 * h, q[2] as f32 * h], 0.5)
    };

    // A：球心在格点原点，网格含负块；网格化块 (−1,−1,−1)（窗口格点 [−5, 1)³）
    let mut a = SparseGrid::new(4);
    for z in -9..9i32 {
        for y in -9..9i32 {
            for x in -9..9i32 {
                a.set([x, y, z], v([x, y, z]));
            }
        }
    }
    // B：同一几何平移 +4（vB(q) = vA(q − 4)）；网格化块 (0,0,0)（窗口格点 [−1, 5)³）
    let mut b = SparseGrid::new(4);
    for z in -5..9i32 {
        for y in -5..9i32 {
            for x in -5..9i32 {
                b.set([x, y, z], v([x - 4, y - 4, z - 4]));
            }
        }
    }

    let ma = mesh_block(&hd, &a, [-1, -1, -1], None);
    let mb = mesh_block(&hd, &b, [0, 0, 0], None);
    assert!(ma.quad_count > 0, "负块应有几何");
    assert_eq!(ma.quad_count, mb.quad_count, "平移对四边形数应一致");
    assert_eq!(ma.flags, mb.flags, "平移对标记应逐槽位一致");

    // 四边形集合：A 的 cell +4 == B 的 cell（整数平移，精确）
    let s = ma.c + 1;
    let c = ma.c as i32;
    let keys_of = |bm: &BlockMesh, shift: i32| -> Vec<QuadKey> {
        let cell_of_v = |v: u32| -> [i32; 3] {
            let lc = decode_slot(v, s);
            [
                bm.block[0] * c - 1 + lc[0] + shift,
                bm.block[1] * c - 1 + lc[1] + shift,
                bm.block[2] * c - 1 + lc[2] + shift,
            ]
        };
        let mut out: Vec<QuadKey> = bm
            .indices
            .as_chunks::<6>()
            .0
            .iter()
            .map(|q| {
                canon_pair(
                    [cell_of_v(q[0]), cell_of_v(q[1]), cell_of_v(q[2])],
                    [cell_of_v(q[3]), cell_of_v(q[4]), cell_of_v(q[5])],
                )
            })
            .collect();
        out.sort();
        out
    };
    assert_eq!(
        keys_of(&ma, 4),
        keys_of(&mb, 0),
        "平移对四边形规范集合不一致（A 的 cell +4 应等于 B）"
    );

    // 位置：跨坐标平移不保证逐位（f32 加法量级不同 ⇒ ±1 ulp），档位 1e-5
    let mut maxd = 0f32;
    for (i, &f) in ma.flags.iter().enumerate() {
        if f != 1 {
            continue;
        }
        for k in 0..3 {
            maxd = maxd.max((ma.positions[i][k] + 4.0 - mb.positions[i][k]).abs());
        }
    }
    assert!(maxd <= 1e-5, "平移对位置偏差超档：{maxd}");
    println!(
        "negative block: {} 四边形，平移最大偏差 {maxd:.2e}",
        ma.quad_count
    );
}

#[test]
fn unallocated_block_meshes_empty() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let g = SparseGrid::new(4);
    let bm = mesh_block(&hd, &g, [7, 7, 7], None);
    assert_eq!(bm.quad_count, 0, "未分配块不应有四边形");
    assert!(bm.indices.is_empty());
    assert!(bm.flags.iter().all(|&f| f == 0), "未分配块标记应全零");
    assert!(
        bm.positions.iter().all(|p| p == &[0.0, 0.0, 0.0]),
        "未分配块位置应全零"
    );
}
