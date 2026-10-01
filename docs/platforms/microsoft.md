# 微软：WPF / milcore（dotnet/wpf @ e89a851c5792c8777a7115cc7a797fd0c045fdd4，2026-10-01 抓取）

> 轨道 B8。**source 深度来自本地浅克隆**：`D:/KF/LSSMJ/scratch/src/wpf`（`--depth 1 --filter=blob:none --sparse`，
> sparse 集 = `src/Microsoft.DotNet.Wpf/src/{PresentationCore,PresentationFramework,WindowsBase,WpfGfx}`）。
> 注意：**MilCore 在仓库里的目录名是 `WpfGfx/`**（不是 `MilCore/`），核心在 `WpfGfx/core/`。
> doc 深度补 DComp / DWrite / DWM / WPF 架构页（learn.microsoft.com，抓取快照 `scratch/fetch/w2f/v/ms_*.txt`）。
> 账本：`w2f.jsonl` W2F-025..049（source 25 条）+ W2F-070..079（doc 10 条）。

## TL;DR（每条带锚）

1. **milcore = 唯一非托管组件，为 DirectX 紧耦合而写**："Milcore is written in unmanaged code in order to enable
   tight integration with DirectX."（W2F-070 `ms_wpf_arch.txt:61`）。托管/非托管的分界即"合成器核心"的边界。
2. **milcore 内部是"组合节点树"，每节点带渲染指令，且只通过消息协议访问**（W2F-071 `ms_wpf_arch.txt:95`）。
3. **保留模式的实质是"指令列表"**："A Visual object stores its render data as a vector graphics instruction list."
   （W2F-072）；`DrawingVisual` 的"轻"= 不带布局与事件（W2F-073）。
4. **脏区不是一个矩形也不是无限列表，而是上限 8 个矩形**（W2F-028 `dirtyregion.h:114` "MaxDirtyRegionCount = 8"），
   是否/如何合并由"允许的额外面积开销"参数决定（W2F-029），代价函数 = 并集面积减两矩形面积和再加交叠面积（W2F-030 "overhead"），
   合并策略是贪心取最小开销对、低于阈值提前收敛（W2F-031）。
5. **脏传播沿父链上行、遇父节点已带同名标记即停**（W2F-027 `node.cpp:129` 的 while 条件），
   代价与"树高 × 未标记段"成正比，而不是每次全链。
6. **呈现不是无条件动作**：合成设备每轮 `Render(pfPresentNeeded)`，是否 present 由回调决定（W2F-032）；
   HWND 目标用 1 bit 记录"存在未呈现内容"（W2F-035 `hwndtarget.h:201` "There is unpresented rendered content"）。
7. **帧提交与 VSync 解耦**：`CommitChannelAfterNextVSync()`——先渲染，等到下一个 VSync 才 commit channel（W2F-037）；
   期间由 `_needToCommitChannel` 状态位抑制重复渲染（W2F-038 `MediaContext.cs:535`）。
8. **WPF 自己记录了"渲染优先级会饿死输入"**：在 Render 优先级排队一旦超出一帧会阻塞输入处理，
   于是改用 Input 优先级的标记操作探测阻塞时长并降级（W2F-036 `MediaContext.cs:610`）。
9. **CPU 后端是"扫描算子管线"**：`This class composes scan operations to form the back-end rasterizer`
   （W2F-039），中间缓冲最多 3 个并乒乓复用；AA 是 **per-primitive（PPAA）**（W2F-040）。
10. **文本栈硬件路径把"颜色 + gamma 索引"一起下发**（W2F-033 `d3dglyphpainter.cpp:125`），
    gamma 是**逐字形 run** 的参数，并配一张按 gamma 分档的 ClearType 滤波系数表（W2F-034 `Gamma.cpp:27` "// gamma = 1.0"）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 脏区用"≤8 个矩形 + 面积开销阈值贪心合并" | W2F-028/029/030/031 | **吸收**——直接补强 LSSMJ 的 damage 设计（`lssmj-design` §4.3 只用单调 union，会在大面板退化成全窗） |
