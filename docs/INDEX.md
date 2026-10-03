# LSSMJ 总导航

> 见根的 [README.md](../README.md) 的口径声明。本页给"有什么、在哪、什么状态"。

## 交付主体（先看这个）

| 文件 | 内容 | 状态 |
| --- | --- | --- |
| `lssmj-design/00-master-plan.md` | ★ **总体设计文档（Master Plan）**：双终局目标（真实画质 / 三渲二）、判据全景 C1–C13、架构总览、风险表 | ✅ v1 |
| `lssmj-design/03-task-backlog.md` | ★ **任务列表**：五轨任务表 + 里程碑 M1–M4 | ✅ |
| `lssmj-design/04-ci-and-gates.md` | ★ **CI 流程与门禁**：本仓 A0–A5 + 引擎仓目标态 + YAML | ✅ |

| 文件 | 内容 | 状态 |
| --- | --- | --- |
| `renderer-qingjian/README.md` | 青简自绘渲染器文档：导读与阅读顺序 | ✅ |
| `renderer-qingjian/01-overview.md` | 定位与总体数据流（为什么自绘 / 一帧怎么走） | ✅ |
| `renderer-qingjian/02-architecture.md` | 模块地图（38 文件逐个职责 + 依赖方向） | ✅ |
| `renderer-qingjian/03-text-pipeline.md` | 文本管线（cosmic-text/swash + 平台字体后端） | ✅ |
| `renderer-qingjian/04-layout-and-frames.md` | 帧模型与布局（行/横竖排/状态行/顶行/预编辑） | ✅ |
| `renderer-qingjian/05-raster-canvas.md` | 光栅与画布（tiny-skia / 色彩 / 阴影 / 云雾 / 齿轮） | ✅ |
| `renderer-qingjian/06-theme.md` | 主题与配色（字体规格 / 调色板） | ✅ |
| `renderer-qingjian/07-platform-backends.md` | 平台后端（Windows DirectWrite + 分层窗；macOS 位图；Linux） | ✅ |
| `renderer-qingjian/08-performance.md` | 一帧成本结构与测量方法 | ✅ |
| `renderer-qingjian/09-assessment.md` | 对标评估（可吸收清单随波次补全） | 🟡 骨架已就绪，待大分析回填 |
| `renderer-qingjian/10-glossary.md` | 术语表 | ✅ |

## 分析工程

| 入口 | 内容 |
| --- | --- |
| `analysis/METHOD.md` | 协议：深度阶梯 / 证据锚纪律 / 复算方法 |
| `analysis/gap-analysis.md` | 缺口盘点（已覆盖 / 本轮补缺 / 判过不做及理由） |
| `analysis/AGENT-BRIEF.md` | 分析代理作业简报 |
| `analysis/targets.md` | 目标总清单（含行数目标与账本文件登记） |
| `analysis/targets-lock.json` | url + commit + 本地路径（机器可读） |
| `analysis/ledger-stats.md` | 账本统计（机器生成；含 rejected 计数） |
| `engines/**` | 逐引擎报告 |
| `papers/**` | 论文/文章分析与述评 |
| `platforms/**` | 闭源与平台 UI |
| `targets/**` | 案例解剖（GN SDK / Mineradio） |
| `reports/**` | 综合报告与 ADR |

## 副档与综合

