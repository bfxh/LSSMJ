# 字体工程规范深读（OpenType/TrueType 表体系 · 文档轨道，抓取 2026-10-02）

> **本批定位**：字体**文件格式与字形工程**的规范面（第五轮·字形工程）。与既有三批的分工——
> `w1b`（A2 文本栈）读**消费侧源码**（cosmic-text/swash/fontdb/glyphon…）；`w1f`（A6）读**文本渲染论文**
> （SDF/亚像素/gamma/断行/UAX）；本批只补**上游规范与权威实现文档**：OpenType 表定义、变体字体机制、
> 彩色字体四格式、字体匹配/回退算法、FreeType 实现结构、TT 指令（IUP）、设备度量与提示。
> 即回答一个问题：“青简文本栈（cosmic-text + harfrust + swash/skrifa + fontdb）背后，规范上有什么、没有什么”。
>
> **上游 commit**：无（纯 doc/paper 轨道，无可钉源码）。每条锚 = `URL` + **逐字引文**；引文取自快照
> `D:/KF/LSSMJ/scratch/w7d/flat/<slug>.txt`（HTML 空白折叠；fontconfig 的 sgml 另去标签），
> 由 `D:/KF/LSSMJ/scratch/w7d/gen.py` 机器校验（87/87 逐字命中）后才写盘。
> 账本：`docs/analysis/ledger/w7d.jsonl`，**87 条**（doc 80 / paper 4 / source 3），`verify` 0 拒绝；**49 个来源**（48 在线 URL + 1 GitHub 镜像源码）。
>
> **抓取账目（透明）**：唯一 URL **68** 个（任务上限 85）。**失败留档**：`fonts.google.com`（color_fonts /
> optical_size_axis 两页超时）、`developer.chrome.com` COLRv1 博文（超时）、`fontconfig.org/fontconfig-user.html`
> （HTTP 418 反爬）、GitLab fontconfig raw（Anubis 反爬挑战页，改用 GitHub 镜像）、MS 规范
> `trak` / `recommendations` 页（404；trak 的正典是 Apple 文档，w1f 已有条目）。
> 引文纪律：quote 均为快照逐字子串；`W7D-082..084` 为 source 深度（fontconfig `src/fcmatch.c` 镜像文本，行锚=快照行号）。
> 过程留痕：FreeType 四页的首次 fetch-log 记录被并发覆盖丢失，已重取复核（4/4 返回 200，内容一致）。

## TL;DR（10 条，每条带锚）

1. **sfnt 只是“表容器”**：一轮 4 字节 tag 的目录装载任意表，扩展性来自目录而非版本号（`W7D-001`）；
   目录里的**表长度不含 4 字节对齐 padding**（`W7D-002`）——自解析表（如青简的 trak）唯一的位移真相。
2. **cmap format 14 是变体序列（VS）的唯一通道**，且与 (Platform 0, Encoding 5) 强绑定（`W7D-003`/`W7D-004`）；
   而 VS 的语义来自 UTS#51：VS15=文本呈现、VS16=emoji 呈现、无 VS 默认按 `Emoji_Presentation` 走彩色（`W7D-048`/`W7D-049`）。
   → **emoji 的“黑白/彩色”切换在规范上是 cmap 查询问题，不是渲染问题**。
3. **复合字形=numberOfContours<0，递归引用其它 glyph ID 作组件**（`W7D-009`/`W7D-007`）；
   光栅器为每个字形追加 **4 个 phantom 点**承载左右边距（`W7D-008`），且 `gvar` 会为 phantom 点带增量
   ——**TrueType 变体字体里“度量”本身是可变的**（`W7D-027`）。
4. **IUP 的完整语义是“轮廓内、两分支”**：逐轮廓处理，只动“被触及点之间”的未触及点；区间内=线性插值，
   区间外=整体平移（`W7D-012`/`W7D-013`）；`UTP` 可把点显式标回“未触及”交给 IUP（`W7D-014`）。
   → 这是 TT 指令面与我们项目**直接相关**的少数机制之一（指令级金丝雀/快照判据）。
