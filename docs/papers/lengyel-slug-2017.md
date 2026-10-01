# Lengyel 2017（JCGT）：GPU-Centered Font Rendering Directly from Glyph Outlines

> 锚：`W1F-008..014,091`；URL <https://jcgt.org/published/0006/02/02/paper-lowres.pdf>（JCGT 6(2), 2017）
> 这是 **Slug 算法的论文本体**——w3f 此前只引到 sluglibrary.com 的营销文字，本条补上论文侧。

## 它是什么

在 GPU 上**直接从轮廓数据**渲染抗锯齿文本，不用任何预计算纹理或距离场（`W1F-008/009`）：
每像素做绕数（winding number）计算 + 覆盖率估计，天然支持仿射/投影变换下的任意缩放旋转
（论文 Figure 1 口径）。

## 三个可通用于我们的论点

1. **图集原理上限**："All of the techniques that store data in a texture atlas are inherently using a
   discrete sampling of what is actually an infinitely precise description of a glyph outline."
   （`W1F-012`）——提高分辨率只能缓解、无法消除。
2. **对既有格式的评价**：SDF "tend to round off sharp corners and thus do not preserve the true
   outlines"（`W1F-010`）；msdf "corrected the corner rounding problem, but required a complicated
   analysis step in the preparation of the texture atlas"（`W1F-011`）。
3. **代价的诚实口径**：多射线方向更各向同性但更贵，作者取"坐标轴平行 + 超采样"折中
   （`W1F-014`）；小字号下三角形过碎会降低 GPU 占用率（未成条，`W1F-014` 邻近段落）。

## 工程可行性

- "Our method requires only widely available GPU features and can be implemented on OpenGL 3.x / DX10
  hardware."（`W1F-013`）——门槛不高，但需要把轮廓上传到 GPU（顶点/存储纹理）。
- 论文含 2 百万像素填充的计时表（GeForce GTX 1060），**表内数字在 pdftotext 输出中被版式打乱，
  本批未入账**；要引性能必须回到原文版面复核。

## 对 LSSMJ 文本层的取舍

- **不吸收**（GPU 档的"第三条路"备选，CPU 基线不做）：我们的显示面是候选窗/浮层量级，
  用不到投影变形下的字形光栅；但**论文的 `W1F-012` 原理句值得抄进图集档设计**——
  它说明"图集档必然有采样上限"，因此必须写明缓解手段与触发条件。
- 与我们既有两条路线的关系：图集/直存（SwashCache）是默认；SDF/msdf 是"大字号档"；
  Slug 式是"GPU 档 + 需要极致保真"时的候选——写进 ADR 备选表即可。
