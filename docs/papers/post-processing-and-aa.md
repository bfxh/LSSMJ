# 后处理链与抗锯齿（第五轮 W7A；2026-10-02）

> 范围：TAA 谱系与"为什么糊"、空间 AA（FXAA/SMAA/MLAA）、MSAA 的现代地位、Bloom/Glare、DOF/运动模糊、
> 后处理管线架构（UE/Unity/Godot）、compute vs 全屏 pass。
> 账本：`../analysis/ledger/w7a.jsonl`（**161 条**：paper 99 / doc 52 / source 6 / web 4；verify rejected=0）。
> 上游 commit：playdeadgames/temporal @ `4795aa0007d464371abe60b7b28a1cf893a4e349`（commit 行=W7A-161；
> 快照 `D:/KF/LSSMJ/scratch/w7a/raw/`，转换规范见各短分析）。
> 短分析（10 篇，均在本目录）：`taa-playdead-inside` / `taa-karis-ue4-2014` / `taa-salvi-variance-clipping-2016` /
> `taa-survey-2020` / `fxaa-lottes` / `smaa-jimenez-2012` / `mlaa-reshetov` / `codaw-postfx-2014` /
> `killzone-temporal-aa-2014` / `engine-post-chains`。

## TL;DR（10 条，每条带账本锚）

1. **TAA=事实标准，且是"反馈回路"**：综述定位"Particularly suitable for deferred renderers, replacing MSAA"
   （W7A-051）；Playdead 定义"output becomes next frame in history (feedback loop)"（W7A-001）——
   历史缓冲不是可选项，是结构本身。
2. **"TAA 为什么糊"有机制级答案（两因）**：①历史重采样逐帧损失高频（"Culprit #1: History resampling"，
   W7A-053）；②历史裁剪/钳制本身引入模糊（"Culprit #2: History-clipping/clamping"，W7A-054）。
   引擎文档同口径：Godot"静止即糊、运动更糊"（W7A-147）。
3. **TAA 的硬前置=速度缓冲（含膨胀）**：Karis"Motion without correct velocity will smear"+"Take front most
   velocity"（W7A-034/035）；Playdead 用 3x3 最近深度取速度（W7A-005）、重投影=读取+减法（W7A-004）；
   Unity HDRP 明示"must enable motion vectors"（W7A-137）。速度标注有资产成本（W7A-025）。
4. **邻域裁剪是 TAA 质量的"主因"，已有三代实现**：3x3 min/max（W7A-036）→ 圆化+clip（Karis，W7A-037/038）→
   方差裁剪 VC（Salvi，"main ingredient"，W7A-045/046，γ=1，W7A-047/048）。
5. **TAA 的公开成本量级≈1080p/1.7ms**（XB1，2016，Playdead 自述，W7A-015）；10/90 混合比与
   0.88–0.97 反馈区间双来源互证（W7A-044 / W7A-017）。
6. **FXAA=最省的空间档，且有 UI 边界纪律**：GTX480/1920x1200 <1ms（W7A-069）；"applied prior to drawing
   the HUD or UI elements"（W7A-071）；须在 tonemap 之后（HDR 上做=白做，W7A-078）；官方判定桌面已
   被时间 AA 取代（W7A-151）。
7. **SMAA=T2x 1.3ms 的"更锐空间档"**（W7A-081/082）；Unity/Godot 均口径"比 FXAA 锐"（W7A-132）；
   T2x 本质=后滤波+时间重投影（需速度缓冲，W7A-083/084）。
8. **MSAA 的现代地位=窄：** 与 TAA 互斥（Unity W7A-133；Playdead W7A-024）、只治几何边缘（W7A-135/149）、
   对高光无效（W7A-152）、硬件上限 8x（W7A-058）、tiled GPU 上成本反转（W7A-136）。
