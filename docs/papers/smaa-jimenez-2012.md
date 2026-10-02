# Jimenez 等 · SMAA: Enhanced Subpixel Morphological Antialiasing（CGF 31(2), 2012）

> 来源：论文 PDF（作者页 iryoku.com 下载，`smaa_paper.txt` 快照）+ 作者页时间线 + Crossref（DOI
> 10.1111/j.1467-8659.2012.03014.x，页 355-364）。账本：`../analysis/ledger/w7a.jsonl`
> W7A-081..088（paper）+ 099..101（doc）；日期 2026-10-02。

## 要点（每条带账本行）

1. **定位**：图像域后处理 AA，改进 MLAA；首次把形态学与 MSAA/SSAA 多采样、时间重投影组合
   （"preserving the sharpness"）（W7A-087）。
2. **性能（GTX470/1080p）**：T2x 平均 1.3ms、4x 2.6ms（图 1 口径）（W7A-081）；正文表式读数
   1x=1.02ms / T2x=1.32ms / S2x=2.04ms / 4x=2.34ms（W7A-082）；1x/T2x 与其他方案同处 1ms 量级。
3. **四档构成**：1x=本体（精确距离搜索+局部对比+锐几何特征+对角模式）；S2x=+空间多采样；
   T2x=+时间超采样；4x=两者叠加（W7A-083）。
4. **ghosting 与速度**：朴素 resolve（线性混合）→明显 ghosting；重投影缓解；速度差加权（w=0.5·max(0,1−K|vc−vp|)）
   才消除（W7A-084）；速度模长粗存 alpha 通道"顺路"处理（W7A-086）。
5. **内存**：最重档仅 MSAA 8x 的 43%（前向）/17%（延迟）（W7A-085）。
6. **结论口径**：1x=低端配置的显然选择（渐变准确+时间稳定+开销极小）（W7A-088）；T2x=避免 2x 多采样
   的同时重建子像素细节。
7. **版本史（作者页）**：2.7（2012-01-16）引入 S2x/4x；2.8（2013-08-23）清理与微优化（W7A-099）；
   出处=CGF 31(2) EUROGRAPHICS 2012（W7A-100）。
8. **工程细节（论文内）**：子采样偏移 -0.25/+0.25 的面积计算修正、非轮廓特征忽略、对角直线重建、
   低对比区域回退 MSAA 2x 等——均为模式级细节，本节仅登记目录。

## 对 LSSMJ 的落点

- **SMAA 1x 是"比 FXAA 更锐"的空间档**：引擎文档口径（Unity：sharper than FXAA，W7A-132；Godot：
  runs slightly slower than FXAA, but produces less blurriness）互证；成本 1ms 量级（W7A-081/082）。
- **T2x = 后滤波+时间重投影**：与 TAA 同族，需要速度缓冲——LSSMJ 的决策树里"要不要速度缓冲"是分水岭：
  不建速度缓冲→FXAA/SMAA 1x；建→TAA（或 SMAA T2x）。
- 用于"最小可用集"的备选层：TAA 不可用平台（GL 档）退 SMAA 1x（比 FXAA 锐，成本同量级）。

## 未验证 / 边界

- 全部性能数字为 2012 年 GTX470/1080p 的论文自述，未复测；引用时保留硬件与分辨率口径。
- 论文的 SSAA16x/MSAA8x 对比图在本波未逐图核对（只取文字结论）。
- SMAA 的授权为论文+MIT 实现（iryoku 页发布），本波未核对 SMAA 源码仓库最新版许可（列待办）。
