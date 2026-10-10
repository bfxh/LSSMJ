//! LSSMJ 表示转换核心（第十四轮 G14 → 任务轨 16 / 判据 C22）。
//!
//! 本 crate 是 `scratch/conv-proto` 原型的迁入版（判据先行的 CPU 参照）+
//! GPU 积木第一片（JFA 距离场，`jfa` 模块）。判据对应关系：
//! - J2 mesh→SDF 解析对拍：`tests/cpu_legs.rs::mesh_sdf_matches_analytic_band`
//! - J3a 体积守恒 / J3b 亏格 / J3c 逐位确定性：同文件三个 test
//! - J4 粒子→无符号场（naive 基线）：`particles_unsigned_field_matches_analytic`
//! - GPU JFA vs CPU 暴力参照 + 确定性：`tests/jfa_gpu.rs`
//!
//! # 能力边界（v5 阶段 0 · 不宣称面）
//!
//! - **几何**：mesh→SDF 符号两档——`Winding`（绕数）对任意**闭合定向**网格成立；
//!   `Radial`（径向出射）只对**凸体**口径成立（多分量/非星形会判错，判据里钉着错例证据）。
//!   开放网格 / 非流形网格不在支持面。
//! - **尺寸**：网格 `n ∈ [2, 1625]`（n³ ≤ u32::MAX）；公共入口边界拒绝
//!   （`require_grid` / `require_device_buffer`，显式 assert ⇒ debug/release 同判）；
//!   GPU 缓冲另受适配器 `max_buffer_size` 约束。
//! - **后端**：wgpu 适配器（storage buffer 必需；timestamp query 可选）；判据实测
//!   RTX 4060 Ti (Vulkan)。无适配器 ⇒ `headless_device` panic（明确失败，不静默降级）。
//! - **确定性口径**：GPU 逐位确定只在**同端同机**主张；跨端/跨机不主张（atan2 降低与
//!   驱动差异），对拍走容差带。

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

pub mod budget;
pub mod gaussian;
pub mod gsn;
pub mod jfa;
pub mod kernels;
pub mod mesh_sdf_gpu;
pub mod ply;
pub mod quant;
pub mod scan;
pub mod sparse;
pub mod spz;
pub mod surfel;
pub mod timer;

/// 域边长（index 坐标 [0, N)），网格间距 1；世界坐标由调用方缩放。
pub const GRID: u32 = 64;

/// 网格尺寸契约（v5 阶段 0 能力边界 / F12 附加约束）：`n ≥ 2` 且 `n³ ≤ u32::MAX`（n ≤ 1625）。
/// 所有分配 n³ 缓冲的公共入口在边界统一调用——**显式拒绝，debug/release 同判**
/// （先于任何 u32 乘法执行，不做会溢出的 `n*n*n`，避免 overflow 行为差异）。
pub fn require_grid(n: u32, ctx: &str) {
    assert!(n >= 2, "{ctx}: 网格 n={n} < 2（cell 域 [0, n−1) 至少一格）");
    let n3 = (n as u64) * (n as u64) * (n as u64);
    assert!(
        n3 <= u32::MAX as u64,
        "{ctx}: 网格 n³={n3} 溢出 u32 线性索引（n={n} > 1625）"
    );
}

/// GPU 资源上限的边界预检：请求字节数超适配器 `max_buffer_size` 时给明确拒绝，
/// 不把 wgpu 的验证错误裸抛给调用方（v5 阶段 0：未支持输入不被宣称通过）。
pub fn require_device_buffer(device: &wgpu::Device, bytes: u64, ctx: &str) {
    let cap = device.limits().max_buffer_size;
    assert!(
        bytes <= cap,
        "{ctx}: 请求缓冲 {bytes} B 超适配器上限 {cap} B（降网格 n 或换大显存适配器）"
    );
}

/// 确定性 LCG（判据禁用不确定源）。
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// 均匀 [0, 1)：取状态高 24 位乘 2^-24（粒度 2^-24，最大值 1−2^-24 < 1）。
    /// 旧实现（>>33 除 u32::MAX）分子只有 31 位、值域实为 [0, 0.5]（F11，2026-10-10 修复；
    /// 采样序列变更已换约：gaussian 金样哈希随本修复披露更新）。
    pub fn next01(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) * (1.0 / (1u64 << 24) as f32)
    }
}

