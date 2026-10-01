# Unity：UGUI 与 UI Toolkit（docs.unity3d.com / unity.com；抓取 2026-10-01）

> 上锚：UGUI 公开源码 `Unity-Technologies/uGUI` @ `9b8c5df053bf9a235c673b9bf4a549e573f2a279`（main，GitHub API 记最近推送 2026-09-26）；
> UI Toolkit 包源码镜像 `needle-mirror/com.unity.ui` @ `510d7618e549150d73c2e053f25400386fe81c8a`（**1.0.0-preview.18 / Unity 2020.3 期快照，非 Unity 6 现行版本**）。
> 深度口径：本报告以 doc 为主（Unity 官方手册/博客/UGUI 包手册），source 仅用上述两个公开仓做局部印证；
> 每条主张对应 `docs/analysis/ledger/w2e.jsonl` 的 W2E-0xx 行；doc 引文已对抓取缓存逐条复验（bad=0）。

## TL;DR（每条带锚）

1. UGUI 的重建是**帧末批处理**：脏标记只注册队列，`Canvas.willRenderCanvases += PerformUpdate` 时统一消费（`W2E-032`）。
2. 重建循环分 5 个采样阶段（Prelayout/Layout/PostLayout/PreRender/LatePreRender），布局与图形分两轮循环（`W2E-031`/`W2E-033`/`W2E-034`）。
3. 单个 Canvas 上改一个元素可能造成**数毫秒 CPU 尖峰**——官方博客原话与多 Canvas 切分建议（`W2E-011`/`W2E-012`）。
4. 合批三条件：同一 Canvas 内元素须**同 Z、同材质、同纹理**；子画布对父/兄弟双向隔离（`W2E-014`/`W2E-013`）。
5. 禁用 Canvas 组件停发 draw call 但**保留顶点缓冲**，重新启用不触发重建（`W2E-015`/`W2E-016`）。
6. 布局重建固定四遍：算水平 → SetLayoutHorizontal → 算垂直 → SetLayoutVertical；立即重建（`ForceRebuildLayoutImmediate`）源码注释自带性能警告（含原文拼写错误 unavaoidable）（`W2E-040`/`W2E-039`）。
7. UI Toolkit 的布局就是 **Yoga**（`visualTree.yogaNode.CalculateLayout()`），USS 属性语义对齐 Yoga/Flexbox；但 Yoga 不允许布局期改树（UI Toolkit 直接抛异常）（`W2E-048`/`W2E-001`/`W2E-050`）。
8. UI Toolkit 用 **usage hints** 把优化决策显式化：`style.translate` 不重算布局；`DynamicTransform` 把变换更新推给 GPU；`GroupTransform` 明确写「强制单独 draw batch」的代价（`W2E-009`/`W2E-010`/`W2E-052`）。
9. 纹理图集被官方定义为**合批破坏的第一来源**（"batches broken by texture changes"），UI Toolkit 有运行期 dynamic atlas 子系统（`W2E-008`/`W2E-004`）。
10. **许可红线**：UI Toolkit 包源码是 Unity Companion License（仅限 Unity 相关项目），不能移植进 GPL 的青简；只能作阅读参考（`W2E-057`）。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 脏标记 + 帧末统一重建（两段循环：先布局后图形） | `W2E-032`/`W2E-033`/`W2E-034`/`W2E-035` | 吸收：与青简「不做同步重排」的纪律同向，阶段划分可直接照搬为渲染器内的 rebuild 阶段。 |
| 失效域切分（多 Canvas / 嵌套子画布 = island） | `W2E-012`/`W2E-013`/`W2E-018` | 吸收：候选窗的「静态皮肤 + 高频变化列表」应做同样的域切分。 |
| 合批键 = 材质 + 纹理 + Z（Layer ID） | `W2E-014` | 吸收：作为青简批键设计的对照清单。 |
| 保留几何的隐藏路径（禁用 Canvas 不丢顶点缓冲） | `W2E-015`/`W2E-016` | 吸收：候选窗隐藏/显示切换不应销毁几何缓存。 |
| 缩放策略集中（CanvasScaler 三模式 / PanelSettings Scale Mode） | `W2E-019`/`W2E-020`/`W2E-021`/`W2E-003` | 有界吸收：语义可参考；但候选窗只有单一窗口语义，不需要三套模式。 |
| usage hints（translate 免布局 / DynamicTransform 走 GPU） | `W2E-009`/`W2E-010`/`W2E-051` | 吸收：把「哪条通道更新」显式化，避免布局属性参与每帧动画。 |
| 动态图集（自动增删纹理）+ 显式提交点 Commit | `W2E-004`/`W2E-053` | 有界吸收：图集思想吸收；Unity 的实现不可复用（许可），青简需自建（与 cosmic-text/swash 图集合流）。 |
| SDF 字形图集 + 动态字符累积与清理策略 | `W2E-027`/`W2E-028`/`W2E-026` | 有界吸收：SDF 结论可用；TMP 的 FontAsset 资产化流程是 Unity 特有产物，不照搬。 |
| 布局四遍 pass / 对象池 / 裁剪差分更新 | `W2E-040`/`W2E-041`/`W2E-043`/`W2E-047` | 吸收：热路径零分配与「不变不下发」是通用工程手段。 |
| 批断裂可观测（Batch Breaking Reason / 图集可视化） | `W2E-024`/`W2E-070` | 吸收：性能工程需要「批次为何断」的归因字段，不是只看数字。 |
| **UGUI/UI Toolkit 源码本体** | `W2E-057`（License.md 原文） | **不吸收（许可）**：Unity Companion License 限制在 Unity 项目内使用，GPL 仓不得移植。 |
| UGUI 的 GameObject/Component 双层模型 | `W2E-017`/`W2E-022` | 不吸收：属于编辑器工作流遗产，与自绘渲染器的元素模型无关。 |

