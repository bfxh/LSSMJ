# 高斯体素 → 网格：路线、实现与 UX（2026-10-20 实现批）

> **这份文档是什么**：只回答一条边——**高斯体素怎么转网格，以及转换工具的 UX 该怎么做**。
> 范围口径来自用户 2026-10-20："你就只研究研究高斯体素怎么转网格和 UX……而且要做出来"。
> **实现位置**：`crates/conv-core/src/gauss.rs` + `src/gauss/{ply,voxel,mesh}.rs` +
> `src/bin/gauss2mesh/{main,args,report,html}.rs` + `src/viewer.html`；
> 机器判据在 `crates/conv-core/tests/gauss_leg.rs`。
> **证据纪律**：设计判决只依赖本仓账本锚（`W15A-*`/`W15C-*`）与**本地实测**；
> 2025–2026 的新论文只作 web 旁证，未入账，不单独支撑设计判决（本仓 `web` 纪律）。
> 本轮没有新增 ledger 条目；新增的外部链接在 §9 明示"未入账/未复算"。

## TL;DR（8 条）

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
7. **本机实测（64³，CPU 单线程，release）**：球体积误差 **0.41%**、环面 **0.40% 且 χ=0（亏格 1）**；
   20k 高斯总耗时 **0.18–0.39 s**，其中体素化约 90%；同输入逐位哈希稳定（球 `cd8f000dba7bfb4e`）。
8. **UX 的核心不是参数面板**，而是"先诊断、再转换、可解释降级、可预览"：
   `inspect` 先看资产；`auto` 说出选路理由；三旋钮（分辨率/剪枝/阈值）都有默认值与后果提示；
   失败/回退/重试进警告与 JSON；自包含 WebGL 预览默认只显示网格（20k 高斯点盖上去会摩尔纹）。

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
| `--prune-opacity` | 0.05 | 丢低不透明度 floaters；太高会削掉真实薄结构 |
| `--iso` | auto（0.65×q99.9） | 密度阈值；**低=胖/易闭合，高=紧/易破壳**；surfel 忽略 |
| `--padding` | 3 | 域边界留白格；覆盖 3h 采样带宽，防止表面被截断 |
| `--hollow` | 关 | 不做未知区封闭；输出内外双面的壳（体积参考不适用） |
| `--min-radius` / `--max-radius` | 0（关） | 按 3σ 半径剪枝；可去背景雾团/超大板 |
| `--raw-scale` / `--raw-opacity` | 关 | PLY 的 scale/opacity 已激活时使用 |

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
```

### 5.2 阶段进度（实跑输出）

```text
[1/5] 读入     binary_little_endian · 20,000 高斯 · 1328.5 KB · 5.5 ms
[2/5] 剪枝     20,000 保留 · 丢 0（低不透明度 0 / 半径 0 / 非法 0）· 0.5 ms
[3/5] 体素化   surfel · 64³ · h=0.02772 · iso=0.00000 · 观测 68,057 · 封闭内部 53,291 · 324.7 ms · auto→surfel：资产带法线
[4/5] 网格     Surface Nets · 13,860 顶点 · 27,716 三角形 · χ=2 · 1 分量 · 3.4 ms
[5/5] 写出     out/ply_sphere.obj · out/ply_sphere.html · out/ply_sphere.json · 27.9 ms

体积 1.77411 · χ=2 · 分量 1 · 64³ · surfel · 362.0 ms 总计
哈希 bf166ee3634a1f3d · 导出 ...
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

`mode_requested / mode_used / auto_reason / fallback / iso_retry / has_normals / flatness_p50 /
gaussians{input,kept,dropped_*} / grid{n,h} / field{mode,iso,observed,sealed_interior} /
mesh{vertices,triangles,components,chi,volume,boundary_touch,hash,bounds} /
timings_ms{load,prune,voxel,mesh,write} / warnings[]`。

### 5.5 自包含 WebGL 预览

