# Avalonia（https://github.com/AvaloniaUI/Avalonia @ `17350180c33b063f0e98abbfd19aa3cae63f5d56`, 抓取 2026-10-01）

> 上游：main 分支，包版本串 `<Version>12.2.999</Version>`（`build/SharedVersion.props:5`，W4F-081），commit 时间 2026-10-01。
> 本地：`D:/KF/LSSMJ/scratch/src/avalonia`（浅克隆 + sparse：`src/Avalonia.Base`、`src/Avalonia.Controls`、`src/Avalonia.Themes.Fluent`、`src/Avalonia.Desktop`、`src/Avalonia.X11`、`src/Avalonia.Native`、`src/Avalonia.Wayland`、`src/HarfBuzz`、`src/Windows/Avalonia.Win32`、`src/Skia/Avalonia.Skia`、`src/Avalonia.{Metal,OpenGL,Vulkan}`、`build`）。
> 文档快照：`D:/KF/LSSMJ/scratch/avalonia-docs/*.txt`（docs.avaloniaui.net 抓取，curl）。
> 证据账本：`docs/analysis/ledger/w4f.jsonl`（86 条：source 76 / doc 8 / web 2，0 拒绝；`verify` 全绿）。
> 只读分析：未编译、未运行、未提交；本报告每条主张均可指到 W4F-xxx。行号只对上述 commit 有效。

## TL;DR（每条带锚）

1. **失效模型=逐 Visual 脏集合，不是脏矩形**：`InvalidateVisual` 的唯一动作是 `PresentationSource?.Renderer.AddDirty(this)`（`Visual.cs:418`，W4F-002）；属性经 `AffectsRender<T>(...)` 订阅（`Visual.cs:444`，W4F-003）。
2. **布局失效=入队 + 帧内执行**：`_toMeasure.Enqueue(control)`（`LayoutManager.cs:72`，W4F-005）→ `MediaContext.Instance.BeginInvokeOnRender(_invokeOnRender)`（`:456`，W4F-006）；队列有防环计数与告警（`LayoutQueue.cs:63`，W4F-008），循环上限 `MaxPasses = 10`（`:20`，W4F-004）。
3. **有 damage，但默认档只有一个包围盒**：脏区为空且无强制重绘时整帧 `return`（`ServerCompositionTarget.cs:183`，W4F-017）；跟踪器选择条件是 `SupportsRegions == true && Options.UseRegionDirtyRectClipping == true`（`:65`，W4F-018），否则 `DirtyRects ??= new SingleDirtyRectTracker();`（`:75`，W4F-019）。
4. **区域裁剪是 opt-in 且官方自认有代价**：文档明说「自 Avalonia 12.1 起默认关闭」（`docs.avaloniaui.net/docs/app-development/performance`，W4F-071）；多矩形上限默认 8、合并 eager 阈值默认 1000（`ServerCompositionTarget.cs:67/72`，W4F-018/020）。
5. **多矩形合并算法=WPF CDirtyRegion2 逐行移植**（`MultiDirtyRectTracker.CDirtyRegion.cs:10`，W4F-023）；脏区在使用时压成裁剪并逐 Visual 子树剔除（`ServerCompositionTarget.cs:292/299`，W4F-021/022；`...Render.cs:86`，W4F-026）。
6. **默认有独立渲染线程**：Win32/X11 默认 `SleepLoopRenderTimer`（后台线程，`Win32Platform.cs:88`，W4F-012；`X11Platform.cs:84`，W4F-085；`SleepLoopRenderTimer.cs:45`，W4F-011），`ShouldRenderOnUIThread` 文档自述「false by default」（`Win32PlatformOptions.cs:148`，W4F-013）；macOS 同样是独立线程（W4F-082）。UI 线程只录制并提交序列化批次（`Compositor.cs:20`，W4F-031；`ServerCompositor.cs:17`，W4F-032）。
7. **显示列表=可序列化字节码**：`internal enum RenderDataOpcode : byte`（`RenderDataOpcode.cs:3`，W4F-034），渲染线程反序列化后回放（`ServerCompositionRenderData.cs:25`，W4F-035）——脏 Visual 是整元素重录（`CompositingRenderer.cs:193`，W4F-067）。
8. **Skia 后端自认支持区域**（`PlatformRenderInterface.cs:59`，W4F-036），文本一行一个 `SKTextBlob` 走 `Canvas.DrawText`（`DrawingContextImpl.cs:635`，W4F-038），GPU 资源缓存默认 ~28MB（`SkiaOptions.cs:18`，W4F-041；文档 W4F-072），默认预乘 alpha（`:50`，W4F-044）。
9. **文本栈三层**：桌面默认 HarfBuzz 整形（`AppBuilderDesktopExtensions.cs:10`，W4F-049；`HarfBuzzTextShaper.cs:64`，W4F-047）、字体索引/回退归 SkiaSharp `SKFontManager`（`FontManagerImpl.cs:16/36`，W4F-050/051）、UAX#9 等分段表在框架内（`BiDiAlgorithm.cs:13`，W4F-046）。
10. **平台壳把「不抢焦点/置顶/透明」做成一等属性**：`ShowActivated` 默认 true（`Window.cs:153`，W4F-052）→ `PlatformImpl?.Show(ShowActivated, modal)`（`:1090`，W4F-053）；`Topmost` 绑定 `SetTopmost`（`WindowBase.cs:49`，W4F-054）；`TransparencyLevelHint` 是 TopLevel 属性序列（`TopLevel.cs:63`，W4F-055）。Win32 实现：`ShowNoActivate`（`WindowImpl.cs:1284`，W4F-056）、`HWND_TOPMOST + SWP_NOACTIVATE`（`:920`，W4F-057）；X11 有 `_NET_WM_USER_TIME` 实现（`DefaultWindowMode.cs:41`，W4F-060）；macOS 经 IDL 下传（`avn.idl:775`，W4F-062）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"）

