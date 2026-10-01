# Chromium（https://github.com/chromium/chromium @ 6f0de2f0027ba69edad4914a81575abf63c56476, 抓取 2026-10-01）

> 轨道：B3 文档轨道（**未克隆主体**）。抓取方式：python urllib 拉 GitHub 镜像 `chromium/chromium` 的
> `docs/**` 与 `cc/`、`third_party/blink/renderer/**` 的 README/头文件（存于
> `D:/KF/LSSMJ/scratch/src/chromium-docs/`，source 条目逐行引用），以及 chromium.org 设计文档正文
> （curl/urllib 抓 HTML 后转文本，存同目录 `txt/`）。
> **可取得性留档**：`lcd-text-and-text-rendering`、`display-compositor`、`slimming-paint/v2`、`blinkng`、
> `paint`、`compositing-in-blink` 等 6 条 chromium.org 旧链接在 2026-10-01 抓取为 **404**
> （W2C-084/085；`how-repaint-works` 自述迁往 Google Docs，本机不可达 W2C-081），对应主题改用
> 仓内 README 与源码替代（已逐条注明）。
> 账本：`docs/analysis/ledger/w2c.jsonl`（100 条，source=45 / doc=55，rejected=0，2026-10-01 复算）。

## TL;DR（每条带锚）

1. **三层管线是清晰的：绘制产物 → 合成 → 显示聚合。** Blink 的 `core/paint` 把 LayoutObject 树转成
   「含 display item list 的 cc::Layer 列表 + cc::PropertyTrees」（W2C-016）；`PaintArtifactCompositor`
   由 paint chunks 做图层化生成 cc::Layer（W2C-023）；各 client 的 compositor frame 最终由
   `SurfaceAggregator` 递归替换 SurfaceQuad 聚合成一帧（W2C-013）。
2. **绘制记录与合成分层解耦（CompositeAfterPaint，M94 上线）**：合成决策放在绘制之后
   （W2C-060），属 Slimming Paint 系列（2015–2021 分阶段，W2C-055；M45/M58/M67/M75 里程碑见
   W2C-058/059）。
3. **paint chunk 是共性中间表示**：属性树状态相同的相邻 display item 合成 chunk，chunk 决定
   「怎么画/怎么合成」（W2C-022）；栅格失效（RasterInvalidator）就是新旧 chunk 的逐项比对
   （W2C-025）。
4. **失效传播有三本不同的账**：paint invalidation（Blink 侧谁要重画）→ raster invalidation
   （chunk 差异转 tile 重栅格）→ damage（两次 CompositorFrame 之差，支撑 partial swap 省电）
   （W2C-028/009/010）。
5. **属性树把层级属性传播从 O(图层数) 降到 O(受影响节点数)**（W2C-006），并用 isolation boundary
   （`contain: paint`，alias 节点实现）截断脏传播（W2C-018）。
6. **栅格化绝不阻塞出帧路径**：软件栅格在专门 worker 线程做（W2C-043）；pending 树要等视口内高分辨率
   内容全栅格完才激活（W2C-044）；栅格没赶上就按现有 active 树提交——文档明说这是掉帧来源之一
   （W2C-011/012）。
7. **tile 是调度单位**：优先级 = 距视口距离/预计上屏时间等（W2C-042），分四档紧急度桶
   （W2C-068），按优先级发显存；尺寸启发式软件约 256×256、GPU 约「视口宽 × 1/4 视口高」
   （W2C-007）。
8. **GPU 光栅不是天然并行**：受上下文锁限制同一时刻只有一条栅格 worker（W2C-008）；GPU 光栅
   在 Chrome 37 引入，页面自报部分负载 100ms/帧 → 4–5ms/帧，且可被内容特征一票否决
   （W2C-076/078）。
9. **帧生命周期被切成可观测的 17 步**（BeginFrame→…→Presentation），并以 PipelineReporter 分段
   记账：`BeginImplFrameToSendBeginMainFrame` … `SubmitCompositorFrameToPresentationCompositorFrame`
   （W2C-011~015）。
10. **文本渲染是运行时多因子决策**：Chrome UI 文本统一走 RenderText/RenderTextHarfbuzz
    （W2C-034/079）；LCD（亚像素）文本用 `LCDTextDisallowedReason` 枚举逐项否决——非整数平移
    （W2C-035）、内容不透明（W2C-036）等；能力挂在 **tile** 上（`can_use_lcd_text`，W2C-037），
    全局默认开（W2C-038）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 记录（display item/chunk）与栅格分离，一份记录按需多次栅格 | W2C-002/022 | 吸收 |
