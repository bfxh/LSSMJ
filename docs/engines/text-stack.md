# 文本栈（cosmic-text / swash / rustybuzz / fontdb / taffy / parley+fontique / glyphon / harfbuzz）

> 分析日期 **2026-10-01**。所有锚点 = 本地浅克隆的绝对路径 + 行号；克隆目录 `D:/KF/LSSMJ/scratch/src/<slug>`。
> **上游 commit**（`git rev-parse HEAD`，2026-10-01 抓取）：
>
> | slug | 仓库 | commit | 备注 |
> | --- | --- | --- | --- |
> | `cosmic-text-qj` | qingjian-team/cosmic-text（青简 fork，**main**） | `daae9c75d52322f8fb3af6168d76561540914e1f` | 2026-08-08；与 pop-os main 源码全等，仅 Cargo.toml 一行不同 |
> | `cosmic-text-opsz-snapshot` | qingjian-team/cosmic-text **qingjian-opsz** 分支 | `9cf0d65a5e4db381e34907103326673386096be3` | 2026-09-13；**青简实际编译用的那个 rev**（qingjian/Cargo.toml:99 钉死）。仅拉 3 个被改文件（raw 快照） |
> | `cosmic-text-up` | **pop-os/cosmic-text**（真上游） | `f1a3461f3e8df67d2bbcfafb8bc61dc7aee5b2a6` | 2026-09-30；v0.19.0 |
> | `cosmic-text-grovesNL` | grovesNL/cosmic-text | `3c573df26152d16f1aacd9fdf478a46445e6c37a` | **是 fork、不是上游**：`gh api` 报 `fork:true, parent:pop-os/cosmic-text`、`pushed_at 2024-01-15`、stars 0、version 0.1.0 |
> | `swash` | dfrg/swash | `7773843df0d63cd468db61a29c152b5e7a99d4ab` | v0.2.10（cosmic-text 依赖 `swash 0.2.6`，`^` 允许 0.2.10） |
> | `rustybuzz` | RazrFalcon/rustybuzz（已转 harfbuzz/rustybuzz） | `9faca967408677f17bc15366dbdc3d2b683d1489` | v0.20.1；README 首行自述 **unmaintained/archived** |
> | `fontdb` | RazrFalcon/fontdb | `ecb707e6096b59146b750f66a32c125a43515192` | v0.24.0（fork 钉 0.23） |
> | `taffy` | DioxusLabs/taffy | `fb461a7826e49f488f31220744bf12227ffb580e` | v0.14.0；**青简不依赖**，同族对照 |
> | `parley` | linebender/parley（**folio 已并入此仓**，见 §0.3） | `5794c7461bdeea70366cd2489e42c0864c97f7a8` | v0.11.0 workspace |
> | `glyphon` | grovesNL/glyphon | `49dc8f7bafa8091f4d71521fd62ee6f647b556f5` | v0.12.0；GPU 文本 |
> | （doc） | harfbuzz/harfbuzz | `main`（README 于 2026-10-01 curl） | 只做 doc 深度 |

## TL;DR（每条带锚）

