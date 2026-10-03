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

## 三、第四轮补缺（纹理/光追/光照/资产与动画/可见性——用户点名，正在跑）

| # | 缺口（为什么算缺） | 轨 | 账本 |
| --- | --- | --- | --- |
| R1 | **纹理系统**：压缩（BCn/ASTC/ETC/Basis）、虚拟纹理（SVT/megatexture/tiled resources）、mipmap/各向异性过滤、流送与预算、bindless——前三轮只在"图集"层面擦边 | `../papers/texture-systems.md` | `w6a` |
| R2 | **实时光追与降噪**：Ray Tracing Gems、ReSTIR、SVGF/NRD、DXR/KHR 规范、混合渲染；并核验 GN SDK"光追全局光照"声称对应什么技术面 | `../papers/raytracing.md` | `w6b` |
| R3 | **光照、材质与色彩管理**：PBR 谱系（Disney/GGX/Karis）、IBL/SH、GI（Lumen/DDGI/LPV/烘焙）、阴影（CSM/VSM）、tonemap（ACES）、色彩管理 | `../papers/lighting-and-materials.md` | `w6c` |
| R4 | **资产/动画/着色器管线**：glTF 2.0、USD/FBX 现状、几何压缩（Draco/meshopt）、动画压缩（ACL）、状态机、WGSL/SPIR-V/naga、管线缓存 | `../reports/13-asset-animation-pipeline.md` | `w6d` |
| R5 | **可见性、剔除与场景组织**：视锥/遮挡剔除（硬件查询/CHC/Umbra）、BVH/松散八叉树、GPU 驱动剔除、HLOD、可见性缓冲 | `../papers/visibility-and-culling.md` | `w6e` |

> 设计侧回写计划：`lssmj-design/` 将增 **"场景档"补充设计**（`02-scene-tier.md`：纹理/光照/RT/可见性/资产在 P5+ 的取舍与判据），
> 让"整个渲染引擎"不只 UI 档——本轮结果落盘后由主代理写。

## 三·五、第五轮（缺口收尾 + 四个新扫面，2026-10-02）

- **登记缺口收尾**：各向异性+IBL+PBRT 深挖（`w6h` 13）· RTG/降噪书目（`w6j` 8）· 平台视频 API（`w6k` 6）——全部 ✅。
- **四个新扫面**：GPU 测量工具链（`w7b` **72** ✅）· 后处理与 AA（`w7a` **160** ✅）· 场景内容系统（`w7c` **85** ✅）· 字形工程（`w7d` **87** ✅）。
- **第五轮合计 +431 条**（账本总 3431）；决策回写在 ADR J1–J9。
- **新固化方法**：PDF 抽取（pypdf）与 Apple 文档 JSON 通道（两条抓取路径写进报告）。
- **第六轮（G-B 三渲二/NPR）**：`w8a` **109** ✅（共核结论与 C11–C13 锚已回填 Master Plan §3；T-NPR-11 光照双模实验立项）。
- **第七轮（压缩/电影感/前端迁移）**：`w8b` **137** + `w8c` **108** + `w8d` **90** ✅——压缩"降质即不可用"判定表
  （不可用 13 项含神经压缩）、电影感效果链 E1–E13、前端引导选"中档"（均入 ADR K1–K7）。
- **第八轮（P0 实时光影 + P1 组件层）**：`w9a` **128** + `w9b` **74** + `w9c` **84** ✅——
  实时光影主链（阴影缓存优先/多光源 clustered/GTAO/DDGI·SHaRC/RT 逐光可选，wgpu RT=实验特性边界）、
  组件层五段蓝图（AccessKit 唯一 a11y schema）、复杂组件最小清单（虚拟列表/停靠/命令/缓冲）。
  本轮子代理在报告阶段遇 API 余额中断，账本完整、两份报告由主代理补全（见 `targets.md` 第八轮留档）。
- **第九轮（角色动画运行时 + 色彩管理与 HDR 显示链）**：`w10a` **81** + `w10b` **64** ✅——
  动画六层（蒙皮公式/混合图 DAG/状态机/FABRIK/根运动分量/表情与弹簧骨）与色彩管理三段链
  （内容空间→线性 FP16 合成→显示空间；ICC D50 PCS / Bradford / PQ·HLG / wgpu SurfaceColorSpace 八档），
  新增判据 C18（色彩一致）。两处空白由英文关键词扫描发现（skeletal/skinning/blend tree/ICC/wide gamut 等命中≈0）。
