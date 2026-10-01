# 04 · 帧模型与三种排布

## 4.1 `Frame`：一帧里有什么（`frame/mod.rs:11`）

`Frame` "只是展示形态，不含排序或查词"（`frame/mod.rs:1`）——引擎算完，渲染器只管画。

| 字段 | 类型 | 语义 | 关键约定 |
| --- | --- | --- | --- |
| `preedit` | `Option<Preedit>` | 顶部拼音行 | 配置为"只在行内显示"时为 `None`（`:13`） |
| `rows` | `Vec<Row>` | 候选行 | `Row{index, text, code, annotation, cloud}`（`frame/row.rs:5`） |
| `highlighted` | `Option<usize>` | 高亮行下标 | `None` 不高亮（`:20`）；矩阵在信息行展示被截断的高亮候选全文 |
| `columns` | `usize` | 矩阵每行几格 | 0 = 不展开（`:23`）；矩阵是"横排展开"，不是第三种 `Layout` |
| `column_ems` | `Vec<f32>` | **各列预留的候选字宽** | "壳按**整份**候选估的，滚动、移动高亮时不变，窗口才不跳"（`:25`）——见 4.4 |
| `footer` | `Option<String>` | 页码，如 `2/12` | 竖排右下 / 横排行尾 / 矩阵信息行右端 |
| `sentence` | `Option<String>` | 右侧整句补全（云联想） | 与 `status` 互斥，见下 |
| `status` | `Option<String>` | 临时状态（如"已删除「开放」"） | |

两个辅助方法把"顶行右侧画什么"收敛成一条规则（`:49`）：

```rust
// frame/mod.rs:49
pub fn trailing(&self) -> Option<(&str, bool)> {
    self.status.as_deref().map(|s| (s, false))          // 状态优先，不带云朵
        .or_else(|| self.sentence.as_deref().map(|s| (s, true)))  // 其次整句补全，带云朵
}
```
`has_top_line()`（`:44`）= preedit 或 trailing 任一存在；`is_empty()`（`:39`）= 三者皆无。

`Row` 上两个语义要点：
- `code`（辅码，如 `[kf]`）：**紧跟在候选词后面**，"码是词本身的属性，不进右侧的 annotation 列"（`frame/row.rs:13`）；
  渲染时用 `Tone::Code` 色（`renderer/mod.rs:271`）。
- `cloud`：云端词——词前画小云朵、词用云色（`renderer/mod.rs:261`–`:268`）。
- `annotation: Vec<(String, Tone)>`：**按顺序绘制**的片段（`phr. ` / 译文 / ` · ` 可以混排成一段），
  每个片段自带深浅（`Tone::{Gloss, Fresh, Faint, Code}`，`frame/tone.rs`）。

## 4.2 常量表（`renderer/mod.rs:29`–`:50`，单位：点）

| 常量 | 值 | 用途 |
| --- | --- | --- |
| `CARET_WIDTH` | 1.5 | 自绘光标宽度 |
| `CLOUD_SIZE` / `CLOUD_GAP` | 13.0 / 4.0 | 云朵图标边长 / 与后文间距 |
| `SENTENCE_GAP` | 16.0 | preedit 与右侧整句补全的间距 |
| `INDEX_GAP` | 3.0 | 序号与候选词的间距（横排） |
| `HIGHLIGHT_INSET` | 5.0 | 横排/矩阵高亮底色在候选两侧多出的宽 |
| `OPTICAL_SIZE` | 17.0 | 光学字号（见 03） |
| `MIN_VERTICAL_WIDTH` | 200.0 | 竖排最小窗宽——"竖排时候选都很短（没有译词）窗口会窄得难看"（`renderer/mod.rs:206`） |

## 4.3 顶行（`renderer/top_line.rs`）

**量**（`:9`）：宽 = 各段宽之和 + `CARET_WIDTH`；有 trailing 时再加 `SENTENCE_GAP`（仅当 preedit 存在）
+ `cloud_width()`（若带云朵）+ 文本宽。高 = 注释行高 + `row_padding × 2`。

**画**（`:32`）：
1. 拼音段按样式映射（`:78`）：
   `Typed → gloss 色`、`Rest → pos 色`、`Struck → pos 色 + 删除线`、`AuxCode → pos 色 + 下划线`。
   删除线/下划线是渲染器手工画的矩形（见 03.3）。
2. **自绘光标**（`:87`）：x = 起点 + `measure(before_cursor)`（按 gloss 样式量，`Preedit::before_cursor`
   按**字符**截取，`frame/preedit/mod.rs:36`），再 `fill_rect(CARET_WIDTH, line_height, text 色)`。
   光标位置由引擎给的**字符下标**（`Preedit.cursor`）换算，渲染器不管输入法内部状态。
3. 右侧按 `trailing()`：云朵 + 云色文字（sentence）/ 纯 gloss 灰字（status）（`:53`–`:61`）。

