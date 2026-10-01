# GPU/2D 渲染技术：论文与正式长文批次（C1）（抓取 2026-10-01）

> 本批是**论文/正式长文**轨道（无源码 commit 可锚）：全部锚 = 抓取时的 URL + 逐字引文，
> 落盘原文在 `D:/KF/LSSMJ/scratch/c1/raw/`（HTML 转文本 / PDF 经 `pdftotext -layout`）。
> 账本：`docs/analysis/ledger/w3a.jsonl`，**85 条**（paper 62 / doc 23，覆盖 **38 个 URL、23 个主题**），
> `python tools/ledger.py verify --file docs/analysis/ledger/w3a.jsonl` ⇒ `rows_ok=85 rejected=0`。
> 引文命中方式（`scratch/c1/gen.py` 逐条测）：82 条原样逐字；2 条空白折叠后命中（HTML 换行）；
> 1 条需去标签后命中（Crossref 摘要内嵌 `<jats:italic>`）——**未命中者一律不写进账本**（本批 0 拒收）。
> 逐篇短分析（≥6 份，每份 ≤80 行）在 `docs/papers/`：`karis-nanite-2021.md`、`mesh-shading-meshlets.md`、
> `gpu-driven-rendering.md`、`gpu-2d-rasterization.md`、`oit-and-overdraw.md`、`impeller-shader-offline.md`、
> `compositor-latency.md`。

## TL;DR（每条带锚 W3A-xxx）

1. **GPU-driven 的立论是"省 CPU"，不是"提画质"**：Ubisoft 主机侧口径"CPU 是最稀缺资源"（`W3A-077`），
   vkguide 同场景 CPU 每帧 <0.5 ms（`W3A-019`）——收益以"主线程腾空"计。
2. **GPU-driven 的规模区间在 10^5 对象量级**：125,000 对象剔除后 290 FPS（`W3A-017`）、
   25 万 drawcall 在 Switch 上 >60 fps（`W3A-018`）；Nanite 更到 10^6 实例 / 10^12 三角形（`W3A-084/001`）。
   我们的场景是 10^2–10^3，量级差 2–3 个数量级。
3. **管线切换 > 绑定 > draw 是公认成本序**：`VkCmdBindPipeline` 是最贵调用之一（`W3A-022`），
   Doom Eternal 全游戏 <500 条管线 vs UE 游戏常 10 万+（`W3A-021`）。
4. **bindless 是 GPU-driven 的前置能力**：Vulkan 侧靠 `VK_EXT_descriptor_indexing` 建"装下几乎全部资源"的
   大描述符集（`W3A-026`），且允许绑定后更新（`W3A-027`）；不支持时退路 = 按材质各发 1 个 draw-indirect（`W3A-028`）。
5. **meshlet/mesh shader 是 3D 几何的"簇化"路线**，硬件约束 64 顶点 /126 三角形（`W3A-007`），
   Khronos 明确"多数用例传统管线仍最合适"（`W3A-012`）——**我们不追**。
6. **2D 矢量填充在 GPU 上的三代路线**：Stencil-then-Cover（`W3A-030/031`）、凸包三角+shader 内外判定
   （`W3A-032/033/034`）、解析覆盖（`W3A-035`）与随机访问矢量（`W3A-036`）；Rive 走"路径→唯一三角形补丁"（`W3A-069`）。
7. **透明正确性 = 顺序正确性**（`W3A-040`）；三条免排序/免整序路线：深度剥离（`W3A-041`）、
   逐像素链表（`W3A-042`）、加权混合（免排序近似，`W3A-037`；内存有界 3–5 张缓冲 `W3A-039`）。
8. **过度绘制是 2D/粒子共通的成本源**：全屏粒子 overdraw "几乎无界"（`W3A-043`），
   解法之一是降分辨率离屏再放大（`W3A-044`）；meshoptimizer 甚至提供阈值 1.05 的 overdraw 重排（`W3A-008`），
   但 TBDR 平台上无收益（`W3A-009`）。
9. **2D GPU 管线的成熟骨架 = 元素包围盒 → 分箱（sort-middle）→ 逐 tile 处理**（`W3A-055/056`），
   跨 tile 边界用 backdrop 计数替代全局排序（`W3A-058`）；作者自评其可解释性"每一步成本都有理由"（`W3A-059`）。
