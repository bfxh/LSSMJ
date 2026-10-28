//! 把建造态求值出的网格转成高斯（面元采样），直接接 [`crate::gauss::codec`] 的 16 B 码。
//!
//! 这就是"草图 → 拉伸实体 → 三维资产"的最后一段：网格是精确几何，高斯是渲染资产。
//! 采样规则（确定性，不用随机数）：
//! - 每颗高斯覆盖一个面元：`πr² = 该三角分到的面积 / 颗数`，法向是三角法向，第三轴压薄 1/10；
//! - 三角按面积比例分颗数（最大余数法补齐），三角内用固定低差异序列取点；
//! - 所有量都是列表顺序累加 ⇒ 同一网格必得同一组高斯。

use crate::gauss::Gaussian;

use super::eval::Mesh;

/// 面元采样：`per_area` 是每单位面积的高斯数（越大越密）。
pub fn surfels(m: &Mesh, per_area: f32, color: [f32; 3]) -> Result<Vec<Gaussian>, String> {
    if !(per_area.is_finite() && per_area > 0.0) {
        return Err(format!("per_area 必须 >0 且有限，实得 {per_area}"));
    }
    let mut areas = Vec::with_capacity(m.tris.len());
    let mut total = 0f32;
    for t in &m.tris {
        let a = tri_area(m, *t);
        areas.push(a);
        total += a;
    }
    if areas.is_empty() || total <= 0.0 {
        return Err("网格没有面积（空或退化）".into());
    }
    let want = ((total * per_area).round() as usize).max(1);
    let counts = allocate(&areas, total, want);

    let mut out = Vec::with_capacity(want);
    for (ti, t) in m.tris.iter().enumerate() {
        let n_i = counts[ti];
        if n_i == 0 {
            continue;
        }
        let (p0, p1, p2) = (
            m.verts[t[0] as usize],
            m.verts[t[1] as usize],
            m.verts[t[2] as usize],
        );
        let nrm = normalize3(cross3(sub3(p1, p0), sub3(p2, p0)));
        let r = (areas[ti] / n_i as f32 / std::f32::consts::PI).sqrt();
        // 面元局部半径 r 决定平面内尺度，法向压到 1/10（约等于一片薄片）
        let scale = [r, r, r * 0.1];
        let rot = quat_from_z(nrm);
        for j in 0..n_i {
            let (u, v) = low_discrepancy(j);
            let w0 = 1.0 - u - v;
            let p = [
                p0[0] * w0 + p1[0] * u + p2[0] * v,
                p0[1] * w0 + p1[1] * u + p2[1] * v,
                p0[2] * w0 + p1[2] * u + p2[2] * v,
            ];
            out.push(Gaussian {
                mean: p,
                scale,
                rot,
                opacity: 1.0,
                color,
                normal: Some(nrm),
            });
        }
    }
    Ok(out)
}

fn tri_area(m: &Mesh, t: [u32; 3]) -> f32 {
    let (a, b, c) = (
        m.verts[t[0] as usize],
        m.verts[t[1] as usize],
        m.verts[t[2] as usize],
    );
    0.5 * norm3(cross3(sub3(b, a), sub3(c, a)))
}

/// 最大余数法：先按比例取整，余数按小数部分从大到小补一（同分按三角下标，保证确定性）。
fn allocate(areas: &[f32], total: f32, want: usize) -> Vec<usize> {
    let mut counts: Vec<usize> = areas
        .iter()
        .map(|a| ((a / total) * want as f32).floor() as usize)
        .collect();
    let mut rest = want.saturating_sub(counts.iter().sum::<usize>());
    let mut frac: Vec<(usize, f32)> = areas
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let exact = (a / total) * want as f32;
            (i, exact - exact.floor())
        })
        .collect();
    frac.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut k = 0;
    while rest > 0 {
        counts[frac[k % frac.len()].0] += 1;
        rest -= 1;
        k += 1;
    }
    counts
}