| 脏标记上行"已标即停"剪枝 | W2F-027 | **吸收**——LSSMJ 信号→标脏→汇总两级传播照此剪枝 |
| 呈现必要性用状态位显式建模 | W2F-032/035 | **吸收**——对齐判据 C3（空 damage 跳过合成） |
| 提交点与 VSync 解耦、用状态位抑制重复渲染 | W2F-037/038 | **有界吸收**——取"提交是帧内一个显式末段"，VSync 对齐留作可选档（无平台 VSync 时退化为帧末提交） |
| 显示列表不可变 + 可独立释放的资源类型 | W2F-044/045/048 | **吸收**——对应 LSSMJ "构建期累积、关闭即冻结" |
| 排版产物是显式生命周期对象（TextFormatter : IDisposable） | W2F-046 | **吸收**——字形/排版缓存对象化，避免每帧现算 |
| 扫描算子管线 + 3 个中间缓冲乒乓 | W2F-039 | **吸收（有条件）**——tiny-skia 路线同族；我们只要"缓冲数量是常数预算"这一条 |
| per-primitive AA（PPAA） | W2F-040 | **有界吸收**——AA 档位逐绘制项决定，但不必引入算子级可插拔 |
| 字形 run 粒度 + 逐 run gamma 索引 | W2F-033/034/047 | **有界吸收**——run 粒度吸收；逐 run gamma 表暂不做（青简现为按极性/主题的全局分档） |
| 渲染优先级高于输入（Render 优先级排队） | W2F-036 | **不吸收**——反面教材：会把输入饿死 |
| 逻辑树 + 可视树双树（加资源查找走逻辑树） | W2F-074 | **不吸收**（理由一行：双树同步是著名成本，我们只有一个显示列表） |
| 分区线程并行渲染（partition thread） | W2F-049 | **有界吸收**——记为未来大场景并行栅格的参考；P0–P2 单线程出帧 |
| 托管/非托管"通道"批传输（DUCE.Channel） | W2F-043/042 | 不适用于当前形态——LSSMJ 渲染器与壳同进程同语言；若未来跨进程则升级为吸收项 |
| DrawingVisual.contents 类双写者模式 | W2F-014（Apple 侧） | 不吸收——单一写者纪律 |

## 1. 架构全景（模块地图）

```text
PresentationFramework（托管：控件/布局/样式）
PresentationCore（托管：Visual / DrawingVisual / DrawingContext / CompositionTarget / MediaContext / TextFormatter）
        │  DUCE.Channel（命令批传输，W2F-042/043）
        ▼
WpfGfx = milcore（非托管核心，W2F-070）
  ├─ core/uce/        组合引擎与通道：composition.cpp（CComposition）、dirtyregion.cpp（脏区）、
  │                   cmdbatch.cpp（命令批）、clientchannel.cpp、VisualCache.h、glyphcacheslave.cpp、
  │                   hwndtarget.h（HWND 目标）、partitionthread.cpp（分区线程）、graphwalker.cpp
  ├─ core/resources/  资源与可视节点：node.cpp（CMilVisual）、renderdata.h（RenderData）、
  │                   drawinggroup/drawingimage/glyphrundrawing、geometry 资源…
  ├─ core/hw/         D3D 路径：d3dglyphpainter/d3dglyphrun/d3dglyphbank、d3ddevice、d3dswapchain…
  ├─ core/sw/         CPU 光栅：aarasterizer.h、scanpipelinerender.h、swglyphpainter、swhwndrt…
  ├─ core/glyph/      字形实现与 run 核心：baseglyphpainter、GlyphRunCore、glyphruncore.cpp
  ├─ core/common/     公共：Gamma.cpp（gamma 表）、scanop 配套、matrix、dwritefactory、engine.cpp
  └─ common/scanop/   扫描算子库：scanpipeline、soblend(_sse2)、halftone、soalphamultiply
```

## 2. 关键机制

### 2.1 保留模式与显示数据

- Visual 存的是**矢量图形指令列表**（W2F-072）；`DrawingVisual.RenderOpen()` 打开上下文，`Close()` "flushes the content"（W2F-044/045）。
- milcore 侧对应 `CMilSlaveRenderData`（可逐帧取用/回收）（W2F-048），节点是 `CMilVisual`（`core/resources/node.cpp`）。

### 2.2 脏区（本报告对 LSSMJ 最有用的一段）

- 容量：`MaxDirtyRegionCount = 8`（W2F-028）；构造时传入 `allowedDirtyRegionOverhead`（W2F-029）。
- 合并代价：`overhead = areaOfUnion - (RectArea(pR0) + RectArea(pR1) - RectArea(&intersected))`（W2F-030）。
- 选择：为每个新矩形算出与现有各矩形的开销，取最小对合并；`if (overhead_N_K < c_allowedDirtyRegionOverhead)` 时提前收敛（W2F-031）。
- 复杂度上界 = 8×8 次两两计算，常数极小。

### 2.3 脏传播

- `CMilVisual::PropagateFlags(pNode, fNeedsBoundingBoxUpdate, fDirtyForRender, fAdditionalDirtyRegion, …)`（W2F-026）。
- while 条件的实质：只在"父缺此标记"时继续上行，父已有标记即终止（W2F-027）。

### 2.4 提交与呈现

- 渲染轮：`MIL_THRX(hrRender, Render(pfPresentNeeded))`（W2F-032）——**present 是渲染结果驱动的**。
- HWND 目标：`m_fNeedsPresent` 1 bit，"There is unpresented rendered content"（W2F-035）。
- 帧提交：`CommitChannelAfterNextVSync()`（W2F-037），等待期由 `_needToCommitChannel` 抑制重复渲染（W2F-038）。
- 调度的反面教训：Render 优先级会饿死输入，需要 Input 优先级的探测器降级（W2F-036）。

### 2.5 光栅（CPU）

- 管线 = 扫描算子组合：笔刷配色生成 → alpha 掩码 → alpha 混合；中间缓冲 3 个并乒乓（W2F-039 及其页内注释）。
- AA = per-primitive（PPAA）（W2F-040）；`core/sw/aarasterizer.h` 里的 `CEdgeStore`、DDA 活动边表推进是扫描线实现。

