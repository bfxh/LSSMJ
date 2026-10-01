# Web 栈与 JS 包装的代价（多源专项：js-framework-benchmark chrome152 @21d7204d / Tauri v2 docs / Electron docs / React·Vue·Svelte 文档 / Solid @b25c5577 源码；抓取 2026-10-01）

> **本报告是什么**：C4 专项（任务：Web 栈与 JS 包装的代价——给「替代 JS 包装」的取舍分账）。
> 账本 `../analysis/ledger/w3d.jsonl` **75 条 / 0 拒绝**（source 32 / doc 43），11 个 target、16 个来源（域/仓库）。
> **可复算锚**（快照全在 `D:/KF/LSSMJ/scratch/c4/`）：
> ① JFB 数据 = 仓库 `krausest/js-framework-benchmark` **commit `21d7204d`**（"chrome152"，2026-09-01 同步）的
> `webdriver-ts/results.json`——本机快照 sha256 前缀 `279e12f131b7cf25`，与钉版 raw URL 重下逐字节一致；
> ② Tauri 基准 = `tauri-apps/benchmark_results@f156b794`（默认分支 gh-pages，2026-09-30 更新）；
> ③ Solid 源码 = 浅克隆 `solidjs/solid@b25c5577`；④ 文档 = 逐页快照 + HTML→txt 转换。
> **引文纪律**：doc 引文由 `scratch/c4/gen_w3d.py` 生成（引文不在快照中即中止构建）；source 引文由
> `python tools/ledger.py verify` 逐字机器校验（±5 行内子串匹配）。
> **JFB 数字口径（三句先说清）**：数字取自**仓库原始样本数组**（非本机复测、非官方交互页显示值转抄）；
> 表中 median 为我对 `values.total` 全部 15 个样本的独立复算（与官方分发的 stats 库同一算法）；
> 数据集版本=chrome152、官方口径含渲染时间与 CPU 节流（见 §3.1）。
> **抓取**：curl（raw.githubusercontent.com 需 `--ssl-no-revoke`，见 §4.4）/ python urllib（WebFetch 禁用）。
> **未覆盖**：web.dev、developer.chrome.com 本机不可达（连接超时），浏览器成本章改用 MDN 性能页替代（见 §5）。

## TL;DR（每条带锚：`W3D-0xx` → 账本行；URL 与逐字引文见账本）

1. **JFB 六组数字（chrome152，2026-09-01）**：创建 1000 行 median = vanillajs **20.6** / solid **21.4** / react **23.7** ms——
   首建成本由 DOM/渲染支配，框架运行时分摊仅 ~1–3ms（`W3D-001/002/003`；含渲染口径 `W3D-016`）。
2. **键控交换揭示「实现性」差一个数量级**：1k 表 swap 两行 median = solid **12.6** vs react-hooks **89.9** ms（≈7×）（`W3D-004/005`）。
3. **包体税（压缩后）**：vanillajs **2.5** / solid **4.5** / svelte **9.7** / vue **23.3** / react **51.4** KB（`W3D-006..010`）。
4. **内存与首屏**：加 1000 行内存 react **4.435** vs vanilla **1.861** MB；首屏 react **221.4** / vanilla **52.7** / solid **47.3** ms（`W3D-011..015`）。
5. **Tauri 的「小」= 不随包携带 WebView**：官方自述「use the OS's webview / do not ship a runtime」（`W3D-018/021`），
   Windows 上即 **WebView2（Edge/Chromium 血缘）**（`W3D-022`）——去的是分发体积，不是 Web 引擎常驻。
6. **量级对照（带强限定）**：hello world 二进制 tauri **2.84MB**（2026-09-30）vs electron **166.5MB**（2023-09-24）；
   Electron 空窗 max RSS **454MiB**、**84** 线程（`W3D-023/026/027/028`）——体积差是结构性事实可引，**时间/内存跨年不可直接比**（`W3D-029`）。
