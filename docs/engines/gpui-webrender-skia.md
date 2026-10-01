# gpui / WebRender / Skia+Graphite（高性能 GPU UI 与工业级 2D 引擎）

抓取 2026-10-01（静态阅读，只读；未执行上游代码、未构建）。

**上游 commit（本次锚定）**
- zed-industries/zed @ `06afd454ec3f23b7c97ce7527e9be9e1478564a4`，sparse 取 `crates/gpui`、`gpui_apple`、`gpui_macos`、`gpui_linux`、`gpui_platform`、`gpui_macros`（<https://github.com/zed-industries/zed>）
- servo/webrender @ `e1c924ebad9ffdfe8c8c606aba77eb3f888c396a`（**该仓自述为 mozilla-central `gfx/wr` 的 downstream mirror**，见 W1E-140；wiki 另 clone，内容无 commit 锚）（<https://github.com/servo/webrender>）
- google/skia @ `af9d144331802b134cd67e21521b10cd1d430315`，sparse 取 `docs/`、`src/core`、`src/text`、`src/gpu`（<https://github.com/google/skia>）

本报告每条主张对应账本行 `docs/analysis/ledger/w1e.jsonl`（id 形如 W1E-0NN）；未入账的主张一律不写。

## TL;DR（每条带锚）

1. **gpui 的 Scene 是"分桶 + 层号 + 归并批处理"**：8 类图元各自成 `Vec`，加一份 `paint_operations` 录制流；`insert_primitive` 先按 `content_mask` 裁剪剔除，再取层栈顶的 DrawOrder（同层共享）→ `finish()` 分桶排序 → `batches()` 8 路归并产出区间批（W1E-001/004/005/006/008）。
2. **gpui 的脏追踪是"布尔窗口脏 + 实体集合"**，不是脏矩形；窗口只在"由净转脏"那一次唤醒平台，脏粒度=实体（W1E-014/015/016/017）。
3. **gpui 的跨帧复用靠两样东西**：区间索引（`prepaint_index`/`paint_index` → `reuse_paint` → `Scene::replay`）与子树缓存键（id+bounds+content_mask+text_style 全等且未标脏才复用）（W1E-011/024/025/029/030）。
4. **gpui 的文本栈**：X 轴 4 档亚像素量化 + 字形光栅边界缓存 + 双帧布局缓存（previous/current frame，按索引区间搬键而非克隆）（W1E-026/031/033/034/035）。
5. **gpui 的 Window→GPU 路径极短**：`present()` 把 `&Scene` 交给 `PlatformWindow::draw`，平台层（如 macOS）只把它转交 MetalRenderer（W1E-020/021/036/041）。
6. **WebRender 的局部重绘最终由"分块内容描述符双缓冲比较"决定**：`Tile` 持三套矩形 + `current/prev_descriptor`，`pre_update` 里 `mem::swap` 后重算，`is_valid=false` 才重画（W1E-050/052/055/056）。
7. **WebRender 的脏区是单调 union 的单个世界矩形**（每帧 reset），脏区为空则整帧可跳过合成（`calculate_dirty_rects`）（W1E-058/060/082）。
8. **WebRender 的批键只有三个字段**（kind/blend_mode/textures），`BatchKind` 仅 4 个变体；纹理相容（而非相等）即可同批（W1E-065/067/068）。
9. **Skia 的 SkPicture = SkRecord + 可选 BBH**：录制时算保守边界批量插入 STR 批量建的 R-tree（min 6/max 11 子），重放时用画布裁剪域查一次 `search` 得到要执行的命令索引集（W1E-085/088/089/092/094/095）。
10. **Ganesh 合并是三态枚举 + 有界回看（10）**；Graphite 换成"排序键聚类"（64 位位域键，实测 24B 键排序比 16B 慢约 30%）（W1E-100/103/105/115/116）。

## 可吸收 / 不可吸收（对"青简候选窗/自绘渲染器 + 高帧率 UI"）

