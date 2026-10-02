# 三渲二 / NPR 全线研究（第六轮 W8A；抓取 2026-10-03；无上游 commit——文献/文档批量，逐 URL 快照在 `scratch/w8a/raw/`）

> 任务：为 LSSMJ 的 **P-NPR 阶段**（用户终局目标之一）做三渲二/非真实感渲染全线材料。
> **核心设计问题**：三渲二与 photoreal 能否共用同一内核（几何/可见性/纹理/后处理），只换 shading 与描边层？
> 账本：`../analysis/ledger/w8a.jsonl`（103 行：paper 53 / source 25 / doc 12 / web 13，verify rejected=0）。
> 短分析（10 份，含逐条锚）：`ggxrd-2d-looks-3d-pipeline.md`、`mihoyo-toon-lineage.md`、`unity-toon-shader-uts.md`、
> `genshin-sdf-face-shadow.md`、`outline-techniques.md`、`npr-classic-papers.md`、`hair-shading-kajiya-marschner.md`、
> `stylized-pbr-shared-core.md`、`botw-rime-stylized-world.md`、`2d-in-3d-workflow.md`。
> 引文纪律：source 行引文经 `tools/ledger.py` 逐字校验；URL 行引文由本仓自检脚本对同一份本地快照逐字比对后才落账。

## TL;DR（10 条，每条带账本锚）

1. **共核可行，且已有官方级样本**：Unity 官方 UTS3 一套 toon shader 兼容 Built-in/URP/HDRP 三条管线（W8A-040），各方共用同一份光照核 `UTSLighting.hlsl`（W8A-044）。"三渲二 = 换 shading/描边层"这一命题在官方工具链里是既成事实。
2. **最直白的共核样本是 kShading**：README 自述 Toon Lit "supports all features of the Lit shader but uses a stepped physical approximation BSDF"（W8A-063）；实现=对 GGX 镜面项 `step` 量化两瓣（W8A-064/065）。**同管线、同特性、只换 BSDF**。
3. **GGXrd 的"2D 外观"由三件事构成**：Inverted Hull 描边（W8A-006）、手工法线编辑（W8A-004/103）、逐角色专光（W8A-010）——全部是"改数据/加 pass"，几何与材质仍是标准 3D 资产。
4. **SDF 面部是极小分支**：`step(归一化前向点积, SDF掩码)` 一行判光（W8A-059）+ 左右双掩码按光照方向选择（W8A-058）+ 每帧 CPU 送 LdotF（W8A-053）。面部要有专属 shading 分支已是硬结论（W8A-060）。
5. **ramp 的工业形态=2D 纹理 + 材质分区**：横轴 halfLambert、纵轴由 lightmap.a 五档分区选行（W8A-054）；昼夜（冷阴影）折进同一图 V 轴 +0.5（W8A-055）。
6. **描边默认档 = inverted hull，且宽度是距离/FOV 的函数**：官方两法=法线挤出/整体缩放（W8A-041）；宽度按相机距离 smoothstep 缩放（W8A-046）或视空间深度补偿（W8A-062）；逐顶点宽度图/顶点色（W8A-005、UTS Outline Width Map）。
7. **后处理是描边的第二条腿**：UE 官方 custom depth buffer 做对象掩码（W8A-036）+ 后处理材质 blendable 挂链（W8A-035）；线宽控制可建在 Jump Flooding 距离场（常数轮次近似，W8A-017..020）。工业分工惯例=角色背面法、场景图像检测（W8A-098/099）。
8. **头发谱系清晰**：Kajiya-Kay 是默认基线（W8A-021、071），Marschner 修其缺口（多重高光/绕轴变化，W8A-022/023），实时化走 dual scattering（W8A-078）；工程侧用高低频双层高光+Jitter（W8A-031）。
9. **NPR 有挂在物理光路核心上的公开先例**：SIGGRAPH 2021 在传统光追器上做"改造"产生卡通/绘画风格、保留反射/折射/GI（W8A-025/026/091）。
10. **风格化的第一性理由是"可读性/记号化"，不是"好看"**：BotW 官方口径（CEDEC2017 报道）「スタイライズド」=「記号化された」（W8A-066），动机是玩家动作→世界快速可辨识反馈（W8A-067）；Gooch 1998 的原始动机同样是"信息传达"（W8A-080）。

