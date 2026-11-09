#![allow(clippy::needless_range_loop)]
//! UV 展开：接缝切分 + 每岛 LSCM 共形参数化 + shelf 打包。
//!
//! 与旧版“主轴平面投影”的区别：
//! - 接缝按二面角/边界切，封闭曲面按主轴补切（见 [`super::uv_lscm`]）；
//! - 每岛用 LSCM 求共形 UV，不再把前后表面投到同一块；
//! - 验收除岛间 AABB 外，还统计 **同岛三角形精确重叠**（空间哈希 + SAT）。
//!
//! 三值记账（`POLICY.md` §1）：
//! - A：岛朝向是 6 态，可用两个 trit 覆盖；岛状态 `TriState` 的 `Zero` 承担“未知/混合”。
//! - B：UV 坐标与 LSCM 求解是连续量，走 f32；三值化会毁掉贴图。
//! - C：OBJ `vt` 是生态格式，必须 f32。
//! - D：接缝/打包/LSCM 的复杂度与进制无关。

use std::collections::HashMap;

use super::eval::Mesh;
use super::uv_lscm;

/// 法线主轴（6 向）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    /// +X
    PosX,
    /// -X
    NegX,
    /// +Y
    PosY,
    /// -Y
    NegY,
    /// +Z
    PosZ,
    /// -Z
    NegZ,
}

impl Axis {
    /// 固定顺序。
    pub const ALL: [Axis; 6] = [
        Axis::PosX,
        Axis::NegX,
        Axis::PosY,
        Axis::NegY,
        Axis::PosZ,
        Axis::NegZ,
    ];

    /// 下标。
    pub fn index(self) -> usize {
        match self {
            Axis::PosX => 0,
            Axis::NegX => 1,
            Axis::PosY => 2,
            Axis::NegY => 3,
            Axis::PosZ => 4,
            Axis::NegZ => 5,
        }
    }

    /// 世界点投影到该轴的 2D 平面（等距，面积保持）。
    pub fn project(self, p: [f32; 3]) -> [f32; 2] {
        match self {
            Axis::PosX => [p[2], p[1]],
            Axis::NegX => [-p[2], p[1]],
            Axis::PosY => [p[0], -p[2]],
            Axis::NegY => [p[0], p[2]],
            Axis::PosZ => [p[0], p[1]],
            Axis::NegZ => [-p[0], p[1]],
        }
    }

    /// 由法线选主轴。
    pub fn of_normal(n: [f32; 3]) -> Self {
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        if ax >= ay && ax >= az {
            if n[0] >= 0.0 { Axis::PosX } else { Axis::NegX }
        } else if ay >= az {
            if n[1] >= 0.0 { Axis::PosY } else { Axis::NegY }
        } else if n[2] >= 0.0 {
            Axis::PosZ
        } else {
            Axis::NegZ
        }
    }
}

/// 三值朝向：`Pos` / `Neg` / `Zero`（Zero = 退化或同岛混合）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriState {
    /// 同向。
    Pos,
    /// 反向。
    Neg,
    /// 退化/混合。
    Zero,
}

/// 一个参数化岛。
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    /// 平均法线主轴。
    pub axis: Axis,
    /// 三值朝向。
    pub side: TriState,
    /// 三角形数。
    pub tris: usize,
    /// 3D AABB `[x,y,z,w,h,d]`（仅元数据）。
    pub world_rect: [f32; 6],
    /// 3D 表面积。
    pub world_area: f32,
    /// 打包后实际 UV 三角形面积。
    pub uv_area: f32,
    /// 打包后的 UV 矩形 `[u,v,w,h]`。
    pub uv_rect: [f32; 4],
}

/// UV 展开结果。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UvLayout {
    /// 每三角形角一个 UV。
    pub uvs: Vec<[f32; 2]>,
    /// 与输入三角形等长的 UV 索引。
    pub uv_tris: Vec<[u32; 3]>,
    /// 岛元数据。
    pub islands: Vec<Island>,
    /// atlas 尺寸（世界/UV 单位）。
    pub atlas_size: [f32; 2],
    /// UV 三角形面积 / atlas 面积。
    pub fill: f32,
    /// 岛 AABB 两两相交次数。
    pub overlaps: usize,
    /// 同岛三角形精确重叠对数。
    pub tri_overlaps: usize,
}