7. **Electron 官方自述的税**：鼠标点击先经主进程才到窗口（`W3D-033`）；`require('request')` 近 **0.5s**（`W3D-034`）；
   离屏默认路径要从 GPU 拷回 CPU「requires more system resources」（`W3D-036/037`）。
8. **框架路线在收敛（官方自认）**：纯运行时 VDOM 必须全树 diff + 每次重渲染新建 vnode（`W3D-042/043`）→
   编译期 patch flags（`W3D-044`）；Svelte 走编译器+signals（`W3D-047/048`）；Solid 明确不用 VDOM（`W3D-050`）。
9. **Solid 源码里的通用件**：调度器移植自 React Scheduler（5ms 让出/300ms 上限/输入优先让出，`W3D-053..056`）；
   信号比较器、pure/effect 双队列、环路熔断（`W3D-058/059/060`）——自绘渲染器可直接照搬的纪律。
10. **wasm 不是免费加速**：官方 FAQ「解码比 JS 解析快 20×+」（`W3D-070`）；Liftoff 比 TurboFan 慢 **50%/70%**、
    后台优化还要「several more seconds」（`W3D-072/073`）；-Os/-Oz 以速度换体积（`W3D-074`）。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 帧预算 16.67ms / 主线程唯一队列 | `W3D-062/063` | **吸收**：帧预算表以 16.7ms 为总账（自绘不受 DOM 税，但受显示/合成约束） |
| 「无变化不出帧」（帧驱动） | `W3D-035` | **吸收**：脏标记为一等公民（与青简/WebRender/LOW_SPEC 同族） |
| 按变化频率分层（多 canvas 叠层惯例） | `W3D-066` | **吸收**：静态底/动态层分离（与 cc 图层化、GN 脏区互证） |
| 编译期把「可变性」编码进数据（patch flags / 编译器） | `W3D-044/047` | **吸收（语义级）**：帧模型+脏标记显式化；不引入 DOM/VDOM |
| 依赖图：未变不传播、纯计算先于副作用、环路熔断 | `W3D-058/059/060` | **吸收**：增量数据→像素的依赖图与熔断阀 |
| 协作式调度：5ms 让出、无输入 300ms 上限、输入优先 | `W3D-054/055/056` | **有界吸收**：自绘主线程在长任务上设让出检查点（我们无 Worker，用分批） |
| 「启动不 require 一切」（依赖加载成本） | `W3D-034` | **吸收**：启动路径最小加载（与 LOW_SPEC 白名单同族） |
| GPU 直通优先（共享纹理 vs CPU 位图往返） | `W3D-036/037` | **吸收**：默认路径=零拷贝那条（自绘合成直接持纹理） |
| 复用系统 WebView 换体积（Tauri 路线） | `W3D-018/021/022` | **不吸收**：把版本/行为控制权交给平台；LSSMJ 选自带渲染器换确定性与帧纪律 |
| Web 框架全套机制（React/Vue/Svelte/Solid 的 diff/编译器） | `W3D-038..050` | **不吸收（作为替代对象）**：这些机制服务于 DOM 更新；候选窗直接产位图，跳过 DOM 层 |
| 在 Electron/Tauri 壳内跑 wasm 化 Rust | `W3D-070..074` | **不吸收**：壳的渲染税与进程模型不变；wasm 另有编译窗口与慢档代价 |
| 响应式代理税 / 全量 diff 换正确性 | `W3D-042/045` | **不吸收（反面参照）**：证明「省法只在编译器/依赖图里，不在运行时碰运气」 |

## 1. 全景：这些材料各自是什么、离 LSSMJ 多远

