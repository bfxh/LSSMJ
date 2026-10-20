# 高斯体素 → 网格：路线、实现与 UX（2026-10-20 实现批）

> **这份文档是什么**：只回答一条边——**高斯体素怎么转网格，以及转换工具的 UX 该怎么做**。
> 范围口径来自用户 2026-10-20："你就只研究研究高斯体素怎么转网格和 UX……而且要做出来"。
> **实现位置**：`crates/conv-core/src/gauss.rs` + `src/gauss/{ply,voxel,mesh,gpu}.rs` +
> `src/gauss/{splat_density,splat_surfel}.wgsl` + `src/gauss/simd.rs` + `src/bin/gauss2mesh/{main,args,report,html}.rs` +
> `src/viewer.html`；机器判据在 `tests/gauss_leg.rs` + `tests/gauss_gpu.rs` + `tests/cli_batch.rs`。
> **证据纪律**：设计判决只依赖本仓账本锚（`W15A-*`/`W15C-*`）与**本地实测**；
> 2025–2026 的新论文只作 web 旁证，未入账，不单独支撑设计判决（本仓 `web` 纪律）。
> 本轮没有新增 ledger 条目；新增的外部链接在 §9 明示"未入账/未复算"。

## TL;DR（10 条）

1. **高斯没有拓扑**；"高斯体素→网格"不是一条转换公式，而是三步：**选一个可度量的标量场 → 定阈值 → 提等值面**。
   这是本报告最重要的口径修正：讨论"怎么转"之前先回答"场是什么"。
2. 两条能落地的场：**密度/不透明度核**（原始 3DGS 的保守路线）与**朝向面元 TSDF**（2DGS/SuGaR/带法线资产的质量路线）。
   前者不依赖法线但会糊/破壳；后者几何好但依赖法线与闭合表面。
3. **原始 3DGS 不能直接出好网格**：SuGaR 先正则化让高斯贴到表面（`W15A-003`），2DGS 天生是取向平面盘（`W15A-005/006`）；
   所以 `auto` 只在**资产带法线**时走 surfel，无法线一律 density，surfel 必须显式 `--mode surfel`。
4. **采样核在法线/最薄轴上至少一格宽**：薄于体素的高斯在格点上会采样成空场，Surface Nets 无面可提；
   本实现把 normal σ 下限钉在 `1.0 × h`（密度核下限 `0.75 × h`）。
5. **TSDF 只观测表面窄带**：未观测内部若不填，会长出"内壳"。实测球首跑（未封闭）得到
   χ=4 / 2 分量 / 体积 0.596；加**边界泛洪封闭**后 χ=2 / 1 分量 / 体积 1.774（解析 1.767，误差 0.41%）。
6. **密度场的全局 iso 在稀疏处会破壳**：实测一份 3DGS PLY 在 iso=16.04 时 χ=-34、无封闭内部；
   `auto` 检测到后降阈值重试（×0.75 → iso=12.03），得到 χ=2 / 1 分量，体积 2.136（比球解析值高 20.9%）。
   这条降级必须**显式记账**，不许静默换参数（C22 的"失败显式降级记账"）。
7. **本机实测（64³，release，24 线程 splat）**：球体积误差 **0.41%**、环面 **0.40% 且 χ=0（亏格 1）**；
   20k 高斯总耗时 **0.04–0.06 s**（1T 参照 0.17–0.38 s）；体素化 **8.5× / 5.6× / 8.8×**
   （球 / 环面 / blobs），且与 1T **逐位一致**；同输入哈希稳定（球 `cd8f000dba7bfb4e`）。
8. **UX 的核心不是参数面板**，而是"先诊断、再转换、可解释降级、可预览"：
   `inspect` 先看资产；`auto` 说出选路理由；三旋钮（分辨率/剪枝/阈值）都有默认值与后果提示；
   失败/回退/重试进警告与 JSON；自包含 WebGL 预览默认只显示网格（20k 高斯点盖上去会摩尔纹）。
9. **GPU（`--gpu`）已覆盖 splat + iso + 密封**：定点整数原子 + 全格 ping-pong 泛洪 ⇒ 两次 GPU 运行
   逐位一致，且与 frontier 版后处理逐位等价（四个 CLI 哈希不变）；设备复用后体素化+后处理
   **14–30 ms**（64³/128³，20k 球，GPU clear 后），Vulkan-first 设备初始化 **~0.44 s**（全后端枚举 ~0.87 s），
   单次转换端到端仍慢于缓存的 CPU ⇒ 默认仍 CPU；GPU 是批处理/更高分辨率档，
   `voxelize_gpu_with(&Headless, …)` 是复用入口。
10. **GPU 网格目前不划算**：复用仓内 `gsn` 后与 CPU 网格的三角形数/χ/顶点数完全一致
   （同一场、体积差 ~1e-6）；索引缓冲改精确分配后 128³ 解除 128 MB binding 限制，
   但 64³ 热态 13–15 ms vs CPU 3.7–3.8 ms，128³ 96–183 ms vs CPU ~21 ms，首跑还含管线编译
   ⇒ `--gpu-mesh` 只作实验档，默认仍 CPU；转正需要"场留在 GPU + GPU compaction/scan"的改造。

## 1. 术语：三种"高斯体素"

用户说"高斯体素"时，可能指三件不同的事；先把它们分开，否则"转网格"的答案会互相打架：

| 读法 | 是什么 | 转网格的第一跳 | 本实现 |
| --- | --- | --- | --- |
| **① 体素化高斯** | 把一组 3D 高斯 splat 到规则体素格，得到密度/不透明度标量场 | 阈值 → 等值面 | ✅ 默认 `density` |
| **② 高斯参数进体素** | 体素/稀疏块里存高斯锚点或参数（Scaffold/Octree/混合体素支架） | 先解码/求值成 SDF/密度场，再等值面 | 只接受标准 3DGS PLY；不解码锚点结构 |
| **③ 朝向面元** | 每个高斯压成带法线的平面盘（2DGS/SuGaR 一类） | 加权 TSDF 融合 → 零等值面 | ✅ `surfel` |

本报告的"高斯体素→网格"按 **①/③** 落地；**②** 的论文在 §9 作为旁证，但"解码器资产"不在本轮范围
（w15a 也把"体素→高斯"标成依赖解码器的有界边，`W15A-009/010/011`）。

## 2. 路线矩阵（只列能落地的）

| # | 路线 | 场 / 提取 | 优点 | 代价 / 失败模式 | 本实现 |
| --- | --- | --- | --- | --- | --- |
| A | **密度 splat → iso → Surface Nets** | 核密度（α·exp(-½ Mahalanobis)） | 不需要法线；原始 3DGS 可直接跑；实现短 | 全局阈值对稀疏区敏感 → 破壳；阈值低 → 糊/膨胀；floaters 会成碎片 | ✅ 默认（无法线） |
| B | **朝向面元 TSDF → 0 → Surface Nets** | 以最薄轴为法线，加权平面距离 | 几何准；球/环面误差 ~0.4%；χ 正确 | 依赖法线；法线翻面/开放面会漏；未封闭内部会长内壳 | ✅ 默认（有法线） |
| C | **GOF 式 opacity levelset** | 光线追踪 opacity field + Marching Tetrahedra | 高质量、自适应，论文口径强（`W15A-007/008`） | 离线、实现重；需要可微/训练或高质量求值 | ❌ 未实现，质量档候选 |
| D | **SuGaR 式正则 + Poisson** | 先把高斯压到表面，再 Poisson | 可编辑 mesh；论文分钟级（`W15A-003/004`） | 需要训练/优化；离线；不是"拿来即转" | ❌ 未实现 |
| E | **混合 Gaussian–Voxel 支架 + SDF** | 稀疏体素支架约束高斯，联合优化后提取 SDF | 2026 论文口径：表面质量与训练收敛更好（§9） | 需要联合训练/解码器；工程量大 | ❌ 未实现，路线旁证 |
| F | **体素→mesh 的通用后端** | Surface Nets（默认）/ MC/DC/MT（质量档） | `fast-surface-nets` CPU 单核 ~20M tri/s（`W15A-032`）；分块 1-voxel halo（`W15A-034/035`） | Surface Nets 会抹尖角；DC/MT 未接 | ✅ Surface Nets |

**为什么默认 Surface Nets 而不是 MC/MT**：本仓已有 CPU 参照与 GPU 实现（T-GC-02），
`fast-surface-nets` 的确定性/速度/分块契约都已验证（`W15A-032/034/035`）；
高斯侧的首要问题是"场对不对"，不是"等值面算法高不高级"。质量档（DC/MT）留到 T-GC-02 的质量档。

## 3. 实现的管线