1. **青简的真实依赖不是"上游 0.19.0"，而是 fork 的一个未合并分支**：qingjian 工作区把 cosmic-text 钉在 `9cf0d65`（分支 `qingjian-opsz`），该分支相对 fork main 只改了 3 个文件（font/mod.rs、font/system.rs、swash.rs），加的是 `FontSystem::set_optical_size` 光学字号；`qingjian-render/src/text/mod.rs:49` 已在调它。
2. **fork main（`daae9c7`）与 pop-os 上游 main 源码逐字节相同**，唯一差异是 `fontdb` 版本从 0.24 钉回 **0.23**（`cosmic-text-qj/Cargo.toml:16`）——即"fork"当前只是一条版本钉，不是代码分叉。
3. **整形引擎已经换代**：0.19.0 依赖 `harfrust 0.5.0`（HarfBuzz 的 Rust 移植），**不再是 rustybuzz**（`cosmic-text-qj/Cargo.toml:17`）；rustybuzz 上游 README 首行自己写了"unmaintained, and archived … 建议全部用户切到 HarfRust"（`rustybuzz/README.md:1`）。
4. **断行 = UAX#14 直接调库**：`unicode_linebreak::linebreaks(span)` 一行搞定（`cosmic-text-qj/src/shape.rs:994`），但外围补了一层"连字保护"启发式：标点对（如 `|>`、`!=`）先试整形，若字形数 < 字符数或字形 id 偏离 cmap，就**放弃这次断点**（`shape.rs:1032`）。
5. **栅格化模式：默认走覆盖率 Alpha 遮罩，不做亚像素 LCD**：`Render::new(...).format(Format::Alpha)`（`cosmic-text-qj/src/swash.rs:67`），`Content::SubpixelMask` 分支只 `log::warn!("TODO: SubpixelMask")`（`swash.rs:242`）——**但 swash 底座本身有 LCD 提示**：`HintingMode::Smooth { lcd_subpixel: Some(LcdLayout::Horizontal), preserve_linear_metrics: true }`（`swash/src/scale/hinting_cache.rs:27`）。
6. **亚像素定位是 4 档量化**：`SubpixelBin{Zero,One,Two,Three}` = 0/0.25/0.5/0.75，进 `CacheKey`（`cosmic-text-qj/src/glyph_cache.rs:65`）⇒ 单字形缓存键 = 字号×字重×x档×y档×flags，最多 16 个位置变体。
7. **字体回退是四级瀑布**：default families → 脚本专属表 → common 表 → **"其余所有字体（扣掉 forbidden 黑名单）"**（`cosmic-text-qj/src/font/fallback/mod.rs:469`），最后一级带 `//TODO: do not evaluate fonts more than once!`（同文件:467-468）——大字体库上有可测的二次开销面。
8. **taffy 的增量 = 脏标 + 每节点 9 槽缓存**：`mark_dirty` 沿祖先链上行、遇 `ClearState::AlreadyEmpty` 立即停（`taffy/src/tree/taffy_tree.rs:942`），缓存 `CACHE_SIZE = 9`（`taffy/src/tree/cache.rs:11`）；公开读数（M1 Pro + criterion）在 `taffy/README.md:113-123`。
9. **glyphon 的图集策略 = 256 起步、×2 增长 + 代际 LRU**：`INITIAL_SIZE = 256`（`glyphon/src/text_atlas.rs:33`）、`GROWTH_FACTOR = 2`（:128）、"本帧用过就不可淘汰"用 `generation` 单调计数判断（:89/:99）；每帧 bump（`text_atlas.rs:223`）。
10. **HarfBuzz 本体不做 hinting，而且是故意不做**："Notable missing feature: font hinting (including autohinting) is not implemented. For hinted rasterization, use FreeType or Skrifa."（harfbuzz README, 2026-10-01 抓取）；它自己往 GPU 走：`libharfbuzz-gpu` 用 **Slug 算法**编码轮廓并给 GLSL/WGSL/MSL/HLSL 源码。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 4 档亚像素分箱（0/0.25/0.5/0.75）当缓存键 | `cosmic-text-qj/src/glyph_cache.rs:65` | **吸收**：CPU 光栅下这是"视觉几乎无损、缓存放大 16×"的标准折中；青简已经在用（`qingjian-render/src/text/mod.rs:8`）。 |
| 覆盖率 Alpha 遮罩（不引 LCD 子像素） | `cosmic-text-qj/src/swash.rs:67` | **有界吸收**：候选窗在浅色/深色底上，Alpha 遮罩无颜色污染，稳；若将来要 12–16pt 正文级清晰度再谈 LCD（见下行）。 |
| swash 底座已有 `LcdLayout::Horizontal` 提示模式 | `swash/src/scale/hinting_cache.rs:28` | **有界吸收**：能力在，但 cosmic-text 的消费路径没接（:242 TODO）⇒ 要亚像素得自己写 `Render` 调用，不能只靠高层 API。 |
| `set_optical_size`（opsz 轴）| `cosmic-text-opsz-snapshot/src/font/system.rs`（`set_optical_size`） | **有界吸收**：青简已在用；代价是**改一次清空整个 font_cache**（该函数体内 `self.font_cache.clear()`），只能当"全局一次"用，不能按候选字号分档。 |
| 连字保护式断行（标点对探测） | `cosmic-text-qj/src/shape.rs:1032` | **吸收**：这对"输入法里显示代码/符号串"是刚需；注意它是启发式（只看 ASCII 标点对），中文标点不覆盖。 |
| 断行直接调 `unicode-linebreak` | `cosmic-text-qj/src/shape.rs:994` | **有界吸收**：UAX#14 全表但**不带 CJK 标点挤压/避头尾**的排版增强；候选窗单行展示时可无视，长候选截断要自己判。 |
| `Ellipsize::Middle` + `EllipsizeHeightLimit` | `cosmic-text-qj/src/layout.rs:173`、:186 | **吸收**：候选词过长时"掐中间"是现成能力（`layout_line` 已实现 Middle 分支）。 |
| `Wrap::WordOrGlyph` | `cosmic-text-qj/src/layout.rs:128` | **吸收**：长词不溢出、还能折行，候选窗/预览框直接用。 |
| `ShapeRunCache` 的"代际 + keep_ages"淘汰 | `cosmic-text-qj/src/shape_run_cache.rs:37` | **吸收**：比 LRU 实现简单一个量级（`retain`+age 自增），适合"每帧只重用最近 N 代的形状结果"。 |
| 四级回退链的"最后兜底 = 全部字体扣黑名单" | `cosmic-text-qj/src/font/fallback/mod.rs:469` | **有界吸收**：兜底强但慢，且带 TODO（:467）；候选窗字形集小，可接受。 |
| HarfBuzz/rustybuzz 的复杂文种分派表 | `rustybuzz/src/hb/ot_shaper.rs:128` | **不吸收（自研层）**：阿拉伯/天城文连着 6 个机器生成状态机，自绘渲染器不该重写；用 harfrust 即可。 |
| rustybuzz 作为整形依赖 | `rustybuzz/README.md:1` | **不吸收**：上游 archived，且自报比 harfbuzz 慢 1.5–2×（:43）。选 harfrust。 |
| taffy 的 flexbox/grid 全量实现 | `taffy/src/compute/flexbox.rs:1` | **有界吸收（思想）**：候选窗是"单行/单列 + 定宽"，不需要 flexbox 状态机；但 **9 槽 space-keyed 缓存 + 上行脏标提前停** 值得照搬到别的布局层。 |
| glyphon 的 etagere 图集 + 代际 LRU | `glyphon/src/text_atlas.rs:33`、:89 | **不吸收（当前）**：候选窗是 CPU 出位图再贴图，没有 GPU 图集；若将来 GPU 化，这是最短路径。 |
| fontique 的 `FallbackMap`（script × locale） | `parley/fontique/src/fallback.rs:15` | **有界吸收**：与 cosmic-text 的 `FallbackKey` 同构；青简已用 cosmic-text 的平台回退表（`qingjian-render/src/fonts/mod.rs:4` 注释），两套别混用。 |
| parley 用 ICU4X + WPT 做断行验收 | `parley/parley/src/tests/test_analysis.rs:279` | **吸收（方法）**：把"和 ICU 字典断行逐点对齐"写成断言——这是可复算的断行判据模板。 |

