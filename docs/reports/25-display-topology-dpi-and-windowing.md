# 25 · 显示拓扑、DPI 缩放与窗口合成（多显示器 / 逻辑-物理换算 / 交换链 / VRR）

> **这份报告是什么**：LSSMJ 显示面的**拓扑与缩放证据锚定**。此前的显示面是分块的：w3g 给了平台合成器
> （DWM/SurfaceFlinger/WindowServer）、w5a 给了分层窗贴图、w10b 给了色彩管理与 HDR 显示链、
> w11a 给了输入坐标；但**多显示器、逐显示器 DPI、逻辑↔物理换算、缩放变化、窗口放置与交换链重建**
> 是空白（关键词扫描：per-monitor/fractional scale/hiDPI/multi-monitor/devicePixelRatio/backing scale/
> window placement/work area/hotplug 全仓命中≈0）。
>
> **上游身份**：多源——Windows High DPI 与多显示器/VRR 文档 / Apple `NSScreen`（JSON 通道）/
> W3C Window Management 与 CSSOM View / Wayland fractional-scale 与 wl_output（wayland.app 镜像）/
> XRandR 协议 / Vulkan WSI 规范（raw adoc）/ wgpu `SurfaceCapabilities` 与 `SurfaceConfiguration` /
> **winit `8b5f46d4` DPI 与 monitor/window/event** / **egui `6b420bc1` 缩放** /
> **Flutter `4e5a0929` view metrics** / **Godot `084a2caa` Window/DisplayServer 文档** / Crossref 论文元数据。
> **抓取物**：`D:/KF/LSSMJ/scratch/w13a/raw/`；复算脚本 `scratch/w13a/fetch.py`、`gen.py`。
> **账本**：`docs/analysis/ledger/w13a.jsonl` **49 条 / verify rejected=0**（source 18 / doc 26 / paper 5；15 target）。
> **口径**：判定三档——**可用 / 有界 / 不可用**；Rust 可得性——**纯 Rust / 平台 API / 用系统能力**。

## TL;DR（10 条，每条带锚）

1. **逻辑与物理必须类型分离且显式换算**：winit 把 Physical* 定义为"设备上的实际像素"、Logical* 为逻辑单位
   （`W13A-001`），并用不同结构体承载（`W13A-003`）；缩放因子入参做合法性校验（正、有限、`W13A-002`）。
   Web 侧对应 CSS 像素与 `devicePixelRatio`（`W13A-034/035`），Flutter 把它做成**每个视图自带**的
   `devicePixelRatio`（`W13A-011/012`）。⇒ LSSMJ 的物理像素只出现在光栅/交换链边界，换算只在一处发生。
2. **"显示器缩放"与"窗口缩放"是两个字段**：winit 的 `MonitorHandle::scale_factor` 明确与窗口的不同
   （`W13A-004`），Wayland 下二者可能不一致（`W13A-005`）；Godot 的 DisplayServer 提供逐屏
   `screen_get_scale` 与全局 `screen_get_max_scale`（`W13A-016/017`）。⇒ 混合 DPI 多屏下不能互相代替。
3. **缩放变化是显式事件、也是一级失效源**：winit 有 `ScaleFactorChanged` 事件（`W13A-007`）。
   LSSMJ 收到缩放变化必须重建字形图集/位图/布局——它和"内容版本号变化"同级，不是小优化。
4. **平台 DPI 模式决定坐标系**：Windows 默认对未声明的应用做位图拉伸（`W13A-019`），
   推荐 per-monitor DPI 感知（`W13A-020`）；per-monitor 下应用看到物理像素（`W13A-021`）；
   不同模式使用不同坐标空间（`W13A-022`）。`GetDpiForWindow` 提供逐窗口 DPI（`W13A-025`），
   `SetThreadDpiAwarenessContext` 允许同进程内隔离不同模式（`W13A-024`）。⇒ 目标=per-monitor v2 + 进程内隔离。
