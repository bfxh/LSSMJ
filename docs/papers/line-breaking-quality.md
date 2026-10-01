# 断行质量：Knuth-Plass 之后的改进工作（题录 + 摘要层）

> 源（2026-10-01）：Semantic Scholar API 记录 ×3（KP-Revisited、KP-Similarity、Wilber）、
> Crossref 记录（Hirschberg-Larmore）、`drafts.csswg.org/css-text-4/`。
> 账本：W3B-088..094、032..034。**注意：本批论文均未取得全文**，结论只到"题录/摘要可核实"的强度。

## 1. 谱系与本文档批拿到的证据等级

| 工作 | 年/会 | 本次证据 | 锚 |
| --- | --- | --- | --- |
| Knuth & Plass, Breaking paragraphs into lines | 1981 SPE（既有序曲，w3f W3F-001） | 已有 DOI | — |
| Hirschberg & Larmore, The Least Weight Subsequence Problem | 1987 SIAM J. Comput. | Crossref 题录 | W3B-094 |
| Wilber, The Concave Least-Weight Subsequence Problem Revisited | 1988 J. Algorithms | S2 题录（无摘要/无 OA） | W3B-093 |
| Hassan & Hunter, **Knuth-Plass Revisited: Flexible Line-Breaking for Automatic Document Layout** | 2015 DocEng | S2 题录（无 OA） | W3B-088 |
| **Similarity Problems in Paragraph Justification: An Extension to the Knuth-Plass Algorithm** | 2024 DocEng | S2 摘要（有 OA 链接但落 ACM DL） | W3B-089..092 |

## 2. 2024 相似性扩展（本批唯一读到摘要的"KP 之后"工作）

- 缺陷定义："In high quality typography, consecutive lines beginning or ending with the same word or
  sequence of characters is considered a defect."（W3B-089）——相邻行首尾同词/同字序。
- 工程自评："The extension is simple and lightweight, making it a useful addition to production
  engines."（W3B-090）——定位是生产引擎可加装的小改。
- 交互形态："Our extension automates the detection and avoidance of similarities while leaving the
  ultimate decision to the professional typographer, thanks to a new adjustable cursor."（W3B-091）。
- 效果自评："Experimentation shows that getting rid of similarities is both worth addressing and
  achievable."（W3B-092）——**作者自评口径**，未经我们复算。

## 3. 理论加速线（题录，W3B-093/094）

- Wilber（1988）与 Hirschberg-Larmore（1987）给出"least weight subsequence"类问题的加速——KP 断行
  正是其中之一。⇒ 若断行成为热点，**算法层有近线性升级路径**（先量后改，不得直接采信年代久远的
  复杂度声明代替实测）。

## 4. 规范层的工程判据（W3B-032/033/034）

- 三分口径："trading off between speed, quality and style of layout, or stability."（W3B-032）。
- 阈值：`balance` 超十行可退回 auto（W3B-033）——**质量档要有"退化回快速档"的显式出口**。
- 性能警告：`pretty` 计算贵、作者须自评（W3B-034）——质量档默认关闭 + 有测量背书。

## 5. 对 LSSMJ 的取舍

- **默认**：贪心/快速断行（候选窗延迟敏感）；
- **质量档（P2 之后）**：短块（≤10 行）+ 只在内容稳定时启用；先实现"减少行长差异/避免过短末行"
  这类 `pretty` 式改善，**相似性规避列为 P4 候选**（W3B-089..092 提供了可立项的形态与自评效果，
  但需按本仓测量协议复测收益）。
- **不做**：全量 KP 重排作默认路径（W3B-034 的代价警告 + 我们文本短、收益窄）。

## 6. 未验证（本文件全部结论的证据上限）

1. KP-Revisited（W3B-088）**机制未读**：其"flexible"具体做法（可伸缩字距？多目标？）不明。
2. 相似性扩展（W3B-089..092）**只读摘要**：算法细节、代价、失败案例均未读；OA PDF 在 ACM DL（未取）。
3. Wilber / Hirschberg-Larmore（W3B-093/094）**仅题录**：复杂度结论与适用前提未经本次复核。
4. 未做任何本机断行质量/耗时测量。
