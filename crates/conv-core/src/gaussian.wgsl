// 高斯腿恒等直通（T-GC-04 第一片）：f32 平面逐元素拷贝。
// 恒等边 = 无转换=无漂移：该 pass 是未来量化/变换阶段的挂载点，本片语义为字节级直通。
// 绑定：0=src(read) 1=dst(rw) 2=params(uniform n)

struct PT {
    n: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
};

@group(0) @binding(0) var<storage, read> src : array<f32>;
@group(0) @binding(1) var<storage, read_write> dst : array<f32>;
@group(0) @binding(2) var<uniform> pp : PT;

@compute @workgroup_size(64)
fn passthrough_f32(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= pp.n) {
        return;
    }
    dst[i] = src[i];
}
