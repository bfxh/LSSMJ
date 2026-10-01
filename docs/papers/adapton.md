# Adapton 与自调整计算（PLDI'14 全文 + Acar 线题录）

> 源：`mhicks.me/papers/adapton-submit.pdf`（Hammer/Khoo/Hicks/Foster, PLDI 2014，DOI 10.1145/2594291.2594324，
> Crossref 现场核，W3B-100；抓取 2026-10-01，PyMuPDF 抽文本）。
> 关联题录：Adaptive functional programming（POPL'02，W3B-072）、Imperative self-adjusting computation
> （POPL'08，W3B-073）、Incremental computation with names（OOPSLA'15 系，W3B-074）。
> 账本：W3B-063..074、100。

## 1. 核心主张与机制（W3B-066/067/070）

- 机制=**被需求计算图 DCG**："In ADAPTON, DCGs operate behind the scenes by recording inner layer
  computations."（W3B-066）——计算留下"读过什么"的图，下次修复而非重做。
- 成本摊销："Note that, in both dirtying and propagation, we only traverse an edge if it is clean or
  dirty, respectively."（W3B-067）——只走必要边，脏化成本跨多次 set 摊销。
- 正确性=与重算等价："patching process is sound in that patched results will match what (re)computation
  from scratch would have produced."（W3B-070）——**增量不改变语义**，这正是我们要的"增量渲染=全量渲染
  逐位相同"（设计判据 C4）的理论对应物。

## 2. 收益的分布（W3B-063/065/071）

- 惰性交互："where traditional IC gets 2× to 20× speedups over naive recomputation, ADAPTON gets 7× to 2000× speedups."（W3B-063）。
- 极端案例：mergesort 上传统 IC 反而 **-6.5×**、Adapton **+300×**（W3B-065）——粗粒度记忆化会被
  输入变化击穿。
- 表格应用（AS2）：Adapton 最高 +20×，经典 IC 在同基准"always resulting in a slowdown (up to 100×)"
  （W3B-071）——**同类问题不同引擎可一快一慢**。

## 3. 代价与边界（W3B-064/069）

- **全量需求时负收益**："ADAPTON does not perform as well as traditional IC when all output is
  demanded—it can be 1.5× to 3.5× slower"（W3B-064）——脏标记/精化有固定开销。
- 实验口径为 2013 年（OCaml 4.00.1、8 核 2.26GHz Mac Pro、macOS 10.6.8，W3B-069）：只作方向性证据，
  数字不可搬到今天/搬进 Rust。

## 4. Acar 线的位置（题录，W3B-072/073/074）

- POPL'02《Adaptive functional programming》= 语言/λ 演算层起点（W3B-072）；
  POPL'08《Imperative self-adjusting computation》= 命令式（可变状态）推广（W3B-073）——**更贴我们的
  可变 Frame 形态**；OOPSLA'15《Incremental computation with names》= "同名即同缓存"（W3B-074），
  对候选行按**语义身份**记忆化有直接启发。
- Acar 学位论文（CMU-CS-05-129）三个镜像均不可达，**论文本体未读**（见主报告 §8 未验证 1）——
  语义层结论以上述论文为准。

## 5. 对 LSSMJ 的取舍

- **不可吸收**：Adapton 运行时/惰性精化引擎（与"每帧全窗合成 + 单线程"形态冲突；全量需求负收益，
  W3B-064）。
- **可吸收的三条原则**：
  1. 记录依赖图（我们=内容→显示列表的推导记录；W3B-066 同构）；
  2. 只沿必要边传播并记账（W3B-067 → 我们的"边界集"标脏）；
  3. 增量必须与全量等价且可检验（W3B-070 → 判据 C4）。
- **选型纪律**：任何增量方案先用**自有场景**复测收益（W3B-071 的例子证明"先进引擎也可能在别的
  访问模式下变慢"）。

## 6. 未验证

- 论文的增量语义形式化（§4）与 OCaml 实现细节（§5）未逐节精读（本批主题级）。
- AS2/微基准的具体表格数值（表 1）未誊录；如需引用请回原文图表。