| # | 项 | 锚 | 判定 |
| --- | --- | --- | --- |
| 1 | 空脏区整帧跳过（零绘制） | W4F-017 | **吸收**——判据 C3 的现成对照实现 |
| 2 | 脏区=裁剪 ∪ 子树剔除（两层，成本=矩形求交） | W4F-021/022/026 | **吸收**——我们 damage 落地的最简形态，先于 tile 比较 |
| 3 | 脏盒帧末外扩 1px 再像素化 | W4F-024 | **吸收**——AA 出血的对齐细节，易漏 |
| 4 | 多矩形合并（WPF CDirtyRegion2：overhead 矩阵 + 提前退出） | W4F-023/020 | 有界吸收——矩形上限与阈值可调并记账；候选窗量级单包围盒已够 |
| 5 | 区域（SkiaRegion）做裁剪 | W4F-025/036 | 有界吸收——CPU 侧退化为矩形列表，GPU 档再上区域 |
| 6 | 失效=逐元素脏集合 + 帧内批量执行 | W4F-002/005/006 | **吸收**——与设计 §6「信号标脏→帧末统一重建」同型 |
| 7 | 空闲停表（无人要下一帧就关定时器） | W4F-009/030 | **吸收**——空闲零 CPU 的显式契约 |
| 8 | 渲染线程与 UI 线程用可序列化批次解耦 | W4F-031/032/034 | 有界吸收——我们 P1 单线程出帧，大面板档再引入，显示列表先定型为可序列化 |
| 9 | 文本一行一个 SKTextBlob 单调用提交 | W4F-038 | **吸收**——我们的文本 run 亦应按行成批 |
| 10 | 图元缓存键=一切影响产物的输入（TextOptions 进键） | W4F-039 | **吸收**——对我们的字形缓存键（4×4 子像素 + opsz）是同一原则 |
| 11 | Skia 对象池（ConcurrentBag 复用） | W4F-040 | **吸收**——每类十几行的零分配热点路径 |
| 12 | 「性能开关不得悄悄改画质」（stencil 默认禁用，issue #21760） | W4F-043 | **吸收**——与 tiny-skia 静默降级反面教材互证 |
| 13 | 每个 BoxShadow 一趟 pass（官方口径） | W4F-074 | **吸收**——支持我们「尺寸不变即复用贴图」的阴影优化 |
| 14 | 缓存面位图（BitmapCache 整子树栅格化复用） | W4F-073 | 有界吸收——我们的等价物是 tile/元素缓存，注意与 ClearType 文本模式的交互 |
| 15 | 平台壳一等属性（ShowActivated/Topmost/TransparencyLevelHint） | W4F-052..055 | **吸收（作为契约范本）**——我们的壳契约应给同类三件套，而不是让渲染器关心 |
| 16 | 默认档单包围盒脏区 + 无像素级 diff | W4F-019/076 | **不吸收**——候选窗够用，大面板必抖（我们设计 §4.3 已判）；记录为下限而非目标 |
| 17 | 全平台 Skia 自绘（含浏览器/移动） | W4F-078 | 不吸收——我们定位是位图+薄壳（设置程序另议），不引入 toolkit |
| 18 | 材质级透明（Acrylic/Mica）绑特定合成后端 | W4F-059/064 | 不吸收（候选窗级）——候选窗要的是真透明，不是系统材质 |