### 可吸收（含"有界吸收"，有界项须附带自己的判据）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| Scene 按图元种类分桶 + 同层共享 DrawOrder + 归并出区间批 | W1E-001/005/008 | **吸收**：实现小、与 CPU 光栅后端同构（批=连续区间，非 GPU 概念） |
| `insert_primitive` 单一入口做裁剪/空剔除 | W1E-003/004 | **吸收**：候选窗面积小，CPU 侧提前剔除直接省光栅化 |
| 子树缓存四元组键（id/bounds/content_mask/text_style）+ notify 失效契约 | W1E-029/030 | **吸收**：四键任一变化即失效，保守但可证正确 |
| 双帧文本布局缓存 + 按索引区间搬键（不克隆） | W1E-034/035 | **吸收**：与"候选窗每帧重排但文本多不变"的负载匹配 |
| 字形光栅边界先查缓存、空字形不入图集 | W1E-033 | **吸收**：实现成本低 |
| X 轴 4 档亚像素 / Y 轴 1 档 | W1E-031/032 | **有界吸收**：档位×图集大小的乘法关系要按候选窗实际字号算，不能默认 gpui 的取值最优 |
| 事件驱动帧模型（有变化才绘制）+ 空脏区跳过合成 | W1E-125/082 | **吸收**：是"高帧率"与"高功耗"的分界判据 |
| 平台后端只消费 `&Scene` 的接口边界 | W1E-021/036/041 | **吸收**：青简可让 CPU 光栅后端与未来 GPU 后端共用 Scene |
| tile 内容描述符双缓冲比较（prim_uid+clip box+依赖） | W1E-055/056/057 | **有界吸收**：字段集按青简图元模型裁剪，但"比较基准换位而非拷贝"应保留 |
| 裁剪掩码按屏幕空间 tile 惰性分配 | W1E-132/133 | **有界吸收**：对圆角/阴影类小面积裁剪价值大；对纯矩形候选窗收益小 |
| 批键三字段（kind+blend+textures）与 4 种批类型 | W1E-065/067/068 | **有界吸收**：CPU 侧无 blend 状态机，"纹理相容"对应的是图集页号 |
| SkPicture 录制 + BBH 索引 + 按裁剪查询重放 | W1E-085/092 | **有界吸收**：适合"内容稳定、局部更新"的场景；候选窗若每次全变则索引维护成本反超 |

### 不可吸收

| 项 | 锚 | 判定 |
| --- | --- | --- |
| WebRender 式任务树/pass 划分 | W1E-069/083/131 | **不吸收**：模糊/滤镜的离屏依赖链在候选窗量级不成立，引入即纯开销 |
| GPU 侧 early-z 顺序反转、深度读写 | W1E-080/129 | **不吸收**：CPU 光栅无 early-z，反转顺序反而破坏画家顺序前提 |
| slab 图集分配器（零碎片换 ~50% 浪费） | W1E-134/135 | **不吸收**（除非图集容量余量足够）：候选窗显存/内存预算紧，浪费 50% 不可接受 |
| 按 OpenGL 驱动成本论证的"批越多越好" | W1E-126/137 | **不吸收**：CPU 光栅无该成本结构，判据必须重立（平台差异亦不成立） |
| Skia 多页图集 + 页回收整套（quarter 判定） | W1E-108/109/110 | **不吸收**：单页够用时引入页管理只增加复杂度；先量再定 |

## 1. 架构全景（模块地图）

### 1.1 gpui（zed 编辑器 UI 库）

