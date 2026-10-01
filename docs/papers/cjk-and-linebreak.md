# CJK 断行与分段：JLREQ + ICU + libunibreak + CSS Text 4

> 锚：`W1F-033..037,084..086,089,093`。来源：<https://w3c.github.io/jlreq/>、
> <https://unicode-org.github.io/icu/userguide/boundaryanalysis/>、
> <https://raw.githubusercontent.com/adah1972/libunibreak/master/README.md>、
> <https://drafts.csswg.org/css-text-4/>。
> 与 w3f 的分工：w3f 已有 UAX#14 的 ID 类语义与 clreq 的"先挤进后推出"；
> 本文件补**术语源流、实现约束、平台化命名**。

## 1. JLREQ（W3C，JIS X 4051 的国际版）

- 权威源："mainly based on a standard for Japanese layout, JIS X 4051"（`W1F-034`）。
- 禁则处理（kinsokushori）定义：行头/行末禁则、分割禁止等的总称（`W1F-033`）；
  术语表另给"行末禁则"（line-end prohibition rule）的定义锚（`W1F-037`，引 JIS Z 8125）。
- 行内注（割注）断行判据：断点合法 + **两行尽量等长、第二行不长于第一行**（`W1F-035`）
  ——可直接作为"长候选两行显示"的验收。
- 覆盖范围含 ruby（注音）与和欧混植（`W1F-036`）——CJK 专项的两大难点在标准内有正式章节，
  不需要自创规则。

## 2. 断行的实现约束（libunibreak）

- "Some rules (like LB28a and the quotation rules of UAX #14) require looking ahead"（`W1F-086`）
  ——**断行器不能写成"只看相邻两字符"的单遍贪心**；接口要留前瞻窗口。
- 该库支持 Unicode 17.0 的断行/断词/字素（同 README）。

## 3. 断词与分段（ICU）

- 字素边界=UAX#29 规则，"try to match what a user would think of as a character"（`W1F-089`）。
- **词典路径**：中日泰高棉文的断词"supplemented by a word dictionary"（`W1F-084`）
  ——无词典则无法做词级断行。
- 同页还有"Line Breaking Strictness, a CSS Property"一节，把 CSS 的严格度分级接进实现。

## 4. 断行质量进入 Web 规范（CSS Text 4）

- `text-wrap-style: auto | balance | stable | pretty | avoid-short-last-line`（`W1F-085`）
  ——"平衡/美观断行"已有规范命名，我们的质量档位可直接对齐这组词。

## 对 LSSMJ 文本层的取舍

1. 单行 + 截断档**不需要**断行器；但接口要给未来留位（`W1F-086` 的前瞻约束影响接口形状）。
2. 若做 CJK 断行：禁则分"行头/行末"两类建用例（`W1F-033/037`）；
   长候选两行显示用割注判据（`W1F-035`）；词级断行须接词典（`W1F-084`）。
3. 断行质量档位命名对齐 CSS Text 4（`W1F-085`），避免自造词造成文档迁移成本。
4. 判据纪律：CJK 断行的"正确"=规则集可枚举复算（UAX#14+JLREQ 条款），不写"看起来对"。