## 1. 架构全景（模块地图）

保留模式视觉树 + 进程内「双半」合成引擎，Skia 单一光栅后端：

- `src/Avalonia.Base/`
  - `Visual.cs`：失效入口（W4F-002/003）；`Layout/`：`LayoutManager.cs` + `LayoutQueue.cs`（W4F-004/005/008）、`Layoutable` 的 Measure/Arrange。
  - `Media/MediaContext.cs`：UI 线程的帧调度器（渲染回合、动画兜底定时器、输入饥饿让路，W4F-014/016）。
  - `Rendering/RenderLoop.cs`：定时器驱动的最小循环（W4F-009/010）；`Rendering/Composition/`：**UI 线程半**（`Compositor.cs`、`CompositingRenderer.cs`、`Drawing/RenderData*` 字节码）与**渲染线程半**（`Server/`：`ServerCompositor.cs`、`ServerCompositionTarget.cs`、`Server/DirtyRects/`）。
  - `Media/TextFormatting/`：TextFormatter/TextLayout + `Unicode/`（UAX#9/#14 等全套分段表，W4F-046）。
- `src/Skia/Avalonia.Skia/`：`PlatformRenderInterface.cs`（区域支持、预乘，W4F-036/044）、`DrawingContextImpl.cs`（Skia 绘制上下文 + lease，W4F-037/038）、`GlyphRunImpl.cs`（TextBlob 缓存，W4F-039）、`SKCacheBase.cs` 等对象池（W4F-040）、`Gpu/{OpenGl,Vulkan,Metal}/`（GPU 会话）。
- `src/HarfBuzz/Avalonia.HarfBuzz/`：`ITextShaperImpl` 的 HarfBuzzSharp 实现（W4F-047/048）；`src/Avalonia.Desktop/`：`UsePlatformDetect()` 无条件加载 HarfBuzz（W4F-049）。
- `src/Windows/Avalonia.Win32/`：`Win32Platform.cs`（定时器/合成选择，W4F-012）、`WindowImpl*.cs`（窗口/透明/置顶，W4F-056..059）、`DirectX/`（DXGI 低延迟档，W4F-045/065）、`DComposition/`、`WinRT/Composition/`。
- `src/Avalonia.X11/`：`X11Platform.cs`、`X11Window*.cs`、`X11WindowModes/`（activate 语义，W4F-060）、`TransparencyHelper.cs`。
- `src/Avalonia.Native/`：macOS 壳（C# ↔ `libAvaloniaNative` 经 `avn.idl`，W4F-061/062）。

## 2. 关键机制

### 2.1 失效与脏传播（问题①）

- 失效是**视觉级集合**：`AddDirty` 把 Visual 加进 `HashSet<Visual> _dirty`（W4F-002/008）；渲染回合内逐个重跑 `visual.Render(_recorder)` 并替换其 DrawList（W4F-010/067）。渲染期再次失效=抛异常（W4F-066），「帧内不可变」是硬约束。
- 布局另有一套队列：measure/arrange 各自 `LayoutQueue`（去重 + 防环告警，W4F-005/008），单轮上限 10 次（W4F-004）；执行挂在 `MediaContext` 的渲染前回调里（W4F-006），另有 153 次回调风暴硬闸（W4F-014）。测量顺序靠向上递归取「先根后叶」，注释自述是 benchmark 选择（W4F-007）。
- 视觉更新阶段的脏传播：`visitChildren = _isDirtyForRenderInSubgraph || _needsBoundingBoxUpdate`（W4F-070），脏节点包围盒经变换后汇入脏区（W4F-028）；不可见子树把 bounds 置 null 整支跳过（W4F-029）。

