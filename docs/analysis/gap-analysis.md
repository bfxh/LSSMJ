# 缺口盘点（第三轮，2026-10-01）

> 回答一句话："还缺什么、缺的都在补什么、哪些判过不做"。**状态随轮次更新**；判"不做"的必须给一行理由。

## 一、已覆盖（截至第二轮收口）

| 面 | 覆盖 | 量 |
| --- | --- | --- |
| 交付主体 | `../renderer-qingjian/`（11 篇）+ `../renderer-gn/`（4 篇） | `w5a` 125 + `w4a` 62 |
| 引擎深读（源码级） | qingjian、tiny-skia/lyon/femtovg/vello/pathfinder/raqote、gpui/WebRender/Skia、Flutter+Impeller、Godot、Bevy、Qt Quick、GTK4/GSK、Rust UI 五框架、即时模式群（ImGui/Nuklear/microui/RmlUi/Fyrox/O3DE/Cocos） | `w1a..w1e,w2a..w2d,w4c,w4d` 合计 ≈1400 条 |
| 平台/闭源 | Chromium（文档轨）、Unity、Unreal Slate、Apple、微软 WPF、Android | `w2c,w2e,w2f` 290 条 |
| 论文轨道 | 文本渲染、GPU 技术、布局与增量、UI 系统与延迟、CJK、Web 成本 + `w3f` 基础批 | paper **224 条** / 29 篇述评与深读 |
| 案例 | Mineradio（Electron） | `w4b` 26 |
| 设计 | `../lssmj-design/README.md` + ADR（A–G） | — |

## 二、第三轮补缺（五轨已全部交付，2026-10-01 晚；各轨行数见 `targets.md` 第三轮表）

| # | 缺口（为什么算缺） | 轨 | 账本 |
| --- | --- | --- | --- |
| N1 | **商业游戏/应用 UI 中间件谱系**：闭源阵营（Noesis、Coherent GameFace、Ultralight、Scaleform 遗产、引擎内置 CEF 路线）此前只有路线传闻、没有文档级证据 | `../platforms/game-ui-middleware.md` | `w2g` |
| N2 | **C++ 光栅器第二组**：Blend2D（JIT/SIMD/多线程）与 ThorVG 是 CPU 光栅主线（设计 §4.1）的直接候选，第一组（w1d）没覆盖；NanoVG、Cairo 作对照 | `../engines/cpp-2d-rasterizers.md` | `w4e` |
| N3 | **Avalonia**：青简设计档把它列为"Avalonia 是排除 WebView 后最顺的现成框架"，但那段评估只有文档级；本轮做源码级复核（失效模型/脏区/文本栈/平台壳六条） | `../engines/avalonia.md` | `w4f` |
| N4 | **平台合成器**：DWM/UI.Composition、SurfaceFlinger、WindowServer、SwiftUI、WinUI3——此前 §7 只引到 DComp 一句，合成侧证据链不全 | `../platforms/compositors-and-system-ui.md` | `w3g` |
| N5 | **工程方法学（targets C6）**：qingjian-gates / unified-rx / BSHSQ 的门禁-棘轮-测量纪律 vs 本仓账本门，做一次三仓对照与可移植清单 | `../reports/12-engineering-method-crossref.md` | `w3h` |

## 三、判过不做（含理由，一行一条）

| 项 | 理由 |
| --- | --- |
| Noesis/Coherent 的反编译级分析 | 闭源产品 + 无授权；文档级（N1）已足以判定"许可红线与可吸收点" |
| Unreal / CryEngine / Lumberyard 源码主体 | Unreal 需 EULA 账号方可取源码；后两者已停且文档稀薄——其 UI 路线由 N1 的 CEF/Scaleform 谱系条目代表 |
| 主机平台（PS/Xbox/Switch）渲染路径 | 全部 NDA；公开资料≈零，强行写会造假 |
| TUI / 终端类 UI | 与"显示面位图/GPU"的形态目标无关 |
| "1000 篇论文"式全库扫描 | 篇级不可行且必稀释为浑水；论文轨道以**与设计决策相关**为轴（现 224 条 paper，逐条带 URL+引文） |
| `reports/02–08` 合成报告（原 D5 计划） | **决定不做**：各 `engines/`、`papers/` 报告已含 TL;DR+可吸收/不可吸收表，重复合成只增账面；以 `11-papers-bibliography.md`（机器生成）与 `01-engine-landscape.md` 作索引 |
| 浏览器四件套（Chrome/Edge/Firefox/Safari 逐版本对比） | 合成层结论已由 Chromium 文档轨（w2c）代表；WebKit/Gecko 差异不改变本设计决策 |

## 四、复算与更新

- 账本门：`python tools/ledger.py verify --min 1000`（当前值见 `ledger-stats.md`，机器生成）。
- 本盘点写于第三轮发射时；收口后由主代理更新"已覆盖"行（N1–N5 结果并入）。
