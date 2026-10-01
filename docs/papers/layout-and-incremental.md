# 布局算法与增量计算（论文/文档批，C2）

> **范围**：约束布局（Cassowary 系）、W3C 布局规范（Flexbox/Grid/CSS Text 4）、布局引擎（Yoga/Taffy/Flutter）、
> 增量计算理论（Adapton/Acar 线/Differential Dataflow/Naiad）、失效与批处理（Salsa/React/Svelte/Solid/Qt）、
> 断行质量（Knuth-Plass 之后）。
> **抓取日期**：2026-10-01（本文件所有数字与引文均为该日抓取的版本）。
> **账本**：`../analysis/ledger/w3b.jsonl`（**83 条**，paper 50 / doc 29 / source 4；`ledger.py verify` **0 拒绝**）。
> **方法**：`curl`/`python urllib`（本机 WebFetch 证书失败不用）；PDF 用 PyMuPDF 抽文本、HTML 去标签后落
> `scratch/w3b/txt/`；**所有引文由脚本从落盘文本按起止短语程序化截取**（空白折叠，不改字符）——引文可对
> 落盘文件复算。DOI 一律 Crossref API 现场核（两处错引被拦，见 §7）。
> **版本锚（"上游 commit"对本文档批的等价物）**：taffy 克隆 `fb461a7826e49f488f31220744bf12227ffb580e`
> （其中基准快照锚 taffy `71027a8` / yoga crate `0.4.0`，`w3b` W3B-041/042）；React 文档页版本 `v 19.3`；
> W3C 规范取 2026-10-01 的 drafts.csswg.org 当次快照；qt-6 在线文档同次抓取。

## TL;DR（10 条，每条带锚）

1. **Cassowary 的真问题是"增量"，不是"约束"**——"For interactive graphical applications, we need to solve
   similar problems repeatedly, rather than solving a single problem once."（W3B-003）；其快来自
   "minimal update of the tableau"（W3B-014）。→ 我们照抄的是**最小更新原则**，不是求解器。
2. **约束法的最坏情形是结构性的**：链基准里"every constraint must be touched"（W3B-011），同一问题上
   Cassowary 比局部传播求解器慢一个数量级（W3B-007）——通用性是用代价换的。
3. **强度分层是"要求 vs 偏好"的正规机制**：required/strong/weak 三层 + medium stay 实例（W3B-004、
   W3B-012、W3B-019），并用于设计者/查看者意图**仲裁**（W3B-015）。我们只需两级。
4. **平台布局的共识形态=两遍法 + 取整**：WPF "a measure pass and an arrange pass"（W3B-024）、Flutter
   "Constraints go down. Sizes go up. Parent sets position."（W3B-028）、WPF layout rounding（W3B-025）。
5. **W3C 布局规范只保证结果不保证步骤**："The algorithms here are written to optimize readability and
   theoretical simplicity, and may not necessarily be the most efficient."（W3B-030）——合规空间=换实现。
6. **基准数字必须带口径与版本**：taffy 基准"measure layout computation only. They do not measure tree
   creation."（W3B-039），宽树 -82% 一行原文 135.78/241.34/247.42 ms（W3B-043），版本=0.3@`71027a8`、
   M1 Pro、criterion 10 迭代（W3B-041、W3B-042）、sample_size(10)（W3B-044）——**w1b 的 -82% 至此有完整复算口径**。
7. **增量引擎的收益与代价都会极端化**：Adapton 惰性场景 7×–2000×（W3B-063），但全量需求时**慢于**传统 IC
   1.5–3.5×（W3B-064），mergesort 上传统 IC 反慢 6.5×（W3B-065）；正确性承诺=与从头重算一致（W3B-070）。
8. **差分法的"保留史"很便宜**：保留 24h 窗口全部迭代差分只比增量数据流多 1.5% 状态（W3B-079）；
   滑窗更新工作量=全量重评的 0.003%（W3B-080）；未变部分状态直接复用（W3B-082）。
