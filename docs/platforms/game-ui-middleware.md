# 游戏/应用商业 UI 中间件技术谱系（6 对象，抓取 2026-10-01）

> **上锚说明**：本报告**无源码主体**（任务规定 doc/web 深度为主；不克隆、不读源码）。全部主张对应
> `docs/analysis/ledger/w2g.jsonl` 的 `W2G-0xx` 行；引文逐字抽自抓取缓存
> `D:/KF/LSSMJ/scratch/fetch/w2g/`（成功落盘 65 个抓取物 = 62 页面 + 3 sitemap；另有约 12 次
> 403/404/DNS 探针未落盘）。**版本锚**：Coherent 文档 `Version:3.1.3.3`（W2G-019）、
> Ultralight API 1.4.0、Noesis NuGet 3.2.12（W2G-007）、CRYENGINE 5.7 LTS 文档、
> Rive Renderer 公告 2024-03-19（W2G-086）、O3DE 文档 2026 版（CC BY 4.0）。
> **未获得**：NoesisGUI 官网（403）、autodesk.com（403）、Wikipedia/archive.org（本机不可达）——
> 相关结论一律降级为「未验证」，见 §6。

## TL;DR（每条带锚）

1. **三家独立实现共用的渲染契约是「画进宿主给的纹理/缓冲，空帧早退」**：Cohtml 用
   `ViewRenderer::SetRenderTarget` 画进用户纹理并**只更新变化部分**（`W2G-012`/`W2G-013`）；
   Ultralight 明列 CPU→像素缓冲（Surface API）与 GPU→纹理（GPUDriver API）两路（`W2G-049`）；
   CEF 自述用例即「离屏渲染给自有绘制框架」（`W2G-069`）。**这与我们「壳只贴图 + 渲染器出位图」
   同构，且是三家独立实现的共识**。
2. **命令录制 / 回放分离**：Cohtml 异步录制、渲染线程 `Paint()` 时执行、无变化立即返回
   （`W2G-014`）；Ultralight 在 `Renderer::Render()` 里排队命令（`UpdateCommandList`）由宿主提交
   （`W2G-050`）——**两条独立实现都选「命令队列 + 宿主提交点」**，我们的 GPU 档可按此定契约。
3. **商业中间件把「可关可测」当交付面**：Gameface 自述 Whitebox（渲染/任务/日志/内存/资源都可接管，
   `W2G-011` 所在页）、CVar `sys_flash` 开关与剖析（`W2G-061`）、图集工具与图集使用三步
   （`W2G-027`）。**我们 P1 的开关/计数器应达到同等可观测性**。
4. **布局语义是「curated subset」而非整浏览器**：Gameface 让所有元素行为等同 `display:flex` 列方向
   （`W2G-031`）、明确列出不支持项（`flex-basis: content`，`W2G-032`）；Ultralight 自述无
   WebGL/WebRTC/HTML5 音视频（`W2G-042`）。**能力边界写成文档是行业做法**。
5. **文本栈两支**：Gameface 默认 FreeType + 单通道 SDF、GPOS 表走 HarfBuzz（`W2G-020`/`W2G-021`/
   `W2G-022`）；Rive 用解析式抗锯齿声称任意字号清晰（`W2G-085`）。**SDF 与「覆盖率位图」是两条路线，
   与我们「候选窗不用 SDF」（青简评估）不冲突但适用面不同**。
6. **IME 是商业件的必备能力**：Gameface 有完整 IME 章节与三态 API（`IMESetComposition/
   IMEConfirmComposition/IMECancelComposition`，`W2G-024`/`W2G-025`），并承认「IME 是 OS 特性但库
   必须提供方法」（`W2G-026`）。**候选窗/输入类 UI 的 IME 契约不能被推给平台壳**。
7. **引擎内置浏览器路线已被验证，但任务点名的对象未命中**：Unreal 有 WebBrowser 模块（Slate
   viewport，`W2G-072`）+ UMG `UWebBrowser` 控件（`W2G-073`）；而 O3DE 的工具 UI 是 **Qt**
   （`W2G-079`/`W2G-080`），仓库检索无 Chromium/CEF 路径（`W2G-081`）——**「O3DE 前身内置 CEF」
   在本批证据下未获证实（记为否证候选）**；UE=CEF3 的具体归属本轮未拿到原文锚（`W2G-074`）。
