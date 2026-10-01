# 03 · 文本管线：整形、字形栅格、gamma、字距、字体加载

> 一句话：`TextPainter`（`text/mod.rs:17`）持有 **FontSystem（回退链）+ SwashCache（字形位图缓存）
> + 一个复用的 Buffer + 两张惰性表（trak 字距 / gamma 覆盖率）**；渲染器其余部分只通过
> `measure` / `draw` / `trace_families` 三个方法碰文字。

## 3.1 整形（shape）

每次 `measure`/`draw` 都先 `shape()`（`text/mod.rs:156`）：

| 步骤 | 代码 | 说明 |
| --- | --- | --- |
| 属性 | `Attrs::new().family(UI_FAMILY).color(...)`（`:157`） | `UI_FAMILY = Family::SansSerif`（`fonts/mod.rs:129`）——实际指向 `db.set_sans_serif_family(ui_family)`（`fonts/mod.rs:86`），即平台界面字体 |
| 度量 | `Metrics::new(style.size, style.line_height)`（`:161`） | 字号/行高都是**像素** |
| 尺寸约束 | `set_size(None, None)`（`:162`） | 不换行——所有文本都是单行；折行/截断由渲染器自己管（见 04 的 `truncate`） |
| 整形等级 | `Shaping::Advanced`（`:163`） | 完整整形（复杂文种、连字、kerning） |
| 执行 | `shape_until_scroll(&mut self.font_system, false)`（`:164`） | 写出 layout runs |

Buffer 复用（`text/mod.rs:24`）：一个 `Buffer` 撑起所有文本，避免每段文字分配。

## 3.2 量宽（measure）

`measure()`（`:54`）遍历 `layout_runs()`：`width = max(run.line_w + 该行 tracking 总和)`；
高度直接取 `style.line_height`（`TextSize`，`text/size.rs:4`）。
**tracking 是渲染器自己加的**：`line_w` 不含 `trak` 字距（见 3.5）。

## 3.3 画字形（draw）

`draw()`（`:74`）逐 run、逐字形：

1. `glyph.physical((x + tracked, y), 1.0)`（`:91`）——`tracked` 是**前面所有字形累计的 `trak` 字距**，
   也就是说字距不是作用在自己身上，而是把后面所有字形右推（与 CoreText 的观感一致，`:89` 注释）。
2. `cache.get_image(&mut font_system, physical.cache_key)`（`:93`）——**SwashCache 的缓存键包含字体、
   字号与亚像素位移**（`text/mod.rs:21` 注释），同一字形在不同亚像素位置是不同缓存项。
3. 放置：`gx = physical.x + placement.left`；`gy = line_y.round() + physical.y - placement.top`（`:99`）。
   注意 `line_y.round()`——基线取整，字形纵向不被亚像素模糊。
4. 按内容类型分派（`:102`）：
   - `SwashContent::Mask`（普通字形，8 位覆盖率）→ 先过 **gamma 查找表**（`:104`）再
     `canvas.blend_mask(...)`（`canvas.rs:88`），用样式颜色乘覆盖率混合；
   - `SwashContent::Color`（彩色 emoji，RGBA）→ `canvas.blend_rgba(...)`（`:108`）；
   - `SwashContent::SubpixelMask` → 空实现，注释说明"没有申请亚像素格式，不会出现"（`:109`）。

删除线/下划线**不是** cosmic-text 的装饰功能，是渲染器拿基线手工画的 `fill_rect`（`:121`–`:132`）：
粗细 `(size/14).max(1)`，删除线在 `baseline − size×0.3`，下划线在 `baseline + size×0.14`。

## 3.4 覆盖率 gamma（笔画加深）

对齐 CoreText 的关键之一（设计档 `docs/design/rendering.md:96`）：文字抗锯齿覆盖率过一道 gamma，
"线性混合出来的字会偏细，深色背景上尤其明显"。实现（`text/mod.rs:186`）：

- 256 项查找表：`out = (i/255)^gamma × 255`（`:190`），**按 gamma 值缓存**（键 = `f32::to_bits()`）。
- 主题给值：浅色 `0.85`、深色 `0.75`（`theme/mod.rs:47`/`:52`），调参依据写在字段注释里：
  "按真机截图并排调"（`theme/mod.rs:41`）。
