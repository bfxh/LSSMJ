# gamma 与亚像素渲染：Novak + Raster Tragedy + ClearType/DWrite

> 锚：`W1F-038..050,067,068,073..075`。三组来源：
> Novak 长文 <https://blog.johnnovak.net/2016/09/21/what-every-coder-should-know-about-gamma/>；
> Beat Stamm <http://rastertragedy.com/RTRCh2.htm>（第一章 `RTRCh1.htm`）；
> 微软 <https://learn.microsoft.com/en-us/windows/win32/gdi/cleartype-antialiasing> 与
> <https://learn.microsoft.com/en-us/windows/win32/api/dwrite/nn-dwrite-idwriterenderingparams>。

## 一、gamma 为什么是"必做"而非"美化"（Novak）

- 口径：显示系统标准 gamma ≈ 2.2，"approximately matches the power law sensitivity of human vision"
  （`W1F-038/041`）。
- 错误清单：在 gamma 编码空间里做 "resizing, blurring, compositing, interpolating between pixel
  values, antialiasing" 全错（`W1F-039`）——**字形降采样与覆盖率合成都在清单里**。
- 作者专列"Effects of gamma-incorrectness"一节（渐变/混合/缩放/AA 的可见后果，`W1F-042`）
  → 可直接当我们的回归样本集（浅底细笔画最敏感）。

## 二、亚像素渲染的结构性代价（Stamm）

- 归类："ClearType, FreeType, and Quartz do not differ in spirit from ClearType, hence the generic
  term sub-pixel anti-aliasing method."（`W1F-047`）——同族，差异只在滤波器。
- 代价 1：y 向无抗锯齿（`W1F-048` "the complete absence of anti-aliasing in y-direction"）。
- 代价 2：**无 gamma 时的"weight gain"（变粗）**（`W1F-049`）。
- 诚实定义：AA 是"以更好看为目标替换采样伪影"（`W1F-050`）→ 我们的判据只写可复算项。

## 三、平台侧参数（微软）

- ClearType 定位=平滑方法、提升"display resolution"感（`W1F-073`）；**硬件前提**=竖条纹 RGB LCD
  （`W1F-074`）——方向/顺序敏感 ⇒ 必须有探测与否决清单。
- DWrite 渲染参数三旋钮：ClearType level / enhanced contrast / gamma（`W1F-075`）。
- FreeType 侧配套：五点滤波权重 `[0x08 0x4D 0x56 0x4D 0x08]`（`W1F-067`）/ 不滤波=严重彩边
  （`W1F-068`，详见 `freetype-hinting-lcd.md`）。

## 对 LSSMJ 文本层的取舍

1. **gamma 表（浅 0.85/深 0.75）保留**，并把依据改为"2.2 幂律的 UI 化近似"（`W1F-041`），
   附复算脚本口径；
2. **亚像素=可选档**，启用条件=几何探测通过 + 彩边金丝雀通过 + （若开 stem 增强）gamma 已开；
3. **验收三条**：x 向清晰度、y 向锯齿（`W1F-048`）、重量漂移（`W1F-049`）——后两条是"变清晰"的代价侧，
   常被漏测；
4. 顺序纪律：**先 gamma，后字重增强**（`W1F-070` 与之互证），不许反序。