8. **Scaleform 是「活着的遗产」**：CRYENGINE 5.7 LTS 仍同时支持 Scaleform 3/4（`W2G-056`），
   AS2/AS3 资产经 GFx 管线转换（`W2G-057`/`W2G-059`），运行期以 `IFlashPlayer` 接口 +
   `sys_flash` CVar 控制（`W2G-060`/`W2G-061`）；**但作者工具链绑定 Flash/GFxExport.exe（`W2G-063`），
   导出默认还踩 TGA 坑（`W2G-062`）**。CRYENGINE 5.0–5.6.7 已于 2022-05 从 GitHub 下架（`W2G-065`）。
9. **许可结论（对本项目 GPL-3.0-or-later 工程，逐条）**：Coherent = per-title/per-platform 商业授权
   （`W2G-035`）、源码是付费加项（`W2G-036`）；Ultralight = 免费档**仅个人且年收入<100K、仅应用用途**
   （`W2G-052`），Pro $3,000/年/应用（`W2G-051`），条款**禁止修改/衍生**（`W2G-053`）、完整源码专有
   （`W2G-041`）；NoesisGUI = 运行时闭源二进制、需从门户下载 Native SDK（`W2G-005`）；CEF = BSD
   （`W2G-068`，但引入整棵 Chromium）；Qt WebEngine = 基于 Chromium 并有逐项许可清单（`W2G-075`/
   `W2G-078`）；**Rive 运行时 = MIT（`W2G-082`），是本批唯一可直接用于 GPL 工程的运行时**。
10. **代价账（不是技术账）**：Ultralight 自述内存比 Chromium 轻近 10×（`W2G-039`）、Rive 自述
    120fps（blog 2024-03-19）、Coherent 自述亚毫秒/帧（`W2G-034`）——**三条都没有机器/场景/口径，
    一律按「不可复算」记账，不进我们的预算表**。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 「画进宿主纹理 + 只更新变化部分」 | `W2G-012`/`W2G-013` | 吸收益处：与 §4.3 damage 设计同构，且是三家独立共识 |
| 命令异步录制 + 无变化立即返回 | `W2G-014`/`W2G-050` | 吸收：我们 GPU 档契约按「队列 + 宿主提交点」写；CPU 档借同一语义 |
| 库不私自开线程，工作回调交宿主执行 | `W2G-016`/`W2G-018` | 吸收：线程策略归宿主，便于测量与工程控制 |
| 输入事件可被宿主吞掉/放行（userData） | `W2G-033` | 有界吸收：我们用 hit_table 的「命中/未命中」表达同一语义 |
| IME 三态 + 组合窗口定位需渲染器回传 | `W2G-024`/`W2G-025`/`W2G-026` | 吸收：preedit 生命周期与光标矩形回传是必做项 |
| 图集离线工具（默认 2048、输出 JSON 映射、预乘转换） | `W2G-027`/`W2G-028` | 有界吸收：P2 图集档时参考；默认值要选「默认即最优」 |
| 「几何式 AA」GPU 档方案 | `W2G-029` | 有界吸收：GPU 档候选路线之一（对照我们 CPU 2bit 超采样） |
| 解析式抗锯齿（Rive Renderer） | `W2G-085`/`W2G-086` | 不吸收（现在）：P3 评估项，需先测字形缓存/子像素成本 |
| 抽象 Renderer 接口 + 多后端 | `W2G-083`/`W2G-084` | 吸收（结构）：保持「显示列表→执行器」边界，外接实现成本低 |
| 多后端导致特性碎片化的教训 | `W2G-087` | 吸收（反向）：GPU 档若多后端，特性须按后端显式降级并报错 |
| 整数坐标吸附（GPU 像素网格） | `W2G-030` | 有界吸收：仅在 GPU 档启用，且要把动画抖动/色渗写进测试 |
| Chrome DevTools 式调试面（Gameface） | （`W2G-011` 所在 overview 页） | 不吸收：我们不做 DOM；但「可内省的面」思路对应我们的出帧计数器 |
| 引擎工具 UI 走 Qt（O3DE） | `W2G-079`/`W2G-080` | 不吸收：我们自建最小组件集，规模不需要 Qt |
| 引入 CEF/Qt WebEngine 整棵 Chromium | `W2G-068`/`W2G-075`/`W2G-076` | 不吸收：与「小窗位图 + GPL + Rust」三条约束冲突（进程模型、体量、依赖） |
| NoesisGUI 的 XAML/WPF 语义与 MVVM 绑定 | `W2G-001`/`W2G-002` | 不吸收（语言层）：绑定语义与 Rust 所有权模型不同族；Frame 模型更贴近我们 |
| NoesisGUI/Coherent/Ultralight 运行时本体 | `W2G-005`/`W2G-036`/`W2G-041` | **不吸收（许可红线）**：闭源/商业授权，GPL 工程不可链接 |
| Rive 运行时（MIT）与 .riv 资产格式 | `W2G-082`/`W2G-088` | 有界吸收：许可可用；若将来要矢量动画资产档，它是候选格式 |

