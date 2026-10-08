//! 稀疏体素格写侧判据（自研缺口②第一片）：写读逐位 + 稀疏性可观察 + 全填稠密化逐位等价 +
//! 部分填语义 + halo 读侧跨块窗。纯 CPU。

use conv_core::{
    field_to_voxels,
    sparse::{EMPTY_VALUE, SparseGrid},
};

#[test]
fn write_read_roundtrip_and_sparsity() {
    let mut g = SparseGrid::new(8);
    let pts = [
        [0, 0, 0],
        [7, 7, 7],
        [8, 0, 0],
        [-1, -1, -1],
        [15, 15, 15],
        [16, 2, 3],
    ];
    for (k, p) in pts.iter().enumerate() {
        g.set(*p, k as f32 + 0.25);
    }
    for (k, p) in pts.iter().enumerate() {
        assert_eq!(g.get(*p), k as f32 + 0.25, "写读不回环：{p:?}");
    }
    assert_eq!(g.get([100, 100, 100]), EMPTY_VALUE, "未写块应读 EMPTY");
    // 触块： (0,0,0)×2 / (1,0,0) / (-1,-1,-1) / (1,1,1) / (2,0,0)
    assert_eq!(g.allocated_blocks(), 5, "块数应等于唯一触块数");
}

#[test]
fn full_fill_materialize_matches_dense() {
    let n = 32u32;
    let dense = field_to_voxels(n, 0.6);
    let mut g = SparseGrid::new(8);
    let nn = n as i32;
    for z in 0..nn {
        for y in 0..nn {
            for x in 0..nn {
                g.set([x, y, z], dense[(x + y * nn + z * nn * nn) as usize]);
            }
        }
    }
    assert_eq!(g.allocated_blocks(), 4 * 4 * 4, "全填触块数");
    let m = g.materialize(n);
    let diff = m
        .iter()
        .zip(&dense)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    assert_eq!(diff, 0, "全填稠密化逐位不一致（{diff} 项）");
}

#[test]
fn partial_fill_semantics() {
    let n = 32u32;
    let dense = field_to_voxels(n, 0.6);
    let mut g = SparseGrid::new(8);
    let nn = n as i32;
    // 只写块 (1,1,1)（格点 [8,16)³）
    for z in 8..16 {
        for y in 8..16 {
            for x in 8..16 {
                g.set([x, y, z], dense[(x + y * nn + z * nn * nn) as usize]);
            }
        }
    }
    assert_eq!(g.allocated_blocks(), 1);
    // 块内逐位；块外 EMPTY
    assert_eq!(g.get([8, 8, 8]), dense[(8 + 8 * nn + 8 * nn * nn) as usize]);
    assert_eq!(g.get([7, 8, 8]), EMPTY_VALUE, "邻块未写 ⇒ EMPTY");
    let m = g.materialize(n);
    for z in 8..16 {
        for y in 8..16 {
            for x in 8..16 {
                let i = (x + y * nn + z * nn * nn) as usize;
                assert_eq!(m[i].to_bits(), dense[i].to_bits(), "部分填块内逐位");
            }
        }
    }
    assert_eq!(m[0], EMPTY_VALUE, "未写区 = EMPTY");
}

#[test]
fn corner_window_halo_cross_block() {
    // 块 (1,1,1) 的本地 (0,0,0) = 全局 (4,4,4)；块 (0,0,0) 的角值窗 [0,4]³ 应跨块读到它
    let mut g = SparseGrid::new(4);
    g.set([4, 4, 4], 1.0);
    let w = g.extract_corner_window([0, 0, 0]);
    assert_eq!(w.len(), 5 * 5 * 5, "角值窗应为 (C+1)³");
    assert_eq!(w[4 + 4 * 5 + 4 * 25], 1.0, "halo 角应跨块读到邻块值");
    let ones = w.iter().filter(|&&v| v == 1.0).count();
    assert_eq!(ones, 1, "其余角值应为 EMPTY");
}

#[test]
fn pack_blocks_and_neighbor_table() {
    let mut g = SparseGrid::new(4);
    // 触三块：(0,0,0) / (1,0,0) / (0,1,0)（+y 的块由全局点 (0,4,0) 触发）
    g.set([0, 0, 0], 1.0);
    g.set([4, 0, 0], 2.0);
    g.set([0, 4, 0], 3.0);
    let pk = g.pack();
    assert_eq!(pk.c, 4);
    // 块表：z→y→x 字典序
    assert_eq!(pk.blocks, vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]]);
    let n3 = 4usize * 4 * 4;
    assert_eq!(pk.packed.len(), 3 * n3);
    assert_eq!(pk.nbr.len(), 3 * 27);

    let idx = |b: [i32; 3]| pk.blocks.iter().position(|&x| x == b).unwrap();
    let (ka, kb, kc) = (idx([0, 0, 0]), idx([1, 0, 0]), idx([0, 1, 0]));
    let slot = |d: [i32; 3]| ((d[2] + 1) * 9 + (d[1] + 1) * 3 + (d[0] + 1)) as usize;
    // 邻块表：本块自指 / +x / +y / 缺失
    assert_eq!(pk.nbr[ka * 27 + slot([0, 0, 0])], ka as i32);
    assert_eq!(pk.nbr[ka * 27 + slot([1, 0, 0])], kb as i32);
    assert_eq!(pk.nbr[ka * 27 + slot([0, 1, 0])], kc as i32);
    assert_eq!(pk.nbr[ka * 27 + slot([0, 0, 1])], -1, "缺失邻块应为 −1");
    // packed 内容 = 块内所有权层（局部 (0,0,0) 抽查）
    assert_eq!(pk.packed[ka * n3], 1.0);
    assert_eq!(pk.packed[kb * n3], 2.0);
    assert_eq!(pk.packed[kc * n3], 3.0);
}