9. **失效复用的前提是纯函数 + 红绿判定**：Salsa "purely deterministic function of its inputs"（W3B-049）、
   red-green（W3B-048）、rust-analyzer 全局修订号+计算取消（W3B-050）；React 则是批处理（W3B-053）
   + "只改差异"（W3B-052）；细粒度派（Svelte/Solid）把失效粒度压到单项（W3B-055/057）。
10. **断行质量在 KP 之后仍有可立项的工程进展**：Knuth-Plass Revisited（DocEng 2015，W3B-088）、
    "相似性缺陷"扩展（DocEng 2024，"simple and lightweight"，W3B-090/091）、线性化理论依据
    （Wilber 1988，W3B-093；Hirschberg-Larmore 1987，W3B-094）；CSS 已把它做成属性并**带阈值**
    （>10 行可退回 auto，W3B-033）。

## 可吸收 / 不可吸收（对 LSSMJ 布局内核 + 失效三段）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 最小更新原则（只改变化状态） | W3B-014、W3B-082、W3B-080 | **吸收**——失效三段/显示列表复用的总纲 |
| 两遍布局（约束下传/尺寸上收）+ 纯函数内核 | W3B-024、W3B-028、W3B-049 | **吸收**——即设计 §6 的 `layout(content, theme, constraints)` |
| 布局输出对齐设备像素（layout rounding） | W3B-025 | **吸收**——@2x 文字/边框清晰度的前置条件 |
| 强度只留两级（硬锚点/软回退）+ 显式优先级仲裁 | W3B-004、W3B-012、W3B-015 | **有界吸收**——不建三层强度体系，仅两级 |
| Flexbox 的 grow/shrink 分配语义（结果层） | W3B-029 | **有界吸收**——借语义、不借规范全过程；限候选行/工具条 |
| 规范即验收基准（实现可换、结果须同） | W3B-030 | **吸收**——差量算法允许，但以规范结果验收 |
| 文本断行档位：默认快 / 质量档可开 + n≤10 行阈值 | W3B-032、W3B-033、W3B-034 | **吸收**（P2 后）——候选窗恰在阈值内 |
| 浏览器当布局 oracle（fixture→Chrome 期望值） | W3B-036 | **有界吸收**——用于宽度/换行金丝雀，不引入浏览器依赖 |
| tile/描述符双缓冲 + 未变复用（保留史便宜） | W3B-079、W3B-080、W3B-082 | **吸收**——判据 C3 的机制依据 |
| 行级失效粒度（列表单项不连坐） | W3B-055、W3B-057 | **吸收**——候选行=失效单元 |
| 帧内批处理（多次标脏合并到帧末） | W3B-053 | **吸收**——与 Slate/UGUI 共识一致 |
| Salsa 式增量引擎（红绿/修订号/依赖记录） | W3B-046..050 | **不可吸收**（当前）——单线程候选窗不需要运行时；只借"纯函数+显式依赖"前提 |
| Cassowary 通用约束求解器 | W3B-007、W3B-011 | **不可吸收**——最坏情形触及全部约束；候选窗无此需求 |
| Yoga/Taffy 作布局内核 | W3B-039、W3B-040、W3B-043 | **不可吸收**（候选窗级）——taffy 数字不含文本测量且宽树回归；大面板阶段按 ADR-09 复测 |
| Differential Dataflow / Naiad 运行时 | W3B-083、W3B-085 | **不可吸收**——背数据流运行时；只取"差分集合+版本"概念 |
| Adapton 式惰性精化引擎 | W3B-064 | **不可吸收**——全量需求时负收益，与"每帧全窗合成"形态冲突 |
| Knuth-Plass 全量重排（默认档） | W3B-034、W3B-088 | **不可吸收**（默认）——计算贵；仅质量档 + 短块 |

