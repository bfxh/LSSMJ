# 平台合成器与闭源系统 UI 框架（DWM / SurfaceFlinger / WindowServer·CoreAnimation / SwiftUI / WinUI3，抓取 2026-10-01）

> 批次 w3g（第三轮补缺）。补 `microsoft.md` §5「未验证」第 6 条（Windows.UI.Composition / WinUI3 未抓）与本报告中 Android/macOS/SwiftUI 侧新缺口。
> **来源与快照**（全部落在 `D:/KF/LSSMJ/scratch/fetch/w3g/v/`，清单 `v/manifest.jsonl`，抓取脚本 `fetch_w3g.py`）：
> - Microsoft Learn 文档页（win32/dwm、directcomp、uwp/composition、windows/apps）——URL 级锚，2026-10-01 抓取；DWM 两页另取**文档源码 md**
>   `MicrosoftDocs/win32`@`e103fa4e8810bd8d42c4777e17081e24dbe62dbd`；WinUI 侧 md `MicrosoftDocs/windows-dev-docs`@`aabd22a9113af71e56bd6f5af8fde06df68162e0`。
> - **Android（source 深度）**：`LineageOS/android_frameworks_native`（AOSP `frameworks/native` 镜像）@ `9568531f9be7ae65f260699811f32ec4e461de6e`（2026-08-04）。
>   **`source.android.com` 与 `developer.android.com` 本机均不可达**（curl 超时 / 000），`aosp-mirror` 组织**没有** `platform_frameworks_native` 仓库（已列全量仓库名核实），
>   故按任务授权改用 LineageOS 镜像 raw，**来源已在每条台账注明**。
> - Apple：developer.apple.com（WWDC/Tech Talk 文字稿、归档 Core Animation Programming Guide）——doc 深度；**WindowServer 内部无任何官方文档**，macOS 侧只能到「render server 进程」（见 §7）。
> - 账本：`docs/analysis/ledger/w3g.jsonl`（81 条：doc 56 / source 25，`verify` rejected=0）。**WinUI3 与 SwiftUI 均为闭源**，本报告对这两家只到「文档/WWDC 摘要」深度，不外推到实现。

## TL;DR（每条带锚）

1. **Windows 合成器有显式的「合成帧节拍」**：`Compositor Frame per cycle`，且该周期**不一定与 vblank 对齐**
   （W3G-001）。帧率是合成器给应用的抽象：DRR boost 靠「只把每 2/3/N 个 vblank 暴露给应用」虚拟化实现（W3G-003）。
2. **DXGI swapchain 的节拍固定取主显示器**，与窗口所在显示器无关——这是 compositor clock API 存在的理由（W3G-002）；
   DRR **只能升频不能降频**（插人造 vblank 不比忽略真实 vblank 便宜，W3G-004）。
3. **桌面组合=重定向**：窗口绘制被重定向到显存离屏表面，再由 DWM 合成桌面；DWM 是独立**服务进程**（W3G-006/007）。
   直接画主显示表面会触发 **DWM 自动关闭组合**（W3G-008/024）。
4. **提交路径有两条且官方给了取舍**：blt（每次 Present 拷进重定向表面）vs **flip（后备缓冲直接共享给 DWM，零拷贝合成）**
   （W3G-019/020）；flip 下缓冲数 2–16，太多会因等 DWM 释放上一缓冲而付罚金（W3G-021）。
5. **DWM 自带双缓冲并以单帧呈现**（应用不必自备，W3G-022）；被遮挡窗口**不再收 WM_PAINT**——
   「不产生绘制请求」是比「少画」更彻底的空 damage 处理（W3G-023）。
6. **「应用只出位图」的官方边界语料已凑齐三句**：DComp「works with bitmap content only」（W2F-075）、
   SpriteVisual「画刷渲染像素」（W3G-032）、Core Animation「这些层现在只是图像」（W3G-058）。
   Apple 侧更进一步：自定义绘制拿到的就是**纹理背书**的上下文（W3G-059）。
