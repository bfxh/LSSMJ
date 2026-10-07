// JFA（Jump Flooding）距离场——conv-core 粒子→隐式场腿的 GPU 积木。
// 坐标口径：体素坐标 = 其整数 index（与 CPU 参照一致），种子同为 index 坐标。
// 统一绑定布局（两个入口点共用一张 BGL，避免同模块重复绑定）：
//   0=label_in(read) 1=label_out(rw) 2=seeds(read) 3=params(uniform) 4=dist_out(rw)

struct Params {
    step: u32,
    n: u32,
};

@group(0) @binding(0) var<storage, read> label_in : array<u32>;
@group(0) @binding(1) var<storage, read_write> label_out : array<u32>;
@group(0) @binding(2) var<storage, read> seeds : array<vec4<f32>>;
@group(0) @binding(3) var<uniform> params : Params;
@group(0) @binding(4) var<storage, read_write> dist_out : array<f32>;

const INVALID : u32 = 0xFFFFFFFFu;
const FAR : f32 = 3.4e38;

fn d2(a: vec3<f32>, b: vec3<f32>) -> f32 {
    let d = a - b;
    return dot(d, d);
}

@compute @workgroup_size(4, 4, 4)
fn jfa_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = params.n;
    if (gid.x >= n || gid.y >= n || gid.z >= n) {
        return;
    }
    let idx = gid.x + gid.y * n + gid.z * n * n;
    let pos = vec3<f32>(gid);
    var best = label_in[idx];
    var best_d2 = FAR;
    if (best != INVALID) {
        best_d2 = d2(seeds[best].xyz, pos);
    }
    let s = i32(params.step);
    let base = vec3<i32>(gid);
    for (var dz = -1; dz <= 1; dz++) {
        for (var dy = -1; dy <= 1; dy++) {
            for (var dx = -1; dx <= 1; dx++) {
                let off = base + s * vec3<i32>(dx, dy, dz);
                if (off.x < 0 || off.y < 0 || off.z < 0 || off.x >= i32(n) || off.y >= i32(n) || off.z >= i32(n)) {
                    continue;
                }
                let ci = off.x + off.y * i32(n) + off.z * i32(n) * i32(n);
                let c = label_in[ci];
                if (c == INVALID) {
                    continue;
                }
                let dd = d2(seeds[c].xyz, pos);
                if (dd < best_d2) {
                    best_d2 = dd;
                    best = c;
                }
            }
        }
    }
    label_out[idx] = best;
}

@compute @workgroup_size(4, 4, 4)
fn to_distance(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = params.n;
    if (gid.x >= n || gid.y >= n || gid.z >= n) {
        return;
    }
    let idx = gid.x + gid.y * n + gid.z * n * n;
    let label = label_in[idx];
    if (label == INVALID) {
        dist_out[idx] = FAR;
        return;
    }
    dist_out[idx] = sqrt(d2(seeds[label].xyz, vec3<f32>(gid)));
}
