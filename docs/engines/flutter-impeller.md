# Flutter + Impeller（https://github.com/flutter/flutter @ 4e5a09292c1c64a38d604c7ec096537f50d69f93, 抓取 2026-10-01）

> 轨道：B2。抓取方式：`git clone --depth 1 --filter=blob:none --sparse` 到
> `D:/KF/LSSMJ/scratch/src/flutter`，`git sparse-checkout set packages/flutter/lib/src/rendering
> packages/flutter/lib/src/painting packages/flutter/lib/src/widgets packages/flutter/lib/src/scheduler
> engine/src/flutter/impeller engine/src/flutter/lib/ui engine/src/flutter/flow engine/src/flutter/docs`；
> `git rev-parse HEAD` = `4e5a09292c1c64a38d604c7ec096537f50d69f93`（W2B-120）。
> **注意**：本仓已把 engine 并入主仓（Impeller 实际路径为 `engine/src/flutter/impeller/`，
> 不是任务书里写的 `engine/src/flutter/impeller` 之外的位置；`//impeller/aiks` 目录**已不存在**，
> 更名为 `display_list`，源码内已有注释留档，见 W2B-050）。
> 三个文件（`shell/common/vsync_waiter.h`、`shell/common/animator.h`、`flow/raster_cache_util.cc`）
> 不在 sparse 集合内，用 python urllib 按同一 commit 拉 raw 存于 `D:/KF/LSSMJ/scratch/web/engine/`
> （source 条目逐行引用，路径见账本）。
> 官方文档正文用 python urllib 抓 HTML 转文本存 `D:/KF/LSSMJ/scratch/web/`（doc 条目引文出自该副本；
> 站点自述「reflects Flutter 3.47」，各页 last updated 未逐页记录）。
> 账本：`docs/analysis/ledger/w2b.jsonl`（**120 条**，source=103 / doc=15 / web=2，rejected=0，
> 2026-10-01 复算：`python tools/ledger.py verify --file docs/analysis/ledger/w2b.jsonl`）。

## TL;DR（每条带锚）

1. **框架与引擎的分界线是一条不可变绘制包**：`PaintingContext` 负责把渲染对象录进 `PictureLayer`
   （W2B-001/007），`endRecording()` 把它固化成不可变 `ui.Picture`（W2B-008），图层树再整体
   下发给引擎（W2B-018/025）。**中文字形/整形完全不在这一侧**——Impeller 自述只渲染「已整形的
   run」（W2B-049）。
2. **`RepaintBoundary` 的语义是「脏标记的冒泡上界」**：`markNeedsPaint` 只在「现在是且曾经是」
   边界时才把自身加进 `_nodesNeedingPaint`（W2B-009/010）；非边界一律冒泡给 parent（W2B-009）。
   代价模型同样明确：被标脏的边界会**在同一图层里重画其全部后代**（W2B-021）。
3. **「内容脏」与「属性脏」是两条独立通道**：`markNeedsCompositedLayerUpdate` 只改图层参数
   （典型用例 `RenderOpacity`，W2B-013/024），`markNeedsPaint` 才重录；框架对边界层强制
   「实例复用」契约（创建新层是断言级错误，W2B-012）。
4. **Impeller 取消了框架级光栅缓存**：整个 `RasterCache` 与 `LayerRasterCacheItem` 被
   `#if !SLIMPELLER` 包住（W2B-040/041）——RepaintBoundary 在 Impeller 下的收益主要是
   「限制重录范围」，不再有位图图层复用；Skia 路径的位图缓存每帧上限是 3 条（W2B-042）。
5. **脏区是「双账」**：`Damage` 区分 frame_damage（帧间差，对应 `EGL_KHR_partial_update` 的
   surface damage）与 buffer_damage（本帧实际写到 = 帧差+已累积脏区，W2B-036/037）；带滤镜时
   还要把 readback 区域整块计入（W2B-038）。
6. **Impeller 用深度缓冲表达嵌套裁剪**：`Entity.kDepthEpsilon = 1/262144`，第 n 层裁剪映射成
   深度上的极小增量（W2B-054/055），因此裁剪不需要打断 render pass；被裁空的实体在 CPU 侧
   直接跳过（W2B-079）。
7. **全屏纯色可以不画**：`Contents::AsBackgroundColor` 允许把「铺满目标的纯色」吸收进 subpass 的
   clear 颜色（W2B-062），实现是「反预乘→Blend→预乘」三段式（W2B-067/068）；同处还有一次
   自动降级 kSrcOver→kSrc（不透明内容，W2B-069）。
