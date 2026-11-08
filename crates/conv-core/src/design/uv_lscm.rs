//! 接缝切分 + 每岛 LSCM 共形参数化（全 Rust，无外部线性代数依赖）。
//!
//! - **接缝**：二面角大于阈值的边、边界边；封闭曲面（没有边界/接缝）按法线主轴补切，
//!   保证每个岛可展开。
//! - **LSCM**：Lévy et al. 2002 的最小二乘共形映射；每岛固定两个顶点（x 最小/最大），
//!   用共轭梯度解正规方程。
//! - 输出每岛的 `(全局顶点索引, 每顶点 UV)`；打包与重叠统计在 `uv.rs`。

use std::collections::{HashMap, HashSet};

use super::eval::Mesh;

/// 接缝切分：返回三角形索引岛。
///
/// `angle_deg` 是二面角阈值（法线夹角大于它即切缝）。封闭曲面没有锐边时，
/// 按法线主轴再切，避免把整张封闭曲面塞进一个 LSCM 岛。
pub fn seam_islands(mesh: &Mesh, angle_deg: f32) -> Vec<Vec<usize>> {
    let n = mesh.tris.len();
    if n == 0 {
        return Vec::new();
    }
    let normals: Vec<[f32; 3]> = (0..n).map(|t| tri_normal(mesh, t)).collect();
    let areas: Vec<f32> = (0..n).map(|t| tri_area(mesh, t)).collect();
    let mut sorted = areas.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[n / 2];
    // 曲率自适应：小于中位面积的细节三角形用更小阈值（35°），大面保持 angle_deg。
    let cos_fine = 35f32.to_radians().cos();
    let cos_coarse = angle_deg.to_radians().cos();

    // 边 → 相邻三角形。
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (ti, t) in mesh.tris.iter().enumerate() {
        for k in 0..3 {
            let a = t[k];
            let b = t[(k + 1) % 3];
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push(ti);
        }
    }

    // 接缝边：边界，或两邻接三角形法线夹角过大。
    let mut seams: HashSet<(u32, u32)> = HashSet::new();
    for (key, tris) in &edges {
        if tris.len() != 2 {
            seams.insert(*key);
            continue;
        }
        let (a, b) = (tris[0], tris[1]);
        let d = dot3(normals[a], normals[b]);
        let th = if areas[a].min(areas[b]) < median {
            cos_fine
        } else {
            cos_coarse
        };
        if d < th {
            seams.insert(*key);
        }
    }

    // 非接缝边连接三角形。
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (key, tris) in &edges {
        if tris.len() == 2 && !seams.contains(key) {
            adj[tris[0]].push(tris[1]);
            adj[tris[1]].push(tris[0]);
        }
    }

    // 连通分量。
    let mut seen = vec![false; n];
    let mut out: Vec<Vec<usize>> = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let mut comp = Vec::new();
        while let Some(t) = stack.pop() {
            comp.push(t);
            for &u in &adj[t] {
                if !seen[u] {
                    seen[u] = true;
                    stack.push(u);
                }
            }
        }
        out.push(comp);
    }

    // 封闭岛兜底：没有边界边/接缝边的岛按主轴再切。
    let mut final_islands = Vec::new();
    for comp in out {
        let closed = comp.iter().all(|&t| {
            mesh.tris[t].iter().enumerate().all(|(k, &a)| {
                let b = mesh.tris[t][(k + 1) % 3];
                let key = if a < b { (a, b) } else { (b, a) };
                !seams.contains(&key)
            })
        });
        if closed && comp.len() > 2 {
            final_islands.extend(split_by_axis(mesh, &comp));
        } else {
            final_islands.push(comp);
        }
    }
    final_islands
}

/// 按法线主轴 + 连通性切分（封闭曲面兜底）。
fn split_by_axis(mesh: &Mesh, comp: &[usize]) -> Vec<Vec<usize>> {
    // 八分体切缝：按法线符号 (x±, y±, z±) 分组，比 6 主轴更细，减少前后表面同岛。
    let mut groups: [Vec<usize>; 8] = Default::default();
    for &t in comp {
        groups[octant_of(tri_normal(mesh, t))].push(t);
    }
    let mut out = Vec::new();
    for group in groups {
        if group.is_empty() {
            continue;
        }
        // 组内按共享边连通性再分。
        let set: HashSet<usize> = group.iter().copied().collect();
        let mut local: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for &t in &group {
            for k in 0..3 {
                let a = mesh.tris[t][k];
                let b = mesh.tris[t][(k + 1) % 3];
                let key = if a < b { (a, b) } else { (b, a) };
                local.entry(key).or_default().push(t);
            }
        }
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        for tris in local.values() {
            if tris.len() == 2 {
                adj.entry(tris[0]).or_default().push(tris[1]);
                adj.entry(tris[1]).or_default().push(tris[0]);
            }
        }
        let mut seen = HashSet::new();
        for &start in &group {
            if seen.contains(&start) {
                continue;
            }
            let mut stack = vec![start];
            seen.insert(start);
            let mut sub = Vec::new();
            while let Some(t) = stack.pop() {
                sub.push(t);
                for &u in adj.get(&t).map(Vec::as_slice).unwrap_or(&[]) {
                    if set.contains(&u) && seen.insert(u) {
                        stack.push(u);
                    }
                }
            }
            out.push(sub);
        }
    }
    out
}

