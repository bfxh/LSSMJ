# 风格化 PBR 与"共核"证据集（本批最关键的选型材料）

> 回答的设计问题：**三渲二与 photoreal 能否共用同一内核（几何/可见性/纹理/后处理），只换 shading 与描边层？**
> 本文把正/反证据各归位；详细架构判据见 `npr-toon-rendering.md` §5。

## 正面证据（同核可行）

1. **UTS3（官方）**：一套 toon shader 兼容 Built-in/URP/HDRP 三条管线（W8A-040）；跨管线共用一份 `UTSLighting.hlsl` 光照核（W8A-044、045）——官方级"着色层与管线解耦"样本。
2. **kShading（社区，最直白）**：README 自述 "Toon Lit: A cel style shader that supports all features of the Lit shader but uses a stepped physical approximation BSDF"（W8A-063）；实现是对 GGX 镜面项做 `step` 两瓣量化（W8A-064、065）——**同管线、同特性、只换 BSDF**。
3. **UW 2020 毕设项目页**：在 Cook-Torrance + GTAO 的物理光照实现上"增强支持非写实"（color ramp + outline）——现成的"photoreal 内核上加 NPR 层"的公开工程记录（本批未逐字引其页面文本，仅由检索摘要定位，**列为待补**）。
4. **SIGGRAPH 2021 poster（学术）**：在传统光追器上做改造（modification）产生卡通/绘画风格，且保留反射/折射/GI（W8A-025、026、090、091）——NPR 可挂在物理光路的核心上。
5. **米哈游 console 档（官方）**：卡通角色跑在通用设施上——clustered deferred lighting 1024 灯（W8A-013）、局部灯阴影压缩（W8A-014）、capsule AO（W8A-015）——角色外观差异化没有另立渲染器。
6. **原神式社区全链路**：SDF 脸/ramp/hair mask 全部实现为 URP 的 shader 变体，复用 URP 光照与阴影函数（`light.distanceAttenuation`、`light.shadowAttenuation`，见 W8A-049 同级代码 `ShadeSingleLight` 里的引用）——工程上"挂在标准管线上"无阻碍。

## 反面/条件证据（共核的边界）

1. **逐角色光照覆写**：GGXrd 没有全局光照照角色，每角色专光（W8A-010）；UTS 允许颜色设计覆盖实际光色（官方 index 文本）——共核必须支持 **per-object shading/光照覆写**。
2. **面部是硬分支**：脸部忽略常规 cel shade、走 SDF 专线（W8A-060、061）——"同一 BSDF 参数化"不够，需要**按部位派发 shading 变体**。
3. **GI 要退化**：NPR 实现主动丢掉 SH 细节项只留常数（W8A-061）——与 photoreal 的 SH 细节诉求相反，需要档位切换而非二选一。
4. **后处理参数重调**：饱和度增强 HBAO / Bloom 色相偏移（W8A-032）——同一后处理链要允许 NPR 参数集。
5. **描边是额外结构**：第二套几何壳（W8A-006、041）或深度/法线后处理缓冲（W8A-036）——共核要为其预留资源与 pass 位。

## 倾向性结论（本批证据的读法）

- **可以共核，但"核"的准确边界是**：几何/可见性/纹理/动画/阴影设施 + 一个**可派发的 shading 层**（含 per-object 覆写、per-部位分支、Step/Feather/ramp 参数化）+ **一个可开关的描边层**（几何壳或后处理）+ **NPR 化的后处理参数集**。
- 判据强度分级（按证据深度）：
  - 强（官方/源码双证）：UTS 跨管线、kShading 的 BSDF 替换、URP 社区链路的函数复用。
  - 中（学术 poster/摘要级）：光追上的 NPR 改造。
  - 弱（二手/转录）：米哈游工程细节。
- **否证条件（写清以防过度外推）**：若 LSSMJ 的 NPR 目标要求"逐帧光照动画 + 逐角色唯一光向量"（GGXrd 式）与 photoreal 场景光同时存在，则"共核"退化为"共用大部分设施 + 光照层双模"，实现复杂度高于单模估计——需在 P-NPR 立项时用一个小场景实测（光向量覆写 × 阴影/反射的耦合）。

## 未验证

- UW capstone 页面文本未逐字进账本（只有检索摘要）——**不构成证据**，仅列线索。
- 无任何本机实测；全部为文献/代码口径。