## 0. fork 差异（单列一节）

### 0.1 青简 fork **main**（`daae9c7`）vs pop-os 上游 main（`f1a3461`）

- 全部非 `.git` 文件 `diff -rq` 结果：仅 6 类文件不同，**`src/` 下 0 处差异**。
  - `Cargo.toml`：**1 行**——`fontdb` `0.24` → `0.23`（`cosmic-text-qj/Cargo.toml:16`）。理由推断：与青简工作区其余部件（tiny-skia 0.12 + 旧 fontdb 生态）对齐；**不是能力差异**，仅版本钉。
  - `examples/{editor,editor-test,multiview,rich-text,terminal}/Cargo.toml`、`deny.toml`、`.github/PULL_REQUEST_TEMPLATE.md`：fork 侧为旧版模板/策略副本；上游多一个 `.github/workflows/require-checklist.yml`。
- **结论**：`cosmic-text-qj` 这棵树**不能当作"青简定制版本"来分析**——它是"上游 + 版本钉"。真正定制在下一节。

### 0.2 真正被编译的 rev：分支 `qingjian-opsz`（`9cf0d65`，2026-09-13）

`qingjian/Cargo.toml:99` 直接钉 `rev = "9cf0d65a5e4db381e34907103326673386096be3"`。该分支相对 fork main：`ahead 1 / behind 9`（`gh api .../compare/main...qingjian-opsz`），改动 3 文件：