## 来源地图（22 项；paper=学术，doc=官方文档，source=本地快照/仓库源码，web=二手旁证）

| # | 来源 | URL | 深度 | 账本 |
| --- | --- | --- | --- | --- |
| 1 | GGXrd GDC2015 官方 handout（Arc System Works） | https://www.ggxrd.com/Motomura_Junya_GuiltyGearXrd.pdf | paper/source | W8A-001..012, 086..093, 103 |
| 2 | GDC Vault：Xrd session 摘要 | https://www.gdcvault.com/play/1022031/Guilty-Gear-Xrd-2D-Looks | doc | W8A-033 |
| 3 | 米哈游官方 deck：Genshin 主机版渲染（docswell） | https://image.docswell.com/slide/KWRPQ5/98VYJZ8Q/download | paper | W8A-013..016 |
| 4 | 游戏葡萄：Unite2017 米哈游演讲实录 | https://www.gameres.com/750798.html | web | W8A-028..032, 104, 105 |
| 5 | 博客园转录（同场演讲） | https://www.cnblogs.com/nafio/p/9137010.html | web | W8A-098..100 |
| 6 | Unity Toon Shader 官方文档（index/Outline/ShadingStep） | https://docs.unity3d.com/Packages/com.unity.toonshader@0.9/manual/index.html | doc | W8A-039, 040, 041, 042, 043 |
| 7 | UTS 官方仓库源码（UTSLighting / URP Outline / UCTS_Outline / 3D-as-2D 工具） | https://github.com/Unity-Technologies/com.unity.toonshader | source | W8A-044..048 |
| 8 | URP_Toon（社区·原神风，含 SDF 脸与描边 pass） | https://github.com/ChiliMilk/URP_Toon | source | W8A-049..053 |
| 9 | GenshinCelShaderURP（社区·原神拆解） | https://github.com/Gaolingx/GenshinCelShaderURP | source | W8A-054..057 |
| 10 | URPSimpleGenshinShaders（社区·面部 SDF 最小实现） | https://github.com/NoiRC256/URPSimpleGenshinShaders | source | W8A-058..062 |
| 11 | kShading（社区·stylized PBR） | https://github.com/Roland09/kShading | doc/source | W8A-063..065 |
| 12 | Jump Flooding（Rong & Tan, I3D 2006，PDF） | https://www.comp.nus.edu.sg/~tants/jfa/i3d06.pdf | paper | W8A-017..020, 088, 089 |
| 13 | Marschner 2003 官方论文页（Stanford graphics） | https://graphics.stanford.edu/papers/hair/ | paper | W8A-021..024, 086, 087 |
| 14 | NPR 光追 poster（SIGGRAPH 2021，PDF） | history.siggraph.org/.../2021-Poster-40-Moon_Non-photorealistic-ray-tracing-with-paint-and-toon-shading.pdf | paper | W8A-025..027, 090, 091 |
| 15 | Unreal 官方：Post Process Materials | https://dev.epicgames.com/documentation/en-us/unreal-engine/post-process-materials-in-unreal-engine | doc | W8A-035, 036 |
| 16 | Live2D 官方：About Deformers | https://docs.live2d.com/en/cubism-editor-manual/deformer/ | doc | W8A-037, 038 |
| 17 | GDC Vault：Rime session 摘要 | https://gdcvault.com/play/1020812/Rime-A-Symphony-of-Images | doc | W8A-034 |
| 18 | IGN Japan：BotW CEDEC2017 讲演报道 | https://jp.ign.com/the-legend-of-zelda-hd/17049/botw3d | web | W8A-066..068 |
| 19 | Chuapp：BotW GDC2017 编译稿（含幻灯片翻拍） | http://www.chuapp.com/article/282419.html | web | W8A-101 |
| 20 | Simon Schreibt：Rime 风格化 VFX 讲演页 | https://simonschreibt.de/gat/stylized-vfx-in-rime-water-edition/ | web | W8A-102 |
| 21 | Crossref API（11 篇经典论文题录核对） | https://api.crossref.org/works/<DOI> | paper | W8A-069..079 |
| 22 | Semantic Scholar API（6 篇摘要逐字） | https://api.semanticscholar.org/graph/v1/paper/DOI:<doi> | paper | W8A-080..085, 094..097 |