10. **GPU 档的准入纪律来自 Impeller**：着色器编译/反射与管线状态全部构建期完成（`W3A-072/073`）；
    而 GPU 渲染器评测本身多变量难比（`W3A-057`）、延迟是吞吐/延迟/功耗三角权衡（`W3A-062`）——
    读数必须带机器/口径，否则不可比。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 批键成本序：先并管线、再并绑定、最后并 draw | `W3A-022/021` | 吸收（GPU 档批处理设计序） |
| 构建期离线编译全部着色器 + 少管线数 | `W3A-072/073` | 吸收（GPU 档第一条纪律，已在设计 §4.2） |
| 元素包围盒 → 分箱 → 分区处理（sort-middle） | `W3A-055/056` | 有界吸收（CPU 侧=damage/tile 分箱；GPU 侧仅作参考） |
| backdrop 计数替代全局排序 | `W3A-058` | 有界吸收（GPU 档若做分箱时采用） |
| 保守包围盒（宁多画不误删） | `W3A-056/081`（斜角假阳性是常态） | 吸收（damage 联合矩形与裁剪同原则） |
| Stencil-then-Cover 两段式接口 | `W3A-030/031` | 有界吸收（矢量填充备选；候选窗主路径是字形掩码/图集） |
| 加权混合 OIT（免排序、有界内存） | `W3A-037/038/039` | 有界吸收（仅在需要透明叠加且排序太贵时启用，质量有损需留档） |
| 降分辨率离屏 + 上采样省 overdraw | `W3A-044` | 有界吸收（模糊/阴影的候选实现） |
| overdraw 重排（阈值 1.05） | `W3A-008` | 有界吸收（不透明批不做重叠检查时的补充） |
| "每步成本都有理由"的可解释性验收 | `W3A-059` | 吸收（性能报告模板） |
| 数字必带口径（平台/单位/机器） | `W3A-080/017` | 吸收（写进 reports/09 协议） |
| GPU-driven 全量管线（间接绘制/GPU 剔除/GPU 批） | `W3A-017/018/021/023` | 不吸收（量级差 2–3 个数量级，复杂度不划算） |
| meshlet + mesh shader（簇化几何） | `W3A-006/007/011/012` | 不吸收（2D 图元不需要簇化；khronos 亦劝退） |
| Nanite 虚拟几何（万亿三角形） | `W3A-001/002/003` | 不吸收（场景量级完全不同；仅取"格式/执行器分离"范式） |
| GPU 字形光栅（Pathfinder 路线） | `W3A-066/067/071` | 不吸收（作者自述小字号慢；我们以小字号为主） |
| 逐像素链表 OIT / 深度剥离 | `W3A-041/042` | 不吸收（原子/无界存储与"有界内存"纪律冲突） |
| GPU 遮挡查询（occlusion query） | `W3A-048` | 不吸收（需 CPU-GPU 同步；候选窗重叠少） |
| 深队列 present（多缓冲排队） | `W3A-060/061/063` | 不吸收（延迟优先；tearing-free 双缓冲已足够） |

## 1. 全景：这一批在讲什么（按层）

| 层 | 代表来源（本批） | 核心问题 | 账本段 |
| --- | --- | --- | --- |
| 几何/簇化 | meshoptimizer README、Khronos 博客、DX12 MeshShader 规范、AMD work-graph 演示、Nanite 讲义 | 一个"工作单元"该多大、由谁决定画什么 | `W3A-001..016` |
| 驱动层 | vkguide GPU-driven 章、Ubisoft SIGGRAPH 2015、GPUOpen work graphs(Vulkan/DX12) | 把 CPU 决策搬上 GPU：间接绘制、剔除、批 | `W3A-017..025` |
| 资源绑定 | Vulkan `VK_EXT_descriptor_indexing` 附录 | 大描述符集 + 非统一索引（bindless） | `W3A-026..028` |
| 矢量填充 | Kilgard TOG 2012、GPU Gems 3 ch25、解析光栅、随机访问矢量、Rive | 曲线如何在 GPU 上判定覆盖并着色 | `W3A-029..036` |
| 顺序/透明/过度绘制 | JCGT 加权混合 OIT、Everitt 2001、A-buffer、逐像素链表、GPU Gems 3 ch23、GPU Gems 1 ch28/29、Hi-Z、MLAA | 顺序、遮挡、填充率三者的成本与近似 | `W3A-037..051` |
| 2D GPU 管线 | Raph Levien 系列（modern-2d / sort-middle / fast-2d）、pcwalton 两篇、NanoVG、Vello | 2D 场景如何映射到 compute/硬件光栅 | `W3A-052..071` |
| 运行期纪律 | Impeller README、Vulkan 片元操作规范 | 离线编译、管线状态预建、规格化阶段顺序 | `W3A-072..074` |
| 合成与延迟 | Raph"合成器是恶"、交换链与帧节奏 | 多缓冲/合成器带来的延迟 | `W3A-060..065` |

