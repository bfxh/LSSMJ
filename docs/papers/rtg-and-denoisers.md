# RTG 书目与降噪谱系补挖（第五轮）

> 补齐已登记缺口（RTG I/II 全文书目面、ReSTIR/SVGF/BMFR 原文锚）。
> 账本：`../analysis/ledger/w6j.jsonl`（8 条）；与 `raytracing.md`（w6b）合并阅读。

## TL;DR（带锚）

1. **RTG I/II 的官方主题清单本身就是技术地图**：RTG I="效率/采样/混合光栅-RT/GI…"（W6J-003）；RTG II 把"**降噪与滤波**"升为独立主题（W6J-004）——**两年间议程变化=降噪升格**。
2. **ReSTIR 官方页可作一手锚**（标题+JSON 元数据双形态，W6J-001/002）——NVIDIA Research 页面结构也是"机器可核验"的样本。
3. **降噪三谱系**：滤波（SVGF，W6J-006）、回归（BMFR，W6J-005）、机器学习（RTG II 的 neural 主题，W6J-004 语境）。
4. **缺口如实**：NRD 与 RTXGI 的 README 两路抓取 404（W6J-007/008）——SDK 侧文档未达，但 DDGI 论文与 RTG 书目锚仍有效。
5. **对 LSSMJ**：本批全部归场景高配档（P7）；**Open Access 边界明确**（RTG II 可全文引用，W6B-011），与闭源 SDK 红线形成对照。

## 书目表（本轮终态）

| 来源 | 形态 | 锚 |
| --- | --- | --- |
| Ray Tracing Gems I（2020） | Springer 页 + 主题清单 | W6B-008/009、W6J-003 |
| Ray Tracing Gems II（2021，**Open Access**） | Springer 页 + 主题清单 | W6B-010/011、W6J-004 |
| ReSTIR（2020） | NVIDIA Research 页（标题+JSON） | W6J-001/002、W6B-012 |
| SVGF（2017） | Crossref | W6J-006、W6B-013 |
| BMFR | Crossref | W6J-005 |
| NRD / RTXGI（SDK） | **未达**（404） | W6J-007/008 |

## 未验证 / 缺口

- RTG 章节正文未逐章读（只到书目/主题级）——**Open Access 的 RTG II 可续读**（编号段 `w7e` 备）。
- ReSTIR 全文 PDF 未取（官方页只到标题级）；SVGF/BMFR 只到 Crossref。
- NRD/RTXGI 文档未达（两路 404）。

## 对设计的落点

- 场景档 P7（动态光）开工前的必读清单固定为：**RTG II 相关章 + ReSTIR + SVGF/BMFR**（本表全列）。
- 判据侧：降噪/时序复用类算法一律配"历史注入/失效"金丝雀（继承 `02-scene-tier.md` 判据 C7 思路）。