## 可吸收 / 不可吸收（对 LSSMJ 三渲二轨 P-NPR）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| shading 层与管线解耦（跨后端共用一份光照核） | W8A-040/044 | **吸收**（架构第一原则） |
| 三色体系 + Step/Feather 参数化阴影边界 | W8A-043/045 | 吸收（shading 公共接口） |
| ramp 纹理（横=光照、纵=风格/材质/时段）+ lightmap 通道分区 | W8A-054/055/029 | 吸收（角色默认档） |
| SDF 面部分支（L/R 双掩码 + Step/Feather + 逐帧 LdotF 上传） | W8A-049/053/058/059 | 吸收（含掩码生成工具账） |
| inverted hull 描边（法线外扩/缩放两模式，宽度图+距离衰减+Z 偏移，独立 pass 开关） | W8A-041/042/046/048 | 吸收（描边默认档） |
| 后处理描边（custom depth/stencil + 边缘检测）+ JFA 距离场线宽 | W8A-035/036/017 | 有界吸收（质量档/内描边/远景） |
| "角色背面法 + 场景图像检测"分工 | W8A-098/099 | 吸收（分档依据，二手旁证） |
| Kajiya-Kay 变体头发（切向高光+双层+Jitter） | W8A-021/031 | 吸收（默认档）；Marschner/双散射留质量档（W8A-024/078） |
| 逐角色/逐场景光照与颜色覆写（无视实际光色） | W8A-010/UTS index | 有界吸收（须与 photoreal 档互斥/可切换） |
| 帽/链式"有限动画"（关键帧不插值） | W8A-009/012 | 有界吸收（动画轨参数，非渲染核） |
| GI 退化（丢 SH 细节项留常数） | W8A-061 | 有界吸收（NPR 档参数） |
| 公告板 LOD / Billboard clouds | W8A-079 | 有界吸收（远景；octahedral impostor 一手缺，先不用） |
| Live2D 式网格+变形器（2D 角色路线） | W8A-037/038 | 有界吸收（做 2D 立绘角色时再启用） |
| 笔刷式描边（轮廓提取→连接→分段→笔触） | W8A-100 | 不吸收（离线/质量档，超出实时内核目标） |
| 2D 图像/视频风格化谱系（Kyprianidis 分类学） | W8A-076/085/097 | 不吸收到场景轨（后效/滤镜轨单列） |
| TF2/Rime 的美式卡通报导 | 无一手 | **不吸收**（本批未获一手，不做主张） |

## 1. 经典案例

### 1.1 Guilty Gear Xrd（Arc System Works，GDC 2015）
- 目标句（官方 session 摘要逐字）：在完整 3D 框架内重建 2D 格斗观感（W8A-033）。
- 手法四件套：**弃法线贴图**（信息进顶点属性，分辨率无关优先，W8A-002/011）；**细节直接建模进几何**（W8A-106）；**手工法线**（每个主要特征都改，面部最重，W8A-004/103）；**Inverted Hull 描边**（顶点着色器控宽、可局部擦除、可预览；明确对比过后处理描边，W8A-005..007）；**内描边**用轴对齐光束+UV 重叠（W8A-008）。
- 官方结论句：令人信服的 2D 观感只能来自对明暗分布的精确控制（W8A-107）。
- 光照：无全局光照照角色，逐角色专光、过场逐帧动画（W8A-010）。
- 动画：有限动画（关键帧不插值）+ 大量骨骼（W8A-009/012）。
- 读法：它是"**改数据 + 加 pass**"路线的极致样本；技术自述"nothing innovative technology-wise"（演讲原话），难点在美术工具链与覆盖度。

