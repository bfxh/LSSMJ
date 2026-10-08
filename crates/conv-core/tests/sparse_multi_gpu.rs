//! 装配器判据：多块稀疏网格化（块表驱动、一次提交全部已分配块）——
//! ① 与单块原语**逐块逐位一致**：33³ 球面场 125 块（含空块）全量对拍槽位/标记 + 规范四边形集合；
//! ② 确定性：两跑计数一致、抽块槽位与四边形集合一致；
//! ③ 规模：512³ 包围盒壳层（C=16，~1 万块）网格化 + 抽块对单块原语一致（判据不变、只换规模）；
//! ④ **超单维派发上限**（装配器第二片）：C=8 壳层 blocks×wg_axis > 65535，块号 grid-stride 生效。

use conv_core::{
    gsn::{MultiMesh, mesh_block, mesh_blocks},
    jfa::{Headless, headless_device},
    sparse::SparseGrid,
    sphere_sdf,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

type QuadKey = ([u32; 3], [u32; 3]);

fn quad_key(q: &[u32]) -> QuadKey {
    let mut t1 = [q[0], q[1], q[2]];
    t1.sort();
    let mut t2 = [q[3], q[4], q[5]];
    t2.sort();
    if t1 > t2 { (t2, t1) } else { (t1, t2) }
}

fn canon_quads(indices: &[u32]) -> Vec<QuadKey> {
    let mut v: Vec<_> = indices
        .as_chunks::<6>()
        .0
        .iter()
        .map(|q| quad_key(q))
        .collect();
    v.sort();
    v
}

fn assert_slots_bitwise(a: &[[f32; 3]], b: &[[f32; 3]], tag: &str) {
    assert_eq!(a.len(), b.len(), "{tag}: 槽位数不一致");
    let diff = a
        .iter()
        .zip(b)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "{tag}: 位置槽逐位不一致槽位数={diff}");
}

fn build_sphere_grid(c: i32, n: u32, r: f32) -> SparseGrid {
    let mut g = SparseGrid::new(c);
    let nn = n as i32;
    let h = 2.0 / (n as f32 - 1.0);
    for z in 0..nn {
        for y in 0..nn {
            for x in 0..nn {
                let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
                g.set([x, y, z], sphere_sdf(p, r));
            }
        }
    }
    g
}

#[test]
fn multi_matches_single_blocks_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let c = 8;
    let n = 33u32;
    let grid = build_sphere_grid(c, n, 0.5);
    assert_eq!(grid.allocated_blocks(), 125, "格点 [0,33)³ 应触 5³ 块");

    let mm = mesh_blocks(&hd, &grid, None);
    assert_eq!(mm.blocks, grid.block_list(), "块表应与 block_list 一致");
    assert_eq!(mm.blocks.len(), 125);

    let mut total_single_quads = 0usize;
    let mut nonempty = 0usize;
    let s = mm.c + 1;
    for (k, &b) in mm.blocks.iter().enumerate() {
        let single = mesh_block(&hd, &grid, b, None);
        let (mpos, mflag) = mm.read_block_slots(&hd, k);
        let midx = mm.read_block_indices(&hd, k);
        assert_slots_bitwise(&mpos, &single.positions, &format!("块 {b:?}"));
        assert_eq!(mflag, single.flags, "块 {b:?} 标记不一致");
        // 单块索引 → 全局槽（+ block·(C+1)³）
        let base = (k as u32) * s * s * s;
        let single_global: Vec<u32> = single.indices.iter().map(|&i| i + base).collect();
        assert_eq!(
            canon_quads(&midx),
            canon_quads(&single_global),
            "块 {b:?} 四边形集合不一致"
        );
        assert_eq!(mm.quad_counts[k] as usize, single.quad_count as usize);
        total_single_quads += single.quad_count as usize;
        if single.quad_count > 0 {
            nonempty += 1;
        }
    }
    assert_eq!(
        mm.quad_total as usize, total_single_quads,
        "总四边形数不一致"
    );
    assert_eq!(
        mm.idx_offsets[mm.blocks.len()] as usize,
        mm.quad_total as usize * 6
    );
    assert!(nonempty >= 1, "应有非空块");
    println!(
        "multi vs single: {} 块全量逐位一致（{} 块非空，{} 四边形）",
        mm.blocks.len(),
        nonempty,
        mm.quad_total
    );

    // 确定性：重跑一次，计数与抽块内容一致
    let mm2 = mesh_blocks(&hd, &grid, None);
    assert_eq!(mm.quad_counts, mm2.quad_counts, "重跑计数不一致");
    assert_eq!(mm.idx_offsets, mm2.idx_offsets, "重跑段表不一致");
    let k = mm
        .quad_counts
        .iter()
        .position(|&q| q > 0)
        .expect("抽块应有非空");
    let (p1, f1) = mm.read_block_slots(&hd, k);
    let (p2, f2) = mm2.read_block_slots(&hd, k);
    assert_slots_bitwise(&p1, &p2, "重跑抽块");
    assert_eq!(f1, f2);
    assert_eq!(
        canon_quads(&mm.read_block_indices(&hd, k)),
        canon_quads(&mm2.read_block_indices(&hd, k)),
        "重跑抽块四边形集合不一致"
    );
}