### 2.2 渲染循环与线程（问题⑥）

- 循环极小：`bool Render()` 聚合下一 tick 需求，没人要就停表（W4F-009/010）；空闲不空转是显式目标（W4F-030）。
- Win32/X11 默认 `SleepLoopRenderTimer`（后台线程，W4F-011/012/013/085）；macOS 默认 `ThreadProxyRenderTimer`（内部再起专用 `RenderTimerLoop` 线程转发原生定时器 tick，W4F-082）。UI 线程只做视觉树、录制与提交（W4F-031）；渲染线程反序列化批次并驱动各 target `Render()`（W4F-032）。
- 多窗口并行渲染（每窗一个 Compositor）在注释里仍是「planned」（W4F-033），当前共享一个 Compositor。

### 2.3 脏区/damage（问题③，判定重点）

- 三层结构：**更新期收集 → 帧末定形 → 渲染期使用**。收集=脏节点包围盒的世界空间并集（W4F-028）；定形=`FinalizeFrame` 外扩 1px、像素化、裁剪（W4F-024）；使用=压裁剪 + 逐 Visual 剔除（W4F-021/022/026）。
- 跟踪器三档：`SingleDirtyRectTracker`（默认兜底，单包围盒，W4F-019）、`RegionDirtyRectTracker`（直接用平台区域）、`MultiDirtyRectTracker`（≤8 矩形 + 合并，W4F-018/020/023）。**后两者需要 `UseRegionDirtyRectClipping = true` 且平台支持区域**（W4F-018）；官方明说 12.1 起默认关闭（W4F-071）。
- 单矩形档下渲染期不做逐视觉剔除（`dirtyRects = null`，W4F-027/069）——默认档的「剔除」实际由更新期的脏集完成。
- 空脏区且无请求 → 整帧不渲染（W4F-017）；`SceneInvalidated` 对外的粒度是整窗矩形（W4F-068）。**结论：Avalonia 有 damage，但默认粒度=单包围盒/元素级，没有像素或 tile 级 diff**（文档「diffs the new scene」指的是场景/视觉级，W4F-076）。

### 2.4 绘制列表与提交（GPU/线程契约）

录制产物是字节码操作流（W4F-034）+ 资源表（字形 run/位图等引用登记，W4F-035），跨线程序列化后在渲染线程回放；资源生命周期挂在流上。对我们是「显示列表若要过线程，先定义成可序列化」的先例（W4F-031/032/034/035）。

## 3. Skia 后端（问题②）

- 无自研图集/批处理：绘制调用直接下发 Skia canvas，成批交给 Skia 内部（GPU 档由 Skia 的 GRContext 管理纹理与合批）；Avalonia 侧只做三类事：上下文 lease（`ISkiaSharpApiLeaseFeature`，W4F-037 的 `Monitor.Enter(_grContext)` 串行化）、对象池（W4F-040）、语义缓存（TextBlob 按 TextOptions 两级缓存，W4F-039）。
- 文本绘制=一行一个 `SKTextBlob` 单次 `Canvas.DrawText`（W4F-038）；字形光栅与（GPU 下的）图集在 Skia 内部，Avalonia 不管理字形位图。
- 预算旋钮：GPU 资源缓存上限默认 ~28MB（`SkiaOptions.cs:18`，W4F-041；与文档 W4F-072 一致），超限纹理每帧重传；`UseOpacitySaveLayer` 默认 false（W4F-042）；stencil 默认禁用为保 AA 质量（W4F-043）。

## 4. 文本栈（问题④）