9. **Bloom/DOF 的工程共识**：bloom=多尺度 mip 链+升采样同步滤波（UE 5 级 1/2..1/32，W7A-123；
   "绝不跨级直接双线性"，W7A-108；Karis average 反 firefly，W7A-112）；DOF=半分辨率+两层 COC
   （W7A-106/107/113；UE 三层结构 W7A-126）。默认都保守：URP bloom 默认强度 0（W7A-139）、
   Godot glow 默认弱/不可见（W7A-153）。
10. **对 LSSMJ 的最小可用集（P5+ 场景档）**：**TAA（有速度缓冲时）/ FXAA（无速度缓冲时）二选一，
    bloom 可选且默认极弱，tonemap 沿用 W6C 的 ACES 拟合；UI 档维持无后处理**（后处理链止于 UI 合成前：
    W7A-071 + W7A-145 双来源）。DOF/运动模糊不列入最小集（成本与屏幕空间近似性，W7A-105/115）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

**吸收**（含"有界吸收"，括号注明条件）：

- TAA 反馈回路+EMA（10/90、0.88–0.97）｜W7A-001/044/017；Halton(2,3) jitter（8/16/32 档）｜W7A-014/020/031；
  速度缓冲+3x3 膨胀+tile max（TAA 前置）｜W7A-004/005/022/035；邻域裁剪三代 min/max→圆化 clip→VC（按档位）｜W7A-036..038/046..048；
  反 flicker/拖尾/糊的验收用例（C11–C13）｜W7A-050/055/054/011。
- FXAA（单 pass、UI 之前、tonemap 之后；最低档）｜W7A-069/070/071/078；SMAA 1x/T2x（有界：更锐档）｜W7A-081/082/132；
  Bloom 多尺度 mip+Karis average（可选、默认弱）｜W7A-123/108/112/139；TAAU（有界：若做动态分辨率）｜W7A-128/129；
  后处理默认开关=显式决策（UI 档关；场景档默认弱开可关）｜W7A-117/146。

**不吸收**（含理由）：

- MSAA 作默认（保留为无 TAA 时的几何档）｜W7A-133/135；DOF 半分辨率两层（本轮最小集外）｜W7A-106/113/105；
  运动模糊（McGuire 重建滤波，最小集外）｜W7A-114/115；卷积 bloom（影视/高端）｜W7A-124；
  compute 后处理作默认（GL 档无、移动驱动差）｜W7A-157/158。

## 来源地图（24 个来源，快照在 `scratch/w7a/raw/`）

