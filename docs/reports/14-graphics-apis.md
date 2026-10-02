# 图形 API 研究（第四轮·补：用户点名）

> 范围：D3D11/D3D12（含 Work Graphs）、Vulkan（资源/描述符/稀疏）、OpenGL（MDI）、Metal、WebGPU 与 wgpu。
> 账本：`../analysis/ledger/w6f.jsonl`（13 条；快照在 `scratch/r4/api/`）。
> 定位：本研究报告**不吃 API 全貌**，只回答一个设计问题——**LSSMJ 的 GPU 档应该站在哪一层 API 上**。

## TL;DR（每条带锚）

1. **"显式化"是 D3D12/Vulkan 一路的主线**：Vulkan 用不透明描述符容器 + 逐绑定类型/数量声明组织资源（W6F-004/005）；稀疏资源有规范级内存上限条款（W6F-003）。
2. **D3D12 的新方向是 GPU 自治**：Work Graphs 正式发布="enabling new types of GPU autonomy"——把"谁产生工作"交给 GPU（W6F-002）；这与 ExecuteIndirect（W6E-015）是同一条演进线。
3. **跨 API 共识**：间接绘制（GL 的 MultiDrawIndirect，W6F-012；D3D12/Vulkan 同族）——**GPU 驱动管线已是各 API 的共同能力**。
4. **WebGPU 是"可移植 GPU 层"的当代标准**：规范摘要句（W6F-006）+ 适配器抽象（W6F-007）；**wgpu 以它为蓝本**、纯 Rust、原生跑 Vulkan/Metal/D3D12/OpenGL（W6F-008/009）。
5. **Metal 是单栈紧耦合**（"graphics and compute API coupled with a powerful shading language"，W6F-010）——苹果侧没有多后端选择，跨平台策略必须包含"后端差异处理"。
6. **D3D11 的定位句**（W6F-001）适合引用做历史对照：它代表"驱动替你决策"的一代，D3D12/Vulkan 代表"你来决策"的一代。
7. **对 LSSMJ 的选型结论（本批最重要的一行）**：
   **GPU 档统一站在 `wgpu`（WebGPU 模型）上，不直接写原生 API**；原生 API 的特性（Work Graphs/稀疏/描述符索引）作为**能力参考与降级判据**，不进内核。
8. **理由链**：①多后端单代码（W6F-008）②规范先行、可审计（W6F-009）③我们不需要 Work-Graphs 级自治（UI/小面板档）④Rust 生态与 naga/WGSL 同栈（W6D-015）。
9. **代价也要记账**：wgpu 抽象层=新能力到达滞后（Work Graphs 等先在原生 API 出现，W6F-002）+ 特定后端缺陷需绕行——列为 GPU 档的风险项。
10. **降级梯度**：wgpu（Vulkan/Metal/D3D12）→ wgpu（GL 后端）→ CPU 位图路径（本仓现状，`renderer-qingjian` 全套）——三级全部有现成实现。

## API 对照表（按对我们的关注点）

| 关注点 | D3D11 | D3D12 / Work Graphs | Vulkan | Metal | WebGPU/wgpu |
| --- | --- | --- | --- | --- | --- |
| 显式性 | 低（驱动决策）W6F-001 | 高（GPU 自治，W6F-002） | 高（描述符/内存显式，W6F-004/005） | 中高（单栈，W6F-010） | 中（安全封装，W6F-006） |
| 多后端 | 仅 Windows | 仅 Windows | 跨平台 | 仅 Apple | **跨平台单代码**（W6F-008） |
| 间接/GPU 驱动 | 部分 | ✓（W6E-015 + W6F-002） | ✓（W6F-003/012 同族） | ✓ | ✓（WebGPU 子集） |
| 稀疏/虚拟纹理 | ✓（Tiled，w6a W6A-003） | ✓ | ✓（W6F-003） | ✓ | 部分 |
| 我们的用法 | 历史对照 | 特性参考 | 特性参考 | 差异处理参考 | **实现层（P3 首选）** |

## 未验证 / 缺口

- D3D12 programming guide 页 404（路径变动）——D3D12 侧以 Work Graphs devblog 为锚（W6F-002/013）。
- AZDO（OpenGL 零驱动开销）演讲材料未取到（多镜像 404）——OpenGL 侧只有 MDI 扩展一条（W6F-012）。
- 未做任何 API 层实测（无运行时）。
