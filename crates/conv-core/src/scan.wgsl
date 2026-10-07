// GPU 排他前缀和（T-GC-05 第三片）：u32 数组 → 排他前缀和 + 总数。
// 三段式、无原子、全确定（"需要 GPU 顺序时走 GPU scan（规范保证）"）：
//   ① block_scan：每 workgroup（256）共享内存 Hillis-Steele 扫描一块 + 写块和；
//   ② block_carry：单 workgroup——每线程先串行累加自己的 stripe，stripe 间共享扫描，
//      再串行回填每块偏移（线程内串行 ⇒ 全程无原子）；
//   ③ scan_final：块内重扫 + 加块偏移 → 输出。
// 绑定：0=in 1=out 2=block_sums(nb) 3=block_offsets(nb) 4=params(n)
// 长度任意（末块越界按 0 补，"n" 之外的加载返回 0）。

struct Params {
    n: u32,
    nb: u32,
    _p0: u32,
    _p1: u32,
};

@group(0) @binding(0) var<storage, read> s_in : array<u32>;
@group(0) @binding(1) var<storage, read_write> s_out : array<u32>;
@group(0) @binding(2) var<storage, read_write> s_sums : array<u32>;
@group(0) @binding(3) var<storage, read_write> s_off : array<u32>;
@group(0) @binding(4) var<uniform> pp : Params;

const WG : u32 = 256u;

var<workgroup> sh : array<u32, 256>;

fn load(i: u32) -> u32 {
    if (i >= pp.n) {
        return 0u;
    }
    return s_in[i];
}

// 对共享内存 sh（已填 256 项）做排他扫描；返回本线程的排他值。
// 迭代序：读（上一轮已 barrier）→ B1（读齐）→ 写 → B2（写可见，供下一轮读）。
fn sh_exclusive_scan(tid: u32) -> u32 {
    workgroupBarrier();
    var d = 1u;
    loop {
        if (d >= WG) {
            break;
        }
        var t = 0u;
        if (tid >= d) {
            t = sh[tid - d];
        }
        workgroupBarrier();
        sh[tid] = sh[tid] + t;
        workgroupBarrier();
        d = d * 2u;
    }
    if (tid == 0u) {
        return 0u;
    }
    return sh[tid - 1u];
}

@compute @workgroup_size(256)
fn block_scan(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let tid = lid.x;
    let base = wid.x * WG;
    sh[tid] = load(base + tid);
    let e = sh_exclusive_scan(tid);
    if (base + tid < pp.n) {
        s_out[base + tid] = e;
    }
    if (tid == WG - 1u) {
        s_sums[wid.x] = sh[WG - 1u];
    }
}

@compute @workgroup_size(256)
fn block_carry(@builtin(local_invocation_id) lid: vec3<u32>) {
    let tid = lid.x;
    // 连续 stripe（块序保持）：线程 t 负责块 [t·K, min((t+1)·K, nb))，K = ceil(nb/256)。
    // 必须连续：stripe 间扫描得到的偏移只有在"stripe 序 = 块序"时才是块偏移的前缀。
    let k = (pp.nb + WG - 1u) / WG;
    let b0 = tid * k;
    let b1 = min(b0 + k, pp.nb);
    // ① 线程内层：stripe 串行求和
    var stripe = 0u;
    var b = b0;
    loop {
        if (b >= b1) {
            break;
        }
        stripe = stripe + s_sums[b];
        b = b + 1u;
    }
    sh[tid] = stripe;
    // ② stripe 间排他扫描
    let e = sh_exclusive_scan(tid);
    // ③ 线程内层：stripe 串行回填块偏移（run 从 stripe 前缀起，逐块累加）
    var run = e;
    b = b0;
    loop {
        if (b >= b1) {
            break;
        }
        s_off[b] = run;
        run = run + s_sums[b];
        b = b + 1u;
    }
}

@compute @workgroup_size(256)
fn scan_final(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let tid = lid.x;
    let base = wid.x * WG;
    sh[tid] = load(base + tid);
    let e = sh_exclusive_scan(tid);
    if (base + tid < pp.n) {
        s_out[base + tid] = e + s_off[wid.x];
    }
}
