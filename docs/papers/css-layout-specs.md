# W3C 布局规范：Flexbox / Grid / CSS Text 4（规范即验收基准）

> 源：`drafts.csswg.org/css-flexbox-1/`、`css-grid-1/`、`css-text-4/`（2026-10-01 快照，HTML 去标签后落盘）。
> 账本：W3B-029、030、031、032、033、034。

## 1. Flexbox：语义在"分配自由空间"（W3B-029/030）

- 定调句："The defining aspect of flex layout is the ability to make the flex items “flex”, altering
  their width/height to fill the available space in the main dimension."（W3B-029）——剩余空间按
  flex-grow 比例分配、超限按 flex-shrink 收缩。
- **规范自述可换实现**："The algorithms here are written to optimize readability and theoretical
  simplicity, and may not necessarily be the most efficient."（W3B-030）——允许实现者自选算法，
  **但结果必须一致**。这正是我们做"可缓存/增量同结果算法"的合规空间。
- 工程含义：把 flexbox 当**结果契约**（宽度分配、换行、最小尺寸），不当步骤清单。

## 2. Grid：min/max 双函数推轨道（W3B-031）

- "The remainder of this section is the track sizing algorithm, which calculates from the min and max
  track sizing functions the used track size."（W3B-031）。
- 同节（§11.3）给出的五步流程中，我们只需要前两步+最大化；min/max 函数模型可直接映射到
  我们的"测量值/上限"双值尺寸变量。

## 3. CSS Text 4：断行质量变成属性（W3B-032/033/034）

- 取舍三分："trading off between speed, quality and style of layout, or stability."（W3B-032）
  ⇒ 我们的断行档位设计（默认快 / 质量档）与规范口径一致。
- **硬阈值**：`balance` 的"exact algorithm is UA-defined. UAs may treat this value as auto if there are
  more than ten lines to balance."（W3B-033）⇒ 平衡只对短块（≤10 行）承诺，超过退回 auto。
- **性能警告**：`pretty` "The necessary computations may be expensive… Authors are encouraged to
  assess the impact on performance"（W3B-034）⇒ 质量档必须可关闭 + 需测量背书。

## 4. 对 LSSMJ 的取舍

- **吸收**：flexbox 的结果语义（候选行/工具条宽度分配）；grid 的 min/max 双值尺寸模型；
  text-4 的"档位 + 阈值 + 性能警告"三件套（照搬为我们的断行 API 形状）。
- **不吸收**：完整规范算法链（重复步骤、collapsed item strut 等）——候选窗无此形态；
  若未来做表格/多列设置面板，再按需移植（并以规范测试样例验收）。
- **验收法**：规范文本 + 测试用例即基准；实现可重写（W3B-030 的合规空间）。

## 5. 未验证

- 规范为 2026-10-01 的 ED 快照（非 CR/PR 冻结版）；引用时应复核当日版本号与状态。
- 未做浏览器实测对照（本批只读）；"宽度判据 C2 ≤0.01 pt"仍需真机对比表。
