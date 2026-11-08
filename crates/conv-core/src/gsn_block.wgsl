// 稀疏块单块网格化（T-GC-05 第六片；锚 W15A-034/035 的块级 halo 契约）：块窗口 (C+2)³ →
// 本块局部顶点槽 (C+1)³（含 −1 halo 层）+ C³ 发射 cell 的四边形。
// 语义 = gsn.wgsl 的 gen/emit 逐语义裁剪：
//   - 值源 = 块窗口（全局格点 [b·C−1, b·C+C]，索引 = 局部窗坐标，无全局 n）；
//   - cell 域 = 局部 [0, C+1)³（全局 [b·C−1, b·C+C)）；**发射限局部各轴 ≥ 1**（= 本块 cell；
//     halo 层 cell 只产顶点，其 quad 属 −1 邻块）——块间 quad 归属 = "发射 cell ∈ 本块"；
//   - 无域边界条件：稀疏无全局边界，缺块读侧即 EMPTY（大正数 ⇒ 无符号变化 ⇒ 无产出）；
//   - 顶点槽 = 局部 cell 索引（(C+1)³ 布局，b·C−1 层 = lc 0 平面）；位置 = 全局坐标
//     （与整块 GSN 的 `vec3<f32>(cell) + sum/cnt` 同算式 ⇒ 逐位可对拍）。
// 顺序口径：四边形槽位 = atomicAdd（执行序，非确定）；判据在读回侧按"四顶点规范键"排序后对拍。

struct BP {
    c: u32,       // 块边长（cell 数/轴）
    w: u32,       // 窗口边长 = C+2
    wg_axis: u32, // 每轴 workgroup 数 = ceil((C+1)/4)
    p0: u32,
    ox: i32, // 窗口下界全局坐标 = b·C − 1
    oy: i32,
    oz: i32,
    p1: i32,
};

@group(0) @binding(0) var<storage, read> window : array<f32>;
@group(0) @binding(1) var<storage, read_write> vtx_pos : array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> vtx_flag : array<u32>;
@group(0) @binding(3) var<storage, read_write> quad_counter : array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> idx_out : array<u32>;
@group(0) @binding(5) var<uniform> bp : BP;

fn slot_of(lc: vec3<u32>) -> u32 {
    let s = bp.c + 1u;
    return lc.x + lc.y * s + lc.z * s * s;
}

fn wval(lc: vec3<u32>) -> f32 {
    return window[lc.x + lc.y * bp.w + lc.z * bp.w * bp.w];
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
    let lc = wid * 4u + lid;
    if (any(lc >= vec3<u32>(bp.c + 1u))) {
        return;
    }
    var d : array<f32, 8>;
    var neg = 0u;
    for (var i = 0u; i < 8u; i++) {
        let v = wval(lc + vec3<u32>(i & 1u, (i >> 1u) & 1u, (i >> 2u) & 1u));
        d[i] = v;
        if (v < 0.0) {
            neg = neg + 1u;
        }
    }
    let slot = slot_of(lc);
    if (neg == 0u || neg == 8u) {
        vtx_flag[slot] = 0u;
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
    // 全局 cell = 局部 cell + 窗下界（整数和 → f32，与整块同算式）→ 全局坐标位置
    let cell = vec3<i32>(lc) + vec3<i32>(bp.ox, bp.oy, bp.oz);
    vtx_pos[slot] = vec4<f32>(vec3<f32>(cell) + sum / cnt, 1.0);
    vtx_flag[slot] = 1u;
}

fn sign_diff(a: f32, b: f32) -> bool {
    return (a < 0.0) != (b < 0.0);
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
    let lc = wid * 4u + lid;
    let s = bp.c + 1u;
    if (any(lc >= vec3<u32>(s))) {
        return;
    }
    // halo 层（局部 0 平面）不发射 quad（其 quad 属 −1 邻块）
    if (any(lc < vec3<u32>(1u))) {
        return;
    }
    let l = slot_of(lc);
    let p = wval(lc);
    // 三轴边：cell 最小角点（= lc 处窗值）与 +轴邻点异号 ⇒ 该边穿面
    // x 轴边：quad 引用 cell (·, y−1, z−1) 系（sb = y 步、sc = z 步；槽位布局线性于局部 cell）
    if (sign_diff(p, wval(lc + vec3<u32>(1u, 0u, 0u)))) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, l, s, s * s, p, wval(lc + vec3<u32>(1u, 0u, 0u)));
    }
    // y 轴边：引用 (z−1, x−1)
    if (sign_diff(p, wval(lc + vec3<u32>(0u, 1u, 0u)))) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, l, s * s, 1u, p, wval(lc + vec3<u32>(0u, 1u, 0u)));
    }
    // z 轴边：引用 (x−1, y−1)
    if (sign_diff(p, wval(lc + vec3<u32>(0u, 0u, 1u)))) {
        let slot = atomicAdd(&quad_counter[0], 1u) * 6u;
        write_quad(slot, l, 1u, s, p, wval(lc + vec3<u32>(0u, 0u, 1u)));
    }
}
