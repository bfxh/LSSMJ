# Guilty Gear Xrd 的"2D 外观 / 3D 管线"（Arc System Works，GDC 2015 handout）

- 一手材料：官方 handout PDF `https://www.ggxrd.com/Motomura_Junya_GuiltyGearXrd.pdf`（2026-10-03 抓取，HTTP 200，5.18MB，pdftotext 提取 443 行）
  快照：`D:/KF/LSSMJ/scratch/w8a/raw/ggxrd_motomura.txt`
- 讲者：Junya Christopher Motomura（Arc System Works 技术美术/角色建模，演讲自述）
- 官方 session 页：`https://www.gdcvault.com/play/1022031/Guilty-Gear-Xrd-2D-Looks`（W8A-033）
- 账本：W8A-001..012、086..093

## 要点（逐条带账本锚）

1. **目标句**：在完整 3D 框架内重建 2D 格斗观感（"rebuild a classic 2D fighting game within a modern full-3D graphical framework" —— W8A-033，官方 session 摘要逐字）。
2. **几何预算**：角色平均约 4 万三角面，理由是过场特写要抗住极近景（W8A-001）。细节直接建模进几何（"Details were directly modeled-in with geometry."）。
3. **弃法线贴图**：角色不用 normal map 类纹理，信息塞进顶点属性（法线/顶点色/UV）（W8A-002）；理由是纹理分辨率相关、特写下必锯齿，顶点属性线性插值则分辨率无关（W8A-011）。
4. **着色=step**：卡通着色"表面非亮即暗"，真正可控的只有阈值/光向量/法线三件（W8A-003、W8A-092）；官方自述 shader 代码"pretty simple"（W8A-093）。
5. **法线是美术数据**：手工修改所有主要特征处的法线，面部尤其要手作（W8A-004）——因为卡通阈值会把任何法线噪声放大成"巨大污斑"。
6. **逐角色光照**：没有全局光照系统照角色，每个角色有自己的光向量；过场里逐帧动画该光向量（W8A-010）。
7. **描边 = Inverted Hull**：shader 里生成第二套暗色多边形沿法线外扩（W8A-006）；与后处理描边对比后明确选了前者，因为可在建模视口预览、顶点级控制宽度、可局部擦除（W8A-005、W8A-007）。
8. **内描边**：面内分界线用"轴对齐光束 + UV 对齐重叠"技巧，换取特写不外锯齿（W8A-008）。
9. **内描边代价**：UV 会被扭曲得很厉害，但"我们不在纹理上放细节"所以无所谓（"we do not put any details on the texture"）。
10. **动画 = 有限动画**：关键帧之间不插值，配大量骨骼（400–600 级，"a LOT of bones"）保持 2D 停格感（W8A-009、W8A-012）。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **学**：①"3D 管线 + 2D 外观"的目标表述与 LSSMJ 场景档同构（几何/可见性/纹理复用，shading 与描边是替换层）；②法线/顶点色作为美术数据通道的资产契约；③描边=独立 pass + 逐顶点宽度；④"分辨率无关"判据（该进顶点的不放纹理）。
- **弃/慎**：①手工改法线依赖 Softimage 级 DCC 工具链（LSSMJ 暂无可供的美术工具，需先立工具账）；②逐角色专灯光照与 photoreal 全局光路冲突，需在渲染图里开"按对象覆写光照"的口子；③40k 三角/角色 + 双套几何（描边壳）的预算要进 LSSMJ 的场景档记账。
- **不可直接搬**：Xrd 是固定镜头 2D 平面格斗——光照/描边的取巧前提（视角几乎不变）在自由视角场景不成立。

## 与 photoreal 共核的判据贡献（本份）

- 反证半边：Xrd 的"2D 外观"是通过**改数据**（法线）与**加 pass**（描边壳）实现的，几何/材质/动画仍是标准 3D 资产 —— 支持"共核 + 换 shading/描边层"。
- 但注意其**光照被逐角色覆写**（W8A-010），说明"共核"必须包含"per-object 光照覆写"这一能力，否则美术目标达不到。

## 未验证

- GDC 2015 演讲视频正文（Vault 需登录）未看；本文只依据官方 handout PDF + 官方 session 摘要页。
- 400–600 骨骼数为行业转述（本次未在 handout 中找到具体数字，账本未收该条）。