8. **saveLayer 的透明度有「窥孔」快路径**：条件满足时根本不建离屏，只把 alpha 累乘进
   `distributed_opacity`（W2B-070/071），判定条件写死为四条可枚举的排除项（W2B-072）。
9. **曲线细分用 Wang 公式按像素误差给界**（公式与出处写在头文件注释里，W2B-084/085），
   `scale_factor` 取当前变换的最大 XY 基，因此是分辨率无关的质量保证；MSAA 统一 4x
   （W2B-099/100，官方文档同口径 W2B-109）。
10. **帧调度是「两段计时 + 幂等排帧」**：`FrameTimingsRecorder` 用状态机记录
    vsync→buildStart→buildEnd→rasterStart→rasterEnd，并保存「本帧期望呈现时刻」（W2B-032/033）；
    `scheduleFrame` 靠 `_hasScheduledFrame` 去重、`framesEnabled` 可整体闸断（W2B-029）；
    官方帧预算口径为「构建 8ms + 渲染 8ms ≤ 16ms」（延迟导向，W2B-112/113）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 「内容脏 / 属性脏 / 位置脏」三分离，属性脏不重录 | W2B-013/024/006 | **吸收**：候选窗高亮移动、页码切换、主题色替换都属属性脏，可只改参数 |
| 脏标记冒泡到最近边界即停 | W2B-009 | **吸收**：自绘渲染器可把「拼音行 / 候选行 / 页脚」各设一个逻辑边界 |
| 先聚合脏标记再下发（`updateSubtreeNeedsAddToScene`→`addToScene`） | W2B-018 | **吸收**：两趟可避免深度递归重复判断 |
| 「估计边界」只做提示、不做裁剪保证 | W2B-002/061 | **吸收**：把 prompt 与 guarantee 分成两个输入通道，避免优化越界 |
| 全屏纯色吸收进 clear、不透明自动降级 kSrc | W2B-062/067/068/069 | **吸收**：候选窗背景/页脚色可零成本 |
| 透明度窥孔（可枚举四条件 → 折叠掉离屏） | W2B-070/071/072 | **吸收**：半透明候选窗高亮最容易踩 saveLayer |
| 深度编码嵌套裁剪（1/262144 步长） | W2B-054/055 | **有界吸收**：CPU 自绘无深度缓冲；仅当未来上 GPU 后端时适用 |
| 脏区膨胀到 readback 区域（滤镜读域） | W2B-038 | **吸收**：毛玻璃/阴影类效果做脏区时必须扩到读域 |
| 缓存填充限速（每帧最多 3 条） | W2B-042 | **吸收**：一次性填满缓存会造尖峰帧，应按帧限额 |
| 字形图集「先排版、后定址」两阶段 | W2B-089/090/091 | **吸收**：同一帧内所有文本共享一次图集构建 |
| 曲线细分按屏幕像素误差解析给界 | W2B-084/085 | **吸收**：分辨率无关的曲线质量证明，可直接移植数学 |
| 几何自重叠由生产者声明（四种 Mode） | W2B-063 | **吸收**：省掉昂贵的运行时重叠判定 |
| 分数像素描边用 alpha 覆盖率补偿 | W2B-064/065 | **吸收**：1px 网格线/分隔线的便宜解法 |
| 不透明元素逆序渲染以获得遮挡剔除 | W2B-080/081 | **有界吸收**：需先证明「顺序无关」才可重排，CPU 自绘收益小 |
| 显式 LayerHandle 引用计数管理长生命周期资源 | W2B-019 | **吸收**：长驻缓存不泄漏的前提 |
| 提交点唯一 + 提交后立即释放（`view.render` → `scene.dispose`） | W2B-026 | **吸收**：一帧一次提交，资源账清晰 |
| 设备能力表集中探测、逐项列举 | W2B-097 | **吸收**：但 CPU 自绘只需一张很小的能力表 |
| RasterCache / 位图图层复用 | W2B-040/041 | **不吸收**：Impeller 自己都取消了；自绘渲染器的等价物应是自己实现的 tile 缓存 |
| 两趟 DisplayList dispatcher（FirstPass + 渲染趟） | W2B-095/096 | **不吸收**：已知两趟裁剪不一致（W2B-118），一致性成本高于收益 |
| Framebuffer fetch / blit-to-onscreen 类后端特化快路径 | W2B-074/075 | **不吸收**：无 GPU 后端，无对应问题 |
| 模板缓冲 + 模板重放恢复 backdrop | W2B-078 | **不吸收**（CPU 自绘）；但「裁剪状态必须随 backdrop 一同恢复」这条语义是吸收的 |
| 运行时着色器（RuntimeStage + 用户 fragment shader） | W2B-101 | **不吸收**：青简自绘无用户着色器契约 |
| Skia 时代的 ShaderWarmUp（100–200ms 启动成本） | W2B-043/044/045 | **不吸收**（是反例留档）：只有运行时编译着色器才需要它 |