- `crates/gpui/src/scene.rs`：一帧的绘制数据容器。`Scene` = 8 类图元桶（shadows/quads/paths/underlines/monochrome_sprites/subpixel_sprites/polychrome_sprites/surfaces）+ `paint_operations` + `primitive_bounds`(BoundsTree) + `layer_stack`（W1E-001/002）。层用 `push_layer/pop_layer` 开闭，层内图元共享一个 DrawOrder（W1E-005）。
- `crates/gpui/src/element.rs`：`Element` trait 的三段式契约 request_layout → prepaint → paint，状态以关联类型在阶段间传递；带 `id` 的元素获得 `GlobalElementId`，才可跨帧跟踪（W1E-012/013）。
- `crates/gpui/src/window.rs`（承载 Window/Frame/脏追踪/绘制入口）：`Window::draw`（准备平台帧 → 失效实体 → 清脏 → `draw_roots` → 换帧）与 `Window::present`（把 Scene 交平台）分居两处（W1E-020/021）。脏追踪集中在 `WindowInvalidator`（W1E-014）。
- `crates/gpui/src/view.rs`：`ViewElement::cached(style)` 冻结子树，缓存键与失效判据在 `prepaint_view`/`paint_view` 内（W1E-029/030）。
- `crates/gpui/src/text_system.rs` + `text_system/line_layout.rs`：文本系统。亚像素档位常量、字形光栅边界缓存、`RenderGlyphParams` 作为图集键（W1E-031/033）；`LineLayoutCache` 双帧结构（W1E-034/035）。
- `crates/gpui_apple/src/metal_renderer.rs`：macOS/Metal 后端——实例缓冲池、逐批绘制、路径走 4x MSAA 中间纹理（W1E-037/038/039/040）。`crates/gpui_macos/src/window.rs` 只做转交（W1E-041）。`crates/gpui/src/platform.rs` 定义 `PlatformWindow`（`fn draw(&self, scene: &Scene)`）（W1E-036）。
- Linux 后端在 `crates/gpui_linux`（本次未读，见 §5）。

### 1.2 WebRender（servo/webrender 镜像）

- `src/lib.rs`：crate 级文档描述 Renderer/RenderApi/RenderApiSender 的分工与异步工作线程（W1E-042）。
- `src/scene_building.rs`：display list → 内部 scene，跑在 scene builder 线程；线性遍历（自然 back-to-front）+ stacking context 栈 + interning（W1E-043/044）。
- `src/tile_cache.rs`：picture caching 的**切片指派**（不是分块缓存本身）；`MAX_CACHE_SLICES=12`（W1E-045/046）。
- `src/picture.rs`（8 千行级）：真正的分块缓存——`Tile`、`TileDescriptor`、`DirtyRegion`、`TileCacheInstance`、`SubSlice`、`TileNode`（W1E-047…062）。
- `src/clip.rs`：clip-tree（nodes/leaves/root stack），场景构建期建、帧构建期消费（W1E-063/064）。
- `src/batch.rs`：批键与批列表（W1E-065…068）。`src/render_task.rs`：20 余种渲染任务类型，任务图是帧骨架（W1E-069）。
- `src/texture_cache.rs` / `texture_pack/` / `glyph_cache.rs`：共享纹理缓存（region 512）、图集分配器抽象、字形缓存只存句柄（W1E-070/071/072）。
- `src/gpu_cache.rs`：GPU 侧通用块缓存（顶点着色器可读），初始 20 行、扩容 +10 行、10 帧逐出、20% 低水位 + 5s 延迟回收（W1E-073…078）。
- `src/renderer/mod.rs`：`draw_frame` 入口、批容器绘制（不透明 front-to-back / 透明 back-to-front）、脏矩形计算与原生合成器路径（W1E-079…082）。`src/frame_builder.rs` 组装 pass（W1E-083），`src/spatial_tree.rs` 承载变换/滚动（W1E-084）。

### 1.3 Skia（Ganesh + Graphite + CPU）