| 「先记录、后分层」：分层决策依赖绘制产物 | W2C-019/060 | 吸收 |
| 三层脏账本分离（paint/raster/damage） | W2C-028/009 | 吸收 |
| 属性树 + 隔离边界做 O(受影响节点) 更新与剪枝 | W2C-006/018 | 有界吸收（元素少时收益有限） |
| 栅格出帧分离 + pending/active 双树原子提交 | W2C-027/044/070 | 有界吸收（双树为正确性方案，成本是延迟与内存） |
| tile 优先级函数（何时可见/何时清晰）与显存预算制 | W2C-042/053 | 吸收 |
| partial swap / 局部呈现省电 | W2C-010/029 | 吸收（常驻小窗收益明确） |
| 亚像素文本的否决清单（非整数平移/不透明性/…） | W2C-035/036/037 | 吸收（直接可用作候选窗判定表） |
| 段落级文本布局（LayoutNG）与片段缓存 | W2C-032/033/074 | 吸收 |
| CJK 合字需段落级整形才不受限 | W2C-075 | 吸收（中文候选窗强相关） |
| 隔离边界允许跳过子树遍历 | W2C-018 | 有界吸收（需先把「隔离」编码进树） |
| GPU 光栅上下文锁单 worker、内容 veto | W2C-008/078 | 不吸收（本目标规模不需要；但要记住锁语义） |
| 多进程 GPU 进程/命令缓冲/mailbox/sync point | W2C-047 | 不吸收（青简历史上单进程，收益与成本不成比） |
| Surface 的 untrusting/双 ID 权限模型 | W2C-062/065 | 不吸收（无不可信客户端嵌入场景） |
| 图层压扁（squashing）与合并取舍（overlap/sparsity） | W2C-045/024 | 有界吸收（层数少时简化版足够） |
| 具体实现类名与 2014 年文档描述 | W2C-039 | 不吸收（类名已变，仅作历史锚） |

## 1. 架构全景（模块地图）

| 模块 | 职责（锚） |
| --- | --- |
| `third_party/blink/renderer/core/layout` | 布局：LayoutSubtreeChange/PreLayout/PerformLayout 等生命周期；实现细节见 LayoutNG（W2C-032） |
| `third_party/blink/renderer/core/layout/layout_ng.md` | LayoutNG：算法输入封闭 (BlockNode, ConstraintSpace)，片段缓存判据=输入相等（W2C-032/033） |
| `third_party/blink/renderer/core/paint` | PrePaint（paint invalidation + 属性树构建）与 Paint（display item → chunk → cc::Layer）（W2C-016/017/019） |
| `third_party/blink/renderer/platform/graphics/paint` | PaintArtifact 数据结构、PaintController、RasterInvalidator、GeometryMapper（W2C-022/025/026） |
| `cc/` | 合成器：Layer/LayerImpl、四棵树（main/pending/active/recycle）、TileManager、Scheduler（W2C-001/027） |
| `components/viz` + `services/viz` | 显示合成器（Display Compositor）、frame sink、surface 管理、聚合（W2C-030/031/064） |
| `ui/gfx/render_text*`（经 docs 描述） | Chrome UI 文本：RenderText/RenderTextHarfbuzz + Skia 绘制（W2C-034/079） |
| `docs/`（镜像） | 仓内设计文档：how_cc_works、life_of_a_frame、frame_trees（W2C-093）等（W2C-001 起） |

> 注：cc 在浏览器进程（ui/compositor、Android）与渲染进程（Blink/RenderWidget）**两个宿主**内嵌
> （W2C-001）；多线程版用于渲染进程，单线程版用于浏览器 UI（W2C-003）。

## 2. 关键机制

### 2.1 绘制记录（paint artifact / display item list）
- PaintArtifact = 按绘制顺序的 display item 列表，按 paint chunk 分区；chunk 携带 transform/clip/
  effect/scroll 四棵属性树上的节点引用（W2C-022）。
- `PaintController` 负责产出 artifact，并通过 `UseCachedItemIfPossible`/`UseCachedSubsequenceIfPossible`
  复用上次结果（脏才重画）；缓存失效在 chunk 级与 display item 级两级比对（W2C-025）。
- 绘制按多个 paint phase 多次走树，空阶段由 `NeedsPaintPhaseXXX` 标志短路（W2C-020）。
- 像素对齐贯穿全链：文档单列「Pixel snapping and bluriness」一节（W2C-021）。
- hit-test 信息在同一遍绘制中记录（paint-order 保序），供 cc 侧命中测试（W2C-086）。

### 2.2 失效与脏传播
- PrePaint 阶段以先序遍历、跨 frame 边界，只走被标脏的子树/对象（W2C-017）；被标脏的原因是
  style/layout/compositing 变更，标记发生在更早的生命周期阶段。
- 属性树更新有快路径：transform/opacity 可以绕过整树构建直接改节点，但须延迟到 PrePaint 执行
  （避免 paint offset 变化漏检）——当前快路径只覆盖这两类更新（W2C-087）。