/// 三角内的确定性低差异点（u+v ≤ 1）。
fn low_discrepancy(j: usize) -> (f32, f32) {
    let g = 0.618_034;
    let u = (j as f32 + 0.5) * g;
    let u = u - u.floor();
    let v = (j as f32 + 0.5) * g * g;
    let v = v - v.floor();
    if u + v > 1.0 {
        (1.0 - u, 1.0 - v)
    } else {
        (u, v)
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

fn norm3(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn normalize3(a: [f32; 3]) -> [f32; 3] {
    let n = norm3(a);
    if n > 0.0 {
        [a[0] / n, a[1] / n, a[2] / n]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// 把 +Z 转到 `n` 的四元数 `[w, x, y, z]`。
pub fn quat_from_z(n: [f32; 3]) -> [f32; 4] {
    let z = [0.0, 0.0, 1.0];
    let d = z[0] * n[0] + z[1] * n[1] + z[2] * n[2];
    if d < -1.0 + 1e-6 {
        return [0.0, 1.0, 0.0, 0.0]; // 反向：绕 X 轴 180°
    }
    let c = cross3(z, n);
    let q = [1.0 + d, c[0], c[1], c[2]];
    let nn = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / nn, q[1] / nn, q[2] / nn, q[3] / nn]
}

#[cfg(test)]
mod tests {
    use super::super::eval::extrude;
    use super::super::ir::{Contour, Plane};
    use super::*;
    use crate::gauss::codec;

    fn square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    fn dist_to_tri(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
        // 点到三角所在平面的距离（这些采样点都在面内，平面距离就够了）
        let n = normalize3(cross3(sub3(b, a), sub3(c, a)));
        (sub3(p, a)[0] * n[0] + sub3(p, a)[1] * n[1] + sub3(p, a)[2] * n[2]).abs()
    }

    #[test]
    fn sample_count_and_coverage_follow_area() {
        let m = extrude(&Contour(square()), Plane::Xy, 0.5).unwrap();
        // 表面积 = 2 个盖 + 4 个侧面 = 2*1 + 4*0.5 = 4
        let gs = surfels(&m, 250.0, [0.5, 0.5, 0.5]).unwrap();
        assert_eq!(gs.len(), 1000, "面积 4 × 250/单位面积 = 1000");
        let covered: f32 = gs
            .iter()
            .map(|g| std::f32::consts::PI * g.scale[0] * g.scale[1])
            .sum();
        let rel = (covered / 4.0 - 1.0).abs();
        assert!(rel < 0.01, "面元总面积应≈网格面积，相对偏差 {rel}");
    }

    #[test]
    fn every_sample_lies_on_the_surface() {
        let m = extrude(&Contour(square()), Plane::Xy, 0.5).unwrap();
        let gs = surfels(&m, 100.0, [0.0, 0.0, 0.0]).unwrap();
        for g in &gs {
            // 采样点必须落在某一三角的平面上
            let d = m
                .tris
                .iter()
                .map(|t| {
                    dist_to_tri(
                        g.mean,
                        m.verts[t[0] as usize],
                        m.verts[t[1] as usize],
                        m.verts[t[2] as usize],
                    )
                })
                .fold(f32::INFINITY, f32::min);
            assert!(d < 1e-6, "采样点离面 {d}");
            assert!(g.is_finite());
            assert!((g.opacity - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn counts_split_by_triangle_area() {
        // 拉伸体的侧面与盖面面积不同：分配必须按面积，否则小面会被过度采样
        let m = extrude(&Contour(square()), Plane::Xy, 2.0).unwrap();
        let areas: Vec<f32> = m.tris.iter().map(|t| tri_area(&m, *t)).collect();
        let total: f32 = areas.iter().sum();
        let c = allocate(&areas, total, 1200);
        assert_eq!(c.iter().sum::<usize>(), 1200);
        for (a, n) in areas.iter().zip(&c) {
            let expect = (a / total) * 1200.0;
            assert!((*n as f32 - expect).abs() <= 1.0 + 1e-4, "面积 {a} 分到 {n}，应≈{expect}");
        }
    }

    #[test]
    fn deterministic() {
        let m = extrude(&Contour(square()), Plane::Xy, 0.3).unwrap();
        let a = surfels(&m, 64.0, [1.0, 0.0, 0.0]).unwrap();
        let b = surfels(&m, 64.0, [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn packs_into_sixteen_bytes_each() {
        let m = extrude(&Contour(square()), Plane::Xy, 0.5).unwrap();
        let gs = surfels(&m, 32.0, [0.2, 0.4, 0.6]).unwrap();
        let (buf, st) = codec::pack(&gs);
        assert_eq!(st.bytes, codec::bytes_for(gs.len()));
        assert_eq!(st.bytes, gs.len() * codec::BYTES_PER_GAUSS + (gs.len().div_ceil(codec::CHUNK)) * codec::CHUNK_HEADER);
        let back = codec::unpack(&buf).unwrap();
        assert_eq!(back.len(), gs.len());
        // 回读后法向仍与原法向同向（旋转经 10 bit 量化）
        for (a, b) in gs.iter().zip(&back) {
            assert!(a.normal.is_some() && b.normal.is_none(), "码不含法线，回读为 None");
            let ca = a.covariance();
            let cb = b.covariance();
            let num: f32 = (0..3).map(|i| (0..3).map(|j| (ca[i][j] - cb[i][j]).powi(2)).sum::<f32>()).sum();
            let den: f32 = (0..3).map(|i| (0..3).map(|j| ca[i][j] * ca[i][j]).sum::<f32>()).sum();
            assert!((num / den.max(1e-20)).sqrt() < 0.2, "回读协方差偏差过大");
        }
    }
}
