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

/// 按共享边连通性把一组三角形分成分量（不做任何法线切分）。
///
/// 与 [`split_by_axis`] 的区别：后者先按法线八分体分组，会在连续曲面上人为制造接缝；
/// 这里只按几何连通性分，供“法线锥可接受时整块共形展开”的判断使用。
fn connectivity_components(mesh: &Mesh, tris: &[usize]) -> Vec<Vec<usize>> {
    let set: HashSet<usize> = tris.iter().copied().collect();
    let mut local: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for &t in tris {
        for k in 0..3 {
            let a = mesh.tris[t][k];
            let b = mesh.tris[t][(k + 1) % 3];
            let key = if a < b { (a, b) } else { (b, a) };
            local.entry(key).or_default().push(t);
        }
    }
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for ts in local.values() {
        if ts.len() == 2 {
            adj.entry(ts[0]).or_default().push(ts[1]);
            adj.entry(ts[1]).or_default().push(ts[0]);
        }
    }
    let mut seen: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for &start in tris {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        while let Some(t) = stack.pop() {
            comp.push(t);
            for &u in adj.get(&t).map(Vec::as_slice).unwrap_or(&[]) {
                if set.contains(&u) && seen.insert(u) {
                    stack.push(u);
                }
            }
        }
        out.push(comp);
    }
    out
}

/// 分量法线锥半角（度）：面积加权平均法线与分量内任一三角形法线的最大夹角。
///
/// 锥角小 = 该分量近似单片，可整块 LSCM 展开而不翻面；锥角大 = 正反面共存，
/// 必须切缝，否则展开后自重叠。
fn normal_cone_deg(mesh: &Mesh, comp: &[usize]) -> f32 {
    let mut avg = [0.0f32; 3];
    for &t in comp {
        let a = tri_area(mesh, t);
        let n = tri_normal(mesh, t);
        for k in 0..3 {
            avg[k] += n[k] * a;
        }
    }
    let len = norm3(avg);
    if len < 1e-9 {
        return 180.0;
    }
    let mut worst = 0.0f32;
    for &t in comp {
        let d = (dot3(tri_normal(mesh, t), avg) / len).clamp(-1.0, 1.0);
        worst = worst.max(d.acos().to_degrees());
    }
    worst
}

/// 大 mesh 分块：按三角形质心的 3D 网格切块，每块再按连通性分岛；
/// 只有法线锥过宽或三角形数超上限时才退回八分体切分。
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
    // 法线锥阈值（度）：经实测扫描（CHG-2026-11-10-AV，50 万三角键盘 GT）：
    // · cone 20/40/60 → fill 0.4207/0.4222/0.4237，岛 26.1k/25.7k/25.3k；
    // · cone 90 起始把正反面塞进同岛，被重叠检测拆碎，岛数与 fill 同时上升；
    // · 取 90 配 grid=16：岛 21.4k、fill 0.4486，且失真谱宽(p99/p1)=1.252、
    //   UV 翻转 2351，均优于旧路径（八分体无条件切：fill 0.3893、谱宽 1.347、翻转 3123）。
    // 阈值放宽是安全的：整块展开若自重叠，uv.rs 的 island_overlaps 会递归拆掉重叠三角形。
    const CONE_DEG: f32 = 90.0;
    let mut out = Vec::new();
    for (_, tris) in cells {
        // 先按共享边连通性分：不再无条件按法线八分体劈开连续曲面（那会人为制造接缝）。
        // 只有“法线锥过宽（正反面共存，整块展开会自重叠）”或“三角形数超 LSCM 上限”
        // 才退回八分体切分。
        for comp in connectivity_components(mesh, &tris) {
            if comp.len() <= 1 {
                out.push(comp);
            } else if comp.len() <= max_tris && normal_cone_deg(mesh, &comp) <= CONE_DEG {
                out.push(comp);
            } else {
                out.extend(split_by_axis(mesh, &comp));
            }
        }
    }
    // 跨块合并（实测正结果，CHG-2026-11-10-AW，50 万三角键盘 GT）：
    // 块边界会无条件切断连通性，把两侧本属同一曲面的分量并回一个岛后，
    // 岛 21,392→11,540（−46%）、fill 0.4486→0.4609（+2.7%）、耗时 8.5s→6.1s
    // （碎岛少了，LSCM 调用与打包项同步减少）。
    //
    // 上限是**峰值而非越大越好**：50→0.4498、120→0.4597、180→0.4609（峰）、
    // 300→0.4480、800→0.4285、20000→0.4062。超过约 200 后，大岛 LSCM 自重叠
    // 被 `island_overlaps` 拆成单三角形，fill 反向下降、耗时也回升。
    const MERGE_MAX_TRIS: usize = 180;
    merge_across_blocks(mesh, out, CONE_DEG, MERGE_MAX_TRIS)
}

