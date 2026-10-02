# 实时光追与降噪研究（第四轮 R2）

> 范围：硬件 RT（RT Core/DXR/Vulkan RT）、降噪（SVGF/ReSTIR/NRD）、Ray Tracing Gems、混合渲染。
> 账本：`../analysis/ledger/w6b.jsonl`（14 条；快照在 `scratch/r4/rt/`）。
> 口径提醒：本批**未读任何论文全文**（RTG 只到页面级；ReSTIR/SVGF 只到 Crossref 检索锚）——
> "Open Access 全文未读"是明确缺口，不是已覆盖。

## TL;DR（每条带锚）

1. **RT 的真实形态是混合渲染**：Turing 官方定位"hybrid rendering model：real-time ray tracing + rasterization + AI"（W6B-003）；RT Core 在每个 SM，加速 BVH 遍历（W6B-004）。
2. **降噪是组成部分而非可选项**：Turing 白皮书直接写"RT Cores work together with advanced denoising filtering"（W6B-005）。
3. **DXR 的任务模型**：`DispatchRays()` 派发 ray generation 网格（W6B-001）；规范含"构建输入固定 ⇒ 结果确定"的确定性条款（W6B-002）——与我们的逐位金丝雀同族。
4. **Vulkan RT 语义**：加速结构找潜在相交（W6B-006）、miss shader 显式化未命中路径（W6B-007）。
5. **工程书目**：RTG I（DXR 时代，W6B-008/009）、**RTG II（Open Access，可自由取全文**，W6B-010/011）——DXR/Vulkan/OptiX 跨 API。
6. **算法双璧**：ReSTIR（分段时间蓄水池重采样，W6B-012）与 SVGF（时空方差引导滤波，W6B-013）——采样与降噪各占一边。
7. **GN SDK 的"光追全局光照"声称**：样例自述"快捷开启光追全局光照和HDR自适应曝光"（W6B-014）——**对照现实技术面**应为 RTG/Lumen 类近似体系（SDF/屏幕空间/降噪）；闭源无从验证实现，**按"宣称 vs 实现"两栏记账**，不得当作能力引用。
8. **对 UI 档：不吸收**。位图候选窗与 RT 无交集（青简路线 `w5a`；连 GPU 都明确不上，`rendering.md:60`）。
9. **对场景档（P6+）**：若做到需要真实 GI/反射的档次，技术序=混合渲染（光栅为主）+ 屏幕空间先 + RT 补远场 + 降噪（SVGF 类）——与 Lumen 的"屏幕→远场→硬件可选"次序一致（`lighting-and-materials.md` W6C-014..016）。
10. **确定性与降噪的工程纪律**：DXR 的确定性条款（W6B-002）+ 降噪的时空复用（W6B-012/013）提示——**时序复用类算法必须配"历史注入/失效"判据**（我们的 damage/tile 复用同族）。

## 可吸收 / 不可吸收

| 项 | 判定 | 锚 |
| --- | --- | --- |
| 混合渲染（光栅为主轴） | 有界吸收（仅场景高配档） | W6B-003/004 |
| 降噪作为管线成员 | 有界吸收（同上） | W6B-005 |
| 加速结构确定性条款的思想 | 吸收（写成"构建输入固定 ⇒ 结果可复现"的判据） | W6B-002 |
| miss 路径显式化 | 吸收（查询类功能的分支纪律） | W6B-007 |
| RTG II 全文（Open Access） | 吸收（预算内可续读） | W6B-010/011 |
| GN SDK "光追" 宣称 | **不可引用**（未验证口径） | W6B-014 |
| UI 位图档引入 RT | **不吸收** | 与 w5a/`rendering.md:60` 冲突 |

## 未验证 / 缺口

- RTG I/II 全文、ReSTIR/SVGF/A-SVGF/BMFR 原文（A-SVGF/BMFR 抓取页无静态正文；ReSTIR/SVGF 仅 Crossref）——编号段 `w6j` 空闲可续。
- Metal ray tracing 文档页 404；NRD README 502；RTXGI README 404——三家一手材料缺失（如实登记）。
- 无任何本机 RT 实测（无 RT 硬件条件下的诚实边界）。
