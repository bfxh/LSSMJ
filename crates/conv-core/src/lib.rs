//! LSSMJ 表示转换核心（第十四轮 G14 → 任务轨 16 / 判据 C22）。
//!
//! 本 crate 是 `scratch/conv-proto` 原型的迁入版（判据先行的 CPU 参照）+
//! GPU 积木第一片（JFA 距离场，`jfa` 模块）。判据对应关系：
//! - J2 mesh→SDF 解析对拍：`tests/cpu_legs.rs::mesh_sdf_matches_analytic_band`
//! - J3a 体积守恒 / J3b 亏格 / J3c 逐位确定性：同文件三个 test
//! - J4 粒子→无符号场（naive 基线）：`particles_unsigned_field_matches_analytic`
//! - GPU JFA vs CPU 暴力参照 + 确定性：`tests/jfa_gpu.rs`

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
pub mod surfel;
pub mod timer;

/// 域边长（index 坐标 [0, N)），网格间距 1；世界坐标由调用方缩放。
pub const GRID: u32 = 64;

/// 确定性 LCG（判据禁用不确定源）。
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next01(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (u32::MAX as f32)
    }
}

/// 解析球 SDF（世界坐标，球心在原点）。
pub fn sphere_sdf(p: [f32; 3], radius: f32) -> f32 {
    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - radius
}

/// 解析场 → 体素网格（域 [-1,1]³，N³）。
pub fn field_to_voxels(n: u32, radius: f32) -> Vec<f32> {
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

/// 三角网 → SDF（窄带 ±band_h；暴力点-三角 + 径向符号）。
pub fn mesh_to_sdf_band(
    n: u32,
    radius: f32,
    band_h: f32,
    verts: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> (Vec<f32>, f32) {
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
        *d = d_min * radial_sign(p, verts, faces);
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
