# 描边技术谱系（inverted hull / 后处理 / 线宽控制）

- 一手/官方：UTS 文档与源码（`Documentation~/Outline.md`；`URP/UniversalToonOutline.hlsl`；`BuiltIn/UCTS_Outline.cginc`）——W8A-041、042、046、047、048
- 一手/官方：UE 后处理材质文档（custom depth buffer）——W8A-035、036
- 一手/学术：Saito & Takahashi 1990（G-buffer + 2D 图像处理，W8A-082、095）；Lake 2000（轮廓/材质边界/折痕三类线，W8A-083）；Rong & Tan 2006（Jump Flooding，W8A-017..020、088、089）
- 案例：GGXrd handout（inverted hull 采用理由，W8A-005..008、092）
- 社区实现：URP_Toon（裁剪空间外扩，W8A-052）、GenshinCelShaderURP（视图空间外扩 + 深度 rim，W8A-056、057）、Nilo（屏幕恒定宽度补偿，W8A-062）
- 另：Rusinkiewicz 线描课程（线型术语，W8A-077）

## 三类做法与证据

**A. 几何外扩（inverted hull）——有两条实现子路线**
1. 物体空间沿法线外扩：UTS 主模式（W8A-041 官方两法：法线挤出 / 整体缩放）；外扩方向可用**烘焙法线**替换以免破损（W8A-047）。
2. 裁剪/视图空间外扩：URP_Toon `positionCS.xy += 0.01 * outlineWidth * projectedNormal.xy`（W8A-052）；GenshinURP 把法线压到 XY 平面加到视空间位置（W8A-056）——目的是**屏幕宽度均匀**。
3. 宽度控制：按相机距离 smoothstep 缩放（UTS，W8A-046）；按视空间深度×FOV 补偿（Nilo，"keep outline similar width on screen accoss all camera distance"，W8A-062，含正交相机分支与魔数 50）。
4. 逐顶点宽度：GGXrd 用顶点色控制并可局部擦除（W8A-005）；UTS 用 Outline Width Map（文档）。
5. 开关粒度：UTS 把描边做成独立 pass 可整体开关（W8A-048）。

**B. 后处理/图像向**
1. Saito 1990：线/边/剖面线全部用 2D 图像处理（非逐线追踪），几何属性存 G-buffer（W8A-082、095）——描边后处理的祖型。
2. UE 官方：custom depth buffer 把对象渲进独立深度缓冲（W8A-036）；后处理材质以 blendable 挂链（W8A-035）。
3. 距离场线宽控制：Jump Flooding 常数轮次算距离变换近似（W8A-017..020、088、089）——线宽渐变/外发光的基础设施。
4. 线型不止轮廓：Lake 2000 的轮廓/材质边界/折痕三类（W8A-083）；GGXrd 的内描边另用"轴对齐光束+UV 重叠"（W8A-008）；GenshinURP 的深度 rim 是又一种"图像向风格线"（W8A-057）。

**C. 两种路线怎么选（证据里的取舍点）**
- GGXrd 明说：后处理描边常见，但他们要**建模视口可预览 + 顶点级控制**，故选 inverted hull（W8A-007）。
- 代价对照：inverted hull 增几何与 draw（第二套壳）；后处理增全屏 pass、需要深度/法线缓冲，且线宽受分辨率约束（Saito 系是分辨率相关的）。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **默认档建议（有界吸收）**：inverted hull（法线外扩，可选烘焙法线）+ 独立 pass 开关 + 距离/FOV 补偿 = 最少依赖、与现有几何柱零冲突（描边壳只是网格实例的变体）。
- **质量档**：后处理边缘（custom depth/stencil 式对象掩码 + Sobel/深度阈值）用于内描边与远景线；JFA 距离场用于线宽渐变。
- **记账**：描边壳使角色顶点/绘制翻倍；后处理描边需要深度+法线双缓冲（与 W6E 可见性柱共用）。
- **判据**：内描边（材质边界）不能靠 hull，需要 UV 技巧（GGXrd 路线）或后处理——LSSMJ 在"特写不糊"这一点上必须给判据（对应 Xrd 的"分辨率无关"纪律）。

## 未验证

- inverted hull 在自由视角+动态光下的一致性问题（暗面壳可见/自遮挡）本批无双盲材料；GGXrd 的前提是固定镜头。
- JFA 的 GPU 实现细节（轮次/带宽）未逐节读，只取了算法性质。