struct Packed {
    axis: Axis,
    side: TriState,
    tris: Vec<usize>,
    uv: HashMap<u32, [f32; 2]>,
    single: Option<[[f32; 2]; 3]>,
    rotated: bool,
    uv_area: f32,
    world_area: f32,
    rect: [f32; 4],
    uv_rect: [f32; 4],
    world_rect: [f32; 6],
}

/// 完成一个岛：3D/UV 面积、平均法线、AABB、密度归一化。
fn finish_island(
    mesh: &Mesh,
    tris: &[usize],
    mut map: HashMap<u32, [f32; 2]>,
) -> Option<Packed> {
    let (mut wmin, mut wmax) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let mut normal = [0.0f32; 3];
    let mut area3 = 0.0f32;
    for &ti in tris {
        let t = mesh.tris[ti];
        for &vi in &t {
            let v = mesh.verts[vi as usize];
            for k in 0..3 {
                wmin[k] = wmin[k].min(v[k]);
                wmax[k] = wmax[k].max(v[k]);
            }
        }
        let n = tri_normal(mesh, ti);
        for k in 0..3 {
            normal[k] += n[k];
        }
        area3 += tri_area(mesh, ti);
    }
    let axis = Axis::of_normal(normal);
    let side = if normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2] > 1e-12 {
        side_of(normal, axis)
    } else {
        TriState::Zero
    };
    let mut area_uv = 0.0f32;
    let (mut umin, mut umax) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for &ti in tris {
        let t = mesh.tris[ti];
        let a = map[&t[0]];
        let b = map[&t[1]];
        let c = map[&t[2]];
        area_uv += 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
        for p in [a, b, c] {
            for k in 0..2 {
                umin[k] = umin[k].min(p[k]);
                umax[k] = umax[k].max(p[k]);
            }
        }
    }
    if !(area_uv > 1e-12) {
        return None;
    }
    let s = (area3 / area_uv).sqrt().max(1e-6);
    for uv in map.values_mut() {
        uv[0] = (uv[0] - umin[0]) * s;
        uv[1] = (uv[1] - umin[1]) * s;
    }
    let w = (umax[0] - umin[0]) * s;
    let h = (umax[1] - umin[1]) * s;
    if !(w > 1e-9 && h > 1e-9) {
        return None;
    }
    Some(Packed {
        axis,
        side,
        tris: tris.to_vec(),
        uv: map,
        single: None,
        rotated: false,
        uv_area: area_uv * s * s,
        world_area: area3,
        rect: [0.0, 0.0, w, h],
        uv_rect: [0.0; 4],
        world_rect: [
            wmin[0],
            wmin[1],
            wmin[2],
            wmax[0] - wmin[0],
            wmax[1] - wmin[1],
            wmax[2] - wmin[2],
        ],
    })
}


/// 单三角形快路径：不建 HashMap，直接按 3 个顶点算。
fn finish_single(mesh: &Mesh, ti: usize, uvs: [[f32; 2]; 3]) -> Option<Packed> {
    let t = mesh.tris[ti];
    let (mut wmin, mut wmax) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for &vi in &t {
        let v = mesh.verts[vi as usize];
        for k in 0..3 {
            wmin[k] = wmin[k].min(v[k]);
            wmax[k] = wmax[k].max(v[k]);
        }
    }
    let normal = tri_normal(mesh, ti);
    let axis = Axis::of_normal(normal);
    let side = side_of(normal, axis);
    let area3 = tri_area(mesh, ti);
    let [a, b, c] = uvs;
    let area_uv = 0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
    if !(area_uv > 1e-12) {
        return None;
    }
    let (mut umin, mut umax) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in [a, b, c] {
        for k in 0..2 {
            umin[k] = umin[k].min(p[k]);
            umax[k] = umax[k].max(p[k]);
        }
    }
    let s = (area3 / area_uv).sqrt().max(1e-6);
    let uvs = uvs.map(|p| [(p[0] - umin[0]) * s, (p[1] - umin[1]) * s]);
    let w = (umax[0] - umin[0]) * s;
    let h = (umax[1] - umin[1]) * s;
    if !(w > 1e-9 && h > 1e-9) {
        return None;
    }
    Some(Packed {
        axis,
        side,
        tris: vec![ti],
        uv: HashMap::new(),
        single: Some(uvs),
        rotated: false,
        uv_area: area_uv * s * s,
        world_area: area3,
        rect: [0.0, 0.0, w, h],
        uv_rect: [0.0; 4],
        world_rect: [
            wmin[0],
            wmin[1],
            wmin[2],
            wmax[0] - wmin[0],
            wmax[1] - wmin[1],
            wmax[2] - wmin[2],
        ],
    })
}

