// 粒子腿各向异性核 splat（T-GC-03 第一片）：每线程 = (粒子, 支撑窗 cell)。
// 累积 = Q16 定点 + atomicAdd<u32>（整数加法可交换 ⇒ 与执行序无关、逐位确定）。
// 绑定：0=centers(f32×3/核) 1=rots(f32×4/核) 2=scales(f32×3/核) 3=field(atomic u32) 4=kp(uniform)

struct KP {
    n: u32,
    count: u32,
    side: u32,   // 支撑窗边长（cell）= 2R+1
    _pad: u32,
};

@group(0) @binding(0) var<storage, read> centers : array<f32>;
@group(0) @binding(1) var<storage, read> rots : array<f32>;
@group(0) @binding(2) var<storage, read> scales : array<f32>;
@group(0) @binding(3) var<storage, read_write> field : array<atomic<u32>>;
@group(0) @binding(4) var<uniform> kp : KP;

const CUT2 : f32 = 9.0;    // (3σ)²
const Q : f32 = 65536.0;   // Q16

// 逆旋转（共轭四元数，q=(x,y,z,w)）：u = Rᵀd
fn rotate_inv(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let u = vec3<f32>(q.x, q.y, q.z);
    let s = q.w;
    return v - 2.0 * s * cross(u, v) + 2.0 * cross(u, cross(u, v));
}

@compute @workgroup_size(64)
fn splat(@builtin(global_invocation_id) gid: vec3<u32>) {
    let side = kp.side;
    let box_vol = side * side * side;
    let p = gid.x / box_vol;
    if (p >= kp.count) {
        return;
    }
    let cell = gid.x % box_vol;
    let half = side / 2u;
    let off = vec3<i32>(i32(cell % side), i32((cell / side) % side), i32(cell / (side * side)))
        - vec3<i32>(i32(half));

    let c = vec3<f32>(centers[3u * p], centers[3u * p + 1u], centers[3u * p + 2u]);
    let q = vec4<f32>(rots[4u * p], rots[4u * p + 1u], rots[4u * p + 2u], rots[4u * p + 3u]);
    let s = vec3<f32>(scales[3u * p], scales[3u * p + 1u], scales[3u * p + 2u]);

    let h = 2.0 / (f32(kp.n) - 1.0);
    let ci = (c + vec3<f32>(1.0)) / h;
    let v = vec3<i32>(floor(ci)) + off;
    if (any(v < vec3<i32>(0)) || any(v >= vec3<i32>(i32(kp.n)))) {
        return;
    }
    let vw = vec3<f32>(v) * h - vec3<f32>(1.0);
    let u = rotate_inv(q, vw - c);
    let m = u / s;
    let nd2 = dot(m, m);
    if (nd2 > CUT2) {
        return;
    }
    let wq = u32(round(exp(-0.5 * nd2) * Q));
    if (wq == 0u) {
        return;
    }
    let lin = u32(v.x) + u32(v.y) * kp.n + u32(v.z) * kp.n * kp.n;
    atomicAdd(&field[lin], wq);
}