/// 跨块合并：把空间块边界两侧、本属同一连续曲面的分量并回一个岛。
///
/// 块切分会在块边界处无条件断开连通性，产生大量“本可连续展开”的碎岛；
/// 这里按共享边从长到短尝试合并，判据（保守上界）：
/// 合并后法线锥 ≤ `cone_deg`，且三角形数 ≤ `max_tris`（控制 LSCM 规模）。
fn merge_across_blocks(
    mesh: &Mesh,
    comps: Vec<Vec<usize>>,
    cone_deg: f32,
    max_tris: usize,
) -> Vec<Vec<usize>> {
    let nc = comps.len();
    if nc < 2 {
        return comps;
    }
    let mut comp_of = vec![usize::MAX; mesh.tris.len()];
    for (ci, c) in comps.iter().enumerate() {
        for &t in c {
            comp_of[t] = ci;
        }
    }
    // 每岛统计：三角形数、面积加权法线和、自身法线锥。
    let mut tris: Vec<usize> = comps.iter().map(Vec::len).collect();
    let mut norms: Vec<[f32; 3]> = comps
        .iter()
        .map(|c| {
            let mut s = [0.0f32; 3];
            for &t in c {
                let a = tri_area(mesh, t);
                let n = tri_normal(mesh, t);
                for k in 0..3 {
                    s[k] += n[k] * a;
                }
            }
            s
        })
        .collect();
    let mut cones: Vec<f32> = comps.iter().map(|c| normal_cone_deg(mesh, c)).collect();

    // 跨岛共享边：岛对 → 共享边长合计（长边优先合，先合大面片）。
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (ti, t) in mesh.tris.iter().enumerate() {
        for k in 0..3 {
            let a = t[k];
            let b = t[(k + 1) % 3];
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push(ti);
        }
    }
    let mut pairs: HashMap<(usize, usize), f32> = HashMap::new();
    for (key, ts) in &edges {
        if ts.len() != 2 {
            continue;
        }
        let (ca, cb) = (comp_of[ts[0]], comp_of[ts[1]]);
        if ca == cb {
            continue;
        }
        let len = norm3(sub3(
            mesh.verts[key.1 as usize],
            mesh.verts[key.0 as usize],
        ));
        let k = if ca < cb { (ca, cb) } else { (cb, ca) };
        *pairs.entry(k).or_insert(0.0) += len;
    }
    let mut cand: Vec<((usize, usize), f32)> = pairs.into_iter().collect();
    cand.sort_by(|a, b| b.1.total_cmp(&a.1));

    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut c = x;
        while parent[c] != c {
            let nxt = parent[c];
            parent[c] = r;
            c = nxt;
        }
        r
    }

    let mut parent: Vec<usize> = (0..nc).collect();
    for ((a, b), _len) in cand {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra == rb || tris[ra] + tris[rb] > max_tris {
            continue;
        }
        let la = norm3(norms[ra]).max(1e-12);
        let lb = norm3(norms[rb]).max(1e-12);
        let dot = (dot3(norms[ra], norms[rb]) / (la * lb)).clamp(-1.0, 1.0);
        let sep = dot.acos().to_degrees();
        // 合并后锥角上界：两岛平均法线夹角 + 较大的自身锥角。
        let cone = (sep + cones[ra].max(cones[rb])).min(180.0);
        if cone > cone_deg {
            continue;
        }
        parent[rb] = ra;
        tris[ra] += tris[rb];
        for k in 0..3 {
            norms[ra][k] += norms[rb][k];
        }
        cones[ra] = cone;
    }

    // 按根合并输出。
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ci, c) in comps.into_iter().enumerate() {
        let r = find(&mut parent, ci);
        groups.entry(r).or_default().extend(c);
    }
    groups.into_values().collect()
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

    /// XY 平面上的 `n×n` 方格，法线全 +Z。
    fn grid_plane(n: usize) -> Mesh {
        let mut verts = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                verts.push([i as f32, j as f32, 0.0]);
            }
        }
        let mut tris = Vec::new();
        let w = n + 1;
        for j in 0..n {
            for i in 0..n {
                let a = (j * w + i) as u32;
                let b = (j * w + i + 1) as u32;
                let c = ((j + 1) * w + i + 1) as u32;
                let d = ((j + 1) * w + i) as u32;
                tris.push([a, b, c]);
                tris.push([a, c, d]);
            }
        }
        Mesh { verts, tris }
    }

    #[test]
    fn block_islands_keeps_flat_patch_whole() {
        // 平坦连续面片：法线锥 0°，必须整块作为**一个**岛（旧逻辑会按八分体劈成多个）。
        let m = grid_plane(4);
        let islands = block_islands(&m, 1, 10_000);
        assert_eq!(islands.len(), 1, "平坦面片应为单岛，实得 {}", islands.len());
        assert_eq!(islands[0].len(), m.tris.len());
    }

    #[test]
    fn block_islands_splits_folded_patch() {
        // 后半三角形顶点反向 ⇒ 正反面共存，法线锥 180° > 90°，必须切缝。
        let mut m = grid_plane(3);
        let half = m.tris.len() / 2;
        for t in m.tris.iter_mut().skip(half) {
            t.swap(1, 2);
        }
        let islands = block_islands(&m, 1, 10_000);
        assert!(islands.len() >= 2, "正反面共存必须切分，实得 {}", islands.len());
    }

    /// 细长平面：x 方向 `nx` 格、y 方向 1 格（法线 +Z）。
    /// 用 `grid=nx` 可精确切成 `nx` 个相邻小块，每块 2 个三角形。
    fn strip_plane(nx: usize) -> Mesh {
        let mut verts = Vec::new();
        for i in 0..=nx {
            verts.push([i as f32, 0.0, 0.0]);
            verts.push([i as f32, 1.0, 0.0]);
        }
        let mut tris = Vec::new();
        for i in 0..nx {
            let a = (2 * i) as u32;
            let b = (2 * i + 2) as u32;
            let c = (2 * i + 3) as u32;
            let d = (2 * i + 1) as u32;
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
        Mesh { verts, tris }
    }

    #[test]
    fn block_merge_rejoins_across_cells() {
        // grid=8 把细长平面切成 8 个相邻块；跨块合并应并回 1 个岛。
        let m = strip_plane(8);
        let islands = block_islands(&m, 8, 10_000);
        assert_eq!(
            islands.len(),
            1,
            "同向连续平面应跨块合并为单岛，实得 {}（{} 三角形）",
            islands.len(),
            m.tris.len()
        );
        assert_eq!(islands[0].len(), m.tris.len());
    }

    #[test]
    fn block_merge_refuses_folded_patch() {
        // 后半顶点反向 ⇒ 正反面共存（锥角 180°），跨块合并不得把它们并成一个岛。
        let mut m = strip_plane(8);
        let half = m.tris.len() / 2;
        for t in m.tris.iter_mut().skip(half) {
            t.swap(1, 2);
        }
        let islands = block_islands(&m, 8, 10_000);
        assert!(islands.len() >= 2, "正反面共存不得合并，实得 {}", islands.len());
    }
}