/// 单次 shelf 打包：按高度降序的 `order` 逐个放入，返回 (uv_rects, rotated, atlas_w, atlas_h)。
///
/// `rotate_all` 为真时每个岛旋转 90°——岛形状差异大时这会改变 atlas 长宽比，
/// 与外层的宽度扫描一起决定最终 fill。
fn shelf_pack(
    packed: &[Packed],
    order: &[usize],
    target_w: f32,
    rotate_all: bool,
    pad: f32,
) -> (Vec<[f32; 4]>, Vec<bool>, f32, f32) {
    let mut rects = vec![[0.0f32; 4]; packed.len()];
    let mut rots = vec![false; packed.len()];
    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;
    let mut row_h = 0.0f32;
    let mut used_w = 0.0f32;
    for &pi in order {
        let rw = packed[pi].rect[2];
        let rh = packed[pi].rect[3];
        let w0 = rw + 2.0 * pad;
        let h0 = rh + 2.0 * pad;
        let (w, h) = if rotate_all { (h0, w0) } else { (w0, h0) };
        if cursor_x > 0.0 && cursor_x + w > target_w {
            cursor_y += row_h;
            cursor_x = 0.0;
            row_h = 0.0;
        }
        rots[pi] = rotate_all;
        rects[pi] = [
            cursor_x + pad,
            cursor_y + pad,
            if rotate_all { rh } else { rw },
            if rotate_all { rw } else { rh },
        ];
        cursor_x += w;
        row_h = row_h.max(h);
        used_w = used_w.max(cursor_x);
    }
    (
        rects,
        rots,
        used_w.max(1e-6),
        (cursor_y + row_h).max(1e-6),
    )
}