| 层 | 材料（target） | 它对 LSSMJ 的意义 |
| --- | --- | --- |
| 基准数据 | js-framework-benchmark（`W3D-001..017`） | 给「JS 包装」的**量化底账**：首建/局部更新/包体/内存/首屏五轴 |
| 壳方案 A | Electron（文档；`W3D-032..037`；案例见 `../targets/mineradio-electron.md`） | 随包 Chromium+Node：体积/内存/输入路径上最重的方案 |
| 壳方案 B | Tauri v2（文档+基准；`W3D-018..031`） | 复用系统 WebView：体积降一个数量级，但引擎仍在（Windows=WebView2） |
| 运行时机理 | React / Vue / Svelte / Solid（`W3D-038..061`） | 「为什么需要这么多机制」的机制侧答案：DOM 更新成本驱动一切 |
| 浏览器硬约束 | MDN（`W3D-062..067`） | 16.67ms 帧预算、主线程单队列、DOM 节点数——与架构选择无关的物理约束 |
| 加速件边界 | wasm / Emscripten / V8（`W3D-070..074`） | 「换语言/换编译目标」的收益与代价边界 |
| 平台组件 | WebView2 文档（`W3D-068/069`） | Tauri 在 Windows 的真实底座=Chromium 系引擎 |

## 2. 关键机制（运行时与调度）

### 2.1 三个与架构无关的「每帧」事实（MDN）

- 主线程总账：样式计算+重排+重绘**全部**要在 **16.67ms** 内完成才能保平滑（`W3D-062`）；
  浏览器大体单线程，改进方向就是「减少主线程职责」（`W3D-063`）。破坏后果被量化：JS 执行 **1.5s** 期间主线程
  完全被占、点击/触屏全程无响应（`W3D-064`）。
- DOM 规模是显式成本变量：「节点越多，后续阶段耗时越长」（`W3D-065`）——自绘候选窗把行数/字样封顶，
  即以常量规模替代动态最坏情况。
- 画布层的既有工程惯例：按变化频率分层（`W3D-066`）、变换走 GPU（`W3D-067`）。

### 2.2 四路框架机制对照（官方文档自述）

| 机制问题 | React | Vue | Svelte 5 | Solid |
| --- | --- | --- | --- | --- |
| 更新模型 | 递归重渲染子树（`W3D-038`），官方承认高层更新「not optimal」（`W3D-039`） | 运行时 VDOM + 编译期提示：patch flags 位运算「least amount of work necessary」（`W3D-044`） | 编译器「看见引用点」→ 定向更新；signals 引擎（`W3D-047/048`） | 无 VDOM，编译为真实 DOM 节点 + 细粒度反应（`W3D-050`） |
| 协调算法 | 启发式 O(n)（两个假设），类型变⇒整树重建（`W3D-040/041`） | 纯运行时版必须全树 diff 且每次新建 vnode=「unnecessary memory pressure」（`W3D-042/043`） | —（无 diff，作用域=依赖） | 依赖图（observer 数组）+ 未变不传播（`W3D-057/058`） |
| 响应式代价 | — | 深层代理：单次渲染访问 10 万+ 属性时每访问都是陷阱（`W3D-045`）；依赖图=WeakMap 三层结构（`W3D-046`） | signals 为内部实现细节（不暴露 API）、大列表改动不必失效其他成员（`W3D-048/049`） | 同步传播（`W3D-052`），提交前用调度器把让出权还给宿主 |
| 对本项目的含义 | 「默认全子树」是纯成本模型 | 编译器提示=把可变性变成数据 | 「编译期知道」与「运行时依赖图」两条正交路 | 与自绘最同构：依赖精确到值 |

### 2.3 Solid 运行时代码（source 深读，clone `b25c5577`）

- **信号结构**：`SignalState`=值+`observers/observerSlots`（双向数组登记，读时追加订阅者）（`W3D-057`）。
- **写入路径**：先跑比较器（默认全等）——值未变则整棵下游不动（`W3D-058`）；变化时按 `pure` 分流：
  memo 类进 `Updates` 队列、副作用进 `Effects` 队列，**纯计算先算、副作用后跑**（`W3D-060`）。
- **熔断**：更新队列 > `10e5` 即抛「Potential Infinite Loop Detected.」——反应式系统的失控传播有显式上界（`W3D-059`）。
- **调度器**：源码首行自述是 **React Scheduler 的移植**（`W3D-053`）；参数：让出间隔 **5ms**（`W3D-054`）、
  无输入时上限 **300ms**（`W3D-055`）、有 `isInputPending` 则输入优先立即让出（`W3D-056`）。