```text
3DGS PLY / 合成场景
   │  ① prune：不透明度 / 半径 / 非法值
   ▼
② 选场：density（无法线） 或 surfel TSDF（有法线）
   │  每个高斯按 3σ 局部包围盒 splat；核宽在法线轴 ≥ 1 格
   ▼
③ 未知区封闭：从域边界泛洪 → 外部；被围住的未知区 → 内部（--hollow 可关）
   ▼
④ Surface Nets：n³ 场 → 顶点/三角形；用有符号体积校正绕序
   ▼
⑤ 统计/导出：体积、χ、连通分量、边界触碰、逐位哈希 → OBJ + JSON + HTML 预览
```

### 3.1 高斯与协方差

输入标准 3DGS 语义：`μ, scale=exp(·), rot=[w,x,y,z], opacity=sigmoid(·), f_dc`。
协方差 `Σ = R diag(s²) Rᵀ`；对称 3×3 用固定顺序/固定迭代上限的 Jacobi 分解，得到升序特征值
`λ0≤λ1≤λ2` 与特征向量。**最薄轴 `vectors[0]` 就是面元法线**；资产带 `nx/ny/nz` 时优先用资产法线。
无资产法线时，用"质心朝外"给最薄轴定向——这只对凸/近凸物体可靠，所以 `auto` 不会在无法线时自动走 surfel。

### 3.2 密度场（A）

对每个体素中心 `p`：

```text
d = p − μ
maha = Σ_k (v_k·d)² / λ_k        （v_k 为特征向量，λ 下限 max(λ, (0.75h)²)）
ρ(p) += α · exp(−½·maha)
```

阈值 `iso = frac × q99.9(ρ>0)`，默认 `frac=0.65`；场值 `f = iso − ρ`（负=内部）。
`frac` 越低 → 表面越靠外（更胖、更容易闭合）；越高 → 越紧、越容易破壳。
体素化后可选**封闭填充**：`ρ>iso` 的体素为已知，`ρ≤iso` 的未知区从边界泛洪；
连通到边界的是外部，被围住的是内部（写负值）。不封闭时输出"壳"（内外两张面）。

### 3.3 朝向面元 TSDF（B）

对每个体素中心 `p`，高斯法线 `n`，平面距离 `t = (p−μ)·n`：

```text
λ0_eff = max(λ0, (1.0h)²)         （法线方向核宽下限：保证窄带能被格点采到）
maha   = Σ_k (v_k·d)² / λ_k_eff
w      = α · exp(−½·maha)
num   += w·t ;  den += w
f(p)   = num/den  if den ≥ min_weight else 未知
```

`f=0` 即表面；未知区按 §3.2 同样的泛洪封闭。法线轴用**资产法线**而不是强行用 `vectors[0]`，
因为 2DGS 的平面法线与协方差最薄轴通常一致，但资产法线更可信。

### 3.4 网格与统计

- Surface Nets 输出数组局部坐标（单位=格），统一换算为世界坐标 `origin + p·h`。
- 用有符号体积校正绕序；法线取 SDF 中心差分结果并按同一符号翻转。
- 统计：体积（发散定理）、欧拉特征 χ=V−E+F、连通分量（并查集）、边界触碰（距域边界 <1.5h）、
  逐位哈希（FNV-1a over f32 bits + indices）。
- 解析对拍：球 `χ=2`，环面 `χ=0`（亏格 1）；体积误差见 §6。

### 3.5 默认值与三旋钮

| 旋钮 | 默认 | 语义 / 后果 |
| --- | --- | --- |
| `--grid N` | 64 | 体素分辨率；N³。128³ 体素数 ×8，时间约 ×8；薄特征/小孔建议 128 |
| `--threads N` | 0（自动） | splat 线程数；0=`available_parallelism()`，1=逐位参照。z-slab 分块，无原子/锁 |
| `--gpu` | 关 | GPU splat+iso+密封（定点整数原子）；失败回退 CPU；忽略 `--threads`；单次转换受设备 init 影响 |
| `--gpu-mesh` | 关 | GPU Surface Nets 实验档；64³ 热态 13–15 ms、128³ 96–183 ms（CPU 4/21 ms），通常更慢 |
| `--bins3d` | 关 | CPU splat 走 3D tile 分桶（20k 1.14–1.25×）；默认 z-slab；GPU 忽略 |
| `--simd-prep` | 关 | 可选 AVX2 cyclic eigen prep；默认标量逐位；opt-in 改哈希；当前规模无端到端收益 |
| `--largest-component` | 关 | 只保留三角形数最多的连通分量；去 floaters/碎片，但会删掉真实小物体 |
| `--prune-opacity` | 0.05 | 丢低不透明度 floaters；太高会削掉真实薄结构 |
| `--iso` | auto（0.65×q99.9） | 密度阈值；**低=胖/易闭合，高=紧/易破壳**；surfel 忽略 |
| `--padding` | 3 | 域边界留白格；覆盖 3h 采样带宽，防止表面被截断 |
| `--hollow` | 关 | 不做未知区封闭；输出内外双面的壳（体积参考不适用） |
| `--min-radius` / `--max-radius` | 0（关） | 按 3σ 半径剪枝；可去背景雾团/超大板 |
| `--raw-scale` / `--raw-opacity` | 关 | PLY 的 scale/opacity 已激活时使用 |

### 3.6 并行口径（多线程 splat）

按 z 层切块（`chunks = min(n, workers×2)`），每块独占自己的输出切片；worker 只写本块，
体素累加**无原子、无锁**；取块用 `AtomicUsize` 动态领取，避免各 z 块成本不均时的尾部等待。
每个体素的 Gaussian 累加顺序与单线程完全相同（外层仍是 prepared 数组顺序），
所以 `threads=N` 与 `threads=1` 的场值与网格哈希**逐位一致**
（`tests/gauss_leg.rs::parallel_splat_matches_single_thread_bitwise`）。
`--threads 1` 保留为逐位参照；`--threads 0`（CLI 默认）= `available_parallelism()`。
代价：每块分配 `n²×块高` 的局部数组；256³ 时约 75–150 MB，量产仍需稀疏分块。

**AVX2 density 行内核（默认开启，逐位一致）**：x86_64 + AVX2 时一次算 8 个体素的
Mahalanobis；点积/除法的运算顺序与标量 `max_maha` 逐项一致（无 FMA/无重排），
exp 与累加仍按 x 升序标量执行 ⇒ 场与网格哈希不变（单元测试 + 既有 hash 判据）。
surfel 的 A/B 无收益，保持标量；`LSSMJ_NO_AVX2=1` 可强制标量做对拍。

### 3.7 GPU splat（定点整数原子；`--gpu`）

GPU 侧不直接用浮点原子（WGSL 无 `atomicAdd<f32>`，且浮点归约顺序不可复现），而是：

1. **CPU 建 job 表**：每个 prepared 高斯算 3σ bbox，前缀和成 `offsets`；一个 job = 一个
   (Gaussian, voxel)，GPU 线程用二分查回高斯下标。表大小 = Σ bbox 体素数（64³/20k 高斯约 15M）。
2. **GPU 整数原子**：density 把权重 `w·scale` 量化成 u32；surfel 把 `w·dist` 量化成 i32、
   `w` 量化成 u32，分别 atomicAdd。整数加法可交换/可结合 ⇒ **与执行顺序无关，逐位确定**。
3. **GPU iso（density）**：stats（atomicMax + 正密度计数）→ 直方图（`step = max/65536 + 1`，
   65536 桶在 `counters_hist[8..]`）→ 回读直方图，CPU 做前缀和选 q99.9（`round((positive-1)·0.999)`），
   `iso = iso_abs.unwrap_or(frac · q99.9)`。q99.9 取桶下界，误差 ≤ step（约为 max 的 1/65536）。
4. **GPU 密封**：`seal_init` 把与域边界相连的未知体素写成种子；之后**全格 ping-pong 泛洪**——
   每趟只读 `mask_in`、只写 `mask_out`（两份 n³ u32），端点 `known` 的体素恒为 0。
   `min(n,64)` 趟编码进一个 command buffer；每趟先 `clear_buffer` 清零"有变化"标志，批次末趟无变化即到不动点；
   最多 `2n` 趟，未收敛则显式报错。`seal=false` 跳过泛洪，未知区一律 +outside。
   掩码最终用于 field pass：外部未知区 +outside，被围住的内部未知区 −outside。
5. **GPU 写场 + 一次回读**：density/surfel 各一个 field 入口写 f32 场并统计 observed/sealed；
   回读后交给 CPU Surface Nets。GPU 已覆盖 splat + iso + 密封，只剩网格化在 CPU。

