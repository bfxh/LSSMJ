//! 稀疏体素格写侧（自研缺口②第一片；锚 `W15A-034/035` 的块级 halo 契约与
//! `W15A-047/048` 的"Rust 无现成轮子：稀疏体素格**写侧** + GPU 遍历"）。
//!
//! 口径（钉死）：
//! - 值格点坐标为任意 `i32`（负数可用）；块 = C³（构造参数），块号 = 坐标 `div_euclid(C)`。
//! - **值所有权唯一**：块 `b` 拥有格点 `∈ [b·C, (b+1)·C)³`——块间**不复制**、无 halo 存储。
//!   "1-voxel halo" 是**读侧虚拟重叠**：网格化块 `b` 时读角值窗 `[b·C, b·C+C]`
//!   （每轴 C+1 个角值，来自本块与邻块；邻块缺失 ⇒ 该角为 [`EMPTY_VALUE`]，"带外"约定）。
//!   好处：写路径简单（单点单写）、无跨块一致性维护；代价：网格化需跨块读取（GPU 遍历侧后续片）。
//! - 稀疏性可观察：**只分配被写过的块**（`allocated_blocks()`）。
//!
//! `materialize(n)`：稠密化到 `[0, n)³` 值格（界外角 = `EMPTY_VALUE`）——可直接喂既有
//! GSN 运行器（`surface_nets_gpu_chunked`）。

use std::collections::HashMap;

/// 缺失角值的"带外"约定（大正数：对 SDF 口径 = 远离表面；限 finite 避免 inf 运算）。
pub const EMPTY_VALUE: f32 = 1e30;

/// 稀疏体素格（值所有权唯一；halo 读侧虚拟重叠）。
pub struct SparseGrid {
    side: i32,
    blocks: HashMap<[i32; 3], Vec<f32>>,
}

impl SparseGrid {
    /// `block_side` = 每轴块内格点数（C ≥ 2；C = 2、4、8、16 常见）。
    pub fn new(block_side: i32) -> Self {
        assert!(block_side >= 2, "块边长须 ≥ 2");
        Self {
            side: block_side,
            blocks: HashMap::new(),
        }
    }

    /// 写入格点值（首次触块时分配并按 `EMPTY_VALUE` 初始化）。
    pub fn set(&mut self, pos: [i32; 3], v: f32) {
        let b = [
            pos[0].div_euclid(self.side),
            pos[1].div_euclid(self.side),
            pos[2].div_euclid(self.side),
        ];
        let l = [
            pos[0].rem_euclid(self.side),
            pos[1].rem_euclid(self.side),
            pos[2].rem_euclid(self.side),
        ];
        let side = self.side as usize;
        let block = self
            .blocks
            .entry(b)
            .or_insert_with(|| vec![EMPTY_VALUE; side * side * side]);
        block[l[0] as usize + l[1] as usize * side + l[2] as usize * side * side] = v;
    }

    /// 读取格点值（未分配块 ⇒ `EMPTY_VALUE`）。
    pub fn get(&self, pos: [i32; 3]) -> f32 {
        let b = [
            pos[0].div_euclid(self.side),
            pos[1].div_euclid(self.side),
            pos[2].div_euclid(self.side),
        ];
        let Some(block) = self.blocks.get(&b) else {
            return EMPTY_VALUE;
        };
        let side = self.side as usize;
        let l = [
            pos[0].rem_euclid(self.side) as usize,
            pos[1].rem_euclid(self.side) as usize,
            pos[2].rem_euclid(self.side) as usize,
        ];
        block[l[0] + l[1] * side + l[2] * side * side]
    }

    /// 已分配的块数（稀疏性可观察）。
    pub fn allocated_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// 块边长（每轴格点数 C）。
    pub fn block_side(&self) -> i32 {
        self.side
    }

    /// **网格化窗口**（C+2)³：每轴 `[b·C−1, b·C+C]` 闭区间——相比角值窗多借**两侧各 1**
    /// 格点。必要性：cell 的四边形要引用 −1 邻 cell 的顶点（其角值含更低的 −1 层）。
    /// 返回按 z→y→x 排布。
    pub fn extract_mesh_window(&self, block: [i32; 3]) -> Vec<f32> {
        let w = (self.side + 2) as usize;
        let mut out = Vec::with_capacity(w * w * w);
        for z in -1..=self.side {
            for y in -1..=self.side {
                for x in -1..=self.side {
                    out.push(self.get([
                        block[0] * self.side + x,
                        block[1] * self.side + y,
                        block[2] * self.side + z,
                    ]));
                }
            }
        }
        out
    }

    /// 角值窗 `[b·C, b·C+C]`（每轴 C+1 个）——含跨块 halo 读（邻块缺失 ⇒ `EMPTY_VALUE`）。
    /// 返回按 z→y→x 排布的 `(C+1)³` 值（与既有值切片提取同序）。
    pub fn extract_corner_window(&self, block: [i32; 3]) -> Vec<f32> {
        let n = (self.side + 1) as usize;
        let mut out = Vec::with_capacity(n * n * n);
        for z in 0..=self.side {
            for y in 0..=self.side {
                for x in 0..=self.side {
                    out.push(self.get([
                        block[0] * self.side + x,
                        block[1] * self.side + y,
                        block[2] * self.side + z,
                    ]));
                }
            }
        }
        out
    }

    /// 稠密化到 `[0, n)³` 值格（界外角 = `EMPTY_VALUE`）。
    pub fn materialize(&self, n: u32) -> Vec<f32> {
        let mut out = vec![EMPTY_VALUE; (n as usize).pow(3)];
        let nn = n as i32;
        for (&b, block) in &self.blocks {
            let side = self.side as usize;
            for lz in 0..self.side {
                for ly in 0..self.side {
                    for lx in 0..self.side {
                        let p = [
                            b[0] * self.side + lx,
                            b[1] * self.side + ly,
                            b[2] * self.side + lz,
                        ];
                        if p.iter().any(|&c| c < 0 || c >= nn) {
                            continue;
                        }
                        let si = lx as usize + ly as usize * side + lz as usize * side * side;
                        let di = (p[0] + p[1] * nn + p[2] * nn * nn) as usize;
                        out[di] = block[si];
                    }
                }
            }
        }
        out
    }
}