## 1. 谱系表（对象 × 六个维度）

| 对象 | 渲染路径 | 布局/文本 | 输入与 IME | 分发与许可 | 状态（2026-10-01 观察） |
| --- | --- | --- | --- | --- | --- |
| **NoesisGUI** | 自绘矢量 UI（XAML 语义），有 Unreal/Unity 集成层 | XAML/WPF 语义 + 数据绑定（`W2G-001`/`W2G-002`） | 未获得官网文档（403，`W2G-009`） | 运行时闭源二进制，门户下载（`W2G-005`）；集成层源码公开（`W2G-004`）；NuGet 3.2.12、版权自 2013（`W2G-007`/`W2G-008`） | 现役；官网 403，仅官方 GitHub 组织材料可读 |
| **Coherent Gameface** | **Cohtml（HTML 引擎）+ Renoir（自研 GPU 渲染库）**（`W2G-011`） | 全元素 flex 语义（`W2G-031`）；FreeType+SDF、GPOS→HarfBuzz（`W2G-020`/`W2G-021`） | 完整 IME 三态 API（`W2G-024`/`W2G-025`）；事件可拦截（`W2G-033`） | per-title/per-platform 授权（`W2G-035`）；源码=付费加项（`W2G-036`） | 现役；文档版本 3.1.3.3（`W2G-019`） |
| **Ultralight** | WebKit/Safari 自维护 fork；CPU（Skia 系 Surface）与 GPU（GPUDriver）双路（`W2G-037`/`W2G-049`） | 自建 WebCore 布局（`W2G-045`）；无 WebGL/WebRTC/音视频（`W2G-042`） | FileSystem/FontLoader 为必选平台处理器（`W2G-049`） | 免费档限个人/年收入<100K/仅应用（`W2G-052`）；Pro $3k/年/应用（`W2G-051`）；禁止修改衍生（`W2G-053`） | 现役；API 1.4.x |
| **Scaleform（遗产）+ CRYENGINE** | GFx 外部中间件（`.gfx` 预烘焙），`IFlashPlayer` 接口（`W2G-055`/`W2G-059`/`W2G-060`） | Flash/Animate 作者工具 + GFxExport 管线（`W2G-057`/`W2G-063`） | FSCommand 双向脚本桥（`W2G-060` 行内） | 第三方 SDK（Autodesk），引擎侧需授权（见 §6 未验证） | **已停用候选**：CRYENGINE 5.7 LTS 仍支持 SF3/4（`W2G-056`）；5.0–5.6.7 已下架（`W2G-065`） |
| **引擎内置浏览器（CEF 路线）** | CEF = 嵌 Chromium 的框架，含离屏渲染用例（`W2G-068`/`W2G-069`） | 浏览器语义（完整 Web） | 浏览器语义 | BSD（`W2G-068`）；Qt WebEngine 同族（`W2G-075`） | UE：WebBrowser 模块 + UMG 控件（`W2G-072`/`W2G-073`）；O3DE：**未采用，工具 UI=Qt**（`W2G-079`/`W2G-081`） |
| **Rive（可选）** | 自研 GPU 渲染器（矢量三角 patch）+ 抽象 Renderer 外接（`W2G-083`/`W2G-086`） | 编辑器内建布局/动画/状态机（`W2G-089`） | 未在本批证据中覆盖 | **运行时 MIT**（`W2G-082`） | 现役；Renderer 2024-03 开源（`W2G-086`） |

## 2. 关键机制拆解

### 2.1 三条技术路线（不是一条）

