# 增量框架与失效/批处理：Salsa / rust-analyzer / React / Svelte / Solid / Qt

> 源（2026-10-01 抓取）：`salsa-rs.github.io/salsa/overview.html`、`rust-analyzer.github.io/book/contributing/architecture.html`、
> `react.dev/learn/render-and-commit`（页脚版本 `v 19.3`）、`react.dev/blog/2022/03/29/react-v18`、
> `svelte.dev/blog/runes`、`docs.solidjs.com/advanced-concepts/fine-grained-reactivity`、
> `doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html`。账本：W3B-046..053、055、057、058、060、061。

## 1. Salsa book：增量复用的最小模型（W3B-046..049）

- 目标："The goal of Salsa is to support efficient incremental recomputation"（W3B-046）；
  运行模型=主循环改输入→重调程序，"re-using some of the results from the first call"（W3B-047）。
- 失效判定=red-green："The algorithm Salsa uses to decide when a tracked function needs to be
  re-executed is called the red-green algorithm, and it's where the name Salsa comes from."（W3B-048）。
- **硬前提**："Salsa assumes that your_program is a purely deterministic function of its inputs, or else
  this whole setup makes no sense."（W3B-049）——谁想要增量，谁就必须先交出纯函数。
- 对照我们的帧：`layout(content, theme, constraints)` 把字体度量/主题全部显式入参化，正是该前提。

## 2. rust-analyzer：代价清单（W3B-050）

- "The salsa database maintains a global revision counter. When applying a change, salsa bumps this
  counter and waits until all other threads using salsa finish."（W3B-050）——全局修订号 + 变更时
  等待/取消协议：多线程下的必要复杂度。
- **我们单线程出帧（设计 §3）⇒ 这整段可省**：失效只需帧内标记，无取消协议。

## 3. React：批处理 + 差异才提交（W3B-052/053）

- "React only changes the DOM nodes if there’s a difference between renders."（W3B-052）——**渲染≠提交**，
  差异为空则不动目标。⇔ 我们的 tile 描述符比较（判据 C3）。
- "Batching is when React groups multiple state updates into a single re-render for better
  performance."（W3B-053）——帧内多次标脏合并到帧末一次重建（Slate/UGUI 侧的共识见既有账本 w2e W2E-032..035）。

## 4. Svelte / Solid：把失效粒度压到单项（W3B-055/057/058）

- Svelte 5："Signals unlock fine-grained reactivity , meaning that (for example) changes to a value
  inside a large list needn’t invalidate all the other members of the list."（W3B-055）。
- Solid 文档给出对照："In Solid, updates are made to the targeted attribute that needs to be
  changed, avoiding broader and, sometimes unnecessary, updates."（W3B-057）。
- memo 的定位："Memos resemble effects but are distinct in that they return a signal and optimize
  computations through caching."（W3B-058）——**缓存计算与传播是一体的**；我们只借"缓存计算"半边。
- 我们的取舍：候选**行**=最小失效单元（行内容变→只重排该行+标行区域 damage），不做通用信号系统。

## 5. Qt Scene Graph：数据/执行分离（W3B-060/061）

- "Although we refer to it as a scene graph, a more precise definition is node tree."+
  "The nodes themselves do not contain any active drawing code nor virtual paint() function."（W3B-060）
  ——节点只是数据，渲染由渲染器统一执行。
- "The scene graph API is low-level and focuses on performance rather than convenience."（W3B-061）
  ——内部 API 的定位声明。
- 补充说明：Qt 的批处理/渲染器细节已由 w2d（W2D-004..008、W2D-161..180 等）覆盖，本篇只补节点/材质
  与 API 定位两条不重复的文档锚。

## 6. 对本项目失效三段的合成结论

| 机制 | 参照 | 我们的实现 |
| --- | --- | --- |
| 复用前提=纯函数 | W3B-049 | 布局/测量函数全显式入参 |
| 失效判定 | W3B-048（red-green） | 内容版本号 + 两级（Layout/Paint）红绿标记 |
| 批处理 | W3B-053 | 帧内合并标脏，帧末统一重建 |
| 差异才提交 | W3B-052 | tile 描述符比较；空 damage 跳过合成（C3） |
| 失效粒度 | W3B-055/057 | 行级 |
| 运行时开销 | W3B-050 | 单线程 ⇒ 无修订号等待；留档以防未来多线程 |
| 数据结构定位 | W3B-060/061 | DisplayList=数据；渲染器=执行；内部 API 性能优先 |

## 7. 未验证

- 未读 Salsa book 的 Red-green 算法细节页与 "How Salsa works" 章（本批只取 overview）。
- React/Svelte/Solid 的**实测数字**未取（文档层无基准）；这些来源只支撑"机制形态"结论。