| # | 来源 | 类型/年份 | 快照 | 账本 |
| --- | --- | --- | --- | --- |
| 1 | Playdead《Temporal Reprojection AA in INSIDE》GDC2016 | paper（讲稿） | `playdead_gdc_pdf.txt` | W7A-001..016 |
| 2 | playdeadgames/temporal 源码（MIT） | source | `playdead_*.cs/.shader` | W7A-017..022 |
| 3 | 同上 README | doc | `playdead_readme.md` | W7A-023..026 |
| 4 | Karis《High Quality Temporal Supersampling》SIGGRAPH 2014 | paper | `karis_taa_2014.txt` | W7A-027..043 |
| 5 | Salvi《An Excursion in Temporal Supersampling》GDC2016 | paper | `salvi_gdc2016.txt` | W7A-044..050, 159, 160 |
| 6 | Yang/Liu/Salvi《TAA Survey》EG/CGF 2020（演讲版） | paper | `taa_survey_talk.txt` | W7A-051..056 |
| 7 | Koskela 等 AGAA（UE4, SIGGRAPH 2016） | paper | `agaa_s2016.txt` | W7A-057..059 |
| 8 | Guerrilla《Killzone Shadow Fall》GDC2014 | paper | `killzone_gdc2014.txt` | W7A-060..064 |
| 9 | elopezr《Temporal AA and the Quest for the Holy Trail》 | web | `elopezr_taa.txt` | W7A-065..068 |
| 10 | Lottes/NVIDIA FXAA 白皮书 | paper | `fxaa_whitepaper.txt` | W7A-069..080 |
| 11 | Jimenez 等《SMAA》CGF 31(2) 2012 | paper | `smaa_paper.txt` | W7A-081..088 |
| 12 | Reshetov/Jimenez MLAA 复盘（HPG 2017） | paper | `mlaa_retro.txt` | W7A-089..098 |
| 13 | SMAA 作者页（iryoku.com） | doc | `iryoku_smaa_page.txt` | W7A-099..100 |
| 14 | Crossref（SMAA/TAA survey/MLAA 记录） | doc | `crossref_*.json` | W7A-101..103 |
| 15 | Jimenez《Next Generation Post Processing in COD AW》SIGGRAPH 2014 | paper | `codaw_slides.txt`+`codaw_notes.txt` | W7A-104..116 |
| 16 | UE《Post Process Effects》 | doc | `ue_postprocess.txt` | W7A-117..118 |
| 17 | UE《Post Process Materials》 | doc | `ue_ppmaterials.txt` | W7A-119..121 |
| 18 | UE《Bloom》 | doc | `ue_bloom.txt` | W7A-122..125 |
| 19 | UE《Depth of Field》 | doc | `ue_dof.txt` | W7A-126..127 |
| 20 | UE《Screen Percentage with Temporal Upscale》 | doc | `ue_temporal_upscale.txt` | W7A-128..129 |
| 21 | Unity URP《Add anti-aliasing》+ HDRP《Anti-Aliasing》 | doc | `unity_*_aa.txt` | W7A-130..138 |
| 22 | Unity URP Bloom/DOF/FullScreenPass/PP-index | doc | `unity_*.txt` | W7A-139..146 |
| 23 | Godot《3D antialiasing》/《Environment & post-processing》/《Compute shaders》 | doc | `godot_*.txt` | W7A-147..158 |
| 24 | Microsoft《Rasterizer Stage》（MSAA 检索落空留档） | doc | `ms_rast_msaa.txt` | 未入账（无 multisample 文本） |

## 1. 分类学：AA 的三族与"吃什么信息"

AGAA 官方表述（W7A-059）：**超采样光栅**（SSAA/MSAA，吃几何采样）、**预滤波**（mipmap/粗糙度限制，
吃纹理链）、**后滤波**（吃最终颜色：FXAA/MLAA 用空间邻域；时间类用前帧重投影）。三族的失效面互不相同，
信息量递增的谱系（MLAA 复盘列出"若知道更多"：子像素数据→3D 数据→前帧，W7A-096）。

- **空间后滤波**：零额外输入（W7A-095）、成本 <1ms 级（W7A-069）、但边缘是"幻觉"重建（W7A-091/092）、
  有 Nyquist 问题（W7A-093）。
- **MSAA**：硬件采样，边缘最优且零模糊（W7A-149），但只治边缘（W7A-135/149）、上限 8x（W7A-058）、
  与 TAA 互斥（W7A-133）。
- **时间类（TAA/SMAA T2x）**：信息量最大（含时间），但需要速度缓冲（W7A-137）、引入糊（W7A-053/054）
  与鬼影（W7A-055/084）。

## 2. TAA 机制与实现清单（跨四来源合成）

**管线位置**：TAA 是"防火墙"，立在场景渲染与 Bloom/DOF/Post 之间（Karis，W7A-040）；空间滤波在其后。
UE 的 Blendable Location 枚举把这一思想产品化（Before/After Tonemapping，W7A-119/120）。

**实现清单**（每条有公开先例）：

1. jitter：Halton(2,3)（Karis"优于任何硬件 MSAA 排序"，W7A-031；Playdead 前 16 点，W7A-014/020）。
2. 速度缓冲：每物体标注（W7A-025）或引擎内置；3x3 最近深度膨胀（W7A-005/035）；tile 最大速度可选
   （W7A-022）。
