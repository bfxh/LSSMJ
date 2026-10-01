# 07 · 平台贴图后端

> 渲染器的契约只有两件事留在壳侧：**把位图贴上窗口**、**把点击坐标传回来**
> （`docs/design/rendering.md:49`；iOS/Android 明确"命中测试在 Rust 里做"，`:56`）。
> 本文档覆盖已实现的两个桌面后端（Windows / macOS）；Linux 无显示面（见 7.3）。

## 7.1 Windows：分层窗口 + GDI 退路

### 装配与热切换（`apps/windows/server/src/ui/painter/mod.rs`）

- UI 线程共享一份：`SharedPainter = Rc<RefCell<Option<Painter>>>`（`:16`）——**候选窗与状态条共用**
  同一个渲染器（连同字体库与字形缓存，"字形缓存共享"，`:1`）。
- `Painter { renderer, font }`（`:18`）：`font` 记"建它时用的字族名"，`configure()` 只在
  **`[general] renderer` 或 `[general] font` 变化时**重建（`:58`–`:73`）：
  - `Qingjian` 档：字体名没变不重建；
  - `System` 档：`*painter = None`，两个窗口切回 GDI（`:66`–`:71`）。
- 每帧换算：`theme(dark)`（`:119`）+ `scale(dpi) = dpi.max(96)/96`（`:124`）+ 固定
  `SHADOW = Shadow::mac_panel()`（`:117`，理由："分层窗口没有系统阴影"）。

### 失败回退链（三层，箭头是回退方向）

```text
Painter::new 字体库失败 ──→ 记 warn，返回 None ──→ GDI 绘制（painter/mod.rs:42）
单帧 render 失败       ──→ 记 warn，返回 None ──→ 该帧 GDI（painter/mod.rs:91）
配置 renderer="system" ──→ 主动切 None        ──→ GDI 绘制（painter/mod.rs:66）
```
（上游把 GDI / AppKit 旧路径都定义为"过渡期退路"，`docs/design/rendering.md:116`。）

### 贴图：`layered::present`（`ui/layered/mod.rs:95`）

1. `Pixmap`（tiny-skia，预乘 **RGBA**）→ DIB（预乘 **BGRA**）：`chunks_exact_mut(4)` 逐像素换通道
   （`dst[0]=blue, dst[1]=green, dst[2]=red, dst[3]=alpha`，`:102`–`:107`）——"都是预乘，只换通道顺序"（`:101`）。
2. `UpdateLayeredWindow(hwnd, dst, size, hdc, src, COLORREF(0), BLENDFUNCTION{AC_SRC_OVER, AC_SRC_ALPHA},
   ULW_ALPHA)`（`:112`–`:139`）：**一次调用整张位图贴上并定位**，窗口不做 GDI 重绘。
3. 状态条：`render_status` 出的位图走同一条 `present`（调用点 `ui/status/mod.rs:255`；候选窗在
   `ui/candidates/mod.rs:146`），`cell_edges` 供点击命中。

`layered::composite`（`:58`）是**GDI 路径**的合成器（圆角+阴影+一段 GDI 内容）：
阴影参数硬编码在文件里——留边 `SHADOW_MARGIN=16` 逻辑像素（按 DPI 缩放，`:29`）、
定向主阴影峰值 `KEY_MAX=65`（光从上方来）、环境光晕 `AMBIENT_MAX=18`、衰减 `(1−d/margin)²`（`:167`），
画完还要把 GDI 写坏的内容区 alpha 补回 255（`restore_content_alpha`，`:181`，注释解释了原因：
"GDI 只写 RGB、把碰到的像素 alpha 留成 0（分层窗口里会全透明）"，`:89`）。
渲染器路径取代它之后，这段只服务退路。

### 真机验收（`docs/design/rendering.md:123`，2026-09-15）

Windows 11 26200 上通过：深/浅色、竖排/横排、Segoe UI Emoji（COLRv0）彩色、阴影、悬浮状态条与
矢量齿轮、`renderer`/`font` 设置与热切换（换成 Maple Mono NF CN 立即生效）、`renderer="system"`
退回 GDI。**留档缺口**：灰度抗锯齿与微软雅黑回退只是"看着与 GDI 版没有可感差异"（主观），
**Yu Gothic 回退与首帧耗时没有单独测**（同一行原文）。