/// 解析球 SDF（世界坐标，球心在原点）。
pub fn sphere_sdf(p: [f32; 3], radius: f32) -> f32 {
    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - radius
}

/// 解析盒 SDF（世界坐标，心在原点、半边长 half）——与 `sphere_sdf` 同族的对拍参照。
pub fn box_sdf(p: [f32; 3], half: f32) -> f32 {
    p[0].abs().max(p[1].abs()).max(p[2].abs()) - half
}

/// 轴对齐盒网格（半边长 half，心在原点）：8 顶点 / 12 三角，法向一致朝外。
/// T-GC-01 精度档的"盒"金样——三角化即精确（无弦差），解析内外与解析距离都由 `box_sdf` 给出。
pub fn box_mesh(half: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let corners: [[f32; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let verts = corners
        .iter()
        .map(|c| [c[0] * half, c[1] * half, c[2] * half])
        .collect();
    let faces = vec![
        [0u32, 2, 1],
        [0, 3, 2], // −z
        [4, 5, 6],
        [4, 6, 7], // +z
        [0, 1, 5],
        [0, 5, 4], // −y
        [3, 7, 6],
        [3, 6, 2], // +y
        [0, 7, 3],
        [0, 4, 7], // −x
        [1, 2, 6],
        [1, 6, 5], // +x
    ];
    (verts, faces)
}

/// 解析场 → 体素网格（域 [-1,1]³，N³）。
pub fn field_to_voxels(n: u32, radius: f32) -> Vec<f32> {
    require_grid(n, "field_to_voxels");
    let h = 2.0 / (n as f32 - 1.0);
    let count = (n * n * n) as usize;
    let mut sdf = vec![0f32; count];
    for (i, d) in sdf.iter_mut().enumerate() {
        let i = i as u32;
        let x = i % n;
        let y = (i / n) % n;
        let z = i / (n * n);
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        *d = sphere_sdf(p, radius);
    }
    sdf
}

/// 点-三角精确距离（Ericson, Real-Time Collision Detection）。
pub fn point_tri_dist(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm(ap);
    }
    let bp = [p[0] - b[0], p[1] - b[1], p[2] - b[2]];
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return norm(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
        return norm(sub(q, p));
    }
    let cp = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return norm(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        let q = [a[0] + ac[0] * t, a[1] + ac[1] * t, a[2] + ac[2] * t];
        return norm(sub(q, p));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let q = [
            b[0] + (c[0] - b[0]) * t,
            b[1] + (c[1] - b[1]) * t,
            b[2] + (c[2] - b[2]) * t,
        ];
        return norm(sub(q, p));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    let q = [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ];
    norm(sub(q, p))
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn mul(a: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] * t, a[1] * t, a[2] * t]
}

/// icosphere（20 面体 + sub 次细分，半径 radius）。
pub fn icosphere(sub: u32, radius: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut v: Vec<[f32; 3]> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .iter()
    .map(|p| {
        let l = norm(*p);
        mul(*p, radius / l)
    })
    .collect();
    let mut f: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..sub {
        let mut cache: HashMap<(u32, u32), u32> = HashMap::new();
        let old = f.clone();
        f.clear();
        for [a, b, c] in old {
            let ab = midpoint(a, b, &mut v, &mut cache, radius);
            let bc = midpoint(b, c, &mut v, &mut cache, radius);
            let ca = midpoint(c, a, &mut v, &mut cache, radius);
            f.push([a, ab, ca]);
            f.push([b, bc, ab]);
            f.push([c, ca, bc]);
            f.push([ab, bc, ca]);
        }
    }
    (v, f)
}

fn midpoint(
    a: u32,
    b: u32,
    v: &mut Vec<[f32; 3]>,
    cache: &mut HashMap<(u32, u32), u32>,
    radius: f32,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(&m) = cache.get(&key) {
        return m;
    }
    let m = mul(sub(v[b as usize], v[a as usize]), 0.5);
    let m = [
        v[a as usize][0] + m[0],
        v[a as usize][1] + m[1],
        v[a as usize][2] + m[2],
    ];
    let m = mul(m, radius / norm(m));
    v.push(m);
    let idx = (v.len() - 1) as u32;
    cache.insert(key, idx);
    idx
}

/// 径向出射符号（凸体口径：射线起点=原点，|p| 小于出射半径 ⇔ 内部）。
/// 首跑判据抓到的仪器错就在这个起点上（见 w15c 账本 W15C-007）。
pub fn radial_sign(p: [f32; 3], verts: &[[f32; 3]], faces: &[[u32; 3]]) -> f32 {
    let pl = norm(p);
    if pl == 0.0 {
        return -1.0;
    }
    let d = mul(p, 1.0 / pl);
    let mut t_exit = 0f32;
    for [ia, ib, ic] in faces {
        let a = verts[*ia as usize];
        let b = verts[*ib as usize];
        let c = verts[*ic as usize];
        let e1 = sub(b, a);
        let e2 = sub(c, a);
        let pv = cross(d, e2);
        let det = dot(e1, pv);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        // 射线起点=原点（此前误用 p 起点，导致全部内部点翻号）
        let tv = [-a[0], -a[1], -a[2]];
        let u = dot(tv, pv) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let qv = cross(tv, e1);
        let v = dot(d, qv) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = dot(e2, qv) * inv;
        if t > t_exit {
            t_exit = t;
        }
    }
    if pl < t_exit { -1.0 } else { 1.0 }
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// 1/(4π)；与 WGSL 侧 `mesh_sdf.wgsl` 的 `INV_4PI` 同字面量（CPU↔GPU 绕数对拍口径）。
const INV_4PI: f32 = 0.07957747;

/// 广义绕数（Van Oosterom–Strackee 立体角和 / 4π）——精度档内外判定的核（T-GC-01）。
///
/// 闭合定向网格上取值恰为 ±1（内部）/ 0（外部），与三角化粗细无关；多分量重叠处按重数叠加（≈2）。
/// 相对 `radial_sign` 的差别：不需要凸体/星形前提——原点落在实体外时径向出射口径会整片判错。
/// 法向整体翻转只翻 w 的符号 ⇒ 判定用 |w|（见 `winding_sign`）。
/// `p` 与某三角顶点重合时该三角立体角无定义，贡献记 0（与 WGSL 侧同口径）。
pub fn winding_number(p: [f32; 3], verts: &[[f32; 3]], faces: &[[u32; 3]]) -> f32 {
    let mut sum = 0f32;
    for [ia, ib, ic] in faces {
        let a = sub(verts[*ia as usize], p);
        let b = sub(verts[*ib as usize], p);
        let c = sub(verts[*ic as usize], p);
        let (la, lb, lc) = (norm(a), norm(b), norm(c));
        if la == 0.0 || lb == 0.0 || lc == 0.0 {
            continue;
        }
        let num = dot(a, cross(b, c));
        if num == 0.0 {
            // 分子恰为 0（退化三角 b×c≡0，或 p 恰在三角平面上）⇒ 立体角真值 0。
            // 不能交给 atan2：den 在 f32 下可为负（p 贴近退化弦时相消），atan2(0, 负) = π
            // 会让 w 突跳 0.5 ⇒ 符号翻转（robustness 判据在 GPU 全场抓到 12 格，2026-10-11）。
            continue;
        }
        let den = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
        sum += 2.0 * num.atan2(den);
    }
    sum * INV_4PI
}

/// 精度档符号：|w| ≥ 0.5 ⇔ 内部（-1），否则外部（+1）。
/// 阈值 0.5 的口径：整体翻转的法向只改 w 符号不改判定；带内体素离表面有限距，
/// w 不会停在 0.5 附近（闭合网格上 w 在实体内恒 ±1、外恒 0）。
pub fn winding_sign(p: [f32; 3], verts: &[[f32; 3]], faces: &[[u32; 3]]) -> f32 {
    if winding_number(p, verts, faces).abs() >= 0.5 {
        -1.0
    } else {
        1.0
    }
}

/// 三角网 → SDF（窄带 ±band_h；暴力点-三角 + 径向符号）。
pub fn mesh_to_sdf_band(
    n: u32,
    radius: f32,
    band_h: f32,
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> (Vec<f32>, f32) {
    band_sdf(n, radius, band_h, verts, faces, radial_sign)
}

/// 三角网 → SDF（窄带；暴力点-三角 + 绕数符号）——与 `mesh_to_sdf_band` 同带口径，仅符号换精度档。
/// 本参照为球面金样而写（带门用解析球）；一般网格请直接用 `winding_sign` 逐点判定。
pub fn mesh_to_sdf_band_winding(
    n: u32,
    radius: f32,
    band_h: f32,
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> (Vec<f32>, f32) {
    band_sdf(n, radius, band_h, verts, faces, winding_sign)
}

/// 符号档函数指针（`radial_sign` / `winding_sign`）。
type SignFn = fn([f32; 3], &[[f32; 3]], &[[u32; 3]]) -> f32;

fn band_sdf(
    n: u32,
    radius: f32,
    band_h: f32,
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
    sign: SignFn,
) -> (Vec<f32>, f32) {
    require_grid(n, "mesh_to_sdf_band*");
    let h = 2.0 / (n as f32 - 1.0);
    let mut sdf = vec![f32::INFINITY; (n * n * n) as usize];
    let mut max_err = 0f32;
    for (i, d) in sdf.iter_mut().enumerate() {
        let i = i as u32;
        let x = i % n;
        let y = (i / n) % n;
        let z = i / (n * n);
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        let a = sphere_sdf(p, radius);
        if a.abs() > band_h {
            continue;
        }
        let mut d_min = f32::MAX;
        for f in faces {
            let dd = point_tri_dist(
                p,
                verts[f[0] as usize],
                verts[f[1] as usize],
                verts[f[2] as usize],
            );
            if dd < d_min {
                d_min = dd;
            }
        }
        *d = d_min * sign(p, verts, faces);
        max_err = max_err.max((*d - a).abs());
    }
    (sdf, max_err)
}

/// 网格体积（发散定理；positions 为世界坐标）。
pub fn mesh_volume(positions: &[[f32; 3]], indices: &[u32]) -> f32 {
    let mut vol = 0f32;
    for t in indices.as_chunks::<3>().0 {
        let a = positions[t[0] as usize];
        let b = positions[t[1] as usize];
        let c = positions[t[2] as usize];
        vol += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    (vol / 6.0).abs()
}

/// 欧拉特征数 (V, E, F)。
pub fn euler_char(positions: &[[f32; 3]], indices: &[u32]) -> (usize, usize, usize) {
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for t in indices.as_chunks::<3>().0 {
        for e in 0..3 {
            let a = t[e];
            let b = t[(e + 1) % 3];
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    (positions.len(), edges.len(), indices.len() / 3)
}

/// 逐位确定性哈希（FNV-1a over f32 bits + indices）。
pub fn mesh_hash(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
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

/// Fibonacci 球面粒子（确定性）。
pub fn fibonacci_sphere(k: usize, radius: f32) -> Vec<[f32; 3]> {
    let golden = 0.5 * (5.0f32.sqrt() - 1.0);
    (0..k)
        .map(|j| {
            let zz = 1.0 - 2.0 * (j as f32 + 0.5) / k as f32;
            let r = (1.0 - zz * zz).sqrt();
            let th = 2.0 * PI * golden * j as f32;
            [radius * r * th.cos(), radius * r * th.sin(), radius * zz]
        })
        .collect()
}

/// 粒子 → 无符号场（naive 最近粒子，JFA 的 CPU 参照口径）。
pub fn naive_particle_field(
    n: u32,
    radius: f32,
    band_h: f32,
    particles: &[[f32; 3]],
) -> (Vec<f32>, f32, f32) {
    require_grid(n, "naive_particle_field");
    let h = 2.0 / (n as f32 - 1.0);
    let mut field = vec![f32::INFINITY; (n * n * n) as usize];
    let mut sum_err = 0f32;
    let mut max_err = 0f32;
    let mut cnt = 0usize;
    for (i, d) in field.iter_mut().enumerate() {
        let i = i as u32;
        let x = i % n;
        let y = (i / n) % n;
        let z = i / (n * n);
        let p = [x as f32 * h - 1.0, y as f32 * h - 1.0, z as f32 * h - 1.0];
        let a = sphere_sdf(p, radius);
        if a.abs() > band_h {
            continue;
        }
        let mut d_min = f32::MAX;
        for q in particles {
            let dd = norm(sub(p, *q));
            if dd < d_min {
                d_min = dd;
            }
        }
        *d = d_min;
        let err = (d_min - a.abs()).abs();
        sum_err += err;
        max_err = max_err.max(err);
        cnt += 1;
    }
    (field, sum_err / cnt as f32, max_err)
}