## 1. 架构全景（模块地图，逐文件职责）

分四层，边界清楚：

**① Dart 框架 / 渲染对象层**（`packages/flutter/lib/src/rendering/`）
- `object.dart`（6886 行）：`RenderObject` 生命周期 + `PipelineOwner`（帧内四段 flush，
  W2B-015）+ `PaintingContext`（录制上下文，W2B-001）。
- `layer.dart`（3017 行）：层树。`PictureLayer` 是叶子（W2B-016），`ContainerLayer.buildScene`
  是每帧唯一下发点（W2B-018）；未脏图层走 `addRetained`（W2B-017）。
- `proxy_box.dart`：`RenderRepaintBoundary` 只做一件事——`isRepaintBoundary => true`（W2B-023）。
- `view.dart`：`RenderView.compositeFrame()` 是提交点（W2B-025/026）。
- `binding.dart`：`drawFrame()` 四行固定顺序（W2B-030/031）。

**② Dart 框架 / 绘制与调度层**
- `painting/`：`TextPainter.layout({minWidth,maxWidth})`（W2B-121/122/123）、
  `PaintingBinding.shaderWarmUp`（Skia 时代产物，W2B-043/044/045）。
- `scheduler/binding.dart`（2194 行）：`SchedulerPhase` 五态（idle / transientCallbacks /
  midFrameMicrotasks / persistentCallbacks / postFrameCallbacks，W2B-027/028），
  `handleBeginFrame`/`handleDrawFrame` 双段（W2B-028/029）。
- `widgets/basic.dart`：`RepaintBoundary` widget 与其语义长注释（W2B-021/022）。

**③ dart:ui 桥 + 引擎合成层**（`engine/src/flutter/lib/ui/`、`engine/src/flutter/flow/`）
- `lib/ui/painting.dart`：`PictureRecorder`（抽象接口 + native 实现，W2B-124/125）、`Picture`；
  `lib/ui/compositing.dart`：`SceneBuilder.addPicture(offset, picture, {isComplexHint, willChangeHint})`
  （W2B-126/127）。
- `flow/`：`compositor_context.h`（`FrameDamage::ComputeClipRect` 树级 diff，W2B-039）、
  `diff_context.h`（`Damage` 双账 + readback 膨胀，W2B-036/037/038）、
  `frame_timings.h`（状态机计时，W2B-032/033）、`raster_cache.h`（**`#if !SLIMPELLER`**，W2B-040）。
- `shell/common/vsync_waiter.h`：平台 vsync 收口成 `AwaitVSync()` 纯虚（W2B-034/035）。

**④ Impeller**（`engine/src/flutter/impeller/`，README 自述五个目标 W2B-046/047）
- `geometry/`：后端无关数学库，POD 布局可直接拷进设备内存且刻意模仿 GLSL（W2B-102）；
  `wangs_formula.h` 曲线细分（W2B-084/085）。
- `tessellator/`：`path_tessellator.h` 的 `SegmentReceiver`（四类段显式建模，W2B-087）、
  `PathToFilledVertices` + `CountFillStorage` 两段式（W2B-086）。
- `entity/`：`Entity`（内容+变换+混合+裁剪深度，W2B-053）、`Contents`（自己下命令，
  W2B-059/060/061/062）、`geometry/`（生产者声明重叠语义，W2B-063）、
  `entity_pass_clip_stack.h`（裁剪重放，W2B-078/079）、`draw_order_resolver.h`（重排绘制，
  W2B-080/081）。
- `display_list/`：`canvas.h`/`canvas.cc`（2842 行，pass 管理 + 优化集中地，W2B-066~077）、
  `dl_dispatcher.h`（两趟 dispatcher，W2B-095/096）、`paint.h`（窥孔判定，W2B-072）。