## 1. UGUI 架构全景

- Canvas = UI 布局与渲染的抽象空间，所有 UI 元素必须是 Canvas 子物体；嵌套 Canvas 的官方定位是「for optimization purposes」（`W2E-017`/`W2E-018`）。
- 渲染模式三种：Screen Space - Overlay / Screen Space - Camera / World Space（`W2E-017` 所在页面的 Render Mode 表）。
- 布局与绘制的桥：`Graphic`（可绘制组件基类）持两枚独立脏标记 `m_VertsDirty` / `m_MaterialDirty`，Set 时只注册队列（`W2E-035`/`W2E-036`/`W2E-037`）。
- 共享默认材质 `s_DefaultUI = Canvas.GetDefaultCanvasMaterial()`——「同材质」合批前提在代码层的第一步（`W2E-038`）。
- 布局系统：`LayoutRebuilder` + `ILayoutElement/ILayoutController`，重建按四遍固定顺序执行；实例走 `ObjectPool`（`W2E-040`/`W2E-041`）。
- 缩放：`CanvasScaler` 三模式（Constant Pixel Size / Scale With Screen Size / Constant Physical Size），代码里按枚举分派到各自处理函数（`W2E-019`/`W2E-020`/`W2E-042`）。
- 裁剪两条路：`RectMask2D`（几何裁剪，clip rect 差分下发）与 `Mask`（`IMaterialModifier` 改材质，模板实现）（`W2E-043`/`W2E-044`/`W2E-045`）。

## 2. 重绘/失效模型（UGUI）

1. **登记**：`SetVerticesDirty()` → `m_VertsDirty = true` + `CanvasUpdateRegistry.RegisterCanvasElementForGraphicRebuild(this)`（`W2E-035`）。
2. **消费点**：`Canvas.willRenderCanvases += PerformUpdate`——渲染前统一执行（`W2E-032`）。
3. **阶段**：`CanvasUpdate` 枚举 5 阶段，各自有 ProfilerMarker；第一轮循环到 PostLayout（布局），第二轮从 PreRender 起（图形）（`W2E-031`/`W2E-033`/`W2E-034`）。
4. **回调**：`Graphic.Rebuild(CanvasUpdate)` 按阶段 switch；PreRender 时若 `m_VertsDirty` 则 `UpdateGeometry()`（`W2E-037`）。
5. **官方语义**：改一个元素 → 整个 Canvas 的几何/批次重建（毫秒级尖峰）；对策是切分 Canvas/嵌套子画布（`W2E-011`/`W2E-012`/`W2E-013`）。
6. **静态/动态分离原则（官方原话）**：静态元素放独立 Canvas，同期更新的动态元素放更小的 sub-canvas；同 Canvas 内元素保持同 Z/材质/纹理（`W2E-014`）。
7. **隐藏路径**：禁用 Canvas 组件 = 停 draw call 但保留网格与顶点（`W2E-015`/`W2E-016`）。
8. **可观测性**：UI (Canvas)/UI Details (Canvas) Profiler 模块解释「批怎么形成/为何断裂」，含 Self/Cumulative Batch Count 与 Batch Breaking Reason 列（`W2E-024`/`W2E-025`）。

## 3. UI Toolkit（Uxml/Uss/VisualElement/Yoga/PanelSettings）

