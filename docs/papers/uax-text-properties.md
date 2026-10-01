# UAX #9 / #11 / #50：bidi、东亚宽度、竖排的文本层细节

> 锚：`W1F-019..032`。来源：<https://www.unicode.org/reports/tr9/>（Unicode 18.0.0, 2026-09-01）、
> <https://www.unicode.org/reports/tr11/>（Unicode 18.0.0, 2026-07-31）、<https://www.unicode.org/reports/tr50/>。
> 分工：w3f 已引 tr9/tr11/tr50 的入口条目；本文件给**属性定义与流水线顺序**的细节。

## 1. UAX#9：顺序即架构

- 内存表示=逻辑序（`W1F-019` "memory representation order known as logical order"）。
- **规范流水线**："The characters are shaped into glyphs according to their context (taking the
  embedding levels into account for mirroring). The accumulated widths of those glyphs (in logical
  order) are used to determine line breaks. For each line, rules L1–L4 are used to reorder…"
  （`W1F-020/021`）——**整形（含镜像）→ 量宽 → 断行 → 行内重排**，顺序不可交换。
- 解释序不变：显示序只是显示（`W1F-022`）；RTL 文种清单与"显示序≠自然序"（`W1F-023`）。

**对 LSSMJ**：单行 + 截断场景只需"逻辑序存储 + 显示序绘制"的分界；命中表用逻辑序区间即可天然支持
RTL（`W1F-022`）。多行面板阶段再补 L1–L4。

## 2. UAX#11：宽度不是字符的绝对属性

- 规范概念="固有宽度"（inherent width，`W1F-024`）；二值基准 narrow/wide（`W1F-027`）。
- **Ambiguous 类**："Their default width property is considered ambiguous and needs to be resolved into
  an actual width property based on context."（`W1F-025`）——同一码点在东/非东上下文宽度不同。
- 历史对应：narrow≈单字节、wide≈多字节遗留编码（`W1F-026`，与 w4a 的 GBK 点阵案例同源）。

**对 LSSMJ**：宽度判据（≤0.01pt）必须写明"按哪个 EAW 上下文解析 Ambiguous"（`W1F-025`）；
金丝雀语料要含 Ambiguous 字符（如 U+00A1 类）两套上下文各跑一遍。

## 3. UAX#50：竖排朝向是属性，不是字体行为

- 取向表（Table 1）：U="displayed upright, with the same orientation that appears in the code
  charts"（`W1F-029`）；R="sideways, rotated 90 degrees clockwise compared to the code charts"
  （`W1F-030`）。
- 默认实践："Han ideographs, Kana syllables, Hangul syllables, and Latin letters in acronyms are
  upright, while words and sentences in the Latin script are typically sideways."（`W1F-028`）。
- 少数字符要**换形**而非旋转（`W1F-031`）；标准自限："may not be publishing-material quality"
  （`W1F-032`）——验收只能承诺"默认合理"。

**对 LSSMJ**：竖排若做，起步=U/R 查表（两档覆盖候选窗场景），换形字符列为已知缺口（`W1F-031`）；
**不要把标准当质量承诺**（`W1F-032`）。
