// GPU Surface Nets（T-GC-02）：SDF→mesh，逐语义复刻 fast-surface-nets 0.2.1：
//   顶点 = cell 内 12 条边的等值面交点平均（CUBE_CORNERS bit0=x,bit1=y,bit2=z）；
//   四边形 = 每表面 cell 的 X/Y/Z 三轴边（边界条件 y,z>0 && x<n-2 等），
//   对角线按短边拆分，negative_face 翻绕序。
// 绑定布局：0=sdf(read) 1=vtx_pos(rw,vec4) 2=vtx_flag(rw) 3=params(uniform)
//   4=quad_counter(rw,atomic) 5=idx_out(rw)
// 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"
// 排序后对拍——四边形集合与绕序确定 ⇒ 排序流逐位确定（稀疏 compaction 属后续片）。

struct P {
    n: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> sdf : array<f32>;
@group(0) @binding(1) var<storage, read_write> vtx_pos : array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> vtx_flag : array<u32>;
@group(0) @binding(3) var<uniform> pp : P;
@group(0) @binding(4) var<storage, read_write> quad_counter : array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> idx_out : array<u32>;

fn corner_sdf(cell: vec3<u32>, corner: u32) -> f32 {
    let o = vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
    let g = cell + o;
    return sdf[g.x + g.y * pp.n + g.z * pp.n * pp.n];
}

fn corner_vec(corner: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u));
}

const EDGES : array<vec2<u32>, 12> = array<vec2<u32>, 12>(
    vec2<u32>(0u, 1u), vec2<u32>(0u, 2u), vec2<u32>(0u, 4u),
    vec2<u32>(1u, 3u), vec2<u32>(1u, 5u),
    vec2<u32>(2u, 3u), vec2<u32>(2u, 6u),
    vec2<u32>(3u, 7u),
    vec2<u32>(4u, 5u), vec2<u32>(4u, 6u),
    vec2<u32>(5u, 7u),
    vec2<u32>(6u, 7u),
);

fn sign_diff(a: f32, b: f32) -> bool {
    return (a < 0.0) != (b < 0.0);
}

@compute @workgroup_size(4, 4, 4)
fn gen_vertices(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = pp.n;
    if (gid.x >= n - 1u || gid.y >= n - 1u || gid.z >= n - 1u) {
        return;
    }
    let cell = gid;
    var d : array<f32, 8>;
    var neg = 0u;
    for (var i = 0u; i < 8u; i++) {
        let v = corner_sdf(cell, i);
        d[i] = v;
        if (v < 0.0) {
            neg = neg + 1u;
        }
    }
    let lin = cell.x + cell.y * n + cell.z * n * n;
    if (neg == 0u || neg == 8u) {
        vtx_flag[lin] = 0u;
        return;
    }
    var sum = vec3<f32>(0.0, 0.0, 0.0);
    var cnt = 0.0;
    for (var e = 0u; e < 12u; e++) {
        let c1 = EDGES[e].x;
        let c2 = EDGES[e].y;
        let d1 = d[c1];
        let d2 = d[c2];
        if ((d1 < 0.0) != (d2 < 0.0)) {
            let w = d1 / (d1 - d2);
            sum = sum + (1.0 - w) * corner_vec(c1) + w * corner_vec(c2);
            cnt = cnt + 1.0;
        }
    }
    vtx_pos[lin] = vec4<f32>(vec3<f32>(cell) + sum / cnt, 1.0);
    vtx_flag[lin] = 1u;
}

fn write_quad(slot: u32, p1: u32, sb: u32, sc: u32, d1: f32, d2: f32) {
    let neg_face = (d2 < 0.0) && !(d1 < 0.0);
    let v1 = p1;
    let v2 = p1 - sb;
    let v3 = p1 - sc;
    let v4 = p1 - sb - sc;
    let pos1 = vtx_pos[v1].xyz;
    let pos2 = vtx_pos[v2].xyz;
    let pos3 = vtx_pos[v3].xyz;
    let pos4 = vtx_pos[v4].xyz;
    // 对角线沿短边拆分（与 CPU 同判据），negative_face 翻绕序
    var quad : array<u32, 6>;
    if (dot(pos1 - pos4, pos1 - pos4) < dot(pos2 - pos3, pos2 - pos3)) {
        if (neg_face) {
            quad = array<u32, 6>(v1, v4, v2, v1, v3, v4);
        } else {
            quad = array<u32, 6>(v1, v2, v4, v1, v4, v3);
        }
    } else if (neg_face) {
        quad = array<u32, 6>(v2, v3, v4, v2, v1, v3);
    } else {
        quad = array<u32, 6>(v2, v4, v3, v2, v3, v1);
    }
    for (var k = 0u; k < 6u; k++) {
        idx_out[slot + k] = quad[k];
    }
}

@compute @workgroup_size(4, 4, 4)
fn emit_quads(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = pp.n;
    if (gid.x >= n - 1u || gid.y >= n - 1u || gid.z >= n - 1u) {
        return;
    }
    let cell = gid;
    let lin = cell.x + cell.y * n + cell.z * n * n;
    if (cell.y > 0u && cell.z > 0u && cell.x < n - 2u && sign_diff(sdf[lin], sdf[lin + 1u])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin, n, n * n, sdf[lin], sdf[lin + 1u]);
    }
    if (cell.x > 0u && cell.z > 0u && cell.y < n - 2u && sign_diff(sdf[lin], sdf[lin + n])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin, n * n, 1u, sdf[lin], sdf[lin + n]);
    }
    if (cell.x > 0u && cell.y > 0u && cell.z < n - 2u && sign_diff(sdf[lin], sdf[lin + n * n])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin, 1u, n, sdf[lin], sdf[lin + n * n]);
    }
}