## 1. 约束布局：Cassowary 及其谱系（W3B-001..020、022、027、028）

- **动机**：真实 UI 约束集"often cyclic, and included both equalities and inequalities"（W3B-001），
  而当时的 UI 求解器"cannot efﬁciently handle simultaneous linear equations and inequalities"（W3B-018）；
  能处理等式+不等式环的求解器"is thus highly desirable"（W3B-002）。
- **增量三操作**：改输入值（拖动）→ 对偶单纯形重解，只在部件首次撞上/首次离开屏障时才 pivot
  （W3B-010 段落的对偶起点描述）；加约束走 Section 2.5 的增量路径。UI 版把增量列为"首要问题"（W3B-020）。
- **强度体系**：每条约束带 strength；required 必须满足，其余按层级支配（W3B-004）；UIST 版强调
  "preferences as well as requirements"（W3B-019）；Star 基准把它用成 medium stay/weak stay（W3B-012）。
- **实测数字（2001 年、机器口径见原文）**：SCWM 加 25 窗 3.6 ms/窗 + 0.14 ms/次重解（W3B-006）；
  二叉树 1015 约束拖动 41.4 ms/次、其中 33% 在求解器（W3B-005）；**结论句**："time to refresh the
  picture and handle input dominated the constraint satisfaction time"（W3B-009）——瓶颈不在求解。
- **反面**：局部传播对照慢一个数量级（W3B-007）；链基准"触及全部约束"（W3B-011）；树基准里
  DeltaBlue 只触 log n 条、Cassowary 动一条 n 规模约束（W3B-013）；作者自荐混合架构（W3B-008）。
- **重实现空间**：Kiwi 自报比原版快 10–500×、典型 40×、内存省 >5×（W3B-022）——算法同、工程不同，
  **切不可把原论文数字当算法成本上界**。
- **谱系与平台**：SkyBlue（UIST'94，W3B-096）与 constraint hierarchies（1992，W3B-097）是前置概念；
  WPF 把布局做成两遍递归系统（W3B-024）并按需取整（W3B-025）；Apple 的 Auto Layout 是约束式
  （W3B-027）；Flutter 用约束三律表达两遍法（W3B-028）。
- **取舍（对 §6 布局内核）**：候选窗**不用约束求解器**——形态固定（锚点+排列格式）可覆盖 95% 场景；
  求解器留作"大面板极端形态"的候选，且引用其最坏情形数字（W3B-011/013）做否证材料。

## 2. 规范与引擎：Flexbox / Grid / CSS Text 4 / Yoga / Taffy（W3B-029..044）

- **Flexbox**：核心=让项目"flex"分配自由空间（W3B-029）；规范算法明言"可能不是最高效"（W3B-030）。
  结果层语义（flex base size、grow/shrink 比例分配）可直接用作候选行/工具条的宽度分配。
- **Grid**：轨道尺寸算法"calculates from the min and max track sizing functions the used track size"
  （W3B-031）——min/max 双函数模型对我们"测量值/上限"双值有直接借鉴价值。
- **CSS Text 4**：把断行策略变成"速度/质量/稳定性"取舍（W3B-032）；`balance` 明确 **>10 行可退回 auto**
  （W3B-033）；`pretty` 警告"computations may be expensive"并要求作者评估（W3B-034）。
- **Yoga**：定位"portable layout engine targeting web standards"（W3B-037）；工程金矿是其**测试法**：
  HTML fixture 渲染进 Chrome 取期望布局（W3B-036）——我们可复用为布局宽度金丝雀。