5. **变体字体是 Apple GX 的遗产**（`W7D-018`）：默认实例可完全忽略变体表（`W7D-020`）；
   用户坐标→归一化坐标默认分段线性（min→-1/default→0/max→+1，`W7D-023`），`avar` 在此之上再做**第二层弯曲映射**（`W7D-024`）；
   形状随轴连续插值、不是离散档位（`W7D-033`）。**青简 opsz 补丁的规范底座就是这三条**。
6. **度量变体按轮廓类型分岔**：TrueType 可走 `gvar` 的 phantom 增量，**CFF2 没有 phantom 点、必须靠 HVAR**
   （`W7D-028`）；HVAR 同时给 advance 与 LSB/RSB 增量（`W7D-027`）；MVAR 用 4 字节 tag（如 `hasc`）索引字体级度量（`W7D-029`）。
7. **彩色字体四格式并存且各有平台归属**（SBIX/COLR/CBDT/SVG，`W7D-045`；OT-SVG 出自 Mozilla+Adobe，`W7D-046`）：
   **只有 OT-SVG 能矢量+栅格混合**（`W7D-043`）且字形自包含（`W7D-044`）；COLRv1=Paint 表组成的**有向无环图**（`W7D-035`），
   支持渐变且色标/alpha 也可变（`W7D-034`/`W7D-036`）；sbix 的 strike 键是 **(ppem, ppi) 双键**（`W7D-039`），
   数据格式由 `graphicType` 四字符码自述（`W7D-040`）。
8. **回退/匹配有三套权威模型且互不等价**：fontconfig=“按覆盖去重的排序列表”（文档：`W7D-052`/`W7D-053`；
   源码判据：trim 只看新增覆盖 `W7D-083`、单属性分数 `v*1000+j*100+k` `W7D-082`、语言不满足记 10000 分 `W7D-084`）；
   CSS=**逐字符**匹配（`W7D-054`）且“已装字体集合”被规范显式留白（`W7D-056`）；DirectWrite=“字符区间→字体”的映射序列（`W7D-058`/`W7D-059`）。
9. **小字号宽度不是缩放的线性外推**：LTSH 逐字形给“可假设线性”的阈值（`W7D-066`）、hdmx 按设备像素（非 pt）选记录
   （`W7D-065`）、VDMX 按宽高比分组且全 0=默认组（`W7D-067`）、gasp 逐字号档声明是否网格拟合/灰度平滑且建议 v1（`W7D-063`/`W7D-064`）。
   → 我们的“宽度对表”在小字号档必须声明口径，不能三者混用。
10. **提示会改图像尺寸**（FreeType 官方：hinting 对齐像素网格，“slightly modifies the dimensions”，`W7D-069`），
    且 FreeType 的 `FT_TRUETYPE_ENGINE_TYPE_PATENTED`=完整字节码解释器档（专利 2010-05 已过期，`W7D-068`）；
    locl 的规范形态=**GSUB type1 的 GID→GID 1:1 替换**（`W7D-079`/`W7D-080`），入口键是注册制语言系统标签（`W7D-081`）。