## 2. 关键机制

### 2.1 GPU-driven：把"画什么"下沉到 GPU

- 机制：绘制参数放 GPU 缓冲（`VkDrawIndexedIndirectCommand`），由 compute 写入——**draw 的计数与参数**在 GPU 端生成；
  vkguide 的标准流程 = 对象数据全体上 GPU → compute 剔除 → 按 batch 累加 instance count（`W3A-017/020`）。
- 收益口径：Ubisoft 的目标是"无 CPU 干预、少数间接 draw 渲染高密度场景"（`W3A-023`）；
  vkguide 教程读数 125k 对象 290 FPS（RTX 2080）/ CPU <0.5 ms（Ryzen 1700）（`W3A-017/019`）。
- 代价与反例：Ubisoft 自列 cluster 化两坑——退化三角形导致**内存增加**、并行写出导致**cluster 顺序不确定**
  （`W3A-078/079`）→ 任何并行剔除后的顺序都必须显式再定序（对 UI = 渲染顺序即正确性）。
- 最新形态：AMD 把"绘制节点"做成 work graph 的叶子（DX12 `VK_AMDX_shader_enqueue`），
  不再调 compute 而是直接派发 mesh-shader 管线（`W3A-013/024/025`）。

### 2.2 meshlet / mesh shader（本批的"不追"方向，但术语要对齐）

- 定义：mesh shader 合并顶点与图元处理（`W3A-015`），走 compute 编程模型、线程协作生成网格（`W3A-010`）；
  task(~amplification) shader 做簇级剔除/LOD，最终意图取代硬件细分器（`W3A-016`）。
- 约束口径：NV 推荐 max_vertices=64 / max_triangles=126（`W3A-007`）；典型用法是**预计算三角形簇**（`W3A-011`）。
- 适用性：Khronos 原文即写明"传统管线对多数用例仍最合适"（`W3A-012`）；TBDR 平台（PowerVR/Apple）
  甚至不吃 overdraw 重排（`W3A-009`）。
- Nanite 是这条线的极端：内部网格格式 + 渲染技术两层（`W3A-003`），全链路 流式/解压/剔除/光栅/着色（`W3A-002`），
  官方也承认仍有实用上限（`W3A-005`）。讲义 PDF 补齐后的关键口径：**簇=128 三角形**（`W3A-082`）、
  百万实例（`W3A-083`）、10k→1M 三角形只多 43% 树层级（`W3A-084`）、成本应随**屏幕分辨率**而非场景复杂度走（`W3A-085`）。

### 2.3 bindless：一次绑定、索引取用

- `VK_EXT_descriptor_indexing` 的用途 = 建"包含几乎全部资源"的大描述符集 + shader 内（非统一）索引（`W3A-026`）；
  描述符允许在绑定后更新（`W3A-027`）。
- 生态现状（vkguide 口径）：bindless 纹理支持面仍有限，教程退化为**每材质 1 个 draw-indirect**（`W3A-028`）——
  这与我们"材质批键"的降级路径等价。

### 2.4 矢量填充：三条路线与一个工程化选择

- **Stencil-then-Cover（NV_path_rendering, TOG 2012）**：stencil 步判覆盖、cover 步着色，接口显式解耦
  （`W3A-030/031`）；出发点是"30 年来 2D 标准都靠 CPU 做路径填充/描边"（`W3A-029`）。
- **凸包三角 + shader 内外判定（GPU Gems 3 ch25）**：曲线转"内部三角（直接填充）+ 边界三角（u/v 参数 + 片元判定）"
  （`W3A-032/033/034`）。