- 整形：桌面默认 HarfBuzzSharp（`UsePlatformDetect` 无条件加载，W4F-049；`font.Shape(buffer, features)`，W4F-047），以 `ITextShaperImpl` 单例注册、可替换（W4F-048）。
- 字体与回退：`SKFontManager`（平台字体索引）+ 字符匹配 `TryMatchCharacter(codepoint, ..., culture, ...)`（W4F-050/051）——回退链责任在 Skia/平台，不在框架。
- 段落/行/字素/双向分段：框架自带 UAX#9 等实现与 Unicode 表（W4F-046）；即「分段规则在框架层、整形在 HarfBuzz、光栅在 Skia」的三层分工。

## 5. 平台壳（问题⑤）

- 契约：不抢焦点（`ShowActivated` → `PlatformImpl.Show(activate)`，W4F-052/053）、置顶（`Topmost` → `SetTopmost`，W4F-054）、透明（`TransparencyLevelHint` 回退序列，W4F-055）全是一等属性；平台层各自映射。
- Win32：`ShowNoActivate` 且不抢前后台焦点（W4F-056）；`HWND_TOPMOST + SWP_NOACTIVATE`（W4F-057）；透明两条腿（DWM blur-behind / 不用重定向位图），Acrylic/Mica 的可用性绑 WinUI/DComp 合成后端（W4F-058/059/086）；默认合成链 = WinUI → DComp → 重定向表面（W4F-064），默认渲染链 = AngleEgl → Software（W4F-063）；DXGI vblank 驱动是 opt-in 低延迟档（W4F-045/065）。
- X11：`ShowActivated=false` 于 map 前把 `_NET_WM_USER_TIME` 置 0（W4F-060）；透明请求按级别序列探测平台能力、无合成器时回退 None（`TransparencyHelper.cs:40`，W4F-084）。macOS：Compositor 在进程内创建（`AvaloniaNativePlatform.cs:188`，W4F-061），`activate` 经 IDL 下传原生层（W4F-062，原生实现不在本仓）。

## 6. 性能手段与公开读数（数字必须带锚）

- 官方性能页（docs.avaloniaui.net/docs/app-development/performance）：区域脏区裁剪默认关闭（12.1 起，W4F-071）；Skia GPU 缓存约 28MB（W4F-072）；BitmapCache 复用（W4F-073）；每个 BoxShadow 一趟 pass（W4F-074）。
- 源码侧常量：单轮布局上限 10（W4F-004）、渲染前回调上限 153（W4F-014）、动画兜底 16ms（W4F-015）、多矩形上限 8 / 合并阈值 1000（W4F-018/020）、GPU 缓存 1024×600×4×12 字节（W4F-041）。
- 调试面：`RendererDebugOverlays` 内置 FPS/脏矩形/布局耗时图/渲染耗时图四种叠加（W4F-083）；渲染返回 `(VisitedVisuals, RenderedVisuals)` 计数（W4F-022）——性能可观测性是一等公民。
- 注意：以上无「帧率/耗时」实测数字（本任务未跑基准，也没找到官方发布的带机器口径读数）——**性能数字缺口见 §8**。

## 7. 坑与反例

1. 默认档区域裁剪关闭 + 单包围盒：一次大范围变化（整行候选窗更新、滚动）会把脏区放大成整窗；这是官方为帧率做的取舍（W4F-018/019/071）。
2. 单矩形档下渲染期不再逐视觉剔除（W4F-027）——想省 GPU 访问就得上区域档，而区域档「adds extra CPU time」（W4F-071），是二选一而非白拿。
3. 脏区依赖「更新阶段拿到世界变换」；变换一变，旧变换下的包围盒无从得知，故算法在效果/内容类变化时退回整节点包围盒（`Update.cs:74-90` 的 disable 计数设计，W4F-028/070）——**不能指望它精确到内容像素**。
4. 渲染期失效直接抛异常（W4F-066）：把「帧中改树」当 bug 而非容错。
5. 动画兜底定时器固定 16ms 会在有订阅时持续唤醒（W4F-015）。
6. GPU 资源缓存超限 → 每帧重传（W4F-041/072），大图/大图集场景必须显式上调并验证。

## 8. 未验证项（写明缺什么证据）

