# 运行期纪律：离线编译、管线预建与规范阶段序（C1 深读 6/7）

> 来源（抓取 2026-10-01，全部 200）：
> Impeller README `raw.githubusercontent.com/flutter/flutter/master/engine/src/flutter/impeller/README.md`；
> Vulkan 规范片元操作章 `raw.githubusercontent.com/KhronosGroup/Vulkan-Docs/main/chapters/fragops.adoc`；
> Vulkan 描述符索引附录 `.../appendices/VK_EXT_descriptor_indexing.adoc`；
> vkguide GPU-driven 章（管线切换成本与 ubershader 段）。
> 账本：`w3a.jsonl` 的 `W3A-026..028`、`W3A-072..074`、`W3A-021/022`。
> 注：Impeller 的**代码侧**事实已在 `w2b`（Flutter 轨道）与 `w3f` 有条目；本条只取 README 的**目标纪律**表述。

## 1. 它解决什么成本

- 运行期最贵的三件事：**着色器编译、管线状态创建、绑定/切换**（在 UI 帧预算里都是尖刺）。
  Impeller 的答案是全部**前移到构建期**：
  - "所有着色器编译与反射都在构建期离线完成"（README 目标段，逐字见 `W3A-072` 的下半句
    "All pipeline state objects are built upfront"）；
  - 离线编译器是**独立子框架**（GLSL 4.60 → 后端专用表示，`W3A-073`）。
- vkguide 侧的成本序佐证：`VkCmdBindPipeline` 是 Vulkan 绘制对象最贵调用之一（`W3A-022`）；
  少管线（Doom Eternal <500）是 bindless/ubershader 路线的核心（`W3A-021`）。

## 2. 机制清单（可直接当 GPU 档准入清单）

1. **构建期编译全部着色器 + 预建管线状态对象**（`W3A-072`）——运行期零编译。
2. **编译器独立成件**（离线工具，`W3A-073`）——而非运行期组件。
3. **资源普遍可标记**（README 目标第 2 条：纹理/缓冲/管线状态全部打标签，便于捕获与复盘）——可观测性前移。
4. **大描述符集 + 绑定后更新**（`W3A-026/027`）：一次绑定、索引取用；退路=每材质 1 个 draw-indirect（`W3A-028`）。
5. **阶段顺序由规范定义**：Vulkan 片元操作章固定了"丢弃后不再执行任何后续操作"等语义（`W3A-074`），
   我们的 CPU stage 列表与之对拍时有明确参照。

## 3. 对本项目的取舍

- **吸收（GPU 档第一条纪律，设计 §4.2 已写）**：构建期离线编译全部着色器（`W3A-072`）+
  编译器独立工具（`W3A-073`）——GPU 档立项的准入条件（不满足不立项）。
- **吸收**：成本序"管线 > 绑定 > draw"（`W3A-022`）指导批键设计；少管线数目标对齐 ubershader 思路（`W3A-021`）。
- **有界吸收**：bindless（大描述符集）**待核证**目标后端支持度（`W3A-026`）；不可用则走材质批键退路（`W3A-028`）。
- **不吸收**：Impeller 的 C++ 子框架结构本身（我们单 crate 更小）；只取纪律不取结构。

## 4. 未验证

- Impeller README 为 `master` 分支快照（抓取日固定），未记录 commit；引用时以"2026-10-01 抓取"为口径。
- Vulkan 规范 adoc 为**生成源**（含 `ifdef` 标记，`W3A-074` 引文落在无标记段落），引用时以此为准。
