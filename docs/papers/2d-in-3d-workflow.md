# 2D-in-3D 工作流：纸片/公告板、Live2D 式变形、动画帧驱动

## 材料

- Live2D 官方文档（doc）：`https://docs.live2d.com/en/cubism-editor-manual/deformer/`（2026-10-03，HTTP 200；W8A-037、038）
- Billboard clouds（paper，TOG 2003，DOI 经 Crossref 核对）：`10.1145/882262.882326`（W8A-079）
- GGXrd 动画做法（官方 handout，paper）：有限动画/不插值/大量骨骼（W8A-009、012）
- 旁证：UTS 仓库内有 `Toon3Das2DMaterialUtility`（"3D as 2D"工具，命名即立场）

## 三条路线与证据

**A. 2D 模型即网格变形（Live2D 式）**
1. 官方口径：Cubism 通过移动网格顶点变形对象；变形器（deformer）成组编辑顶点省工时（W8A-037）。
2. 变形器可嵌套（warp 里套 mesh 一并变形），并区分 warp/rotation 两类（W8A-038）。
3. 插值注意：大幅旋转做线性插值会"缩形"，需 rotation deformer（官方文档正文）。

对 LSSMJ：如果未来要"2D 立绘角色"（非 3D 模型），这是除骨骼动画外的现成范式；其数据结构（网格+层级变形器+参数）与 3D 场景图的"节点+变换"可共用同一层抽象。

**B. 纸片/公告板渲染（billboard/impostor）**
1. 学术出处：Billboard clouds——用公告板云做极端模型简化（W8A-079，题录级）。
2. 工程含义：远景物用"面向相机的贴片"替代实体网格（LOD 最末档）。
3. 本批**未获**一手引擎文档（Epic "Octahedral Impostors" 页已下线：shaderbits.com 只剩空壳；GitHub 无对应官方仓）——列为缺口。

**C. 动画帧驱动（3D 骨架演 2D 手感）**
1. GGXrd：有限动画（关键帧间不插值），"full animation"路线试过但不传达 2D 感（W8A-009）。
2. 靠大量骨骼让每个特征可动，逐关键帧手摆（W8A-012）。
3. 手臂/身体的"2D 作画感"来自停格与形变，不来自平滑（同 handout 正文）。

对 LSSMJ：动画层需要"断帧/停格"的表达能力（不是曲线时间轴独裁）；这与"帧驱动"的 UI/文字轨（本仓既有）共享同一时间轴模型。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **吸收（有界）**：①公告板 LOD 作为远景默认档（与场景档第 6 步的 HLOD 同族）；②Live2D 式"网格+变形器"抽象若做 2D 角色再启用；③动画侧保留"有限动画"参数（关键帧插值开关）。
- **不吸收（本阶段）**：octahedral impostor 的具体烘焙管线（无一手材料）；Live2D 运行时集成（官方 SDK 为闭源+许可约束，本批未核许可）。
- **与共核的关系**：三条路线都不改渲染内核——A 是数据形态、B 是几何替代（LOD）、C 是动画时序；都可挂在同一场景图/可见性/纹理柱上。