- 视觉树：VisualElement 为所有节点基类（样式、布局数据、事件处理器同盒）（`W2E-005`/`W2E-006`）。
- 布局：Yoga（Flexbox 子集）真实存在于源码路径 `UIRLayoutUpdater.Update()` → `visualTree.yogaNode.CalculateLayout()`，并可多趟（`kMaxValidateLayoutCount = 5`）（`W2E-048`；`W2E-001` 文档侧口径）。
- 增量策略：与标准布局更新的唯一差别是「只有布局矩形真变化才标脏重绘」（源码注释原话），而非看 `yogaNode.HasNewLayout`（`W2E-049`）。
- 布局重入约束：Yoga 不允许布局计算中改节点树，UI Toolkit 在布局期检测到层级变更直接抛 `InvalidOperationException`（`W2E-050`）。
- 运行期面板：PanelSettings 管 Scale Mode 参数与 **dynamic atlas settings**（`W2E-003`/`W2E-004`）；动态图集管理器有显式 `Commit()` 提交点，提交后当前图集纹理可能切换（`W2E-053`）。
- 绘制：渲染链有纹理槽管理器与显式 `StartNewBatch()` 入口（`W2E-055`），与官方「批断裂原因」观测闭环。
- 文本：UI Toolkit 走 TextCore（`TextGenerator.GenerateText`），与 TMP/FontAsset 同源（`W2E-054`）。
- 优化契约：`style.translate` 在 transform 阶段生效（不重算布局）；`DynamicTransform` 推 GPU；`GroupTransform` 注释明写「强制单独 draw batch」的交换（`W2E-009`/`W2E-010`/`W2E-052`）。
- 官方迭代方向：Unity 官方文章把 UI Toolkit 定位为运行期 UI 的 UGUI 替代路线（Web 式工作流）（`W2E-030`），并有高级开发者 BPG（含性能优化章）（`W2E-029`）。

## 4. 文本（TMP / FontAsset）

- FontAsset = 黑白/灰度**字形图集**纹理 + 字符表（`W2E-026`）。
- 图集类型：Distance Field（SDF，官方推荐，缩放/变换下平滑）/ Smooth / Raster（`W2E-027`）。
- 动态字符累积：运行期新增字符会加入字形表并扩图集；`Clear Dynamic Data`（及 on Build）把字符/字形表与图集纹理清空重置（`W2E-028`）。
- 级联代价：旧版 Text 订阅 `Font.textureRebuilt += RebuildForFont`——字体图集重建会反查并重建所有用该字体的文本（`W2E-046`）。
- 与 Unreal 的互证：Slate 侧字体图集同样按需把字符缓存进纹理，且图集内容类型已含 Msdf（见 `docs/platforms/unreal-slate.md`，`W2E-078`/`W2E-081`）。

## 5. 公开读数与性能手段（数字带锚）

| 读数 | 锚 | 口径 |
| --- | --- | --- |
| 单 Canvas 改一元素「CPU spike costing multiple milliseconds」 | `W2E-011` | Unity 官方博客（how-to/unity-ui-optimization-tips，页面无版本号；仅在段内引用 Unite 演讲） |
| 禁用 Canvas 组件不触发重建 | `W2E-015`/`W2E-016` | 同上博客 |
| 批次计数口径（Self vs Cumulative，含嵌套画布） | `W2E-025` | UGUI 包手册 ProfilerUI.html |
| 批断裂原因列为可观测项 | `W2E-024` | 同上 |

> 说明：本批**未取得**任何带机器/帧率口径的 Unity 官方基准数字（如具体 ms 分布、设备型号），上述「multiple milliseconds」是定性描述，不做定量外推。

## 6. 坑与反例（负面留档）

1. **同步强制布局是逃生舱**：`ForceRebuildLayoutImmediate` 注释明确「multiple layout passes are unavaoidable despite the extra cost in performance」（原文含拼写错误）（`W2E-039`）。
2. **Mask 走材质改写**：模板实现会产生材质实例（`StencilMaterial` 静态注册表去重），与「同材质」合批条件天然张力（`W2E-044`/`W2E-045`）。
3. **Yoga 重入崩溃**：布局期改树 → `EXC_BAD_ACCESS` 记录在源码注释里（`W2E-050`）。
4. **usage hints 的隐藏代价**：GroupTransform 换 CPU 更新量于「单独 draw batch」，是显式交换（`W2E-052`）。
5. **文档语法错误留档**：「It is also possible use nested Canvases」（官方页面原文），引用时按逐字保留（`W2E-018`）。
6. **旧版快照风险**：UI Toolkit 镜像停在 1.0.0-preview.18（2021-10-07 推送），其渲染链/图集实现与 Unity 6 现状可能有差异（`W2E-056`/`W2E-102`）。

## 7. 未验证项（缺什么证据）

- Unity 6 现行 UI Toolkit 的渲染链实现（本镜像为 2020.3 期 preview；Unity 6 的 jobified 网格生成、并行文本生成等博客口径**未取得页面原文**，仅搜索摘要见过，不计入账本）。
- `UIE-performance-consideration-runtime` 等 6000.2 页面里被工具提示（glossary tooltip）插入的正文片段已避开；页面要求的具体平台限制（如 mobile）未逐条摘录。
- UGUI 的 `Canvas.BuildBatch` 是否仍在 worker 线程（`Canvas.GeometryJob`）——属社区转述，本批未抓到官方原文，**无账本行**。
- TextMeshPro 的动态图集扩页阈值与内存上限（未抓取 TMP 高级设置页）。
- 合批实测：本任务不做源码执行/基准复现，全部为文档口径。