| 文件 | 改动 | 锚（raw 快照 `cosmic-text-opsz-snapshot/`） |
| --- | --- | --- |
| `src/font/mod.rs` | `Font::new(db, id, weight)` → 增参 `optical_size: Option<f32>`；轴设置从"只写 wght"改成 `settings = vec![(wght, …)]`，有 opsz 时再 `push((Tag::new(b"opsz"), opsz))` | `cosmic-text-opsz-snapshot/src/font/mod.rs`（`let mut settings = vec!` 一带） |
| `src/font/system.rs` | 新增字段 `optical_size: Option<f32>` + `optical_size()` / `set_optical_size()`；**改变即 `self.font_cache.clear()`**；`get_font` 把 opsz 传给 `Font::new` | `cosmic-text-opsz-snapshot/src/font/system.rs` |
| `src/swash.rs` | 原先只查 `wght` 变体，现改成 `variation_settings(&font, weight, font_system.optical_size())` 后统一 `normalized_coords(settings)` | `cosmic-text-opsz-snapshot/src/swash.rs` |

- 消费侧印证：`qingjian-render/src/text/mod.rs:47` 注释说明"光学字号（点）：SF 这类带 opsz 轴的字体…CoreText 对系统字体自动做，这里要显式给"，:49 `set_optical_size(points)`；:48 同时承认"现在是整个画笔一个值（cosmic-text 的字体实例缓存没按它分键）"——与 `font_cache.clear()` 的实现互相印证（**没分键 ⇒ 只能全局换档**）。
- **风险留档**：该分支 `behind 9`（未 rebase 到最新 main），且改的是 `font_cache` 清空语义——清空会连带丢掉所有字体的 `SharedFaceData` 缓存；若未来按字号频繁切档，这是已知热点。

### 0.3 另外两处"名字对不上"的清单偏差

- **"grovesNL/cosmic-text（上游）"不成立**：`gh api repos/grovesNL/cosmic-text` → `fork:true`、`parent/source: pop-os/cosmic-text`、`pushed_at 2024-01-15T14:19:57Z`、`stargazers_count:0`、内容 version 0.1.0（`cosmic-text-grovesNL/Cargo.toml:3`）。本报告的上游基准改用 **pop-os/cosmic-text**（2158 stars，`pushed_at 2026-09-30T16:07:42Z`）。
- **`linebender/folio` 已不存在**：`gh api repos/linebender/folio` → 404。其职能 = **`fontique`**（"Font enumeration and fallback."），现已作为 **parley 仓的工作区成员**发布（`parley/Cargo.toml:6` 成员列表含 `"fontique",`；`parley/fontique/Cargo.toml:4` description 行）。

## 1. 架构全景（模块地图）

### 1.1 cosmic-text 0.19（= 青简消费层）

自述分工（`cosmic-text-qj/src/lib.rs:7`）："Shaping utilizes harfrust, font discovery utilizes fontdb, and the rasterization is optional and utilizes swash."

