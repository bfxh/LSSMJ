// 多块稀疏网格化（装配器第一片）：**一次提交网格化全部已分配块**——块表驱动，
// 窗口从打包存储（块数据 + 27 邻块表）按轴分裂算术**gather**（无 CPU 窗打包、无 GPU 哈希）。
// 语义逐字复用 gsn_block.wgsl（单块原语）：局部 cell 域 [0, C+1)³、发射限 lc ≥ 1、
// **全局槽空间**（block·(C+1)³ + lc，索引段直接可拼）、全局坐标输出；缺块 ⇒ EMPTY
// （大正数）⇒ 无符号变化 ⇒ 无产出。
// 输出段：每块局部槽（vtx_pos/vtx_flag，全局槽空间）+ **全局索引段**（每块段起点 = offsets
// 前缀和，两趟发射：count 趟只数 → 读回 + CPU 前缀 → emit 趟写入精确段）。
// 派发：3D (wg_axis, wg_axis, wg_axis × n_blocks)——z 维解块号；上限 n_blocks×wg_axis ≤ 65535。
// 绑定：0=packed 1=nbr 2=blocks(vec4<i32>, 存块起−1 全局坐标) 3=slot_pos 4=slot_flag
//   5=counters(3 段：count[0..n) / emit[n..2n) / offsets[2n..3n)) 6=idx_out 7=mp(uniform)

struct MP {
    c: u32,          // 块边长（cell 数/轴）
    wg_axis: u32,    // 每轴 workgroup 数 = ceil((C+1)/4)
    n_blocks: u32,   // 块数（段偏移用）
    count_only: u32, // 发射趟相位：1 = 只数不写
};

@group(0) @binding(0) var<storage, read> packed : array<f32>;
@group(0) @binding(1) var<storage, read> nbr : array<i32>;
@group(0) @binding(2) var<storage, read> blocks : array<vec4<i32>>;
@group(0) @binding(3) var<storage, read_write> slot_pos : array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> slot_flag : array<u32>;
@group(0) @binding(5) var<storage, read_write> counters : array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> idx_out : array<u32>;
@group(0) @binding(7) var<uniform> mp : MP;

const EMPTY : f32 = 1e30;

fn slot_side() -> u32 {
    return mp.c + 1u;
}

/// 每块局部槽位 = block·(C+1)³ + lc 线性索引。
fn slot_of(block: u32, lc: vec3<u32>) -> u32 {
    let s = mp.c + 1u;
    return block * s * s * s + lc.x + lc.y * s + lc.z * s * s;
}

/// 轴分裂：窗口局部坐标 t ∈ [0, C+2) →（邻块增量 +1 编码, 邻块内局部坐标）。
/// t = 0 ⇒ 邻块 −1、块内 C−1；t = C+1 ⇒ 邻块 +1、块内 0；否则本块、块内 t−1。
fn split1(t: u32) -> vec2<u32> {
    let c = mp.c;
    if (t == 0u) {
        return vec2<u32>(0u, c - 1u);
    }
    if (t == c + 1u) {
        return vec2<u32>(2u, 0u);
    }
    return vec2<u32>(1u, t - 1u);
}