- **第十轮（输入系统与事件路由）**：`w11a` **60** ✅——统一指针模型（id 生命周期/取消路径/primary）、
  事件合并与原子报告（accumulate/SYN_REPORT/设备时间戳/单次投递）、命中测试与指针捕获、设备面
  （笔轴·手柄标准布局·多设备多指）、延迟补偿（coalesced 默认 + predicted 可开关；10 篇论文）。
  新增判据 C19（输入一致）。**范围裁决**：物理/音频/网络/国际化架构/编辑器工具链/视频内容链不在本项目范围。
- **仍然开放的缺口（自愿续跑，编号段空闲）**：Karis/Frostbite/Hoffman course notes 全文（PDF 截断，`w7e`）；
  RTG II 全文与 ReSTIR PDF（`w7e`）；MediaCodec/FFmpeg 文档正文（`w7f`）；Noesis 官网（403）；
  米哈游角色 shader 官方一手（仅社区还原，w8a 已标注）；Marschner/X-Toon 正文 PDF（仅摘要）；
  AFBC/Apple memoryless render target（文档未达，`w8b` 留档）；SMPTE RDD5/Kolb 1995/ARRI White Paper（付费/403，`w8c` 留档）；
  Android 宽色域/HDR 官方文档（本机网络超时，`w10b` 留档）；Skia 官网文档（超时；已用仓库内 color.md 替代）；
  BT.2100 正文 PDF（ITU 直链 404，只取到条目页）与 SMPTE ST 2084（付费）；
  场景档 P5 前的必读清单见 `papers/rtg-and-denoisers.md` 末节。

> **范围裁决（用户 2026-10-03）**：本项目 = **渲染引擎 + 前端范式**。**输入系统**纳入分析（第十轮执行）；
> **物理/碰撞/刚体**由用户侧实现，本项目只保留接缝（变换/可见性/根运动），不分析、不设计；
> 音频引擎、网络/多人、通用国际化架构、编辑器/工具链产品、通用资源/内存分配器、视频内容链——
> 一并从"待补缺口"移入"判过不做（范围外）"。下面第四节同步更新。

## 四、判过不做（含理由，一行一条）

| 项 | 理由 |
| --- | --- |
| Noesis/Coherent 的反编译级分析 | 闭源产品 + 无授权；文档级（N1）已足以判定"许可红线与可吸收点" |
| Unreal / CryEngine / Lumberyard 源码主体 | Unreal 需 EULA 账号方可取源码；后两者已停且文档稀薄——其 UI 路线由 N1 的 CEF/Scaleform 谱系条目代表 |
| 主机平台（PS/Xbox/Switch）渲染路径 | 全部 NDA；公开资料≈零，强行写会造假 |
| TUI / 终端类 UI | 与"显示面位图/GPU"的形态目标无关 |
| "1000 篇论文"式全库扫描 | 篇级不可行且必稀释为浑水；论文轨道以**与设计决策相关**为轴（现 224 条 paper，逐条带 URL+引文） |
| `reports/02–08` 合成报告（原 D5 计划） | **决定不做**：各 `engines/`、`papers/` 报告已含 TL;DR+可吸收/不可吸收表，重复合成只增账面；以 `11-papers-bibliography.md`（机器生成）与 `01-engine-landscape.md` 作索引 |
| 浏览器四件套（Chrome/Edge/Firefox/Safari 逐版本对比） | 合成层结论已由 Chromium 文档轨（w2c）代表；WebKit/Gecko 差异不改变本设计决策 |
| 物理/碰撞/刚体（含物理引擎选型） | **范围外**（用户 2026-10-03 口径）：用户侧已有实现；本项目只保留渲染/动画接缝（变换/可见性/根运动） |
| 音频引擎、网络/多人、通用国际化架构、编辑器/工具链产品、通用资源/内存分配器、视频内容链 | **范围外**（同一口径）：与"渲染引擎 + 前端范式"的交付边界无关；平台解码能力已在 `reports/15` 记录 |

## 四、复算与更新

- 账本门：`python tools/ledger.py verify --min 1000`（当前值见 `ledger-stats.md`，机器生成）。
- 本盘点写于第三轮发射时；收口后由主代理更新"已覆盖"行（N1–N5 结果并入）。