- isolation boundary（`contain: paint`）以 alias 节点实现，子树节点只能引用到隔离节点，因而祖先
  拓扑变更不必穿透（W2C-018）。

### 2.3 合成器分层与栅格化调度
- 分层：`PaintArtifactCompositor::LayerizeGroup` 起点，为每个 chunk 建 PendingLayer 再尝试合并；
  阻止合并的三种理由：属性节点上有直接合成理由、交叠（overlap testing）、稀疏浪费
  （`kMergeSparsityAreaTolerance`）（W2C-023/024）。
- 直通更新：纯重画（layerization 不变）的变更走 `UpdateRepaintedLayers`，避免整次
  `PaintArtifactCompositor::Update`（W2C-088）。
- 栅格：TileManager 收集「active 树要画的、pending 树要激活的、视口附近次要的、离屏要解码的」
  tile；软件栅格在 worker 线程、GPU 栅格受上下文锁仅一 worker（W2C-043/008）。
- 显存：GPU 进程全局 GpuMemoryManager 定预算，各宿主在预算内按 tile 优先级发放（W2C-053/068）。

### 2.4 帧生命周期（life of a frame 的 17 步）
`[1]BeginFrame → [2]合成线程更新 → [3]BeginMainFrame → [4]主线程更新(输入/Animate/Style/Layout)
→ [5]Commit(阻塞主线程拷贝) → [6]等栅格 → [7]Activation → [8]等 deadline → [9]SubmitCompositorFrame
→ [10]AggregateSurfaces → [11]Draw Frame(DDL 录制) → [12]RequestSwap → [13]SyncToken 等待
→ [14]GPU 排队 → [15]GPU draw → [16]Swap → [17]Presentation`（W2C-011~015）。
- 任一前置步骤超时 ⇒ 以既有 active 树提交，掉帧（W2C-012）；GPU 队列不可抢占，排队延迟单列
  （W2C-015）。
- 观测口径：PipelineReporter 把管线切成 7 段（BeginImplFrameToSendBeginMainFrame、SendBeginMainFrameToCommit、
  Commit、EndCommitToActivation、Activation、EndActivateToSubmitCompositorFrame、
  SubmitCompositorFrameToPresentationCompositorFrame）用于 trace 记账（W2C-090）。

### 2.5 Viz：surface 与聚合
- Surface = 可含同/异客户端 surface 引用的矩形，提交帧即隐式生成 sequence number 供跨客户端同步；
  「引用不等于访问」（untrusting）（W2C-061/062/065/091）。
- 聚合算法递归替换 SurfaceQuad，遇「无可用帧」或「环」则跳过该 quad；SurfaceAggregator 近无状态，
  SurfaceManager/ResourceProvider/DisplayManager 各单实例（W2C-063/064）。
- 显示合成器把多 client 帧合成到单一后备存储；平台细节（OutputSurface/SoftwareOutputDevice）分离
  （W2C-030/031）；draw 阶段 SkiaRenderer 录 DDL、GPU 线程重放（W2C-014）。

### 2.6 文本与 LCD 渲染决策
- Chrome UI 文本统一走 `gfx::RenderText`（Canvas 亦以其为底），平台整形差异被吃掉，绘制走公共
  Skia 路径；RenderText 有状态、跨绘制缓存布局（W2C-034/079/080）。
- LCD（亚像素）文本：`LCDTextDisallowedReason` 枚举列出全部否决理由——kSetting、
  kBackgroundColorNotOpaque、kContentsNotOpaque、kNonIntegralTranslation、kNonIntegralXOffset/
  YOffset、kWillChangeTransform、kPixelOrColorEffect、kTransformAnimation、kNoText（W2C-035/036）。
  枚举值注释表明「These values are used in benchmarks」（基准口径）。
- 能力挂在 **tile**：`Tile::CreateInfo::can_use_lcd_text` 默认 false（W2C-037）；全局
  `LayerTreeSettings::can_use_lcd_text` 默认 true（W2C-038）；枚举全集覆盖 11 项
  （kNone..kNoText，含像素/颜色 effect 与变换动画，W2C-092）。

### 2.7 布局（LayoutNG）
- 每种 CSS 布局模式一个 LayoutAlgorithm，输入封闭为 (BlockNode, ConstraintSpace)，越界访问破坏
  片段缓存不变量（W2C-032）。
- 片段缓存复用判据：constraint space 完全相等 + 无 break token + 未标脏（W2C-033）。
- 文本布局改为段落级再切行（W2C-074），解除旧引擎 CJK 连字限制（W2C-075）；Chrome 77 发布
  （W2C-072），分阶段替换（先 inline/block）。

## 3. 性能手段与公开读数（数字带口径）