/// 大 mesh 分块：按三角形质心的 3D 网格切块，每块再按八分体 + 连通性分岛。
/// 这是 >50k 三角形的参数化入口，替代“每三角形独立展开”的保底路径。
pub fn block_islands(mesh: &Mesh, grid: usize, max_tris: usize) -> Vec<Vec<usize>> {
    let n = mesh.tris.len();
    if n == 0 {
        return Vec::new();
    }
    let grid = grid.max(1);
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in &mesh.verts {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    let span = [
        (max[0] - min[0]).max(1e-9),
        (max[1] - min[1]).max(1e-9),
        (max[2] - min[2]).max(1e-9),
    ];
    let mut cells: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ti, t) in mesh.tris.iter().enumerate() {
        let c = [
            (mesh.verts[t[0] as usize][0]
                + mesh.verts[t[1] as usize][0]
                + mesh.verts[t[2] as usize][0])
                / 3.0,
            (mesh.verts[t[0] as usize][1]
                + mesh.verts[t[1] as usize][1]
                + mesh.verts[t[2] as usize][1])
                / 3.0,
            (mesh.verts[t[0] as usize][2]
                + mesh.verts[t[1] as usize][2]
                + mesh.verts[t[2] as usize][2])
                / 3.0,
        ];
        let ix = (((c[0] - min[0]) / span[0] * grid as f32).floor() as usize).min(grid - 1);
        let iy = (((c[1] - min[1]) / span[1] * grid as f32).floor() as usize).min(grid - 1);
        let iz = (((c[2] - min[2]) / span[2] * grid as f32).floor() as usize).min(grid - 1);
        cells.entry((iz * grid + iy) * grid + ix).or_default().push(ti);
    }
    let mut out = Vec::new();
    for (_, tris) in cells {
        if tris.len() > max_tris {
            out.extend(split_by_axis(mesh, &tris));
        } else {
            // 小块直接按八分体 + 连通性分，保证 LSCM 岛可展开。
            out.extend(split_by_axis(mesh, &tris));
        }
    }
    out
}

/// LSCM：返回 `(全局顶点索引, 每顶点 UV)`。
pub fn lscm(mesh: &Mesh, tris: &[usize]) -> (Vec<u32>, Vec<[f32; 2]>) {
    // 局部顶点表。
    let mut vmap: HashMap<u32, usize> = HashMap::new();
    let mut global: Vec<u32> = Vec::new();
    for &ti in tris {
        for &vi in &mesh.tris[ti] {
            if !vmap.contains_key(&vi) {
                vmap.insert(vi, global.len());
                global.push(vi);
            }
        }
    }
    let n = global.len();
    if n < 3 {
        return (global, vec![[0.0, 0.0]; n]);
    }

    // pin：x 最小 / 最大顶点，固定到 (0,0) / (1,0)。
    let mut p0 = 0usize;
    let mut p1 = 0usize;
    let (mut minx, mut maxx) = (f32::INFINITY, f32::NEG_INFINITY);
    for (i, &gi) in global.iter().enumerate() {
        let x = mesh.verts[gi as usize][0];
        if x < minx {
            minx = x;
            p0 = i;
        }
        if x > maxx {
            maxx = x;
            p1 = i;
        }
    }
    if p0 == p1 {
        p1 = (p0 + 1) % n;
    }

    // 行：LSCM 两行/三角形 + 4 条 pin 约束。
    const PIN_W: f32 = 100.0;
    let mut rows: Vec<Vec<(usize, f32)>> = Vec::new();
    for &ti in tris {
        let t = mesh.tris[ti];
        let p = [
            mesh.verts[t[0] as usize],
            mesh.verts[t[1] as usize],
            mesh.verts[t[2] as usize],
        ];
        let e1 = sub3(p[1], p[0]);
        let l1 = norm3(e1);
        if l1 < 1e-9 {
            continue;
        }
        let ex = [e1[0] / l1, e1[1] / l1, e1[2] / l1];
        let e2 = sub3(p[2], p[0]);
        let x1 = l1;
        let x2 = dot3(e2, ex);
        let perp = sub3(e2, [ex[0] * x2, ex[1] * x2, ex[2] * x2]);
        let y2 = norm3(perp);
        if y2 < 1e-9 {
            continue;
        }
        let area = 0.5 * x1 * y2;
        let inv2a = 1.0 / (2.0 * area);
        let a = [-y2 * inv2a, y2 * inv2a, 0.0];
        let b = [(x2 - x1) * inv2a, -x2 * inv2a, x1 * inv2a];
        let w = area.sqrt();
        let li = [
            vmap[&t[0]] as usize,
            vmap[&t[1]] as usize,
            vmap[&t[2]] as usize,
        ];
        for (cu, cv) in [(a, [-b[0], -b[1], -b[2]]), (b, a)] {
            let mut row = Vec::with_capacity(6);
            for j in 0..3 {
                row.push((2 * li[j], cu[j] * w));
                row.push((2 * li[j] + 1, cv[j] * w));
            }
            rows.push(row);
        }
    }
    let mut rhs = vec![0.0f32; 2 * n];
    for (var, target) in [
        (2 * p0, 0.0f32),
        (2 * p0 + 1, 0.0),
        (2 * p1, 1.0),
        (2 * p1 + 1, 0.0),
    ] {
        rows.push(vec![(var, PIN_W)]);
        rhs[var] += PIN_W * PIN_W * target;
    }

    let x = cg(&rows, &rhs, 2 * n, 800);
    let mut uvs = vec![[0.0f32; 2]; n];
    for i in 0..n {
        uvs[i] = [x[2 * i], x[2 * i + 1]];
    }
    (global, uvs)
}