- `src/core/SkRecord.{h,cpp}` / `SkRecordCanvas.cpp` / `SkRecordOpts.cpp` / `SkRecordDraw.cpp`：命令录制、录制期优化、重放（含 BBH 剪枝）（W1E-085/097/098/099/091/092/093）。
- `src/core/SkPictureRecorder.cpp`：录制入口、finish 时优化→算边界→建索引→收窄 cullRect（W1E-086…090）。`src/core/SkRTree.{h,cpp}`：默认 BBH（STR 批量建树）（W1E-094/095/096）。
- `src/gpu/ganesh/ops/`：Ganesh 的 op 与批处理调度（`GrOp.h`、`OpsTask.cpp`、`FillRectOp.cpp`）（W1E-100…107）；`src/gpu/ganesh/GrDrawOpAtlas.*` 字形/小图元图集（W1E-108…111）。
- `src/text/gpu/`：SubRun/GlyphVector/StrikeCache（图集约束下单线程）（W1E-121）。
- `src/gpu/graphite/`：新后端——`DrawList.{h,cpp}` 排序键聚类（W1E-112…118）、`Renderer.h` RenderStep 聚批（W1E-119/120）、`ComputePathAtlas.h` 计算式路径覆盖掩码、`compute/VelloRenderer.h` 内嵌 Vello 的 C++ 实现（W1E-122/123）、`text/TextAtlasManager.h`（W1E-124）。
- `docs/architecture/CPU.md`：官方 CPU 后端架构文档（分块策略的官方定义）（W1E-141）。

## 2. 关键机制

### 2.1 分层与合成模型

- **gpui**：唯一的"层"是 `Scene::push_layer` 产生的 DrawOrder 分组（W1E-005），语义是"同一 DrawOrder 内的图元共享同一覆盖层"。没有通用的离屏 surface 概念：`Surfaces` 桶是"把外部图像缓冲（CVPixelBuffer）当图元画入"，且只在 macOS/iOS 编译（W1E-143）。合成由 GPU 后端按批顺序逐条绘制完成（W1E-039）。
- **WebRender**：三层结构——stacking context（场景层）/ picture cache slice（分块缓存层）/ render task（离屏任务层）。slice 上限 12，超出退化为单 slice（W1E-046）；每个 picture cache 最多 4 个合成器表面（W1E-048）；单个 surface 单轴上限 4096（W1E-049）。离屏依赖用任务树表达，pass 数=树深（W1E-131/138）。
- **Skia**：SkPicture 是"录制/重放"分层（无缓存语义，缓存由调用方或 SkImageFilter 层负责）；Ganesh 的"层"是 OpsTask（≈render pass）与 op 链（W1E-100/101）；Graphite 的层是 DrawPass 与 RenderStep（W1E-119）。
- **对青简的取舍含义**：候选窗是"小面积、图层少、无滤镜链"的负载，**应采用 gpui 式的单 Scene + 图元分桶**；WebRender 的 slice/任务树只有在出现"大面积滚动 + 局部更新"时才值得引入；Skia 的录制/重放只在"内容稳定却要反复重画"时有意义。

### 2.2 批处理键（三种范式）

| 系统 | 批等价判据 | 边界触发 | 锚 |
| --- | --- | --- | --- |
| gpui | (DrawOrder, 图元种类[, 图集页号]) | order 变化 / 纹理变化 | W1E-008/009/010 |
| WebRender | BatchKind + blend_mode + 纹理集合相容 | 键不等 | W1E-065/067/068 |
| Ganesh | `GrOp::CombineResult` 三态 + 有界回看（10 链）+ 相交即停 | 不可合并 / 相交 / 越界 | W1E-100/103/104/105 |
| Graphite | 64 位 SortKey（画家序号+模板序号+RenderStep+管线索引+绑定索引） | 键分段变化 | W1E-115/118 |

- **对青简的取舍含义**：CPU 光栅没有"状态切换"成本，但有**字形图集页切换**与**混合/裁剪切换**成本。gpui 的键（order + kind + 页号）是最小充分集；Skia 的位域排序键是"键越小越快"的工程证据（W1E-116），若青简要排序，键应控制在 16 字节内。

### 2.3 文本图集与字形缓存