| 文件/目录 | 职责 | 关键锚 |
| --- | --- | --- |
| `src/shape.rs`（3084 行） | 整形管线：run 切分 → 回退挑字体 → harfrust shape → 词/行/视觉行组装 + 断行 + 省略号 + 排版 | `shape_fallback` :143、`shape_run` :298、`ShapeSpan::build` :963、`layout` :1589、`layout_line` :2224 |
| `src/buffer.rs` | `Buffer`/`BufferLine`：脏标、只对可见窗口做 shape/layout | `shape_until_scroll` :582、`prune` 时 `reset_shaping()` :625 |
| `src/bidi_para.rs` | 段落切分；**纯 ASCII 快路径**绕过 `BidiInfo` | :16 文档、:51 回退分支 |
| `src/glyph_cache.rs` | `CacheKey`（font_id/glyph_id/size_bits/x_bin/y_bin/weight/flags）+ `SubpixelBin` | :19、:65 |
| `src/swash.rs` | swash 光栅化封装 + `SwashCache`（image_cache / outline_command_cache） | :67 格式、:132 cache 定义 |
| `src/shape_run_cache.rs` | 形状结果代际缓存（`age` + `trim(keep_ages)`） | :37 |
| `src/font/system.rs` | `FontSystem`：fontdb 库、locale、`get_font`、`get_font_matches` | :380、:437 |
| `src/font/fallback/{mod,unix,windows,macos,other}.rs` | 四级回退迭代器 | :185 `FontFallbackIter`、:469 兜底 |
| `src/render.rs` | `Renderer` trait（rectangle/glyph）+ 装饰线绘制 + `LegacyRenderer` | :11、:21、:126 |
| `src/edit/`、`cursor.rs`、`layout.rs`、`attrs.rs`、`cached.rs` | 编辑器动作、光标、`Wrap`/`Ellipsize`、属性、缓存包装 | `layout.rs:128/173/186` |

### 1.2 依赖分工（下游 → 上游）

```
qingjian-render ── cosmic-text 0.19(fork@9cf0d65)
                     ├─ harfrust 0.5.0   ← 整形（OpenType GSUB/GPOS + 复杂文种）
                     ├─ fontdb 0.23/0.24 ← 字体枚举/查询（ID → 文件）
                     ├─ unicode-bidi 0.3.18（hardcoded-data） ← 双向算法
                     ├─ unicode-linebreak 0.1.5 ← UAX#14 断点
                     ├─ skrifa 0.40.0    ← 读字体/度量/可变轴
                     └─ swash 0.2.6(→0.2.10) ─┬─ zeno 0.3.3（路径填充/栅格）
                                              └─ skrifa（HintingInstance）
对照物：taffy 0.14（布局）· parley 0.11 + fontique（ICU4X/HarfRust/Skrifa）· glyphon 0.12（wgpu 图集）
```

## 2. 关键机制

### 2.1 整形（含阿拉伯/复杂文种/双向）

- **harfrust 调用面**：`buffer.set_direction(...)` → `push_str` → `guess_segment_properties()` → `ShapePlan::new(shaper, direction, script, language, features)` → `shape_with_plan`（`cosmic-text-qj/src/shape.rs:160-225`）。
- **ShapePlan 缓存很小**：`const NUM_SHAPE_PLANS: usize = 6;`（:108），FIFO 淘汰（"least recently **added**"而非 LRU，:113-114 自述）。
- **字形缺失 → 回退**：`info.glyph_id == 0` 记为 missing cluster（:235-237），整个 run 用回退字体**重整一遍**，再按 cluster 区间把命中 glyph 插回（:351-400）；注释自承 `//TODO: improve performance!`（:350）。
- **复杂文种不在 cosmic-text 里**：分派全在整形引擎侧（rustybuzz 版见 `rustybuzz/src/hb/ot_shaper.rs:128`：ARABIC/SYRIAC→Arabic shaper，THAI/LAO→Thai，HANGUL、HEBREW，BENGALI…TELUGU→Indic，MYANMAR/KHMER→各自机器）。**harfrust 沿用同一套分派**（HarfBuzz 移植），所以"阿拉伯连写/天城文重排"对青简是**白拿的**，只要选对字体。
- **双向**：段落级 `unicode-bidi::BidiInfo`（`bidi_para.rs:51`），run 级用 `Level` 判定方向并拼 RTL 字形序（`shape.rs:268-290` 的 end 修正分支）。纯 ASCII 段落跳过 BidiInfo 分析（:19-24），候选窗里绝大多数是纯 ASCII ⇒ 这条快路径直接受益。

