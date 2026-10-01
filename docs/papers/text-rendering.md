# 文本渲染与整形 论文批（A6 / 账本 w1f）

> 抓取日期 **2026-10-01**。本批为**论文/文档轨道**（无上游源码 commit）：29 个来源、93 条账本
> （paper 66 / doc 27，`verify` 0 拒绝，见 `docs/analysis/ledger/w1f.jsonl`）。
> 引文纪律：每条 `quote` 都是抓取正文（`scratch/a6/flat/<slug>.txt`，空白折叠快照）的**逐字子串**，
> 由 `scratch/a6/mkledger.py` 机器校验后才写盘；**HTML 源另经 `scratch/a6/check_html.py` 直接对原始 HTML 双口径复核（63/63 通过，0 fail）**。抓取用 `curl`/`python urllib`（WebFetch 本机证书坏，全程未用）。
> 与既有 `w3f` 的分工：msdfgen/Slug/UAX#14/#29/#50/parley/pathfinder/imgui 的**入口条目已在 w3f**，
> 本批给的是**论文本体与实现细节**（如 Lengyel JCGT 论文、UAX#50 属性值定义、FreeType 滤波权重）。
> **抓取账目（透明）**：唯一 URL **92** 个（任务上限 90，超 2 个=两处纠错重取：Chlumský 论文的
> DSpace 直链与 GitHub files 降级替代）；产出可用正文 **57** 份（`scratch/a6/flat/`），其余为 404/403/超时
> （chromium.org 旧设计文档、Skia、web.archive、Medium 等，名录见 `scratch/a6/fetch.py` 与 §5）。

## TL;DR（10 条，每条带锚）

1. **SDF 的三件事本体**（Valve Green 2007）：距离场由高分辨率图 → 低分辨率纹理通道（`W1F-002`）；
   **0.5 = 边缘**是 alpha-test 路径的阈值基准（`W1F-003`）；AA 来自天然的「软区」而非后处理（`W1F-007`），
   并可用硬件双线性插值在采样间重建（`W1F-006`）。
   <https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf>
2. **SDF 的圆角损失是原理性的**：单通道「编码边缘必然磨圆角」（`W1F-004`）；msdf 用三通道修尖角
   （`W1F-063` "reproduce sharp corners almost perfectly by using all three color channels"），
   但 Lengyel 论文指出它"required a complicated analysis step in the preparation of the texture atlas"
   （`W1F-011`）；mtsdf(rgb=msdf, alpha=真 SDF) 可兼顾特效（`W1F-066`）。
3. **图集=对无限精度轮廓的离散采样，上限无法消除**（`W1F-012`，Lengyel 2017）；绕开的唯一路线是
   **绘制时从轮廓直接光栅**（Slug `W1F-008/009`；Dobbie 矢量纹理 "let the GPU render from the original
   vector data" `W1F-015`）。对本项目=「图集档必须写明采样上限与缓解手段」。
4. **图集打包有可复算的实测结论**（Jylänki《A Thousand Ways to Pack the Bin》：实测 2619 个变体
   `W1F-056`，"MAXRECTS algorithms perform the best of all" `W1F-057`，离线 GLOBAL 档最佳 `W1F-059`）；
   工程实践反例：**WebRender 把 guillotine 换成了定长 2 幂方 slab**（512×512 区域定长网格 `W1F-060/061`），
   评测口径是"wasted space"（`W1F-062`）。<https://raw.githubusercontent.com/juj/RectangleBinPack/master/RectangleBinPack.pdf>
5. **亚像素渲染的三条硬约束**：ClearType 只对"vertical stripes that are ordered RGB"的 LCD 成立
   （`W1F-074`）；FreeType 默认滤波=五点 `[0x08 0x4D 0x56 0x4D 0x08]`/1/256（`W1F-067`），不滤波会产生
   "sometimes severe color fringes"（`W1F-068`）；亚像素只在 x 向抗锯齿（`W1F-048` "complete absence of
   anti-aliasing in y-direction"），且**无 gamma 时观感「变粗」**（`W1F-049`）。
6. **gamma 是纠错而非美化**：2.2 的选取依据=人眼幂律灵敏度（`W1F-041`）；在 gamma 编码空间里做
   缩放/模糊/合成/插值/抗锯齿**全部错**（`W1F-039`）；FreeType 官方因"需要线性 alpha 混合+gamma 校正"
   而默认关闭 stem darkening（`W1F-070`）——顺序是先 gamma 后字重增强。
