# 10 · 术语表（给读源码的人）

> 每条给"一句话定义 + 在本 crate 里的落点"。上游术语（中文注释口径）照抄，英文为代码标识。

## 产品/形态

| 术语 | 定义 | 落点 |
| --- | --- | --- |
| **显示面**（display surface） | 候选窗、悬浮状态条、手机键盘面板这类"只画不交互"的界面 | `docs/design/rendering.md:13` |
| **控件面**（control surface） | 设置程序这类需要完整控件库/无障碍的界面；**不自绘**，走各平台原生 | `docs/design/rendering.md:14` |
| **候选窗** | 输入法在光标旁弹出的候选列表窗口 | `renderer/mod.rs:1` |
| **状态条**（悬浮状态条） | Windows 侧 `[中/英][，。][⚙]` 三格小条 | `renderer/status/mod.rs:1` |
| **preedit（拼音行）** | 候选窗顶部显示已敲拼音的行（分段 + 光标） | `frame/preedit/mod.rs:10` |
| **辅码**（aux code） | 候选词后紧跟的编码提示（如 `[kf]`）；`Tone::Code` 色 | `frame/row.rs:13` |
| **annotation** | 候选词右侧的译文/词性片段列（`Vec<(String, Tone)>` 按序绘制） | `frame/row.rs:16` |
| **云联想** | 云端返回的词/整句；渲染上表现为云朵图标 + 云色 | `renderer/mod.rs:120`、`261` |
| **生词**（fresh） | 上屏时在候选窗出现轮次 < 3 的译词，用橙色强调（判定在 Core） | `docs/design/candidate-ui.md:54` |
| **矩阵**（matrix） | 横排展开成固定列数网格的候选布局（`Frame::columns > 0`） | `renderer/matrix.rs:1` |
| **唯一不变量：窗口不跳** | 矩阵的列宽只由整份候选决定，滚动/移高亮不改窗口尺寸 | `renderer/matrix.rs:3` |

## 文本与字体

| 术语 | 定义 | 落点 |
| --- | --- | --- |
| **整形**（shaping） | 文本 → 字形序列（含连字/kerning/复杂文种） | `text/mod.rs:163`（`Shaping::Advanced`） |
| **覆盖率遮罩** | swash 输出的 8 位/像素字形位图（普通字形） | `text/mod.rs:103`、`canvas.rs:88` |
| **`trak`** | AAT 字距表：Apple 系统字体按字号给每字形加减间距 | `fonts/trak.rs:1` |
| **`opsz`（光学字号）** | 可变字体轴；CoreText 在 20pt 以下用 17 | `renderer/mod.rs:47`、`docs/design/rendering.md:91` |
| **text_gamma** | 覆盖率 gamma 查找表，用于对齐 CoreText 的"笔画加深" | `theme/mod.rs:40`、`text/mod.rs:186` |
| **TTC / 面（face）** | TrueType Collection 与其中单个字体面（解析需面下标） | `fonts/trak.rs:66` |
| **sbix / COLRv0** | 两种彩色 emoji 字体格式（Apple / Segoe UI Emoji） | `docs/design/rendering.md:82` |
| **字形缓存**（SwashCache） | 键 = 字体 × 像素字号 × 亚像素位移 | `text/mod.rs:21` |

## 图形与平台

| 术语 | 定义 | 落点 |
| --- | --- | --- |
| **预乘 alpha** | RGB 已乘 alpha 的表示；本 crate 的正式输出契约 | `lib.rs:5`、`layered/mod.rs:101` |
| **盒式模糊** | 滑动窗口均值模糊；3 遍近似高斯 | `shadow.rs:91` |
| **`Clear` 挖空** | 用 `BlendMode::Clear` 把内缩形从图标里抠掉，得到描边 | `gear.rs:29`、`cloud.rs:25` |
| **分层窗口**（layered window） | Windows 的 `UpdateLayeredWindow` 逐像素 alpha 窗口 | `layered/mod.rs:95` |
| **DIB / BGRA** | Windows 位图格式（通道序与 tiny-skia 的 RGBA 不同） | `layered/mod.rs:101` |
| **stride** | 位图每行字节数（可能大于 宽×4，AppKit 侧要按行拷） | `bitmap/mod.rs:172` |
| **cell_edges** | 状态条各格右边界表；**渲染器唯一的命中数据** | `renderer/status/rendered.rs:9` |
| **column_ems** | 矩阵每列预留的候选字宽（按整份候选估；估宽：ASCII 0.62 / 其余 1.0） | `frame/mod.rs:25`、`matrix.rs:227` |
| **Rendered.content_*** | 内容区在位图内的偏移与尺寸（阴影边之外） | `renderer/rendered.rs:8` |

## 工程口径（本仓文档集用）

| 术语 | 定义 |
| --- | --- |
| **证据锚** | `file:line` + 逐字引文，或 URL + 引文；`tools/ledger.py` 机器校验（±5 行容差） |
| **深度阶梯** | `source`（逐字读源码）/ `doc` / `paper` / `web`（旁证，不得单独支撑决策） |
| **in-repo 文档** | 上游仓库内的设计/笔记文档；本仓按 `targets.md` C6 约定以 `source` 深度计入（**统计报表中披露构成**） |
| **估计量口径** | 读数必须带：版本/提交、机器、场景、样本范围（不同口径的结论不可互套） |