3. 历史约束：3x3 min/max → 圆化 clip（W7A-036/037/038）→ 方差裁剪（W7A-046..048）。
4. 累积：EMA 10/90（W7A-044）、反馈 0.88–0.97（W7A-017）、>0.9 需防循环（W7A-013）。
5. 输出：可选运动模糊兜底 `k_trust = invlerp(15,2,||v||)`（W7A-012）。
6. 反 flicker：VC 闪烁机理与对策（W7A-050）；Karis 讲稿另有"接近裁剪时降反馈"段（未单独入账，源 `karis_taa_2014.txt` 反 flicker 页）。

**参数默认值**（可直接抄）：Halton(2,3)16 点、反馈 0.88–0.97、γ=1（VC）、速度阈值 2/15px。

## 3. Bloom / DOF / 运动模糊（工程共识）

- **Bloom**：多尺度 mip（UE5 级 W7A-123；COD 6 级 R11G11B10 W7A-111）；降/升双向滤波，升采样逐级
  且带滤波（tent，W7A-108/109）；firefly 用 Karis average（W7A-112/110）；阈值语义（W7A-125）；
  默认强度保守（URP=0，W7A-139；Godot=弱/不可见，W7A-153）；卷积版（FFT）不入游戏默认档（W7A-124）。
- **DOF**：屏幕空间近似（信息缺失，W7A-105）；半分辨率主滤波（W7A-106/107）；两层 COC 分裂-平均-alpha
  混合（W7A-113）；UE 三层结构（W7A-126）；两档（简单高斯 vs 类相机 Bokeh，W7A-127/142）；
  半径>1 会欠采样（W7A-143）。
- **运动模糊**：McGuire2012 重建滤波为共同基线（W7A-114）；速度方向 tile max 采样区（COD 三 pass=tile max/3x3 邻域/全分辨率模糊，
  讲稿细节未单独入账）；屏幕空间重建的固有边界（W7A-105）。
- **成本锚（公开读数）**：XB1：Veil 0.35ms / MB 0.52ms / DOF 1.27ms（W7A-115）；TAA 1.7ms（W7A-015）；
  FXAA <1ms@GTX480（W7A-069）；SMAA 1x 1.02ms / T2x 1.32ms@GTX470（W7A-082）。

## 4. 对 LSSMJ 的落点：两层 × 最小可用集

### 4.1 UI 档（位图）：不做后处理（正面证据，不是省事）

- FXAA 白皮书："prior to drawing the HUD or UI elements"（W7A-071）；Unity 全屏 pass 默认注入点=
  "After Rendering Post Processing"（后处理之后、AfterRendering 之前，W7A-145）——**两来源独立指向
  同一边界：后处理链止于 UI 合成之前**。
- TAA 对透明/单历史是弱项（W7A-039），UI 是 alpha 密集的贴着相机的内容，进 TAA 只会更糟。
- 结论：UI 档维持 `w5a` 位图契约（壳只贴最终帧），AA 选项仅影响场景层。

### 4.2 场景档（P5+）：最小可用集

| 组件 | 判决 | 依据 | 条件/否证 |
| --- | --- | --- | --- |
| AA：**TAA** | 默认候选 | W7A-051（事实标准）、W7A-015（成本量级） | 能输出速度缓冲（含膨胀）；否则退 FXAA |
| AA：**FXAA** | 备选/降级档 | W7A-069/070（省、单 pass、零额外输入）、W7A-151（现状） | 清晰度优先场景可换 SMAA 1x（W7A-132） |
| AA 叠加 | 可选 | Killzone 出货=TAA+FXAA（W7A-061）；SMAA T2x 与回退（W7A-083） | 成本上升；先不做 |
| **Bloom** | 可选，默认极弱/关 | W7A-139（默认 0）、W7A-153（默认不可见）、W7A-123/109（实现蓝本） | 需 HDR 输入段；降采样起点按档（URP 建议 Quarter，W7A-140） |
| **Tonemap** | 已有（ACES 拟合） | W6C-004/006（第四轮） | — |
| DOF / 运动模糊 | **不列入最小集** | W7A-105（信息缺失）、W7A-115（1.27ms/0.52ms 成本） | 待 P7（动态光/降噪）后再评估 |
| MSAA | 不采用 | W7A-133（与 TAA 互斥）、W7A-135/149（窄应用面） | 若永远不做 TAA，可作几何档选项 |
| compute 化 | 不默认 | W7A-157（仅 RD 系渲染器）、W7A-158（移动驱动差） | 全屏 pass 实现为默认，compute 作优化路线 |
| 卷积 bloom / DLSS 类 | 不吸收 | W7A-124（影视/高端）；ML 路线（survey 讲稿仅方向性提及）未评估 | — |