1. **自建 HTML/CSS 引擎 + 自研 GPU 渲染库**（Coherent Gameface）：Cohtml 做布局与逻辑，Renoir
   做绘制，画进宿主纹理（`W2G-011`/`W2G-012`）。它连 `flex-basis: content` 都不支持
   （`W2G-032`）——**HTML 兼容性是「选择实现的子集」，不是「跟着 W3C 走」**。
2. **自维护浏览器内核 fork**（Ultralight）：GPU 优先的 WebKit 轻量 fork（`W2G-038`），团队前作
   Awesomium（约 2008，首个游戏/桌面 HTML 引擎）（`W2G-043`）；模块分层 AppCore/Ultralight/WebCore/
   JavaScriptCore/UltralightCore（`W2G-044`），WebCore 负责解析→样式→布局→paint 节点树
   （`W2G-045`）。它与 Chromium 的对比卖点是「轻」（`W2G-039`，无口径）。
3. **嵌整棵 Chromium**（CEF / Qt WebEngine）：CEF 自述用途第一条就是「给自有绘制框架做离屏渲染」
   （`W2G-069`），另列「轻量原生壳 + Web UI」（`W2G-070`）并自述装量超 1 亿（`W2G-071`）；Qt WebEngine 明说核心基于 Chromium 且把渲染/JS 拆到独立进程（`W2G-075`/
   `W2G-076`），渲染本身走 GPU + RHI 抽象（`W2G-077`）。**代价是体量与进程模型**，收益是完整 Web 语义与生态。

### 2.2 渲染契约（三家共识，可直接抄）

- **宿主提供目标纹理**：`SetRenderTarget`（`W2G-012`）；Ultralight 的 RenderTarget/View 加速开关
  （`W2G-049` 同页）。
- **只画变化部分**：`incremental rendering`（`W2G-013`）、批处理 + 只重画变化（`W2G-017`）。
- **命令队列**：异步录制 → 渲染线程执行 → 空帧立即返回（`W2G-014`）；GPUDriver 命令排队由宿主
  提交（`W2G-050`）。
- **极小 shader 面**：Ultralight 只有 `Fill` 与 `FillPath` 两种 shader 覆盖边框/圆角/阴影/渐变
  （`W2G-047`）——**GPU 档先做通用填充路径、构建期预编译才有意义**（与 Impeller 结论互证）。
- **渲染器可替换**：Ultralight 明说 renderer-agnostic、绘制全走 GPUDriver 接口（`W2G-046`）；
  Cohtml 的 View/ViewRenderer 按线程分工（UI 线程控制页面、渲染线程只画）（`W2G-015`）。
- **混合公式必须显式且一致**：Ultralight 自述自定义混合模式，宿主需用同样函数才能颜色一致
  （`W2G-048`）——等同我们位图契约里的预乘 RGBA 纪律。

### 2.3 布局与文本

- 布局：Cohtml 强制 flex 列语义（`W2G-031`）、明确不支持项（`W2G-032`）；Ultralight 布局在 WebCore
  内、以 paint 树为输出（`W2G-045`）。
- 文本：默认 FreeType + **单通道 SDF**（`W2G-020`/`W2G-022`）；含 GPOS 表时用 HarfBuzz 处理
  kerning（`W2G-021`）；1.13 起支持小数像素定位（`W2G-023`，我们暂列待测）；另有 MSDF/Bitmap 字体
  自定义装载路径（`W2G-020` 所在页）。
- **对我们**：我们的三件套（opsz 分键 / trak / gamma）与「按表项切引擎」是同一思想
  （显式规则 > 默认值）；SDF vs 覆盖率位图是分档取舍，不是对错。

### 2.4 输入与 IME

- 事件可被条件性截留（`void* userData`，`W2G-033`）。
- IME：三态 API + 组合下划线提示 + 组合窗口定位事件（`W2G-024`/`W2G-025`），官方承认库必须提供方法
  （`W2G-026`）。**候选窗/输入类 UI 若不回传光标矩形，IME 窗口就会错位**——与我们壳契约的
  `content_rect`/`hit_table` 是同一条要求。

### 2.5 工具链（这类栈的隐性成本都在这里）