量化尺度：`DENS=2^16`、`DEN=2^16`、`DIST=2^14`；在 20k 量级/常规场景内不溢出 u32/i32。
整数原子 + ping-pong 掩码使 GPU 路径自身逐位确定；与上一版 frontier 实现相比，四个 CLI 场景的
网格哈希完全相同（`0d1d…`/`a36f…`/`89a8…`/`27ad…`，见 §6.2），即泛洪重写是**逐位等价**的。
代价：GPU 场与 CPU f32 场**不是逐位相等**（量化 + WGSL `exp` 差异）；判据看 GPU 两次逐位一致 +
与 CPU 的体积/χ 容差（`tests/gauss_gpu.rs`）。surfel 场最大差 ~2e-2（低权重体素在 `min_weight`
附近翻转观测判定）；density 的原始场在 iso 边界会从 0 翻到 `+outside`，因此密度判据看 iso 差 + 网格。

缓冲面：post 的 mask/计数器与 splat 输出缓冲不在 CPU 侧 `vec![0u8]` 上传，而是
`create_buffer` 后用 `enc.clear_buffer` 在 GPU 清零（field 缓冲由 field pass 全写，不初始化）。
128³ 的 post setup 从 26–32 ms 降到 5–6 ms，见 §6.12。

存储面：默认 `max_storage_buffers_per_shader_stage = 8`，后处理把"计数器 + 直方图"合并成
一个 `counters_hist`（0..5 计数，8.. 直方图），避免第 9 个 storage buffer；两份 n³ u32 掩码在
128³ 占 16 MB、256³ 占 128 MB，仍是稠密路径的固有成本。

**设备 init 是单次转换的主要成本**：本机 Vulkan-first 初始化 ~0.42–0.45 s（全后端枚举 ~0.87 s），
而 48³/4k 高斯的 kernel+后处理只有 3–6 ms。因此 `--gpu` 默认关闭；`voxelize_gpu_with(&Headless, …)` 供批处理复用设备；
单次 64³/128³ 目前 CPU 24T 更快（见 §6.2）。

### 3.8 Gaussian 分桶 + 并行 prep（128³ 档）

两处只改“做多少”，不改“怎么算”：

1. **并行 prep**：先按输入顺序过滤有效高斯，再分块并行 `eigen()`/normal/λ/radius；
   拼回顺序与单线程相同，prepared 数组逐位一致。`n < 2048` 或 `threads=1` 时仍走顺序。
2. **z 块 bin**：每个 z 块预建“与它相交的 Gaussian 下标”表（按 prepared 顺序）；
   splat 块内只遍历 bin，不再扫全部高斯。每个体素的累加顺序不变 ⇒ 场/网格哈希不变。

微基准（100k 高斯，128³，48 块）：

| 项 | 旧 | 新 | 变化 |
| --- | --- | --- | --- |
| 每块扫全部（仅范围检查） | 73.4 ms | 建桶 2.5 ms | 少做约 29× |
| prepare density | 8.7 ms（1T） | 6.5 ms（24T） | 1.34× |
| prepare surfel | 9.0 ms（1T） | 5.7 ms（24T） | 1.58× |

端到端 128³/20k（3 次中位数）：球 166.4→**141.1 ms**、blobs 402.9→**367.5 ms**；
球/环面/blobs/PLY/GPU 的所有既有哈希不变（`cd8f…`、`ab6d…`、`13af…`、`c66e…`、`865a…`）。
分桶只按 z；x/y 仍是单高斯全 bbox。另提供 `--bins3d` 3D tile 可选档（默认关，默认仍 z-slab；见 §6.4）。

### 3.9 可选 SIMD prep（`--simd-prep`，默认关）

`gauss/simd.rs`：固定序列 cyclic Jacobi（(0,1)(0,2)(1,2) × 5 sweeps）+ 8 路 AVX2；
非 x86_64 或无 AVX2 时走标量 cyclic。默认仍是自适应 Jacobi 的标量逐位路径。

- 判据：`eigen_batch` 对自适应 eigen 的特征值容差 1e-6；`simd_prep_close_to_scalar` 对拍
  volume/χ/iso（surfel 的 mesh 哈希允许变，density 在测试场景哈希相同）。
- 实测 100k 高斯：`eigen_batch` 4.6 ms vs 自适应 6.3 ms（**1.37×**）；但完整 prep
  SIMD 5.4 ms vs 24T 标量 5.4 ms（density）、6.0 vs 5.3 ms（surfel）——现有 24T 标量已在
  噪声底，**当前规模没有端到端收益**。128³/20k vox 球 154.6→153.0 ms、blobs 384.6→371.4 ms
  （噪声/3% 内），也说明 prep 不是瓶颈。
- 因此保留为 opt-in 实验档（默认关，不推荐当前规模使用）；若后续 1M 高斯/融合 covariance+eigen
  的 AVX2 prep 有实测收益，再考虑转正。

### 3.10 GPU Surface Nets 实验档（`--gpu-mesh`，默认关）

复用仓内已有的 `crate::gsn`（T-GC-02，逐语义复刻 fast-surface-nets 0.2.1）：

- `field_to_mesh_gpu` 上传 `field.values`，GPU 生成稀疏槽位顶点与四边形；宿主侧先按
  **槽位升序** 压缩顶点（与 GPU atomicAdd 的写入序无关），再把三角形旋转到最小索引开头、
  按字典序排序，最后算体积/χ/哈希 ⇒ 同输入逐位一致。
- 绕序由有符号体积校正；法线用面积加权的面法线（与 CPU 版的 SDF 梯度法线不同）。
- 判据（48³，同一 GPU 场）：三角形数、χ、分量、顶点数与 CPU 完全一致，体积相对差 ~1e-6；
  两次 GPU 网格哈希相同。64³/128³ 对拍也保持三角形数/χ 一致（128³ 体积差 ~1e-6）。
- **索引缓冲精确分配**：`gsn` 以前按"每 cell 3 四边形"预分配 18·n³ 字节，128³ = 144 MB，
  超过 wgpu 默认 128 MB binding 上限。现在宿主先按与 shader 相同的判据（三条轴边 + 同样
  边界限制）数一遍 sign-changing edge，再按实际四边形数分配：128³ 球降到 ~1.6 MB
  （density 139,248 quads）/ ~3.3 MB（surfel 135,840 quads），128³ 可运行；
  GPU 计数与 CPU 预算不一致时显式报错。
- **仍不推荐**：64³ 热态 13–15 ms（CPU fast-surface-nets 3.7–3.8 ms），首跑还含管线编译
  ~460 ms；128³ 热态 96 ms（density）/ 183 ms（surfel），CPU 约 21 ms。瓶颈变成 gsn 的
  n³ 稀疏槽位读回（pos 16 B/cell、flag 4 B/cell）与全格顶点生成；256³ 的 pos/flag 缓冲
  268 MB/67 MB 仍会超过 128 MB binding 上限，`field_to_mesh_gpu` 显式返回 Err。
- 要转正需要"场留在 GPU + GPU compaction/scan + 只回读压缩结果"的一整片改造；
  当前保留为库侧实验入口与 `--gpu-mesh` opt-in，默认 CPU 网格不变。

## 4. 关键工程判决（每条都有实测或明确理由）

1. **padding=3 而不是 1**：数据包围盒按 3σ 取，但 clamped 核在法线方向还要伸出 3h；
   留 1 格会被截断。实测球/环面 `boundary_touch=false`。
2. **未知区必须封闭**：TSDF 只在表面 ±3h 有值；未观测内部若直接写 +outside，会在内边界再造一个零交叉。
   首跑球（未封闭）：χ=4、2 分量、体积 0.59646（相对误差 66.2%）；加泛洪封闭后：
   χ=2、1 分量、体积 1.77436（0.41%）。**"先跑出坏结果再定位"比事后辩解有用**。
3. **auto 只在有法线时走 surfel**：原始 3DGS 的高斯无组织、无法线（`W15A-003`），
   强行 surfel 会翻面/漏面；density 是保守档，surfel 是资产质量档。
4. **density 的 auto 破壳重试**：全局 iso 对非均匀密度不鲁棒。检测 `sealed_interior==0 && χ<0`
   后按 ×0.75、×0.5 重试，选第一个 `sealed>0 且 χ≥2` 的结果；日志/JSON 明示。
   实测 3DGS PLY：16.04 破壳（χ=-34）→ 12.03 闭合（χ=2，体积高 20.9%）。
5. **surfel 空网格回退 density**：auto 下 surfel 出错/空/体积≤0 时回退，并在警告与 JSON 记录原因；
   显式 `--mode surfel` 不偷偷回退（用户的显式选择优先）。
6. **默认只显示网格**：20k 高斯点以 2.2px 叠在球面上会形成摩尔纹，肉眼看不到网格质量；
   高斯点改为按需开关，默认关。
7. **预览必须真跑**：本实现用浏览器 WebGL2 渲染后 `readPixels` 验证（GL error=0、非背景像素/ASCII 轮廓），
   不是"HTML 写出来了就算完成"。

## 5. UX 设计（工具面）

### 5.1 三个命令

