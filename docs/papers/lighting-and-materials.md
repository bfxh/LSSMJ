# 光照、材质与色彩管理研究（第四轮 R3）

> 范围：PBR 谱系、IBL/SH、GI（烘焙/软光追/探针）、阴影（CSM/VSM/PCSS）、tonemap/HDR/色彩管理。
> 账本：`../analysis/ledger/w6c.jsonl`（21 条；快照在 `scratch/w6c/raw/`，由中断代理预先抓取、本仓复挖）。
> 说明：本项目 UI 档与光照关联很薄——**凡是"无"就写"无"**；价值在场景档。

## TL;DR（每条带锚）

1. **PBR 是"可打补丁的谱系"**：Disney BRDF 笔记后来补了 sheen 项（官方勘误，W6C-002）；UE 官方要求 Metallic **按二值**用（W6C-001）。
2. **Tonemap 双层策略成熟**：Uncharted2/Hable 是流行起点（W6C-003），ACES 有实时近似拟合（Narkowicz，W6C-004/021），ACES 本体是行业标准（W6C-006）。
3. **色彩工作流是历史双轨**：Unity 明确 linear/gamma 并存（W6C-007）——与 w1f 的 gamma 纪律一致：**工作流必须显式**。
4. **HDR 显示链是"能力查询+降级"**：Windows Advanced Color 官方指南（W6C-008）；Apple 侧走 Metal（W6C-009）。
5. **阴影的本质是分辨率分配**：CSM 的官方动机是透视走样（W6C-010），机制=视锥分片（W6C-011）；PSSM 被 3DMark 2006 采用（W6C-021）。
6. **软阴影有"估计量"思想**：PCSS 先 blocker 搜索再按距离定 penumbra（W6C-012）；SAVSM 是方差滤波经典（W6C-013）。
7. **GI 的默认档是软路线**：Lumen **默认**基于 SDF 软光追（W6C-015），三级次序=Screen Traces 先（W6C-016），硬件光追是可选加速（W6C-014）。
8. **实时 GI 的"实时"来自预计算底座**：Enlighten 依赖预计算可见性（W6C-017）；烘焙是慢过程（Unity 明示，W6C-019）、档位化（Lightmass，W6C-018）；探针=连续场离散采样（W6C-020）。
9. **对 UI 档：无直接对应**。唯一交集=颜色管理（sRGB/gamma，w1f 已覆盖）与 tonemap（若 UI 素材含 HDR 视觉）。
10. **对场景档：默认组合=CSM(阴影)+探针/烘焙(GI 静物)+Lumen 式软光追(GI 动态)+ACES 拟合(tonemap)**——每项都有官方材料支撑。

## 可吸收 / 不可吸收

| 项 | 判定 | 锚 |
| --- | --- | --- |
| Metallic 二值化等工作流纪律 | 吸收（若做材质管线） | W6C-001 |
| ACES + 实时拟合双层 | 吸收（场景档 tonemap） | W6C-004/006 |
| 线性/gamma 工作流显式化 | 吸收（与 w1f 合并为一条纪律） | W6C-007 |
| HDR 能力查询+降级 | 吸收（跨平台必备） | W6C-008/009 |
| CSM/PSSM 作为默认阴影 | 吸收（场景档） | W6C-010/011/021 |
| PCSS/SAVSM | 有界吸收（质量档） | W6C-012/013 |
| Lumen 式软光追等级序（屏幕→远场→硬件加速可选） | 吸收（场景档 GI 架构参照） | W6C-014/015/016 |
| 预计算底座（Enlighten/Lightmass/探针） | 吸收（静物/离线） | W6C-017..020 |
| 光照细节贴图/IBL 一手材料 | **缺口**（PBRT/Karis/IBL 快照抓到了 PDF 但本轮未逐条挖） | — |

## 对 LSSMJ 的落点

- UI 档：**不改设计**（唯一相关项=gamma 纪律已在 ADR G9）。
- 场景档：本篇与 `gpu-rendering-techniques.md`（Nanite/meshlet）、`raytracing.md`（待写）、`visibility-and-culling.md` 一起构成场景档的四根柱子：**几何（Nanite 式）/光（本篇）/可见性（同批）/RT（同批）**。

## 未验证

- PBRT 章、Karis/Frostbite course notes 的快照在 `scratch/w6c/raw/`（PDF+部分 txt）但本轮未逐条挖掘（预算用于已引 21 条）——**留给续跑**（编号段 `w6h` 空闲）。
- 所有引擎数字为官方文档自述，未本机复测。