7. **提示（hinting）的历史包袱已过期**：TrueType 字节码专利 **2010-05 全球到期**（`W1F-071`，
   FreeType 官方页）；FreeType **2.7 起默认 v40 亚像素 hinting**（`W1F-069`，"emulating a modern version
   of ClearType"）⇒ **版本号即渲染行为**，钉版本必须连带钉渲染快照。autohinter 的职责=把轮廓对齐像素网格
   （`W1F-072`）。
8. **整形不自研有权威背书**：Levien 的长文结论 "a very high quality open source implementation exists,
   in the form of HarfBuzz"（`W1F-052`）；HarfBuzz 水平默认启用 calt/clig/curs/dist/kern/liga/rclt
   （`W1F-081`）；标记定位（越南语等 `W1F-076`）与连写（cursive attachment `W1F-077`）由 GPOS 承载——
   这些都属于"选现成件即得"的能力清单。
9. **rustybuzz 的自宣读数与生命周期**：官方 README 自报 "We're 1.5-2x slower than harfbuzz"（`W1F-082`），
   且已 "unmaintained, and archived"、官方建议迁 HarfRust（`W1F-083`）——与 w1b（A2 文本栈）的源码读数互证；
   引用时注意这是**自宣口径**，不能当第三方实测。
10. **断行/分段/CJK/竖排的规范底座**：流水线顺序=整形（含镜像）→ 量宽 → 断行 → 行内重排（`W1F-020/021`）；
    UAX#14 的 LB28a 与引号规则**需要前瞻**（`W1F-086`）；ICU 对中日泰高棉按**词典**补断词（`W1F-084`）；
    JLREQ 的禁则处理源自 **JIS X 4051**（`W1F-033/034`）；UAX#50 竖排默认（汉字/假名/谚文直立、
    拉丁侧倒 `W1F-028`）；UAX#11 的 Ambiguous 宽度需按上下文解析（`W1F-025`）。

## 可吸收 / 不可吸收（对「单行 + 截断 + 图集档」的文本层）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| gamma 校正表（覆盖率→亮度）随主题/极性走 | `W1F-039/041/049/070` | **吸收**（浅底 0.85/深底 0.75 的既有决策有了原理锚） |
| 亚像素作为"可选档"+否决清单 | `W1F-067/068/074/048` | **吸收**（否决清单照抄"几何/条纹序/彩边金丝雀"三条） |
| 整形=现成件（harfrust/HB 语义），默认特性集照抄 | `W1F-052/081/076/077` | **吸收**（与 A2 结论一致，本条是外部权威背书） |
| 断行实现必须留前瞻窗口（UAX#14 LB28a/引号） | `W1F-086` | **吸收**（自研断行若做，接口先留 lookahead） |
| 宽度判据带 EAW 上下文档位（Ambiguous） | `W1F-025/027` | **吸收**（宽度对表工具要按东/非东上下文分别跑） |
| 图集分配器：MaxRects 系（BSSF 变体）+ 浪费率指标 | `W1F-056/057/059/062` | **吸收**（图集档的分配器选型与验收指标一并用） |
| 图集参数起步：定长 slab（512 区域/定长网格） | `W1F-060/061` | **有界吸收**（先简单后碎片；命中率与浪费率实测后再升级） |
| msdf/mtsdf 生成（含 -angle 角阈值） | `W1F-063/065/066` | **有界吸收**（仅"大字号/极端缩放"再评估；候选窗不用） |
| Slug 式"绘制时从轮廓直接光栅" | `W1F-008/009/012/013/015` | **不吸收**（GPU 档的后备；CPU 基线不做，成本模型不符） |
| 竖排完整支持（换形字符、ruby、混植） | `W1F-031/035/036` | **不吸收当档**（P2/P4 未决；先只落"U/R 查表"最小口径） |
| hinting 全档（字节码解释器/自动提示） | `W1F-069/071/072` | **不吸收**（候选窗量级用不到；记录"专利已过期"以免后人重启旧假设） |

## 1. 来源地图（29 源 / 93 条）