- 依据外证（设计档自注）：muri #71 与 skip.house 都记录过 macOS 字体平滑按极性调（`docs/design/rendering.md:106`–`:107`）。

## 3.5 `trak` 字距表（自解析 AAT）

Apple 系统字体按字号给每个字形加减字距（SF：11pt +12、16pt −40 字体单位），CoreText 自动应用；
cosmic-text 不读，所以渲染器自己解析（`fonts/trak.rs:1`）。

| 点 | 实现 |
| --- | --- |
| 解析 | `Trak::parse(face_data, face_index)`（`:19`）：从 sfnt 目录找 `head`（取 `units_per_em`，`:20`）与 `trak`；TTC 先按面下标取目录（`table()`，`:66`） |
| 取表 | 只取**水平方向、track 0**（`:33` 找 track == 0.0）；采样字号表（Fixed）+ 每字号 i16 值 |
| 插值 | `tracking_em(size)`（`:46`）：采样点间线性插值、范围外取端点；**坏表防护**——"采样点该严格升序；相邻相等的坏表别除出 NaN"（`:53`–`:55`） |
| 应用 | `tracking_px()`（`text/mod.rs:169`）：按**字形所用字体**各查各的（`HashMap<ID, Option<Trak>>` 惰性缓存，`:28`）；`tracking_em(points) × size(像素)` |
| 点/像素 | 字距按**点**（`style.points`）查表（`text/style.rs:9` 注释），乘的是像素字号 |
| 测试 | 单测锁插值/边界（`:102`）；macOS-only 测试直接读 `/System/Library/Fonts/SFNS.ttf` 验证 16pt = −40（`:117`） |

对齐结果（设计档 `docs/design/rendering.md:95`）：补完后 "hello / ni'hao / 1/6 / phr. you change"
三个字号的宽度与 `NSAttributedString.size()` **到小数点后两位相等**。

## 3.6 光学字号（opsz）——上游 fork 的补丁

SF 是变量字体；CoreText 在 20pt 以下用 `opsz = 17`（Text 视觉尺寸），cosmic-text 只设 `wght`，
落在缺省 28（Display），小字号英文窄 16–24%（`docs/design/rendering.md:91`）。

- 渲染器侧：`Renderer::new` 固定设 `OPTICAL_SIZE = 17.0`（`renderer/mod.rs:47`、`:128`）；
  `TextPainter::set_optical_size` 转发给 FontSystem（`text/mod.rs:49`）。
- 上游侧：补丁在 **qingjian-team/cosmic-text 的 `qingjian-opsz` 分支**（基于 0.19.0，一个提交），
  workspace `[patch.crates-io]` 钉 rev（设计档 `docs/design/rendering.md:92`）。
- **已知局限**（代码注释自述）："整个画笔一个值（cosmic-text 的字体实例缓存没按它分键），
  候选窗几种字号都在 20 pt 以下，落到同一档"（`text/mod.rs:48`）——主题字号可调时要改成按字号分键。

## 3.7 字体加载（FontLibrary）——不扫系统目录

设计动机（`fonts/mod.rs:1`）："fontdb 全扫几百毫秒、几十 MB"，改成按平台清单只加载
界面字体、中文、日文、emoji 几个文件；mmap 加载，"只解析名字表与 cmap，Apple Color Emoji 那种
190 MB 的文件也只在用到字形时才读页"（`fonts/mod.rs:3`）。

装配顺序（`build()`，`fonts/mod.rs:56`）：

1. （可选）用户字体：`with_ui_font(locale, &UiFont{family, files})`——加载文件数 > 0 **且**
   名字能对上才生效，否则 warn 退回系统字体（`:58`–`:76`）；文件由壳查（macOS 用 CoreText 按字族名
   查文件，Windows 用 DirectWrite，见 3.8）。
2. 界面字体：`load_first(platform::ui_fonts())`（`:77`）——依次尝试，取第一个成功文件的第一张面；
   失败则 `RenderError::NoUiFont`。
