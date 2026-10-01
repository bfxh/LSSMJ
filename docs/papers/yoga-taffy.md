# 布局引擎基准：Taffy vs Yoga（含 w1b「宽树 -82%」的完整口径）

> 源：`raw.githubusercontent.com/DioxusLabs/taffy/main/README.md`、仓内快照 `benches/results-2023-02-08.md`
> 与 `benches/benches/flexbox.rs`（本地克隆 taffy `fb461a7826e49f488f31220744bf12227ffb580e`）；
> `facebook/yoga` README、`yogalayout.dev`；`docs.flutter.dev/ui/layout/constraints`。抓取 2026-10-01。
> 账本：W3B-028、036、037、039、040、041..044。

## 1. 数字的口径（引用前必须先背这几条）

- **只测布局计算**："The benchmarks measure layout computation only. They do not measure tree creation."
  （W3B-039）——不含建树，**也不含文本布局**（README 明确 yoga/taffy 都不做文本，W3B-040）。
- **规模参照**：主流网站 "between 3,000 and 10,000 nodes"（W3B-040）；10 万节点场景远超我们形态。
- **版本/硬件/工具**（results 文件头）：Taffy 0.3+Yoga 出自 commit `71027a8…`；yoga 基准用 `yoga` crate
  `0.4.0`；跑在 2021 M1 Pro MacBook Pro；criterion(10 iterations)（W3B-041、W3B-042）。
- **采样**：宽树组 `sample_size(10)`（W3B-044）——样本极小，单点别当稳态。

## 2. 宽树 -82% 的复算（w1b 的上下文补齐）

W3B-043 原文一行（`benches/results-2023-02-08.md:18`）：

| Benchmark | Yoga | Taffy 0.2 | Taffy 0.3 |
| --- | --- | --- | --- |
| wide/100_000 nodes (2-level hierarchy) | 135.78 ms | 241.34 ms | 247.42 ms |

- 247.42 / 135.78 ≈ **1.822 ⇒ Taffy 0.3 比 Yoga 慢约 82%**（与 w1b 的"宽树 -82%"一致）。
- 同格 0.2→0.3 由 241.34→247.42 ms：**该回归在 0.3 线就已存在**（不是单版偶发）。

## 3. 引擎的自我描述与测试法

- Taffy：Rust 实现 CSS Block/Flexbox/Grid 三套算法（W3B-036→本报告行内引用）；
  "It currently implements the CSS **Block**, **Flexbox** and **CSS Grid** layout algorithms."
- **Yoga 的测试法最值得借**："Many of Yoga's tests are automatically generated, using HTML fixtures
  describing node structure. These are rendered in Chrome to generate an expected layout result for the
  tree."（W3B-036）——**用浏览器当布局 oracle**，与我们"金丝雀期望值锚到规则之外"的纪律同构。
- Yoga 定位："A portable layout engine targeting web standards"（W3B-037）。
- Flutter 把两遍法说成三律："Constraints go down. Sizes go up. Parent sets position."（W3B-028）——
  约束模型的规范表述，直接对齐我们纯函数内核的入参/出参契约。

## 4. 对 LSSMJ 的取舍

- **候选窗级不引 taffy/yoga**（设计 §6 既定）：我们形态是"宽而浅 + 文本为主"，而 taffy 的劣势格恰好
  是宽树，优势格（深树）我们不用。
- 若大面板阶段要评估 taffy：**必测形态**=| 宽而浅（单层 1k–1w 节点）| 含文本测量 | 本机 | 报告波动
  （对齐 W3B-039/040/044 的口径纪律）；否证条件=比自研内核慢或内存高（设计 §10 未决 1）。
- 借 Yoga 的 fixture→Chrome oracle 法做**换行/宽度金丝雀**（W3B-036），不引入浏览器运行时。

## 5. 未验证

- 本批**未做本机复测**（只读分析）：-82% 的可迁移性待复测（引文条目只到"版本化复算"）。
- taffy README 在 main 分支的数字与克隆快照可能不同步（本报告用快照文件，其头部自带版本声明）。
- Yoga 官网仅取到标签句（JS 壳）；Yoga 与规范的差异文档未取。
