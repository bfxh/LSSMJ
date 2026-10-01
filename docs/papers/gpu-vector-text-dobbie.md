# Dobbie：GPU Text Rendering with Vector Textures

> 锚：`W1F-015..018,092`；URL <https://wdobbie.com/post/gpu-text-rendering-with-vector-textures/>
> 定级：`paper`（正式技术长文，按 METHOD 的"权威长文"归 paper）；抓取 2026-10-01。

## 主张链条（与 Slug 同族，但更早给出问题清单）

1. **翻转**：前两代方案（位图图集、SDF）都是"CPU 预生成 → GPU 消费"；本文提出
   "What if we let the GPU render from the original vector data?"（`W1F-015`）——每像素评估矢量数据。
2. **图集硬约束**：不能穷举字形×字号，放大即插值模糊（`W1F-016` "you can't store every glyph at
   every possible size or you'll run out of memory"；`W1F-018` "the glyphs will start to get blurry
   due to interpolation"）。
3. **SDF 的缓解手法与残余成本**：为防圆角必须"keep storing higher resolution signed distance fields
   for each glyph"（`W1F-017`）——内存换保真，问题回到起点。

## 对本文的定位

- 它是"三选一"叙事里**最清楚的一篇问题陈述**：位图图集（清晰但不可放大）→ SDF（可放大但圆角）→
  矢量光栅（全都要，但实现最贵）。与 Valve（`W1F-001..007`）、Lengyel（`W1F-008..014`）、
  Chlumský（`W1F-063..066`）构成同一条时间线，**互相印证同一组取舍**。
- 文中附 Wolfire 博客的 SDF 伪影图（外部引用，本批未单独核）。

## 对 LSSMJ 文本层的取舍

- **不吸收其 GPU 路线**；**吸收其判据语言**：
  - 图集档的启用条件要写"字号量化档"（`W1F-016` 的直接推论：既然不能穷举，就必须定档并说明
    档间如何插值/取最近）；
  - 观感损失的触发条件（大字号放大）要写进验收（`W1F-018`）；
  - SDF 档的内存账按"最大目标字号 × 最坏字形"估（`W1F-017`）。
- 一句话写进设计文档：**"图集/SDF 的分辨率上限是内存约束的下游，不是精度约束"**——这条把
  `W1F-012`（原理上限）与 `W1F-017`（成本手段）绑定，便于后人复算。