7. **Android 的每帧输入是「快照」**：FrontEnd 每帧向合成引擎交一份不可变快照（W3G-038），
   由 LayerSnapshotBuilder 把层树拉平成 z 序列表（W3G-040）；Transaction 是对若干层的**原子**变更集（W3G-039）。
8. **Android 合成是混合的且逐帧统计**：`CompositionCoverage::{Hwc, Gpu, GpuReuse}` 三态（W3G-044/045），
   GPU 合成可 offload 到后台执行器（W3G-049）；present 是一次显式调用收束（W3G-043）。
9. **Android 有真 damage 机制**：`eSurfaceDamageRegionChanged` 逐层声明 + `small dirty` 标记
   （门控 `enable_small_area_detection`，W3G-046/047）+ `forceFullDamage` 全窗回退开关（W3G-048）。
   **Windows 公开文档层没有对应 API**（damage 决定权在 DWM/应用重绘），这是各家最大差异。
10. **Apple 渲染循环：三段（应用 / render server / 显示）× 五相（event→commit→render prepare→render execute→display）**，
    帧经两个帧周期才上屏（双缓冲），迟帧时切三缓冲多给一整帧时长（W3G-054/055/056）；
    官方明确「render server 的活是替你干的，责任不能外推」（W3G-071）。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 帧=对层树的一次**不可变快照**，合成引擎只消费快照 | W3G-038/040（SF）、W2F-048（milcore） | **吸收**——LSSMJ 显示列表按此纪律做（帧内冻结、跨帧可比较） |
| 状态变更走**原子事务**（合并结合律、顺序敏感） | W3G-039 | **吸收**——对标 §6 失效模型：一帧内的标脏应聚成一次可原子应用的变更集 |
| **small dirty / damage 区域作为一等信号**（可被刷新率/功耗消费者读取） | W3G-046/047 | **吸收**——LSSMJ 的 damage 除驱动光栅外，应输出可记账信号（判据 C3 的可观测面） |
| **全窗 damage 是开关式回退**（forceFullDamage）而非默认 | W3G-048 | **吸收**——与 ADR G3「矩形数封顶超限回退全窗」同构，且证明该回退值得做成显式档位 |
| 合成结果**整帧复用**（GpuReuse）作为 damage 的极端档 | W3G-044 | **有界吸收**——等价于「空 damage 跳过合成」，但需自证复用判定（tile 描述符比较）不劣化 |
| 合成/提交可**分派到后台执行器**，但提交点仍单点 | W3G-043/049 | **吸收**——正是 lssmj-design §3 的「damage 后台栅格化 + 帧提交单点」 |
| **一次合成未收敛就排下一帧**（needsAnotherUpdate → commit） | W3G-050 | **吸收**——damage 未清空时自驱动出帧，不依赖定时器唤醒 |
| 平台合成器**自己双缓冲**、应用不必自备 | W3G-022（DWM） | **吸收**——Windows 壳不做双缓冲层；tearing-free 交平台 |
| 帧时账本含「在途帧数 / 迟帧 / 丢帧」计数器 | W3G-011/012（DWM_TIMING_INFO） | **吸收**——LSSMJ 帧账本字段清单直接照此列 |
| 缓冲数是**2 或 3 的可切档**（迟帧追赶时 3） | W3G-056/062 | **有界吸收**——作为后备档，不进默认路径（默认仍是 2/单缓冲位图） |
| blur 的成本依赖**下方内容**的更新，缓存键须含下层 damage | W3G-025 | **吸收（修正既有设计）**——§4.4「尺寸不变即复用」需补一条：blur/backdrop 类效果的失效输入含其覆盖区**其下**的 damage |
| 硬件 overlay 与帧率门槛耦合（OverlayMinFPS） | W3G-013/014 | **有界吸收**——GPU 档想吃 overlay 快路径须先保稳态帧率；记为 P3 设计约束 |
| 应用层「保留模式 UI + 合成层下沉」（XAML on Visual layer） | W3G-028/029 | **吸收（对定位）**——证实「框架层 / 合成层」两分法；LSSMJ 站合成层一侧，与 WinUI 可并存（W3G-034） |
| SwiftUI 的依赖图「只更新需要新 body 的视图」 | W3G-067 | **语义吸收、实现不可照搬**——闭源属性图无公开规范，且其粒度是「视图」而非「绘制项」 |
| presentation tree / render tree 与应用树分离 | W3G-065/066 | **不吸收**——LSSMJ 不做在飞动画的双树；动画属上层（若要，按单一写者纪律自建层） |
| 绕过合成直接画主显示面 | W3G-008/024 | **不吸收**——全局降级代价（整个桌面失去组合），平台侧已明令禁止 |
| 依赖「创建序」做 z 序 | W3G-042 | **不吸收**——Android 官方劝阻；LSSMJ 的层号/行序一律显式 |
| XAML↔DX 共享表面（表面所有权在框架） | W3G-036 | 不适用于当前形态——LSSMJ 壳自己不提供控件框架；若未来嵌入宿主框架，升级为「有界吸收」 |