| slug | 深度 | 条 | URL |
| --- | --- | --- | --- |
| valve-sdf-2007 | paper | 8 | steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf |
| lengyel-slug-jcgt | paper | 8 | jcgt.org/published/0006/02/02/paper-lowres.pdf |
| msdfgen-readme | doc | 4 | raw.githubusercontent.com/Chlumsky/msdfgen/master/README.md |
| dobbie-vectortex | paper | 5 | wdobbie.com/post/gpu-text-rendering-with-vector-textures/ |
| uax9 | paper | 5 | unicode.org/reports/tr9/（Unicode 18.0.0, 2026-09-01） |
| uax11 | paper | 4 | unicode.org/reports/tr11/（Unicode 18.0.0, 2026-07-31） |
| uax50 | paper | 5 | unicode.org/reports/tr50/ |
| jlreq-github | paper | 6 | w3c.github.io/jlreq/ |
| johnnovak-gamma | paper | 5 | blog.johnnovak.net/2016/09/21/what-every-coder-should-know-about-gamma/ |
| rastertragedy-ch1 | paper | 4 | rastertragedy.com/RTRCh1.htm |
| rastertragedy-ch2 | paper | 4 | rastertragedy.com/RTRCh2.htm |
| raph-text-layout | paper | 4 | raphlinus.github.io/text/2020/10/26/text-layout.html |
| rectbinpack-pdf | paper | 5 | raw.githubusercontent.com/juj/RectangleBinPack/master/RectangleBinPack.pdf |
| nical-etagere | paper | 3 | nical.github.io/posts/etagere.html |
| ft-lcd-rendering | doc | 3 | freetype.org/freetype2/docs/reference/ft2-lcd_rendering.html（2.14.3） |
| ft-changes-raw | doc | 2 | raw.githubusercontent.com/freetype/freetype/master/docs/CHANGES |
| ft-patents | doc | 2 | freetype.org/patents.html |
| ft-autohint | doc | 1 | freetype.org/autohinting/index.html |
| ms-cleartype | doc | 2 | learn.microsoft.com/windows/win32/gdi/cleartype-antialiasing |
| dwrite-rendering-params | doc | 1 | learn.microsoft.com/windows/win32/api/dwrite/nn-dwrite-idwriterenderingparams |
| ms-gpos | doc | 2 | learn.microsoft.com/typography/opentype/spec/gpos |
| ms-devanagari | doc | 1 | learn.microsoft.com/typography/script-development/devanagari |
| apple-trak | doc | 1 | developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6trak.html |
| apple-fvar | doc | 1 | developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6fvar.html |
| hb-features | doc | 1 | harfbuzz.github.io/shaping-opentype-features.html |
| rustybuzz-readme | doc | 2 | raw.githubusercontent.com/harfbuzz/rustybuzz/main/README.md |
| icu-boundary | doc | 2 | unicode-org.github.io/icu/userguide/boundaryanalysis/ |
| css-text-4 | doc | 1 | drafts.csswg.org/css-text-4/ |
| libunibreak-readme | doc | 1 | raw.githubusercontent.com/adah1972/libunibreak/master/README.md |

**抓取到但未入账（留档，防重复劳动）**：css-text-3 / css-inline-3 / css-fonts-4 / css-writing-modes-4
（草案节选未成条）、ms-glyf / ms-kern / ms-var-overview / ms-use / ms-features-pt / hb-what / hb-why /
hb-clusters / hb-concepts / dwrite-portal / pango-rendering / blink-layout-readme / icu-strings
（文本已抓、可随时补条）；**抓取失败**：chromium.org 的 LCD text 设计文档（404，站内已迁走）、
Skia text 文档（超时）、linebreak 文章（vimgadgets 403/404）、Evan Wallace Medium 文（超时）、
web.archive.org（本机超时）。

## 2. 机制细节（按主题）

### 2.1 SDF / MSDF / 矢量纹理（GPU 路线）
- Valve：生成侧=高分辨率二值图 + spread factor + 8bit 通道（`W1F-002/005`）；消费侧=0.5 阈值、
  软区抗锯齿、双线性插值、阈值可动态调（描边/阴影/光晕同源）（`W1F-003/006/007`）。
- msdf 修角："utilizing all three color channels"（`W1F-063`）；`-angle` 决定"最大多少度算角"（`W1F-065`）；
  mtsdf 双格式并存（`W1F-066`）。Lengyel 的反面证据：msdf 的"complicated analysis step"与
  "difficult-to-avoid artifacts for complex glyphs"（`W1F-011`）。
- Slug（Lengyel 2017）：无需预计算、OpenGL 3.x/DX10 级硬件即可（`W1F-013`）；代价=多射线方向射程
  与性能的权衡、小字号三角形过碎（`W1F-014`）。
- 图集原理上限（`W1F-012`）与"每字形×每字号"内存墙（`W1F-016/017/018`，Dobbie）——**这是本项目
  「图集档 vs SwashCache 直存」判据的外部依据**。

### 2.2 亚像素与 gamma（观感层）
- ClearType 定位与硬件前提（`W1F-073/074`）；DWrite 参数三大旋钮：ClearType level / enhanced
  contrast / gamma（`W1F-075`）。