5. **各平台模型可统一成"统一虚拟桌面坐标系 + 每显示器矩形"**：winit 用桌面坐标系的左上角定义显示器
   （`W13A-006`）；Wayland 用合成器坐标系（`W13A-038`）；XRandR 是"一个 screen + 多个 monitor 矩形"
   （`W13A-039`），并把 output/CRTC/screen 分离（`W13A-040`）。Windows 同理（`W13A-026`）。
   W3C 的 `ScreenDetailed`/`availLeft`（`W13A-031/032/033`）可直接映射字段名。
6. **缩放是"系数 × 策略"两层，且应用缩放与系统缩放解耦**：Godot 的窗口有
   `content_scale_factor` + `content_scale_mode`（`W13A-014/015`）；egui 的最终值是
   `zoom_factor × native_pixels_per_point`（`W13A-009`），用户级 GUI 缩放是独立能力（`W13A-010`）。
   ⇒ LSSMJ 的缩放=系统 DPI × 应用缩放，各自可改、可金样。
7. **交换链配置从能力集里选，尺寸以 surface 当前值为准**：wgpu 的 `SurfaceCapabilities` 暴露
   formats/present_modes/alpha_modes（`W13A-043`）；Vulkan WSI 规定 `currentExtent` 为特殊值时
   图像会被缩放到窗口大小（`W13A-041`），系统合成器下 `currentExtent` 就是窗口大小（`W13A-042`）。
   配置里还有 `desired_maximum_frame_latency`（`W13A-044`）⇒ 重建时按能力集重选，不缓存旧值。
8. **VRR 与"允许撕裂"绑定，且重建时标志必须一致**：Windows 文档明确 VRR 需要 tearing/vsync-off
   （`W13A-027`），并要求 `ResizeBuffers` 与 `Present` 使用同一 tearing 标志（`W13A-028`）。
   感知研究（`W13A-047`）给高刷/延迟的取舍锚——不是"越高越好"，而是**帧延迟与刷新率分开记账**。
9. **"可用区"是窗口系统的第一类概念**：Godot 有 `screen_get_usable_rect`（`W13A-018`）；
   Apple 用 `frame` vs `visibleFrame` 区分整屏与扣除菜单栏/Dock 的区域（`W13A-030`）；
   W3C 用 `availLeft/availTop`（`W13A-033`）。⇒ LSSMJ 的窗口贴合/最大化用可用区，全屏用整屏，二者不混用。
10. **平台差异要落到能力位，而不是 if-平台**：macOS 的 `backingScaleFactor`（`W13A-029`）、
    Wayland 的 `preferred_scale`/`set_buffer_scale`（`W13A-036/037`）、X11 的 monitor 矩形（`W13A-039/040`）
    描述的是同一组能力（逐屏缩放/分数缩放/统一坐标系/可用区）。LSSMJ 用能力位建模，平台分支只出现在壳层。

## 1. 建议模型（显示器 → 视图 → 交换链 三层 + 失效源）

```text
显示器层    统一虚拟桌面坐标系：每显示器 {位置, 尺寸, 可用区, 缩放, 色彩/HDR, 刷新率}   W13A-006/026/031/032
    │  ① 逐屏查询（scale/dpi/usable_rect；Wayland 分数缩放协商）
    ▼
视图层      每窗口/视图：{逻辑尺寸, 物理尺寸, device_pixel_ratio, 应用缩放}             W13A-001/003/009/011/014
    │  ② 缩放变化 = 一级失效源（字形图集/位图/布局/命中表全部重建）
    ▼
交换链层    能力集选择（格式/呈现模式/alpha）+ currentExtent + 最大帧延迟 + VRR 标志    W13A-041/042/043/044/027
```

**失效源清单（进 C21 门）**：缩放因子变化（`W13A-007`）、显示器切换/热插拔（`W13A-005/006`）、
窗口跨屏（`W13A-004/033`）、交换链尺寸变化（`W13A-041/042`）、VRR/呈现模式切换（`W13A-027/028`）。

## 2. 技术判定表（逐项：技术 / 性质 / Rust 可得性 / 判定 / 锚）

