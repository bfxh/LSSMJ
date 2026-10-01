# Green 2007（Valve）：Improved Alpha-Tested Magnification for Vector Textures and Special Effects

> 锚：`W1F-001..007,090`；URL <https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf>
> 抓取：`curl` PDF（`pdftotext -layout`，双栏混排，见文末口径说明）；日期 2026-10-01。

## 它是什么

SDF（signed distance field）文字/矢量纹理的开山文：把**高分辨率字形图 → 距离场 → 8bit 纹理通道**，
绘制时用阈值/插值重建边缘（`W1F-002`）。三句骨架：

1. 方法本体："A simple and efficient method is presented which allows improved…glyphs composed of
   curved and linear elements."（`W1F-001`）；
2. 边缘基准："A texel value of 0.5 represents the exact position of the edge"（`W1F-003`）；
3. 抗锯齿来源：距离场自带"软区"，AA 是**编码的一部分**（`W1F-007`），且可由硬件双线性插值在
   采样间重建（`W1F-006`）。

## 关键机制

- **spread factor**：距离→[0,1] 的映射范围，同时决定描边/阴影/光晕的"作用半径"（`W1F-005`）——
  距离场把"效果半径"变成纹理常量，阈值可由像素着色器动态改。
- **消费侧两种实现**：alpha test（阈值 0.5，连自定义着色器都不需要）与像素着色器
  （soft edges/outline/glow/drop shadow）。
- **已知损失**：单通道"编码边缘必然磨圆角"（`W1F-004`）——后续 msdf（三通道）与
  Slug/矢量纹理（不预计算）都是为修这一条而生。

## 对 LSSMJ 文本层的取舍

- **不吸收为主**：候选窗规模（小字号、字形少）用 SwashCache 直存/图集即可；SDF 只作为
  "大字号/极端缩放的再评估档"（与 `09-assessment` §9.4 一致）。
- **若启用**：0.5 阈值 + spread 半径必须进参数表并可复算（`W1F-003/005`）；
  AA 验收=软区宽度 vs 目标字号（`W1F-007`），不许写成"更清晰"。
- **留给文档的一句**：SDF 的圆角是编码方式而非缺陷（`W1F-004`），修它要换格式（msdf/mtsdf）而非调参。

## 口径与坑

- PDF 为双栏排版，`pdftotext` 会跨栏交错；本批所有引文都取**同栏连续片段**（逐字校验通过）。
- 引用时带年份即可（SIGGRAPH 2007；Valve 版权页 `c 2007 Valve Corporation`，未成条）。
