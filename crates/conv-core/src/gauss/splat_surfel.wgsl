// GPU surfel TSDF splat：定点 i32/u32 原子累加（整数加法与顺序无关）⇒ 逐位确定。

struct G {
    mean: vec4<f32>,      // xyz + opacity
    lam: vec4<f32>,       // eigenvalues
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    normal: vec4<f32>,
    bbox_min: vec4<u32>,
    bbox_size: vec4<u32>,
};

struct Params {
    n: u32,
    count: u32,
    dispatch_x: u32,
    pad: u32,
    origin_h: vec4<f32>,
    extra: vec4<f32>, // y = dist scale, z = den scale, w = clamp_abs
};

@group(0) @binding(0) var<storage, read> gs: array<G>;
@group(0) @binding(1) var<storage, read> offsets: array<u32>;
@group(0) @binding(2) var<storage, read_write> out_num: array<atomic<i32>>;
@group(0) @binding(3) var<storage, read_write> out_den: array<atomic<u32>>;
@group(0) @binding(4) var<uniform> params: Params;

@compute @workgroup_size(64)
fn splat_surfel(@builtin(global_invocation_id) gid: vec3<u32>) {
    let job = gid.x + gid.y * params.dispatch_x * 64u;
    if (job >= offsets[params.count]) {
        return;
    }

    var lo = 0u;
    var hi = params.count;
    while (lo + 1u < hi) {
        let mid = (lo + hi) / 2u;
        if (offsets[mid] <= job) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let gi = lo;
    let g = gs[gi];
    let sx = g.bbox_size.x;
    let sy = g.bbox_size.y;
    let plane = sx * sy;
    if (sx == 0u || sy == 0u || plane == 0u) {
        return;
    }

    let local = job - offsets[gi];
    let z = local / plane;
    let rem = local % plane;
    let y = rem / sx;
    let x = rem % sx;
    let vx = g.bbox_min.x + x;
    let vy = g.bbox_min.y + y;
    let vz = g.bbox_min.z + z;
    if (vx >= params.n || vy >= params.n || vz >= params.n) {
        return;
    }

    let p = params.origin_h.xyz
        + vec3<f32>(f32(vx), f32(vy), f32(vz)) * params.origin_h.w;
    let d = p - g.mean.xyz;
    let y0 = dot(d, g.v0.xyz);
    let y1 = dot(d, g.v1.xyz);
    let y2 = dot(d, g.v2.xyz);
    let m = y0 * y0 / g.lam.x + y1 * y1 / g.lam.y + y2 * y2 / g.lam.z;
    if (m > 18.0) {
        return;
    }

    let dist = clamp(dot(d, g.normal.xyz), -params.extra.w, params.extra.w);
    let w = clamp(g.mean.w * exp(-0.5 * m), 0.0, 1.0);
    let dq = i32(round(w * dist * params.extra.y));
    let wq = u32(w * params.extra.z + 0.5);
    let idx = vx + vy * params.n + vz * params.n * params.n;
    atomicAdd(&out_num[idx], dq);
    atomicAdd(&out_den[idx], wq);
}