## 可吸收 / 不可吸收（对“候选窗/自绘渲染器 + 高帧率 UI”这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| sfnt 表目录口径：表长度不含 padding、按 tag 查表 | `W7D-001`/`W7D-002` | **吸收**：自解析表（trak/未来 COLR）的唯一位移真相，防“按偏移差读长度”的经典错。 |
| cmap format 14（VS）与 (0,5) 绑定 | `W7D-003`/`W7D-004` | **吸收**：emoji 文本/彩色切换的规范路径；自写 cmap 解析时 VS 单开一路。 |
| UTS#51 的 VS15/VS16/Emoji_Presentation 三元语义 | `W7D-048`/`W7D-049` | **吸收**：“无 VS 默认彩色”是规范默认，不是我们发明的行为。 |
| IUP 两分支语义 + UTP 配套 | `W7D-012`~`W7D-014`、`W7D-072`/`W7D-073` | **有界吸收**：只用于指令级验证/快照判据；渲染不自研。 |
| 变体三件套：默认归一化 / avar 二次映射 / 连续插值 | `W7D-020`/`W7D-023`/`W7D-024`/`W7D-033` | **吸收**：opsz 补丁的行为基线；按 opsz 精确复算宽度要先过 avar。 |
| 度量变体：HVAR/CFF2 分岔、MVAR、STAT | `W7D-027`~`W7D-030` | **吸收（有界）**：宽度/行高读数来源按字体格式分岔；STAT 是实例名权威表。 |
| COLRv1（Paint DAG、渐变、变体色） | `W7D-034`~`W7D-036`、`W7D-047` | **有界吸收**：emoji 未来路径；现状（swash 源优先级）已覆盖 COLR，v1 支持面需另核（见 §5）。 |
| sbix strike (ppem,ppi) 双键 + graphicType | `W7D-039`/`W7D-040` | **吸收**：选 strike 的判据修正——Retina 场景不能只看 ppem。 |
| CBDT/CBLC 两级索引 | `W7D-041`/`W7D-042` | **有界吸收**：自解析位图字体时按“索引子表→数据”查，勿按 glyph id 直查。 |
| OT-SVG（矢栅混合、自包含字形） | `W7D-043`/`W7D-044` | **不吸收**：需带 SVG 渲染器（依赖/面积大）；记录“唯一矢栅混合格式”这一独有能力即可。 |
| fontconfig 覆盖去重列表 + lang 重罚 | `W7D-052`/`W7D-053`/`W7D-082`~`W7D-084` | **有界吸收**：抄“按新增覆盖去重”的列表语义；勿抄“语言不合仍兜底”的软排序（与 cosmic-text 硬过滤是两套）。 |
| CSS 逐字符匹配 + 本地化名强匹配 + 集合留白 | `W7D-054`/`W7D-055`/`W7D-056` | **吸收**：缺字形判据下探到字符；字体族查询按多名字集合；复算口径须冻结字体清单。 |
| 合成粗体“匹配时视同存在” | `W7D-057` | **吸收**：宽度对表区分真粗体/合成粗体两档（与 swash embolden 实现呼应）。 |
| DirectWrite 区间映射回退（返回字体+消费长度） | `W7D-058`/`W7D-059` | **有界吸收**：接口契约可借鉴（一次解一段），实现仍用 cosmic-text 链。 |
| 设备度量三表（hdmx/LTSH/VDMX）+ gasp | `W7D-063`~`W7D-067` | **有界吸收**：小字号宽度/渲染档的规范口径；快照指纹纳入 gasp 版本。 |
| FreeType 引擎档（PT/UNPATENTED 语义） | `W7D-068` | **旁证**：专利期已过、解释器合法；“档位即渲染行为”再获一锚。 |
| locl=GSUB type1、语言系统标签注册制 | `W7D-075`/`W7D-079`/`W7D-080`/`W7D-081` | **吸收**：与 w3e 应用层条目互证；语言标签口径写进文本栈契约。 |
| 复合字形递归 + phantom 点 + 二阶贝塞尔 | `W7D-007`/`W7D-008`/`W7D-010` | **有界吸收**：轮廓工具/极值工具的实现须知；swash/skrifa 已封装，不自研。 |

## 1. 来源地图（49 源 / 87 条）

