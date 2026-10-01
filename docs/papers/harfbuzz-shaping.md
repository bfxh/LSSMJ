# HarfBuzz / rustybuzz：整形引擎的边界与默认集

> 锚：`W1F-076..078,081,082,083`。来源：
> HB 特性页 <https://harfbuzz.github.io/shaping-opentype-features.html>、
> OpenType GPOS <https://learn.microsoft.com/en-us/typography/opentype/spec/gpos>、
> Devanagari 塑造文档 <https://learn.microsoft.com/en-us/typography/script-development/devanagari>、
> rustybuzz README <https://raw.githubusercontent.com/harfbuzz/rustybuzz/main/README.md>。
> （HB 的 what/why/concepts/clusters 页与 `hb-index` 本轮抓了但未入账，可随时补条。）

## 1. 整形引擎的职责（外部权威背书"不自研"）

- Levien（`raph-text-layout.md`，`W1F-052`）："a very high quality open source implementation exists,
  in the form of HarfBuzz."——与 A2 的源码结论（cosmic-text 0.19 起用 harfrust）一致。
- 默认特性集（水平）：`calt, clig, curs, dist, kern, liga, rclt`（`W1F-081`）——**这些能力
  "选现成件即得"，不会成为我们的差异化项，也不该自研**。
- 垂直方向默认启用 `vert`（同页，未入账）。

## 2. GPOS 承载什么（我们因此免费获得什么）

- 标记定位："controlled placement of all marks in relation to one another for legibility and
  linguistic accuracy"（越南语示例，`W1F-076`）——不做 GPOS 就等于不支持这类文字。
- 连写：GPOS "cursive attachment" 用锚点连接字形（阿拉伯语系核心机制，`W1F-077`）。
- 定位的另一半在字库：legacy `kern` 表只给两字形水平间距（GSUB/GPOS 之外的唯一传统手段，
  ms-gpos 同页，未入账）。

## 3. 复杂文种的一个实例（Indic）

- 天城文整形四步：Analyze（切音节簇）→ Reorder → GSUB（形替换）→ GPOS（定位）
  （`W1F-078` 为第一步）——**簇是重排/替换的最小单位**，与 HB 的 cluster 概念对齐
  （`hb-clusters` 页已抓未入账：cluster ≠ grapheme）。

## 4. rustybuzz：与 w1b 互证的两条

- 自宣性能："At the moment, performance isn't that great. We're 1.5-2x slower than harfbuzz."
  （`W1F-082`）——**自报口径**，只能做旁证，不能单独支撑"换引擎"的决策。
- 生命周期："not developed further, unmaintained, and archived"（`W1F-083`），README 建议迁
  HarfRust——选型时避开归档件。

## 对 LSSMJ 文本层的取舍

- **吸收**：整形=现成件（harfrust/rustybuzz 接口同族）；默认特性集照抄（`W1F-081`），
  出现宽度差异先查特性开关再查字体。
- **记录**：rustybuzz 的 1.5–2× 与"归档"状态（`W1F-082/083`）写进依赖风险表——
  与 w1b 的源码读数合并成一条"整形引擎成本"条目。
- **缺口**：复杂文种（Indic/Arabic）在我们的产品形态里是否必须，未决；本文件给的是
  "若需要，代价=0（现成件已覆盖）"的结论。