- **解析/随机访问**：多项式滤波器解析覆盖（CGF 2013，`W3A-035`）、随机访问通用矢量（TOG 2008，`W3A-036`）。
- **工程化落地（Rive 2023）**：几何归约为"唯一三角形补丁"喂硬件光栅器（`W3A-069`）；
  其自述因**多渲染器矩阵**（Skia/Canvas）导致特性无法跨平台上齐才自研（`W3A-070`）——多后端成本的一手证据。

### 2.5 排序、过度绘制、遮挡

- 成本源：全屏粒子 overdraw "几乎无界"（`W3A-043`）；省法=降分辨率离屏+放大（`W3A-044`）；
  3D 侧还有显式重排 API（阈值 1.05，`W3A-008`）。
- 透明：正确性=顺序（`W3A-040`）；免序遍历（深度剥离，`W3A-041`）/链表（`W3A-042`）/加权混合（`W3A-037`，内存有界 3–5 张，`W3A-039`，不需特殊硬件，`W3A-038`）。
- 遮挡：排序（前到后）是两条路线共同前提（`W3A-047`）；查询有同步代价、early-z 在光栅级（`W3A-048`）；
  层次 Z 缓冲是"粗层先淘汰"的鼻祖（`W3A-050`）。
- 抗锯齿旁证：MLAA 这类后处理 AA 是"改不了光栅路径时"的方案（`W3A-051`）——我们 CPU 侧自带 2-bit 超采样，无需。

### 2.6 2D GPU 管线的成熟骨架（Raph Levien 三代）

1. **modern-2d（2019）**：结论"依赖现代 compute，2D 直接上 GPU 可行"（`W3A-052`），但刻意只做 Metal 2.1（`W3A-053`），
   谱系上溯 2014 Massively-Parallel Vector Graphics（`W3A-054`）。
2. **sort-middle（2020）**：整体借鉴 Laine & Karras 2011（软件光栅化 GPU 化）（`W3A-055`）；
   第一步给每个元素标包围盒供分箱（`W3A-056`）。
3. **fast-2d（2020）**：跨 tile 边界用 backdrop 原子计数（`W3A-058`）；自评"每步成本都有理由"（`W3A-059`）。

对照物：NanoVG 定位"面向 UI 的轻量抗锯齿矢量库"（`W3A-075`），描边质量开关只在宽描边可见（`W3A-076`）；
Vello 已拆出 CPU 版（多线程/SIMD）（`W3A-071`）——**同一渲染器双档**的外部先例，与本项目形态一致。
反面：Pathfinder 是 GPU 字形路线的作者自述"小字号慢"的实例（`W3A-066/067`）。

### 2.7 合成、交换链与延迟

- 直写 framebuffer 时代延迟极低（`W3A-060`），窗口 GUI 引入合成器后延迟问题出现（`W3A-061`）。
- 现代交换链：性能=吞吐/延迟/功耗三角（`W3A-062`）；vsync on 时交换推迟到扫描结束、队列深度即延迟（`W3A-063`）。
- 选型旁证：自建渲染器换"跨平台一致 + 测试负担低"（`W3A-065`）；系统库路线构建成本高（Skia 克隆 349MB，`W3A-064`）。

## 3. 性能手段与公开读数（数字都带口径）

| 读数 | 口径（来源自述） | 锚 |
| --- | --- | --- |
| 125,000 对象 / 290 FPS（+4000 万三角形，2 个 mesh pass） | vkguide 教程，RTX 2080 | `W3A-017` |
| 250,000 "drawcall" >60 fps（PC 500 fps） | vkguide 教程，Nintendo Switch | `W3A-018` |
| CPU 每帧 <0.5 ms | vkguide 教程，Ryzen 1700 | `W3A-019` |
| >100 万对象剔除 <0.5 ms | vkguide 教程 | `W3A-020` |
| 遮挡深度：300 遮挡体 ~600us；降采样 512x256 100us；重投影 50us；层次 50us | Ubisoft SIGGRAPH 2015 幻灯片，***PS4** | `W3A-080` |
| 静态背面剔除 10–30% 三角形 | 同幻灯片（cubemap 逐簇，含斜角假阳性） | `W3A-081` |
| Doom Eternal <500 管线 vs UE 常 100,000+ | vkguide 转述（二手） | `W3A-021` |
| 唯一三角形补丁、120 fps 目标 | Rive 官方博客 | `W3A-069` |
| 内存有界 3–5 张附加缓冲 | JCGT 2(2) 2013 论文自述 | `W3A-039` |
| Nanite 簇=128 三角形；10k→1M 三角形仅 +43% 树层级；百万实例 | SIGGRAPH 2021 讲义正文（自述） | `W3A-082/083/084` |

