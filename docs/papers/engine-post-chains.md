# 三引擎后处理链与 AA 档位对照（UE / Unity / Godot 官方文档）

> 来源：15 份官方文档快照（UE 5.x dev.epicgames.com 5 篇、Unity 6/URP17/HDRP 6 篇、Godot stable 4 篇；
> UE 的 Anti-Aliasing 与 Motion Blur 两页为 JS 壳未取到，见未验证）。
> 账本：`../analysis/ledger/w7a.jsonl` W7A-117..158（全 doc，42 条）；日期 2026-10-02。

## 1. 后处理链的抽象与默认值

| 维度 | UE | Unity URP | Godot |
| --- | --- | --- | --- |
| 默认 | **默认有**后处理（可改）（W7A-117） | 新场景**默认无**（需 Volume+相机开关）（W7A-146） | Environment 资源显式挂到节点（W7A-153 邻域：默认弱/不可见） |
| 抽象 | Post Process Volume + 混合优先级（W7A-118） | Volume + Camera 开关 | Environment（相机优先） |
| 注入点语义 | Blendable Location 枚举：Before Translucency / Before Tonemapping / After Tonemapping / Replacing Tonemapper（W7A-119/121） | Full Screen Pass 注入点：Before Transparents / Before PP / After PP（默认）（W7A-144/145） | 自定义后处理=单 pass/多 pass 两种官方 tutorial 路线（本波未入账） |
| 性能纪律 | After Tonemapping（LDR）=性能首选（W7A-119）；Before Tonemapping 修 TAA/GBuffer 问题（W7A-120） | — | — |

**对 LSSMJ**：两条独立来源（UE 的 Blendable Location、Unity 的注入点）都表明"位置=语义"，
**后处理链应由显式枚举的段位组成**，不是任意插队；LSSMJ 的显示列表/执行器可沿用该结构
（段位：场景渲染 → 空间 AA → 时间 AA → bloom → tonemap → UI 合成）。

## 2. AA 四选与互斥

- 三家都把 AA 列为**可选配置**，且都含 FXAA/SMAA(或类)/TAA/MSAA 家族：
  Unity URP 四选（W7A-130..136）、Godot 五法（MSAA/TAA/FSR2/FXAA/SMAA1x，W7A-147..152）、
  UE 以 TAA 为默认（Karis 2014，W7A-027）。
- **MSAA↔TAA 互斥是三引擎口径**：Unity "cannot be used with TAA"（W7A-133）、Playdead README
  "disable MSAA"（W7A-024）、Salvi"TAA before resolve"（W7A-049）——LSSMJ 的档位表按互斥处理。
- **MSAA 的定位**（三家一致）：几何边缘最好、零模糊（W7A-149）、不治着色走样（W7A-135）、
  成本在 tiled GPU 上可能反转（W7A-136）、硬件上限 8x（W7A-058）。
- **TAA 的代价**（三家一致）：需要速度缓冲（W7A-137）、静止也糊+运动更糊（W7A-147）、
  鬼影（W7A-133 附近的 URP TAA 段）、渲染器能力门槛（Godot 仅 Forward+，W7A-148）。
- **FXAA 的现状**：最省（W7A-130/131），但"桌面已被时间 AA 取代"（Godot 口径，W7A-151）；
  SMAA 1x 是"更锐的替代"（W7A-132）。

## 3. Bloom / Glow / DOF 的默认值与档位

- **Bloom（UE）**：多级高斯 mip（UE3 三级→UE4 五级，1/2..1/32）（W7A-122/123）；阈值-1=全参与（W7A-125）；
  卷积版（FFT）判为影视/高端，标准版推荐游戏用（W7A-124）。
- **Bloom（Unity URP）**：默认强度 0=禁用、阈值 0.9；Downscale=Quarter 为性能推荐（W7A-139/140）；
  High Quality Filtering 降闪烁（W7A-141）。
- **Glow（Godot）**：默认弱/不可见（W7A-153）；混合模式五档、默认 Softlight 最弱（W7A-154）；
  低端档换实现（W7A-155）。
- **DOF**：UE 三层（Near/Far/Focal）+ 桌面/移动两档（W7A-126/127）；Unity Gaussian/Bokeh 两模式、
  Max Radius>1 会欠采样伪影（W7A-142/143）。

## 4. TAA 兼任上采样（UE 独有档）

- TAAU：时间积分与上采样同一 shader，比纯空间上采样锐（W7A-128）；代价=后续 pass 全分辨率（W7A-129）。
  对 LSSMJ 的含义：TAA 不是只买 AA，还能买"动态分辨率的锐度"——但成本结构要重排（链位置前移）。

## 5. compute vs 全屏 pass（证据有限，如实标注）

- Godot 文档给出 compute 的两条硬约束：仅 RenderingDevice 系渲染器可用（W7A-157）；移动端驱动
  支持普遍差（W7A-158）；定义层面 compute 无固定功能（W7A-156）。
- 本波**未取得**"compute vs 全屏 pass"的权威性能对比（GPUOpen/Intel 未找到合格材料）——
  列入未验证；LSSMJ 默认按全屏 pass 实现后处理，compute 仅作可选优化路线。

## 未验证 / 边界

- UE"Anti-Aliasing"与"Motion Blur"两页为客户端渲染壳（curl 取到的是 JS 骨架），本波无文本可引——
  相关主张改用 Karis 2014 与 UE Post Process 文档承载。
- 三家文档均为"当前版本"快照（UE 5.8 文档站、Unity 6000.0/URP17、Godot stable）；版本漂移未跟踪。
- 所有引擎行为描述为官方自述，本机未复测（无引擎运行环境）。