## 4.4 三种排布

`Layout` 只有两值（`layout.rs:4`）；"矩阵"= `Horizontal` 且 `columns > 0`（`renderer/mod.rs:168`）。

### 竖排（`renderer/vertical.rs`，缺省）

- **量**（`:9`）：三列各自取最大宽——序号列（`:46`）、候选词列（含云朵与辅码宽，`:36`–`:40`）、
  annotation 列（整行片段宽之和，`:41`）；总宽 = 三列 + 两个 `column_gap`（无译文列时省一个）。
  行高 = 各行"文本高 + `row_padding×2`"的最大值（`:49`）。
  页码若存在：并入宽度参与取 max、高度加"页码高 + row_padding"（`:16`–`:20`）。
- **画**（`:54`）：高亮行先铺满宽底（`content_width − padding` 宽，圆角 `corner_radius/2`，`fill_highlight`）；
  序号与小字都下移 `small_offset`——"(text_height − annotation 行高) 让两者底部对齐"（`renderer/mod.rs:115`）；
  译文片段从 `annotation_x` 起**顺序**绘制（`:90`）；页码贴右下（`content_width − padding − 页码宽`，`:103`）。

### 横排（`renderer/horizontal.rs`）

- **量**（`:9`）：逐候选量 `index + INDEX_GAP + text(+cloud+code)`，项间加 `column_gap`，
  两侧加 `HIGHLIGHT_INSET`；页码并入宽度；**高亮候选的译文独占下面一行**
  （`highlighted_annotation_size`，`:34`：宽 = 片段之和，高 = 注释行高 + row_padding）。
- **画**（`:72`）：单行依次排开；高亮铺 `item_width + inset×2` 的底（`:93`）；页码贴行尾；
  最后单独画高亮候选译文行（`:131`）——"每个都带译文会把窗口拉得很宽"（`docs/design/candidate-ui.md:30`）。

### 矩阵（`renderer/matrix.rs`，`columns > 0`；macOS 已接、Windows 未接，开关缺省关）

设计目标一句话（`renderer/matrix.rs:3`）：**"整个窗口的宽度只由列宽决定……高亮怎么移、视口怎么滚，窗口都不跳。"**

- **列宽固定**：优先用帧给的 `column_ems × 字号(em) + CELL_SLACK(1.5pt)`（`:183`）。
  `column_ems` 由 Core 按**整份**候选估（汉字 1 字宽、拉丁 0.62、云朵另加，单格封顶 4 字宽——
  `estimated_ems`，`:227`；上限 `MAX_CELL_EMS = 4.0`，`:12`）。
  历史留档：第一版按视口实测列宽，**"滚一行窗口就跳一下，最宽能到 1100 pt"**，现在 9 列约 756pt
  （`docs/design/candidate-ui.md:27`–`:28`）。
- **估算快路径**（`:202`）：列宽已给且"按字数估着放得下"的格子**不实测**——
  "一屏五十多格，省掉大半次整形"。这是矩阵场景把"measure+draw 双整形"代价压下去的主要手段。
- **截断**（`:235`）：超宽候选从末尾去字加 `…`，逐次回退直到放得下（`truncate` 返回 `(显示文本, 是否截断)`）。
- **信息行**（`:117`）恒定一行：页码贴右（先占预算），左边依次是**被截断的高亮候选的完整文本**、
  它的译文；都用 `draw_clipped` 按预算截断——"信息行放不下的也截断，不撑开窗口"（`renderer/matrix.rs:3`）。
- 行高以「国」实测（`:191`）、序号列按「8」实测（`:182`）——都在目标字号下取"最宽/最高"的代表字。
- 图形行数 = `rows.len().div_ceil(columns)`（`:53`）——`columns` 总被 `max(1)` 保护（`:74`）。

> 视口/滚动/按键规则不在渲染器：Core `candidate::layout::Grid` 管"哪几格进帧"，
> 固定 6 行视口（`GRID_ROWS`）、翻页与跨行移动都在那边（`docs/design/candidate-ui.md:21`）；
> 渲染器只负责把帧里给的格子画好。两条绘制路径（本 crate 的 `renderer/matrix.rs` 与
> 系统绘制路径 `view/matrix.rs`）**同一套规则**（`docs/design/candidate-ui.md:28`）。

## 4.5 状态条（Windows 专属，`renderer/status/mod.rs`）

`[中 / 英][，。/ ,.][⚙]` 三格并排（模块注释，`renderer/status/mod.rs:1`）：
每格宽 = 内容宽 + 两侧 padding；格间 1pt 细线（pos 色）；文字/齿轮各自居中；
输出 `cell_edges`（各格右边界）供壳做**唯一的命中测试**（`:9`；测试锁死"边界递增且最后一个 = 内容宽"，
`:148`）。齿轮是矢量画的，原因见 `05-raster-canvas.md` 5.4。