- FreeType：默认五点滤波权重 `[0x08 0x4D 0x56 0x4D 0x08]`（1/256 单位）（`W1F-067`）；NONE=严重彩边
  （`W1F-068`）；LCD 目标自 2.6.x 起是 LIGHT 的变体（CHANGES）。
- Raster Tragedy 把 ClearType/CoolType/FreeType/Quartz 归一为"亚像素抗锯齿"（`W1F-047`），并给两条
  结构性损失：y 向无 AA（`W1F-048`）、缺 gamma 导致"weight gain"（`W1F-049`）；同时给出对 AA 的
  诚实定义（主观优化，`W1F-050`）——**我们的 AA 验收只写可复算项**。
- Novak：2.2/人眼幂律（`W1F-038/041`）、错清单（`W1F-039`）、可见后果清单（`W1F-042`）。

### 2.3 提示与栅格化
- 专利：《TrueType Bytecode Patents Have Expired!》2010-05（`W1F-071`）；ClearType 色彩滤波专利
  2019-08 到期（同页，未入账条目可补）。
- FreeType 2.7（2016-09-08）起 v40 亚像素 hinting 为默认（`W1F-069`）；stem darkening 默认关闭=
  缺线性 alpha+gamma 时才正确（`W1F-070`）。
- Auto-Hinter 职责（`W1F-072`）；轮廓最小模型=控制点（`W1F-046`）；竖排/横排测量不改变轮廓本体
  （`W1F-045`，Raster Tragedy 第一章的"四方面"框架 `W1F-043/044`）。

### 2.4 整形（复杂文种）
- 分层与"不自研"（`W1F-051/052`）；布局缓存粒度=词边界（`W1F-053`）；断行很难（`W1F-054`）。
- GPOS 承载标记定位（越南语，`W1F-076`）与连写锚点（`W1F-077`）；默认特性集（`W1F-081`）；
  天城文第一步=音节簇切分（`W1F-078`）；HB 的 cluster≠grapheme（`W3F-015/016` 已有，本批补
  实现视角：cluster 在整形中不得断开）。

### 2.5 断行 / 分段 / 竖排 / 平台层
- UAX#9 的规范流水线（`W1F-020/021`）+ 逻辑序解释（`W1F-019/022`）；RTL 文种（`W1F-023`）。
- UAX#11 固有宽度/Ambiguous（`W1F-024/025/026/027`）。
- UAX#50 朝向默认与属性值定义（`W1F-028/029/030/031`）+ 标准自限（`W1F-032`）。
- JLREQ：禁则术语源流（`W1F-033/034/037`）、割注双行等长（`W1F-035`）、ruby/混植（`W1F-036`）。
- CSS Text 4 把断行质量写进规范（text-wrap-style: balance/stable/pretty/avoid-short-last-line，
  `W1F-085`）；libunibreak 的前瞻实现注记（`W1F-086`）；ICU 词典断词（`W1F-084`）。
- opsz 轴默认 12.0（`W1F-080`）；trak 按字号档（`W1F-079`）——**本项目"文本三件套"的规范侧锚**。

### 2.6 图集打包
- 实测结论（`W1F-055/056/057/059`）+ Guillotine 的切分约束（`W1F-058`）；WebRender 的 slab 化
  与浪费率口径（`W1F-060/061/062`）。

## 3. 数字与口径（可复算清单）

| 数字 | 口径 | 锚 |
| --- | --- | --- |
| 0.5 | SDF 纹理值=边缘；alpha test 阈值 | `W1F-003` |
| `[0x08 0x4D 0x56 0x4D 0x08]` | 5 taps / 1/256 单位（FreeType 默认滤波） | `W1F-067` |
| 1.5–2× | rustybuzz 自宣 vs harfbuzz（自报口径，非第三方） | `W1F-082` |
| 2.2 | 显示系统标准 gamma（约等于人眼幂律） | `W1F-038/041` |
| 2619 | 装箱变体实测样本量 | `W1F-056` |
| 512×512 | WebRender slab 分配器区域尺寸 | `W1F-061` |
| 2010-05 / 2016-09-08 | 字节码专利到期月 / FreeType 2.7 默认 v40 | `W1F-071/069` |
| Unicode 18.0.0（tr9 2026-09-01 / tr11 2026-07-31） | 规范版本（引用必须带） | `W1F-019/024` |
| 2048 units/em、10 FUnit=4.88E-3 em | AAT trak 的 FUnit 换算示例 | `W1F-079` |
| opsz 默认 12.0 | fvar 轴缺省值 | `W1F-080` |
| 2.10.3 | FreeType 起默认启用 LCD 滤波 | `W1F-087` |
| 2019-08 | ClearType 色彩滤波专利到期月 | `W1F-088` |