/// 展开：接缝 → LSCM → shelf 打包。
pub fn unwrap(mesh: &Mesh, padding: f32) -> UvLayout {
    let pad = padding.max(0.0);
    if mesh.tris.is_empty() {
        return UvLayout::default();
    }
    let fast = mesh.tris.len() > 50_000;
    let tri_islands = if fast {
        uv_lscm::block_islands(mesh, 4, 4000)
    } else {
        uv_lscm::seam_islands(mesh, 45.0)
    };

    let mut packed: Vec<Packed> = Vec::with_capacity(tri_islands.len());

    let mut work: Vec<Vec<usize>> = tri_islands;
    while let Some(tris) = work.pop() {
        if tris.is_empty() {
            continue;
        }
        let (global, raw) = if tris.len() == 1 {
            single_triangle_uv(mesh, tris[0])
        } else {
            uv_lscm::lscm(mesh, &tris)
        };
        if global.is_empty() {
            continue;
        }
        let map: HashMap<u32, [f32; 2]> =
            global.iter().copied().zip(raw.iter().copied()).collect();
        let bad = island_overlaps(mesh, &tris, &map);
        if !bad.is_empty() && tris.len() > 1 {
            let bad_set: std::collections::HashSet<usize> = bad.iter().copied().collect();
            let clean: Vec<usize> = tris
                .iter()
                .copied()
                .filter(|t| !bad_set.contains(t))
                .collect();
            // clean 保留原 LSCM UV，只递归拆掉仍有重叠的子集；不重新 LSCM。
            let mut pending = vec![clean];
            while let Some(sub) = pending.pop() {
                if sub.is_empty() {
                    continue;
                }
                let sub_set: std::collections::HashSet<u32> = sub
                    .iter()
                    .flat_map(|&t| mesh.tris[t].iter().copied())
                    .collect();
                let sub_map: HashMap<u32, [f32; 2]> = map
                    .iter()
                    .filter(|(k, _)| sub_set.contains(*k))
                    .map(|(k, &v)| (*k, v))
                    .collect();
                let sub_bad = island_overlaps(mesh, &sub, &sub_map);
                if sub_bad.is_empty() {
                    if let Some(p) = finish_island(mesh, &sub, sub_map) {
                        packed.push(p);
                    }
                } else {
                    let s: std::collections::HashSet<usize> = sub_bad.iter().copied().collect();
                    let clean2: Vec<usize> =
                        sub.iter().copied().filter(|t| !s.contains(t)).collect();
                    if !clean2.is_empty() {
                        pending.push(clean2);
                    }
                    for ti in sub_bad {
                        work.push(vec![ti]);
                    }
                }
            }
            for ti in bad {
                work.push(vec![ti]);
            }
            continue;
        }
        if let Some(p) = finish_island(mesh, &tris, map) {
            packed.push(p);
        }
    }

    // shelf 打包：高度降序（同高按轴序）。
    let mut order: Vec<usize> = (0..packed.len()).collect();
    order.sort_by(|&a, &b| {
        packed[b]
            .rect[3]
            .total_cmp(&packed[a].rect[3])
            .then_with(|| packed[a].axis.cmp(&packed[b].axis))
    });
    let total_area: f32 = packed
        .iter()
        .map(|p| (p.rect[2] + 2.0 * pad) * (p.rect[3] + 2.0 * pad))
        .sum();
    let max_w = packed
        .iter()
        .map(|p| p.rect[2] + 2.0 * pad)
        .fold(0.0f32, f32::max);
    // 多宽度 × 旋转 尝试，取 atlas 面积最小者（fill = uv_area / (aw*ah)，uv_area 固定）。
    // 单一 target_w = sqrt(total_area) 在大 mesh（岛多且碎）上会浪费大量 atlas 空间。
    let mut best: Option<(f32, Vec<[f32; 4]>, Vec<bool>, f32, f32)> = None;
    for wf in [0.55f32, 0.7, 0.85, 1.0, 1.15, 1.3, 1.5] {
        for rotate_all in [false, true] {
            let target_w = (total_area.sqrt() * wf).max(max_w).max(1e-6);
            let (rects, rots, aw, ah) = shelf_pack(&packed, &order, target_w, rotate_all, pad);
            let area = aw * ah;
            if best.as_ref().is_none_or(|b| area < b.0) {
                best = Some((area, rects, rots, aw, ah));
            }
        }
    }
    let (_, rects, rots, atlas_w, atlas_h) = best.expect("打包尝试至少一个");
    for (pi, r) in rects.iter().enumerate() {
        packed[pi].uv_rect = *r;
        packed[pi].rotated = rots[pi];
    }
    let scale = 1.0 / atlas_w.max(atlas_h);

    // 写 UV（每三角形角一个）。
    let mut tri_island: Vec<usize> = vec![usize::MAX; mesh.tris.len()];
    for (pi, p) in packed.iter().enumerate() {
        for &ti in &p.tris {
            tri_island[ti] = pi;
        }
    }
    let mut uvs = Vec::with_capacity(mesh.tris.len() * 3);
    let mut uv_tris = Vec::with_capacity(mesh.tris.len());
    for ti in 0..mesh.tris.len() {
        let pi = tri_island[ti];
        let base = uvs.len() as u32;
        if pi == usize::MAX {
            // 退化/被跳过的三角形：占位 UV，保持 uv_tris 与 mesh.tris 等长。
            uvs.push([0.0, 0.0]);
            uvs.push([0.0, 0.0]);
            uvs.push([0.0, 0.0]);
        } else {
            let p = &packed[pi];
            if let Some(single) = p.single {
                for uv in single {
                    let uv = if p.rotated {
                        [uv[1], p.rect[2] - uv[0]]
                    } else {
                        uv
                    };
                    uvs.push([
                        (p.uv_rect[0] + uv[0]) * scale,
                        (p.uv_rect[1] + uv[1]) * scale,
                    ]);
                }
            } else {
                for &vi in &mesh.tris[ti] {
                    let uv = p.uv[&vi];
                    let uv = if p.rotated {
                        [uv[1], p.rect[2] - uv[0]]
                    } else {
                        uv
                    };
                    uvs.push([
                        (p.uv_rect[0] + uv[0]) * scale,
                        (p.uv_rect[1] + uv[1]) * scale,
                    ]);
                }
            }
        }
        uv_tris.push([base, base + 1, base + 2]);
    }

    // 统计。
    let islands: Vec<Island> = packed
        .iter()
        .map(|p| Island {
            axis: p.axis,
            side: p.side,
            tris: p.tris.len(),
            world_rect: p.world_rect,
            world_area: p.world_area,
            uv_area: p.uv_area * scale * scale,
            uv_rect: [
                p.uv_rect[0] * scale,
                p.uv_rect[1] * scale,
                p.uv_rect[2] * scale,
                p.uv_rect[3] * scale,
            ],
        })
        .collect();
    let uv_area: f32 = packed.iter().map(|p| p.uv_area).sum();
    let fill = if atlas_w * atlas_h > 0.0 {
        (uv_area * scale * scale) / (atlas_w * atlas_h * scale * scale)
    } else {
        0.0
    };
    let overlaps = {
        let mut overlaps = 0usize;
        for i in 0..islands.len() {
            for j in (i + 1)..islands.len() {
                if rects_overlap(islands[i].uv_rect, islands[j].uv_rect) {
                    overlaps += 1;
                }
            }
        }
        overlaps
    };
    let island_of: Vec<usize> = tri_island.clone();
    let tri_overlaps = {
        count_tri_overlaps(mesh, &uvs, &uv_tris, &island_of)
    };

    UvLayout {
        uvs,
        uv_tris,
        islands,
        atlas_size: [atlas_w, atlas_h],
        fill,
        overlaps,
        tri_overlaps,
    }
}

