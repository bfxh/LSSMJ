// GPU mesh→SDF 实时档（T-GC-01）：三角网→表面采样→JFA 种子→窄带距离场→径向符号。
// 单位口径：全程 index 单位（顶点已在 CPU 侧缩放到 [0,n)³），回读后由调用方缩放回世界单位。
// 统一绑定布局（8 项，四个 kernel 共用一张 BGL）：
//   0=verts(read,vec4) 1=faces(read,vec4u) 2=mp(uniform) 3=counts(rw)
//   4=base(read) 5=samples(rw,vec4) 6=label(rw,atomic) 7=dist(rw)

struct MeshParams {
    n_tris: u32,
    n: u32,
    density: f32,
    band: f32,
};

@group(0) @binding(0) var<storage, read> verts : array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> faces : array<vec4<u32>>;
@group(0) @binding(2) var<uniform> mp : MeshParams;
@group(0) @binding(3) var<storage, read_write> counts : array<u32>;
@group(0) @binding(4) var<storage, read> base : array<u32>;
@group(0) @binding(5) var<storage, read_write> samples : array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> label : array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> dist : array<f32>;

const INVALID : u32 = 0xFFFFFFFFu;

@compute @workgroup_size(64)
fn count_samples(@builtin(global_invocation_id) gid: vec3<u32>) {
    let t = gid.x;
    if (t >= mp.n_tris) {
        return;
    }
    let a = verts[faces[t].x].xyz;
    let b = verts[faces[t].y].xyz;
    let c = verts[faces[t].z].xyz;
    let area = 0.5 * length(cross(b - a, c - a));
    counts[t] = u32(clamp(ceil(area * mp.density), 1.0, 64.0));
}

@compute @workgroup_size(64)
fn emit_samples(@builtin(global_invocation_id) gid: vec3<u32>) {
    let t = gid.x;
    if (t >= mp.n_tris) {
        return;
    }
    let a = verts[faces[t].x].xyz;
    let b = verts[faces[t].y].xyz;
    let c = verts[faces[t].z].xyz;
    let m = counts[t];
    let b0 = base[t];
    for (var k = 0u; k < m; k++) {
        // 确定性分层采样（低差异序列 + 三角镜像），槽位来自 CPU 前缀和 ⇒ 全链确定
        let r1 = fract(0.61803399 * f32(k + 1u) + 0.12345 * f32(t));
        let r2 = fract(0.75487767 * f32(k + 1u) + 0.54321 * f32(t));
        var u = r1;
        var v = r2;
        if (u + v > 1.0) {
            u = 1.0 - u;
            v = 1.0 - v;
        }
        let pos = a + u * (b - a) + v * (c - a);
        samples[b0 + k] = vec4<f32>(pos, 1.0);
    }
}

@compute @workgroup_size(64)
fn scatter_seeds(@builtin(global_invocation_id) gid: vec3<u32>) {
    let s = gid.x;
    if (s >= base[mp.n_tris]) {
        return;
    }
    let pos = samples[s].xyz;
    let ci = vec3<i32>(floor(pos));
    let cl = clamp(ci, vec3<i32>(0), vec3<i32>(i32(mp.n) - 1));
    let idx = cl.x + cl.y * i32(mp.n) + cl.z * i32(mp.n) * i32(mp.n);
    atomicMin(&label[idx], s);
}

@compute @workgroup_size(4, 4, 4)
fn sign_radial(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = mp.n;
    if (gid.x >= n || gid.y >= n || gid.z >= n) {
        return;
    }
    let idx = gid.x + gid.y * n + gid.z * n * n;
    let d = dist[idx];
    if (d > mp.band) {
        return; // 带外不投符号（窄带口径）
    }
    let pos = vec3<f32>(gid);
    // 射线起点 = 球心（world 原点 → index (n-1)/2）。index 单位换算后"原点"语义
    // 移到了网格角 (0,0,0)（球外！）——判据首跑的符号翻转即此因。
    let center = vec3<f32>(f32(n) - 1.0) * 0.5;
    let rel = pos - center;
    let pl = length(rel);
    var inside = false;
    if (pl < 1e-6) {
        inside = true;
    } else {
        let dir = rel / pl;
        var t_exit = 0.0;
        // 逐体素 MT 射线（凸体判据口径；一般网格换 winding/树属后续片）
        for (var t = 0u; t < mp.n_tris; t++) {
            let a = verts[faces[t].x].xyz;
            let b = verts[faces[t].y].xyz;
            let c = verts[faces[t].z].xyz;
            let e1 = b - a;
            let e2 = c - a;
            let pv = cross(dir, e2);
            let det = dot(e1, pv);
            if (abs(det) < 1e-10) {
                continue;
            }
            let inv = 1.0 / det;
            let tv = center - a; // 射线起点=球心（与 CPU radial_sign 的 world 原点同语义）
            let u = dot(tv, pv) * inv;
            if (u < 0.0 || u > 1.0) {
                continue;
            }
            let qv = cross(tv, e1);
            let v = dot(dir, qv) * inv;
            if (v < 0.0 || u + v > 1.0) {
                continue;
            }
            let tt = dot(e2, qv) * inv;
            if (tt > t_exit) {
                t_exit = tt;
            }
        }
        inside = pl < t_exit;
    }
    if (inside) {
        dist[idx] = -d;
    }
}
