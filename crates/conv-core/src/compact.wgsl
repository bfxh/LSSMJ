// 稠密顶点 compaction（T-GC-05 第三片）：稀疏 n³ 槽位 → 稠密流。
//   scatter：flag=1 的 cell 的顶点按 cellmap（= flags 排他前缀和）散射到稠密缓冲；
//   remap：索引经 cellmap 重映射到稠密顶点号（四边形的顶点必为表面 cell ⇒ 映射有效）。
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
fn scatter_vertices(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= cp.n || flags[i] == 0u) {
        return;
    }
    pos_out[cellmap[i]] = pos_in[i];
}

@compute @workgroup_size(64)
fn remap_indices(@builtin(global_invocation_id) gid: vec3<u32>) {
    let k = gid.x;
    if (k >= cp.count) {
        return;
    }
    idx_out[k] = cellmap[idx_in[k]];
}