## 4. 坑与反例（负面留档）

1. **SDF 圆角不是 bug 而是编码方式**（`W1F-004`）——不要试图用「调分辨率」以外的办法修。
2. **msdf 的修法自带新坑**：预处理分析复杂 + 复杂字形难避伪影（`W1F-011`）——选型时把这部分成本
   算进构建期。
3. **不滤波的亚像素=彩边**（`W1F-068`）；且亚像素只在 x 向 AA（`W1F-048`）——「更清晰」与「更粗糙」
   同时发生，验收必须两个方向都测。
4. **无 gamma 的亚像素会「变粗」**（`W1F-049`），而无 gamma 就开 stem darkening 也是错的
   （`W1F-070`）——顺序错就会两头不讨好。
5. **FreeType 版本升级会改渲染结果**（2.7 默认 v40，`W1F-069`）——"钉版本"必须连带"钉渲染快照"。
6. **Guillotine/图集碎片**：切分约束使摆放不能跨切线（`W1F-058`）；WebRender 直接用定长 slab
   绕开碎片（`W1F-060`）——小规模场景"更聪明的分配器"未必更优。
7. **LB28a/引号需要前瞻**（`W1F-086`）——单遍贪心断行在 CJK 标点上必错。
8. **竖排的"旋转"不是全部**：少数码点要换形（`W1F-031`），标准也自限"非出版级"（`W1F-032`）。
9. **UAX#14 的标点类断行在 Ambiguous 宽度上放大误差**（`W1F-025`）——宽度对表要按上下文分组测。

## 5. 未验证 / 缺口

1. **Chlumsky 硕士论文 PDF 未取到**：CTU DSpace 直链三次尝试均返回**截断流**（671500/978944/
   1153927/457495 字节，皆无 `%%EOF`，pdftotext/pymupdf 均 0 页），GitHub files 镜像（github.com）
   本机 TLS/超时失败，web.archive 超时。故 MSDF 的 4 条（`W1F-063..066`）锚在**官方 README**（任务允许
   的降级路径），论文侧结论（cut-corner 分析、误差修正章）**未逐字读过**——不要引用论文内页码/章节。
2. **Valve 论文的 PDF 文本为双栏混排**：个别跨栏句子无法作为连续引文（已规避），需要读全文时建议
   按栏重排（`pdftotext -layout` 原文件在 `scratch/a6/raw/valve-sdf-2007.pdf`）。
3. **Chromium LCD text 设计文档**（w2c 引用过其 11 项否决清单）：chromium.org 旧路径 404、镜像未找到，
   本批**未独立复核**——w2c 的读数仍是一手来源。
4. **Evan Wallace 的 GPU 文本渲染长文、Skia text 文档、vimgadgets/liblinebreak 文章**：本机不可达
   （超时/403/404），未入账。
5. **jlreq 的"割注等长"** 只在英文译文中读到（原文同时有日文），未核对日文原文措辞。
6. ICONV：本轮未覆盖 **OpenType 'CPAL'/COLR 彩色字形**、**Emoji 分段**、**HarfBuzz 的性能读数**
   （HB 官方无公开基准）——若 LSSMJ 要彩色字形档，须另起一批。

## 6. 逐篇短分析（docs/papers/）

| 文件 | 对象 | 账本 |
| --- | --- | --- |
| `valve-sdf-2007.md` | Green 2007 Valve SDF | `W1F-001..007,090` |
| `lengyel-slug-2017.md` | Lengyel JCGT 2017（Slug） | `W1F-008..014,091` |
| `msdf-glyph-atlas.md` | msdfgen/msdf-atlas-gen（论文 PDF 降级说明） | `W1F-063..066` |
| `gpu-vector-text-dobbie.md` | Dobbie 矢量纹理 | `W1F-015..018,092` |
| `atlas-packing.md` | RectangleBinPack + WebRender slab | `W1F-055..062` |
| `gamma-and-subpixel.md` | Novak + Raster Tragedy + ClearType/DWrite | `W1F-038..050,067,068,073..075` |
| `freetype-hinting-lcd.md` | FreeType 滤波/提示/专利/autohinter | `W1F-067..072,087,088` |
| `harfbuzz-shaping.md` | HB 特性集 + GPOS + Devanagari + rustybuzz | `W1F-076..078,081,082,083` |
| `uax-text-properties.md` | UAX#9/#11/#50 | `W1F-019..032` |
| `cjk-and-linebreak.md` | JLREQ + ICU + libunibreak + CSS Text 4 | `W1F-033..037,084..086,089,093` |
| `raph-text-layout.md` | Levien 文本布局长文 | `W1F-051..054` |