- 一个 HTML 内嵌 mesh（positions/normals/indices）、高斯点（positions/colors）与 stats；无外部 CDN/JS。
- 轨道相机：拖拽旋转、滚轮缩放、重置/正视；网格/高斯点/线框/自动旋转开关。
- 默认只显示网格；点云/线框按需开。
- 预览校验（本机）：WebGL2 画布 960×640，`gl.getError()=0`；同步 `render()` 后 `readPixels` 得到
  78,905 个非背景像素（含点云版本），并把画布降采样成 ASCII 轮廓确认球/环面形状。

## 6. 本机实测（2026-10-20，release，CPU 单线程）

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
单线程、按 PLY 顺序累加；没有并行归约带来的顺序不确定性。

**耗时结构**：体素化约 90%（球 356 / 390 ms；环面 155 / 181 ms）；12k density 含重试 0.5–0.9 s。
Surface Nets 本身 2–7 ms，不是瓶颈；GPU 化应先做体素化 splat，而不是先换 mesher。

## 7. 限制与否证条件（不许当结论）

1. **CPU 单线程**：64³/20k 高斯 0.2–0.5 s；128³ 约 8× 体素，未测；逐帧实时不成立。
   下一步是 wgpu compute splat（体素化），Surface Nets 已有 GPU 腿可接。
2. **稠密 n³**：256³ = 6700 万格，内存与遍历都上不去；量产要走稀疏分块 + 1-voxel halo（T-GC-05，`W15A-034/035`）。
3. **只读标准 3DGS PLY**（ASCII/二进制 little-endian）；SPZ/量化/planar 布局未接（`W15A-040..044` 是参照）。
4. **density 的 iso 是全局值**：非均匀场景会破壳或局部过胖；自动重试只是启发式，不是自适应阈值。
   否证条件：在 3 个以上真实非均匀 3DGS 场景上，auto 重试率 >30% 或体积误差 >25% ⇒ 需要局部/分块阈值。
5. **surfel 需要可靠法线与闭合表面**：开放面/布料/树叶会漏；`--hollow` 只改变未知区语义，不修开放面。
6. **无法线时不估法线**：`auto` 走 density；显式 surfel 用"最薄轴+质心定向"，只对凸/近凸场景可信。
7. **Surface Nets 抹尖角**：DC/MT 质量档未接（`W15A-019/008`）；薄流/尖特征的质量判据尚未做（T-GC-03 的薄流场景）。
8. **无颜色/UV/材质**：OBJ 只导几何+法线；SH 只用于预览点的颜色（DC 项）。
9. **封闭填充假设"物体是实心"**：空心物体（杯子/壳体）会被填实；需要 `--hollow` 或专门的开口检测。
10. **未见真 2DGS/SuGaR 资产**：本轮的真实 PLY 是用脚本按 3DGS 语义合成的；
    对训练产物的法线一致性/尺度分布尚未实测。

## 8. 复算命令

```bash
cd crates/conv-core
# 高斯腿判据（球/环面/密度/剪枝/PLY/确定性）
CARGO_INCREMENTAL=0 cargo test --release --test gauss_leg -- --test-threads=1 --nocapture
# 全量门（含 GPU，必须串行）
CARGO_INCREMENTAL=0 cargo test --release -- --test-threads=1
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo fmt --all --check

# 三组 demo + 预览
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo sphere
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo torus
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- demo blobs
# 打开 out/sphere.html（自包含，无外部依赖）

# 真实 PLY
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- inspect scene.ply
CARGO_INCREMENTAL=0 cargo run --release --bin gauss2mesh -- convert scene.ply --grid 128 --json out/report.json
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
2. **GPU splat**：体素化约 90%，先把 density/TSDF splat 搬上 wgpu compute，再接现有 GPU Surface Nets。
3. **稀疏分块**：256³/大场景的 1-voxel halo 无缝契约（T-GC-05）。
4. **质量档**：DC/MT 对尖角/薄流的改善；需要薄流/尖特征测试对象（W15A-025 的教训：光滑球堆测不出差异）。
5. **真实资产**：2DGS/SuGaR 训练产物的法线一致性、尺度分布、SPZ 读入。
