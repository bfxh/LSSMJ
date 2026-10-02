# 纹理系统研究（第四轮 R1）

> 范围：纹理压缩（BC/ASTC/Basis）、虚拟纹理（SVT/Tiled Resources）、mipmap 与过滤、流送与预算。
> 账本：`../analysis/ledger/w6a.jsonl`（13 条，source 为主；快照在 `scratch/r4/tex/`）。
> 与前轮的分工：图集打包（w1f）、图集阈值（w2d）/MSDF 图集（w2a）已覆盖——本篇只做**压缩/虚拟化/mipmap/流送**。

## TL;DR（每条带锚）

1. **虚拟纹理的两种形态**：软件侧 SVT（"虚拟大纹理→物理纹理"页映射，W6A-001/002）与平台侧 Tiled Resources（"大逻辑资源用小物理内存"，W6A-003）。
2. **压缩是按内容分档的清单**：BC7 每 4×4 块 8 模式（W6A-005）、BC6H 面向 HDR（W6A-004）；ASTC 提供 **0.89–8 bpp 连续可调**（W6A-007）；Basis Universal 走"一次编码 + 运行期转码"（W6A-008），分 ETC1S/UASTC 两档（W6A-009）。
3. **容器与编解码分层**是当代标准形态：KTX2（容器，W6D-016/017）+ Basis/平台格式（编解码，W6A-008）；glTF 用 `KHR_texture_basisu` 落进资产格式（W6D-003/004）。
4. **转码是运行期责任**："engines are expected to transcode …into some block-compressed texture format supported by the platform"（W6D-003 原文）。
5. **压缩的收益是双账**：显存/体积 + 采样速度（ACL 对动画给出同构证据："内存越大采样越慢" W6D-007）。
6. **mipmap 是 1983 年形式化的采样问题**（Williams, Pyramidal Parametrics, W6A-013）——不是实现细节。
7. **离线管线可全 GPU 化**：NVTT 3 的 CUDA 压缩与 On-The-Fly API（W6A-011/012）——资产构建自成一档性能线。
8. **流送是"大世界"能力**：UE 的"运行期进出内存"体系（W6A-010）；UI 档不需要，但预算/池化思想通用。
9. **对本项目结论（有意保守）**：候选窗图集**不压缩、不 mipmap、不虚拟化**——字形是覆盖率 AA 图、按 scale 精确栅格（青简路线，`w5a`），任何有损压缩都会与"对齐原生"的目标冲突（w1f 的 gamma/亚像素三硬约束同理）。
10. **未来场景档**：纹理链按"KTX2 容器 + Basis 转码 + 平台格式分档 + mipmap 全程 + 预算池"组装；离线侧用 CUDA/多线程压缩器。

## 可吸收 / 不可吸收（对 LSSMJ）

| 项 | 判定 | 锚 |
| --- | --- | --- |
| SVT/Tiled Resources 的"页映射"概念 | 有界吸收（场景档；UI 档不需要） | W6A-001..003 |
| BC7/ASTC 的"块内模式选择"与连续码率 | 吸收（场景档资产管线） | W6A-004/005/007 |
| Basis Universal 一次编码多平台转码 | 吸收（场景档，配 KTX2） | W6A-008/009、W6D-003 |
| 运行期转码 + 回退（PNG） | 吸收（兼容性判据：老客户端能显示） | W6D-004 |
| 纹理流送/内存池 | 有界吸收（大世界才需要） | W6A-010 |
| GPU 侧离线压缩（NVTT） | 吸收（构建线） | W6A-011/012 |
| **对字形图集压缩/mipmap** | **不吸收**（与覆盖率 AA + 精确栅格冲突） | 与 w5a/w1f 结论一致 |

## 1. 虚拟纹理

- 软件 SVT：虚拟→物理页映射（W6A-001/002）；与我们的 tile 脏区同构（页=图块）。
- 平台 Tiled Resources：逻辑资源可远大于物理常驻（W6A-003）；常驻集可动态更新。
- 对 UI 档无需求；对场景档=大地表/大图集的标准解。

## 2. 压缩谱系

| 格式 | 定位 | 锚 |
| --- | --- | --- |
| BC1-5 | 基线（D3D10 前） | W6A-004 佐证 |
| BC6H | HDR 源数据 | W6A-004 |
| BC7 | 高质量 RGB/RGBA（8 模式/块） | W6A-005 |
| ASTC | 移动/跨平台（连续码率） | W6A-006/007 |
| ETC1S（Basis 档1） | 超小体积 | W6A-009 |
| UASTC（Basis 档2） | ASTC-4×4 风格高质量 | W6A-009 |

## 3. 容器、转码与资产格式

- KTX2：容器 + 超压缩（W6D-016/017）；Basis 负责编解码（W6A-008）。
- glTF：`KHR_texture_basisu` 指定 KTX2 图 + PNG 回退（W6D-003/004）——**"扩展+回退"是资产兼容的标准契约**。

## 4. mipmap 与过滤（现状与缺口）

- mipmap 的原始形式化（W6A-013）；各向异性过滤的**权威一手材料本轮未获**（GPUOpen 404/registry 403）——
  记为缺口（见 §6），不影响 UI 档结论（UI 按 scale 精确重栅格，无 mip 需求）。
- 位图 UI 的缩放降级（如 0.75×、1.25× DPI）：青简按 scale 重画（矢量栅格），比 mipmap 采样更准——**保持**。

## 5. 流送与预算

- UE 的按需进出内存（W6A-010）与 NVTT 的管线边界记账（W6A-011/012）给出两个口径：**常驻预算**与**构建预算**分开算。

## 6. 未验证 / 缺口

- GPUOpen 纹理过滤/压缩文章（404）、Khronos registry 规范页（403，Cloudflare）——一手规范链不完整（已用 MS/ARM/Binomial 官方材料替代）。
- Williams 1983 只到 Crossref 元数据（PDF 未取）。
- 未做本机压缩实测（只读纪律）。

## 7. 对设计的落点

- UI 档：维持"不压缩、无 mipmap、精确栅格"（与 `../renderer-qingjian/03`、w5a 一致）。
- 场景档（待写 `../lssmj-design/02-scene-tier.md`）：按 TL;DR 10 的清单立项；判据=显存预算 + 采样质量对拍（压缩前后 PSNR/并排）+ 兼容回退测试。