## 1. 架构全景（三家合成链拓扑）

```text
Windows
  应用（WPF/WinUI/UWP/D3D 应用）
    ├─ blt 路径：Present → 拷入「重定向表面」            ┐
    └─ flip 路径：后备缓冲直接共享给 DWM（无拷贝）        │ W3G-019
         │                                              │
  dcomp.dll（用户态 API 库，COM 单入口）                   │ W3G-017
         │  win32k.sys 对象库（核态编组）                  │
         ▼
  dwmcore.dll（合成引擎，宿主 dwm.exe；会话级单例，       │ W3G-017/018
              同时管所有应用的 DComp 树 + 桌面 DWM 树）
         ▼
  显示硬件：DWM swapchain = 硬件 overlay 的 plane 0       W3G-014/015
  节拍：compositor clock（DCompositionWaitForCompositorClock，1 帧/周期；DRR 可 boost）W3G-001/003

Android（LineageOS 镜像 @9568531，services/surfaceflinger）
  应用/SystemServer（hwui 或自绘）
    └─ Transaction（原子变更集）→ Layer（BufferQueue 供缓冲） W3G-039
         ▼
  FrontEnd：LayerLifecycleManager + LayerSnapshotBuilder
    → 每帧一份拉平 z 序的 LayerSnapshot 列表                    W3G-038/040
         ▼
  CompositionEngine + Scheduler（pacesetter=最快已上电显示器）    W3G-051
    ├─ 设备合成（HWC/overlay）：state.usesDeviceComposition      W3G-044
    ├─ 客户端 GPU 合成：usesClientComposition → RenderEngine      W3G-045
    └─ GPU 合成可 offload 到 BackgroundExecutor                   W3G-049
    → present(refreshArgs) 一次收束；presentFence 回灌 Scheduler  W3G-043/080

Apple（macOS/iOS 公开抽象）
  应用进程：event 相位（触达/回调）→ commit 相位（布局/绘制/打包层树）  W3G-054/057
         ▼  整棵变更后的层树跨进程提交（递归打包，树越深越慢）        W3G-057/060
  render server 进程（macOS 侧对应 WindowServer；**该名未见于本次文档**）W3G-055/061
         ▼  render prepare（层树→线性 GPU 管线）→ render execute（GPU 合成）
  显示：必须赶上下一个 VSYNC；帧经 2 个帧周期上屏（迟帧退 3 缓冲）      W3G-054/056
```

**共同点**：三家都是「应用产出内容 → 独立合成进程/引擎持有并合成 → 显示硬件扫描出」。差异在**内容货币**
（Windows/Apple=位图+树语义，Android=Buffer 句柄+层属性）与**damage 归属**（Android 有显式 API，Windows/Apple 无公开 API）。

## 2. 关键机制（按任务三问法）

### 2.1 谁持有帧、如何提交、帧率与 vsync

- **Windows：DWM 持有桌面帧**。应用把内容交进 DWM 的重定向表面（blt）或共享后备缓冲（flip，W3G-019）；
  DWM 以「每周期 1 个 Compositor Frame」的节拍合成（W3G-001），**不保证与 vblank 对齐**；
  应用可用 `DCompositionWaitForCompositorClock` 等合成器心跳（W3G-005），
  帧的**驻留刷新数**曾是应用可请求的量（DwmSetDxFrameDuration，Windows 8.1 起恒返回 E_NOTIMPL，W3G-009/010/077）。