| 技术 | 性质 | Rust 可得性 | 判定 | 锚 |
| --- | --- | --- | --- | --- |
| 逻辑/物理坐标类型分离 + 校验 | 正确性基线 | 纯 Rust | **可用（必做）** | W13A-001/002/003 |
| 视图级 device pixel ratio | 多窗/多屏正确性 | 纯 Rust | **可用（必做）** | W13A-011/012 |
| 显示器缩放 vs 窗口缩放两字段 | 混合 DPI | 纯 Rust（平台映射） | **可用（必做）** | W13A-004/005/016/017 |
| 缩放变化事件 → 重建 | 失效源 | 纯 Rust | **可用（必做）** | W13A-007 |
| 每显示器 DPI 感知（Windows v2） | 清晰度 | 平台 API | **可用（默认档）** | W13A-019/020/021/022/025 |
| 进程内 DPI 模式隔离（线程级） | 嵌入兼容 | 平台 API | 有界（宿主隔离用） | W13A-024 |
| 统一虚拟桌面坐标系 | 多屏模型 | 纯 Rust | **可用（必做）** | W13A-006/026/038/039 |
| 输出/控制器/显示区分离 | 镜像/拼接 | 纯 Rust（抽象） | 有界（平台差异） | W13A-040 |
| 可用区（工作区）语义 | 窗口贴合 | 纯 Rust（平台映射） | **可用（必做）** | W13A-018/030/033 |
| 系统缩放 × 应用缩放两层 | 可访问性 | 纯 Rust | **可用（默认档）** | W13A-009/010/014/015 |
| 分数缩放（1.25/1.5） | 平台差异 | 平台 API（Wayland） | 有界（需重采样策略） | W13A-036/037 |
| macOS backingScaleFactor | 平台 API | 平台 API | 可用 | W13A-029 |
| X11 单屏多 monitor 模型 | 平台 API | 平台 API | 可用 | W13A-039/040 |
| 交换链能力集选择 | 跨平台呈现 | 纯 Rust（wgpu） | **可用（必做）** | W13A-043 |
| currentExtent / surface 尺寸语义 | 重建正确性 | 纯 Rust（wgpu/Vulkan） | **可用（必做）** | W13A-041/042 |
| 最大帧延迟（帧在飞数量） | 延迟/吞吐 | 纯 Rust | 可用（预算联动） | W13A-044 |
| VRR + 允许撕裂 | 高刷档 | 平台 API | 有界（显式档，默认关） | W13A-027/028/047 |
| 交换链重建保留标志 | 重建正确性 | 纯 Rust | **可用（必做）** | W13A-028 |
| 高 DPI 内容分级策略（文本/图形/密集数据） | 可用性 | 纯 Rust | 有界（质量档） | W13A-045 |
| 超大逻辑画布（多屏拼接/大屏） | 场景支持 | 纯 Rust | 有界（布局/命中压力） | W13A-046/048/049 |
| 多显示器独立色彩/HDR 配置 | 显示质量 | 平台 API（见 w10b） | 有界（与 C18 联动） | W13A-031/032 |

## 3. 重点来源短分析（5 份）

### 3.1 winit DPI/monitor/event——Rust 侧的坐标与缩放基线

- Physical/Logical 类型分离（`W13A-001/003`）、缩放校验（`W13A-002`）、
  显示器缩放与窗口缩放的差异（`W13A-004/005`）、桌面坐标系（`W13A-006`）、
  `ScaleFactorChanged` 事件（`W13A-007`）、`current_monitor()` 可空（`W13A-008`）。
- 结论：**LSSMJ 的坐标与缩放模型直接建立在 winit 的 DPI 类型上**；多屏差异在抽象层显式保留。

### 3.2 Windows High DPI + 多显示器 + VRR——平台模式与呈现

- DPI 感知模式与坐标空间（`W13A-019/020/021/022`）、线程级隔离（`W13A-024`）、
  逐窗口 DPI（`W13A-025`）、多显示器模型（`W13A-026`）、
  VRR 需要 tearing（`W13A-027`）与标志一致性（`W13A-028`）。
- 结论：**Windows 档目标=per-monitor v2**；VRR 作为显式档，交换链重建必须保留创建标志。

### 3.3 Apple NSScreen + W3C Window Management——字段集与可用区

- `backingScaleFactor`（`W13A-029`）、`frame` vs `visibleFrame`（`W13A-030`）、
  `getScreenDetails()`/`ScreenDetailed`/`availLeft`（`W13A-031/032/033`）、
  CSS 像素与 `devicePixelRatio`（`W13A-034/035`）。