- **列表**：`mapArray`（`<For>` 底座，源自 S-array）「只转换变化的值」（`W3D-061`）——JFB swap 数字的机制解释。

### 2.4 壳架构与输入路径

- Electron：多进程继承自 Chromium（`W3D-032`）；点击事件「go through the main process before it reaches your window」，
  GPU 动画的协调同样绕主进程（`W3D-033`）——输入→反馈链路上的 JS 中转是**架构性**的。
- Tauri：Core（Rust）进程不渲染，UI 由「OS 提供的 WebView 库」的 WebView 进程渲染（`W3D-020`）；
  多进程骨架与 Electron「similar」（`W3D-019`）；Windows 底座=WebView2（`W3D-022/068`，Evergreen 更新 `W3D-069`）。

## 3. 公开读数（数字必须带锚：版本/日期/口径）

### 3.1 js-framework-benchmark（chrome152 数据集；median 由原始 15 样本复算）`W3D-001..015`

| 基准（中位数，ms） | vanillajs-keyed | react-hooks-v19.2.0 | solid-v1.9.3 | svelte-v5.42.1 | vue-v3.5.39 |
| --- | --- | --- | --- | --- | --- |
| 01 创建 1000 行 | **20.6** | **23.7** | **21.4** | — | — |
| 05 交换两行（1k 表） | 10.9 | **89.9** | **12.6** | 12.6 | 13.4 |
| 42 压缩包体（KB，单值） | **2.5** | **51.4** | **4.5** | **9.7** | **23.3** |
| 22 加 1000 行内存（MB，单值） | **1.861** | **4.435** | 2.676 | 2.873 | 3.933 |
| 43 首屏（ms，单值） | **52.7** | **221.4** | **47.3** | 58.4 | 93.9 |

（加粗=有账本行；其余数字同表可复算：`python` 读 `results.json` 取 `values.DEFAULT[0]`/median。
口径：GitHub Actions runner + 官方 CPU 节流；含渲染时间（`W3D-016`）；总分自 chrome118 起为加权几何平均（`W3D-017`）。
**版本=框架名字内嵌**（v19.2.0 / v1.9.3 / v5.42.1 / v3.5.39），数据集=chrome152、commit `21d7204d`、2026-09-01。）

**读法（三条直接结论）**：①「创建」轴框架间只差 1–3ms——DOM/渲染支配，框架不是首要变量；
②「交换」轴差 7 倍——这是协调模型（实现性）的账；③包体/内存/首屏是「运行时+引擎」的账，选型前必须按轴分开算。

### 3.2 Tauri vs Electron（tauri-benchmark_results；注意**时间窗不同**）`W3D-023..031`

| 指标（hello world） | Tauri（2026-09-30 条目） | Electron（2023-09-24 条目） | 可比性 |
| --- | --- | --- | --- |
| 二进制/打包体积 | **2,838,528 B**（≈2.71 MiB） | **166,492,752 B**（≈158.8 MiB） | ✅ 结构性事实（随包引擎与否） |
| 启动耗时 mean | **0.615s**（hyperfine 3 预热+10 次） | 0.475s（同文件条目复算） | ❌ 跨 3 年/runner 代次 |
| 常驻内存 | 未测（README 口径=time -v max RSS；子进程是否计入未说明，`W3D-031`） | **476,053,504 B**（≈454 MiB） | ⚠️ 仅作量级旁证 |
| 线程数 | — | **84** | ⚠️ 同上 |
| 参考点 | wry hello world 347,136 B（`W3D-025`） | — | Rust 壳本体数百 KB 级 |

（**引用红线**：两侧数据文件最新条目相差约 3 年（`W3D-029`），任何「谁更快/更省」的跨端结论都必须带
「2023 vs 2026、不同 runner」限定；体积差不受日期影响，因为它是「打不打包引擎」的机制差。）

### 3.3 WebAssembly / Emscripten（官方源）`W3D-070..074`

