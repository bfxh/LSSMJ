# 08 · 一帧的成本结构、公开读数与测量方法

> 原则：**只写带锚的读数**；口径（release？哪台机器？哪个场景？）与读数同页给出。
> 本节数字全部来自上游设计档与代码，**不是**本仓亲自测量的——本仓建议的测量见 `09-assessment.md`。

## 8.1 一帧的九个动作（源码侧成本结构）

| # | 动作 | 代码位置 | 成本性质与备注 |
| --- | --- | --- | --- |
| 1 | 量尺寸（全部文字各整形一次） | `renderer/mod.rs:198` → `vertical/horizontal/matrix_size` | **整形是这里的大头**；竖/横排 measure+draw 各一次（`:63`/`:84` 注释），矩阵靠 `estimated_ems` 快路径省掉大半（`matrix.rs:202`） |
| 2 | 建画布（含阴影留边） | `canvas.rs:16` | 一次分配 `w×h×4` |
| 3 | 阴影：mask 画布 + 3 遍盒式模糊 | `shadow.rs:38`–`:63` | 2 张全画布缓冲 + 6 次线性扫描（横竖各 3） |
| 4 | 背景圆角矩形 | `canvas.rs:50` | tiny-skia 抗锯齿路径光栅 |
| 5 | 顶行（段 + 光标 + 右侧补全） | `renderer/top_line.rs:32` | 每段一次 measure + 一次 draw |
| 6 | 候选排布（竖/横/矩阵） | `renderer/{vertical,horizontal,matrix}.rs` | 每行：序号/词/译文各 measure+draw，高亮填充 + 页码 |
| 7 | 字形拉取 | `text/mod.rs:93`（`SwashCache`） | 命中即复用位图；未命中走 swash 栅格（首帧冷启动成本的主要来源，见 8.2） |
| 8 | 逐像素混合 | `canvas.rs:88`/`:115`/`:130` | 手写循环 + 每像素边界检查；**无 SIMD**；每个字形绘制另有一次 `Vec<u8>` 覆盖率表分配（`text/mod.rs:105`） |
| 9 | 贴图 | Windows `layered/mod.rs:95`（RGBA→BGRA 逐像素 + `UpdateLayeredWindow`）；macOS `bitmap/mod.rs:150`（逐行 stride 拷贝 + `drawInRect`） | 与位图像素数成正比；macOS 普通帧只花 `drawInRect`（`bitmap/mod.rs:100`） |

**帧间复用清单**（省掉的重复成本）：`TextPainter.buffer`（`text/mod.rs:24`）、`SwashCache` 字形位图、
`tracking: HashMap<ID, Option<Trak>>`、`gamma_tables`（`:28`/`:31`）；Windows 侧候选窗与状态条
**共用同一渲染器**（`painter/mod.rs:1`）。**没有**的复用：measure 结果的跨阶段传参（双整形）、
阴影缓冲、字形覆盖率 gamma 后的 `Vec<u8>`。

## 8.2 公开读数（逐条带锚；口径随行）

| 读数 | 口径 | 锚 |
| --- | --- | --- |
| 字体库加载 **1.5–4 ms**（5 个文件 67 张面，mmap，只解析名字表与 cmap） | macOS spike，release | `docs/design/rendering.md:84` |
| 预览工具整进程峰值 **RSS 16 MB** | macOS spike（离线工具，非壳） | `docs/design/rendering.md:84` |
| 首帧 **1 ms**（2 倍屏 266×300 pt 一帧） | macOS spike，release，样例帧 | `docs/design/rendering.md:87` |
| 冷首帧 **6 ms**（含字形栅格缓存冷启动） | 同上 | `docs/design/rendering.md:87` |
| 壳内字体库 **3.8 ms** | macOS 壳内运行时 | `docs/design/rendering.md:87` |
| 验收基线："首帧渲染耗时与内存**不劣于**现有 GDI / AppKit 实现" | 换渲染器的门槛条款 | `docs/design/rendering.md:73` |
| Windows 真机（Win11 26200，2026-09-15）全部功能项通过；**首帧耗时未单独测** | 真机功能验收，非性能读数 | `docs/design/rendering.md:123` |

**这些读数覆盖不到什么**（据实声明）：① 只有 macOS spike 一台机、一个 266×300pt 场景；
② 没有稳态分布（P50/P95/P99）、没有连续帧时间、没有不同候选数/窗口尺寸的曲线；
③ Windows 侧无耗时读数；④ 内存只有离线工具的峰值。任何"够快"的结论都应以补齐上述①–④为准。

## 8.3 测量方法（上游自带仪器，改渲染器时照用）

1. **离线并排出图 + 每帧计时**：`cargo run --release -p qingjian-render --example preview -- --out <dir>`
   （`examples/preview.rs:1`）——7 个样例帧 × 深/浅两色（`:81`–`:97`）+ 状态条（`:118`–`:136`），
   每张打印 `宽×高(pt) 用时 路径`（`:106`）。
2. **与原生对表**：`--measure` 只量 12 段文字宽度（11/12/16pt），对 `NSAttributedString.size()`（`:56`–`:77`）。
3. **回退核对**：结束时打印 `青简 hello 🙂 日本語 骨直曜` 等样例的逐段字体族（`trace_families`，`:138`–`:147`）。
4. **真机并排**：设计档记录的方法——"TextEdit 里敲 nihao，`screencapture -l` 抓真实候选窗；
   同一帧人工抄进样例，渲染成 PNG 并排；再把渲染器装进壳抓真机"（`docs/design/rendering.md:78`）。
5. **运行日志**：Windows `painter` 在位图画完后记 `elapsed/width/height`（`painter/mod.rs:93`）；
   macOS 在 `repaint()` 里同理（`bitmap/mod.rs:145`）；字体库就绪记 faces/ui_family（`fonts/mod.rs:95`）。
   打开 `RUST_LOG=qingjian_render=debug` 即可拿逐帧耗时。

## 8.4 成本直觉（由 8.1 结构导出的三个"本该如此"）

- **窗口越小越划算**：除字体库/字形缓存外，1–4、9 全部随位图像素数线性；
  设计档对形态的选择（"几百乘几百像素，亚毫秒"，`docs/design/rendering.md:60`）正是押在这一条上。
- **候选窗是"每帧全画"**：没有脏矩形/局部重绘机制（`fill_path` 无剪辑参数、无分层缓存，
  `canvas.rs:64`），但文本量小使全画可接受；**若窗口向"大面板/多行滚动"演进，这是第一个要动的地方**。
- **矩阵模式的快路径是可推广的模式**："能估算就不实测"（`matrix.rs:202`）在候选多、格子多时
  直接省一半整形——同类机会还有竖/横排的双整形（`:63`/`:84` 注释已把它记为 TODO）。

## 8.5 复算清单（拿到新机器时按序做）

```text
1) cargo run --release -p qingjian-render --example preview -- --out /tmp/prev   # 每帧耗时 + 出图
2) 同命令加 --measure                                                           # 文本宽度对表
3) RUST_LOG=qingjian_render=debug 跑壳（macOS/Windows）                          # 字体库/逐帧日志
4) 与原生候选窗并排截图（方法见 8.3.4）                                          # 观感对齐
```
（第 1 步需要一个能跑 Rust 的桌面环境；本仓未代跑——见 `09-assessment.md` 的测量建议。）