3. `db.set_sans_serif_family(ui_family)`（`:86`）——`Family::SansSerif` 从此指向它。
4. `script_fonts(locale)` + `emoji_fonts()` 逐个加载，缺文件只记 debug（`:87`–`:94`）。

**中日同形字按 locale 回退**（`fonts/mod.rs:4`）：`zh-CN → PingFang SC`，`ja → Hiragino Sans`
（cosmic-text 的平台回退表）。真机验收：`zh-CN` 下拉丁/汉字全落 PingFang SC、"骨直曜"没被画成日式字形；
假名原先落 PingFang HK，原因是 **ヒラギノ角ゴシック W3（字重 300）被 cosmic-text 的字重匹配筛掉，
换 W4 后落 Hiragino Sans**（`docs/design/rendering.md:105`）。

### 三平台文件清单

| 平台 | 界面 | 中文/日文 | emoji |
| --- | --- | --- | --- |
| Windows（`fonts/windows.rs:12`–`:34`） | `segoeui.ttf`, `arial.ttf` | `msjh.ttc`(zh-TW/HK) 或 `msyh.ttc`(非 ja)；`YuGothR.ttc`、`yugothm.ttc` | `seguiemj.ttf`（COLRv0） |
| macOS（`fonts/macos.rs:8`–`:35`） | `SFNS.ttf`, `Helvetica.ttc` | PingFang（见下）+ `Hiragino Sans GB.ttc` 兜底 + `ヒラギノ角ゴシック W4.ttc` | `Apple Color Emoji.ttc`（sbix） |
| Linux（`fonts/linux.rs:5`–`:33`） | Noto Sans / DejaVu 常见路径 | NotoSansCJK 常见路径 | NotoColorEmoji 常见路径 |

macOS 的 PingFang 在系统资产目录里、目录名带哈希：`pingfang_paths()`（`fonts/macos.rs:38`）扫一层
`/System/Library/AssetsV2/com_apple_MobileAsset_Font*` 找 `AssetData/PingFang.ttc`，找不到退回
FontServices 里的 `PingFangUI.ttc`（`:60`）。平台清单"每个角色给几个候选"，因为
"系统私有位置，版本升级可能挪"（`fonts/macos.rs:1`）。

## 3.8 Windows 字体登记（DirectWrite，`fonts/directwrite.rs`）

给"用户指定字族"和设置页用：

- `family_files(family)`（`:26`）：`FindFamilyName → GetFontFamily → 逐面 GetFiles → file_path`，
  去重返回该字族所有面的**文件路径**（喂给 `UiFont.files`）。
- `file_path`（`:73`）：`GetReferenceKey` + `IDWriteLocalFontFileLoader` 取路径；
  **网络/内存字体（不是本地加载器）返回 `None`**（`:72` 注释）。
- `families()`（`:89`）：列系统全部字族名，**优先英文名**（`FindLocaleName("en-us")`，`:119`），
  按小写排序——设置页的字体下拉框就是它。

## 3.9 回退核对工具

`trace_families()`（`text/mod.rs:137`）：逐字形查 `db().face(font_id).families.first()`，
相邻相同的合并输出（如 `青简 hello 🙂 日本語 骨直曜` 各字形落到哪家字体）。
这是 spike 四条验收的**仪器**（`examples/preview.rs:138`–`:147`），不是日常路径。

## 3.10 已知差异（每个字形 ≤ 1pt，并排看不出；上游留档）

| 项 | 现状 | 原生 |
| --- | --- | --- |
| `·`（U+00B7） | SF 自己的（3.56pt） | `.CJKSymbolsFallbackSC` 宽点（5.76pt），一行译文差 2pt |
| 汉字 | 公开 PingFang SC（1 em） | 私有 `.PingFang UI Text SC`（advance 0.993 em） |
| 假名 | PingFang HK | `.CJKSymbolsFallbackSC` |
| 云朵 | 矢量描边近似 SF Symbol `cloud` | 比原生略粗 |

（`docs/design/rendering.md:98`–`:102`，逐条在源码侧对应 `cloud.rs` 的形状常量与 `fonts/macos.rs` 的清单。）