| slug | 深度 | 条 | URL |
| --- | --- | --- | --- |
| ms-otff | doc | 3 | learn.microsoft.com/en-us/typography/opentype/spec/otff |
| ms-cmap | doc | 3 | …/spec/cmap |
| ms-kern | doc | 1 | …/spec/kern |
| ms-glyf | doc | 3 | …/spec/glyf |
| ms-ttch | doc | 2 | …/spec/ttch01 |
| ms-ttinst | doc | 4 | …/spec/tt_instructions |
| ms-cvt | doc | 1 | …/spec/cvt |
| ms-cff2 | doc | 2 | …/spec/cff2 |
| ms-otvar | doc | 3 | …/spec/otvaroverview |
| ms-fvar / ms-avar | doc | 3+2 | …/spec/fvar、…/spec/avar |
| ms-gvar / ms-hvar / ms-mvar | doc | 2+2+1 | …/spec/gvar、…/spec/hvar、…/spec/mvar |
| ms-stat | doc | 1 | …/spec/stat |
| ms-colr / ms-cpal | doc | 3+1 | …/spec/colr、…/spec/cpal |
| ms-sbix / ms-cbdt / ms-cblc | doc | 2+1+1 | …/spec/sbix、…/spec/cbdt、…/spec/cblc |
| ms-svg | doc | 2 | …/spec/svg |
| ms-gasp / ms-hdmx / ms-ltsh / ms-vdmx | doc | 2+1+1+1 | …/spec/gasp、hdmx、ltsh、vdmx |
| ms-gsub / ms-gpos | doc | 3+1 | …/spec/gsub、…/spec/gpos |
| ms-featko（locl） | doc | 2 | …/spec/features_ko |
| ms-langtags | doc | 1 | …/spec/languagetags |
| apple-sbix / apple-avar / apple-gvar | doc | 1+1+2 | developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6{sbix,avar,gvar}.html |
| apple-instr（RM05） | doc | 2 | developer.apple.com/fonts/TrueType-Reference-Manual/RM05/Chap5.html |
| apple-overview（RM02） | doc | 1 | developer.apple.com/fonts/TrueType-Reference-Manual/RM02/Chap2.html |
| ft-glyphs1 / ft-glyphs3 / ft-glyphs6 | doc | 1+1+1 | freetype.org/freetype2/docs/glyphs/glyphs-{1,3,6}.html |
| ft-ref-tt | doc | 1 | freetype.org/freetype2/docs/reference/ft2-truetype_engine.html |
| dwrite-fb / dwrite-fb2 | doc | 1+1 | learn.microsoft.com/windows/win32/api/dwrite_2/…IDWriteFontFallback[._MapCharacters] |
| cssfm | doc | 4 | drafts.csswg.org/css-fonts-4/ |
| hb-fallback / hb-font | doc | 2+1 | harfbuzz.github.io/what-does-harfbuzz-do.html、…/harfbuzz-hb-font.html |
| fc-user / fc-fontset | doc | 2+2 | raw.githubusercontent.com/fontconfig/fontconfig/main/doc/{fontconfig-user.sgml,fcfontset.fncs} |
| fc-fcmatch | **source** | 3 | raw.githubusercontent.com/…/fontconfig/main/src/fcmatch.c（镜像文本，行锚） |
| cfwtf | **paper** | 2 | www.colorfonts.wtf/ |
| tr51 | **paper** | 2 | unicode.org/reports/tr51/ |
| gf-colorfonts | doc | 1 | raw.githubusercontent.com/googlefonts/color-fonts/main/README.md |

**读法提示**：MS 规范各表页的“Header/版本/格式”描述与 Apple 手册同表存在**措辞差异但语义同构**（如
`Window.limit`/avar 的 (fromCoord,toCoord)，`W7D-031`/`W7D-032`）；两来源交叉引用的用途是给我们的
自解析实现双锚，防单来源版本漂移。

## 2. 机制细节（按主题）

### 2.1 容器与编码（sfnt / cmap）

- **容器**：表目录 = `sfntVersion(0x00010000|'OTTO') + numTables + 每条 TableRecord(tag/checksum/offset/length)`；
  **length 不含 padding**（`W7D-002`）。→ 对青简：trak 自解析（w5a 已实现）必须按 record.length 切段；
  未来若做 COLR/COLRv1 读取同理。
- **cmap**：多子表共存、除 format 14 外**只应选一个使用**（`W7D-005`）；format 14 是 VS 唯一通道且必须
  (0,5)（`W7D-003`/`W7D-004`）。→ 与 UTS#51 三元语义（`W7D-048`/`W7D-049`）合看：**“emoji 彩色/文本”
  的判据链 = 文本扫描 VS（存在性）→ cmap 14 查变体字形 → 命中彩色字形或回退文本字形**。
- **kern 的 AAT 扩展被 OT 排除**（`W7D-006`）——与 trak 同族，真字体里存在“OT 规范之外的位/版本”，
  自解析须容错。

### 2.2 轮廓、复合字形与 TT 指令

- 二阶贝塞尔（TT）vs 三阶（CFF2）（`W7D-010`/`W7D-016`）；CFF2 的变体走 charstring 内 `blend`、与 gvar 是两套机制（`W7D-017`）；缩放先于网格拟合（`W7D-011`）。
- 复合字形：`numberOfContours<0` 判据（`W7D-009`）、组件=glyph ID + 变换/偏移（`W7D-007`）、
  组件可再复合 → 解析需递归 + 环检测。
- **phantom 四点**（`W7D-008`）= 左右边距在轮廓里的表示；变体字体里 gvar 为其带增量（`W7D-027`）——
  这是“度量会变”的机制来源。