```bash
# 先诊断：格式、高斯数、包围盒、3σ 半径/不透明度分位、各向异性、剪枝预演、auto 建议
cargo run --release --bin gauss2mesh -- inspect scene.ply

# 合成场景：sphere / torus / blobs（blobs 自带 300 个低不透明度 floaters）
cargo run --release --bin gauss2mesh -- demo sphere
cargo run --release --bin gauss2mesh -- demo torus
cargo run --release --bin gauss2mesh -- demo blobs

# 真实 PLY：默认写 <input>.obj/.html，--json 另写报告
cargo run --release --bin gauss2mesh -- convert scene.ply --grid 128 --mode auto

# 批处理：同一进程复用一次 GPU 设备
cargo run --release --bin gauss2mesh -- batch a.ply b.ply --out-dir out/batch --gpu
```

### 5.2 阶段进度（实跑输出）

```text
[1/5] 读入     synthetic · 20,000 高斯 · 2.6 ms
[2/5] 剪枝     20,000 保留 · 丢 0（低不透明度 0 / 半径 0 / 非法 0）· 0.4 ms
[3/5] 体素化   surfel · 64³ · h=0.02655 · iso=0.00000 · threads=24 · 观测 73,964 · 封闭内部 62,176 · 47.5 ms · auto→surfel：资产带法线
[4/5] 网格     Surface Nets · 15,074 顶点 · 30,144 三角形 · χ=2 · 1 分量 · 3.7 ms
[5/5] 写出     out/mt_sphere.obj · out/mt_sphere.html · out/mt_sphere.json · 27.6 ms

体积 1.77436 · χ=2 · 分量 1 · 64³ · surfel · 83.3 ms 总计
哈希 cd8f000dba7bfb4e · 导出 ...
```

GPU（`--gpu`；设备 init 单独计入总计）：

```text
[3/5] 体素化   surfel · gpu (init 436 ms) · 64³ · h=0.02655 · iso=0.00000 · 观测 73,959 · 封闭内部 62,176 · 20.3 ms · auto→surfel：资产带法线
[4/5] 网格     Surface Nets · 15,074 顶点 · 30,144 三角形 · χ=2 · 1 分量 · 4.5 ms
体积 1.77430 · χ=2 · 分量 1 · 64³ · surfel · 472.1 ms 总计（含 init）
哈希 0d1d9007e49ffa7e · 导出 ...
```

每行回答一个用户问题：**读到了什么 / 丢了什么 / 用了什么场和阈值 / 网格质量如何 / 产物在哪**。
`--quiet` 只留最后三行，便于脚本/CI。

### 5.3 可解释的降级（不是静默调参）

| 触发 | 行为 | 记账 |
| --- | --- | --- |
| auto → surfel，但空网格/体积≤0 | 回退 density 重跑 | `⚠ surfel 失败（…）→ density`；JSON `fallback` |
| auto → density，`sealed=0 && χ<0` | iso ×0.75 / ×0.5 重试，选闭合结果 | `⚠ 密度阈值自动重试…`；JSON `iso_retry` |
| 网格触及域边界 | 不自动改，报"边界截断" | 警告 + JSON `boundary_touch` |
| 连通分量 >1 | 不自动合并，报 floaters/开放面/hollow | 警告 + JSON `components` |
| 体积=0 / 无三角形 | 非零退出 + 可执行建议 | stderr，退出码 1 |

### 5.4 JSON 报告字段

`mode_requested / mode_used / auto_reason / fallback / gpu_fallback / iso_retry / has_normals / flatness_p50 /
gaussians{input,kept,dropped_*} / grid{n,h} / field{mode,backend,bins3d,simd_prep,iso,threads,observed,sealed_interior} /
mesh{vertices,triangles,components,chi,volume,boundary_touch,hash,bounds} /
timings_ms{load,prune,voxel,mesh,write,gpu_init} / warnings[]`。

### 5.5 自包含 WebGL 预览

- 一个 HTML 内嵌 mesh（positions/normals/indices）、高斯点（positions/colors）与 stats；无外部 CDN/JS。
- 数组用 **base64 二进制**（f32 x3 / u32 LE）嵌入，不是 JSON 数字数组；128³/68k 顶点时 HTML
  从 6.79 MB 降到 5.00 MB，写盘段 86.8→47.2 ms。查看器仍兼容旧的 JSON 数组载荷。
- 轨道相机：拖拽旋转、滚轮缩放、重置/正视；网格/高斯点/线框/自动旋转开关。
- 默认只显示网格；点云/线框按需开。
- 预览校验（本机）：WebGL2 画布 960×640，`gl.getError()=0`；二进制版同步 `render()` 后
  `readPixels` 得到 77,591 个非背景像素（旧 JSON 版 77,586），并把画布降采样成 ASCII 轮廓确认形状。
- 数据级回归：`tests/html_embed.rs` 校验 base64 长度与 stats 一致、首顶点与 OBJ 对齐、旧数组格式不再出现。

### 5.6 批处理（batch）

```bash
gauss2mesh batch a.ply b.ply --out-dir out/batch --gpu
```

- 同一进程复用 `OnceLock<Headless>`：首个文件付 init，后续 stage 显示 `gpu (cached)`。
- 每文件写 `<out-dir>/<stem>.obj/.json`（默认另写 HTML）；重名 stem 自动 `_2`；
  坏文件不阻断后续，最终退出码非零。
- `--out/--html/--json` 是单文件选项；batch 统一用 `--out-dir`。
- 实测（两份 PLY）：独立 GPU 进程 1424.0 + 1162.8 = **2586.8 ms**（init 1015 / 889）；
  一次 batch **1233.0 ms**（init 911 / 0），省 **1353.8 ms**。CPU 两文件 batch 188.1 ms。

## 6. 本机实测（2026-10-20，release；下表为 1T 参照）

环境：Windows 10.0.28000 x64；`rustc 1.99.0 (b940084d7 2026-09-28)`；
CPU Intel Core Ultra 7 270K Plus；GPU 未参与本轮（体素化/网格化均 CPU）。

| 用例 | 场 | n | 高斯 | 观测/封闭 | 顶点/三角 | χ | 分量 | 体积 | 解析 | 相对误差 | 总时间 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 球（合成，带法线） | surfel | 64 | 20,000 | 73,964 / 62,176 | 15,074 / 30,144 | 2 | 1 | 1.77436 | 1.76715 | **0.41%** | 390 ms |
| 环面（合成，带法线） | surfel | 64 | 19,800 | 44,936 / 7,480 | 9,192 / 18,384 | **0** | 1 | 0.52757 | 0.52546 | **0.40%** | 181 ms |
| blobs（合成，各向同性+floaters） | density | 64 | 12,000（剪 300） | 17,952 / 42,201 | 11,158 / 22,312 | 2 | 1 | 1.99052 | 1.76715 | 12.6% | 510 ms |
| 球 PLY（带法线） | surfel | 64 | 20,000 | 68,057 / 53,291 | 13,860 / 27,716 | 2 | 1 | 1.77411 | 1.76715 | **0.39%** | 362 ms |
| blobs PLY（无法线） | density | 64 | 12,000 | 23,697 / 34,924 | 11,074 / 22,144 | 2 | 1 | 2.13619 | 1.76715 | 20.9% | 925 ms（含重试） |
| 球 `--hollow` | surfel | 64 | 20,000 | 73,964 / **0** | 26,476 / 52,944 | 4 | 2 | 0.61300 | 1.76715 | 65.31%* | 371 ms |

\* `--hollow` 输出内外双面的壳，体积是"外球减内球"，与实心球解析体积不可比；这里列出是为了证明封闭填充的必要性。

**首跑的设计错（留档）**：球在**未做未知区封闭**时：χ=4、2 分量、体积 0.59646（相对 66.2%）。
根因不是 Surface Nets，而是"窄带外的未知内部被写成 +outside"，内边界产生第二个零交叉。
修复=边界泛洪封闭（§3.2/3.3）；修复后 0.41%。

**确定性**：同输入重复运行的网格哈希一致；球 `cd8f000dba7bfb4e`、环面 `34c7846b7b85a520`、
blobs `13af4cee910c6343`（哈希随实现/默认值变化，不能跨版本当契约）。
按 PLY 顺序累加；多线程 z-slab 不改变每体素的累加顺序，1T 与 N 线程逐位一致
（`parallel_splat_matches_single_thread_bitwise`）。

### 6.1 多线程 splat（z-slab，24 线程）

| 用例 | vox 1T | vox auto | 加速 | total 1T | total auto | 哈希 |
| --- | --- | --- | --- | --- | --- | --- |
| 球 surfel | 360.8 ms | **42.2 ms** | **8.55×** | 375.2 ms | **58.4 ms** | 同 `cd8f…` |
| 环面 surfel | 161.8 ms | **29.0 ms** | **5.58×** | 173.1 ms | **39.5 ms** | 同 `34c7…` |
| blobs density | 472.2 ms | **53.7 ms** | **8.79×** | 484.1 ms | **66.6 ms** | 同 `13af…` |