图集工具（命令行、默认 2048、JSON 映射、非预乘→预乘，`W2G-027`/`W2G-028`）；Scaleform 的
GFxExport.exe 双版本（SF3/SF4 各一套，`W2G-063`）；MSDF 字体用 msdf-atlas-gen 生成（`W2G-020`
所在页）。**共同点：作者工具与运行时代一起交付，工具链版本与运行时版本强绑定。**

## 3. 性能主张与读数（全部标注口径）

| 主张 | 出处（锚） | 口径 | 判定 |
| --- | --- | --- | --- |
| 「UI 每帧亚毫秒」 | Coherent 产品页（`W2G-034`） | 无机器/场景/分辨率 | **不可复算**：仅营销旁证 |
| 「内存比 Chromium 轻近 10×」 | Ultralight 首页（`W2G-039`） | 无测法/样本 | **不可复算** |
| 「120 fps、per（矢量）动画」 | Rive blog 2024-03-19 | 无机器/场景 | **不可复算**（有日期可锚） |
| 「An unprecedented amount of vectors」 | Rive Renderer 页（`W2G-085` 同页） | 无单位 | 定性卖点 |
| 「只重画变化部分」 | Coherent 技术概览（`W2G-017`） | 机制描述（无数字） | 机制可信（与 `W2G-013` 互证），数字缺 |
| 「深度 GPU 集成 / 两路渲染器」 | Ultralight 首页（`W2G-037`/`W2G-049`） | 机制描述 | 机制可信 |

> 纪律：以上任何一条**不得**写进我们的性能预算表；我们的预算只认可复现读数（`reports/09`）。

## 4. 坑与反例（负面留档）

1. **默认参数埋雷**：GFx 导出器默认产出 TGA（比 DDS 占资源），要靠使用者知道 `-i DDS`
   （`W2G-062`）。→ 我们的导出/打包工具必须「默认即最优」并有测试。
2. **整数吸附的动画代价**：元素坐标吸附到整数后，`top/left` 动画仍会抖动；相邻元素动画会出现
   背景色渗出（官方给规避法）（`W2G-030` 所在页）。→ 我们的吸附策略必须进测试协议。
3. **HTML 兼容性洞**：`flex-basis: content` 不支持（`W2G-032`）；Ultralight 缺 WebGL/WebRTC/音视频
   （`W2G-042`）。→ 若走 HTML 语义，必须交付「不支持清单」。
4. **多渲染器 → 特性碎片**：Rive 因「某特性在一个后端没有就不能发」而自研单一渲染器
   （`W2G-087`）。→ 我们 GPU 档若多后端，特性按后端显式降级。
5. **上游整体消失**：CRYENGINE 5.0–5.6.7 于 2022-05 从 GitHub 与启动器下架，5.7 LTS 源码需账号
   （`W2G-064`/`W2G-065`）；本批实测 `CRYTEK/CRYENGINE` 已 404。→ 依赖半公开栈 = 可复算性风险，
   快照必须自留。
6. **版本×版本兼容矩阵**：Scaleform 3 需配 CRYENGINE 5.0–5.6.7 + Flash CS6，Scaleform 4 配 5.7 LTS
   （`W2G-067`）。→ 我们的渲染器/壳/青简版本兼容表也要显式化。
7. **免费档不给商业游戏**：Ultralight 免费档为「Indie devs only (< $100K)、Application use only」
   （`W2G-052`），Pro 档 $3,000/年/应用（`W2G-051`）。→ 引用其技术可以，集成须付费且仍不满足 GPL。
8. **每标题每平台授权**：Coherent 授权形态将成本与发行规模绑定（`W2G-035`），源码另计
   （`W2G-036`）。→ GPL 工程不可引入。
9. **任务点名的假设被否证（否证候选）**：O3DE 工具 UI 为 Qt（`W2G-079`/`W2G-080`），仓库内无
   Chromium/CEF 路径命中（`W2G-081`）。→ 「O3DE 前身内置 CEF」不作为事实使用。
10. **文档可达性陷阱**：Unreal 官方页 JS 渲染（本批仅得目录残片，`W2G-074`）；Noesis 官网 403
   （`W2G-009`）。→ 本报告对二者只用可锚材料，其余进 §6。

## 5. 许可红线（逐条，针对本仓 GPL-3.0-or-later）