- **IUP/UTP/IP**：轮廓自治（`W7D-012`）、插值/平移两分支（`W7D-013`）、UTP 复位“未触及”态（`W7D-014`）、
  IP 保持与参考点关系（`W7D-073`）、Apple 指令表同义条目（`W7D-072`）。→ 若做 TT 指令金丝雀（与 w1f
  的“专利已过期”条目衔接），从这三条建期望值。
- cvt = 被指令引用的数值表（`W7D-015`）；渲染器档位（`W7D-068`）。

### 2.3 变体字体的表体系

| 表 | 作用（本条锚） | 关键细节 |
| --- | --- | --- |
| `fvar` | 轴/实例定义 | 轴名走 name ID（`W7D-021`）；InstanceRecord 有可选字段 → 两种尺寸（+4/+6），且规范强制按 `axisSize/instanceSize` 步进（`W7D-022`/`W7D-086`） |
| 归一化 | user→normalized | 分段线性 min/default/max（`W7D-023`） |
| `avar` | 二次映射 | 默认归一化之后再弯曲，可多段（`W7D-024`；Apple 版 `W7D-031`） |
| `gvar` | 轮廓增量 | tuple variation store 的变体（`W7D-025`）、RLE 打包（`W7D-026`）；Apple 术语“tuple=增量列表”（`W7D-032`） |
| `HVAR` | 度量增量 | advance/LSB/RSB（`W7D-027`）；CFF2 无 phantom 必须 HVAR（`W7D-028`） |
| `MVAR` | 字体级度量 | tag（hasc…）→ 别表字段（`W7D-029`） |
| `STAT` | 实例命名 | elidedFallbackNameID 兜底（`W7D-030`）；命名实例可任意取值（`W7D-019`） |

**对青简的最短路径**：opsz 已由 cosmic-text fork 的 `normalized_coords(settings)` 交给 skrifa/swash 处理
（w1b/w5a 已读源码）；本批补的是**规范侧边界**——① 默认实例可无变体表（`W7D-020`）；
② avar 参与了归一化（`W7D-024`），因此“opsz=17.0 的归一化值”不能手算线性比例；
③ 若将来让候选窗按 opsz 精确插值宽度/行高，来源分别是 gvar-phantom（TT）或 HVAR（CFF2）与 MVAR（`W7D-027`~`W7D-029`）。

### 2.4 彩色字体四格式

| 格式 | 结构要点（锚） | 平台史（`W7D-045`/`W7D-046`） | 对本项目 |
| --- | --- | --- | --- |
| COLR/CPAL | v0=色层表；v1=Paint DAG（`W7D-035`），渐变/变换/混合，色标与 alpha 可变体（`W7D-034`/`W7D-036`）；v0 颜色=LayerRecord 索引调色板项 | Microsoft | swash 已按“彩色轮廓→彩色位图→轮廓”分派（w1b）；v1 覆盖面待核（§5） |
| sbix | strike=(ppem,ppi)（`W7D-039`）；graphicType 自述（`W7D-040`）；flags bit0 历史恒 1（`W7D-038`） | Apple（macOS 系统 emoji 实际路径） | 选 strike 判据；macOS 字体实测须含 sbix 面 |
| CBDT/CBLC | CBLC 索引子表按“范围或格式变化”切（`W7D-042`）；CBDT 三档位图格式（17/18/19，度量可外置，`W7D-041`） | Google（Android） | 位图 emoji 路径；与 sbix 解析器可共形 |
| OT-SVG | 唯一矢+栅混合（`W7D-043`）；字形自包含、不引用其它 glyph（`W7D-044`） | Mozilla+Adobe | 不吸收（依赖重）；记其独有能力 |

**测试语料**：Google `color-fonts` 仓提供同字形组的 COLRv1（多轮廓格式）与 OT-SVG 生成字体（`W7D-047`），
可当“同字形不同格式”的对照金丝雀。

### 2.5 匹配与回退（三套模型）