- **Taffy 基准（w1b 的 -82% 完整口径）**：口径="layout computation only"、不含建树（W3B-039）；
  规模参照"3,000 and 10,000 nodes"且两库都不含文本布局（W3B-040）；数据源=仓库内
  `benches/results-2023-02-08.md`：Taffy 0.3+Yoga 出自 commit `71027a8`、yoga crate 0.4.0、
  M1 Pro、criterion(10 iterations)（W3B-041/042）；wide/100k 行=135.78(Yoga) / 241.34(Taffy 0.2) /
  247.42(Taffy 0.3) ms ⇒ **0.3 相对 Yoga 慢 ≈82%，且 0.2→0.3 已回归**（W3B-043）；宽树基准
  `sample_size(10)`（W3B-044）说明单点方差大。
- **取舍（对 §6 布局内核）**：先自研；taffy 复测的**验收形态**固定为"宽而浅 + 含文本测量 + 本机"，
  且与其"深树/超深树"优势无关（深树我们场景不出现）。

## 3. 增量计算理论：Adapton / Acar 线 / Differential Dataflow（W3B-063..086）

- **Adapton（PLDI'14，全文）**：DCG 记录"inner layer computations"（W3B-066）；脏化/传播只沿必要边
  走、成本跨调用摊销（W3B-067）；收益 7×–2000× vs 全量、传统 IC 2×–20×（W3B-063）；**全量需求时
  -1.5×~-3.5×**（W3B-064）；mergesort 上传统 IC -6.5×/Adapton +300×（W3B-065）；AS2 表格里
  经典 IC 一直 slowdown（最高 100×）（W3B-071）；正确性=补丁结果与重算一致（W3B-070）；实验口径
  OCaml 4.00.1 / 8 核 Mac Pro / macOS 10.6.8（W3B-069，时代久远，只作方向性证据）。
- **Acar 线**：Adaptive functional programming（POPL'02，W3B-072）→ Imperative self-adjusting
  computation（POPL'08，W3B-073）→ Incremental computation with names（OOPSLA'15，W3B-074——"同名即
  同缓存"对候选行**按语义身份记忆化**有直接启发）。
- **Differential Dataflow（CIDR'13，全文）**：把增量推广到**偏序版本集**+保留索引化更新史
  （W3B-077、W3B-078）；批评既有 IVM 三种失败模式（W3B-081）；保留史只多 1.5% 状态（W3B-079）；
  输入微变工作量=0.003% 全量（W3B-080）；未变部分复用（W3B-082）；实现载体 Naiad（W3B-083）——
  Naiad（SOSP'13，W3B-084）、timely dataflow（CACM'16，W3B-085）、DBToaster（VLDB'12，W3B-086）
  为谱系记录。
- **取舍（对 §6 失效三段）**：只吸收三条——(a) 变化以**差分集合**表达（行增/删/改 = 三种差分）；
  (b) 保留上一帧产物（保留史便宜，W3B-079）；(c) 语义身份做键（W3B-074）。**不引**任何数据流/响应式运行时。

## 4. 失效与批处理：Salsa / rust-analyzer / React / Svelte / Solid / Qt（W3B-046..061）

- **Salsa book**：目标=高效增量重算（W3B-046）；主循环模型=改输入→重调（W3B-047）；red-green 失效
  （W3B-048）；前提=**纯确定函数**（W3B-049）。
- **rust-analyzer**：salsa 的工程化代价=全局修订号 + 变更等待/取消协议（W3B-050）——单线程出帧可
  整段省掉。
- **React 19**："只改差异才动 DOM"（W3B-052）+ React 18 自动批处理（W3B-053）——与我们"帧末统一重建"
  同构。
- **Svelte/Solid**：信号把失效压到单项（"changes to a value inside a large list needn't invalidate
  all the other members of the list"，W3B-055）；Solid 文档直接点出与 React 的粒度差（W3B-057），
  memo=带缓存的信号（W3B-058）——设计 §6 "最小脏传播"的对照系。
- **Qt Scene Graph（补 w2d 的节点/材质锚）**：官方自纠"more precise definition is node tree"、
  节点不含主动绘制（W3B-060）；API"low-level and focuses on performance"（W3B-061）——与 DisplayList
  的数据/执行分离同构。