> 纪律：上表全部是**来源自述口径**（作者机器/平台），只能作量级锚，不能当我们的预算；我们自己的读数进 `reports/09` 协议。

## 4. 坑与反例（负面留档）

1. **退化三角形换固定拓扑 → 内存涨 + 顺序不定**（Ubisoft 自列，`W3A-078/079`）：统一化技巧的隐性代价。
2. **GPU 字形在小字号上不快**（Pathfinder 作者自述两问题，`W3A-067`）：我们以小字号候选行为主，故不走。
3. **overdraw 重排在 TBDR 上无效**（`W3A-009`）：后端不同，结论不能外推。
4. **遮挡查询要同步**（`W3A-048`）：GPU 剔除的收益可能被往返吃掉。
5. **GPU 渲染器评测多变量**（`W3A-057`）+ **vsync 队列即延迟**（`W3A-063`）：帧率数字不是全局指标。
6. **多渲染器矩阵导致特性无法跨平台上齐**（Rive 自述，`W3A-070`）：我们只做"一个显示列表、两执行器"正是为此。
7. **mesh shader 不是普适升级**（Khronos 原文劝退，`W3A-012`）；**Nanite 亦有实用上限**（`W3A-005`）。
8. **后处理 AA（MLAA 类）是替代品而非最优**（`W3A-051`）：有超采样就不必近似。

## 5. 对"我们"的取舍（CPU 基线 + GPU 可选档）

- **CPU 基线不受本批影响**：全部矢量/AA/OIT 机制在我们已有的 tiny-skia 式扫描线 + 覆盖率体系里都有等价物
  （`W3A-035/051` 是"更贵或更差的替代"，作为否证留档）。
- **GPU 档的准入与边界**（新增/加固）：
  1. 构建期编译全部着色器 + 管线预建（`W3A-072`）；
  2. 批键按"管线 > 绑定 > draw"成本序聚类（`W3A-022/021`），材质批键与 bindless 退路等价（`W3A-028`）；
  3. 分箱/tile 化借鉴 sort-middle（`W3A-055/056/058`），但 damage 精度以保守包围盒为准（`W3A-056`）；
  4. **不做** GPU 剔除/Nanite 式虚拟几何（`W3A-023/001`，量级不匹配）；
  5. **暂不做** GPU 矢量填充与 GPU 字形（`W3A-031/067`，候选窗不需要，且有已知慢场景）；
  6. present 策略按延迟优先（`W3A-062/063`）。
- **判据补充建议**：GPU 档验收除帧时间外，增"主线程 CPU 时间下降幅度"（吸收自 `W3A-019/077` 的立论），
  并附"分箱/damage 面积扰动"的可复算响应（借鉴 `W3A-046` 的扰动判据模板）。

## 6. 来源与抓取状态（本批 38 个 URL / 23 主题，按来源族归并）