n=64，3 次中位数；`field.threads=24`。体素化之外的 load/prune/mesh/write 约 13–17 ms，
所以总加速低于体素加速。1T 与 auto 的网格哈希、体积、χ 完全一致；`threads=1` 保留为逐位基线。

在 3.8 的分桶 + 并行 prep 之后，128³/20k 的端到端：球 166.4→**141.1 ms**、blobs 402.9→**367.5 ms**，
既有哈希全部不变。

### 6.2 GPU splat + iso + 密封（`--gpu`，定点整数原子）

| 场景 | 后端 | init | vox（splat+iso+密封+回读） | total | 体积 / χ / GPU 哈希 |
| --- | --- | --- | --- | --- | --- |
| 64³ / 20k 球 | CPU 24T | — | 45.1 ms | 60.3 ms | 1.77436 / χ=2 |
| 64³ / 20k 球 | GPU | 1129.6 ms | **17.2 ms** | 1163.1 ms | 1.77430 / χ=2 / `0d1d…` |
| 64³ / 12k blobs | CPU 24T | — | 54.6 ms | 67.0 ms | 1.99052 / χ=2 |
| 64³ / 12k blobs | GPU | 980.7 ms | **18.6 ms** | 1012.0 ms | 1.99052 / χ=2 / `a36f…` |
| 128³ / 20k 球 | CPU 24T | — | 145.2 ms | 202.4 ms | 1.77618 / χ=2 |
| 128³ / 20k 球 | GPU | 1051.3 ms | **103.0 ms** | 1213.7 ms | 1.77607 / χ=2 / `89a8…` |
| 128³ / 12k blobs | CPU 24T | — | — | 367.5 ms（§3.8） | — |
| 128³ / 12k blobs | GPU | 921.5 ms | **73.0 ms** | 1036.7 ms | 2.02660 / χ=2 / `27ad…` |

- **后处理全部在 GPU**（本切片）：iso 直方图 + 全格 ping-pong 泛洪 + field 写入；CPU 只做直方图前缀和
  与最终 Surface Nets。泛洪本身不是瓶颈：同一场景 `--hollow`（跳过泛洪）vs 密封，64³ density
  9.5→10.5 ms、128³ density 34.1→35.0 ms、128³ surfel 94.2→103.4 ms（`bench_gpu_scenes`）。
- **逐位等价**：重写泛洪后四个 CLI 场景网格哈希与 frontier 版完全相同（上表 `0d1d…`/`a36f…`/`89a8…`/`27ad…`），
  GPU 两次运行也逐位一致；CPU 与 GPU 哈希仍不同（定点量化 + WGSL `exp`）。
- **设备复用**（`voxelize_gpu_with(&Headless, …)`，48³/4k 探针）：surfel 首 7.6 ms、次 4.5 ms；
  density 首 6.1 ms、次 4.5 ms；设备 init 937.4 ms（当时全后端枚举；Vulkan-first 后见 §6.8）。
- **batch 设备复用**（两份 PLY）：独立 GPU 进程 1424.0 + 1162.8 = 2586.8 ms（init 1015/889）→
  一次 batch 1233.0 ms（init 911/0），省 **1353.8 ms**；第二文件 stage 为 `gpu (cached)`。
- **CPU 对拍（48³/4k）**：surfel 体积差 3e-5、场最大差 0.0197；density 体积差 5.7e-4、iso 同值、
  χ 同为 -184；`--hollow` 分支 `sealed_interior=0` 且与 CPU 壳体积差 5.7e-4。
  density 原始场最大差 7.18 是 iso 边界体素在"0 ↔ +outside"之间的密封翻转（见 §3.7），不作判据。
- **结论**：GPU 体素化+后处理已全面快于 24T CPU（64³ 17–19 vs 45–55 ms；128³ 球 103 vs 145 ms、
  blobs 73 vs ≈368 ms），但单次转换的设备 init（Vulkan-first ~0.44 s）仍让端到端更慢；默认 CPU，
  `--gpu` 留给批处理/更高分辨率。下一步仍是设备/管线生命周期服务化，或直接做稀疏分块降低 256³ 的掩码/场内存。
  完整 CPU/GPU 对比（含 CPU 1T/24T、GPU 网格与单次 init）见 §6.7。

### 6.3 后处理：q99.9 改 k-th 选择

密度阈值原来对全部正密度 `sort_by` 取 99.9 分位（O(n log n)）。改为
`select_nth_unstable_by` 的 k-th 顺序统计量（O(n)），结果与排序后同一下标逐位相同：

| 元素数 | 全排序 | k-th 选择 | 加速 |
| --- | --- | --- | --- |
| 2,000,000 | 34.20 ms | **1.98 ms** | **17.3×** |

microbench 在 `gauss::voxel::bench_post::bench_quantile_select`（`#[ignore]` 诊断项）。
q999/iso/网格哈希不变；128³ density 的排序段不再是瓶颈。密封遍历试过 `Vec<u32>` DFS，
128³ 球无收益（略慢/噪声内），已回退原 BFS。

**耗时结构**：多线程后体素化仍是大头（球 42 / 58 ms ≈ 72%；blobs 54 / 67 ms ≈ 80%）；
1T 参照为 356 / 390 ms。Surface Nets 本身 2–7 ms，不是瓶颈；GPU 化应先做体素化 splat，而不是先换 mesher。

### 6.4 3D tile 分桶（`--bins3d` 可选档，默认关）

8³ tile、每 tile 独占输出、按 prepared 顺序建桶；与现役 z-slab 逐位对拍（128³）：

| 用例 | z-slab | 3D tile | 加速 | 逐位 |
| --- | --- | --- | --- | --- |
| 20k density | 94.6 ms | 83.1 ms | 1.14× | ✓ |
| 20k surfel | 135.3 ms | 108.5 ms | 1.25× | ✓ |
| 100k density | 102.0 ms | 99.1 ms | 1.03× | ✓ |
| 100k surfel | 142.6 ms | 135.9 ms | 1.05× | ✓ |

20k 有 14–25% 收益；100k 被 3D 桶构建与更多 bin entries 吃掉，只剩 3–5%，
所以**不设为默认**，而是作为 `--bins3d` 可选档（默认仍 z-slab）。注意：在 z-slab 改成
动态取块后，20k 的 3D tile 优势已基本消失（全套基准 0.99×/0.99×，100k 0.96×/1.06×，
见 §6.13），更确认保持可选档。动态取块前的 64³/128³/12–20k 复测：
球 64 43.0→33.4 ms、blobs 64 58.6→43.5 ms、球 128 147.9→128.6 ms、blobs 128 371.7→308.9 ms
（hash 不变）；仍维持可选档决策。生产实现
`src/gauss/voxel/tiles.rs`，对照 bench 在 `src/gauss/voxel/bench_tiles.rs`（`#[ignore]`）。
端到端 128³/20k（3 次中位数，vox）：球 141.1→**126.9 ms**、blobs 351.4→**308.1 ms**，
网格哈希不变（`ab6d…` / `c66e…`）。

### 6.5 SIMD prep 试验（`--simd-prep`，默认关）

| 项（100k 高斯） | 标量/24T | SIMD | 变化 |
| --- | --- | --- | --- |
| eigen_batch（1T vs 自适应 1T） | 6.3 ms | 4.6 ms | 1.37× |
| prepare density | 5.4 ms | 5.4 ms | 1.00× |
| prepare surfel | 5.3 ms | 6.0 ms | 0.88× |

eigen 内核确实快了 1.37×，但完整 prep 已被 24T 标量压到 5–6 ms，SIMD 并行分块后与标量持平/略慢；
128³/20k 的端到端差异在噪声/3% 内。结论：**当前规模不推荐**，默认标量逐位不变；`--simd-prep`
保留为实验档。surfel 的 mesh 哈希会因 eigen 末位差异改变，density 在测试场景不变。

### 6.6 GPU Surface Nets 试验（`--gpu-mesh`，默认关）

同一 GPU 场（48³/4k）上，GPU 网格与 CPU fast-surface-nets 的三角形数、χ、分量、顶点数
完全一致，体积相对差 ~1e-6；两次 GPU 网格哈希相同（槽位升序压缩 + 三角形规范排序）：

| n / 场景 | CPU 网格 | GPU 网格（热态） | GPU 网格（首跑） | 结果 |
| --- | --- | --- | --- | --- |
| 48³ / 4k surfel | — | — | — | tri 14,880 一致，χ=2，vol 差 1e-6 |
| 64³ / 20k density | 3.7 ms | **13.3 ms** | 460 ms | tri 31,584 一致，χ=2 |
| 64³ / 20k surfel | 3.8 ms | **15.3 ms** | — | tri 30,144 一致，χ=2 |
| 128³ / 20k density | 21.4 ms | **95.6 ms** | — | tri 139,248 一致，χ=2，vol 差 2e-6 |
| 128³ / 20k surfel | 20.9 ms | **182.8 ms** | 111.5 ms（CLI 单次） | tri 135,840 一致，χ=2，vol 差 1e-6 |

