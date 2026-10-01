# Cassowary 约束布局（TOCHI 2001 / UIST 1997 全文，+ Kiwi 重实现）

> 源：`cassowary-tochi.pdf`（Badros/Borning/Stuckey, TOCHI 8(4):267-306, 2001；DOI 10.1145/504704.504705，
> Crossref 现场核，W3B-099）与 `uist97.pdf`（Borning/Marriott/Stuckey/Xiao, UIST'97，DOI 10.1145/263407.263518）。
> 抓取 2026-10-01；PDF 文本经 PyMuPDF 抽取（含连字符断行与连字，引文以落盘文本为准）。
> 账本：W3B-001..020、022、096、097。

## 1. 文章要解决的问题（W3B-001/002/018）

- 现实 UI 约束集**常成环**且等式/不等式混用："However, in trying to apply constraint solvers to
  real-world problems, we found that the collection of constraints was often cyclic, and included both
  equalities and inequalities."（W3B-001）；作者认为避免环"contrary to the spirit of the whole
  enterprise"——声明式就该由系统兜底。
- 空白点："solvers designed for UI applications cannot efﬁciently handle simultaneous linear equations
  and inequalities."（W3B-018）⇒ 目标是"能同时解等式+不等式环，且快"（W3B-002）。

## 2. 方法：增量单纯形 + 对偶重解（W3B-003/010/014/020）

- 需求侧：交互场景要"反复解相似问题"（W3B-003；UIST 版称 incrementality 是"首要问题"，W3B-020）。
- 机制：改输入后 tableau 仍是最优但可能不可行，"can use the dual simplex algorithm to find a feasible
  solution while staying optimal"（W3B-010 的截取段）；**只做最小更新**——结论自述其快来自
  "minimal update of the tableau which is performed"（W3B-014）。
- 直觉：拖动只在"撞/离屏障"时才 pivot（W3B-010 段落；见原文 §2.4 表述）。

## 3. 偏好体系：约束层级（W3B-004/012/015/019）

- 每条约束带 strength，required 必须满足，其余强度完全支配更弱者（W3B-004）；
  UIST 版明确"preferences as well as requirements"（W3B-019）。
- 用法示例（Star 基准）：输入变量 medium stay、输出变量 weak stay、偏移上加强 edit 约束（W3B-012）。
- 应用层（CCSS）：要求/偏好两层，最终外观是**设计者与查看者意愿的仲裁**（W3B-015）。

## 4. 实测数字（2001 年口径，机器/编译器见原文）

| 场景 | 读数 | 锚 |
| --- | --- | --- |
| SCWM 加 25 窗 | 3.6 ms/窗（8 条初始约束）+ 0.14 ms/次重解（72 次） | W3B-006 |
| 二叉树高 7（1015 约束）拖动根 | 41.4 ms 重解+刷新，其中 33% 在求解器 | W3B-005 |
| 全部交互场景 | 刷新+输入处理耗时**压过**求解耗时 | W3B-009 |

## 5. 反例（比正面更值钱，W3B-007/008/011/013）

- 局部传播对照：同问题下 Cassowary 慢一个数量级（重解），初始解更慢（W3B-007）；
  作者因此自荐**混合求解器**架构（局部传播 + 线性求解器，W3B-008）。
- 链基准（1000 变量）：加 edit 约束"every constraint must be touched"（W3B-011）——
  最坏情形无增量可言。
- 树基准：DeltaBlue 只触 log n 条，Cassowary 一次 pivot 但要动一条 n 规模约束（W3B-013）——
  "谁快"取决于变更形态。
- 重实现（Kiwi）自报 10–500×（典型 40×）与内存 >5× 改善（W3B-022）：**原论文数字 = 2001 年那套
  实现的数字**，不是算法成本上界。

## 6. 谱系（题录层，W3B-096/097）

- SkyBlue（Sannella, UIST'94，DOI 10.1145/192426.192485）是 Cassowary 对比基线之一（W3B-096）。
- Constraint hierarchies（Borning/Freeman-Benson/Wilson, 1992，DOI 10.1007/bf01807506）是强度理论的出处
  （W3B-097，Crossref 检索记录锚）。

## 7. 对 LSSMJ 的取舍

- **弃**：通用约束求解器（候选窗不需要环/不等式能力；最坏情形成本见 W3B-011/013）。
- **取**：最小更新原则（W3B-014→设计 §4.3/§6 的"内容不变=0 工作"）；两级强度（硬锚点/软回退，
  W3B-004/012）；"先量瓶颈"纪律（W3B-009 的反直觉读数：瓶颈常在重画而非布局）。
- **留档**：若大面板出现真环状依赖，才重启约束路线，并以混合架构（W3B-008）为先验。

## 8. 未验证

- TOCHI 的电子附录（ACM DL）未取；§2.6 删约束的增量过程未细读（本批只做主题级覆盖）。
- SkyBlue / constraint hierarchies 仅题录（W3B-096/097），内容未读。
