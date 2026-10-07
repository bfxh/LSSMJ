// GPU Surface Nets（T-GC-02；分块 T-GC-05）：SDF→mesh，逐语义复刻 fast-surface-nets 0.2.1：
//   顶点 = cell 内 12 条边的等值面交点平均（CUBE_CORNERS bit0=x,bit1=y,bit2=z）；
//   四边形 = 每表面 cell 的 X/Y/Z 三轴边（边界条件 y,z>0 && x<n-2 等），
//   对角线按短边拆分，negative_face 翻绕序。
// 分块契约：每块在"值切片（含 1-voxel halo，打包在单一缓冲）+ cell 运行区间"上执行；
//   quad 归属过滤 cell ≥ chunk_min（halo 层的 quad 属邻块）；顶点/索引槽位保持全局 n³ 布局
//   ⇒ 分块与整块输出全缓冲逐位可对拍。
// 遍历口径（T-GC-05 第二片）：块表 = 活跃块的 params 数组（每块 origin/hi/切片偏移/nv）；
//   单趟 dispatch 把 (活跃块 × 每块 workgroup) 展平进 x 轴——每块参数从 params[chunk] 读。
// 绑定布局：0=sdf_packed(read) 1=vtx_pos(rw,vec4) 2=vtx_flag(rw) 3=params(read,array)
//   4=quad_counter(rw,atomic) 5=idx_out(rw)
// 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"
// 排序后对拍——四边形集合与绕序确定 ⇒ 排序流逐位确定（稀疏 compaction 属后续片）。

struct P {
    n: u32,                // 全局宽（顶点/索引槽位步长）
    wg_axis: u32,          // 每块每轴的 workgroup 数（覆盖所有块，多余调用由 hi 守卫剪掉）
    slice_offset: u32,     // 本块值切片在打包缓冲中的起始（f32 元素）
    _pad0: u32,
    nv: vec3<u32>,         // 值切片宽（每轴；因钳位/末块各轴可不等宽）
    _pad1: u32,
    origin: vec3<u32>,     // 运行区间下界（每轴）= 块起 − 1（钳 0），值切片同 origin
    _pad2: u32,
    hi: vec3<u32>,         // 运行区间上界（每轴，exclusive）
    _pad3: u32,
    chunk_min: vec3<u32>,  // 本块 cell 归属下界（每轴）= 块起；quad 过滤用
    _pad4: u32,
};

@group(0) @binding(0) var<storage, read> sdf : array<f32>;
@group(0) @binding(1) var<storage, read_write> vtx_pos : array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> vtx_flag : array<u32>;
@group(0) @binding(3) var<storage, read> params : array<P>;
@group(0) @binding(4) var<storage, read_write> quad_counter : array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> idx_out : array<u32>;

fn slice_index(p: P, cell: vec3<u32>) -> u32 {
    let d = cell - p.origin;
    return p.slice_offset + d.x + d.y * p.nv.x + d.z * p.nv.x * p.nv.y;
}

fn corner_sdf(p: P, cell: vec3<u32>, corner: u32) -> f32 {
    let o = vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
    let g = cell + o - p.origin;
    return sdf[p.slice_offset + g.x + g.y * p.nv.x + g.z * p.nv.x * p.nv.y];
}

fn corner_vec(corner: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u));
}

fn lin_global(p: P, cell: vec3<u32>) -> u32 {
    return cell.x + cell.y * p.n + cell.z * p.n * p.n;
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

// 展平遍历：chunk = wid.x / wg_axis（wg_axis 各块相同，取 params[0]）；块内坐标 = 剩余维。
fn cell_of(wid: vec3<u32>, lid: vec3<u32>, p: P) -> vec3<u32> {
    let wg_axis = params[0].wg_axis;
    let local_wg = vec3<u32>(wid.x % wg_axis, wid.y, wid.z);
    return p.origin + local_wg * 4u + lid;
}

@compute @workgroup_size(4, 4, 4)
fn gen_vertices(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let p = params[wid.x / params[0].wg_axis];
    let cell = cell_of(wid, lid, p);
    if (cell.x >= p.hi.x || cell.y >= p.hi.y || cell.z >= p.hi.z) {
        return;
    }
    var d : array<f32, 8>;
    var neg = 0u;
    for (var i = 0u; i < 8u; i++) {
        let v = corner_sdf(p, cell, i);
        d[i] = v;
        if (v < 0.0) {
            neg = neg + 1u;
        }
    }
    let lin = lin_global(p, cell);
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
fn emit_quads(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let p = params[wid.x / params[0].wg_axis];
    let cell = cell_of(wid, lid, p);
    if (cell.x >= p.hi.x || cell.y >= p.hi.y || cell.z >= p.hi.z) {
        return;
    }
    // halo 层不发射 quad（其 quad 属邻块；全局边界条件仍按全局坐标判）
    if (cell.x < p.chunk_min.x || cell.y < p.chunk_min.y || cell.z < p.chunk_min.z) {
        return;
    }
    let l = slice_index(p, cell);
    let n = p.n;
    let sy = p.nv.x;              // y 步长 = 切片 x 宽
    let sz = p.nv.x * p.nv.y;     // z 步长 = 切片 x 宽 × y 宽
    if (cell.y > 0u && cell.z > 0u && cell.x < n - 2u && sign_diff(sdf[l], sdf[l + 1u])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin_global(p, cell), n, n * n, sdf[l], sdf[l + 1u]);
    }
    if (cell.x > 0u && cell.z > 0u && cell.y < n - 2u && sign_diff(sdf[l], sdf[l + sy])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin_global(p, cell), n * n, 1u, sdf[l], sdf[l + sy]);
    }
    if (cell.x > 0u && cell.y > 0u && cell.z < n - 2u && sign_diff(sdf[l], sdf[l + sz])) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, lin_global(p, cell), 1u, n, sdf[l], sdf[l + sz]);
    }
}
