# LSSMJ 总导航

> 见根的 [README.md](../README.md) 的口径声明。本页给"有什么、在哪、什么状态"。

## 交付主体（先看这个）

| 文件 | 内容 | 状态 |
| --- | --- | --- |
| `renderer-qingjian/README.md` | 青简自绘渲染器文档：导读与阅读顺序 | ⬜ |
| `renderer-qingjian/01-overview.md` | 定位与总体数据流（为什么自绘 / 一帧怎么走） | ⬜ |
| `renderer-qingjian/02-architecture.md` | 模块地图（21 个源文件逐个职责 + 依赖方向） | ⬜ |
| `renderer-qingjian/03-text-pipeline.md` | 文本管线（cosmic-text/swash + 平台字体后端） | ⬜ |
| `renderer-qingjian/04-layout-and-frames.md` | 帧模型与布局（行/横竖排/状态行/顶行/预编辑） | ⬜ |
| `renderer-qingjian/05-raster-canvas.md` | 光栅与画布（tiny-skia / 色彩 / 阴影 / 云雾 / 齿轮） | ⬜ |
| `renderer-qingjian/06-theme.md` | 主题与配色（字体规格 / 调色板） | ⬜ |
| `renderer-qingjian/07-platform-backends.md` | 平台后端（Windows DirectWrite + 分层窗；macOS 位图；Linux） | ⬜ |
| `renderer-qingjian/08-performance.md` | 一帧成本结构与测量方法 | ⬜ |
| `renderer-qingjian/09-assessment.md` | 对标评估（由大分析支撑的吸收/不吸收清单） | ⬜ |

## 分析工程

| 入口 | 内容 |
| --- | --- |
| `analysis/METHOD.md` | 协议：深度阶梯 / 证据锚纪律 / 复算方法 |
| `analysis/AGENT-BRIEF.md` | 分析代理作业简报 |
| `analysis/targets.md` | 目标总清单（含行数目标与账本文件登记） |
| `analysis/targets-lock.json` | url + commit + 本地路径（机器可读） |
| `analysis/ledger-stats.md` | 账本统计（机器生成；含 rejected 计数） |
| `engines/**` | 逐引擎报告 |
| `papers/**` | 论文/文章分析与述评 |
| `platforms/**` | 闭源与平台 UI |
| `targets/**` | 案例解剖（GN SDK / Mineradio） |
| `reports/**` | 综合报告与 ADR |

## 副档

| 入口 | 内容 |
| --- | --- |
| `renderer-gn/**` | GN SDK（闭源）渲染/UI 逆向文档 |
| `reports/10-decisions-adr.md` | 设计决策记录（每条带证据） |