- gpui：字形以 `RenderGlyphParams`（字体/字形/字号/缩放/亚像素变体/是否亚像素渲染/dilation）为键查图集；先取光栅边界，零面积不入图集；亚像素与灰度分成两种 sprite 桶（W1E-026/028/033）。亚像素前置条件：背景不透明 + 平台支持 + 文本模式（W1E-027）。
- WebRender：字形格式是批键的一部分（W1E-066）；`CachedGlyphInfo` 只存格式 + 纹理缓存句柄，被淘汰时以 EvictionNotice 通知（W1E-071）；图集分配器可替换（guillotine→slab）（W1E-072/134）。
- Skia：A8 掩码图集尺寸恒为 ARGB 的 2 倍（W1E-109）；plot 尺寸随图集尺寸分层（≥2048 用 512，否则 256；ARGB/LCD 恒 256，理由是实测更快）（W1E-110/111）；页回收按"近若干次 flush 使用不足 1/4"判定（W1E-108）；StrikeCache 因图集必须单线程（W1E-121）。
- **对青简的取舍含义**：候选窗字号统一、文本短，**图集键可只用（字体, 字号, 字形 id, 亚像素档）**；亚像素档位取 4 已是上游默认，Y 轴取 1 可省变体（W1E-032）。淘汰通知（EvictionNotice）这类机制在单线程 CPU 渲染器里可退化为"句柄失效即重建"。

### 2.4 脏追踪与局部重绘（三档力度）

1. **gpui：实体粒度 + 布尔脏**——窗口只记 dirty_views 集合与一个 dirty 布尔；重绘是"整窗 + 子树复用"（W1E-015/016/017）。复用粒度=带 id 的子树（W1E-013/030）。
2. **WebRender：分块粒度 + 内容指纹**——tile 内容描述符双缓冲比较（prim_uid + clip box + 依赖数据）（W1E-055/056/057）；脏区在 picture 空间逐块累积、映射到世界空间后单调 union 成单矩形（W1E-058/059/060）；每帧 reset 后重算（W1E-062）；`calculate_dirty_rects` 决定是否需要合成（W1E-082）。
3. **Skia：录制粒度 + 空间索引**——BBH（R-tree）把"要重放哪些命令"变成一次矩形查询（W1E-091/092）；索引只在录制结束时建一次（W1E-089）。
- **对青简的取舍含义**：候选窗的"内容稳定、局部更新"特征与 Skia 的 BBH 模型最贴合（查询一次、跳过整条命令），而 WebRender 的分块模型只有在窗口大面积（滚动列表）时才回本。gpui 的实体粒度**已能覆盖"候选窗整体重画"的实际负载**——先落地实体级缓存，再谈分块。

### 2.5 GPU 后端抽象代价（对"是否值得抽 GPU 后端"）

- gpui 的抽象边界极窄：`PlatformWindow::draw(&Scene)`（W1E-036），平台实现里才出现 Metal 细节（W1E-041）；**这个边界不是免费的**：`Scene` 的字段必须对 GPU 友好，例如面向上传的结构体里布尔被显式存成 `u32`（`PaddedBool32`）以避免填充字节（W1E-142）。
- Metal 后端的代价数字：实例缓冲块默认 2MB、上限 256MB，超出直接报错（W1E-038）；路径批要切成"中间纹理 + 4x MSAA + 合成"两次渲染（W1E-037/040），即**矢量路径在 GPU 侧仍有专门的旁路成本**。
- Skia 的两代后端给出另一种证据：Ganesh 的合并靠 op 自身实现 `onCombineIfPossible`（默认不可合并，W1E-102），Graphite 把它换成"统一排序键聚类"（W1E-114），并把路径覆盖掩码交给计算管线（W1E-122/123）——**同一批处理目标，两代实现的抽象位置不同**。
- **对青简的取舍含义**：若青简短期只做 CPU 光栅，收益最大的不是"抽 GPU 后端"，而是**把 Scene 的表示定成后端友好（分桶 + 区间 + 图集页号）**，使将来接 GPU 后端时上层零改动（这正是 gpui 的做法）。

## 3. 性能手段与公开读数（数字全部带锚与口径）

