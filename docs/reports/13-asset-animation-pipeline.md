# 资产、动画与着色器管线研究（第四轮 R4）

> 范围：资产格式（glTF/USD/FBX）、几何压缩（Draco/meshopt）、动画压缩（ACL）、着色器工具链（WGSL/SPIR-V/naga/管线缓存）。
> 账本：`../analysis/ledger/w6d.jsonl`（17 条；快照在 `scratch/fetch/w6d/`，中断代理预抓、本仓复挖）。

## TL;DR（每条带锚）

1. **"核心+扩展"是格式寿命的关键**：glTF 2.0 自述 fully extensible（通用扩展与厂商扩展并存，W6D-001）；二进制 .glb 是规范内形态（W6D-002）。
2. **纹理链的标准闭环**：KTX2 容器（"efficient lightweight container"+supercompression，W6D-016/017）+ Basis 转码（"engines are expected to transcode…"，W6D-003）+ **PNG 回退**（W6D-004）。
3. **量化已在规范层**：KHR_mesh_quantization 把"16/8 位存储+反量化变换"标准化（W6D-005）。
4. **动画压缩的第一真理=体积即性能**：ACL 明示"clip 内存越大采样越慢"（W6D-006/007）；目标是电影级质量上界（W6D-008）。
5. **ACL 博客=压缩每一步的教科书**：位宽同时决定内存与误差（W6D-009）、量化步骤（W6D-010）、轨道语义（恒等元，W6D-011）、**常量/默认轨折叠=首要优化**（W6D-012）。
6. **几何压缩收益已到边际**：Draco 自报改进"最高 10%、平均 ~2%"（W6D-014）——先榨资产侧冗余（如 W6D-012）再谈换压缩器。
7. **着色器侧**：WGSL 是 WebGPU 的语言（W6D-015）；若走 wgpu 路线，naga 承担跨后端转译（快照有 naga README）。
8. **对 UI 档的映射**：UI 资产=字体文件+主题+少量图（`../renderer-qingjian/03`）——本篇机制基本不适用；**唯一直接可用**=W6D-012 的"折叠冗余"思想（青简的 `estimated_ems` 快路径、`trailing()` 合并是同族，`w5a`）。
9. **对场景档**：资产链=glTF 2.0（核心）+ KHR_texture_basisu/quantization（扩展）→ 离线构建（几何 Draco/meshopt、动画 ACL、纹理 Basis/KTX2）→ 运行期流送。
10. **许可注意**：glTF/KTX/WGSL 均为 Khronos/W3C 开放规范；Draco 为 Apache-2.0；ACL 为 MIT——**本批全部可参考可移植**（与 Unity/GN 的红线相反，ADR F1）。

## 可吸收 / 不可吸收

| 项 | 判定 | 锚 |
| --- | --- | --- |
| glTF"核心+扩展+回退"的格式架构 | 吸收（场景档资产格式基线） | W6D-001/002/004 |
| KTX2+Basis 纹理链 | 吸收（场景档；UI 档不压缩，见 texture-systems） | W6D-003/016/017 |
| 网格量化 | 吸收（场景档） | W6D-005 |
| ACL 的折叠/量化纪律 | 吸收（场景档动画；"折叠冗余"思想 UI 档通用） | W6D-006..012 |
| Draco/meshopt | 吸收（构建线） | W6D-013/014 |
| WGSL/naga 工具链 | 吸收（GPU 档源语言候选） | W6D-015 |
| FBX/USD 深读 | **缺口**（快照有 aps-fbx 等材料，本轮未挖） | — |

## 未验证 / 缺口

- USD、FBX 现状、glTF 最小例、godot 导入链的快照未挖（`scratch/fetch/w6d/`，编号段 `w6i` 空闲可续）。
- SPIR-V 官方概览页抓取为空（`khronos-spirv.txt` 无正文）——用 WGSL/Vulkan 章替代，SPIR-V 一手材料记为缺口。
- 未做任何压缩器实测。

## 对设计的落点

- 场景档资产管线（草案）：`glTF 2.0 → 构建期（Draco/meshopt + ACL + Basis/KTX2）→ 运行期（流送 + 转码 + 池化）`。
- UI 档：维持现状（字体清单 + 主题常量），**不引入资产管线**——与 §5 文本清单的分工一致。
