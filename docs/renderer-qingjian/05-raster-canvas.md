# 05 · 光栅与画布：原语、混合、阴影、矢量图标

## 5.1 `Canvas`（`canvas.rs:10`）

`Canvas` 就是"一张 `tiny_skia::Pixmap` + 七个小原语"，坐标一律**像素、左上角原点**（`canvas.rs:1`）。
`sized` 的构造失败（尺寸 0/超大）返回 `RenderError::InvalidSize`（`canvas.rs:16`）。

| 原语 | 位置 | 做什么 |
| --- | --- | --- |
| `fill_rect` | `:38` | 矩形填充（`Rect::from_xywh` + `SourceOver`） |
| `fill_round_rect` | `:50` | 圆角矩形 → 走 `round_rect()` 路径再 `fill_path` |
| `fill_path` | `:64` | 任意路径填充：`FillRule::Winding`、单位变换、**无剪辑参数**（`None`） |
| `blend_pixmap` | `:75` | 整张位图叠放（`alpha()==0` 的像素跳过） |
| `blend_mask` | `:88` | **8 位覆盖率遮罩**（普通字形）按颜色混合（`coverage==0` 跳过） |
| `blend_rgba` | `:115` | 直通 RGBA 位图（彩色 emoji）（`px[3]==0` 跳过；越界提前返回） |
| `blend_pixel` | `:130` | 逐像素 source-over 内核（下面是它） |

`paint()`（`:148`）统一给 skia 填充设 `anti_alias = true`——所有矢量填充（背景、圆角、高亮、图标）
都走 tiny-skia 的抗锯齿光栅。

## 5.2 混合内核与预乘

**没有剪辑栈、没有图层混合模式表**：整张位图就是画布，越界即丢弃（`blend_pixel` 的边界检查，
`canvas.rs:131`）；"挖空"用 `BlendMode::Clear`（图标描边，见 5.4）。source-over 用 8 位定点直写
（`canvas.rs:129`–`:145`）：

```rust
// dst = src + dst × (1 − src.a)   （src/dst 都已预乘）
let inverse = 255 - src.alpha();
let a = src.alpha().saturating_add(mul_u8(dst.alpha(), inverse));
let channel = |s: u8, d: u8| s.saturating_add(mul_u8(d, inverse)).min(a);
```

`mul_u8` 是 `(a×b + 127)/255` 的四舍五入定点乘（`color.rs:50`）；预乘链是
`Color::premultiplied(coverage)` = 先把 alpha 与覆盖率相乘、再把 RGB 按 alpha 预乘（`color.rs:43`、`:55`）。
精度行为的单测就在文件里：红色覆盖率 128 叠在蓝底上 → `(255, 0, 255)`（`canvas.rs:199`）；
半透明灰叠白 → 每通道误差 ≤1（`canvas.rs:211`）。

**文字 gamma 在这里挂**：`blend_mask` 收到的 `data` 已经被 256 项 gamma 表处理过（`text/mod.rs:104`），
混合层本身不做色彩管理。

## 5.3 `Shadow`（`shadow.rs`）

参数（"照 macOS 系统窗口阴影调"，`shadow.rs:1`；Windows 分层窗没有系统阴影，靠它）：

| 字段 | 缺省（`Shadow::mac_panel()`，`:24`） | 含义 |
| --- | --- | --- |
| `blur` | 12.0 pt | 模糊半径 |
| `offset_y` | 6.0 pt | 向下偏移 |
| `color` | `gray(0, 90)` | 纯黑 35% 不透明 |

`margin()`（`:33`）= `blur×2 + offset_y`——渲染器据此在内容四周留边（`renderer/mod.rs:143`）。
画法三步（`paint()`，`:38`）：

1. 新建**整画布大小**的 mask 画布，在 `content + offset_y` 处填一块圆角黑矩形，取 alpha 通道成 `Vec<u8>`；
2. **3 遍盒式模糊**近似高斯（每遍先横后纵；半径 `blur×scale/2`，最小 1px，`:58`）——
   `box_blur_line` 是 O(n) 滑动窗口（`:91`），边界外当 0；
3. 用阴影色做一次 `blend_mask` 叠上（`:63`）。

单测锁定模糊"扩散但守恒"：[0,0,255,0,0] 经 r=1 → [0,85,85,85,0]（`:111`）。

> 成本提示（给 08）：每次 `paint` 都分配两张全画布缓冲（mask + 行/列临时），
> 且对整张位图做 6 次线性扫描；候选窗尺寸（几百×几百 × scale 2）下这是毫秒级里的可观一项。

## 5.4 矢量小图标：云朵与齿轮

两个图标是同一套路（各自模块自述）：

```text
小 Pixmap 上先按外轮廓填色（SourceOver）
再用"同形状、向内收 inset"的路径打 BlendMode::Clear 挖空
 → 剩下的就是描边
```

- **云朵**（`cloud.rs`）：四个圆 + 圆角底边矩形的并集（`:42`–`:52`），描边比例 `STROKE_RATIO = 0.08`（`:10`）；
  对应 macOS 的 SF Symbol `cloud`（18×13 挤进 13×13 方块）与 Windows 的 `☁`（`:1`）。
- **齿轮**（`gear.rs`）：8 齿折线外圈（每齿 4 点：齿根起/齿顶起/齿顶止/齿根止，齿顶窄于齿根——
  `root_half = step×0.28`、`tip_half = step×0.18`，`:60`–`:63`）+ 中心的描边圆孔（`hole = size×0.18`，
  `:33`）；描边比例 `0.09`（`:15`）。

**为什么自绘图标**（上游注释，两条都是"字体回退不可控"这一课）：
- 齿轮："对应 Windows 端原先用的 ⚙ 字形（Segoe UI Symbol）。渲染器自己画是为了不受字体回退影响：
  Segoe UI Emoji 会把 U+2699 画成彩色"（`gear.rs:1`–`:2`）。真机验证记录在
  `docs/design/rendering.md:118`（"齿轮改成矢量画"）。
- 云朵：同理对齐两个平台各自的符号（`cloud.rs:1`）。

## 5.5 颜色与调色板

- `Color` 是纯 sRGB8+alpha，与平台无关；到 tiny-skia / cosmic-text 的换算集中在 `color.rs`
  （`to_skia` `:34`、`to_cosmic` `:38`）——**跨库颜色只此一处**。
- `Palette` 八个色位（`theme/palette.rs:5`）：`text / gloss / pos / fresh / index / cloud / background / highlight`；
  缺省两套取自"macOS 系统语义色在 sRGB 下的实测值"（`palette.rs:1`）：
  候选词 `gray(0,216)` ≈ label 0.847、译文 `gray(0,127)` ≈ secondaryLabel 0.498、词性/序号 `gray(0,66)` 更浅一档；
  生词橙 `(255,141,40)`、云色 `(0,195,208)`（深色档 `(0,210,224)`）、高亮
  浅色 `rgba(176,206,125,127)` / 深色 `rgba(36,76,36,255)`（`palette.rs:33`–`:57`）。
- 深/浅两套主题只是 `Palette` + `text_gamma` 的组合（`theme/mod.rs:47`），渲染路径完全同构。

> 说明：渲染器**没有**渐变/图案/模糊背景的实现（`Paint` 永远单色，`paint()` `:148`）——
> 主题若要毛玻璃一类效果，按设计档的说法是"输出就是一张位图，装饰只是多叠几层，不用换底子"
> （`docs/design/rendering.md:112`），但要先在 `Canvas` 上新增相应原语。