- **Android：SF 的 CompositionEngine 持有帧**，输入是每帧快照（W3G-038）；提交= `present(refreshArgs)`（W3G-043）；
  节拍来自 Scheduler 的 pacesetter 与硬件 VSYNC（无消费者等待时关掉硬件 VSYNC，W3G-052）。
- **Apple：render server 持有帧**；应用 commit=整棵层树递归打包发送（W3G-057/060）；
  三段各自有 VSYNC 截止（W3G-054），双缓冲为默认、三缓冲为迟帧回退（W3G-056/062）。

### 2.2 damage / 区域机制（有则给锚）

- **Android（唯一有公开 API 的一家）**：逐层 surface damage region（`eSurfaceDamageRegionChanged`，W3G-047）
  + `small dirty` 标记（门控 `enable_small_area_detection`，W3G-046）+ `forceFullDamage` 全窗回退（W3G-048）
  + 合成层三态覆盖（Hwc/Gpu/GpuReuse，W3G-044/045）。
- **Windows**：公开面**没有**应用级 damage API。能间接等价的是「重定向表面+遮挡不重绘」语义（W3G-023）；
  DWM 侧只暴露**帧时序计数器**（cFramesLate/Dropped/Missed、cFramesOutstanding，W3G-011/012）与
  overlay 指派门槛（OverlayMinFPS，W3G-013）。
- **Apple**：公开面是**请求式脏**（setNeedsLayout/setNeedsDisplay 被系统**合并**后按序执行，W3G-081 逐字）；
  合成侧无区域 API，提交粒度是整树（W3G-057）。

### 2.3 「应用只出位图/纹理」的官方边界（语料库）

| 家 | 逐字句 | 锚 |
| --- | --- | --- |
| Microsoft（DComp） | "DirectComposition works with bitmap content only; it does not support vectors or text." | W2F-075 |
| Microsoft（WinRT 合成） | SpriteVisual "Has the ability to associate a brush so that the Visual can render pixels including images, effects, or a solid color." | W3G-032 |
| Microsoft（XAML/DX 互操作） | "Shared surfaces are sized regions of the display, defined by XAML, that you can use DirectX to draw into indirectly" | W3G-036 |
| Apple（Core Animation） | "As far as Core Animation is concerned, these layers are now just images." | W3G-058 |
| Apple（Core Animation） | "every custom drawing layer will receive a texture-backed Core Graphics context" | W3G-059 |
| Apple（Core Animation 指南） | "It is an infrastructure for compositing and manipulating"（不是绘图系统） | W3G-063 |
| Android | 合成输入是「buffer 的合成方式快照」（层=缓冲+属性），无矢量语义 | W3G-038/040 |

→ 四家（含 Android 的 Buffer 模型）**没有一家**把矢量/文本交给合成器；LSSMJ 的位图契约不是权宜，而是**平台惯例**。

## 3. 性能手段与公开读数

**数字类（带口径）**：

- Windows 合成器：**1 Compositor Frame/周期**（结构口径，W3G-001）；DRR 两档「60Hz / 120Hz(2×)」
  （文档举例值，W3G-075）；flip 缓冲数 **2–16**（建议区间，W3G-021）。
- Apple：iPhone/iPad **60fps=16.67ms/帧**、iPad Pro **120fps=8.33ms/帧**（Tech Talk 10855 举例口径，W3G-078）；
  双缓冲=**2 个帧周期**上屏（W3G-056）、三缓冲=多给 **1 帧时长**（W3G-079）；Instruments 暴露 buffer count **默认 2、追赶时 3**（W3G-062）。
- Android：**无公开帧时读数**（本批未取到任何 ms/mem 数字）；只取到结构事实（快照/三态覆盖/pacesetter 策略）。

**手段（非数字）**：DWM 遮挡不重绘（W3G-023）、flip 零拷贝合成（W3G-019）、
DComp 独立合成线程不饿死 UI 线程（W3G-016）、Android 后台 offload 合成（W3G-049）、
Apple 布局/绘制请求**合并**执行（W3G-081）、Android 无等待者即关硬件 VSYNC（W3G-052）。