| 入口 | 内容 |
| --- | --- |
| `renderer-gn/**` | GN SDK（闭源）渲染/UI 逆向文档（含复算与未验证清单） |
| `targets/mineradio-electron.md` | Mineradio（Electron 42）案例解剖与成本账 |
| `lssmj-design/README.md` | ★ **自研设计文档（第一层：显示面 UI）**：判据 C1–C6、架构、渲染核心、文本清单、预算、路线图 |
| `lssmj-design/02-scene-tier.md` | ★ **第二层设计（场景/3D 档）**：几何/纹理/光/可见性/资产/API/视频的取舍与判据 C7–C10 |
| `reports/00-overview.md` | 总览与口径 |
| `reports/01-engine-landscape.md` | 全景矩阵（含"未完成 → 二轮补齐"与**口径修正**：taffy/Tessera/整形栈/亚像素） |
| `reports/05-web-js-wrapper-costs.md` | Web 栈与 JS 包装成本（JFB 实抄数字、Tauri vs Electron） |
| `reports/06-cjk-text-and-ime.md` | 中文/东亚排版与输入法 UI |
| `reports/09-performance-methodology.md` | 性能方法论（先打点再改/单点≠稳态/交错 A/B/金丝雀/棘轮） |
| `reports/10-decisions-adr.md` | 决策记录（学谁/弃谁/否证条件）+ 二轮补充 |
| `papers/**` | 论文/长文述评（text-rendering / gpu-rendering-techniques / layout-and-incremental / ui-systems-and-latency / texture-systems / raytracing / lighting-and-materials / visibility-and-culling + 30 余篇深读） |
| `reports/11-papers-bibliography.md` | 论文/来源目录学（机器生成） |
| `reports/12-engineering-method-crossref.md` | 工程方法学三仓对照（18 模式 + P1–P12 可移植清单） |
| `reports/13-asset-animation-pipeline.md` | 资产/动画/着色器管线（glTF/ACL/DRACO/KTX2/WGSL） |
| `reports/14-graphics-apis.md` | 图形 API（D3D11/12+Work Graphs、Vulkan、GL、Metal、WebGPU/wgpu；选型结论） |
| `reports/15-video-pipeline.md` | 视频管线（DXVA/NVDEC/Chromium/GN cVIDEO 契约；边界三层 + 平台解码 API 附表） |
| `reports/16-gpu-tooling.md` | GPU 测量与工具链（抓帧≠计时/HAGS 精度/时间戳链/呈现模式；判据不押停更工具） |
| `reports/17-font-engineering.md` | 字形工程（OpenType 表/Variations/彩色字体四格式/回退算法；对青简文本栈 10 条可吸收） |
| `reports/18-render-compression.md` | 渲染压缩（48 行判定表：可用/有界/不可用；Rust 工具链逐条；神经压缩判死有硬证据） |
| `reports/19-frontend-migration.md` | 前端语言迁移引导（DSL/CSS/JS 引擎/Electron 映射；选型=中档） |
| `reports/20-ui-component-core.md` | UI 组件体系核心（事件/焦点/无障碍/令牌/失效边界；轨 9 锚，w9b 74 条） |
| `reports/21-complex-widgets-and-docking.md` | 复杂组件与工作区（虚拟列表/停靠/模态/DnD/滚动物理/编辑器文本；轨 10 锚，w9c 84 条） |
| `reports/22-color-management-and-hdr-display.md` | 色彩管理与 HDR 显示链（ICC/PCS/色域映射/PQ·HLG/平台色彩管理/wgpu 输出色彩空间；w10b 64 条） |
| `reports/23-input-system-and-routing.md` | 输入系统与事件路由（统一指针模型/捕获/命中/合并/设备/延迟；w11a 60 条） |
| `reports/24-render-resource-and-vram-budget.md` | 渲染资源与显存预算（分配器/驻留/生命周期/上传回读/淘汰；w12a 54 条） |
| `reports/25-display-topology-dpi-and-windowing.md` | 显示拓扑、DPI 缩放与窗口合成（多显示器/逐屏 DPI/可用区/交换链/VRR；w13a 49 条） |
| `reports/26-supplementary-anchors.md` | 补锚批（AccessKit 适配器/Android/Raw Input/D3D12/wgpu/Metal/Vulkan 色彩空间；w14a 29 条） |
| `papers/anime-film-look.md` | 泡沫时期科幻动漫画质（效果链 E1–E13；显示域施加=规范级共识） |
| `papers/real-time-lighting-deep.md` | 实时光影主链（阴影/多光源/AO/GI/混合；P0 游戏向，w9a 128 条） |
| `papers/animation-runtime.md` | 角色动画运行时（蒙皮/混合图/状态机/IK/根运动/表情与次级运动；w10a 81 条） |
| `platforms/game-ui-middleware.md` | 游戏/应用商业 UI 中间件谱系（Noesis/Coherent/Ultralight/Scaleform 遗产/CEF 路线；许可红线） |
| `platforms/compositors-and-system-ui.md` | 平台合成器与闭源系统 UI（DWM/UI.Composition/SurfaceFlinger/render server/SwiftUI/WinUI3） |
| `engines/cpp-2d-rasterizers.md` | C++ 光栅器第二组（Blend2D/ThorVG/NanoVG/Cairo；ThorVG 脏区范式） |
| `engines/avalonia.md` | Avalonia 源码级复核（对青简六条评估的逐条回执） |