### 4.3 建议新增判据（供 02-scene-tier.md 的 C 系列）

- **C11 时间收敛**：相机静止 N=8 帧内图像收敛（对偶 survey"rectification 阻止累积"，W7A-056）；
  高频细节场景不闪烁（VC 闪烁机理，W7A-050）。
- **C12 鬼影金丝雀**：草/高频背景+快速运动物体，鬼影可见度 ≤ 阈值（W7A-055 的用例设计；
  Playdead 拖尾观察，W7A-011）。
- **C13 糊对照**：同场景"有/无裁剪"对照（survey 的实验设计，W7A-054）＋重投影扩散检查（W7A-041）。
- **C14 UI 边界**：AA/后处理只作用于场景层，UI 合成后的像素不得被滤波（W7A-071/145 的合成纪律）。

## 坑与反例（负面留档）

1. **HDR 上做 AA=白做**：FXAA 的 0 vs 16 强度算例（W7A-078）；Karis 的 tonemap 前后之争（W7A-033）。
2. **clip 缺圆化→3x3 盒状伪影**（W7A-037）；**clamp 缺 clip→颜色盒角聚集/ghosting**（W7A-038）。
3. **朴素 resolve（线性混合）→ghosting**（SMAA，W7A-084）；深度拒绝+速度加权在 INSIDE 被否证（W7A-006）。
4. **速度缺失/不精确**：无速度会糊（W7A-034）、草/植被缺速度是常见出货 bug（elopezr 记录，未单独入账；速度敏感=W7A-034）。
5. **数值扩散**：重投影让图像变糊（W7A-062），BFECC 类方法在 Karis 处"效果不佳"（W7A-041）——两家结论相反，留档不定论。
6. **Bloom 直接跨级双线性升采样=错误做法**（W7A-108）；**firefly 用非线性映射会脉动**（COD 讲稿细节，未单独入账；firefly 输入问题本身=W7A-110）。
7. **DOF 半径超采样预算→静态噪点**（W7A-143）；**背景重建外扩→"胖形状"**（COD 讲稿细节，未单独入账；两层 COC 结构=W7A-113）。
8. **MSAA 与延迟渲染的错配**（"Too expensive with deferred"，W7A-028）——LSSMJ 若走延迟必须避开。

## 未验证（缺什么证据）

1. **本机零实测**：所有 ms 级读数均为他方自述（平台/年份见各条），LSSMJ 无 TAA/FXAA/bloom 的任何本机基准。
2. **TAA Survey 全文未读**（Wiley 403，W7A-102 的 Crossref 记录为准）；**Adaptive TAA（HPG2018）与
   TU Wien 2023 报告**抓取失败（diglib 分片损坏/下载截断）——"TAA 的自适应采样/梯度分析"一支留缺口。
3. **compute vs 全屏 pass 的性能对比缺一手材料**（GPUOpen/Intel 未找到合格来源；仅有 Godot 的两条
   约束性证据 W7A-156..158）——LSSMJ 若要把某效果 compute 化，需先自测。
4. **UE 的 Anti-Aliasing / Motion Blur 页面**为客户端渲染壳，未取得文本；相关主张改用 Karis 讲稿与
   UE Post Process 文档承载（覆盖度足够但非同一页）。
5. **SMAA 源码仓库最新版许可**未核对（论文 MIT 声明之外的仓内 LICENSE 未读）。
6. **Bloom/glow 的"默认强度"是文档默认值**，不代表美术调校后的常见值（口径差已注明）。
