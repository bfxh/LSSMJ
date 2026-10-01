# FreeType：LCD 滤波 / 提示模式 / 专利 / Auto-Hinter

> 锚：`W1F-067..072,087,088`。来源：
> 2.14.3 API 参考 <https://freetype.org/freetype2/docs/reference/ft2-lcd_rendering.html>、
> 官方 CHANGES <https://raw.githubusercontent.com/freetype/freetype/master/docs/CHANGES>、
> 专利页 <https://freetype.org/patents.html>、Auto-Hinter 页 <https://freetype.org/autohinting/index.html>。
> 注：旧的 `freetype2/docs/text-rendering-general.html` 与 `.../hinting.html` 已 404（站点改版），
> 本批改用 API 参考页与 CHANGES——**引用 FreeType 文档时先验 URL**。

## 1. LCD 滤波（可直接抄的数）

- 默认滤波器："beveled, normalized, and color-balanced five-tap filter with weights of
  `[0x08 0x4D 0x56 0x4D 0x08]` in 1/256 units."（`W1F-067`）——可复算、无需自造。
- `FT_LCD_FILTER_NONE` 用于亚像素时 "results in sometimes severe color fringes"（`W1F-068`）
  → 彩边=亚像素档头号回归项。
- 相关 API（同页，未入账）：`FT_Library_SetLcdFilter` / `FT_Face_Properties` +
  `FT_PARAM_TAG_LCD_FILTER_WEIGHTS`（按 face 设置权重）、`FT_Library_SetLcdGeometry`；
  且 2.10.3 起默认启用 `FT_LCD_FILTER_DEFAULT`（`W1F-087`）。

## 2. 提示模式：版本即渲染行为

- CHANGES："the 2.7.x series now uses the new subpixel hinting mode as the default, emulating a modern
  version of ClearType."（`W1F-069`）——**升 FreeType 小版就可能改渲染结果**，
  我们"钉版本"必须连带"钉渲染快照/金丝雀图像"。
- CHANGES（stem darkening 为何默认关）："you need linear alpha blending and gamma correction to get
  correct rendering results"（`W1F-070`）——与 `gamma-and-subpixel.md` 的"先 gamma 后增重"互证。
- Auto-Hinter 职责："a module used to align glyph outlines to the pixel grid in order to tremendously
  improve glyph images"（`W1F-072`）——hinting 档位的分界线=是否对齐网格。

## 3. 专利：旧假设已过期（防后人重启）

- "Since May 2010, all patents related to bytecode hinting have expired worldwide."（`W1F-071`），
  FreeType 2.4 起默认启用字节码解释器。
- 同页另有 ClearType 色彩滤波专利 2019-08 到期（`W1F-088`）——**技术选型文档里
  "hinting 有专利风险"这类理由自 2010/2019 年起已不成立**，写文档时不许再当约束用。

## 对 LSSMJ 文本层的取舍

- **不吸收提示全档**：候选窗小字号 + 与系统观感对齐的目标下，用 FreeType/swash 的默认档即可；
  若用户字体带 hinting，交给库的默认（此即"不引入自定义 hinting 引擎"的决策依据）。
- **吸收两条纪律**：
  1. 亚像素若做，滤波权重用官方五点数组（`W1F-067`），不自造；
  2. 依赖版本的渲染行为差异进"变化日志"（`W1F-069`）——每次升级字体/光栅依赖跑金丝雀。
- **文档改写**：把"专利限制"从风险清单删除，改为历史注记（`W1F-071`）。