### 1.2 米哈游谱系（Unite 2017 → 原神主机版官方 deck → 社区还原）
- 2017（二手转录）：多材质多通道 2D Ramp（W8A-028/029）、面部用顶点色 mask 压阴影（W8A-104）、头发各向异性+高低频双层高光（W8A-030/031）、单独 2K 角色 shadowmap+PCSS+半透明阴影（W8A-105）、后处理重调（W8A-032）、勾线三法（W8A-098..100）。
- 官方 deck（paper）：clustered deferred 1024 灯（W8A-013）、局部灯阴影压缩 29.85:1（W8A-014）、运行期 compute shader 解压（W8A-108）、角色 capsule AO（W8A-015/016）——**卡通角色跑在通用管线上**。
- 社区还原（source）：ramp 行由 lightmap.a 分区（W8A-054）、昼夜折 V 轴（W8A-055）、SDF 脸（W8A-049/058/059）、头发投影遮罩（W8A-050）、GI 细节项丢弃（W8A-061）。
- 读法：米哈游路线证明"**同一管线 + 角色专属 shading 分支 + 专属 AO/阴影参数**"可行；其官方一手仅到管线级数字，角色 shader 细节全部是第三方还原。

### 1.3 BotW / Rime（旁证级，web/doc）
- BotW：风格化=记号化、响应可读性动机、三层空间参数从 200+ 砍到 50（W8A-066/067/068、101）。**无官方幻灯片**，全是报道。
- Rime：官方 GDC 摘要只有美术方向（W8A-034）；技术侧公开材料仅第三方 VFX 讲演（W8A-102）。**本批不主张其渲染管线细节**。

## 2. NPR 理论谱系

| 谱系 | 锚 | 对 LSSMJ 的用法 |
| --- | --- | --- |
| 信息传达派（Gooch 1998；Saito 1990 G-buffer） | W8A-080/081/094/095 | 影调重分配（中调留边线/高光）；G-buffer 是描边与后效的共同底座 |
| 动画生产派（X-Toon 2006；Lake 2000；Anjyo 2003；TF2 2007） | W8A-069/075/083/073/074/096 | 2D ramp+抽象法线（X-Toon）；三类线（Lake 083）；实时素描（Lake 096）；风格化高光（Anjyo） |
| 综述/术语（Rusinkiewicz 2008；Kyprianidis 2013） | W8A-077/076/085/097 | 线型术语对齐；2D 谱系单列不混用（097=分类学） |
| 实时化（JFA 2006；dual scattering 2008） | W8A-017..020/088/089/078 | 距离场（种子集/常数轮次）与头发实时近似入质量档 |
| 头发物理修正（Kajiya-Kay→Marschner） | W8A-021..024/071/086/087/109 | 默认档 Kajiya-Kay 变体，质量档 Marschner（109=模型解释路线） |
| 共核/光追改造（SIGGRAPH 2021 poster） | W8A-025..027/090/091 | 光追器上改造做 NPR，保留反射/折射/GI |

## 3. 描边技术（结论表）

| 路线 | 证据 | LSSMJ 判定 |
| --- | --- | --- |
| inverted hull（物体空间法线/缩放外扩；烘焙法线；宽度图；Z 偏移；距离衰减） | W8A-041/042/046/047/006 | 默认档（角色） |
| inverted hull（裁剪/视图空间外扩=屏幕均匀宽） | W8A-052/056 | 默认档的推荐变体 |
| 后处理边缘（custom depth/stencil + 图像检测） | W8A-035/036/082/095/099 | 质量档（场景/内描边） |
| 距离场线宽（JFA 常数轮次） | W8A-017..020 | 质量档（线宽渐变/外发光） |
| 内描边（轴对齐光束+UV / 深度 rim） | W8A-008/057 | 特写抗锯齿判据的必答项 |
| 笔刷法 | W8A-100 | 不吸收 |
- 共性参数族（缺一出瑕疵）：宽度、宽度图/顶点色、Z 偏移、距离/FOV 补偿、独立 pass 开关（W8A-041/042/046/048/062）。
- 记账：hull=第二套几何（顶点/绘制翻倍）；后处理=全屏 pass + 深度/法线双缓冲（与可见性柱共用）。