- `renderer/`：`capabilities.h` 能力表（W2B-097/098）、`render_target.cc` 4x MSAA（W2B-099）。
- `typographer/`：字形图集（alpha/color 两套，W2B-089/090/091）、`text_frame.h` 入口（W2B-092）。
- `compiler/`（host 侧 impellerc，W2B-103/048）、`shader_archive/`、`toolkit/`、`playground/`。

## 2. 关键机制

### 2.1 重绘边界：脏标记的上界 + 图层实例的归属
`markNeedsPaint()` 只有两条出口：是（且曾是）重绘边界 → 进 `_nodesNeedingPaint` 并
`requestVisualUpdate()`；否则冒泡给 parent（W2B-009/010）。`flushPaint` 把脏节点按**深度倒序**
处理（W2B-010），保证父节点不会把已重画的子节点再抹掉。边界层由框架托管：`updateCompositedLayer`
必须原地复用实例（W2B-012），层内容重建用 `removeAllChildren()` + 重录（W2B-004）。
一个真实陷阱：`flushPaint` 遇到已 detach 的层会走 `_skippedPaintingOnLayer()`，把整段 detached
子树**沿祖先边界重新标脏**，否则重新挂回时不会重画（W2B-014）。
边界的父子分叉只有一个点：`paintChild` 判断子是否边界，是则 `stopRecordingIfNeeded()` 后进
`_compositeChild`（子有自己的层），否则直接 `_paintWithContext` 录进当前 `PictureLayer`（W2B-005）；
`_compositeChild` 内部再分「内容脏→重画」与「仅属性脏→`updateLayerProperties` 不重画子树」两条
路径，并复用 `OffsetLayer` 实例只改 `offset`（W2B-006）。边界的重画入口
`repaintCompositedChild` 会连带复用该子树内不需要重画的图层（W2B-003）。

### 2.2 录制模型：惰性开录 → 不可变 picture → 保留复用
`PaintingContext._startRecording()` 只在真正需要 canvas 时才建 `PictureLayer` 与 recorder
（W2B-007）；`stopRecordingIfNeeded()` 用 `endRecording()` 固化（W2B-008）。
`ContainerLayer.buildScene()` 先 `updateSubtreeNeedsAddToScene()` 自底向上聚合脏标记再
`addToScene()`（W2B-018）；未脏图层 `builder.addRetained(engineLayer)` 整段复用上一帧的引擎层
（W2B-017）。`estimatedBounds` 注释明确**允许画到界外**，只是调试观测量（W2B-002）。

### 2.3 Impeller 的 pass 模型与「把绘制折叠成状态」
`Canvas`（display_list）持 `transform_stack_`、`save_layer_state_`、`backdrop_data_` 与一个
`EntityPassClipStack`（W2B-066/078）。每次实体入 pass 前做三件事（W2B-069/067/068）：
① kSrcOver + 不透明 → 改 kSrc；② 若当前 pass 仍在「可改清除色」阶段且该内容能
`AsBackgroundColor`（铺满目标的纯色）→ 只改 `clear_color` 并**直接 return，不产生绘制**；
清除色的合并必须走「反预乘 → Blend → 预乘」（W2B-068）。裁剪深度在入 pass 时单调递增
（`++current_depth_`，W2B-128）并断言不超过当前裁剪深度（W2B-129）。
高级混合按能力分叉：支持 framebuffer fetch 就原地混合，否则 `FlipBackdrop` 多一趟
（W2B-073/074）。`Flip` 对非 MSAA 目标是显式错误（W2B-082），第二颜色纹理惰性分配并可回收
（W2B-083）。
「能不能把中间纹理按内容区收缩」取决于混合模式：`IsBlendModeDestructive` 把
kClear/kSrc/kSrcIn/kDstIn/kSrcOut/kDstOut/kDstATop/kXor/kModulate 列为「即使源色全透明也会改
目标」的黑名单，只有全部实体都非 destructive 时 `EntityPass` 纹理才可收缩（W2B-058）。
`Contents::GetCoverage` 返回 `nullopt` 的语义是「画了等于没画」，框架据此整条丢弃（W2B-060）；
`CoverageHint` 则是「仅供优化、不得当裁剪」的提示通道（W2B-061）。
透明度传播还有一个语义冲突点：`SetInheritedOpacity` 遇到「kSrc + 内容不透明」会把混合模式
降级为 kSrcOver，否则层级透明度无法正确传播（W2B-057）。