### 2.2 断行（UAX#14 实现位置）

- **实现位置 = 外部 crate `unicode-linebreak`**，调用点唯一：`cosmic-text-qj/src/shape.rs:994`。
- cosmic-text 只做两件外围事：① 连字保护（:995-1059）；② 把断点前的**尾部空白**拆成独立 word，并把每个尾随空白字符各自成词（:1062-1103）——这是为了 wrap 时正确悬挂空白。
- 已知缺口（上游 TODO）：`// TODO: Not all whitespace characters are linebreakable, e.g. 00A0 (No-break`（:1064）——**不换行空格未特殊处理**。
- `taffy/parley` 是另一条路线：parley 用 **ICU4X `icu_segmenter`**（`parley/parley/Cargo.toml:46`，`compiled_data`），并在测试里**拿 ICU 字典断行当期望值**（`parley/parley/src/tests/test_analysis.rs:279`）。

### 2.3 字体回退

- 四级（`cosmic-text-qj/src/font/fallback/mod.rs`）：① default（用户给的 family，含 `Family::Monospace` 特判 :307）→ ② 脚本表（`Fallbacks::scripts`，:305 起）→ ③ common 表（:453-465）→ ④ **其余全部字体**，逐个过滤 `forbidden_fallback()` 黑名单（:469-481）。
- 每条都会 `get_font_matches(&attrs)` 先算 `FontMatchKey`（`font/system.rs:437`，带 `font_weight_diff` / `variable_weight_match` 字段，:21-32），避免每次重新匹配字重。
- 失败可观测：`check_missing` 分三种告警（无任何回退 / 预设表没用上 / common 表没用上，:227-253）——**可直接接 tracing**。

### 2.4 栅格化（swash 的 hinting / 亚像素）

- **hinting 默认开**：`builder(...).size(...).hint(!flags.contains(DISABLE_HINTING))`（`cosmic-text-qj/src/swash.rs:37`；outline 路径同 :103）。
- **hint 由 skrifa 实现**（swash 0.2.x 已把 hinting 委托给 skrifa）：`HintingInstance::new(outlines, size, coords, HINTING_MODE)`（`swash/src/scale/hinting_cache.rs:23`），模式常量：`HintingMode::Smooth { lcd_subpixel: Some(LcdLayout::Horizontal), preserve_linear_metrics: true }`（:27-29）。
- **hint 实例缓存只有 8 个**：`const MAX_CACHED_HINT_INSTANCES: usize = 8;`（:12），注释解释"重算 hint 数据是中低成本，偶尔重来可以接受"（:9-11）。
- **源优先级**：`[ColorOutline(0), ColorBitmap(BestFit), Outline]`（`cosmic-text-qj/src/swash.rs:58-65`）⇒ 彩色字体优先走 COLR 轮廓，其次 CBDT/sbix 位图，最后普通轮廓——**emoji 与彩色字形白拿**。
- **合成样式**：伪斜体 = `Transform::skew(14°, 0)`（:70-77、outline 路径 :118）；伪粗体在 swash 侧 `Render::embolden(strength)`（`swash/src/scale/mod.rs:831`），底层是轮廓点集的等距外扩 `fn embolden(points, winding, x_strength, y_strength)`（`swash/src/scale/outline.rs:305`）。
- **输出内容类型**：`Content::{Mask, SubpixelMask, Color}`（`swash/src/scale/image.rs:11`）；cosmic-text 只处理 Mask/Color，SubpixelMask 未接（见 TL;DR 5）。
- **变量字体**：`normalized_coords` 用法 + 一个实测坑：`variations()` 会就地放大 `context.coords` 且**残留**，`normalized_coords()` 才清旧值（`cosmic-text-qj/src/swash.rs:254-256` 的回归测试注释）——上游用测试锁住了这个行为。