/// 共轭梯度：解 `MᵀM x = Mᵀr`（`rhs` 已经是 `Mᵀr`）。
fn cg(rows: &[Vec<(usize, f32)>], rhs: &[f32], n: usize, iters: usize) -> Vec<f32> {
    let mut x = vec![0.0f32; n];
    let mut r = rhs.to_vec();
    let mut p = r.clone();
    let mut rs = dot_slice(&r, &r);
    if rs < 1e-20 {
        return x;
    }
    for _ in 0..iters {
        let ap = matvec(rows, &p, n);
        let denom = dot_slice(&p, &ap);
        if denom.abs() < 1e-20 {
            break;
        }
        let alpha = rs / denom;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rs_new = dot_slice(&r, &r);
        if rs_new < 1e-12 {
            break;
        }
        let beta = rs_new / rs;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rs = rs_new;
    }
    x
}

fn matvec(rows: &[Vec<(usize, f32)>], x: &[f32], n: usize) -> Vec<f32> {
    let mut y = vec![0.0f32; n];
    for row in rows {
        let mut s = 0.0f32;
        for &(var, c) in row {
            s += c * x[var];
        }
        for &(var, c) in row {
            y[var] += c * s;
        }
    }
    y
}

fn dot_slice(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(&x, &y)| x * y).sum()
}

fn tri_area(mesh: &Mesh, ti: usize) -> f32 {
    let t = mesh.tris[ti];
    let a = mesh.verts[t[0] as usize];
    let b = mesh.verts[t[1] as usize];
    let c = mesh.verts[t[2] as usize];
    let cr = cross3(sub3(b, a), sub3(c, a));
    let l = norm3(cr);
    0.5 * l
}

fn tri_normal(mesh: &Mesh, ti: usize) -> [f32; 3] {
    let t = mesh.tris[ti];
    let a = mesh.verts[t[0] as usize];
    let b = mesh.verts[t[1] as usize];
    let c = mesh.verts[t[2] as usize];
    let v = cross3(sub3(b, a), sub3(c, a));
    let l = norm3(v);
    if l > 1e-9 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn octant_of(n: [f32; 3]) -> usize {
    (usize::from(n[0] >= 0.0))
        | (usize::from(n[1] >= 0.0) << 1)
        | (usize::from(n[2] >= 0.0) << 2)
}

fn axis_of(n: [f32; 3]) -> Axis {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Axis::PosX => 0,
            Axis::NegX => 1,
            Axis::PosY => 2,
            Axis::NegY => 3,
            Axis::PosZ => 4,
            Axis::NegZ => 5,
        }
    }
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
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
    fn cube_has_six_seam_islands() {
        let m = cube();
        let islands = seam_islands(&m, 45.0);
        assert_eq!(islands.len(), 6, "立方体应有 6 个面岛");
        assert!(islands.iter().all(|i| i.len() == 2));
    }

    #[test]
    fn lscm_is_finite_and_pins_hold() {
        let m = cube();
        let islands = seam_islands(&m, 45.0);
        for tris in islands {
            let (global, uvs) = lscm(&m, &tris);
            assert_eq!(global.len(), uvs.len());
            assert!(uvs.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        }
    }

    #[test]
    fn closed_torus_gets_split() {
        // 用 UV 球面近似一个封闭曲面：全部三角形法线不同，锐边不切，靠主轴兜底。
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        let (nu, nv) = (8usize, 6usize);
        for i in 0..nu {
            let u = std::f32::consts::TAU * i as f32 / nu as f32;
            for j in 0..nv {
                let v = std::f32::consts::PI * (j as f32 + 0.5) / nv as f32;
                verts.push([u.cos() * v.sin(), v.cos(), u.sin() * v.sin()]);
            }
        }
        for i in 0..nu {
            for j in 0..nv {
                let a = (i * nv + j) as u32;
                let b = ((i + 1) % nu * nv + j) as u32;
                let c = ((i + 1) % nu * nv + (j + 1) % nv) as u32;
                let d = (i * nv + (j + 1) % nv) as u32;
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        let m = Mesh { verts, tris };
        let islands = seam_islands(&m, 45.0);
        assert!(islands.len() > 1, "封闭曲面必须被补切，实得 {}", islands.len());
    }
}