### 2.4 tessellation 策略
分层：`path_tessellator.h` 定义 `SegmentReceiver`（line/quad/conic/cubic 四类段，成对
Begin/EndContour 包裹，W2B-087）与 `VertexWriter`；`CountFillStorage` 先算容量再
`PathToFilledVertices` 直写（W2B-086）。细分步数由 `wangs_formula.h` 的
`Compute{Cubic,Quadradic,Conic}Subdivisions(scale_factor, ...)` 解析给出（W2B-084/085），
`scale_factor` 是当前变换的最大 XY 基 → 屏幕空间误差界。几何层用
`GeometryResult::Mode`（kNormal / kNonZero / kEvenOdd / kPreventOverdraw）声明「是否自重叠」，
由生产者负责，框架不猜（W2B-063）。描边额外开了接收 `Arc` 段的专用接收器
（`PathAndArcSegmentReceiver`，W2B-088），细线有 1.0 的最小尺寸约定（W2B-064）与 alpha 覆盖率
补偿（W2B-065）。

### 2.5 文本
Impeller **不做排版与整形**（W2B-049），接口是 `TextFrame` =「已整形 run 的集合」（W2B-092）。
字形图集两类：kAlphaBitmap（单通道 8 位）与 kColorBitmap（N32 预乘，彩色字形/emoji）
（W2B-089）；`FrameBounds.atlas_bounds` 允许先是占位值再回填（`is_placeholder`，W2B-090），
`LazyGlyphAtlas` 按 alpha/color 两份分别累积 `renderable_frames`，到提交前一次性装箱
（W2B-091）。`TextContents` 需要同时拿「实体变换」与「屏幕空间变换」（W2B-094），并要求
亚像素定位口径与入图集时一致——否则是跨帧抖动类 bug 的来源（W2B-094 的 lesson）。
框架侧排版入口是 `TextPainter.layout({minWidth, maxWidth})`（W2B-121），命中布局缓存则直接返回
（W2B-121），真正计算落到引擎侧 `ui.Paragraph`（W2B-122）；`paint` 前必须已 layout，否则抛
StateError（W2B-123）。

### 2.6 帧调度与耗时记账
`SchedulerPhase` 是五态枚举（W2B-027）；`handleBeginFrame` 先把 `_hasScheduledFrame=false`
再整体替换 transient 回调表执行（本帧新登记的回调不在本帧跑，W2B-028）；`scheduleFrame` 幂等且
受 `framesEnabled` 闸断（W2B-029）。`RendererBinding.drawFrame()` 四行：flushLayout →
flushCompositingBits → flushPaint →（sendFramesToEngine 时）各 RenderView.compositeFrame() +
flushSemantics（W2B-030/031）。引擎侧 `FrameTimingsRecorder` 用 6 态状态机记录关键时间点，
并同时保存 vsync 起点与「期望呈现时刻」（W2B-032/033）；`VsyncWaiter` 把「Animator 要出帧」
与「只想下个 vsync 醒来」分成两个意图不同的接口，前者可施加背压（W2B-034/035）。

## 3. 性能手段与公开读数

**结构类手段（有源码锚）**
- 惰性开录（W2B-007）+ 保留复用（W2B-017）+ 深度倒序 flush（W2B-010）。
- 「属性脏」不重录（W2B-013/024）；边界层实例复用（W2B-012）。
- 全屏纯色 → clear 色（W2B-062/068）；不透明自动降级 kSrc（W2B-069）。
- saveLayer 透明度窥孔（W2B-070/071/072）。
- 被裁空实体 CPU 侧跳过（W2B-079）；绘制顺序重排（W2B-080/081）。
- 一组 `Attempt*` 快路径（模糊 rrect / rsuperellipse / path source、抗锯齿圆、SDF 直线、
  色滤图集化；W2B-076），SDF 兼容性由纯函数判定（W2B-077）。
- 缓存填充限速：`kDefaultPictureAndDisplayListCacheLimitPerFrame = 3`（W2B-042）。
- 4x MSAA 统一口径（W2B-099/100）。