## 4. 坑与反例（负面留档）

1. **直接画主显示表面 ⇒ 全局关闭组合**（Windows，W3G-008/024）。任何「绕过合成更快」的直觉在本平台都是错的：
   代价由整个桌面承担。
2. **提交晚 ⇒ 整帧被跳过**：「If the frame is presented late to the DWM or the DWM is late in composing,
   a frame could be displayed for fewer than the number of refreshes requested or even skipped completely.」（W3G-010）。
   → LSSMJ 的重绘必须**幂等**：丢掉的帧不能留下不一致状态。
3. **窗口内容与 DWM 之间曾靠拷贝**：blt 模型每次 Present 都要拷入重定向表面（W3G-019），
   且撕裂到 2018-04 更新才修好、代价是额外工作（W3G-076）。
   → 提交路径的选型是长期成本项，不是一次性优化。
4. **blur-behind 的隐性依赖**：模糊区自身不更新、**下方**更新也要重算，且窗移/缩放/转场全加价（W3G-025）。
   → §4.4「尺寸不变即复用」的缓存键漏了「覆盖区下方的 damage」就会算错。
5. **深树提交成本**：「deep view hierarchies will take longer to be packaged up.」（W3G-060）。
   → 保持显示列表扁平是硬约束，不是风格偏好。
6. **把合成侧工作排除在自己的帧预算外**：Apple 明确「it does work on your app's behalf… it's on you」（W3G-071）。
   → LSSMJ 的帧预算必须含壳侧贴图/合成时间。
7. **渲染优先级饿死输入**（WPF，W2F-036）与 **DComp 用独立线程规避**（W3G-016）是同一问题的两种答法；
   本项目取「独立线程/独立阶段」，不取「优先级抢占」。
8. **依赖创建序做 z 序**（Android 官方劝阻，W3G-042）→ 顺序语义必须显式可判定。

## 5. 对 LSSMJ §7 平台壳契约的印证 / 否证

| LSSMJ §7 主张 | 本批证据 | 判定 |
| --- | --- | --- |
| 壳只做「贴图 + 回传坐标」，渲染器输出预乘 RGBA + 命中表 | DComp 只吃位图（W2F-075）；SpriteVisual=画刷画像素（W3G-032）；CA「只是图像」（W3G-058）；Android 层=缓冲+属性（W3G-040） | **印证**（四家同构） |
| Windows `UpdateLayeredWindow` 路径 | DWM 重定向+blt 语义（W3G-006/019/022）；DWM 自身双缓冲（W3G-022） | **印证**（分层窗口=应用自供整窗位图，等价于把内容交给 DWM 的离屏表面） |
| 「每帧 1 次整窗提交（面积=damage 时可局部）」 | flip 零拷贝合成（W3G-019）+ Android 每帧 present（W3G-043）+ Apple 整树提交（W3G-057） | **印证**；「局部提交」在 Windows/Apple 公开面**不存在**（只 Android 有区域 API，W3G-047）→ 我们的局部提交属**平台自建**能力，必须自带正确性判据 |
| 帧的提交点永远单点 | DWM 会话级单例合成引擎（W3G-018）；Apple render server 服务所有前台进程（W3G-061）；Android present 单点（W3G-043） | **印证** |
| damage 后台栅格化 | Android offload 合成（W3G-049）+ needsAnotherUpdate 自排帧（W3G-050） | **印证**（有同族先例） |
| 「空 damage → 零重画」 | DWM 遮挡不重绘（W3G-023）+ Android GpuReuse（W3G-044） | **印证**（判据 C3 有平台先例） |
| 平台壳不做动画（动画归渲染器） | 反例：WinRT 合成层**承接动画**（W3G-030）；CA 改属性即生成隐式动画（W3G-074） | **否证「壳绝不做动画」**：若平台合成器能承接（Windows Visual layer / CA），把过渡动画下沉到壳侧是**更省**的一条可选档；LSSMJ P0 仍不做，但应在 ADR 里记这条可选路线 |
| 用位图承载全部内容（含文本） | 无官方否证；但 XAML 共享表面说明**宿主框架掌握表面尺寸/生命周期**（W3G-036） | **有界印证**：嵌入宿主时表面归宿主，壳契约需含 surface 归属条款 |

