# 纹理图集打包：RectangleBinPack 实测 + WebRender 的 slab 转身

> 锚：`W1F-055..062`；URL <https://raw.githubusercontent.com/juj/RectangleBinPack/master/RectangleBinPack.pdf>
> 与 <https://nical.github.io/posts/etagere.html>（该页即《Improving texture atlas allocation in WebRender》全文版）

## 一、Jylänki《A Thousand Ways to Pack the Bin》（survey）

- 定位："以可复现实测比较多种 2D 装箱启发式"（`W1F-055`）；**样本量 2619 个变体**（`W1F-056`）。
- 结论（论文原话）："It is clear that the MAXRECTS algorithms perform the best of all."（`W1F-057`）；
  离线全局档 "produces the ultimately best packings"（MAXRECTS-BSSF-BBF-GLOBAL，`W1F-059`）。
- Guillotine 系约束：切分轴决定空闲矩形、摆放不得跨切线（`W1F-058`）——简单但受限。
- **引文口径注意**：PDF 文本层的小数与部分数字被排版打乱（如成绩 `: : .` 形态），
  **论文里的具体分数不可直接引用**，只能引文字结论；要数字请回原文版面核。

## 二、WebRender 的实践（nical，2021）

- 反转叙事：WebRender 图集分配器**从 guillotine 换成定长 2 幂方 slab**（`W1F-060`），
  参数=512×512 区域 + 区域内定长网格（含少数矩形特例）（`W1F-061`）。
- 评测维度=浪费空间（`W1F-062`）——不是"算法更聪明"，而是"浪费率可量化"。

## 两条合起来读

survey 说"最优解在 MaxRects"；工程实践说"我们的场景选了最简单的定长 slab"。二者不矛盾：
**分配器的选择依赖场景（字形尺寸分布、增量/批处理、碎片容忍度）**，这正是本项目的形态问题——
候选窗字形少、窗口小、增量插入 → slab/guillotine 级即足；大面板/字形多时再上 MaxRects。

## 对 LSSMJ 图集档的取舍（写进 ADR）

1. 起步：定长 2 幂方 slab（`W1F-060/061`），参数先照 512 区域量级；
2. 验收指标：**浪费率 + 命中率**双记账（`W1F-062`）；
3. 升级路径：字形数/窗口面积过阈值 → MaxRects-BSSF 变体（`W1F-057/059`），
   离线整图重建时可用 GLOBAL 档；
4. 引用纪律：只引 `W1F-057` 这类文字结论；分数不复述（见上"引文口径注意"）。
