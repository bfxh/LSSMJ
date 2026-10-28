// 高斯腿恒等直通（T-GC-04 第一片）：f32 平面逐元素拷贝。
// 恒等边 = 无转换=无漂移：该 pass 是未来量化/变换阶段的挂载点，本片语义为字节级直通。
// grid-stride（各腿规模探针第一片）：派发封顶 65535（1D 上限），核内按 64×num_workgroups 步进
// ——单平面 >4.19M 元素（≈1.4M 点云）不再拒发。
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
fn passthrough_f32(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = 64u * nwg.x;
    var i = gid.x;
    loop {
        if (i >= pp.n) {
            break;
        }
        dst[i] = src[i];
        i = i + stride;
    }
}
