// GPU Surface Nets（T-GC-02；分块/全 GPU 遍历 = T-GC-05）：SDF→mesh，逐语义复刻
// fast-surface-nets 0.2.1：
//   顶点 = cell 内 12 条边的等值面交点平均（CUBE_CORNERS bit0=x,bit1=y,bit2=z）；
//   四边形 = 每表面 cell 的 X/Y/Z 三轴边（边界条件 y,z>0 && x<n-2 等），
//   对角线按短边拆分，negative_face 翻绕序。
// 遍历口径（T-GC-05 第五片，全 GPU）：占据检测（occupancy）→ 块扫描（blockmap，scan.wgsl）
//   → 活跃表（build_active）→ indirect 参数（indirect_args）→ gen/emit 走
//   dispatch_workgroups_indirect。每块参数（块起/原点/hi）由块号在 shader 内推导，
//   sdf 全局索引直读（无打包切片）；顶点/索引槽位保持全局 n³ 布局 ⇒ 分块与整块逐位可对拍。
//   （旧 CPU 侧"值切片打包 + params 数组"机械已随本片移除。）
// 绑定：0=sdf(read) 1=vtx_pos(rw,vec4) 2=vtx_flag(rw) 3=block_flag(rw) 4=blockmap(read)
//   5=active_list(rw) 6=indirect_args(rw) 7=quad_counter(rw,atomic) 8=idx_out(rw) 9=rp(uniform)
//   （两组绑定组：gen/emit 的一组不含 6=indirect_args——indirect 源缓冲不得与该
//   dispatch 的 rw 绑定同作用域；写者组与 gen 组各自 storage 数 ≤ 8 = wgpu 默认上限）
// 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"
// 排序后对拍——四边形集合与绕序确定 ⇒ 排序流逐位确定。
// 两趟发射（规模第一片）：索引缓冲不再按最坏情形 72B/格点 预分配（n≥124 即撞默认
// binding 上限、内存 O(72·n³)）——同上 indirect 派发跑两趟：count_only=1 只数、读回后
// 精确分配 idx，再 count_only=0 写。谓词两趟一致 ⇒ 计数即写入条数。

struct RP {
    n: u32,
    per_axis: u32,    // 每轴块数 = ceil((n−1)/chunk_cells)
    chunk_cells: u32,
    wg_axis: u32,     // 每块每轴 workgroup 数 = ceil((chunk_cells+1)/4)
    occ_wg: u32,      // 占据检测派发的 workgroup 数（grid-stride 用）
    count_only: u32,  // 发射趟相位：1 = 只数不写（两趟发射先数后配，规模第一片）
};

@group(0) @binding(0) var<storage, read> sdf : array<f32>;
@group(0) @binding(1) var<storage, read_write> vtx_pos : array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> vtx_flag : array<u32>;
@group(0) @binding(3) var<storage, read_write> block_flag : array<atomic<u32>>;
@group(0) @binding(4) var<storage, read> blockmap : array<u32>;
@group(0) @binding(5) var<storage, read_write> active_list : array<u32>;
@group(0) @binding(6) var<storage, read_write> indirect_args : array<u32>;
@group(0) @binding(7) var<storage, read_write> quad_counter : array<atomic<u32>>;
@group(0) @binding(8) var<storage, read_write> idx_out : array<u32>;
@group(0) @binding(9) var<uniform> rp : RP;

fn total_blocks() -> u32 {
    return rp.per_axis * rp.per_axis * rp.per_axis;
}

fn block_of(id: u32) -> vec3<u32> {
    let bx = id % rp.per_axis;
    let by = (id / rp.per_axis) % rp.per_axis;
    let bz = id / (rp.per_axis * rp.per_axis);
    return vec3<u32>(bx, by, bz) * rp.chunk_cells;
}

fn block_hi(a: vec3<u32>) -> vec3<u32> {
    return min(a + vec3<u32>(rp.chunk_cells), vec3<u32>(rp.n - 1u));
}

/// 占据检测（逐 cell 并行，原子置块标志；精确性：块内两符号 ⇒ 相邻异号点 ⇒ 必有混合
/// cell（共享该点对的 cell 落在块内），混合 cell ⇔ gen 会产出顶点——与"块双符号"判据等价）。
@compute @workgroup_size(64)
fn occupancy(@builtin(global_invocation_id) gid: vec3<u32>) {
    let axis = rp.n - 1u;
    let total_cells = axis * axis * axis;
    let stride = 64u * rp.occ_wg;
    var i = gid.x;
    loop {
        if (i >= total_cells) {
            break;
        }
        let x = i % axis;
        let y = (i / axis) % axis;
        let z = i / (axis * axis);
        var neg = 0u;
        var pos = 0u;
        for (var c = 0u; c < 8u; c++) {
            let o = vec3<u32>(c & 1u, (c >> 1u) & 1u, (c >> 2u) & 1u);
            let g = vec3<u32>(x, y, z) + o;
            if (sdf[g.x + g.y * rp.n + g.z * rp.n * rp.n] < 0.0) {
                neg = neg + 1u;
            } else {
                pos = pos + 1u;
            }
        }
        if (neg > 0u && pos > 0u) {
            let bx = x / rp.chunk_cells;
            let by = y / rp.chunk_cells;
            let bz = z / rp.chunk_cells;
            atomicOr(&block_flag[bx + by * rp.per_axis + bz * rp.per_axis * rp.per_axis], 1u);
        }
        i = i + stride;
    }
}