## 6. 证据索引（同批已锚定、上文未逐条引用的条目）

- **Windows 帧数据面**：DWM Frame Timing API 家族的存在（W3G-026）与理由——媒体应用与 DWM 的呈现时间表
  是**异步**的，不紧控会产生采样伪影（W3G-027）。
- **WinUI3 / WinRT 合成**：Visual 树是 Microsoft.UI.Composition 其他特性的基座，节点种类含 Container/Sprite/Layer/Shape/Redirect/Scene（W3G-031）；
  Windows App SDK 的定位=同一套 API 供应 WinUI3 与 WPF/WinForms/Win32（W3G-035）；WinUI 已被 Windows shell 与内置应用采用（W3G-037）。
- **Android 顺序语义**：层绘制序=中序遍历伪代码（负 z 在父下、非负在父上、同 z 按创建序）（W3G-041）；
  每帧开头清空「有排队帧的层」集合（W3G-053）——帧内集合是显式的。
- **Apple 机制**：视图内容缓存成位图给硬件直接操纵（W3G-064）；presentation tree 的「在飞值」只读不可写（W3G-066）；
  SwiftUI 三要素 identity/lifetime/dependencies（W3G-068）；WWDC23 把「更新速度」当独立维度讲（与绘制分开，W3G-072）。

## 7. 未验证项（缺什么证据）

1. **WinUI 3 渲染栈无官方架构页**：本次只抓到框架级陈述（"high-performance rendering" W3G-033；
   "advanced pixel rendering, high-DPI visuals, and smooth animations" W3G-073），
   WinUI3 与系统 XAML（Windows.UI.Xaml）的**同一性/分叉关系**、以及其合成是否走 Visual layer 的官方文字**未取到**。
   本报告因此不对「WinUI3 渲染实现」下任何结论。
2. **SwiftUI 渲染模型无规范**：Update/Draw 循环只有 WWDC 口语（依赖图、只更新需要新 body 的视图，W3G-067/069），
   无属性图（AttributeGraph）官方文档；本批**不写 SwiftUI 的帧级主张**。WWDC24 TextRenderer 只证明「有自绘入口」（W3G-070）。
3. **macOS `WindowServer` 名称与进程职责无官方锚**：Apple 公开材料统一称 "render server"（W3G-055/061），
   本次未取得任何用 "WindowServer" 指代合成器的官方页面（Tech Talk 为 iOS 语境）。
   → macOS 侧「WindowServer 持有帧」是**行业常识级推断，本报告不作为结论**。
4. **Android 无 docs 域快照**：`source.android.com` 本机不可达（curl 超时），本批 Android 全部结论来自
   `frameworks/native` 源码（LineageOS 镜像 @9568531）与 FrontEnd readme；**未取得 Android 官方图形架构文档的版本级口径**。
   另：`developer.android.com`（RenderThread/hwui 概览页）本次同样 000 不可达——该域上一批（w2f）已有快照，但本批**未复抓**。
5. **Windows damage API 的缺席是「未见」而非「没有」**：本批只覆盖 DWM/DComp/DXGI/UWP 合成族的公开页；
   是否存在仅面向驱动（WDDM）或未公开的 damage 通道，本报告不排除，但**不作为主张**。
6. **Apple 侧无 macOS 专属数字**：16.67/8.33ms 等读数均出自 iOS/iPad 语境的 Tech Talk（W3G-078），
   macOS 上 ProMotion 的处理（CADisplayLink preferredFrameRateRange 等）本批未取得新锚（`preferredframerate.json` 404，已留档 manifest）。
7. **缓冲数、overlay 指派、DRR 档位**这些「运行时状态量」在 LSSMJ 形态（无平台 VSync 依赖的候选窗）下是否需要建模，
   属**待测**：判据应是「不建模时帧时 P95 是否劣化」——本批只提供平台对照，不提供结论。
