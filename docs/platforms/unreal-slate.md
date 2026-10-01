# Unreal：Slate 框架与 UMG（dev.epicgames.com 官方文档；抓取 2026-10-01）

> 上锚：本报告**无源码主体**（任务规定不克隆 Unreal）；全部为官方文档深度，页面标题自述版本为
> **Unreal Engine 5.8 Documentation**（dev.epicgames.com/documentation/en-us/unreal-engine/*，抓取日期 2026-10-01）。
> 每条主张对应 `docs/analysis/ledger/w2e.jsonl` 的 W2E-0xx 行；引文已对抓取缓存逐条复验（bad=0）。
> 说明：Epic 的旧站 `docs.unrealengine.com/4.27/...` 返回 HTTP 403，本批统一改用 dev.epicgames.com 的 SSR 页面。

## TL;DR（每条带锚）

1. 失效系统语义：widget 布局变化被标 `invalidated` 后，**只有失效 widget 及其子级会被重绘**（`W2E-058`）。
2. 三段数据链 **Hierarchy → Layout → Paint**，各自独立缓存；变化时进入 dirty list，下一帧统一重算（`W2E-058` 所在页的失效表）。
3. 局部手段 = **Invalidation Box**（缓存子树几何 + 监视变化；在 UMG 调色板 Optimization 分类）（`W2E-059`）。
4. 全局手段 = `Slate.EnableGlobalInvalidation = true`，等效整窗包一个 Invalidation Box，且**局部 Box 的缓存开关被强制禁用**（`W2E-060`/`W2E-072`）。
5. 高频变化的 widget 应标 **Volatile**：不缓存其 Paint 数据（但布局仍可跳过）（`W2E-064`）。
6. **Retainer Panel** = 把子树压平成单张纹理（可限帧率），代价官方写明：重绘开销大、内存高于 Invalidation Box，应先用 Box（`W2E-061`/`W2E-062`/`W2E-063`）。
7. 合批键含 Z 维：Slate 的 draw call 按 widget **Layer ID** 分组；**Canvas Panel 会递增子级 Layer ID**（为自由叠放）→ 多 draw call（`W2E-065`/`W2E-066`）。
8. 裁剪 2.0（4.17 起）：scissor + stencil 组合，**不再跨裁剪区合批**，换来顶点格式省 6 个 float、像素着色器不再做裁剪（`W2E-073`/`W2E-074`）；默认极少 widget 裁剪（`W2E-075`）。
9. 文本：`FSlateFontInfo`（字体表示）+ `FSlateFontCache`（**按需把字符缓存进纹理**）；图集内容类型含 Alpha/Color/**Msdf**（`W2E-079`/`W2E-078`/`W2E-081`）。
10. 渲染后端分模块：`/Engine/Source/Runtime/Slate/`（widget 库）与 `/Engine/Source/Runtime/SlateRHIRenderer/`（RHI 后端，`ISlateRHIRendererModule`）（`W2E-083`/`W2E-080`）。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 失效域 = 子树（Box 包住的范围）+ 向下传播 | `W2E-058`/`W2E-059` | 吸收：与 UGUI 的「Canvas island」是同一模型的两个实现，青简可按同样方式划域。 |
| Hierarchy/Layout/Paint 三段缓存分离（各自可失效） | `W2E-058` | 吸收：把「布局算过」与「绘制数据算过」分开缓存，是增量 UI 的骨架。 |
| Volatile 逃生阀（按元素关闭缓存） | `W2E-064` | 吸收：自动缓存必须可按元素退出，否则高频元素反被缓存拖累。 |
| 全局/局部两档失效 + 明确互斥规则 | `W2E-060`/`W2E-072` | 有界吸收：模式互斥规则可学；候选窗规模小，全局档不必要。 |
| Retainer（压平成纹理换 draw call） | `W2E-061`/`W2E-062`/`W2E-063` | 有界吸收：仅在极端预算场景才用，且官方排序在 Box 之后。 |
| 合批键含 Z/Layer（叠放语义与合批冲突） | `W2E-065`/`W2E-066` | 吸收：批键设计必须把 Z 维度记账，容器语义直接决定批次预算。 |
| 裁剪方案的成本记账（顶点带宽 ↔ 批次） | `W2E-073`/`W2E-074` | 吸收：两种裁剪路线的取舍数据可复用（青简做圆角/滚动裁剪时会遇到同样选择）。 |
| 「默认不开裁剪」（按需开启） | `W2E-075` | 吸收：默认关闭昂贵特性，比全局开启更稳。 |
| 动画成本分级（材质 0 CPU / transform 触发布局失效） | `W2E-068`/`W2E-069` | 吸收：动画通道选择决定失效级别，与 Unity usage hints 结论互证。 |
| 字体图集按需填充 + 图集内容可 SDF/MSDF | `W2E-078`/`W2E-081` | 吸收：与 TMP 的 Distance Field 结论互证，青简字形图集可预留 SDF 通道。 |
| Widget 层调试器（Reflector + Insights） | `W2E-070`/`W2E-071`/`W2E-076` | 吸收：失效/更新/绘制三段各有开关与逐帧列表，是验证失效模型的验收工具。 |
| Unreal 的 RHI 渲染后端与 widget 树分层 | `W2E-080`/`W2E-083` | 不吸收（结构参考）：模块边界可借鉴，实现与 Unreal 渲染管线强耦合。 |
| UMG 具体控件选型（SizeBox/Spacer/Canvas Panel 等） | `W2E-066`/`W2E-067` | 不吸收（引擎特有）：属 UMG 层资产约束，与青简控件无关。 |

## 1. 模块分层（文档口径）

- **Slate**：widget 库与编程框架（`/Engine/Source/Runtime/Slate/`，含 SRichTextBlock 等大量 widget 类）（`W2E-083`/`W2E-082`）。
- **SlateCore**：框架核心（`/Engine/Source/Runtime/SlateCore/`）；基类语义：`SWidget`（基类）、`SLeafWidget`（叶）、`SCompoundWidget`（「多数非原语 widget 的基类」）、`SPanel`（「arranges its child widgets on the screen」）、Box/Overlay/Stack 面板族（`W2E-077`）。
- **SlateRHIRenderer**：渲染后端模块（`ISlateRHIRendererModule`、`FSlateRHIRenderingPolicyInterface` 等）（`W2E-080`）。
- **UMG**：Slate 的配置化封装（面向设计师/蓝图），优化指南与失效文档都以 UMG 视角给出操作路径（`W2E-059` 的 UMG 调色板入口、`W2E-064` 起）。
- 游戏侧接入：需在 `*.build.cs` 加 `Slate`/`SlateCore` **私有**依赖（`InputCore` 为公有依赖）（`W2E-084`）。

## 2. 失效模型（Invalidation）

1. **定义**：减少 UI 的 CPU 占用，做法是限制 widget 重绘频率；变化者标 invalidated，之后只重绘失效子树（`W2E-058`）。
2. **数据链**：Hierarchy → Layout → Paint 三段，每段各自缓存；缓存位置在父 Invalidation Box 或（全局模式下）SWindow；变化进入 dirty list，下一帧按数据类型重算（`W2E-058` 所在页面）。
3. **Invalidation Box**：缓存子树几何 + 监视变化；不变即复用缓存几何（`W2E-059`）。
4. **Global Invalidation**：cvar `Slate.EnableGlobalInvalidation`；局部 Box 被停用，只留 SWindow 缓存（`W2E-060`/`W2E-072`）。
5. **Volatile**：逐帧变化者标记后不缓存 Paint 数据（布局仍可能跳过）；设置项在 widget Details 的 Performance > Is Volatile（`W2E-064`）。
6. **Retainer Panel**：子树 → 单纹理；可配不同帧率（部分 UI 30 FPS、部分 60 FPS）；代价 = 每次重绘高开销 + 每 Retainer 独立 RT（`W2E-061`/`W2E-062`）。
7. **官方选型次序**：先 Invalidation Box；仍要省 draw call 才上 Retainer（低端移动端等极限预算）（`W2E-063`）。

## 3. 绘制与合批（Layer ID / 裁剪 / 压平）

- **Layer ID 分组**：draw call 按 widget 的 Layer ID 分组（`W2E-065`）。
- **容器语义决定批次**：Vertical/Horizontal Box 会**合并**子 widget 的 Layer ID；Canvas Panel **递增**子级 ID 以支持叠放 → 多次 draw call，官方称其「highly CPU-intensive」；Overlay 同样递增但范围有限（`W2E-066`）。
- **布局原语成本**：SizeBox 多趟计算 + 自身绘制，Spacer 显著更便宜（`W2E-067`）。
- **裁剪 1.0 → 2.0**（4.16 → 4.17 及以后）（`W2E-073`/`W2E-074`/`W2E-075`）：
  - 1.0：轴对齐、布局空间、每顶点 6 float 像素着色器裁剪，「Allowed batching across clipping rects」；
  - 2.0：任意四边形 → scissor（轴对齐区）+ stencil（复杂区）组合；**不再跨裁剪区合批**；默认只有滚动面板/可编辑文本等少量 widget 裁剪。
- **UI 无效数据**：Retainer 的压平是「以 RT 换 draw call」的极端手段（`W2E-061`）。

## 4. 文本（SlateFontInfo / 字体缓存 / 图集）

- `FSlateFontInfo`：Slate 的字体表示，struct，头文件 `/Engine/Source/Runtime/SlateCore/Public/Fonts/SlateFontInfo.h`；构造参数含 `FCompositeFont`、Size、TypefaceFontName、`FFontOutlineSettings` 等（`W2E-079`）。
- `FSlateFontCache`：官方一句话职责「Font caching implementation Caches characters into textures as needed」——**按需把字符缓存进纹理**（即字形图集）（`W2E-078`）。
- 图集内容类型枚举 `ESlateFontAtlasContentType`：`Alpha`（linear，即旧 IsGrayscale）、`Color`、`Msdf`（`W2E-081`）。
- 可视化：Widget Reflector 可显示 Texture Atlas / Font Atlas（`W2E-070`）。
- 与 Unity 互证：TMP 的 Distance Field 图集与 Slate 的 Msdf 属同一技术家族（`docs/platforms/unity.md`，`W2E-027`/`W2E-028`）。

## 5. 工具链（可观测性）

- **Widget Reflector**（`Ctrl+Shift+W` / Tools > Debug）：查看 widget 层级、atlas（NxN 纹理/字体）、以及 Slate Debug Options——`Widget Caching、Invalidation Debugging、Invalidation Root Debugging、Update Debugging、Paint Debugging、Show Clipping、Debug Culling`（`W2E-070`/`W2E-071`）。
- **Invalidation 调试开关与缓存互斥**：`Widget Caching` 在 GlobalInvalidation 模式下恒禁用（`W2E-072`）。
- **Slate Insights**（内置插件）：Slate Frame View 逐帧列出「被绘制/失效/更新」的 widget，用于定位失效是否按预期传播（`W2E-076`）。
- **优化内容入口**：官方 Testing and Optimizing Your Content 收录 GPU 状态记录等前置取证工具（`W2E-085`）。

## 6. 坑与反例（负面留档）

1. **Canvas Panel 的反直觉成本**：为支持自由叠放递增 Layer ID → 多 draw call；官方明确「单元素 widget 一定不需要 Canvas Panel」（`W2E-066`）。
2. **Retainer 的双代价**：重绘开销高 + 每实例 RT 内存；官方定位在 Invalidation Box 之后（`W2E-062`/`W2E-063`）。
3. **全局模式的副作用**：开启 GlobalInvalidation 后局部 Box 缓存被禁用（配置看似叠加、实为互斥）（`W2E-072`）。
4. **裁剪改版的隐式回归**：2.0 换掉 1.0 的「跨裁剪区合批」能力——升级引擎可能让原本合批的 UI 批次变多（`W2E-073`）。
5. **动画属性选错 = 每帧布局失效**：改 render transform 的动画会触发布局失效并每帧重算（官方建议避免或标 Volatile）（`W2E-069`）。
6. **文档可达性坑**：`docs.unrealengine.com/4.27/...` 返回 403（本批实测），旧链接不可依赖。

## 7. 未验证项（缺什么证据）

- **源码级机制**（任务禁止克隆 Unreal）：`FSlateElementBatcher` 的实际合批实现、InvalidationPanel 的缓存数据结构、`SlateRHIRendererModule.cpp` 的图集工厂（`FSlateRHIFontAtlasFactory`）等，均**只有社区/搜索线索，无本批账本行**。
- 字体图集容量治理的官方 cvar 口径（如 `Slate.MaxFontAtlasPagesBeforeFlush`、`Slate.DumpFontCacheStats`）——来自 Epic 论坛/搜索摘要，本批未抓到原文页面，**未入账**。
- UMG 的具体性能数字（优化指南未给出机器/口径化的 ms 表；其「动画成本」为分级定性，非基准）。
- Slate 的绘制线程模型（GameThread vs RenderThread 的提交时机）无官方概要页可锚。
- Widget Reflector / Slate Insights 的界面细节以文档为准，未做运行时实测。
