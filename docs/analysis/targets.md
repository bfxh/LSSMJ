# 目标总清单（targets）

> 每个目标：类型（source 源码 / doc 文档 / paper 论文）/ 抓取方式 / 重点文件 / 产出 / 账本行数目标。
> 完成任务后在「状态」列打 ✅ 并记 commit。**总行数目标 ≥1000（论文轨道单列 ≥200）。**

## 轨道 A — 源码深读（source 主体）

| # | 目标 | 抓取 | 重点 | 产出 | 行数目标 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| A0 | **qingjian-render（交付主体）** | 已克隆 `scratch/src/qingjian` | `crates/qingjian-render/**` 全部 21 文件 + 消费侧 `apps/windows/server/src/ui/**`、`apps/macos/src/candidates/**` | `docs/renderer-qingjian/**`（主代理亲自写） | ≥80 | ⬜ |
| A1 | qingjian 全仓（core/dict/lm/learning/predict/translate/platform + apps + 工程实践） | 同上 | `crates/*/src/lib.rs`、`apps/*/src`、`docs/design/**`、`Cargo.toml`、CI | `docs/engines/qingjian.md` | ≥50 | ⬜ |
| A2 | 文本栈：cosmic-text（含青简 fork 差异）、swash、rustybuzz、fontdb、harfbuzz、taffy、parley/folio | clone 到 `scratch/src/<slug>` | 各自 `src/` 核心 + `docs/` + README 的数字 | `docs/engines/text-stack.md` | ≥60 | ⬜ |
| A3 | Rust UI 框架：egui、iced、slint、makepad、dioxus+blitz | clone（slint 大，sparse `internal/`, `crates/core*`） | 各自架构核心：egui `paint/`、iced `wgpu/`、slint 编译器+渲染、makepad `draw/` | `docs/engines/rust-ui-frameworks.md` | ≥60 | ⬜ |
| A4 | 矢量/2D 光栅：tiny-skia（青简直接依赖）、lyon、femtovg、vello、pathfinder、raqote | clone | tiny-skia：`src/`（路径光栅/毛玻璃/剪辑）；vello：`shaders/` 计算管线 | `docs/engines/2d-rasterizers.md` | ≥60 | ⬜ |
| A5 | gpui（zed）、Servo/WebRender、Skia+Graphite | clone；Skia sparse `src/core`,`src/gpu`,`src/text` | gpui `crates/gpui/src`（element/scene/paint）；WebRender `webrender/src`；Skia `docs/`+`src/core` | `docs/engines/gpui-webrender-skia.md` | ≥60 | ⬜ |
| A6 | **论文/文章批次①：文本渲染与整形** | curl/urllib（arXiv、作者站、官方博客） | SDF/MSDF/Slug、Loop-Blinn、矢量纹理、亚像素 AA、字体提示、HarfBuzz/rustybuzz、Knuth-Plass 断行、UAX#14/#29、CJK 混排 | `docs/papers/text-rendering.md` + 逐篇 `docs/papers/<slug>.md` | ≥60（paper） | ⬜ |
| B1 | Godot（GDScript 侧不做） | sparse clone `scene/gui`,`scene/theme`,`servers/rendering` | Control 布局/主题/`RenderingServer` 的 2D 路径 | `docs/engines/godot.md` | ≥50 | ⬜ |
| B2 | Flutter + Impeller | sparse clone `packages/flutter/lib/src/{rendering,widgets,painting}`；Impeller 走 `engine/src/flutter/impeller` sparse 或 docs | RenderObject/layer 树、`repaint boundary`、Impeller `entity/`+`renderer/` | `docs/engines/flutter-impeller.md` | ≥50 | ⬜ |
| B3 | Chromium（**文档轨道为主**） | `docs/` 镜像 raw：`docs/how_cc_works.md`、`docs/life_of_a_frame.md`、LayoutNG/BlinkNG/Slimming Paint 设计文档、`viz` README；cc 关键源码文件用 raw 拉 | 合成器/绘制缓存/图层化/栅格化调度 | `docs/engines/chromium.md` | ≥50 | ⬜ |
| B4 | Qt（Quick scene graph）+ GTK4（GSK） | Qt：`doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html` + `qtdeclarative` sparse；GTK：`docs.gtk.org/gsk4` + gtk sparse | QSG 节点/材质/图集/批处理；GSK 渲染器（`gsk/ngl`,`gsk/vulkan`） | `docs/engines/qt-gtk.md` | ≥50 | ⬜ |
| D3 | **Bevy（用户点名）** | clone `bevyengine/bevy`（sparse `crates/bevy_ui`,`bevy_text`,`bevy_ui_render`,`bevy_render`,`bevy_core_pipeline`） | UI 布局（taffy 集成）、文本（cosmic-text）、渲染（render graph/批处理） | `docs/engines/bevy.md` | ≥60 | ⬜ |
| D4 | 即时模式与游戏 UI 群：Dear ImGui、Nuklear、microui、RmlUi、Fyrox、O3DE(LyShine)、Flax、Stride、Cocos | clone（各取 `imgui.cpp`/`imgui_draw.cpp` 等核心与 UI 目录） | 即时模式绘制/布局/字体图集/DrawData 批处理 | `docs/engines/immediate-game-ui.md` | ≥55 | ⬜ |

## 轨道 B — 闭源与平台（doc/source 混合）