1. **帧率/延迟实测**：本任务只读，不含任何基准运行；文档页也无带机器口径的数字。
2. **macOS 行为**：`activate` 的原生实现（不抢焦点是否真生效、以及是否真需要「进程外渲染」）在 `libAvaloniaNative` 二进制里，本仓不可验（W4F-062）。
3. **NativeAOT 产物体积**（青简评估的「几十 MB」）：本仓无部署产物证据，必须实测。
4. **X11 `_NET_WM_USER_TIME` 在各 WM 下的实际效果**：机制在源码（W4F-060），但 WM 是否遵守未验；issue #17186 已 closed、PR #20958 已合并（W4F-079/080），仍建议按目标 WM 复测。
5. **tile/像素级 damage**：全库未见（本报告结论之一），但未穷举 wayland/browser 后端。
6. **移动端键盘扩展**：本仓无 iOS 键盘扩展支持证据（属 Avalonia.iOS 之外的产品化问题）——青简评估的「手机键盘进不去」未做源码级核实。

## 9. 对青简 rendering.md 六条评估的逐条复核（`scratch/src/qingjian/docs/design/rendering.md:29–31`）

原文（逐字）：「**Avalonia（C#，Skia 自绘）**：`ShowActivated=false` + `Topmost` + `TransparencyLevelHint` 是一等属性，Windows / macOS 能做不抢焦点的置顶窗（讨论 #15805）；Linux 上 `ShowActivated` 失效（#17186）。是排除 WebView 之后最顺的现成框架，但要常驻 .NET 进程（NativeAOT 后仍几十 MB）、macOS 要改成进程外渲染、手机键盘进不去。」

| # | 青简主张 | 复核结论（@17350180 / 2026-10-01） | 锚 |
| --- | --- | --- | --- |
| ① | 三属性「是一等属性」 | **成立**。三者都在框架属性系统里（`Window.ShowActivated`、`WindowBase.Topmost`、`TopLevel.TransparencyLevelHint`），并有到平台 impl 的直接绑定/传参。 | W4F-052/053/054/055 |
| ② | Windows/macOS 能做不抢焦点的置顶窗 | **Windows 成立（源码级）**：`SW_SHOWNOACTIVATE` + 不调用 `SetFocus/SetForegroundWindow`；置顶用 `SWP_NOACTIVATE`。**macOS 半成立**：activate 经 IDL 下传，原生实现不在本仓，未验。 | W4F-056/057；W4F-062 |
| ③ | Linux 上 `ShowActivated` 失效（#17186） | **已过时**。本 commit 的 X11 壳在 `DefaultWindowMode.Show` 里实现 `_NET_WM_USER_TIME=0`（即 activate=false 的 EWMH 表达）；issue #17186 已 closed（2026-03-25）、实现 PR #20958 已合并（2026-03-20）。qingjian 文档（2026-09-13）引用时该状态已变——应按目标 WM 复测取代此句。 | W4F-060/079/080 |
| ④ | 「排除 WebView 之后最顺的现成框架」 | **可核部分成立，但这是价值判断**：源码层面确实具备「不激活/置顶/透明」的一等契约与六端后端（W4F-063/064/078）；「最顺」取决于项目目标，本报告只提供事实锚，不作总评。 | W4F-063/064/078 |
| ⑤ | 要常驻 .NET 进程（NativeAOT 后仍几十 MB） | **本仓无法核实**（需部署实测）。可核的相邻事实：Avalonia 是 .NET 库，桌面入口 `UsePlatformDetect` 无条件加载 HarfBuzz/Skia 等子系统；体积/常驻内存无源码级数字。 | 未验证（§8.3） |
| ⑥ | macOS 要改成进程外渲染 | **源码不支持该表述**。macOS 壳在本进程创建 Compositor 并绑定平台图形；全库代码搜索 "out of process" 命中 0。若此句指青简自身架构约束（IME 进程模型），应改写归属，否则属误述。 | W4F-061 |
| 附 | 手机键盘进不去 | **未验证**（本仓无 iOS 键盘扩展支持/不支持的源码证据；属部署与平台规则问题）。 | §8.6 |

**一句话复核**：六条里 ① 成立、② 在 Windows 面成立（macOS 待真机）、③ 已过时（X11 已实现）、④ 属价值判断（事实部分成立）、⑤ 无本仓证据、⑥ 本仓源码不支持该说法；两条「代价」应当拆开各自记账，不能整体照抄进设计文档。