**公开读数（带出处与日期口径）**
| 数字 | 口径 | 锚 |
| --- | --- | --- |
| 着色器编译 20ms–200ms/次 | 源码注释给出区间（旧 Skia 路径） | W2B-044 |
| ShaderWarmUp 首装首跑 100ms–200ms | 框架公开注释 | W2B-043 |
| 每帧最多 3 条 picture/display-list 缓存 | 框架常量 | W2B-042 |
| 4x MSAA（全渲染调用） | 官方抗锯齿页 | W2B-109 |
| 构建 8ms + 渲染 8ms ≤ 16ms（60Hz，延迟导向） | 官方 best-practices 页 | W2B-112/113 |
| 监控口径：平均 + P90 + P99 + worst | 官方 metrics 页 | W2B-116 |
| 深度步长 1/262144 | 源码常量 | W2B-054 |
| 每帧深度上限 2^24 | 源码常量 `Canvas::kMaxDepth` | W2B-066 |

**没有找到的读数**：本任务范围内**未找到** Impeller 官方发布的逐版本 A/B 性能数字
（启动、jank 率、帧耗时分布）。官方页面只有机制性陈述（W2B-104~110）与可用性矩阵
（W2B-105~108）。**这是一条明确的缺口**，见第 5 节。

**平台可用性（官方口径，引用时必须同时给出平台）**
- iOS：Impeller 是唯一受支持引擎，无法切回 Skia（W2B-105）；旧渲染器已从代码库移除（W2B-052）。
- Android：默认启用，先试 Vulkan；API 29 以下或不支持 Vulkan 的设备**自动回退**旧 OpenGL 渲染器
  （W2B-051，官方文档同口径 W2B-106）。
- Web：当前仍用 Skia，官方措辞为「将来可能用 Impeller」（W2B-107）。
- macOS/Linux 桌面：自 Flutter 3.47 起默认启用，且官方预告未来会移除退出口（W2B-108）。
- 结论：**跨平台性能结论不可外推**，同一应用的性能画像因端而异。

**其它带锚的官方性能告诫**
- `saveLayer()` 会分配离屏缓冲，且大量代码在不知情的情况下触发（Opacity、部分效果的隐式
  saveLayer）（W2B-111）；官方另有一条「最小化不透明度与裁剪的使用」的通用建议同一页。
- 测性能必须用 profile 构建，debug 构建不代表发布性能（W2B-114）。
- 移动端「首轮动画卡顿」被官方直接归因并指向 Impeller（W2B-115）——即运行期编译着色器的症状。
- 能耗类指标（CPU/GPU 占用）目前只能从 trace 事件取，不在 FrameTiming 指标内（W2B-117）。

**已建档但正文未展开的锚（16 条，保持审计链完整）**
| id | 一句话观察 |
| --- | --- |
| W2B-011 | `isRepaintBoundary` 默认 false，边界是显式声明而非启发式推断 |
| W2B-020 | `willChangeHint`/`isComplexHint` 只影响缓存决策、不改变画面语义 |
| W2B-022 | 框架公开承认合成器存在「图层位图化缓存」 |
| W2B-031 | 合成提交受 `sendFramesToEngine` 开关控制，绘制正确性可与上屏分离测 |
| W2B-033 | 录制器同时保存 vsync 起点与「本帧期望呈现时刻」——掉帧判据的正确口径 |
| W2B-035 | `AwaitVSync` 契约：实现方只回调一次、不得阻塞当前线程 |
| W2B-037 | 脏区口径直接对标 `EGL_KHR_partial_update` 的平台约定 |
| W2B-045 | 着色器预热不可跨设备复用（编译结果依赖具体 GPU 与驱动） |
| W2B-048 | 着色器链路：GLSL→SPIRV→各后端语言→二进制 blob→内嵌 hex（`impellerc` 不进产物） |
| W2B-055 | 深度换算对上界做 `1-epsilon` 钳制，避免与深度缓冲最大值冲突 |
| W2B-056 | `Entity::Render` 是薄壳：兜底 coverage hint 后全部委派给 `Contents` |
| W2B-071 | 透明度折叠的实现就是栈帧上累乘 alpha（「向上累乘、向下一次性应用」） |
| W2B-081 | 不透明元素被归类为「顺序无关」，故可逆序渲染以获得遮挡剔除 |
| W2B-085 | Wang 公式本体与出处（Goldman 2003）写进头文件注释，可复核 |
| W2B-093 | `TextContents::SetForceTextColor`：位图字形（emoji）能否染色需显式开关 |
| W2B-100 | 默认管线描述符把 4x 采样与三角形条带集中在一处设定 |

## 4. 坑与反例（负面留档）