/// 块窗口值 gather：全局点 = 块起−1 + t；缺邻块 ⇒ EMPTY。
fn wval(block: u32, t: vec3<u32>) -> f32 {
    let sx = split1(t.x);
    let sy = split1(t.y);
    let sz = split1(t.z);
    let k = nbr[block * 27u + sz.x * 9u + sy.x * 3u + sx.x];
    if (k < 0) {
        return EMPTY;
    }
    let c = mp.c;
    return packed[u32(k) * c * c * c + sx.y + sy.y * c + sz.y * c * c];
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

@compute @workgroup_size(4, 4, 4)
fn gen_vertices(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let block = wid.z / mp.wg_axis;
    let lc = vec3<u32>(wid.x, wid.y, wid.z % mp.wg_axis) * 4u + lid;
    if (any(lc >= vec3<u32>(mp.c + 1u))) {
        return;
    }
    var d : array<f32, 8>;
    var neg = 0u;
    for (var i = 0u; i < 8u; i++) {
        let v = wval(block, lc + vec3<u32>(i & 1u, (i >> 1u) & 1u, (i >> 2u) & 1u));
        d[i] = v;
        if (v < 0.0) {
            neg = neg + 1u;
        }
    }
    let slot = slot_of(block, lc);
    if (neg == 0u || neg == 8u) {
        slot_flag[slot] = 0u;
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
    // 全局 cell = 块起−1（blocks 里就存这个）+ 局部 cell → 全局坐标位置（与整块同算式）
    let cell = vec3<i32>(lc) + blocks[block].xyz;
    slot_pos[slot] = vec4<f32>(vec3<f32>(cell) + sum / cnt, 1.0);
    slot_flag[slot] = 1u;
}

fn sign_diff(a: f32, b: f32) -> bool {
    return (a < 0.0) != (b < 0.0);
}

fn write_quad(block: u32, slot_idx: u32, p1: u32, sb: u32, sc: u32, d1: f32, d2: f32) {
    let neg_face = (d2 < 0.0) && !(d1 < 0.0);
    let v1 = p1;
    let v2 = p1 - sb;
    let v3 = p1 - sc;
    let v4 = p1 - sb - sc;
    let pos1 = slot_pos[v1].xyz;
    let pos2 = slot_pos[v2].xyz;
    let pos3 = slot_pos[v3].xyz;
    let pos4 = slot_pos[v4].xyz;
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
    let off = atomicLoad(&counters[2u * mp.n_blocks + block]);
    for (var k = 0u; k < 6u; k++) {
        idx_out[off + slot_idx + k] = quad[k];
    }
}

@compute @workgroup_size(4, 4, 4)
fn emit_quads(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let block = wid.z / mp.wg_axis;
    let lc = vec3<u32>(wid.x, wid.y, wid.z % mp.wg_axis) * 4u + lid;
    let s = mp.c + 1u;
    if (any(lc >= vec3<u32>(s))) {
        return;
    }
    // halo 层（局部 0 平面）不发射 quad（其 quad 属 −1 邻块）
    if (any(lc < vec3<u32>(1u))) {
        return;
    }
    // 段号：count 趟（count_only=1）→ 段 0（计数，CPU 侧读回做前缀）；
    // emit 趟（count_only=0）→ 段 n（本趟槽位计数，从 0 起）
    let seg = select(mp.n_blocks, 0u, mp.count_only == 1u);
    let l = slot_of(block, lc);
    let p = wval(block, lc);
    if (sign_diff(p, wval(block, lc + vec3<u32>(1u, 0u, 0u)))) {
        if (mp.count_only == 0u) {
            let slot = atomicAdd(&counters[seg + block], 1u) * 6u;
            write_quad(block, slot, l, s, s * s, p, wval(block, lc + vec3<u32>(1u, 0u, 0u)));
        } else {
            atomicAdd(&counters[seg + block], 1u);
        }
    }
    if (sign_diff(p, wval(block, lc + vec3<u32>(0u, 1u, 0u)))) {
        if (mp.count_only == 0u) {
            let slot = atomicAdd(&counters[seg + block], 1u) * 6u;
            write_quad(block, slot, l, s * s, 1u, p, wval(block, lc + vec3<u32>(0u, 1u, 0u)));
        } else {
            atomicAdd(&counters[seg + block], 1u);
        }
    }
    if (sign_diff(p, wval(block, lc + vec3<u32>(0u, 0u, 1u)))) {
        if (mp.count_only == 0u) {
            let slot = atomicAdd(&counters[seg + block], 1u) * 6u;
            write_quad(block, slot, l, 1u, s, p, wval(block, lc + vec3<u32>(0u, 0u, 1u)));
        } else {
            atomicAdd(&counters[seg + block], 1u);
        }
    }
}
