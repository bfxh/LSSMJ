# 短分析 · 阴影图谱系与可滤波阴影族

> 上游：Crossref / Semantic Scholar 元数据 + 各论文摘要与书章条目。
> 账本：`../analysis/ledger/w9a.jsonl` **W9A-024..037 / W9A-120..127**（paper 主体）。只读；抓取 2026-10-03。

## 谱系（按年代）

| 年 | 工作 | 贡献 | 锚 |
| --- | --- | --- | --- |
| 1977 | Crow《Models of light reflection…》 | 阴影体（shadow volume）路线源头 | W9A-026 |
| 1978 | Williams《Casting curved shadows…》 | 阴影图（shadow map）起点 | W9A-024 |
| 1987 | Reeves《Rendering antialiased shadows with depth maps》 | 阴影图 + 滤波（PCF 思想源头） | W9A-025 |
| 2006 | Donnelly & Lauritzen《Variance shadow maps》 | 把比较改成方差估计，可滤波（I3D） | W9A-027/028 |
| 2008 | Salvi《Exponential shadow maps》 | 指数化深度使阴影可滤波（GI） | W9A-029 |
| 2011 | Lauritzen《Sample distribution shadow maps》 | 按实际深度分布分配 CSM 级 | W9A-033 |
| 2011 | Scherzer 等《A Survey of Real-Time Hard Shadow Mapping Methods》 | 硬阴影方法学综述（1978 起） | W9A-035/120 |
| 2015 | Peters & Klein《Moment shadow mapping》 | 用矩重建遮挡分布，修 VSM 漏光 | W9A-031 |
| 2017 | 《Non-linearly quantized moment shadow maps》 | MSM 的量化改进 | W9A-032 |

## 关键结论

- **两条主线同时起步**：图（1978）与体（1977）同代；体路线与高模/游戏向不兼容，明确排除（`W9A-026/123`）。
- **漏光是可滤波族的结构性代价**：VSM 靠降低方差/多次比较补（`W9A-030`），MSM 用矩重建再修（`W9A-031`）。
  这是"默认档不进可滤波族"的直接理由。
- **谱系独立成章**：软阴影、阴影图采样、阴影算法应用面均有书章级材料（`W9A-121/122/124/125/127`）。
- **消歧**：`VSM` 在文献里指 Variance SM，在 UE 里指 Virtual SM——两个同名不同物（`W9A-022/027`）。
- **元数据纪律**：同一 DOI 至少两路核对（Crossref + S2）；本轮遇过 3 处猜测 DOI 全错（`W9A-117`）。

## 对 LSSMJ（T-PH-05 的取舍）

1. 默认档走"图"系并经 PCF；质量档可替换为可滤波族，但接口要预留"可替换滤波族"抽象（`W9A-032`）。
2. 文档结构照"机制章 + 应用取舍章"分节（`W9A-122`）；采样/滤波独立成节（`W9A-127`）。
3. 体阴影在报告里留排除理由，避免后来者重复评估（`W9A-123`）。