- **fontconfig**：`FcFontSetSort` 返回“按 closeness 排序、trim 掉无新增覆盖者”的列表并给出全体并集覆盖
  （`W7D-052`/`W7D-053`）；实现层：单属性分数 `v*1000 + j*100 + k*(字符串?1:0)`（`W7D-082`）、
  trim 判据 `!i || !trim || adds_chars`（`W7D-083`）、语言不满足 `score[PRI_LANG]=10000`（`W7D-084`）。
  配置语义：族名 strong/weak 绑定，**weak 族名让位 lang**（`W7D-050`）；edit 值带 binding 继承（`W7D-051`）。
- **CSS Fonts 4**：逐字符选族+选面（`W7D-054`）；本地化名必须跨平台/编码匹配（`W7D-055`）；
  已装集合显式留白（`W7D-056`）；合成粗体“匹配时视同存在”（`W7D-057`）。
- **DirectWrite**：fallback sequence=“字符区间→字体”映射（`W7D-058`）；`MapCharacters` 一次解一段、
  返回“字体+可消费长度”（`W7D-059`）。
- **HarfBuzz**：Emoji（含 ZWJ/旗帜/修饰符序列）是一等整形模型（`W7D-060`）；非复杂文种走默认模型（`W7D-061`）；
  `nominal_glyph` 明确**不适用**于 VS 修饰的码点、须走 variation glyph 接口（`W7D-062`）。
- **locl**：语言触发的本地化字形替换（`W7D-079`）、GID→GID 1:1（`W7D-080`）、语言系统标签注册制
  （`W7D-081`）、特性按 Script/LangSys/Feature 三级挂载（`W7D-077`）；GSUB 的基础形态=单字形替换（`W7D-075`）、
  反向链式类型是唯一“从后往前”处理者（`W7D-076`）；GPOS 侧变体调整走 VariationIndex（`W7D-078`）。

### 2.6 提示与设备度量（实现结构面，与 w1b/w1f 互补）

- `gasp`：逐字号档声明网格拟合/灰度平滑；两版格式、v1 多两标志，新字体应用 v1（`W7D-063`/`W7D-064`）。
- `hdmx`：按**设备像素宽**（例：72×96 dpi 下 12pt=16px）选记录（`W7D-065`）——非方形像素下 ≠ pt 的 ppem。
- `LTSH`：逐字形“线性阈值”，低于它不能假设宽度线性缩放（`W7D-066`）。
- `VDMX`：按宽高比分组；**全 0=默认组且须排最后**（哨兵语义）（`W7D-067`）。
- FreeType：glyph conventions 的轮廓模型（闭合轮廓，`W7D-070`）、字符≠字形（`W7D-071`）、
  hinting 改图像尺寸（`W7D-069`）、引擎档（`W7D-068`）；Apple：FUnit→像素的缩放式（`W7D-074`）。

## 3. 数字与口径（可复算清单）

| 数字/常量 | 口径 | 锚 |
| --- | --- | --- |
| `0x00010000` / `'OTTO'` | sfnt 版本两值（TT 轮廓 / CFF） | `W7D-085` |
| padding 不计入 `length` | 表目录 TableRecord 口径 | `W7D-002` |
| (Platform 0, Encoding 5) | format 14 的唯一合法绑定 | `W7D-004` |
| `-1` | numberOfContours=复合字形 | `W7D-009` |
| 4（phantom 点） | 每个字形追加的边距点 | `W7D-008` |
| `0x30 - 0x31` | IUP 码位（a=0 y 向 / a=1 x 向） | `W7D-087` |
| min→-1 / default→0 / max→+1 | 默认归一化端点 | `W7D-023` |
| `axisCount*sizeof(Fixed)+4` 或 `+6` | InstanceRecord 两种尺寸 | `W7D-022` |
| 8-bit BGRA | CPAL 颜色记录（sRGB） | `W7D-037` |
| (ppem, ppi) | sbix strike 键；例 96/192 PPI | `W7D-039` |
| U+FE0E | VS15（文本呈现） | `W7D-048` |
| `v*1000 + j*100 + k*(string?1:0)` | fontconfig 单属性比较分数 | `W7D-082` |
| 10000.0 | fontconfig 语言不满足罚分 | `W7D-084` |
| 12pt@72×96 → 16px | hdmx 记录选择示例 | `W7D-065` |
| 0（Ratio 全零） | VDMX 默认分组哨兵 | `W7D-067` |
| 2010-05 | TT 字节码专利到期（FreeType 命名由来） | `W7D-068` |
| `hasc` | MVAR tag → OS/2.sTypoAscender | `W7D-029` |