| # | 目标 | 抓取 | 重点 | 产出 | 行数目标 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| B5 | Unity（UGUI + UI Toolkit） | 官方文档 + Unity 博客 + `com.unity.ui` 包源码（若可得） | Canvas 重建机制/合批规则；UI Toolkit 的布局与绘制 | `docs/platforms/unity.md` | ≥50 | ⬜ |
| B6 | Unreal（Slate/UMG） | 官方架构文档 + 公开演讲/博客（不做源码主体） | Slate 的 widget/invalidation panel/绘制 | `docs/platforms/unreal-slate.md` | ≥45 | ⬜ |
| B7 | Apple：AppKit/CoreAnimation/CoreText/Metal | 官方文档 + WWDC 摘要 | layer backing/提交与合成/CoreText 排版 | `docs/platforms/apple.md` | ≥45 | ⬜ |
| B8 | 微软：WPF（`dotnet/wpf` sparse）+ DComp/DWrite 文档 + WinUI3 | sparse clone + 文档 | milcore/合成/`DrawingVisual`；DWrite 文本栈 | `docs/platforms/microsoft.md` | ≥45 | ⬜ |
| B9 | Android：Compose（sparse）+ HWUI/Choreographer 文档 | sparse clone `compose/{runtime,ui}` + 文档 | 重组（recomposition）/RenderNode/帧调度 | `docs/platforms/android.md` | ≥45 | ⬜ |
| B10 | 游戏中间件：Noesis/Coherent GameFace/RmlUi(补) 等 | 官方文档 + 公开资料 | 游戏内 UI 商业方案的架构卖点 | `docs/platforms/game-ui-middleware.md` | ≥30 | ⬜ |

## 轨道 C — 论文/文章（paper 主体，单列计数 ≥200）

| # | 组 | 抓取 | 行数目标 | 状态 |
| --- | --- | --- | --- | --- |
| C1 | GPU 渲染技术：Nanite/meshlet/GPU-driven/bindless/stencil-then-cover/NV_path_rendering/Pathfinder 系列/Vello 系列/Skia Graphite/Impeller 白皮书 | arXiv/作者站/官方博客（`curl`） | ≥60（paper） | ⬜ |
| C2 | 布局与增量计算：Cassowary、Yoga 文档、Adapton、Acar 学位论文、Salsa、Differential Dataflow、React Fiber 系列、细粒度响应式（Solid）、Svelte 编译 | 论文/官方文档/长文 | ≥60（paper/doc） | ⬜ |
| C3 | UI 系统与交互延迟：Immediate-mode 反思（Muratori/Elm）、dirty rect/compositor（Wayland damage、Tessera UI 查证）、帧调度（Choreographer/Frame Pacing）、输入延迟（Carmack）、IME（TSF/text-input-v3）、无障碍（UIA/AT-SPI） | 论文/官方文档/长文 | ≥60（paper/doc） | ⬜ |
| C4 | Web 栈与 JS 包装成本：Electron/Tauri 文档与性能分析、js-framework-benchmark 结果表、React/Solid/Svelte/Vue 运行时要点、DOM/Canvas 成本 | 官方文档 + 基准仓库数据（抄数字带 URL+日期） | ≥55（doc/web） | ⬜ |
| C5 | 中文/东亚文本：GB2312/GBK 点阵字库时代（GN SDK 案例）、FreeType hinting、标点挤压/避头尾、混排、输入法候选窗 UI 惯例 | 文档/论文/案例 | ≥50（doc/paper） | ⬜ |
| C6 | 工程方法学参照（门禁/棘轮/测量协议）——本仓已有先例（qingjian-gates、unified-rx） | 本地文档（doc 锚=仓库文件相对路径可用 source 深度） | ≥30 | ⬜ |

## 轨道 D — 案例与综合

| # | 目标 | 抓取 | 产出 | 行数目标 | 状态 |
| --- | --- | --- | --- | --- | --- |
| D1 | GN SDK（闭源）：头文件索引 + DLL 导出表（`objdump -p`）+ 样例交叉印证 + `代码功能简述与规范.txt` | 本地 `D:/KF/GN_SDK1e/GN_SDK1e` | `docs/renderer-gn/**`（渲染/UI/字体/布局） | ≥60 | ⬜ |
| D2 | Mineradio（Electron 42）：架构解剖 + 性能工程（LOW_SPEC 档）+ 成本账（内存/启动/GPU） | 本地 `scratch/src/Mineradio-paused-2.2.0` + 官方数据 | `docs/targets/mineradio-electron.md` | ≥50 | ⬜ |
| D5 | 综合报告（synth）：引擎全景/文本/布局/GPU/平台/Web 成本（由 synth 代理读全部报告后写） | 读 `docs/**` | `docs/reports/02..08,11` | ≥40 | ⬜ |
| D6 | 主代理：总览 + ADR + 性能方法论 + 渲染器对标章 | — | `docs/reports/00,01,09,10` + `docs/renderer-qingjian/09` | ≥40 | ⬜ |

## 账本文件登记（波次命名）

| 文件 | 负责 | 内容 |
| --- | --- | --- |
| `w1a.jsonl` | A1 | qingjian 全仓 |
| `w1b.jsonl` | A2 | 文本栈 |
| `w1c.jsonl` | A3 | Rust UI 框架 |
| `w1d.jsonl` | A4 | 2D 光栅器 |
| `w1e.jsonl` | A5 | gpui/WebRender/Skia |
| `w1f.jsonl` | A6 | 论文①文本 |
| `w2a..w2f.jsonl` | B1–B6（godot/flutter/chromium/qt-gtk/unity-unreal/apple-ms-android） | 大引擎与平台 |
| `w3a..w3e.jsonl` | C1–C5 | 论文②③④ + web 成本 + CJK |
| `w4a..w4d.jsonl` | D1/D2/D3/D4 | 案例 + bevy + 即时模式群 |
| `w5a.jsonl` | 主代理（A0/D6） | 渲染器文档自身条目（逐文件行锚） |

> 状态列由每批完成者更新；`ledger-stats.md` 是最终复算入口。