- 结论：**显示器描述字段集照 ScreenDetailed 对齐**（可用区 + 缩放 + 色彩），前端范式可直接映射。

### 3.4 Wayland fractional-scale + XRandR——分数缩放与统一坐标系

- `preferred_scale`（`W13A-036`）、`set_buffer_scale`（`W13A-037`）、
  合成器坐标系（`W13A-038`）、XRandR 的"一个 screen + 多 monitor"（`W13A-039`）与 output/CRTC 分离（`W13A-040`）。
- 结论：**分数缩放需要显式的重采样策略**（字形/位图不能只按整数缩放做）；统一坐标系是跨平台共识。

### 3.5 Vulkan WSI + wgpu Surface——交换链重建的契约

- `currentExtent` 与缩放语义（`W13A-041/042`）、能力集（`W13A-043`）、
  配置字段含最大帧延迟（`W13A-044`）。
- 结论：**交换链重建=读能力集 → 选配置 → 保留创建标志**；尺寸以 surface 当前值为准。

## 4. 与既有轮的接缝

- **与 w3g（平台合成器）**：w3g 给了 DWM/SurfaceFlinger/WindowServer 的合成侧；本轮给"显示器拓扑 + 缩放 + 交换链"，
  两者共同构成"窗口→合成器→显示器"的完整链。
- **与 w5a（分层窗贴图/壳）**：w5a 的位图契约在逻辑↔物理换算上依赖本轮结论（贴图尺寸=物理像素，
  命中坐标=逻辑像素）。
- **与 w10b（色彩/HDR）**：色彩空间与 HDR 是**逐显示器**属性；本轮的显示器对象要带色彩能力，
  两者共同决定输出配置（C18 与 C21 联动）。
- **与 w11a（输入坐标）**：输入事件的坐标必须带"哪个视图/哪个显示器/逻辑还是物理"的标签；
  缩放变化时命中表与捕获状态要一起更新（否则跨屏拖动会错位）。
- **与 w9c（停靠/工作区）**：可用区（工作区）是停靠与最大化布局的输入；多屏下"工作区"应逐屏定义。

## 5. 未验证项（缺什么证据）

1. **未运行任何上游代码**：全部为文档/协议/源码静态观察；缩放切换重建、跨屏拖动、VRR 档行为都要实测。
2. **Wayland 协议原文未取到**：`gitlab.freedesktop.org` 返回 Anubis 反爬页；改用 wayland.app 的协议镜像
   （`W13A-036/037/038`）。协议版本号与 staging 状态未逐字核。
3. **Android 显示/DPI 文档未取**：`developer.android.com`/`source.android.com` 在本机网络超时
   （与 w10b/w10c/w11a 同因）；Android 的 density/缩放模型未进账本。
4. **无实测读数**：没有混合 DPI 多屏下的错位率/重建耗时、分数缩放的重采样质量、VRR 的延迟收益。
5. **wgpu `SurfaceConfiguration` 版本未钉**：证据来自 docs.rs 的 wgpu-types latest 页面（2026-10-03 抓取）；
   落地需锁版本并做能力探测。
6. **VRR/高刷的平台细节只到"标志"层**：G-Sync/FreeSync 的驱动行为、HDR+VRR 组合、
   `ResizeBuffers1` 的多适配器差异都未展开。
7. **Apple 文档只到 JSON 摘要层**：NSScreen 的完整 API（`convertRectToBacking`、多屏通知）未逐节核；
   `W13A-029/030` 只支撑"backing scale / 可见矩形"两个字段。
8. **显示器热插拔事件未建模**：本轮取了拓扑查询与坐标系，但"显示器增删"的事件与重建流程
   （交换链是否需要重建、窗口如何迁移）未在 ledger 中单列，留引擎期设计。

> **补锚更新（w14a）**：第 3 项（Android）部分补——Android 色彩/HDR/输入文档已取（`W14A-012..019`），但显示 DPI/scaling 页仍未取；第 2 项（Wayland 原文）继续用 wayland.app 协议镜像（`W13A-036..038`）。
