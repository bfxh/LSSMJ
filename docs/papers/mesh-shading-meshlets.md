# meshlet / mesh shader：簇化几何管线（C1 深读 2/7）

> 来源（抓取 2026-10-01，全部 200）：
> meshoptimizer README `raw.githubusercontent.com/zeux/meshoptimizer/master/README.md`；
> Khronos 博客 `khronos.org/blog/mesh-shading-for-vulkan`；
> Microsoft DirectX-Specs `d3d/MeshShader.md`；
> AMD/GPUOpen 演示 `gpuopen.com/presentations/2024/Mesh_Shaders_Work_Graphs-Perfect_Pair.pdf`。
> 账本：`docs/analysis/ledger/w3a.jsonl` 的 `W3A-006..016`。原文落盘 `scratch/c1/raw/`。

## 1. 它解决什么成本

- 传统几何输入是"索引缓冲 + 顶点着色器"，图元装配在固定管线里；簇化把**工作单元**从"一个 mesh"
  改成"一个 meshlet（小簇）"，于是剔除/LOD/压缩都能以簇为粒度做（`W3A-011`）。
- 动机一句话：**GPU 端要能便宜地丢弃看不见的簇**——剔除粒度越细，越能少画（`W3A-006/011`）。

## 2. 关键机制（每条带锚）

1. **定义**：mesh shader 合并"顶点处理 + 图元处理"（`W3A-015`，DX12 规范原文），
   走 compute 编程模型、线程协作生成网格、数据写共享内存（`W3A-010`，Khronos 原文）。
2. **两级结构**：task(~amplification) shader 做簇级剔除/LOD（`W3A-010/011`），
   官方意图最终取代硬件细分器（`W3A-016`）。
3. **簇的硬约束**：NV 推荐 max_vertices=64 / max_triangles=126，cone_weight 默认 0.25（`W3A-007`，
   meshoptimizer README 原文）——这是"一个工作单元最多 126 个三角形"的口径来源。
4. **数据形态**：meshlet = 少量顶点 + 独立 micro-index 缓冲（README 原文段），
   可附带包围球/锥做簇剔除；有平台索引范围限制（`MESHOPTIMIZER_CLUSTERIZER_INDEXLIMIT`）。
5. **GPU 端调度新形态**：work graph 的"draw 节点"不再调 compute，而是直接派发 mesh shader 管线
   （`W3A-013/024/025`）；AMD 自述 work graph ≈ "强化版 amplification shader"（`W3A-014`）。

## 3. 规模量级

- 一个 meshlet ≈ **126 三角形 / 64 顶点**（`W3A-007`）。
- Nanite 是这条线的极端：官方口径"万亿三角形场景实时帧率"（`W3A-001`）、全链路 流式/解压/剔除/光栅/着色（`W3A-002`）。
- 我们候选窗单帧图元量级 ~10^2–10^4（显示列表条目数）——**比一个 meshlet 大不了多少**。

## 4. 对本项目的取舍

- **不吸收**：2D 图元不需要簇化；我们不存在"百万三角形"场景（量级差 2–3 个数量级，`W3A-017/018` 是 10^5 对象）。
- **术语/边界留档**：若 GPU 档以后做"簇剔除"，我们真正的对应物是 **tile/damage 分箱**（见 `2d-gpu-rasterization.md`），
  而不是 meshlet。
- Khronos 自己的劝退句可直接引用为"不追新特性"的决策依据（`W3A-012`）。
- GPU 端剔除的两条坑值得抄进纪律：并行写出的**顺序不确定**、为统一化引入的**内存膨胀**（`W3A-078/079`，Ubisoft 自列）。

## 5. 未验证

- NVIDIA《Introduction to Turing Mesh Shaders》原文 404（本批两次尝试），改用 Khronos + DX12 规范替代（`W3A-*` 已注明）。
- mesh shader 在我们可能用的跨平台后端（wgpu/WebGPU）上的可用性：**未核证**，故不承诺。
