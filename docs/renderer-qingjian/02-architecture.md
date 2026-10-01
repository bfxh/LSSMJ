# 02 · 模块地图与渲染主流程

## 模块地图（逐文件职责，行数为 c08ae57c 实测）

依赖方向是单向无环：`frame / theme / color / error`（数据）← `text / fonts / canvas / shadow / cloud / gear`（能力）
← `renderer/*`（组装）← `lib.rs`（导出）。所有 `mod` 声明与再导出在 `lib.rs:9`。

| 模块 | 文件 | 行数 | 职责（一句话） |
| --- | --- | --- | --- |
| 外壳 | `lib.rs` | 35 | 文档（crate 自述）、mod、pub use；把 `tiny_skia::Pixmap` 再导出给壳直接用（`lib.rs:35`） |
| 数据 | `frame/mod.rs` | 55 | `Frame`：一帧的全部内容；`trailing()` 决定顶行右侧画什么（见下） |
| 数据 | `frame/row.rs` | 34 | `Row`：序号、候选词、辅码 `code`、右侧 annotation（`Vec<(String, Tone)>`）、`cloud` 云联想标记 |
| 数据 | `frame/tone.rs` | 17 | `Tone`：`Gloss` 译文 / `Fresh` 生词 / `Faint` 词性与分隔符 / `Code` 辅码 |
| 数据 | `frame/preedit/{mod,segment,style}.rs` | 39+13+16 | 拼音行：段序列 + 光标字符位置；段样式 `Typed/Rest/Struck/AuxCode` |
| 数据 | `layout.rs` | 12 | `Layout::{Vertical, Horizontal}`——渲染器不依赖配置 crate，壳换算（`layout.rs:1`） |
| 数据 | `theme/mod.rs` + `theme/{font_spec,palette}.rs` | 71+24+58 | `Theme`（全部可视参数，单位点）、`FontSpec`（字号/行高）、`Palette`（8 个色位） |
| 数据 | `color.rs` | 73 | `Color`（sRGB8+alpha）+ 定点乘法 `mul_u8` + 预乘 `premultiply`；到 skia/cosmic 的换算集中地 |
| 数据 | `error.rs` | 15 | 仅两个错误：`NoUiFont`（一个界面字体都没加载到）、`InvalidSize`（位图 0 或超大） |
| 文字 | `text/mod.rs` | 194 | `TextPainter`：cosmic-text 整形 + swash 栅格 + 字形缓存；`measure`/`draw`/`trace_families` |
| 文字 | `text/{size,style}.rs` | 7+52 | `TextSize`（宽 + 行高）；`TextStyle`（像素字号、点字号、行高、色、删除线/下划线、gamma） |
| 字体 | `fonts/mod.rs` | 153 | `FontLibrary`：**不扫系统目录**，按平台清单只加载几个文件；locale 决定中日字形 |
| 字体 | `fonts/{windows,macos,linux}.rs` | 34+64+34 | 三平台的候选文件清单（每个角色给若干候选路径） |
| 字体 | `fonts/directwrite.rs` | 129 | Windows 字体登记：字族名 → 文件路径（`family_files`）、列字族名（设置页用） |
| 字体 | `fonts/trak.rs` | 124 | 手工解析 AAT `trak` 表（含 TTC 面下标），字号插值出每字形附加字距 |
| 字体 | `fonts/ui_font.rs` | 13 | `UiFont`：用户指定字族名 + 壳查出的文件列表 |
| 画布 | `canvas.rs` | 213 | `Canvas`（tiny-skia Pixmap 之上的原语 + 三种位图混合通道）；`round_rect` 路径构建 |
| 画布 | `shadow.rs` | 117 | `Shadow`：圆角轮廓当遮罩，3 遍盒式模糊近似高斯，染色叠加 |
| 画布 | `cloud.rs` | 54 | 云朵图标：四个圆 + 圆角底边并集，描边（大形填色 + 缩一圈的形打 `Clear` 挖空） |
| 画布 | `gear.rs` | 84 | 齿轮图标：8 齿折线外圈描边 + 圆孔；**自绘是为了不受 emoji 字体回退影响**（`gear.rs:1`） |
| 组装 | `renderer/mod.rs` | 310 | `Renderer` + `Metrics`（点→像素）+ 常量表 + `render()` 主流程 + `measure/draw_text` 转发 |
| 组装 | `renderer/{vertical,horizontal,matrix}.rs` | 108+140+249 | 三种排布各自的 `*_size`（量）与 `draw_*`（画） |
| 组装 | `renderer/{item,columns}.rs` | 8+11 | 横排项尺寸 / 竖排列宽行高的中间结构 |
| 组装 | `renderer/top_line.rs` | 97 | 拼音行：段样式映射、**自绘光标**、右侧整句补全/临时状态 |
| 组装 | `renderer/status/{mod,cell,rendered}.rs` | 172+19+11 | Windows 悬浮状态条 `render_status`：各格居中、格间细线、返回 `cell_edges` 命中表 |
| 组装 | `renderer/rendered.rs` | 31 | `Rendered` 输出类型 + `content_size_points()`（像素→点） |

## 核心类型关系