- **取舍（对 §6 失效三段）**：Hierarchy/Layout/Paint 三段缓存、帧末重建、Volatile 逃生阀保持不变；
  新增两条硬约束：布局函数必须纯（W3B-049 式前提）、失效遍历记账（只走必要边，W3B-067 式摊销）。

## 5. 断行质量：Knuth-Plass 之后（W3B-088..094、033、034）

- **Knuth-Plass Revisited**（DocEng 2015）：KP 的柔性断行改进（W3B-088，全文未获，仅元数据）。
- **相似性缺陷**（DocEng 2024）：相邻行首尾同词/同字序=缺陷（W3B-089）；扩展"simple and lightweight,
  making it a useful addition to production engines"（W3B-090）；自动检测/规避+保留人工决定权
  （W3B-091）；实验自评"worth addressing and achievable"（W3B-092）。
- **理论加速线**：Wilber 1988（W3B-093）与 Hirschberg-Larmore 1987（W3B-094）——KP 的 DP 可优化，
  但均为理论层，落地前须先量（本仓"先量后改"）。
- **规范层对策**：CSS 已经把质量档做成属性并带阈值/性能警告（W3B-032/033/034）——这是"质量档必须
  可关闭、要有阈值"的现成判据。
- **取舍（对 §5 文本/§6 布局）**：默认贪心（快、稳定），质量档只在短块（≤10 行）开启，先做
  `text-wrap: pretty` 式的"减少行长差异/避免过短末行"，相似性规避列为 P4 候选（需先立项测量）。

## 6. 对 LSSMJ 的三条落点（设计 §6 对照）

1. **布局内核**：接口=纯函数 `layout(content, theme, constraints) -> DisplayList`；形态=锚点+排列格式
   （弃约束求解器，W3B-007/011/013 是最坏情形证据）；宽度分配借 flexbox 结果语义（W3B-029）；
   输出做设备像素取整（W3B-025）。
2. **失效三段**：内容版本号（复用=0 工作，W3B-047 式）→ tile 描述符双缓冲比较（保留史便宜，
   W3B-079）→ 空 damage 跳过合成（W3B-052 式"差异才提交"）；行级失效粒度（W3B-055/057）；
   帧末批处理（W3B-053）。
3. **状态与响应式**：只做"最小脏传播"（信号→标脏→汇总两段）；不引入 Salsa/Adapton/DD 任何运行时
   （W3B-046..050、063..086 的取舍结论）；语义身份做缓存键（W3B-074）。

## 7. 反例与坑（负面留档）

- **错 DOI 会把主张挂到别的论文上**：本批两次拦下——`10.1145/2594291.2594326` 实为
  "Verification modulo versions"（非 Adapton，W3B-100 给出正确 DOI `…2594324`）；Cassowary TOCHI
  也不是凭记忆的 `10.1145/371056.371060`，而是 `10.1145/504704.504705`（W3B-099）。
- **"先进引擎处处更快"是假的**：Adapton 全量需求慢 1.5–3.5×（W3B-064）；taffy 0.3 在宽树比 0.2
  还慢（W3B-043 的 241.34→247.42）。
- **窄口径数字不可外推**：taffy 基准不含文本测量/建树（W3B-039/040）；Cassowary 的 2001 年机器
  口径不可搬到今天（W3B-006/041）。
- **规范算法 ≠ 高效实现**（W3B-030）——规范步骤可读性优先，实现必须自证效率（本仓"判据先于改动"）。
- **平衡断行有硬阈值**：>10 行可退回 auto（W3B-033）——别忘了实现该退化路径。

## 8. 未验证项（写明缺什么证据）

1. **Acar 学位论文（CMU-CS-05-129）未取到**：csd.cmu.edu 超时/证书失败、reports-archive 拒连、
   archive.org 超时 ⇒ 本批以 POPL'02/08 与 ICN'15 论文（W3B-072..074）替代其学术位置，**论文本体未读**。
