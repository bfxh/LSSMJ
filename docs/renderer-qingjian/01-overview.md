# 01 · 定位与总体数据流

## 一句话

`qingjian-render` 把"候选窗的一帧"（拼音行、候选行、高亮、页脚、右侧整句补全）按主题**画成一张位图**；
各平台壳只做两件事：**把位图贴到窗口上**、**把点击/拖拽坐标传回来**（`lib.rs:1` 的 crate 自述，
命中表见状态条的 `cell_edges`，`renderer/status/rendered.rs:9`）。

## 它服务的场景（决定了全部技术选型）

上游设计档把输入法的 UI 分成两类（`docs/design/rendering.md:13`–`:14` 的对照表）：

| 种类 | 例子 | 要求 | 结论 |
| --- | --- | --- | --- |
| **显示面** | 候选窗、悬浮状态条、手机键盘面板 | 无边框、不抢焦点、置顶、按键后毫秒级出现 | 适合自绘（矩形/阴影/渐变/文字） |
| **控件面** | 设置程序 | 表单、表格、滚动、文本框、无障碍 | 不自绘，继续各平台原生 UI |

本 crate 只负责**显示面**。选型结论（设计档 2026-09-13）：一个 Rust crate 输出位图，平台只贴图；
栅格用 `tiny-skia`（纯 CPU）、文字用 `cosmic-text`；**不上 GPU**——理由（设计档原文）：
"候选窗 2 倍屏几百乘几百像素，亚毫秒；不上 GPU（vello / wgpu 对这个尺寸是负担）"（`docs/design/rendering.md:60`）。

同类场景的硬约束（设计档调研，均带出处）：iOS 键盘扩展内存上限 48–60 MB（phys_footprint，超出被
jetsam 静默杀）、Flutter 进 iOS 键盘扩展当天不可用、Linux 输入法候选窗由 Fcitx5/IBus 自己画、
WebView 因渲染不一致被排除——**这些约束共同指向"小位图 + 平台贴图"这一形态**。

## 契约（壳需要提供什么、得到什么）

输入（`Renderer::render`，`renderer/mod.rs:133`）：

| 参数 | 语义 |
| --- | --- |
| `frame: &Frame` | 一帧的全部内容（纯展示形态，不含排序/查词；`frame/mod.rs:1`） |
| `layout: Layout` | `Vertical` 竖排 / `Horizontal` 横排（`layout.rs:4`；矩阵 = 横排且 `frame.columns > 0`） |
| `theme: &Theme` | 字体、配色、间距——**单位是点**（`theme/mod.rs:1`） |
| `scale: f32` | 点 → 像素倍数（Retina 为 2；Windows 壳取 `dpi.max(96)/96`，见 07） |
| `shadow: Option<&Shadow>` | `None` = 壳自己带系统阴影（macOS）；`Some` = 渲染器画（Windows 分层窗） |

输出（`Rendered`，`renderer/rendered.rs:5`）：**预乘 alpha 的 RGBA 位图** + 内容区在位图内的位置尺寸
（`content_x/y/width/height`，含阴影边之外的那块）+ 生成它的 `scale`。
另有 `render_status`（`renderer/status/mod.rs:25`）画 Windows 悬浮状态条，输出位图 + **各格右边界**
（`RenderedStatus.cell_edges`，点击命中用）。

## 一帧的路径

```text
引擎/壳（Swift / Rust / Kotlin…）
   │  Frame { preedit, rows, highlighted, columns, column_ems, footer, sentence, status }
   │  Layout + Theme + scale + shadow
   ▼
Renderer::render                       renderer/mod.rs:133
   ├─ preferred_size() 先量一遍所有文字      renderer/mod.rs:198（每种排布各自的 *_size）
   ├─ Canvas::new(宽+阴影边, 高+阴影边)     canvas.rs:16
   ├─ Shadow::paint（可选，3 遍盒式模糊）    shadow.rs:38
   ├─ 圆角背景 fill_round_rect             canvas.rs:50
   ├─ 顶部拼音行（段 + 自绘光标 + 右侧补全） renderer/top_line.rs:32
   └─ 三种排布之一：
        Vertical   → renderer/vertical.rs:54   （序号/词/译文三列，页码右下）
        Horizontal → renderer/horizontal.rs:72 （单行 + 高亮译文行）
        矩阵       → renderer/matrix.rs:61     （固定列宽网格 + 信息行）
   ▼
Rendered { pixmap, content_*, scale }
   │
   ▼  壳：贴图
Windows：RGBA→BGRA 逐通道拷贝 → UpdateLayeredWindow（ULW_ALPHA，预乘）
         apps/windows/server/src/ui/layered/mod.rs:95
macOS：  NSBitmapImageRep(8bpc/4 通道/预乘) → NSImage.drawInRect(Copy)
         apps/macos/src/candidates/bitmap/mod.rs:150
```

文字是怎么画上去的：每个字形先由 `SwashCache` 出位图（普通字形 = 8 位覆盖率遮罩，彩色 emoji = RGBA），
渲染器**自己写 source-over 混合**逐像素叠到画布（`canvas.rs:88`、`canvas.rs:115`、`canvas.rs:130`），
文字混色前还过一道覆盖率 gamma 查找表（`text/mod.rs:186`）。

## 代码规模（复核口径：`wc -l`，提交 c08ae57c）

| 区域 | 文件数 | 行数 | 备注 |
| --- | --- | --- | --- |
| `src/` 全部 | 38 | 2886 | 含测试（`#[cfg(test)]` 在文件内） |
| `examples/preview.rs` | 1 | 404 | 离线预览/量宽/回退核对工具 |
| 合计 | 34 | 3290 | — |

最大的三个实现文件：`renderer/mod.rs`（310）、`renderer/matrix.rs`（249）、`canvas.rs`（213）。

## 下一步读什么

- 想看**模块怎么切**：`02-architecture.md`。
- 想看**文字怎么对齐原生**（opsz / trak / gamma 三件套）：`03-text-pipeline.md`。
- 想看**为什么窗口不跳**（固定列宽矩阵）：`04-layout-and-frames.md`。
- 想看**成本在哪**：`08-performance.md`。