## 4. 坑与反例（负面留档）

1. **“按相邻表偏移差算长度”是错的**：表长度不含 padding（`W7D-002`），偏移差含对齐字节——自解析表会多读 0–3 字节。
2. **变异选择符不是渲染层的事**：`nominal_glyph` 接口对 VS 修饰码点给出的是**非变体**字形（`W7D-062`）；
   只调 nominal 接口 = VS15/VS16 全失效（emoji 黑白/彩色无法切换）。
3. **AVAR 是隐形第二层**：默认归一化不是最终归一化（`W7D-024`）；手算线性比例与真实插值有偏差，
   且偏差**随字体**（有无 avar、段表）变化——不能用一个系数兜住。
4. **CFF2 的度量变体不来自 gvar**：CFF2 光栅器不产生 phantom 点（`W7D-028`）；
   按“TT 思路”去找 gvar 增量会在 OTF/CFF2 字体上得到空结果。
5. **fontconfig 的“软排序”不是硬过滤**：语言不合仅重罚 10000 分（`W7D-084`），仍留在列表里；
   与 cosmic-text 的四级瀑布（硬过滤 + 兜底）语义不同——**两套结果不可互推**（`W7D-050`~`W7D-053`）。
6. **本地化族名不匹配 = 静默回退**：规范要求跨平台/编码匹配全部本地化名（`W7D-055`）；
   只按英文族名查库，在中文本地化字体（如系统自带字体）上会“莫名”走回退。
7. **合成粗体会污染宽度**：规范把合成粗体当“族里存在的面”参与匹配（`W7D-057`）；
   若对表时不区分真/伪粗体，宽度差异会被记成“字体差异”。
8. **sbix 只看 ppem 会选错 strike**：键是 (ppem,ppi)（`W7D-039`）；Retina/多密度下同 ppem 有多个 strike。
9. **gasp 版本影响渲染行为**：v0/v1 标志语义不同（`W7D-063`/`W7D-064`）；
   渲染快照若不记录 gasp 版本/标志，跨字体对比不可复算。
10. **“四格式都有”不等于“都可用”**：平台史决定分布（`W7D-045`/`W7D-046`）；
    只实现一种彩色路径会在别系统字体上落空——判定要用**目标系统的实际字体文件**验。
11. **IUP 不是全局插值**：轮廓自治（`W7D-012`），跨轮廓不外推；用全局样条近似在复合轮廓上必错。
12. **历史位/哨兵值不是错误**：sbix flags bit0 恒 1（`W7D-038`）、VDMX 全 0=默认组（`W7D-067`）——
    自解析的容错判据要按规范特例写，别把合法数据判成坏表。

## 5. 未验证项

1. **无任何本机渲染实测**：本批全部为文档/规范读数（含 3 条源码行锚），**没有跑任何渲染/栅格实验**；
   “对观感/宽度的影响”类陈述均为规范含义的推断，未在青简上复算。
2. **COLRv1 在 swash/skrifa 的支持面未核**：swash 的源优先级（w1b）证明能走“彩色轮廓”分支，
   但**v0/v1、Paint 图哪些格式**被覆盖未逐行读 swash/skrifa（不属本批预算），`W7D-034`~`W7D-036` 的
   “现状已覆盖”是**未验证推断**。
3. **未取 Apple 的 `sbix` 之外彩色规范细节**：`dupe` 数据的精确语义（同 strike 内复用）未逐字读取
   （只确认 graphicType 自述机制，`W7D-040`）。
4. **`avar` 的段插值端点约定（OT vs Apple）未做行为级对照**：两侧文档措辞同构（`W7D-024`/`W7D-031`），
   但没有用真实字体复算两条路径的数值一致性。
5. **fontconfig 的完整评分模型未读全**：只取 3 处源码锚（`W7D-082`~`W7D-084`），
   `FcSortCompare`/语言 2000 阈值（第 1269 行的分支条件）等**未逐条展开**；本批不宣称“复现了 fontconfig 排序”。