### 2.5 taffy：flexbox/grid 与增量

- 规范锚：`flexbox.rs:1` 注释指向 W3C `css-flexbox-1`；`grid/mod.rs:1-2` 自述"**partial** implementation of the CSS Grid Level 1 specification"。
- 增量：`mark_dirty(node)` 递归上行，`ClearState::AlreadyEmpty` 时**立刻停**（"they should be marked as dirty already"，`taffy_tree.rs:942-945`）；`dirty()` 判据 = `cache.is_empty()`（:963）。
- 缓存：每节点 `CACHE_SIZE = 9` 槽（`cache.rs:11`），键把 `Option<f32>` 打包进 `CacheKey`（两 f32 塞 u64，SignBit 复用存 `RequestedAxis`，`cache.rs:14-30`）。
- `detailed_layout_info` 暴露网格轨道计算结果（`taffy_tree.rs:930`）。

### 2.6 glyphon：GPU 文本与图集

- 管线：cosmic-text 形状/布局 → swash 光栅 → `etagere::BucketedAtlasAllocator` 打包 → 复用现成 render pass 采样（`glyphon/README.md:19-23`，"middleware pattern"）。
- 图集：起始 `INITIAL_SIZE = 256`（:33）且被 `max_texture_dimension_2d` 夹住（:37）；满则先按 **代际 LRU** 淘汰（本帧 `last_used == generation` 的一律不淘汰，:89/:99），仍不成就 `grow()`：`GROWTH_FACTOR = 2`（:128，"chosen to match the growth factor of `Vec`"，:126-127），**重传全部 glyph**（:149-163）。
- 自定义字形（如 `github.com/grovesNL/glyphon` 的图标）与文本共用一张图集，键含 `x_bin/y_bin`（`text_render.rs:170-176`）。

## 3. 性能手段与公开读数

> 纪律：下面每个数字都带锚 + 口径。**没有机器/版本/口径的"很快"一律不写**。

| 读数 | 值 | 口径（机器/版本/日期） | 锚 |
| --- | --- | --- | --- |
| rustybuzz vs harfbuzz 整形耗时 | "1.5-2x slower than harfbuzz" | 自报，无机器细节；rustybuzz 0.20.1 README，抓取 2026-10-01 | `rustybuzz/README.md:43` |
| rustybuzz 一致性 | 2221/2252 harfbuzz 整形测试通过；对应 harfbuzz v10.1.0 | 同上 | `rustybuzz/README.md:12`、:21 |
| taffy vs Yoga（huge nested, 100k 节点, depth 5） | Yoga 45.804 ms / Taffy 38.559 ms | **2021 MacBook Pro M1 Pro + criterion**；"measure layout computation only"；Yoga 经 `yoga-rs`；taffy commit `71027a8` | `taffy/README.md:105-106`、:116 |
| taffy vs Yoga（big trees wide, 100k, depth 1） | Yoga 135.78 ms / **Taffy 247.42 ms（taffy 输）** | 同上 | `taffy/README.md:119` |
| taffy vs Yoga（super deep, 1k 节点, depth 1000） | 555.32 µs / 472.85 µs | 同上 | `taffy/README.md:123` |
| 典型站点节点规模 | "between 3,000 and 10,000 nodes" | 作者经验估计（非测量） | `taffy/README.md:108` |

**机制性手段（无公开数字，只有实现事实）**：形状计划缓存 6 条（`shape.rs:108`）、hint 实例缓存 8 条（`hinting_cache.rs:12`）、taffy 每节点 9 槽（`cache.rs:11`）、glyphon 图集每帧代际淘汰（`text_atlas.rs:89`）、cosmic-text 只对可见窗口 shape（`buffer.rs:582`）、`ShapeRunCache` 代际淘汰（`shape_run_cache.rs:37`）。

