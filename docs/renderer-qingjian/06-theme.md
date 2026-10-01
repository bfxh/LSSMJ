# 06 · 主题：字体规格、调色板、单位与调参

## 6.1 `Theme` 全字段（`theme/mod.rs:12`）

"所有可视参数都在这里，单位是点；将来从 TOML 读"（`theme/mod.rs:1`）。
视觉层级是产品决定（`theme/mod.rs:3`）：**候选词最深 → 译文稍浅 → 词性最浅 → 序号弱化**。

| 字段 | 类型 | 缺省（`with_palette`，`:56`） | 用途 |
| --- | --- | --- | --- |
| `text_font` | `FontSpec` | 16.0 pt / 行高 19.0 | 候选词 |
| `annotation_font` | `FontSpec` | 12.0 / 15.0 | 译文与词性；**顶行拼音也用这一档**（`renderer/top_line.rs:43`） |
| `index_font` | `FontSpec` | 11.0 / 14.0 | 序号；页码也用（`renderer/mod.rs:101`） |
| `colors` | `Palette` | 见 6.3 | 八个色位 |
| `padding` | f32 | 8.0 | 窗口内边距（状态条同用） |
| `row_padding` | f32 | 4.0 | 行内上下留白（行高 = 文本高 + 2×） |
| `column_gap` | f32 | 8.0 | 序号↔词、词↔译文、候选项之间的间距 |
| `corner_radius` | f32 | 8.0 | 窗口圆角；高亮条取它的一半（`renderer/mod.rs:306`） |
| `max_rows` | usize | 9 | 最多显示几行——**渲染 crate 里只定义不消费**（`rg max_rows crates/qingjian-render` 仅 `theme/mod.rs:38`、`:67` 两处）；读它的是壳（macOS：`apps/macos/src/candidates/window.rs:133` 取每页行数上限，`apps/macos/src/host/presenting/mod.rs:78` 参与页大小计算） |
| `text_gamma` | f32 | 浅色 0.85 / 深色 0.75 | 文字覆盖率 gamma（见 03.4）；"按真机截图并排调"（`:41`） |

行高的出处（`:58` 注释）：**"行高取 AppKit 系统字体在这几个字号下 `NSAttributedString.size()` 的高度"**——
即主题数值不是设计师拍脑袋，而是对 macOS 原生排版的对表结果；这也是 03 里 `trak`/`opsz`/gamma 三件套的同一套对齐工程的组成部分。

## 6.2 单位与换算（两条铁律）

1. **主题一律用点**；进入渲染器先乘 `scale` 变像素——`Metrics::px()`（`renderer/mod.rs:64`），
   `FontSpec::scaled()` 同时乘字号与行高（`theme/font_spec.rs:18`）。
2. **按点查表的量用点**：`trak` 字距与光学字号都按"换算前的点字号"（`text/style.rs:9` 的 `points` 字段注释），
   按像素量宽高。两个口径各有字段，别混。

壳给的 `scale`：macOS 固定 2.0（`apps/macos/src/candidates/bitmap/mod.rs:75` 初值；`set_frame` 每帧传）；
Windows `dpi.max(96)/96`（`apps/windows/server/src/ui/painter/mod.rs:124`）——**渲染器不碰系统 DPI API**。

## 6.3 调色板映射（浅 / 深，`theme/palette.rs:33`）

| 色位 | 浅色 | 深色 | 语义（`palette.rs:5`） |
| --- | --- | --- | --- |
| `text` | `gray(0,216)` | `gray(255,216)` | 候选词（≈ macOS label 0.847，`palette.rs:1` 实测值） |
| `gloss` | `gray(0,127)` | `gray(255,140)` | 译文（≈ secondaryLabel 0.498） |
| `pos` | `gray(0,66)` | `gray(255,63)` | 词性与分隔符（更浅一档） |
| `fresh` | `(255,141,40)` | `(255,146,48)` | 生词译文强调色（橙）；"看熟了就回到译文色" |
| `index` | `gray(0,66)` | `gray(255,63)` | 序号 |
| `cloud` | `(0,195,208)` | `(0,210,224)` | 云联想（云朵与文字）；状态条强调格也用（`renderer/status/mod.rs:116`） |
| `background` | 纯白 | `(30,30,30)` | 窗口底 |
| `highlight` | `rgba(176,206,125,127)` | `rgba(36,76,36,255)` | 高亮底色；**深色档 alpha 一硬一柔**（浅色半透明、深色全不透明） |

生词/云色的判定逻辑不在渲染器：`FRESH_UNTIL`（上屏时该译词出现轮次 < 3 才算生词）在 Core
（`docs/design/candidate-ui.md:54`）；壳只把结果翻译成 `Tone::Fresh`。

## 6.4 主题的边界（现状能力，别误读）

- **没有**渐变、图案、毛玻璃、图片装饰：`paint()` 永远单色（`canvas.rs:148`），
  `fill_path` 不带剪辑与着色器。想做"图片/动图/花边"，设计档给的方向是"多叠几层"
  （`docs/design/rendering.md:112`）——意思是先往 `Canvas` 加原语，形态本身不用换。
- **没有** TOML 加载：注释写着"将来从 TOML 读"（`theme/mod.rs:1`），当前是 `Theme::light()/dark()`
  两组 const 构造（`theme/mod.rs:47`）。用户可调的只有**字体字族名**（`[general] font`，见 07）。
- 主题字号若可调，会撞 03.6 的 opsz 单槽限制与 `trak` 逐字体解析——这是主题化路线上的第一块已知坑。

## 6.5 一致性锚（跨文档）

- 视觉层级的数值（16/12/11pt、8/4/8pt 间距、8pt 圆角）与 macOS 壳的 AppKit 实现对齐
  （`theme/mod.rs:3` 注释"数值对齐 macOS 壳的 AppKit 实现"），并在位图渲染器接管后由
  `examples/preview.rs --measure` 与真机并排截图继续核对（03.5 的对表结果）。
- 深浅色不是两套渲染路径，只是 `Palette + text_gamma` 两个参数组（`theme/mod.rs:45`）。
