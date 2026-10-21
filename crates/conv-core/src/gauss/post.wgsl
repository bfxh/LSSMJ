// GPU iso + 密封：density 直方图求 q99.9；全格 ping-pong 泛洪标记边界连通未知区；
// 最终写密封 f32 场。累加/计数用整数原子；mask 每趟只读 in、只写 out ⇒ 无竞争、逐趟确定。

struct Params {
    n: u32,
    mode: u32,        // 0 density, 1 surfel
    count: u32,
    dispatch_x: u32,
    step: u32,        // density histogram bin width
    seal: u32,        // 0 = 未知区一律外部, 1 = 泛洪区分内部
    pad1: u32,
    pad2: u32,
    iso: f32,
    outside: f32,
    min_weight: f32,
    max_density: f32,
    dens_scale: f32,
    den_scale: f32,
    dist_scale: f32,
    known_margin: f32,  // surfel 密封的 known 阈值乘数
};

@group(0) @binding(0) var<storage, read> density: array<u32>;
@group(0) @binding(1) var<storage, read> num_buf: array<i32>;
@group(0) @binding(2) var<storage, read> den_buf: array<u32>;
@group(0) @binding(3) var<storage, read> mask_in: array<u32>;
@group(0) @binding(5) var<storage, read_write> mask_out: array<u32>;
@group(0) @binding(6) var<storage, read_write> counters_hist: array<atomic<u32>>;
@group(0) @binding(8) var<storage, read_write> field: array<f32>;
@group(0) @binding(9) var<uniform> params: Params;

fn idx2d(gid: vec3<u32>) -> u32 {
    return gid.x + gid.y * params.dispatch_x * 64u;
}

fn total() -> u32 {
    return params.n * params.n * params.n;
}

fn known(i: u32) -> bool {
    if (params.mode == 0u) {
        return f32(density[i]) / params.dens_scale > params.iso;
    }
    let den = f32(den_buf[i]) / params.den_scale;
    return den >= params.min_weight * params.known_margin && den > 0.0;
}

@compute @workgroup_size(64)
fn stats(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let d = density[i];
    atomicMax(&counters_hist[4], d);
    if (d > 0u) {
        atomicAdd(&counters_hist[5], 1u);
    }
}

@compute @workgroup_size(64)
fn histogram(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let d = density[i];
    if (d == 0u) {
        return;
    }
    var b = d / max(params.step, 1u);
    if (b >= 65536u) {
        b = 65535u;
    }
    atomicAdd(&counters_hist[8u + b], 1u);
}

@compute @workgroup_size(64)
fn seal_init(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let n = params.n;
    let x = i % n;
    let y = (i / n) % n;
    let z = i / (n * n);
    let boundary = x == 0u || y == 0u || z == 0u || x + 1u == n || y + 1u == n || z + 1u == n;
    if (!boundary || known(i)) {
        return;
    }
    mask_out[i] = 1u;
}

@compute @workgroup_size(64)
fn dilate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let cur = mask_in[i] != 0u;
    var nxt = cur;
    if (known(i)) {
        nxt = false;
    } else if (!cur) {
        let n = params.n;
        let x = i % n;
        let y = (i / n) % n;
        let z = i / (n * n);
        if (x > 0u && mask_in[i - 1u] != 0u) { nxt = true; }
        if (!nxt && x + 1u < n && mask_in[i + 1u] != 0u) { nxt = true; }
        if (!nxt && y > 0u && mask_in[i - n] != 0u) { nxt = true; }
        if (!nxt && y + 1u < n && mask_in[i + n] != 0u) { nxt = true; }
        if (!nxt && z > 0u && mask_in[i - n * n] != 0u) { nxt = true; }
        if (!nxt && z + 1u < n && mask_in[i + n * n] != 0u) { nxt = true; }
    }
    mask_out[i] = select(0u, 1u, nxt);
    if (nxt && !cur) {
        atomicOr(&counters_hist[1], 1u);
    }
}

fn unknown_value(i: u32) -> f32 {
    if (params.seal == 0u || mask_in[i] != 0u) {
        return params.outside;
    }
    atomicAdd(&counters_hist[3], 1u);
    return -params.outside;
}

@compute @workgroup_size(64)
fn field_density(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let d = f32(density[i]) / params.dens_scale;
    var v = 0.0;
    if (d > params.iso) {
        v = params.iso - d;
        atomicAdd(&counters_hist[2], 1u);
    } else {
        v = unknown_value(i);
    }
    field[i] = v;
}

@compute @workgroup_size(64)
fn field_surfel(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = idx2d(gid);
    if (i >= total()) {
        return;
    }
    let den = f32(den_buf[i]) / params.den_scale;
    var v = 0.0;
    let margin = select(1.0, params.known_margin, params.seal != 0u);
    if (den >= params.min_weight * margin && den > 0.0) {
        let nn = f32(num_buf[i]) / params.dist_scale;
        v = clamp(nn / den, -params.outside, params.outside);
        atomicAdd(&counters_hist[2], 1u);
    } else {
        v = unknown_value(i);
    }
    field[i] = v;
}