1. **重绘边界的成本侧同样明确**：标脏一个边界 = 在同一图层里重画它的全部后代（W2B-021）。
   切得太细层数爆炸、切得太粗等于没切——只能按帧测量。
2. **detach 图层的静默跳过**：`flushPaint` 遇到未 attach 的层会跳过；若不配套
   `_skippedPaintingOnLayer` 的补偿标脏就会残留旧画面（W2B-014）。
3. **Impeller 没有框架级光栅缓存**（W2B-040/041）：把 Skia 时代的
   「RepaintBoundary 会被位图缓存」经验直接套到 Impeller 上是**错的**。
4. **两趟 dispatcher 的一致性缺陷是公开的未决项**：源码注释自认两趟裁剪可能不同
   （W2B-096），且有 open issue（#182639，2026-02-19，W2B-118）。
5. **滤镜子缓冲的取整/钳制是真实 bug 源**：`canvas.cc` 的 subpass 尺寸注释引用了
   issue #144213（iOS 上 ImageFilter 滚动抖动，2024-02-27 建、已关，W2B-119）。
6. **非 MSAA 目标上调用 Flip 是逻辑错误**，代码里留了显式 VALIDATION_LOG 解释
   「非 MSAA 永远不需要交换」（W2B-082）。
7. **GLES 上 blit-to-onscreen 被禁用**：MSAA 采样数不同使 `glBlitFramebuffer` 不可用
   （W2B-075）——跨后端捷径必须逐后端标注可用性。
8. **后台读附件依赖硬件事实**：`SupportsReadFromResolve` 的可行性来自「附件可能还在
   tile memory」这一移动 GPU 特性（W2B-098），不是 API 保证。
9. **ShaderWarmUp 是旧世界的地基**：它存在的前提是「运行期编译着色器」；Impeller 用离线
   编译取消了这个前提（W2B-043/044/045 对照 W2B-047/048）。
10. **`isComplexHint`/`willChangeHint` 的官方自述是「一般没什么用」**：`addPicture` 文档明说
    会缓存的图片至少已被渲染过三次，动画图根本不会被缓存（W2B-126/127）。

## 5. 未验证项（缺什么证据）

1. **Impeller 的量化收益**：未取得任何官方 A/B 数字（jank 率、P90/P99 帧耗时、启动时间、
   APK/包体尺寸变化）。官方页面只有机制陈述与可用性矩阵。**结论：不能声称「Impeller 快多少」**。
2. **`engine/src/flutter/impeller/docs/**`（README 里链接的 faq / blending / benchmarks /
   coordinate_system / vulkan_threading 等 20 余篇）在本 commit 的仓库树中**不存在**
   （`git ls-tree` 搜索结果为空，README 的 `/docs/engine/impeller/docs/...` 链接已失效）。
   这些文档若迁到别处，本轮未定位。
3. **`docs.flutter.dev/perf/impeller.md` 的 Markdown 源**（页面上的 "View as Markdown" 链接）
   未抓取；doc 引文取自 HTML 转文本副本，行内 `<code>` 标签会在文本中留下多余空格
   （已在引文中避开或按渲染文本复原）。
4. **Google I/O 2023 演讲「Introducing Impeller, Flutter's new rendering engine」的 YouTube
   视频 ID 未独立核验**（仅由搜索摘要给出），故未立账；官方文档页有该视频标题，可作锚
   （W2B-104 所在页面正文）。
5. **`skin/` 之外的 Impeller 后端实现细节**（`renderer/backend/{metal,vulkan,gles}`）本轮
   未逐文件读；`Capabilities` 的能力项与代码实现是否一一对应未核。
6. **`flow/frame_timings.cc` 的 `GetLayerCacheCount/Bytes` 在 SLIMPELLER 下返回什么**未验证
   ——只确认了 `RasterCache` 类本身不参与编译（W2B-040），未追调用点。
7. **`scheduler/` 之外的 vsync 来源（Choreographer 等平台实现）**未读
   `shell/platform/android/**`；本报告的帧调度结论只覆盖 Dart 侧 `SchedulerBinding`
   与引擎侧 `VsyncWaiter` 抽象，不含 Android Choreographer 具体实现。
8. **文本栈内部**（`engine/src/flutter/txt/**` 的 SkParagraph 移植、字体回退链）只做了目录级
   确认（`txt/src/{txt,skia}/` 存在），未逐文件取证；本报告的文本结论集中在
   「Impeller 消费已整形 run」与字形图集这一层。