| 读数 | 口径 | 锚 |
| --- | --- | --- |
| GPU 光栅：部分绘制负载 100ms/帧 → 4–5ms/帧 | Chrome 37 引入；页面自报「some paint workloads」；文档约 2015 年 | W2C-076 |
| Slimming Paint 落地后：总 CPU −1.3%；滚动更新 P99 改善 3.5%+；输入延迟 P95 改善 2.2%+ | 页面自报（following launch），未给机器/版本矩阵 | W2C-056 |
| Slimming Paint 删除 22,000 行 C++ | 页面自报（following launch） | W2C-057 |
| 低端设备上传单张 256×256 纹理：数毫秒，极端 3–5ms | 原文口径 low-end devices（impl-side painting 时代） | W2C-067 |
| 低分辨率 tile 栅格成本降 5–6× | 原文自标 anecdotal；ICS 时代用户抱怨字体糊 | W2C-071 |
| LayoutNG：W3C 测试集 1,258 例中旧引擎失败的 103 例通过 | 口径=正交流相关用例；文档 2019 | W2C-073 |
| tile 尺寸启发式：软件 256×256；GPU 视口宽 ×1/4 视口高 | cc 文档（how_cc_works） | W2C-007 |
| PicturePile 以视口为中心截 10,000px | 原文标注「emperically determined」（经验值） | W2C-069 |

> 全部数字均为文档自报、**未在本机复算**；引用时须连同版本与样本口径一起搬。

## 4. 坑与反例（负面留档）

1. **文档过期**：`gpu-accelerated-compositing` 标注 updated May 2014，开篇自述类名因 Slimming Paint
   已变（W2C-039）；「direct to backbuffer/direct Ganesh」在 2014-05 尚未实现（W2C-046）。
2. **链接腐烂**：6 条 chromium.org 旧设计文档 404（W2C-084/085），How repaint works 迁 Google Docs
   （W2C-081），Slimming Paint 历史文档多为需登录 Google Docs（W2C-082）。
3. **GPU 光栅可被内容否决**：大量非凸路径 SVG 会让该次加载退回软件光栅（W2C-078）；
   启用还受视口设置+设备白名单双重条件（W2C-077）。
4. **合并图层的反例**：交叠（overlap testing）与稀疏浪费会阻止 PendingLayer 合并（W2C-024）——
   分层不是越少越好（显存/正确性）也不是越多越好（CPU/内存）。
5. **非整数平移会毁掉亚像素文本**（W2C-035）——动画/缩放期间必须切灰度 AA，否则彩边。
6. **高延迟模式是取舍而非 bug**：主线程慢时 cc 不等 commit 直接画，以吞吐换延迟；恢复需跳过
   BeginMainFrame 追帧（W2C-089）。
7. **棋盘格与原子性的张力**：pending 树等视口高分辨率全就绪才激活；等不及就继续显示旧内容（可能
   过时）或棋盘格（W2C-044）；impl-side painting 的动机正是去掉「补格必须 commit」（W2C-066）。
8. **GPU 队列不可抢占**：显示合成的 draw 可能排到下一帧栅格之后（W2C-015）——把 GPU 当无排队
   资源会低估帧延迟。
9. **tile 栅格参数化**：同一份绘制产物在不同 tile 上可有不同 `can_use_lcd_text`（W2C-037），
   「一份记录、多种栅格」既是能力也是坑（缓存键必须含这些参数）。

## 5. 未验证项（缺什么证据）

1. **Viz 独立设计文档不可得**：`/developers/design-documents/display-compositor/` 404
   （W2C-085）；本报告 Viz 部分仅由仓内 README（components/viz、services/viz、service/display）与
   how_cc_works/surfaces 侧证支撑，未读官方 Viz 设计长文。
2. **LCD 文本设计文档不可得**：`lcd-text-and-text-rendering` 404（W2C-084）；灰度 vs 亚像素的
   决策清单由 `lcd_text_disallowed_reason.h` 枚举 + `tile.h`/`layer_tree_settings.h` 反推，
   未读原始设计文档正文。
3. **未做源码主体核对**（纪律要求）：仅读特定文件（README 与少量头文件/一行源码路径），
   未克隆、未核对实现细节（如 TileManager 具体优先级公式、LayerizeGroup 的完整启发式）。
4. **数字未复算**：§3 全部为 chromium.org 自报读数，未在本机重现。
5. **时效**：镜像 `main` 快照 @ 6f0de2f（2026-10-01）；2014/2015 年的架构描述（命令缓冲、进程
   模型、GPU 光栅流程）在 2026 年是否仍逐字成立未逐条复核。
6. **未取得**：`docs/how_style_engine_get_stylesheet_from_dom.md` 在镜像 404（任务清单给定的
   可选文件不存在，W2C-100），样式/选择器主题未覆盖。