索引缓冲改为**按 CPU 预数的实际四边形数精确分配**（此前最坏 18·n³ = 144 MB @128³，
超过 128 MB binding 上限；精确分配后 128³ 球仅 ~1.6–3.3 MB）。剩余成本是 gsn 的 n³
稀疏槽位：pos 16 B/cell、flag 4 B/cell 的全格生成与回读；256³ 仍会超限并显式回退。
当前 gsn 形态不适合接到本管线，**不推荐**；保留 `--gpu-mesh` 作为实验入口，默认 CPU 网格不变。

### 6.7 完整性能对比（2026-10-20，3 次中位数）

口径：64³/128³；球 = 20k surfel，blobs = 12k density（+300 floaters，先按 CLI 默认剪掉）；
每项 3 次取中位数；GPU 复用同一设备，GPU 网格先预热一次；`vox+mesh` 不含 load/prune/write
与设备 init。复算：`tests/perf_compare.rs`（ignored 诊断）。

| 场景 | CPU 1T | CPU 24T | GPU（CPU 网格） | GPU + GPU 网格 |
| --- | --- | --- | --- | --- |
| 64³ 球 | 356 | 46 | 19 | 28 |
| 64³ blobs | 475 | 60 | 22 | 42 |
| 128³ 球 | 1461 | 179 | 79 | 135 |
| 128³ blobs | 3920 | 448 | 66 | 146 |

分解（同一批中位数）：

| 项 | 64³ 球 | 64³ blobs | 128³ 球 | 128³ blobs |
| --- | --- | --- | --- | --- |
| GPU vox（复用） | 14.6 | 18.4 | 54.4 | 45.2 |
| CPU 24T vox | 41.6 | 57.0 | 157.8 | 427.3 |
| CPU 网格 | 4.0 | 3.3 | 21.6 | 18.4 |
| GPU 网格 | 14.2 | 22.9 | 89.3 | 107.0 |

- **热态 GPU 全面快于 CPU 24T**：vox 段 2.8–9.5×；但 128³ 球的 GPU vox 在 42–116 ms 间波动
  （读回/map/poll 抖动），表中取中位数，三轮结论一致。
- **GPU 网格从未胜过“GPU vox + CPU 网格”**：网格段慢 3.5–6.9×，整腿 28 vs 19、42 vs 22、
  135 vs 79、146 vs 66 ms；`--gpu-mesh` 保持实验档（§3.10/§6.6）。
- CPU 24T 对 1T：7.7× / 7.9× / 8.2× / 8.8×（球 64、blobs 64、球 128、blobs 128）。
- 表中 CPU 24T 是静态轮转读数；动态取块（§6.10）同轮 A/B 再快 11–31%（64³ 更明显）。
- GPU 行是 GPU clear 优化前的读数；clear 后 GPU vox 128³ 降到 ~26 ms、64³ ~14 ms（§6.12）。

**单次 CLI（含设备 init，1 次代表性读数）**：

| 场景 | CPU 24T 转换 / 总计 | GPU 转换 / 总计 | GPU+网格 转换 / 总计 |
| --- | --- | --- | --- |
| 64³ 球 | 46.7 / **57.1** | 24.4 / 472.1 | 37.4 / 503.1 |
| 64³ blobs | 61.8 / **70.5** | 22.4 / 482.3 | 34.2 / 476.3 |
| 128³ 球 | 168.0 / **200.9** | 89.7 / 559.9 | 145.7 / 610.1 |
| 128³ blobs | 388.1 / **413.1** | 72.8 / 520.8 | 138.0 / 588.7 |

init 读数 420–453 ms（Vulkan-first；全后端枚举 ~0.87 s，A/B 见 §6.8）。热态的 GPU 优势在
单次转换里仍被 init 吃掉，所以默认仍 CPU；GPU 的收益场景是 `batch`/`voxelize_gpu_with` 的设备复用。
CPU 行是默认 z-slab；`--bins3d` 在 12–20k 实测快 15–35%（hash 不变），但按既定决策仍为可选档（§6.4）。

### 6.8 Vulkan-first 设备初始化（2026-10-20）

本机默认适配器是 NVIDIA RTX 4060 Ti (Vulkan)，但 wgpu 默认 Instance 会枚举全部后端；
单次 CLI 的 `gpu_init` 为 590–1109 ms。现在先只建 Vulkan Instance，无适配器再回退全后端
（macOS/无 Vulkan 平台仍可用）。A/B 同机、128³ 球、各 4 次交错：

| 后端 | init（ms，4 次） | 中位数 |
| --- | --- | --- |
| Vulkan-only | 600 / 446 / 414 / 426 | **436** |
| 全后端 | 882 / 901 / 861 / 869 | 870 |

转换耗时与四个网格 hash 不变；单次 GPU 总计 64³ 球 1016→**472 ms**、128³ 球 720→**560 ms**。
仍慢于 CPU auto（48/188 ms），但 `batch` 首个文件少付 ~0.43 s，后续文件受益更多。

### 6.9 HTML 二进制载荷（2026-10-20）

自包含预览从"逐浮点写 JSON 数组"改为 base64 二进制（f32 x3 / u32 LE）；查看器用
`atob`→TypedArray 解码，保留旧 JSON 数组回退。128³/20k 球（68k 顶点 / 135,840 三角形）：

| 项 | 旧（JSON 数组） | 新（base64） | 变化 |
| --- | --- | --- | --- |
| HTML 文件 | 6.79 MB | **5.00 MB** | −26% |
| 写盘段（OBJ+HTML+JSON） | 86.8 ms | **47.2 ms** | −46% |
| CLI 总耗时 | 245.0 ms | **221.0 ms** | −10% |
| 浏览器 `readPixels` 非背景像素 | 77,586 | 77,591 | 一致（±抗锯齿） |

载荷是平台原生端序的 f32/u32；当前目标全为 little-endian。旧 HTML 的 JSON 数组格式仍能打开。

### 6.10 动态 z 块调度（2026-10-20）

把静态轮转（`for i in (w..len).step_by(workers)`）换成 `AtomicUsize` 动态取块；只改调度，
不改每体素的 Gaussian 累加顺序。A/B 同轮交替、各 4 次中位数（vox 段，ms）：

| 场景 | 静态 | 动态 | 变化 |
| --- | --- | --- | --- |
| 64³ 球 | 52.9 | **36.3** | −31% |
| 64³ blobs | 77.4 | **54.7** | −29% |
| 128³ 球 | 176.2 | **156.5** | −11% |
| 128³ blobs | 468.2 | **403.8** | −14% |

（绝对值受同轮系统负载影响，只看 A/B 比值。）逐位测试 9/9 通过；`--threads 1` 路径不变。

### 6.11 最大连通分量过滤（`--largest-component`）

按三角形边做并查集，取三角形数最多的分量（同数取最小根 ⇒ 确定性），压缩顶点并重算
体积/χ/哈希。两个分离球的合成场（各 2k 高斯、64³、density）：

| 项 | 默认 | `--largest-component` |
| --- | --- | --- |
| 连通分量 | 2 | **1** |
| 三角形 | 9,664 | **4,832** |
| 体积 | 9.610 | **4.805（0.500×）** |
| χ | 4（两壳） | **2** |

判据在 `tests/largest_component.rs`；opt-in，默认不改变任何网格。

### 6.12 瓶颈剖析（2026-10-20，分阶段）

CPU（24T、z-slab 动态取块；ms；`LSSMJ_STAGE_DEBUG` 临时探针）：

| 场景 | prep | splat | post | vox | mesh | write |
| --- | --- | --- | --- | --- | --- | --- |
| 64³ 球 | 1.8 | 29.5 | 1.4 | 33.3 | 3.6 | 8.5 |
| 64³ blobs | 1.5 | 46.5 | 1.8 | 50.2 | 3.6 | 5.2 |
| 128³ 球 | 2.0 | 132.0 | 12.8 | 147.4 | 23.8 | 31.5 |
| 128³ blobs | 1.9 | 305.7 | 17.1 | 325.1 | 18.7 | 22.3 |

（上表是加 density AVX2 前；AVX2 后的 A/B 见下方。）

GPU（单次 CLI，init 420–474 ms；ms）：