### 2.6 文本与字形

- 托管入口 `TextFormatter`（抽象）→ `TextFormatterImp`（接 DWrite），对象需 Dispose（W2F-046）。
- 硬件绘制以**字形 run 的 painter** 为单位（W2F-047 `CD3DGlyphRunPainter::Paint(`），
  并按 run 下发 ClearType/灰度 + gamma 索引（W2F-033）。
- gamma 表按档存放（W2F-034）。
- DWrite 侧：glyph run 的定义（W2F-077）与逐字形度量集合 `DWRITE_GLYPH_METRICS`
  （advance width、左右边距、上下边距、高度、垂直基线原点）（W2F-078）。

### 2.7 平台组合（Windows 侧的"壳"）

- **DirectComposition 只吃位图**："DirectComposition works with bitmap content only; it does not support vectors or text."（W2F-075）。
- DComp 的树位置决定屏幕位置与 z 序，子视觉相对父定位、z 序按子列表顺序（W2F-076）。
- DWM 的组合 API 只设置/查询组合状态与窗口属性，不负责内容绘制（W2F-079）。

## 3. 性能手段与公开读数

**本报告的数字仅两个，且都是结构常数（不是基准读数）**：
- 脏区矩形上限 **8**（W2F-028，口径 = `dirtyregion.h` 常量）；
- 扫描管线中间缓冲 **3**（W2F-039 页内注释口径："1. Brush colors / 2. Destination pixels / 3. One extra so that we can ping-pong"）。

其余无数字。WPF 文档页不提供帧时基准；`ms_wpf_render_events` 页是"注册表排障开关"清单（抓取快照存在），
本报告未从中提取任何性能数字。**按本仓纪律，没有锚就不写数字。**

## 4. 坑与反例（负面留档）

1. **渲染优先级会把输入饿死**（W2F-036）。这是 WPF 源码注释里的自认教训：渲染排在最高优先级，
   一旦超出一帧就阻塞输入，只能靠探测器在过载时降级到 Input 优先级。
   → LSSMJ 出帧循环必须给输入留确定性让步点，且让步要有判据（不是"感觉卡了才让"）。
2. **partition thread / 多线程渲染的复杂度**：milcore 有分区线程与 PartitionManager 关停逻辑（W2F-049），
   线程句柄生命周期由管理器统一处理——这是并发渲染的必要代价。
   → LSSMJ 记为有界吸收：默认单线程，仅在滚动面板阶段引入 damage 后台栅格化。
3. **可视缓存必须带"渲染路径"进键**：`VisualCache.h:120` 的 `HWRasterRenderTarget`/`SWRasterRenderTarget`
   分支说明同一份缓存内容在不同后端下不可互换（W2F-041）。
   → 字形缓存与 tile 缓存的键必须含后端/DPI/极性。
4. **逻辑树与可视树的两棵树**（W2F-074）：模板生成的 visual 不在逻辑树里，资源查找却走逻辑树，
   这是 WPF 里最容易困惑的部分。→ 我们只保留一棵显示树。
5. **托管/非托管边界的序列化成本**：组合节点树只能通过消息协议访问（W2F-071），
   命令以 `CMilCommandBatch` 为单位传输且其生命周期需要"消费完成"信号（W2F-042）。
   → 同进程同语言时这是纯开销；我们的"显示列表"不跨边界。

## 5. 未验证项（缺什么证据）

1. **milcore 的 GPU 呈现细节未读全**：`core/hw/d3dswapchain*.cpp`、`d3ddevice.cpp` 等未逐行读；
   swap chain 的缓冲数、DWM 交互细节（Present 失败路径、DXGI 状态处理）本报告未锚。
2. **`Render(pfPresentNeeded)` 的完整决策链未读**：只锚到调用点（W2F-032），
   `CRenderTargetManager::Render` 内部如何汇总各目标的 present 需求（含 `m_fNeedsPresent` 的消费点）未读。
3. **文本 shaping 的可调参数未锚**：WPF 的 typography 特性（`Typography` 属性族）与 `TextFormattingMode`/
   `TextRenderingMode` 的具体取值语义本报告未取锚，故不对"WPF 文本宽度口径"下结论。
4. **无任何帧时/内存基准读数**：本报告不含 WPF 的性能数字（除两个结构常数）；
   若需对比青简 1ms 级读数，必须在同一台机器上用同一场景实测，不许跨来源套用。
5. **`scanpipeline.h` 的"3 个中间缓冲"是头文件注释口径**（W2F-039 页内 count 注释），
   实际分配数量在不同算子组合下可能不同——未读 `CScanPipeline::Builder::End` 的分配逻辑。
6. **DComp/DWM 的现代替代（Windows.UI.Composition / WinUI3）**：抓取到的 DComp 页明确建议
   Windows 10 应用改用 Windows.UI.Composition（`ms_dcomp_concepts.txt:57`），但该页与 WinUI3 渲染架构
   本次未抓到（`ms_visual_layer` 返回 HTTP 404，已留档 manifest），故此侧结论只到 DComp 为止。