fn tri_area(mesh: &Mesh, ti: usize) -> f32 {
    let t = mesh.tris[ti];
    let a = mesh.verts[t[0] as usize];
    let b = mesh.verts[t[1] as usize];
    let c = mesh.verts[t[2] as usize];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cr = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt()
}

fn tri_normal(mesh: &Mesh, ti: usize) -> [f32; 3] {
    let t = mesh.tris[ti];
    let a = mesh.verts[t[0] as usize];
    let b = mesh.verts[t[1] as usize];
    let c = mesh.verts[t[2] as usize];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 1e-9 {
        [n[0] / l, n[1] / l, n[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn side_of(n: [f32; 3], axis: Axis) -> TriState {
    let d = match axis {
        Axis::PosX => n[0],
        Axis::NegX => -n[0],
        Axis::PosY => n[1],
        Axis::NegY => -n[1],
        Axis::PosZ => n[2],
        Axis::NegZ => -n[2],
    };
    if d > 1e-6 {
        TriState::Pos
    } else if d < -1e-6 {
        TriState::Neg
    } else {
        TriState::Zero
    }
}

/// 同岛三角形精确重叠对数：空间哈希 + 2D SAT；共享 3D 边的相邻三角形跳过。

/// 单三角形 UV：投影到法线主轴平面，平移到 (0,0)。
fn single_triangle_uv(mesh: &Mesh, ti: usize) -> (Vec<u32>, Vec<[f32; 2]>) {
    let t = mesh.tris[ti];
    let axis = Axis::of_normal(tri_normal(mesh, ti));
    let mut uvs: Vec<[f32; 2]> = t
        .iter()
        .map(|&vi| axis.project(mesh.verts[vi as usize]))
        .collect();
    // 最长边对齐 x 轴，让三角形 AABB 更紧（单三角形岛的 fill 关键）。
    let edges = [
        (0usize, 1usize),
        (1usize, 2usize),
        (2usize, 0usize),
    ];
    let mut best = (0.0f32, 0usize, 1usize);
    for &(i, j) in &edges {
        let d = [uvs[j][0] - uvs[i][0], uvs[j][1] - uvs[i][1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if l > best.0 {
            best = (l, i, j);
        }
    }
    if best.0 > 1e-9 {
        let (_, i, j) = best;
        let d = [uvs[j][0] - uvs[i][0], uvs[j][1] - uvs[i][1]];
        let l = best.0;
        let (c, s) = (d[0] / l, d[1] / l);
        for uv in &mut uvs {
            let x = uv[0];
            let y = uv[1];
            uv[0] = x * c + y * s;
            uv[1] = -x * s + y * c;
        }
    }
    let (mut minx, mut miny) = (f32::INFINITY, f32::INFINITY);
    for uv in &uvs {
        minx = minx.min(uv[0]);
        miny = miny.min(uv[1]);
    }
    for uv in &mut uvs {
        uv[0] -= minx;
        uv[1] -= miny;
    }
    (t.to_vec(), uvs)
}

/// 岛内三角形精确重叠（局部扫描线）。
fn island_overlaps(mesh: &Mesh, tris: &[usize], map: &HashMap<u32, [f32; 2]>) -> Vec<usize> {
    if tris.len() < 2 {
        return Vec::new();
    }
    let mut items: Vec<(f32, f32, f32, f32, usize)> = Vec::with_capacity(tris.len());
    for (k, &ti) in tris.iter().enumerate() {
        let t = mesh.tris[ti];
        let p = [map[&t[0]], map[&t[1]], map[&t[2]]];
        let minx = p.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min);
        let maxx = p.iter().map(|q| q[0]).fold(f32::NEG_INFINITY, f32::max);
        let miny = p.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min);
        let maxy = p.iter().map(|q| q[1]).fold(f32::NEG_INFINITY, f32::max);
        items.push((minx, maxx, miny, maxy, k));
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bad = Vec::new();
    for i in 0..items.len() {
        let (_, maxx, miny, maxy, a) = items[i];
        for item in items.iter().skip(i + 1) {
            let (minx_j, _, miny_j, maxy_j, b) = *item;
            if minx_j > maxx {
                break;
            }
            let (ta, tb) = (tris[a], tris[b]);
            if maxy < miny_j || miny > maxy_j || share_edge(mesh, ta, tb) {
                continue;
            }
            let pa = {
                let t = mesh.tris[ta];
                [map[&t[0]], map[&t[1]], map[&t[2]]]
            };
            let pb = {
                let t = mesh.tris[tb];
                [map[&t[0]], map[&t[1]], map[&t[2]]]
            };
            if tri_overlap_2d(pa, pb) {
                bad.push(ta);
                bad.push(tb);
            }
        }
    }
    bad.sort_unstable();
    bad.dedup();
    bad
}

fn count_tri_overlaps(
    mesh: &Mesh,
    uvs: &[[f32; 2]],
    uv_tris: &[[u32; 3]],
    island_of: &[usize],
) -> usize {
    let pts = |ut: [u32; 3]| [uvs[ut[0] as usize], uvs[ut[1] as usize], uvs[ut[2] as usize]];
    let max_island = island_of
        .iter()
        .copied()
        .filter(|&p| p != usize::MAX)
        .max()
        .unwrap_or(0)
        + 1;
    let mut total = 0usize;
    // 扫描线：按 min_x 排序，活跃集用 max_x 截断；精确、无桶上限、内存 O(F)。
    for isl in 0..max_island {
        let mut items: Vec<(f32, f32, f32, f32, usize)> = Vec::new();
        for (ti, ut) in uv_tris.iter().enumerate() {
            if ti >= island_of.len() || island_of[ti] != isl {
                continue;
            }
            let p = pts(*ut);
            let minx = p.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min);
            let maxx = p.iter().map(|q| q[0]).fold(f32::NEG_INFINITY, f32::max);
            let miny = p.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min);
            let maxy = p.iter().map(|q| q[1]).fold(f32::NEG_INFINITY, f32::max);
            items.push((minx, maxx, miny, maxy, ti));
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        for i in 0..items.len() {
            let (_, maxx, miny, maxy, a) = items[i];
            for item in items.iter().skip(i + 1) {
                let (minx_j, _, miny_j, maxy_j, b) = *item;
                if minx_j > maxx {
                    break;
                }
                if maxy < miny_j || miny > maxy_j || share_edge(mesh, a, b) {
                    continue;
                }
                if tri_overlap_2d(pts(uv_tris[a]), pts(uv_tris[b])) {
                    total += 1;
                }
            }
        }
    }
    total
}

fn share_edge(mesh: &Mesh, a: usize, b: usize) -> bool {
    let ta = mesh.tris[a];
    let tb = mesh.tris[b];
    let mut shared = 0;
    for &x in &ta {
        if tb.contains(&x) {
            shared += 1;
        }
    }
    shared >= 2
}

fn tri_overlap_2d(a: [[f32; 2]; 3], b: [[f32; 2]; 3]) -> bool {
    for tri in [a, b] {
        for k in 0..3 {
            let p = tri[k];
            let q = tri[(k + 1) % 3];
            let axis = [q[1] - p[1], -(q[0] - p[0])];
            let (amin, amax) = project(a, axis);
            let (bmin, bmax) = project(b, axis);
            if amax <= bmin + 1e-7 || bmax <= amin + 1e-7 {
                return false;
            }
        }
    }
    true
}

fn project(tri: [[f32; 2]; 3], axis: [f32; 2]) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for p in tri {
        let v = p[0] * axis[0] + p[1] * axis[1];
        min = min.min(v);
        max = max.max(v);
    }
    (min, max)
}

fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    let eps = 1e-7;
    a[0] + a[2] > b[0] + eps
        && b[0] + b[2] > a[0] + eps
        && a[1] + a[3] > b[1] + eps
        && b[1] + b[3] > a[1] + eps
}

/// 写 OBJ + UV（`vt`）：每个 mesh 自动展开，面写成 `v/vt`。
pub fn write_obj_uv(parts: &[(String, Vec<Mesh>)], path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "# trit design export (uv)")?;
    let mut base_v = 1u32;
    let mut base_vt = 1u32;
    for (name, bodies) in parts {
        for (bi, m) in bodies.iter().enumerate() {
            let layout = unwrap(m, 0.001);
            writeln!(f, "o {}#{}", name, bi)?;
            for v in &m.verts {
                writeln!(f, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2])?;
            }
            for uv in &layout.uvs {
                writeln!(f, "vt {:.6} {:.6}", uv[0], uv[1])?;
            }
            for (ti, t) in m.tris.iter().enumerate() {
                let ut = layout.uv_tris[ti];
                writeln!(
                    f,
                    "f {}/{} {}/{} {}/{}",
                    t[0] + base_v,
                    ut[0] + base_vt,
                    t[1] + base_v,
                    ut[1] + base_vt,
                    t[2] + base_v,
                    ut[2] + base_vt,
                )?;
            }
            base_v += m.verts.len() as u32;
            base_vt += layout.uvs.len() as u32;
        }
    }
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        let verts = vec![
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        Mesh { verts, tris }
    }

    #[test]
    fn cube_has_six_islands_no_overlap() {
        let m = cube();
        let layout = unwrap(&m, 0.01);
        assert_eq!(layout.islands.len(), 6, "立方体应有 6 个面岛");
        assert_eq!(layout.uvs.len(), 36);
        assert_eq!(layout.uv_tris.len(), 12);
        assert_eq!(layout.overlaps, 0, "岛 AABB 不得相交");
        assert_eq!(layout.tri_overlaps, 0, "立方体同岛三角形不应重叠");
        assert!(layout.fill > 0.2, "fill 太低: {}", layout.fill);
        for uv in &layout.uvs {
            assert!((0.0..=1.0).contains(&uv[0]), "u 越界: {uv:?}");
            assert!((0.0..=1.0).contains(&uv[1]), "v 越界: {uv:?}");
        }
    }

    #[test]
    fn empty_mesh_is_empty() {
        let layout = unwrap(
            &Mesh {
                verts: vec![],
                tris: vec![],
            },
            0.01,
        );
        assert!(layout.uvs.is_empty());
        assert!(layout.islands.is_empty());
        assert_eq!(layout.tri_overlaps, 0);
    }

    #[test]
    fn lscm_keeps_planar_squares_area() {
        let m = cube();
        let layout = unwrap(&m, 0.0);
        let ratios: Vec<f32> = layout
            .islands
            .iter()
            .map(|i| i.uv_area / i.world_area.max(1e-9))
            .collect();
        let max = ratios.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min = ratios.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(max / min < 1.05, "texel 密度应一致: {ratios:?}");
    }

    #[test]
    fn overlapping_triangles_are_detected() {
        let m = Mesh {
            verts: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.2, 0.2, 0.0],
                [1.2, 0.2, 0.0],
                [0.2, 1.2, 0.0],
            ],
            tris: vec![[0, 1, 2], [3, 4, 5]],
        };
        let uvs = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [0.2, 0.2],
            [1.2, 0.2],
            [0.2, 1.2],
        ];
        let uv_tris = vec![[0, 1, 2], [3, 4, 5]];
        let island_of = vec![0, 0];
        assert!(count_tri_overlaps(&m, &uvs, &uv_tris, &island_of) >= 1);
    }

    #[test]
    fn deterministic() {
        let m = cube();
        let a = unwrap(&m, 0.01);
        let b = unwrap(&m, 0.01);
        assert_eq!(a, b);
    }
}