| 场景 | prep | splat | post | vox | mesh | write |
| --- | --- | --- | --- | --- | --- | --- |
| 64³ 球 | 3.8 | 4.9 | 11.7 | 21.4 | 4.8 | 7.2 |
| 64³ blobs | 2.7 | 3.3 | 14.2 | 20.8 | 3.4 | 5.6 |
| 128³ 球 | 2.8 | 18.9 | 41.1 | 63.9 | 26.6 | 32.3 |
| 128³ blobs | 3.4 | 9.6 | 48.7 | 62.7 | 20.4 | 25.0 |

GPU post 拆解：**post-buffers 24–30 ms**（CPU 侧 `vec![0u8]` 零初始化并上传 24 MB
mask/field）、pipelines ~2–3 ms、实际计算 pass + 回读 12–15 ms。改成 GPU `clear_buffer`
（mask 只清 mask_b；field 不初始化）后：

| 项 | 修复前 | 修复后 |
| --- | --- | --- |
| post-buffers | 24.1 / 30.2 ms | **1.3 / 0.8 ms** |
| post-setup | 26.8 / 32.2 ms | **5.7 / 5.1 ms** |
| GPU vox 128³ 球 / blobs | 63.9 / 62.7 ms | **24.5–29.7 / 24.5–29.7 ms** |
| GPU vox 64³ 球 / blobs | 21.4 / 20.8 ms | **14.6 / 13.5 ms** |

四个 GPU hash 不变（`89a8…`/`27ad…`/`0d1d…`/`a36f…`）。

**当前瓶颈排序**：

1. **CPU 单次：splat 占 vox 的 88–94%**（128³ blobs 305.7/325.1 ms）。内层是
   `max_maha`（3 次除法）+ `exp` + 访存。已加 **density AVX2 行内核**（m 向量化、逐位不变）：
   同轮 A/B 128³ blobs **24T 939.8→851.1 ms（−9.4%）、1T 1105→818 ms（−25%）**；
   64³ blobs 24T 约 −6%；surfel 无收益保持标量。24T 已是最优（24>20>16>12>8），
   1T→24T 约 9×，受 P/E 核与带宽限制；再快要向量 exp（会改哈希，只能 opt-in）。
2. **GPU 单次：init 420–474 ms ≈ 85%**。转换本身只有 20–64 ms；单次无算法可救，
   收益只能靠 `batch`/设备复用。
3. **GPU 热态：CPU Surface Nets 18–27 ms 成为最大单段**（约占 vox+mesh 的 1/3–1/2）；
   GPU post 计算 + 回读 12–15 ms，splat 3–19 ms，buffer/pipeline setup ~5 ms。
4. **写盘：128³ 22–32 ms**（OBJ 主导；HTML 已 base64 减半）。
5. 1T→24T 约 9×；要再上台阶必须 SIMD splat 或稀疏/更高分辨率架构，而不是调线程数。

### 6.13 全套基准复跑（2026-10-20，当前构建）

`perf_compare`（3 次中位数；vox+mesh，不含 init/读写；GPU init 433.1 ms）：

| 场景 | CPU 1T | CPU 24T | GPU | GPU+GPU 网格 |
| --- | --- | --- | --- | --- |
| 64³ 球 | 358.7 | 40.9 | **15.5** | 24.5 |
| 64³ blobs | 438.3 | 48.2 | **18.8** | 40.2 |
| 128³ 球 | 1483.7 | 162.9 | **44.6** | 119.2 |
| 128³ blobs | 3582.3 | 357.4 | **45.1** | 132.8 |

- GPU 热态比 CPU 24T 快 **2.6–7.9×**；CPU 1T→24T **9.6–10.5×**。
- GPU mesh 仍比 CPU mesh 慢 3.3–7.6×（128³ 球 98.9 vs 21.7、blobs 110.5 vs 18.1）。
- GPU vox 分解（`bench_gpu_scenes`）：64³ 10.8/11.9 ms、128³ density 18.9 / surfel 52.9 ms；
  128³ surfel 的 post（双缓冲 + 密封）仍显著重于 density。

其余基准（同轮）：

| 项 | 读数 |
| --- | --- |
| 100k eigen（adaptive vs AVX2 batch） | 6.4 → 5.2 ms（1.22×） |
| 2M quantile（sort vs select） | 36.7 → 2.5 ms（14.6×） |
| 100k prep density 1T/24T/SIMD | 9.6 / 6.3 / 6.1 ms |
| 100k prep surfel 1T/24T/SIMD | 11.4 / 6.5 / 7.6 ms（SIMD 0.85×） |
| 100k eigen 内部 | covariance 1.5 ms + Jacobi 5.5 ms（78.9%） |
| 100k bins | 旧逐块全扫 80.0 ms → 建桶 2.8 ms（364,280 entries） |
| 3D tile vs 动态 z-slab（20k density/surfel） | 0.99× / 0.99×（100k：0.96× / 1.06×），逐位一致 |

**结论**：动态取块后，3D tile 在 20k 已与 z-slab 持平；100k 只有 surfel 快 6%，
继续维持 `--bins3d` 可选档。GPU 的单次转换仍被 init 433 ms 主导；热态/批处理才是它的场景。

### 6.14 256³ 基线与稀疏可行性（2026-10-20）

稠密 256³ CPU 24T（本机）：

| 场景 | vox | mesh | write | total | 三角形 | 体积 |
| --- | --- | --- | --- | --- | --- | --- |
| 球 surfel | 786.5 ms | 133.4 ms | 137.5 ms | **1061.3 ms** | 576,144 | 1.77614 |
| blobs density | 2690.2 ms | 105.9 ms | 95.6 ms | **2895.1 ms** | 426,468 | 2.04547 |

⇒ 稠密 256³ 在 20k/12k 场景已能跑（1.1–2.9 s），不是“完全上不去”。

稀疏块占用探针（post-seal Field 中**有符号变化**的块）：

| 场景 | 16³ 块 | 32³ 块 |
| --- | --- | --- |
| torus surfel | 688 / 4096 = 16.8% | 176 / 512 = 34.4% |
| blobs density | 188 / 4096 = 4.6% | 48 / 512 = 9.4% |

**注意**：post-seal 会把未知区写成 ±outside，所以“非零块”恒为 100%，不能当原始 splat
分配率。结论：朴素“按块稀疏存储”不会降低分配率；要获得收益必须先做**粗分辨率表面定位**
（如 64³ 粗场）再只对表面块 + halo 做细 splat，并实现稀疏密封与分块网格。这是 T-GC-05
的真实成本。本轮只记录 256³ 基线与占用率，默认路径未改。

## 7. 限制与否证条件（不许当结论）

1. **CPU 多线程，非实时**：64³/20k 高斯总耗时约 0.05–0.07 s（24 线程；1T 参照 0.36–0.48 s）；
   128³ 已测（§3.8/§6.2），逐帧实时仍不成立。GPU 路径已落地（整数原子 + GPU iso/密封），
   但默认档仍是 CPU；CPU 多线程版继续作 GPU 的逐位参照。
2. **稠密 n³**：256³ = 6700 万格，内存与遍历都上不去；量产要走稀疏分块 + 1-voxel halo（T-GC-05，`W15A-034/035`）。
3. **只读标准 3DGS PLY**（ASCII/二进制 little-endian）；SPZ/量化/planar 布局未接（`W15A-040..044` 是参照）。
4. **density 的 iso 是全局值**：非均匀场景会破壳或局部过胖；自动重试只是启发式，不是自适应阈值。
   否证条件：在 3 个以上真实非均匀 3DGS 场景上，auto 重试率 >30% 或体积误差 >25% ⇒ 需要局部/分块阈值。
5. **surfel 需要可靠法线与闭合表面**：开放面/布料/树叶会漏；`--hollow` 只改变未知区语义，不修开放面。
6. **无法线时不估法线**：`auto` 走 density；显式 surfel 用"最薄轴+质心定向"，只对凸/近凸场景可信。
7. **Surface Nets 抹尖角**：DC/MT 质量档未接（`W15A-019/008`）；薄流/尖特征的质量判据尚未做（T-GC-03 的薄流场景）。
8. **无颜色/UV/材质**：OBJ 只导几何+法线；SH 只用于预览点的颜色（DC 项）。
   `--largest-component` 只适合“单主物体 + 碎片”资产；真实多物体场景会被删掉次物体。
9. **封闭填充假设"物体是实心"**：空心物体（杯子/壳体）会被填实；需要 `--hollow` 或专门的开口检测。
10. **未见真 2DGS/SuGaR 资产**：本轮的真实 PLY 是用脚本按 3DGS 语义合成的；
    对训练产物的法线一致性/尺度分布尚未实测。
11. **GPU 档的边界**：定点量化不等于 CPU 逐位；u32/i32 累加在超大重叠/极端尺度下可能溢出；
    job 表与 Surface Nets 仍在 CPU；iso 用 65536 桶直方图取 q99.9 的**桶下界**（误差 ≤ max/65536），
    密封最多 2n 趟、未收敛显式报错；全格 ping-pong 掩码占 2×n³ u32（128³ 16 MB、256³ 128 MB）；
    2D dispatch 上限 65535；设备 init Vulkan-first ~0.44 s（全后端 ~0.87 s），单次转换默认仍 CPU。
    GPU 当前是批处理/
    更高分辨率档，不是“一键实时”。