| 读数 | 值 | 口径/出处 | 锚 |
| --- | --- | --- | --- |
| gpui 热压力帧间隔上限 | 16667 µs（≈60Hz） | 代码常量，热压力档 | W1E-019 |
| gpui 实例缓冲 | 默认块 2MB，上限 256MB；超限报错 | 代码常量 + 错误路径 | W1E-038 |
| gpui 路径 MSAA | 4x（注释称所有设备支持） | 代码常量 | W1E-037 |
| WebRender tile 尺寸 | 1024×512 设备像素（滚动条另两种） | 代码常量 | W1E-047 |
| WebRender slice 上限 | 12 | 代码常量 + 注释承认"arbitrary" | W1E-046 |
| WebRender 合成器表面 | 每 cache ≤4；单 surface ≤4096 | 代码常量 | W1E-048/049 |
| WebRender 纹理缓存 region | 512 | 代码常量 | W1E-070 |
| WebRender GPU cache | 初始 20 行、扩容 +10 行、10 帧逐出、20%/5s 回收 | 代码常量 + 作者实测口径"Firefox 启动约 15 行、浏览升到 30 多行" | W1E-073/074/075/076/077 |
| Ganesh 合并回看上限 | 10 链 | 代码常量 | W1E-103 |
| Ganesh 字形图集 plot | A8：≥2048 宽用 512，否则 256；ARGB/LCD：256（实测更快） | 代码常量 + 注释理由 | W1E-110/111 |
| Skia SortKey | 24 字节；比 16 字节慢约 30% | 作者实测（自述桌面环境），注释留档 | W1E-116 |
| Skia R-tree 扇出 | min 6 / max 11；STR 批量建 | 代码常量 + 注释 | W1E-094/095/096 |
| WebRender 上传对齐 | 256 字节（不满足走慢路径或驱动 bug） | wiki 设计笔记 | W1E-136 |
| slab 图集浪费 | 典型 ~50%（换零碎片） | wiki 设计笔记 | W1E-135 |
| 上传路径平台差异 | Intel：PBO 更快；AMD：PBO 最坏情况更快、简单情况更慢 | wiki 引用 @nical 实测 | W1E-137 |
| 裁剪 tile 网格 | 128×128，惰性分配（例：1000×1000 圆角只分配 4 个角） | wiki 设计提案（非实现现状，见 §4） | W1E-132/133 |

> 口径声明：上表除 wiki 设计笔记三行（W1E-135/136/137）外均为**本机静态读到的源码常量/注释**，**不是**本机复测读数；无 GPU 硬件、未做基准（见 §5）。

**改善手段清单（按收益从高到低，对本项目）**
1. 空脏区跳过整帧合成（W1E-082）+ 事件驱动帧（W1E-125）：直接砍掉无变化帧的全部成本。
2. 子树级缓存（四键 + notify 契约）（W1E-029/030）：文字输入的候选窗通常只有一行变化。
3. 文本布局双帧缓存（W1E-034/035）+ 字形边界缓存（W1E-033）：避免每帧重新整形。
4. 图元分桶 + 归并批处理（W1E-008/009）：把"每图元一次绘制调用"压成"每批一次"。
5. 保留录制流 + 区间复用（W1E-011/025）：为 GPU 化或异步光栅留接口。

## 4. 坑与反例（负面留档）