- 解码/启动：wasm 二进制解码比 JS 解析快（官方 FAQ 同段给「>20×」；移动端大产物解析仍「20–40 秒」）（`W3D-070`）。
- 执行档位：Liftoff 基线产物比 TurboFan 优化产物慢 **~50%（桌面）/ ~70%（MacBook）**（Unity 基准）；
  后台优化编译还要「several more seconds」——先用慢档跑（`W3D-072/073`）。
- 体积-速度：Emscripten `-Os/-Oz` 明确以性能换体积（`W3D-074`）。

### 3.4 成本分账：架构性 vs 实现性

| 成本 | 性质 | 锚 |
| --- | --- | --- |
| Web 引擎常驻（体积基座+内存基座） | **架构性**（只要走 Web） | `W3D-026/027/068` |
| 多进程模型与 IPC 协调面 | **架构性**（Chromium 血缘） | `W3D-032/019` |
| Web 渲染税（DOM/CSS/layout/paint 排主线程 16.7ms） | **架构性**（Web 路径内） | `W3D-062/065` |
| 输入经主进程中转 | **架构性**（Electron 进程模型） | `W3D-033` |
| 框架运行时/协调算法/包体 | **实现性**（可换框架、可换粒度） | `W3D-002..010`（swap 差 7×） |
| 依赖树加载、离屏默认 CPU 拷贝、wasm 编译窗口 | **实现性**（配置/工具链可控） | `W3D-034/037/073` |

## 4. 坑与反例（负面留档）

1. **跨年数据不得当对照**：tauri-benchmark 的 Electron 文件最后条目停在 2023-09-24（`W3D-029`）——
   「Tauri 启动更快/更慢」之类跨端结论在此数据上不成立；能引的只有体积机制差（`W3D-023/026`）。
2. **官方承认的实现性缺陷**（不是我们黑的）：React「默认行为对高层更新 not optimal」（`W3D-039`）、
   整树重建路径（`W3D-041`）；Vue「每次重渲染仍新建 vnode=不必要的内存压力」（`W3D-043`）；
   离屏渲染最优路径（GPU 共享纹理）不是默认（`W3D-036/037`）。
3. **wasm 的「快」有档位**：不注明 Liftoff/TurboFan 与预热窗口的 wasm 性能引用不可比（`W3D-072/073`）。
4. **工具坑（留档）**：本机 web.dev / developer.chrome.com **连接超时不可达**（所以浏览器成本章用 MDN 替代）；
   curl 访问 `raw.githubusercontent.com` 失败于 schannel 吊销检查（CRYPT_E_NO_REVOCATION_CHECK），
   加 `--ssl-no-revoke` 即恢复 200——本次全部 GitHub raw 复算均以此方式验证。
5. **口径坑**：tauri-benchmark 内存口径=time -v max RSS，README 未说明是否涵盖 WebView2 子进程（`W3D-031`）——
   引用该数字与 Electron 内存对比时存在**低估 Tauri 侧**的风险，须带此限定。

## 5. 未验证项（缺什么证据）

- **web.dev / Chrome 官方文章未覆盖**（本机不可达）：其 DOM/Canvas/动画成本数字未能进入账本（改用 MDN 性能页替代）。
- **JFB 官方交互页为纯 JS 渲染**：未用无头浏览器执行页面；表中 median 为对仓库原始样本的复算，
  **未逐项比对**官方表格显示值（算法一致：官方 stats 库 median=排序取中）。
- **未运行任何被测应用**（只读纪律）：Electron/Tauri 的内存/启动未在本机复测；Mineradio（`w4b`）仅只读引用。
- **Tauri 侧 2026 数据 vs Electron 侧 2023 数据**：跨时间窗不可比（§3.2 已标注）；等 Electron 流水线更新后需重取。
- **JFB 的框架版本=名称内嵌**：数据文件不含「框架 commit」清单，版本粒度到 semver（如 v19.2.0）。
- **Solid 调度器移植的差异面**：只读了 `scheduler.ts`/`signal.ts`/`array.ts` 三文件；与 React 原版的行为差异未逐项对照。
