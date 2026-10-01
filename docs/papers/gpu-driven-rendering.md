# GPU-driven rendering：间接绘制、剔除与批（C1 深读 3/7）

> 来源（抓取 2026-10-01，全部 200）：
> vkguide GPU-driven 章 `vkguide.dev/docs/gpudriven/gpu_driven_engines/`（教程，自注"对 legacy vkguide 有效"）；
> Ubisoft `advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf`；
> GPUOpen `gpuopen.com/gpu-work-graphs-in-vulkan/` 与 AMD 演示 PDF。
> 账本：`w3a.jsonl` 的 `W3A-017..025`、`W3A-077..081`。

## 1. 它解决什么成本

- 立论（Ubisoft 讲义动机页逐字）：主机上 **"CPU scarcest resource on consoles"**（`W3A-077`）——
  即"每帧的 CPU 决策与 draw 提交"是瓶颈，而不是 GPU 填充/几何。
- 收益口径：CPU 侧每帧 <0.5 ms（vkguide，Ryzen 1700，`W3A-019`）；目标是"少数间接 draw 渲染高密度场景"（`W3A-023`）。

## 2. 关键机制

1. **DrawIndirect**：绘制参数放 GPU 缓冲，compute 写入（`W3A-017/020`）——draw 的"数量与参数"GPU 端生成。
2. **场景数据全体上 GPU**：对象矩阵/材质 ID/剔除包围体进大 SSBO，绑定降到最少 → 与 **bindless** 合流（`W3A-026/028`）。
3. **簇剔除（Ubisoft 版）**：~10x 实例增长驱动；`DrawInstancedIndirect` + GPU 剔除输出簇列表与 draw 参数；
   每簇 64 三角形；静态背面用逐簇 cubemap 可见性（每三角形 6 bit）剔除 10–30%（`W3A-080/081`）。
4. **遮挡深度生成**：预渲染最优遮挡体 → 全分辨率（Hi-Z/Early-Z）→ 降到 512x256 → 与上一帧深度重投影合并 →
   层次结构供剔除；成本明细 300 遮挡体 ~600us、降采样 100us、重投影 50us、层次 50us（**幻灯片自注 PS4 口径**，`W3A-080`）。
5. **work graph（新一代）**：draw 成为图节点，直接派发 mesh shader 管线（`W3A-013/024/025`）。

## 3. 规模量级

| 读数 | 口径 | 锚 |
| --- | --- | --- |
| 125,000 对象剔除后 290 FPS（4000 万三角形 / 2 passes） | vkguide，RTX 2080 | `W3A-017` |
| 250,000 "drawcall" >60 fps（PC 500 fps） | vkguide，Nintendo Switch | `W3A-018` |
| >100 万对象剔除 <0.5 ms | vkguide | `W3A-020` |
| 遮挡深度生成合计 ~800us/帧 | Ubisoft，PS4 | `W3A-080` |

## 4. 对本项目的取舍

- **不吸收**：我们对象量级 10^2–10^3，收益区间差 2–3 个数量级；GPU 剔除/间接批带来的复杂度（同步、
  顺序不确定、内存膨胀 `W3A-078/079`）远大于收益。
- **有界吸收（3 条，进设计 §4.2/§8）**：
  1. **批键成本序**：管线切换最贵（`W3A-022`）→ 先并管线/绑定，最后并 draw；Doom <500 管线 vs UE 10 万+（`W3A-021`）；
  2. **"CPU 归零"作为 GPU 档的验收指标**（`W3A-019/077`）——不是帧率，而是主线程时间；
  3. **数字带口径**：Ubisoft 幻灯片连"每项成本"都标平台（`W3A-080`），我们读数照此办理。
- **顺序确定性是硬约束**：Ubisoft 的簇顺序不确定（`W3A-079`）在 UI 上就是"渲染顺序错"；我们以金丝雀（判据 C4）守。

## 5. 未验证

- vkguide 章节自注"已过时于新版本 vkguide"（教程正文），其数字仅作量级锚，不作为可行性判据。
- bindless 在目标后端（wgpu）的支持度未核证；退路按"每材质 1 个 draw-indirect"（`W3A-028`）设计。