1. **上游文档可能滞后于代码**：wiki 的 Path-to-the-Screen 仍在讲 `stacking_context_store`/`packed_layers`/`ScreenTile::compile` 等旧名（W1E-130/131/138 的正文语境），与当前源码结构（`scene_building.rs`/`render_task_graph.rs`）不一致。→ 引用 wiki 只能作**设计意图**旁证，不能当实现事实（W1E-140 同族限定）。
2. **"批处理必要性"绑定在 OpenGL 驱动成本上**：Home 页写明"Necessary for good performance with OpenGL"（W1E-126）。→ 迁移到 CPU 光栅时该前提失效，**不能照搬"批越多越好"**。
3. **脏区被合并成单个矩形**：DirtyRegion 只留 `combined` 一个世界矩形（W1E-058），注释里还留着"曾有多矩形/四叉树"的 TODO（W1E-060 语境）。→ 大范围滚动时脏区会被放大成近乎全屏，这是"简单换精度"的显式代价。
4. **保守缓存键会让小改动付整树代价**：gpui 的缓存键含 content_mask 与 text_style（W1E-030）。→ 样式微变（如 hover 改字号）会整树重画，需在青简侧对"哪些样式进入键"做定量裁剪。
5. **硬上限直接失败**：gpui 实例缓冲超 256MB 即报 "scene too large"（W1E-038）。→ 这是"可见失败优先于静默降级"的选择；青简若设上限，错误信息应同样带上分项计数。
6. **平台差异符号相反**：PBO vs glTexSubImage 在 Intel/AMD 上优劣相反（W1E-137）。→ 上传策略必须分平台，且需本机复测（本项目将来若要测，别一次调优到底）。
7. **slab 分配器的代价写在注释里**：~50% 空间浪费（W1E-135）。→ 候选窗的内存预算若按"可用面积"估算，实际需求要乘 ~2。
8. **对齐约束是隐形的**：256 字节行对齐不满足会走驱动慢路径（W1E-136）。→ 小图元逐个上传得不偿失，**必须成页打包**。
9. **共享图集 = 单线程约束**：Skia 明确 StrikeCache 只能单线程（W1E-121）。→ 青简若要多线程光栅字形，分配权必须先集中。
10. **合并默认关闭 + 相交即停**：Ganesh 的 op 默认不可合并（W1E-102），且与已有链相交就停止回看（W1E-104）。→ 顺序敏感内容（如半透明叠加）天然无法跨批，这是画家顺序的硬边界，不要试图绕开。
11. **"为合并而提升质量"是单向的**：Ganesh 允许把硬边 AA 升级成 coverage AA 以促成合并，反向不允许（W1E-107）。→ 若青简做类似松弛，必须写死"只升不降"。
12. **索引算法只试了一种**：SkRTree 注释列出 VAMSplit/TopDownGreedy 等未试变体（W1E-094）。→ 不要把它当"最优已证"结论引用。

## 5. 未验证项（缺什么证据）

1. **无任何本机复测读数**：全部结论来自静态阅读；未构建、未执行、无 GPU/Metal 环境，§3 表的时序类数字（如 60fps 档、SortKey 30%）均为上游自述。
2. **gpui Linux/Windows 后端未读**：`crates/gpui_linux`（Blade/Vulkan）与 Windows 后端已在 sparse 清单但本次未展开；因此"平台层不含渲染逻辑"的结论只对 macOS 路径有直接证据（W1E-041）。
3. **WebRender 软件后端（swgl）未读**：仓内 `swgl/` 目录存在但未纳入本次阅读，故"CPU 光栅在上游的成熟度"无证据。
4. **wiki 内容无 commit 锚**：GitHub wiki 不在代码 clone 内，引用行号为本地 wiki clone 的行号，抓取日期 2026-10-01，内容可能随时变化（W1E-125…139 全部受此限制）。
5. **Graphite 是否默认启用 compute path atlas 未验证**：代码存在（W1E-122/123），但未读 `ContextOptions`/`RecorderOptions` 的默认值与 `SK_ENABLE_SPARSE_STRIPS` 开关状态。
6. **Skia include/ 未取**：`GrOp.h` 的部分语义（如 `chainConcat` 实现、`GrDrawOp::fixedFunctionFlags` 全貌）只看声明与调用点，未读头文件全文。
7. **CJK 量级无数据**：三份系统的文本结论都在"西文/通用"语境下（上游注释未给 CJK 读数）；中文候选窗的**字形变体数、图集压力、整形耗时**需要青简侧自测，不能引用本报告的常量外推。
8. **未验证项的时间戳**：以上"未验证"状态对应 2026-10-01 的阅读范围，不作为永久事实（后续补读即可否证）。