| 对象 | 许可形态（锚） | 判定 |
| --- | --- | --- |
| NoesisGUI | 运行时闭源二进制、门户下载（`W2G-005`）；NuGet 托管层开源（`W2G-010`） | **不可用**：不可修改/不可再分发的专有运行时 |
| Coherent Gameface | per-title/per-platform 商业授权（`W2G-035`）；源码=付费加项（`W2G-036`） | **不可用**：商业授权与自由再分发冲突 |
| Ultralight | 免费档限个人/年收入<100K/仅应用（`W2G-052`）；条款禁修改与衍生、授权不可转让（`W2G-053`/`W2G-054`）；完整源码专有（`W2G-041`）；WebCore 部分 LGPL（`W2G-040`） | **不可用**（整体）；LGPL 部分理论上可组合，但**须逐文件核对覆盖清单**（本批未获得 → §6） |
| Scaleform + CRYENGINE | 引擎文档：需启用 SF4 并持有第三方 SDK（`W2G-058`/`W2G-063`）；CRYENGINE 源码需账号（`W2G-064`） | **不可用**：专有中间件 + 受限引擎源码 |
| CEF | BSD（`W2G-068`）；许可上可用，但引入整棵 Chromium 与其更新/体积 | **许可可行、工程不吸收**（本轮结论） |
| Qt WebEngine | 基于 Chromium；逐项许可清单（`W2G-075`/`W2G-078`） | **许可可行（LGPL/GPL 路线）**，但进程模型与体量不吸收 |
| Rive | 运行时 MIT（`W2G-082`）；渲染器 2024-03 开源（`W2G-086`） | **可用**：若做矢量动画资产档，运行时代码许可无障碍 |

## 6. 未验证项（写明缺什么证据）

1. **NoesisGUI 官网全站 403**（`W2G-009`）：产品页/性能页/定价页/论坛均未获得 → 「sub-millisecond」、
   免 royalty、许可价格等主张**无原文锚**；本报告对其只用了官方 GitHub 组织材料。
2. **「UE WebBrowser = CEF3」未验证**：官方说明页 JS 渲染（`W2G-074`）；已核 API 页/UMG 页均无
   CEF/Chromium 字样。需补：Epic 页面正文或源码级证据。
3. **Scaleform 停售/停产时间线未获得一手页面**：autodesk.com 403、Wikipedia/archive.org 本机不可达
   （`W2G-066`）。现有间接证据：CRYENGINE 5.7 LTS 文档仍在维护 SF3/SF4 集成（`W2G-056`）。
4. **Ultralight LGPL 覆盖清单**：官方只说「a portion of WebCore」（`W2G-040`），未给文件清单；
   若考虑任何形式的复用，必须先核清单。
5. **Coherent/Rive 的性能主张口径**（`W2G-034`/`W2G-086`）：blog 与产品页均未给机器/场景；
   需厂商 benchmark 或第三方实测才能进预算讨论。
6. **NoesisGUI 的 IME/输入文档**：官网文档不可达，本批只有 Unreal 插件 README 与示例列表
   （`W2G-003`/`W2G-006`）——其 IME 能力本轮**未验证**。
7. **O3DE 的否证范围**：本批证明的是「仓库检索无 Chromium/CEF 路径 + 工具 UI 为 Qt」
   （`W2G-079`/`W2G-081`），**不等于**「O3DE 全组件永不含 CEF」；如需绝对结论须读其 CMake/3rdParty
   清单（本轮未做）。

## 7. 复算与来源

- 抓取脚本：`D:/KF/LSSMJ/scratch/fetch_w2g.py`（urllib + `gh api`；每页落 `.html` + `.txt` 纯文本）。
- 缓存目录：`D:/KF/LSSMJ/scratch/fetch/w2g/`（含 `_index.json` 状态表、`sm_*.xml` sitemap）。
- 账本：`docs/analysis/ledger/w2g.jsonl`（89 条：doc 86 / web 3；7 个目标；`verify` rejected=0）。
- 复算命令：`python tools/ledger.py verify --file docs/analysis/ledger/w2g.jsonl`。
- 本批**不可达**（记录在案）：`www.noesisengine.com`（403）、`docs.noesisengine.com`（DNS 失败）、
  `www.autodesk.com`（403）、`en.wikipedia.org`/`web.archive.org`/`raw.githubusercontent.com`
  （本机网络不可达）、`www.gamedeveloper.com`（403）、`assetstore.unity.com`（TLS 超时）。