/// 活跃表：block_flag=1 的块号按 blockmap（排他前缀和）散射进 active_list。
@compute @workgroup_size(64)
fn build_active(@builtin(global_invocation_id) gid: vec3<u32>) {
    let id = gid.x;
    if (id >= total_blocks()) {
        return;
    }
    if (atomicLoad(&block_flag[id]) == 1u) {
        active_list[blockmap[id]] = id;
    }
}

/// indirect 参数：(活跃块 × wg_axis, wg_axis, wg_axis)。
@compute @workgroup_size(1)
fn build_indirect_args() {
    let last = total_blocks() - 1u;
    let n_active = blockmap[last] + atomicLoad(&block_flag[last]);
    indirect_args[0] = n_active * rp.wg_axis;
    indirect_args[1] = rp.wg_axis;
    indirect_args[2] = rp.wg_axis;
}

fn corner_vec(corner: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u));
}

fn corner_sdf(cell: vec3<u32>, corner: u32) -> f32 {
    let g = cell + vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
    return sdf[g.x + g.y * rp.n + g.z * rp.n * rp.n];
}

fn lin_global(cell: vec3<u32>) -> u32 {
    return cell.x + cell.y * rp.n + cell.z * rp.n * rp.n;
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

// 展平遍历：chunk 序 = wid.x / wg_axis → 活跃表块号；块参数（块起/原点/hi）推导。
fn cell_of(wid: vec3<u32>, lid: vec3<u32>) -> vec3<u32> {
    let ordinal = wid.x / rp.wg_axis;
    let id = active_list[ordinal];
    let a = block_of(id);
    let origin = max(a, vec3<u32>(1u)) - vec3<u32>(1u); // 块起−1（钳 0）
    let local_wg = vec3<u32>(wid.x % rp.wg_axis, wid.y, wid.z);
    return origin + local_wg * 4u + lid;
}

@compute @workgroup_size(4, 4, 4)
fn gen_vertices(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let ordinal = wid.x / rp.wg_axis;
    let id = active_list[ordinal];
    let a = block_of(id);
    let hi = block_hi(a);
    let cell = cell_of(wid, lid);
    if (any(cell >= hi)) {
        return;
    }
    var d : array<f32, 8>;
    var neg = 0u;
    for (var i = 0u; i < 8u; i++) {
        let v = corner_sdf(cell, i);
        d[i] = v;
        if (v < 0.0) {
            neg = neg + 1u;
        }
    }
    let lin = lin_global(cell);
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
    let ordinal = wid.x / rp.wg_axis;
    let id = active_list[ordinal];
    let a = block_of(id);
    let hi = block_hi(a);
    let cell = cell_of(wid, lid);
    if (any(cell >= hi)) {
        return;
    }
    // halo 层不发射 quad（其 quad 属邻块；全局边界条件仍按全局坐标判）
    if (any(cell < a)) {
        return;
    }
    let l = lin_global(cell);
    let n = rp.n;
    if (cell.y > 0u && cell.z > 0u && cell.x < n - 2u && sign_diff(sdf[l], sdf[l + 1u])) {
        if (rp.count_only == 0u) {
            let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
            write_quad(slot, l, n, n * n, sdf[l], sdf[l + 1u]);
        } else {
            atomicAdd(&quad_counter[0], 1u);
        }
    }
    if (cell.x > 0u && cell.z > 0u && cell.y < n - 2u && sign_diff(sdf[l], sdf[l + n])) {
        if (rp.count_only == 0u) {
            let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
            write_quad(slot, l, n * n, 1u, sdf[l], sdf[l + n]);
        } else {
            atomicAdd(&quad_counter[0], 1u);
        }
    }
    if (cell.x > 0u && cell.y > 0u && cell.z < n - 2u && sign_diff(sdf[l], sdf[l + n * n])) {
        if (rp.count_only == 0u) {
            let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
            write_quad(slot, l, 1u, n, sdf[l], sdf[l + n * n]);
        } else {
            atomicAdd(&quad_counter[0], 1u);
        }
    }
}