## 4. SDF 面部与 ramp（角色档核心）

- **SDF 脸**：预生成左右双掩码 → 按光照方向选择（W8A-058）→ `step(归一化前向点积, 掩码)`（W8A-059）→ Step/Feather 两参数给美术（W8A-049）；脸部忽略 N·L cel shade 是共识（W8A-060/061）；需要每帧 CPU→GPU 的 LdotF 参数（W8A-053）。
- **头发→脸投影**独立遮罩（W8A-050），不能靠 shadowmap。
- **ramp**：lightmap.a 五档分区选行（W8A-054）；昼夜折 V 轴（W8A-055）；UTS 的 Step/Feather 与三色体系是官方同名物（W8A-043/045）。
- 对 LSSMJ：该组合（SDF+ramp+双遮罩）实现量级小、依赖资产（掩码/lightmap）与工具（掩码生成、分区标注），属"资产管线成本 > 渲染成本"的典型，须在 P-NPR 立项时把工具账单列。

## 5. 与 photoreal 共核：架构判据（核心问题的回答）

**结论（证据倾向）**：**能共核，但共核的边界要写准**——共用：几何/可见性/纹理/动画/阴影设施、后处理链骨架、材质图结构；**分开**：一个可派发的 shading 层（含 per-object/逐部位覆写与 ramp/Step/Feather 参数）、一个可开关的描边层（几何壳或后处理）、一组 NPR 化的后处理参数。
证据强度：官方/源码双证（UTS 跨管线 W8A-040/044；kShading 同特性换 BSDF W8A-063/064；URP 社区链路复用标准光照/阴影函数 W8A-049 同级代码）> 学术 poster（光追上改造 W8A-025/026/091）> 二手（米哈游工程细节）。**没有任何一份材料显示三渲二需要另立几何/可见性/纹理内核。**

判据组（可验收）：
- **G1 管线无关**：同一份 shading 函数被 ≥2 个后端（wgpu 三后端→GL→CPU 位图）编译使用且视觉一致（参照 UTS 跨 Built-in/URP/HDRP，W8A-040/044）。
- **G2 shading 可派发**：按材质/部位标签派发 shading 变体（面部= SDF、头发=各向异性、身体=ramp），无需改几何/纹理管线（W8A-058/060/031）。
- **G3 per-object 光照覆写**：允许对象级光向量/颜色覆写且与全局光路（阴影/反射）正确合成（GGXrd/UTS 的需求，W8A-010/UTS index；**最强否证风险点**）。
- **G4 描边独立开关**：描边作为独立 pass/层可整体或按对象开关，宽度遵循屏幕空间标定（W8A-048/046/062）。
- **G5 NPR 后处理参数集**：后处理链支持按档替换参数（AO 饱和度/Bloom 色相，W8A-032）而不改架构。
- **G6 分辨率无关纪律**：近景特写下线/色不糊——数据放顶点属性/程序式而非固定分辨率纹理（Xrd 纪律，W8A-002/011；内描边方案 W8A-008/057）。
- **G7 光路复用**：物理效果（反射/折射/GI/软光追）与 NPR 外观可共存（W8A-025/026；GI 退化作为参数 W8A-061）。

**否证条件（必须写进立项）**：若"逐角色专光 + 逐帧光照动画"（W8A-010）与"全局时变光 + 反射/GI"同时启用，per-object 覆写会破坏物理一致性；届时共核退化为"共用设施 + 光照双模"，复杂度上升。**用一个小场景做 G3 的实测是 P-NPR 的第一个实验，不是最后一个。**