## 7.2 macOS：NSImage 贴图 + 系统阴影

### `BitmapPainter`（`apps/macos/src/candidates/bitmap/mod.rs`）

- 持"最近一帧"五元组（frame / layout / dark / scale / size，`:27`–`:41`）：
  `set_frame()` 每来一帧就 `repaint()` 并返回点尺寸（`:81`–`:97`）；
  `draw()` 只在 **dark 或 scale 变化时**才重画，然后直接把缓存 `NSImage` 贴上（`:100`–`:105`）——
  普通帧的"重绘"成本 = 一次 `drawInRect`。
- 贴图：`to_image()`（`:150`）建 `NSBitmapImageRep`（8 位 × 4 通道、预乘 alpha、`bytesPerRow = w×4`），
  **按行拷进 AppKit 的 stride**（`copy_nonoverlapping(..., row*stride)`，`:171`），包成 `NSImage`；
  `drawRect:` 里 `drawInRect(Copy, 1.0, respectFlipped=true)`（`:112`–`:119`）。
- **不画阴影**（模块注释 `:4`）："面板背景透明、系统阴影按位图的 alpha 走" ——
  所以 `render(..., shadow: None)`（`:133`）。
- `scale` 缺省 2.0（`:75`），由 `set_frame` 每帧传入（Retina）。
- 字体：用户字族名 → 文件列表由壳查（"mac 壳用 CoreText 按字族名查出文件"，`docs/design/rendering.md:109`）；
  查不到就退回系统字体并记日志（`bitmap/mod.rs:44` 的注释路径，实现见 `with_ui_font` 的失败分支，`fonts/mod.rs:68`）。
- `convert.rs`（`:1`）：壳帧 → 渲染器帧的**过渡兼容层**，"spike 定型后壳直接用渲染器的类型，这层就没了"——
  字段一一对应（`:7`–`:57`）。注意 macOS 侧 `highlighted` 恒为 `Some`（`:11`），与渲染器 `Option` 语义统一。

### 与旧路径并存

`[general] renderer = "system"` 走旧的自绘 `NSView` 路径（`view/mod.rs`），
两条路径**同一套规则**（矩阵亦然：`renderer/matrix.rs` 与 `view/matrix.rs`，`docs/design/candidate-ui.md:28`）。

## 7.3 Linux 与移动端（现状）

- Linux：输入法本身是 Fcitx5 / IBus 的插件，**候选窗由框架画**，"我们只出引擎，显示面几乎为零"
  （`docs/design/rendering.md:33`）；`fonts/linux.rs` 的清单为未来/非显示面场景保留。
- iOS / Android：计划为"键盘 View 贴位图，命中测试在 Rust 里做，自绘位图占几 MB，符合内存上限"
  （`docs/design/rendering.md:56`）——**未实现**（设计档把手机端定位为按需再做的方向）。

## 7.4 回退与热切换总表

| 触发器 | 行为 | 锚 |
| --- | --- | --- |
| `[general] renderer = "system"` | Windows：两个窗口回 GDI；macOS：回 AppKit | design:116 / `bitmap/mod.rs:3` |
| 字体库加载失败 | 记 warn，自动退回系统绘制 | `painter/mod.rs:42` / `bitmap/mod.rs:60` |
| 指定的自定义字体没装/名字对不上 | warn + **回到系统字体**（渲染器继续用） | `fonts/mod.rs:68` |
| 单帧渲染失败 | 该帧退系统绘制（记 warn） | `painter/mod.rs:91` |
| `dark`/`scale` 变化（macOS） | 下一帧重画位图 | `bitmap/mod.rs:100` |

## 7.5 命中与交互的边界（有意留白）

渲染器**只**为状态条产出命中表（`cell_edges`，`renderer/status/rendered.rs:9`）。
候选窗的点击/拖拽/悬停命中**不在本 crate**——上游把它留给各壳/引擎（点击选词、拖拽移动窗口），
`docs/design/rendering.md:48` 的方案表述是"输出一张 BGRA 位图与**命中区域表**"（设计意图），
现状实现里这一项只对状态条落地（**这是设计与实现的已知差距，记此备查**）。