12. **batch 是顺序的**：不并行多个文件；设备缓存是进程级；每文件默认写 JSON，无聚合报告；
    重名 stem 自动 `_2`；首个文件仍付 init。
13. **分桶只按 z，prep 的并行加速有上限**：x/y 仍扫单高斯全 bbox。3D tile 已作为
    `--bins3d` 可选档（默认关；20k 1.14–1.25×，100k 1.03–1.05×，逐位一致）。
14. **AVX2 prep 当前没有端到端收益**：eigen_batch 1.37×，但 24T 标量 prep 已到 5–6 ms；
    SIMD 并行后持平/略慢，且 opt-in 会改 surfel 哈希。保留 `--simd-prep` 实验档，默认标量。
15. **AVX2 density 仅覆盖 x86_64**：无 AVX2/非 x86 自动回退标量；`LSSMJ_NO_AVX2=1`
    可强制标量做对拍。surfel 没有 SIMD 路径，仍是标量热循环。
16. **GPU 网格是实验档，不是加速档**：`field_to_mesh_gpu`/`--gpu-mesh` 复用 `gsn` 的 n³ 稀疏
    槽位缓冲，64³ 热态 13–15 ms（CPU 3.7–3.8 ms），128³ 96–183 ms（CPU ~21 ms），首跑含
    管线编译 ~460 ms。索引缓冲已按 CPU 预数精确分配（128³ 不再触发 binding 上限；256³ 的
    pos/flag 缓冲仍超限并显式回退）。法线是面积加权面法线，不是 SDF 梯度。转正前提是
    GPU compaction/scan + 场留 GPU；在此之前不要把它写进默认档或性能宣传。

## 8. 复算命令

```bash
cd crates/conv-core
# 高斯腿判据（球/环面/密度/剪枝/PLY/确定性/多线程逐位）
CARGO_INCREMENTAL=0 cargo test --release --test gauss_leg -- --test-threads=1 --nocapture
# GPU splat + iso + 密封判据（逐位确定 + CPU 容差对拍 + hollow；串行）
CARGO_INCREMENTAL=0 cargo test --release --test gauss_gpu -- --test-threads=1 --nocapture
# GPU 64³/128³ 计时（含 hollow 对照）
CARGO_INCREMENTAL=0 cargo test --release --test gauss_gpu -- --test-threads=1 --ignored bench_gpu_scenes --nocapture
# 完整性能对比（CPU 1T/24T、GPU、GPU 网格；64³/128³ × density/surfel）
CARGO_INCREMENTAL=0 cargo test --release --test perf_compare -- --test-threads=1 --ignored --nocapture
# 自包含 HTML 的 base64 载荷（长度/首顶点/旧格式）
cargo test --release --test html_embed -- --test-threads=1 --nocapture
# 全量门（含 GPU，必须串行）
CARGO_INCREMENTAL=0 cargo test --release -- --test-threads=1
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo fmt --all --check

# 三组 demo + 预览（CPU 多线程；--threads 1 为逐位基线）
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo torus
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo blobs
# GPU（splat+iso+密封；单次转换 init 主导；批处理用 voxelize_gpu_with）
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere --gpu
# GPU 网格实验档（通常更慢；128³ 会显式回退 CPU）
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere --gpu --gpu-mesh
# CPU 可选档：3D tile / SIMD prep / 只保留最大连通分量
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere --bins3d
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere --simd-prep
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo blobs --largest-component
CARGO_INCREMENTAL=0 cargo test --release --test largest_component -- --test-threads=1 --nocapture
# AVX2 density 对拍：强制标量
LSSMJ_NO_AVX2=1 CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo blobs --grid 128
# 打开 out/sphere.html（自包含，无外部依赖）

# 真实 PLY
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- inspect scene.ply
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- convert scene.ply --grid 128 --json out/report.json
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- convert scene.ply --grid 128 --gpu --json out/report-gpu.json
```

## 9. 参考与锚

### 9.1 本仓账本锚（设计判决只依赖这些 + 本地实测）

| 锚 | 用途 |
| --- | --- |
| `W15A-003/004` | SuGaR：先正则化让高斯贴表面；分钟级离线，逐帧实时不成立 |
| `W15A-005/006` | 2DGS：取向平面盘、视角一致几何、内在表面语义 |
| `W15A-007/008` | GOF：opacity field 的 levelset + Marching Tetrahedra |
| `W15A-016/017` | Gaussian-to-voxel splatting（概率占据）；稀疏高斯 vs 稠密体素内存 4–5× |
| `W15A-018..021/032/034/035` | MC/DC/Surface Nets 谱系、20M tri/s、分块 1-voxel halo |
| `W15A-040..044` | PLY/SPZ 读入、planar/packed 布局、GPU 高斯抽象 |
| `W15C-*` | CPU 参照实测批（mesh→SDF/SDF→mesh 的对拍口径） |
| `00-master-plan.md` C22 | 表示转换一致：守恒/解析对拍 + 逐位确定 + 降级记账 |

### 9.2 2025–2026 外部工作（**web 旁证，未入账、未复算**；不单独支撑本实现的设计判决）

| 工作 | 口径 | 与本实现的关系 |
| --- | --- | --- |
| [SOF: Sorted Opacity Fields for Fast Unbounded Surface Reconstruction](https://dl.acm.org/doi/10.1145/3757377.3763933) | 摘要：从 3D 高斯恢复细节表面，兼顾速度与精度 | 路线 C 的质量/速度改进；未实现 |
| [Confidence-Based Mesh Extraction from 3D Gaussians](https://arxiv.org/abs/2603.24725) | 摘要：可学习 confidence 动态平衡光度与几何监督 | 提示 density/surfel 的权重不应是常数；未实现 |
| [Gaussian-Voxel Duet](https://arxiv.org/abs/2605.26616) | 摘要：Gaussian–voxel 双支架，把锚定高斯拴到联合优化的稀疏体素支架，提取 SDF | "高斯体素"读法 ② 的 2026 代表；需要联合优化/解码器 |
| [TopoSurfel](https://www.emergentmind.com/papers/2608.20687) | 摘要：Gaussian surfels + mesh surface reconstruction，含 TSDF 融合 | 路线 B 的同族；未获取全文 |
| [GVKF: Gaussian Voxel Kernel Functions](https://proceedings.neurips.cc/paper_files/paper/2024/file/bd31bfd4caa85bffe07a35568182cdfa-Paper-Conference.pdf) | NeurIPS 2024：离散 3DGS 上的连续场景表示 | "高斯体素"读法 ② 的另一支；未实现 |
| [GOF 项目页](https://niujinshuchong.github.io/gaussian-opacity-fields/) / [代码](https://github.com/autonomousvision/gaussian-opacity-fields) | opacity levelset + MT 的官方实现 | 路线 C 的参考实现 |
| [3DGS (SIGGRAPH 2023)](https://arxiv.org/abs/2308.04079) / [SuGaR](https://arxiv.org/abs/2311.12775) / [2DGS](https://arxiv.org/abs/2403.17888) / [GaussianFormer](https://arxiv.org/abs/2405.17429) | 资产格式、正则化、surfel、Gaussian-to-voxel 的原始出处 | 已由 W15A 账本锚定；此处只补链接 |

### 9.3 未决问题（下一轮）

1. **自适应 iso**：按每个高斯中心的密度分位/局部块阈值，替代"全局 q99.9 + 失败重试"；
   判据：真实非均匀场景的重试率、体积误差、χ 稳定性。
2. **3D tile / SIMD prep 可选档已试验**：3D tile 有 20k 14–25% 收益（`--bins3d`），
   SIMD prep 的 eigen 1.37× 但完整 prep 与 24T 持平（`--simd-prep` 实验档，默认关）。
   GPU 侧 iso/密封已落地（§3.7/§6.2）；GPU 网格已试但不划算（§3.10/§6.6），索引缓冲精确
   分配已解除 128³ binding 限制，但热态仍慢 5–9×；下一步：256³ 稀疏分块、GPU 网格的缓冲级改造；
   1M 高斯时再评估融合 AVX2 prep。单次转换仍被 ~0.44s 的 wgpu init 拖慢，不能把 GPU 当默认档。
3. **稀疏分块**：256³/大场景的 1-voxel halo 无缝契约（T-GC-05）。
4. **质量档**：DC/MT 对尖角/薄流的改善；需要薄流/尖特征测试对象（W15A-025 的教训：光滑球堆测不出差异）。
5. **真实资产**：2DGS/SuGaR 训练产物的法线一致性、尺度分布、SPZ 读入。
