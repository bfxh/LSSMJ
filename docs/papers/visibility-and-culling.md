# 可见性、剔除与场景组织研究（第四轮 R5）

> 范围：视锥剔除、遮挡剔除（硬件查询/软件遮蔽体/门户）、BVH/空间结构、GPU 驱动剔除、HLOD/世界分区、可见性缓冲。
> 账本：`../analysis/ledger/w6e.jsonl`（18 条；快照在 `scratch/w6e/raw/`，中断代理预抓、本仓复挖）。

## TL;DR（每条带锚）

1. **遮挡查询的固有矛盾=延迟**：CHC 的核心是"复用上帧查询结果"对抗查询延迟（W6E-001/002）。
2. **工业默认两件套**：视锥剔除 + 硬件遮挡查询（UE 官方口径，W6E-007）；距离剔除是便宜的第一刀（W6E-008）。
3. **软件遮挡剔除是替代路线**：以光栅化遮挡体求可见性（Godot 的 occluder/occludee 二分，W6E-003/004；LTH 的 7M 三角场景材料，W6E-011）。
4. **遮挡剔除的正确性风险被显式化**："false negatives or erroneous culling"（W6E-012）——判据必须含"别把该看见的剔掉"方向。
5. **HLOD/世界分区是大世界骨架**：UE HLOD 多 Actor 合并代理（W6E-009）；World Partition=格网单元+距离流送（W6E-010）；Godot 的 Visibility Range 是同族开源物（W6E-005）。
6. **商业预处理可分布式**：Umbra 的构建农场支持（W6E-013）——遮挡数据是**资产**。
7. **GPU 侧早退机制**：ExecuteIndirect（W6E-015）与 VK_EXT_conditional_rendering（"按缓冲值条件执行"→"条件丢弃降延迟"，W6E-016/017）。
8. **可见性缓冲**：以 ID 缓冲替代 GBuffer 的延迟着色路线（JCGT，W6E-006）。
9. **实例化/GPU 常驻**是当代默认优化面（Unity BRG/GPU Resident Drawer，W6E-018）。
10. **对 UI 档映射（不硬扯）**：候选窗的"哪些格子进帧"已有等效物（Core `Grid` 视口 + `Frame::columns`，`w5a`/`candidate-ui.md:21`）——UI 档**不需要**本篇机制；场景档按本清单立项。

## 可吸收 / 不可吸收

| 项 | 判定 | 锚 |
| --- | --- | --- |
| 视锥+硬件遮挡查询默认档 | 吸收（场景档起步组合） | W6E-007 |
| 距离剔除（Actor/Volume） | 吸收（第一刀） | W6E-008 |
| CHC 的"上帧结果复用" | 吸收（对抗查询延迟） | W6E-001/002 |
| 软件遮挡剔除（occluder 显式声明） | 有界吸收（无硬件查询时） | W6E-003/004/011 |
| 正确性方向判据（假阴性） | 吸收（写进门） | W6E-012 |
| HLOD/World Partition/Visibility Range | 吸收（场景档骨架） | W6E-005/009/010 |
| Umbra 式预处理资产化 | 有界吸收（商业授权；概念可自研） | W6E-013 |
| ExecuteIndirect/条件渲染 | 吸收（GPU 档早退） | W6E-015/016/017 |
| 可见性缓冲 | 有界吸收（GBuffer 带宽吃紧时） | W6E-006 |

## 未验证 / 缺口

- 视锥剔除的经典材料（Gribb-Hartmann、RTR blog）在快照中（`scratch/w6e/raw/`）但本轮未逐条挖。
- Wald BVH/SAH、Cohen-Or、Pantaleoni 等论文只到 Crossref 元数据（PDF 部分抓到但未挖）。
- 未本机实测任何剔除方案（只读纪律）。

## 对设计的落点

- 场景档可见性栈（建议次序）：**距离剔除 → 视锥 → 层级遮挡（硬件查询优先、软件遮蔽体次选）→ GPU 侧早退（ExecuteIndirect/条件渲染）**；
  判据=剔除正确性（含假阴性金丝雀）+ 帧预算内命中率。
- 与 `../lssmj-design/README.md` §4.3 的关系：UI 档的 damage 是"像素级可见性"，场景档的剔除是"对象级可见性"——**同一纪律的两层**。