2. **Knuth-Plass Revisited 全文未读**（无开放版，S2 无摘要）：W3B-088 仅锚题录；其"flexible"具体机制待补。
3. **Naiad / timely / DBToaster 仅读 Crossref 记录**（W3B-084..086）：系统实现细节未读，结论只到"谱系存在"。
4. **Wilber / Hirschberg-Larmore 仅题录**（W3B-093/094）：算法改进的具体常数与适用条件未读。
5. **SkyBlue / constraint hierarchies 仅题录**（W3B-096/097）：概念年代已核，内容未读。
6. **Yoga 官网仅取到标签句**（JS 壳，W3B-037）；Yoga 的算法文档（Yoga vs 规范差异）未取。
7. **Qt 批/节点节的深度**：w2d 已覆盖渲染器文档锚，本批只补节点/材质两条（W3B-060/061）……
8. **taffy 本机复测未做**（本批只读）：-82% 的适用性判断待"宽而浅+文本"复测（设计 §10 未决项 1）。

## 9. 来源清单（**23 个内容源**=21 在线 + 2 仓内文件；另 14 个元数据 API 锚（Crossref/S2）；抓取 2026-10-01，HTTP 200）

| 来源 | 类型 | 账本条目 |
| --- | --- | --- |
| constraints.cs.washington.edu/solvers/**cassowary-tochi.pdf**（TOCHI 2001 全文） | paper | W3B-001..015、099 |
| constraints.cs.washington.edu/solvers/**uist97.pdf**（UIST'97 全文） | paper | W3B-017..020 |
| raw.githubusercontent.com/nucleic/**kiwi**/README | doc | W3B-022 |
| learn.microsoft.com/**wpf layout** | doc | W3B-024、025 |
| developer.apple.com/**Auto Layout Guide**（archive） | doc | W3B-027 |
| docs.flutter.dev/**ui/layout/constraints** | doc | W3B-028 |
| drafts.csswg.org/**css-flexbox-1** / **css-grid-1** / **css-text-4** | doc | W3B-029..034 |
| raw.githubusercontent.com/facebook/**yoga**/README + yogalayout.dev | doc | W3B-036、037 |
| raw.githubusercontent.com/DioxusLabs/**taffy**/README + 仓内 `benches/results-2023-02-08.md` + `benches/benches/flexbox.rs` | doc+source | W3B-039..044 |
| salsa-rs.github.io/**salsa**/overview | doc | W3B-046..049 |
| rust-analyzer.github.io/**architecture** | doc | W3B-050 |
| react.dev/**learn/render-and-commit**、**blog/react-v18** | doc | W3B-052、053 |
| svelte.dev/**blog/runes** | doc | W3B-055 |
| docs.solidjs.com/**fine-grained-reactivity** | doc | W3B-057、058 |
| doc.qt.io/**qtquick-visualcanvas-scenegraph** | doc | W3B-060、061 |
| mhicks.me/papers/**adapton-submit.pdf**（PLDI'14 全文） | paper | W3B-063..071 |
| cidrdb.org/cidr2013/**CIDR13_Paper111.pdf**（Differential Dataflow 全文） | paper | W3B-076..083 |
| api.crossref.org ×11（POPL'02/08、ICN'15、Naiad、timely、DBToaster、SkyBlue、Hirschberg、Cassowary TOCHI、Adapton PLDI、constraint-hierarchies 检索） | 元数据 | W3B-072..074、084..086、094、096、097、099、100 |
| api.semanticscholar.org ×3（KP-Revisited、KP-Similarity、Wilber） | 元数据+摘要 | W3B-088..093 |

**复算入口**：`python tools/ledger.py verify --file docs/analysis/ledger/w3b.jsonl`（0 拒绝）；
引文原文落盘在 `scratch/w3b/txt/`（PDF/HTML 抽取件，空白折叠后与账本 quote 一致）。