## 6. 2D-in-3D 工作流（摘要）

- 纸片/公告板：Billboard clouds 为学术出处（W8A-079）；octahedral impostor 一手材料本批缺失（Epic 相关页已下线）→ 远景 LOD 先用公告板云概念，不引具体管线。
- Live2D 式：官方文档给出"网格顶点变形 + 层级变形器（warp/rotation）"模型（W8A-037/038），与 3D 场景图的节点/变换可共享抽象。
- 动画帧驱动：GGXrd 的有限动画（W8A-009/012）说明"停格"是显式参数，不该被默认插值淹没。

## 7. 对 LSSMJ P-NPR 的落点（阶段建议）

1. **P-NPR-0（原型）**：wgpu 上做一份 shading 核（三色 + Step/Feather，照 UTSLighting 形态）+ 一个角色（SDF 脸 + ramp 身体 + Kajiya-Kay 头发）+ inverted hull 描边（视图空间外扩）；验收=G1/G2/G4/G6。
2. **P-NPR-1（共核验证）**：把同一 shading 核挂进 photoreal 场景（W6 的 CSM/软光追链），验证 G3/G7（专光覆写 × 阴影合成）；记录否证数据。
3. **P-NPR-2（质量档）**：后处理描边/内描边（custom depth 式掩码）、JFA 线宽、Marschner/双散射头发、ramp 昼夜轴。
4. **工具账（前置）**：SDF 掩码生成、lightmap 分区标注、法线编辑/烘焙法线——不做工具则上述全不可用（Xrd 的教训：成败在工具链覆盖率，W8A-103）。

## 未验证（不许当结论）

1. **无任何本机实测**：本批全部为文献/文档/源码阅读；G1–G7 全部"待实现"。
2. **米哈游角色 shader 官方一手缺失**：ramp/SDF/hair 细节全部来自社区还原（source 级代码，但非官方规格）；官方一手仅 console deck（管线数字）。
3. **Rime 渲染管线**：常见说法（PBR 改造为卡通）本批未获一手，不立账（W8A-034 只有美术方向）。
4. **TF2 论文正文未读**（Valve PDF 404，仅题录核对 W8A-074）；Anjyo/X-Toon/Gooch/Saito 正文未读，仅题录+摘要。
5. **Marschner 正文 PDF 获取失败**（Cornell 站点连接中断 2 次），只到摘要页（W8A-021..024）。
6. **X-Toon PDF 未取到**（HAL 反爬 Anubis 拦截）；inria.hal.science 页面级记录存在但本批未打开。
7. **octahedral impostor / Unity Billboard 一手文档缺失**；Blender Freestyle/Line Art 官方页 403（未列源）。
8. **Godot `grow` 描边属性正文**未取到（类参考页文本抓取不含属性描述），未立账。
9. **倒挂 hull 在自由视角动态光下的瑕疵**（暗面壳可见/自遮挡）本批无材料。
10. **UTS 的"逐管线特性差异"清单**（官方 Feature Difference 页）未逐条读——跨管线一致性只到声明级（W8A-040）。
11. **GitHub 页面 URL 未由本机 curl 验 200**：本机到 `github.com`/`raw.githubusercontent.com` 的 TLS 连接失败（000）；相关文件（UTS 文档 md/源码、kShading README/源码、三份社区 shader）全部经 `gh api`（已登录）逐字取得并落快照 `scratch/w8a/raw/`，账本 source 行锚本地快照、doc 行锚规范页面 URL（内容已核，页面可达性未在本机验证）。
12. **抓取失败清单（留档）**：Valve TF2 论文 PDF 404（两路径）；Blender 手册 403；Cornell Marschner PDF 连接中断；HAL X-Toon 被 Anubis 反爬拦截；Epic "Octahedral Impostors" 页无正文；`sites.uw.edu` 项目页未逐字引（仅检索摘要）。