## 4. 坑与反例

1. **"fork"可能只是版本钉**：青简 fork main 与上游源码全等，只有 1 行 Cargo 版本差（§0.1）。把 fork 当"定制实现"读会得出错误结论——**定制在未合并分支上**（§0.2）。
2. **打包快照 ≠ 编译版本**：任务给的克隆 SHA（`daae9c7`）与工作区实际钉的 rev（`9cf0d65`）**不是同一个提交**，两者相差 9 个上游提交 + 1 个定制提交。任何"青简用了什么"的结论必须锚到 `qingjian/Cargo.toml:99`。
3. **SubpixelMask 是死支**：`log::warn!("TODO: SubpixelMask")`（`cosmic-text-qj/src/swash.rs:242`）⇒ 想要 LCD 亚像素必须绕开 cosmic-text 的高层封装。
4. **光学字号切档会清空字体缓存**：`set_optical_size` 内 `self.font_cache.clear()`（opsz 快照 `src/font/system.rs`），且缓存键不含 opsz ⇒ 不能"每个字号档一个实例"；青简因此接受"整个画笔一个值"（`qingjian-render/src/text/mod.rs:48`）。
5. **NBSP 断行未处理**：上游自己留的 TODO（`shape.rs:1064`），CJK/西文混排里若用 `\u{00A0}` 占位会被当可断点。
6. **回退兜底会重复评估字体**：`//TODO: do not evaluate fonts more than once!`（`fallback/mod.rs:468`）+ 整个 run 重整（`shape.rs:350 //TODO: improve performance!`）⇒ 大量缺失字形时是 O(字体数 × run)。
7. **`variations()` 的协调泄漏**：会就地改 shared `ScaleContext.coords`，跨字体渲染会污染（`swash.rs:254-256`）；用 `normalized_coords()`。
8. **glyphon 图集增长要全量重传**（`text_atlas.rs:149`）⇒ 首次撞上限的那一帧有尖峰；且"本帧 glyph"永不淘汰（:89）——极端字体大小组合下会持续增长到 `max_texture_dimension_2d` 后**直接放弃**（:122-124）。
9. **taffy 并非全面快于 Yoga**：wide/100k 场景输 82%（`README.md:119`）——"taffy 比 yoga 快"是**分场景**结论，不能升格。
10. **hint 实例只有 8 个槽**：字号种类多时会抖动重算（`hinting_cache.rs:12`）——候选窗若引入多字号（主候选 + 拼音提示），值得实测。

## 5. 未验证项

- **未实测任何性能**：本任务禁跑 cargo/上游代码，§3 的数字全部是**转载自仓库 README**，其机器/口径已如实标注；青简本机的吞吐未测。
- **harfrust 自身未读源码**：本报告对"harfrust 沿用 HarfBuzz 复杂文种分派"的判断来自 rustybuzz 的同源分派表 + harfrust 是 HarfBuzz 移植的自述；**未读 harfrust 仓库**（不在任务清单）。
- **`behind 9` 的 9 个上游提交内容未逐条核**：只确认了数量与方向（`gh api compare`）。
- **folio → fontique 的"官方改名公告"未拿到**：证据链为"folio 仓库 404 + fontique 自述 Font enumeration and fallback + 现为 parley 工作区成员"，**缺一条官方迁移说明**。
- **glyphon 的 `ColorMode`/多图集（Mask+Color 分张）细节未读透**：只确认了 Kind 分型与纹理格式选择存在（`text_atlas.rs:108` `num_channels`）。
- **cosmic-text 的 `examples/` 与 benches 未读**：公开性能数字面板没找到，故 §3 无 cosmic-text 自身的读数。
- **taffy `benches/results-*.md` 未读**：README 表已足够，但仓内另有 2022-12-07 / 2023-02-08 两份结果文件未展开。