6. **DirectWrite `MapCharacters` 的完整算法（扫描策略/超时/区间续传）未读**：只取其接口契约（`W7D-059`）。
7. **未验证“FreeType hinting 改宽度”的量化幅度**：`W7D-069` 只说“slightly modifies dimensions”，
   量级未给；我们的对表若要做 hint on/off 两档，需要实测（本批禁跑上游代码）。
8. **失败来源未补**：Google Fonts 知识库两页、Chrome 的 COLRv1 博文（本机超时）缺席；
   COLRv1 的“权威长文”证据链目前靠 MS 规范 + colorfonts.wtf + Google color-fonts 仓三者支撑。
9. **未覆盖的表**：`bsln/bsln`、`JSTF`、`MATH`、`BASE`、`VORG`、AAT `morx/kerx` 状态机、
   `EBDT/EBLC`（旧位图表）、`DSIG`、TTC 头细节、`cvar` ——均在候选清单外，未入账；若后续做
   “元数据/站姿/竖排”主题需另起一批。

## 6. 对青简文本栈的 10 条可吸收（末节）

1. **自解析表的位移纪律**（`W7D-001`/`W7D-002`）：trak（已实现）与未来任何自解析表统一按
   “tag→TableRecord→record.length”取段，禁用“相邻偏移差”算长度；加一条“长度模 4 的 padding 不读入”的断言。
2. **emoji 文本/彩色切换的判据链**（`W7D-003`/`W7D-004`/`W7D-048`/`W7D-049`/`W7D-062`）：把 VS 检测（含 VS15/VS16）
   写进候选窗文本预处理契约；字形查询对“VS 修饰码点”单开 variation-glyph 路径（nominal 接口不得吞 VS）。
   验收金丝雀：同一 emoji 码点 + VS15/VS16/无 VS 三态，断言取到文本字形/彩色字形/默认彩色。
3. **选 strike/位图面的双键判据**（`W7D-039`/`W7D-065`）：位图 strike 与设备度量都按**设备像素**（ppem×ppi / 设备像素宽）
   而非 pt；对表工具在多密度机（1x/2x）各跑一遍，防“ppem 相同但位图不同”的漏配。
4. **opsz 的规范基线文档化**（`W7D-020`/`W7D-023`/`W7D-024`/`W7D-033`）：在文本栈文档写明
   “默认实例=无变体表路径”“归一化=分段线性+avar 二次映射”“形状连续插值”——opsz 补丁的任何行为变更都对照这三条。
5. **宽度/行高读数的来源分岔**（`W7D-027`~`W7D-029`/`W7D-078`）：TT 字体（gvar phantom/HVAR）、
   CFF2 字体（必须 HVAR）、全局度量（MVAR）、位置调整（GPOS VariationIndex）四类来源写进“度量口径”；
   宽度对表注明字体格式与所读表，防把来源差异记成 opsz 差异。
6. **回退链的两层语义分离**（`W7D-052`~`W7D-054`/`W7D-083`）：cosmic-text 的四级瀑布（硬过滤）继续做**主链**；
   若要“按覆盖递进的候选列表”（用于诊断/兜底展示），另实现 fontconfig 式 trim（新增覆盖才入选），
   两套结果分别命名、不互推。
7. **字体族查询按多名字集合**（`W7D-055`/`W7D-021`）：族名匹配实现为“全部本地化名（含 name 表多语言记录）的集合命中”，
   与 fvar 轴名的 name-ID 读取共用一套 name 表读取器；验收：中文系统上按英文名/本地化名各查一次，结果一致。
8. **合成粗体单独成档**（`W7D-057`）：宽度/观感对表分“真粗体/合成粗体（swash embolden）”两档记录；
   禁止把合成粗体的宽度差并入字体差异。
9. **渲染快照指纹纳入字体侧开关**（`W7D-063`/`W7D-064`/`W7D-068`）：快照元数据记录 gasp 版本/标志、
   hinting 开光（w1b 的 DISABLE_HINTING）、FreeType 引擎档；与 w1f 的“版本号即渲染行为”条目合并成一条验收规约。
10. **彩色字体测试语料与格式分派**（`W7D-045`~`W7D-047`）：用 Google color-fonts 仓做 COLRv1/OT-SVG 对照金丝雀，
    再用本机 macOS（sbix）/Windows（COLR/CBDT）实际字体补面；“彩色可用”验收必须写清**格式 × 平台**，不写“支持 emoji”。