| # | 来源（URL） | 类型 | 条数 | 抓取 |
| --- | --- | --- | --- | --- |
| 1 | advances.realtimerendering.com/s2021/index.html（Nanite 讲义页+摘要） | paper | 2 | 200 |
| 2 | advances.realtimerendering.com/s2021/Karis_Nanite_..._final.pdf（16.8MB） | paper | 4 | 200（多轮续传后完整；pdftotext 190KB 文本） |
| 3 | advances.realtimerendering.com/s2015/：讲义 PDF（aaltonenhaar_...220dpi.pdf）+ 课程页 index.html | paper | 5+1 | 200（PDF 首次截断，重下后完整） |
| 4 | meshoptimizer README（raw.githubusercontent） | doc | 4 | 200 |
| 5 | vkguide.dev/docs/gpudriven/gpu_driven_engines/ | doc | 6 | 200 |
| 6 | Khronos blog：Mesh Shading for Vulkan | paper | 3 | 200 |
| 7 | DirectX-Specs d3d/MeshShader.md | doc | 2 | 200 |
| 8 | gpuopen.com/presentations/2024/Mesh_Shaders_Work_Graphs-Perfect_Pair.pdf | paper | 2 | 200 |
| 9 | dev.epicgames.com Nanite 官方文档 | doc | 3 | 200 |
| 10 | gpuopen.com/gpu-work-graphs-in-vulkan/ | paper | 2 | 200 |
| 11 | Vulkan-Docs appendices/VK_EXT_descriptor_indexing.adoc | doc | 2 | 200 |
| 12 | api.crossref.org/works/10.1145/2366145.2366191（Kilgard TOG 2012） | paper | 3 | 200 |
| 13 | developer.nvidia.com GPU Gems 3 ch25（Rendering Vector Art） | paper | 3 | 200 |
| 14 | api.crossref.org 检索：Analytic Rasterization（CGF 2013） | paper | 1 | 200 |
| 15 | api.crossref.org 检索：Random-access rendering（TOG 2008） | paper | 1 | 200 |
| 16 | jcgt.org/published/0002/02/09/paper.pdf（Weighted Blended OIT） | paper | 3 | 200 |
| 17 | developer.download.nvidia.com OIT（Everitt 2001） | paper | 2 | 200 |
| 18 | api.crossref.org 检索：OIT 综述/链表 | paper | 1 | 200 |
| 19 | developer.nvidia.com GPU Gems 3 ch23（Off-Screen Particles） | paper | 2 | 200 |
| 20 | developer.nvidia.com GPU Gems 1 ch28（Graphics Pipeline Performance） | paper | 2 | 200 |
| 21 | developer.nvidia.com GPU Gems 1 ch29（Efficient Occlusion Culling） | paper | 2 | 200 |
| 22 | api.crossref.org 检索：A-buffer（Carpenter 1984） | paper | 1 | 200 |
| 23 | api.crossref.org/works/10.1145/166117.166147（Hierarchical Z-buffer） | paper | 1 | 200 |
| 24 | api.crossref.org 检索：Morphological antialiasing（HPG 2009） | paper | 1 | 200 |
| 25 | raphlinus.github.io modern-2d / sort-middle / fast-2d / compositor-is-evil / 2d-graphics / swapchain-frame-pacing | paper | 10 | 200 |
| 26 | pcwalton.github.io `_posts` 两篇（Pathfinder / 片元内三角形几何） | paper | 3 | 200 |
| 27 | rive.app/blog/rive-renderer-now-open-source-... | paper | 2 | 200 |
| 28 | vello.dev（GitHub 页） | paper | 1 | 200 |
| 29 | flutter/flutter impeller/README.md（raw） | doc | 2 | 200 |
| 30 | Vulkan-Docs chapters/fragops.adoc | doc | 1 | 200 |
| 31 | memononen/nanovg README（raw） | doc | 2 | 200 |

## 7. 未验证项（写明缺什么证据）

1. **Nanite 讲义 PDF 本体**：本机首次下载多次超时，经多轮 `curl -C -` 续传后取全（16,789,987 字节，`pdftotext -layout` 得 190KB 文本），
   已补 4 条正文条目（`W3A-082..085`）。**仍未取用**：讲座中的缩减/光栅化实现细节页（软件光栅器与硬件光栅器的取舍等）——如需再单独立项。
2. **NVIDIA《Introduction to Turing Mesh Shaders》**：原 URL 与备用 URL 均 404（留档），改以 Khronos 博文与 DX12 规范替代。
3. **Skia Graphite 设计文档**：`skia.org` 抓取连接失败；Graphite 的 SortKey 事实已在 `w1e`（源码锚），本批不重复。
4. **Intel OIT 文章**：403（反爬）；以 JCGT 论文与 Everitt 讲义替代。
5. **ARM Mali / PowerVR 官方性能文档**：403 / 空响应；TBDR 结论仅有 meshoptimizer README 一处旁证（`W3A-009`）。
6. **Crossref 检索引擎条目**（`W3A-035/036/042/049/051`）只取到**书目元数据**（标题/DOI/年/页），
   未读到论文正文——引用时只能支撑"存在该路线与出处"，不能支撑其数值结论。
7. **"120 fps"（Rive）等厂商目标数字**：官方博客自述，非独立实测（`W3A-069`）。
