// 稠密顶点 compaction（T-GC-05 第三片）：稀疏 n³ 槽位 → 稠密流。
//   scatter：flag=1 的 cell 的顶点按 cellmap（= flags 排他前缀和）散射到稠密缓冲；
//   remap：索引经 cellmap 重映射到稠密顶点号（四边形的顶点必为表面 cell ⇒ 映射有效）。
// grid-stride（规模第一片）：派发数封顶 65535（单维上限），核内按 64×num_workgroups 步进
//   ——n³/64 超过上限（n≥161）时不再拒发。
// 绑定：0=flags 1=cellmap 2=pos_in(vec4) 3=pos_out(vec4) 4=idx_in 5=idx_out 6=params

struct CP {
    n: u32,      // cell 总数（n³）
    count: u32,  // 索引元素数（quad_count × 6）
    _p0: u32,
    _p1: u32,
};

@group(0) @binding(0) var<storage, read> flags : array<u32>;
@group(0) @binding(1) var<storage, read> cellmap : array<u32>;
@group(0) @binding(2) var<storage, read> pos_in : array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> pos_out : array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> idx_in : array<u32>;
@group(0) @binding(5) var<storage, read_write> idx_out : array<u32>;
@group(0) @binding(6) var<uniform> cp : CP;

@compute @workgroup_size(64)
fn scatter_vertices(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = 64u * nwg.x;
    var i = gid.x;
    loop {
        if (i >= cp.n) {
            break;
        }
        if (flags[i] != 0u) {
            pos_out[cellmap[i]] = pos_in[i];
        }
        i = i + stride;
    }
}

@compute @workgroup_size(64)
fn remap_indices(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = 64u * nwg.x;
    var k = gid.x;
    loop {
        if (k >= cp.count) {
            break;
        }
        idx_out[k] = cellmap[idx_in[k]];
        k = k + stride;
    }
}