/// 壳层场景：只分配"球面穿过的块 ± 2 cell 裕量"（稀疏分配的写侧用法）。
fn build_shell_grid(c: i32, n: u32, r: f32) -> SparseGrid {
    let h = 2.0 / (n as f32 - 1.0);
    let mut grid = SparseGrid::new(c);
    let nb = (n as i32 + c - 1) / c + 1;
    let cf = c as f32;
    for bz in 0..nb {
        for by in 0..nb {
            for bx in 0..nb {
                // 块 AABB（world 坐标）到球心（原点）：逐轴钳制求最近/最远距离
                let lo = |b: i32| (b * c) as f32 * h - 1.0;
                let hi = |b: i32| ((b * c) as f32 + cf) * h - 1.0;
                let (mut dmin2, mut dmax2) = (0.0f32, 0.0f32);
                for (bl, bh) in [(lo(bx), hi(bx)), (lo(by), hi(by)), (lo(bz), hi(bz))] {
                    let near = if bl > 0.0 {
                        bl
                    } else if bh < 0.0 {
                        bh
                    } else {
                        0.0
                    };
                    dmin2 += near * near;
                    let far = bl.abs().max(bh.abs());
                    dmax2 += far * far;
                }
                let margin = 2.0 * cf * h; // ±2 cell 裕量
                if dmin2.sqrt() <= r + margin && dmax2.sqrt() >= r - margin {
                    for z in bz * c..(bz + 1) * c {
                        for y in by * c..(by + 1) * c {
                            for x in bx * c..(bx + 1) * c {
                                let p =
                                    [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
                                grid.set([x, y, z], sphere_sdf(p, r));
                            }
                        }
                    }
                }
            }
        }
    }
    grid
}

/// 抽块 k 对单块原语 `mesh_block` 逐位/规范键一致（判据不换、只换规模）。
fn check_block_matches_single(
    hd: &Headless,
    grid: &SparseGrid,
    mm: &MultiMesh,
    k: usize,
    tag: &str,
) {
    let b = mm.blocks[k];
    let single = mesh_block(hd, grid, b, None);
    let (mpos, mflag) = mm.read_block_slots(hd, k);
    assert_slots_bitwise(&mpos, &single.positions, &format!("{tag}抽块 {b:?}"));
    assert_eq!(mflag, single.flags, "{tag}抽块 {b:?} 标记不一致");
    let s = mm.c + 1;
    let base = (k as u32) * s * s * s;
    let single_global: Vec<u32> = single.indices.iter().map(|&i| i + base).collect();
    assert_eq!(
        canon_quads(&mm.read_block_indices(hd, k)),
        canon_quads(&single_global),
        "{tag}抽块 {b:?} 四边形集合不一致"
    );
}

#[test]
fn multi_scale_shell_512() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let c = 16i32;
    let n = 512u32;
    let r = 0.75f32;
    let h = 2.0 / (n as f32 - 1.0);
    let r_cells = r / h;

    let grid = build_shell_grid(c, n, r);
    assert!(grid.allocated_blocks() > 500, "壳层块太少");

    let t0 = std::time::Instant::now();
    let mm = mesh_blocks(&hd, &grid, None);
    let wall = t0.elapsed();
    assert!(mm.quad_total > 10_000, "四边形数异常少：{}", mm.quad_total);
    let sum: u32 = mm.quad_counts.iter().sum();
    assert_eq!(sum, mm.quad_total);
    println!(
        "512³ 壳层：{} 块 / {:.1}M cell 数据 | {} 四边形 | 端到端 {wall:?}（r_cells≈{r_cells:.1}）",
        mm.blocks.len(),
        (mm.blocks.len() as f64) * (c as f64).powi(3) / 1e6,
        mm.quad_total,
    );

    // 抽块对单块原语一致（判据不换、只换规模）
    for k in [0usize, mm.blocks.len() / 2, mm.blocks.len() - 1] {
        check_block_matches_single(&hd, &grid, &mm, k, "规模");
    }
}

#[test]
fn multi_beyond_dispatch_cap() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let c = 8i32;
    let n = 512u32;
    let grid = build_shell_grid(c, n, 0.75);
    let blocks = grid.allocated_blocks();
    let wg_axis = ((c as u32 + 1).div_ceil(4)) as usize;
    assert!(
        blocks * wg_axis > 65535,
        "本片场景须超过单维派发上限（blocks={blocks} × wg_axis={wg_axis}）"
    );

    let t0 = std::time::Instant::now();
    let mm = mesh_blocks(&hd, &grid, None);
    let wall = t0.elapsed();
    assert!(mm.quad_total > 100_000, "四边形数异常少：{}", mm.quad_total);
    assert_eq!(mm.quad_counts.iter().sum::<u32>(), mm.quad_total);
    println!(
        "超限装配：{blocks} 块（×wg_axis={} > 65535，块号 grid-stride） | {} 四边形 | 端到端 {wall:?}",
        blocks * wg_axis,
        mm.quad_total,
    );

    for k in [0usize, blocks / 2, blocks - 1] {
        check_block_matches_single(&hd, &grid, &mm, k, "超限");
    }
}
