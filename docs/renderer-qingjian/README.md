# 青简自绘渲染器（qingjian-render）文档集

> 这里记录的是 `qingjian-team/qingjian` 的 crate **`crates/qingjian-render`**（crate 自述：
> "自绘渲染器：把候选窗一帧画成位图，各平台只负责贴图"，`lib.rs:1`）的实现级技术文档。

## 这篇文档是什么 / 不是什么

- **是**：对**现状实现**逐文件、逐机制的技术说明——模块职责、数据流、文本管线、帧模型、
  光栅与混合、主题、平台贴图后端、成本结构——每条主张都带 `file:line` 证据锚（可对着上游源码复核）。
- **不是**：不是设计动机综述。上游自带的 [`docs/design/rendering.md`](https://github.com/qingjian-team/qingjian/blob/main/docs/design/rendering.md)
  （2026-09-13 选型记录：为什么自绘、为什么排除 Flutter/Avalonia/WebView、为什么不上 GPU）
  与 [`docs/design/candidate-ui.md`](https://github.com/qingjian-team/qingjian/blob/main/docs/design/candidate-ui.md)（候选窗交互与布局规则）
  是**设计档**；本文档集是**实现档**，与它们互补，遇冲突以源码为准。

## 版本锚

| 项 | 值 |
| --- | --- |
| 上游仓库 | https://github.com/qingjian-team/qingjian |
| 本文档复核的提交 | `c08ae57cb88b6a4a46f4a5e9c1d6d11c5e69222e`（2026-10-01 抓取） |
| crate | `crates/qingjian-render`（`src/` 38 个 `.rs` + `examples/preview.rs`） |
| 行数 | `src/` 约 2.9k 行；含预览例程共 3290 行（`wc -l` 复核） |
| 依赖 | `tiny-skia`（CPU 光栅）、`cosmic-text`（整形 + swash 栅格；上游 fork，见 `03`）、`thiserror`、`tracing`；Windows 另加 `windows`（DirectWrite） |
| 许可 | GPL-3.0-or-later（与上游一致） |

## 阅读顺序

| # | 文档 | 内容 |
| --- | --- | --- |
| — | `README.md`（本篇） | 定位、版本锚、阅读顺序 |
| 01 | [`01-overview.md`](01-overview.md) | 定位与总体数据流：一帧怎么从 Frame 变成窗口上的位图 |
| 02 | [`02-architecture.md`](02-architecture.md) | 模块地图（逐文件职责）、核心类型、两条渲染主流程 |
| 03 | [`03-text-pipeline.md`](03-text-pipeline.md) | 文本管线：整形、字形栅格、gamma、`trak` 字距、字体加载与回退 |
| 04 | [`04-layout-and-frames.md`](04-layout-and-frames.md) | 帧模型与三种排布（竖排 / 横排 / 矩阵）、顶行与光标 |
| 05 | [`05-raster-canvas.md`](05-raster-canvas.md) | 光栅与画布：tiny-skia 原语、混合、阴影、矢量图标、主题色 |
| 06 | [`06-theme.md`](06-theme.md) | 主题：字体规格、调色板、单位换算与调参依据 |
| 07 | [`07-platform-backends.md`](07-platform-backends.md) | 平台贴图后端：Windows（分层窗 / GDI 退路）、macOS（NSImage），及回退链 |
| 08 | [`08-performance.md`](08-performance.md) | 一帧成本结构与已公开读数（全部带锚）+ 测量方法 |
| 09 | [`09-assessment.md`](09-assessment.md) | 对标评估：可吸收 / 不吸收清单（由本仓大分析工程支撑） |

## 复算与证据

- 文档里的每个数字/结论都带锚。源码锚相对上游仓库（克隆到 `scratch/src/qingjian` 可复核）；
  设计档锚 = `docs/design/*.md:行`。
- 与本文档集对应的机器可查条目在 `../analysis/ledger/w5a.jsonl`（`tools/ledger.py verify` 校验）。
- 本仓与上游的关系：文档**按 GPL-3.0-or-later 发布、可直接回流上游**（引用源码片段逐字、带出处）。