```text
Theme ─ ┐
        ├─→ Metrics{theme, scale}         renderer/mod.rs:58
scale ─ ┘     └─ px(points) = points*scale      renderer/mod.rs:64
                                              style(font,color) → TextStyle   mod.rs:84
Frame ──────┐
Layout ─────┼─→ Renderer::render ─→ Rendered{pixmap(预乘 RGBA), content_*, scale}
Shadow ─────┘        │
                     └─ Renderer 只持一个 TextPainter        renderer/mod.rs:52
                        TextPainter 持：
                          FontSystem（回退链 + locale + opsz 补丁）  text/mod.rs:19
                          SwashCache（字形位图缓存，键=字体×字号×亚像素位移） text/mod.rs:21
                          Buffer（复用的一行缓冲）               text/mod.rs:24
                          tracking: HashMap<ID, Option<Trak>>   text/mod.rs:28
                          gamma_tables: HashMap<u32, [u8;256]>  text/mod.rs:31
```

要点：
- **`Renderer` 是可变对象**（`&mut self`）：measure/draw 都会更新 `Buffer`、字形缓存与两张惰性表。
  帧间复用这些缓存正是性能设计的一部分（见 08）。
- **所有几何量在渲染器内部是像素**：主题按点写，进来先乘 `scale`（`renderer/mod.rs:3` 模块注释）。
- **文字绘制约定**：`y` 一律指"行框顶边"，字形在行高里垂直居中（`renderer/mod.rs:3`）。

## 主流程一：`render()`（renderer/mod.rs:133）

1. 建 `Metrics{theme, scale}`；算 `shadow.margin()`（点→像素；`shadow.rs:33` 定义 margin = blur×2 + offset_y）。
2. `preferred_size()`（`:198`）按排布分派量尺寸：`top_line_size` + `{vertical,matrix,horizontal}_size`，
   取宽的最大值；竖排有最小宽度下限 `MIN_VERTICAL_WIDTH = 200pt`（`:49`，防止无译文时窗口窄得难看）。
3. `Canvas::new(宽, 高)`——含阴影边（`canvas.rs:16`）。
4. `shadow.paint()`：以"内容圆角矩形（下移 offset_y）"为遮罩画三遍盒式模糊，再用阴影色叠上（`shadow.rs:38`）。
5. 画不透明圆角背景 `fill_round_rect`（`canvas.rs:50`，圆角系数 `KAPPA=0.5522848`，`canvas.rs:163`）。
6. 画顶行（返回占用高度），再按 `layout` 分派到 `draw_vertical / draw_matrix / draw_horizontal`。
7. 返回 `Rendered`：`content_x/y = margin`，`content_width/height` 取 `ceil()`（位图尺寸也是 `ceil`，
   `:144` 两处），保证"点尺寸 × scale 向上取整"后窗口不裁边。

**注意 measure 与 draw 会各整形一次**——竖排与横排两处代码里都留了同一句注释：
"量尺寸时已整形过一遍，这里再整形一遍；等渲染器定型再把结果从 render 传下来"
（`renderer/vertical.rs:63`、`renderer/horizontal.rs:84`；矩阵里靠估算快路径绕开大半，见 04）。
这是**已知的、被作者标注的**优化点，不是缺陷。

## 主流程二：`render_status()`（renderer/status/mod.rs:25，仅 Windows 悬浮状态条）

1. 每格宽 = 内容宽 + 两侧 `padding`（`status_cell_width`：文字量宽、齿轮取 `GEAR_SIZE=15pt`，`status/mod.rs:18`）。
2. 取整零头补给最后一格，让**最后一格右边界正好=内容宽**（`:41`）——命中表才能闭合。
3. 背景 + 阴影（与候选窗同源）；逐格：非首格先画 `SEPARATOR_WIDTH=1pt` 的细线（`:21`），
   再 `draw_status_cell` 居中画文字/齿轮；每格结束后把右边界压进 `edges`（`:81`）。
4. 返回 `RenderedStatus{rendered, cell_edges}`——壳按 `x` 落在哪个区间判点击（`status/rendered.rs:9`）。

## 错误与回退（跨层契约）

| 层 | 失败 | 行为 |
| --- | --- | --- |
| `FontLibrary::system` | 界面字体一个都没加载到 | `RenderError::NoUiFont{tried}`（`error.rs:8`） |
| `Canvas::new` | 尺寸 0 / 超大 | `RenderError::InvalidSize`（`error.rs:12`） |
| Windows 壳 | `Painter::new` 拿不到字体库 | 记 warn，**整体退回 GDI 绘制**（`ui/painter/mod.rs:42`） |
| Windows 壳 | 单帧渲染失败 | 记 warn 返回 `None`，该帧退回 GDI（`ui/painter/mod.rs:91`） |
| macOS 壳 | 字体库失败 | 记 warn，退回 AppKit 绘制（`candidates/bitmap/mod.rs:60`） |

两条系统绘制路径（GDI / AppKit）都是上游刻意的**过渡期退路**，由配置 `[general] renderer = "system"` 切换；
设计档说明"稳定一个版本后删"（`docs/design/rendering.md` 的 spike 结果一节）。
