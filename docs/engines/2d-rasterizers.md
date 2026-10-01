# 2D 矢量光栅器：tiny-skia / lyon / femtovg / vello / pathfinder / raqote（抓取 2026-10-01）

> 目标轨道 A4。事实来自 `D:/KF/LSSMJ/scratch/src/<slug>` 的浅克隆，每条主张指到 `docs/analysis/ledger/w1d.jsonl`
>（W1D-nnn）。**口径声明**：只覆盖下列 commit；"更快/更慢"一律带原作者给出的机器与版本限定，不升格为普遍结论。

| 目标 | 上游 | commit（全） | 本地克隆 |
| --- | --- | --- | --- |
| tiny-skia（青简直接依赖） | https://github.com/linebender/tiny-skia | `5d4754777746eef0828be166896eaf482c49f8f2` | `scratch/src/tiny-skia` |
| lyon | https://github.com/nical/lyon | `d036e421cb39f4ef5fd18b4d211b9abfb191d0aa` | `scratch/src/lyon` |
| femtovg | https://github.com/femtovg/femtovg | `f57a2c39e9836c146556c58c98d80c5bf7899029` | `scratch/src/femtovg` |
| vello | https://github.com/linebender/vello | `c7269fba54aa68a84470dc64b1f297dbadf4da94` | `scratch/src/vello`（sparse） |
| pathfinder | https://github.com/servo/pathfinder | `6c3c0466f451c5bd2007087728cd168798cd64e8` | `scratch/src/pathfinder` |
| raqote | https://github.com/jrmuizel/raqote | `9f1340c8ce3909286601a059a3c2077c3502a059` | `scratch/src/raqote` |

## TL;DR（每条带锚）

1. tiny-skia 是 Skia CPU 光栅子集的 Rust 移植，管线"编译"成函数指针数组；**全部** stage 有 lowp 实现才走 u16，否则整条 f32（`src/pipeline/mod.rs:19`；W1D-002）。
2. AA 填充 = 2-bit 纵向超采样（4x）+ 稀疏 RLE 覆盖率；覆盖率 0 的 run 跳过、255 走矩形快路径，只有中间值才跑管线（`src/scan/path_aa.rs:21`；`src/alpha_runs.rs:17`；`src/pipeline/blitter.rs:273`；W1D-008/010/023）。
3. 硬边界由定点数决定：AA 路径裁剪坐标 ≤32767、画布 >8191 自动分块、路径 bounds 超 `SCALAR_MAX*0.25` 直接拒画（`src/scan/path_aa.rs:64`；`src/painter.rs:622`/`:592`；W1D-009/014/016）。
4. 两处**静默**降级：路径∩裁剪框 `<<2` 溢出 short 时直接退非 AA；超 2^22 的三次曲线裁剪打回直线（`src/scan/path_aa.rs:55`；`src/edge_clipper.rs:475`/`:212`；W1D-108/017/018）。
5. 快路径密度是主要性能来源：SourceOver+不透明→Source、Source+纯色→memset、pattern 单位阵/平移→Nearest、AA 矩形走 `hairline_aa` 专用路径（`src/pipeline/blitter.rs:57`/`:63`；`src/shaders/pattern.rs:112`；`src/scan/hairline_aa.rs:47`；W1D-021/022/042/043）。
6. 裁剪是最贵一环：Mask 是整幅 8bit（自认"比 Skia 慢很多"），`intersect_path` 还要另建同尺寸临时 mask（`src/mask.rs:38`/`:349`；W1D-027/028）。
7. 官方性能口径：README 自报 x86-64 慢 Skia 20-100%、ARM 慢 100-300%；bench 环境 AMD 3700X / Apple M1 / Skia v90 / Rust 1.62（W1D-046/051，**无日期标注**）。
8. lyon 是纯 CPU 细分（三角化）库，默认值与直觉相反：默认填充规则 `EvenOdd`；容差 API 上 0.1 而实现取一半 0.05（`crates/tessellation/src/lib.rs:479`/`:478`；`crates/tessellation/src/fill.rs:786`；W1D-054/060）。
9. femtovg 是 nanovg 的 Rust 移植（单一 OpenGL(ES) 3.0+ 后端）：AA 靠路径边界 fringe 三角带；凸单轮廓走 ConvexFill，凹/多轮廓走 stencil-then-cover；细描边用"降 alpha 的粗线"近似（`src/lib.rs:878`/`:3294`/`:3368`/`:3484`；W1D-072/068/069/070）。
10. vello 在本 commit 已重组：主线是 Sparse Strips 的 `vello_cpu`/`vello_gpu`（4x4 tile、解析 AA、CPU 侧深度缓冲），原 compute-centric `vello` 移入 `research/`（W1D-086；`vello_common/src/tile.rs:263`；`strip.rs:589`；`vello_cpu/src/coarse/depth.rs:31`；W1D-074/076/079）。

## 可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 双层管线（f32/u16）按 stage 全有全无地选 | W1D-002/034 | 吸收（"全有全无"的判定要保留，别逐 stage 混） |
| 稀疏 RLE 覆盖率 + 按 run 分发的 0/255 快路径 | W1D-010/023 | 吸收（候选窗大片空区/纯色区直接受益） |
| 强度削减：SourceOver+不透明→Source、Source+纯色→memset | W1D-021/022 | 吸收（背板/分隔线/选中条的最小成本路径） |
| AA 矩形专用快路径（不走路径光栅） | W1D-043 | 吸收（圆角/边框尽量拆成矩形图元以命中） |
| 细线用"降 alpha 的粗线"近似 | W1D-070 | 吸收（亚像素线的通用近似） |
| lyon 的闭式单步步长曲线展平 | W1D-058/059 | 有界吸收（若青简自研展平器，可借"步长与迭代分离"的结构） |
| 定点数硬边界（8191 画布 / 32767 裁剪） | W1D-009/014/015 | 有界吸收（青简画布远小于此 ⇒ 常规不触发；大坐标场景需自测） |
| 静默降级（退非 AA / 曲线打回直线） | W1D-017/018/108 | 有界吸收（必须补日志/计数，不能静默丢质量） |
| femtovg 的 fringe 边带 AA + 凸/凹分流 | W1D-064/068/069 | 有界吸收（思路可借；GL 后端不能直接移植） |
| vello 的 4x4 tile + Sparse Strips（满覆盖区紧凑表示） | W1D-074/085 | 有界吸收（表示法可借；工程量另算） |
| vello 解析 AA（像素覆盖 = winding 积分，非超采样） | W1D-076/077 | 有界吸收（质量上限更高；需重写覆盖率生成，可作二期） |
| vello CPU 侧深度缓冲（不透明段前→后 + 深度剔除） | W1D-079/080 | 有界吸收（依赖"大量重叠不透明层"，青简候选窗重叠少） |
| pathfinder 面积 LUT 解析覆盖（自述等效 256xAA） | W1D-091/098 | 有界吸收（GPU/LUT 路线；CPU 需自建面积表） |
| pathfinder 16x16 tile + 实色瓦片遮挡剔除 | W1D-089/096 | 有界吸收（16x16 vs vello 4x4 是粒度取舍；青简窗口小、收益有限） |

## 不可吸收

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 8bit 全画布 alpha mask 裁剪栈 | W1D-027/028/045 | 不吸收（青简裁剪应以矩形为主、按需生成 mask） |
| 不做任何资源缓存（字形/路径都要上层自管） | W1D-047/048 | 不吸收（缓存必须青简自己做；文本须外部栅格化后以 Mask/Pattern 喂入） |
| 无全局 alpha | W1D-052 | 不吸收（整窗淡入淡出需逐层改色或走 Pattern） |
| lyon 默认 EvenOdd / 实际容差取一半 | W1D-054/060 | 不可作为默认（须显式传 NonZero；引用容差须用实现值 0.05） |
| raqote 的整形状 alpha mask | W1D-104 | 不吸收（作者本人打算改成 RLE；青简无理由复刻） |
| raqote 描边重叠不去重 | W1D-105 | 不吸收（半透明自交处会叠加，可见正确性坑） |


## 1. 架构全景（模块地图，逐文件职责）

### 1.1 tiny-skia（4 层：几何 / 扫描 / 管线 / 位图）

```
path/           非对称依赖：独立的 tiny-skia-path crate（Cargo.toml:15 `members = ["path"]`，W1D-120）
  path.rs / path_builder.rs   路径与构建器
  stroker.rs      2195 行 — 描边（含 dash 分离到 dash.rs）
src/
  pixmap.rs       只此一种像素格式：预乘 RGBA8888，4B/px（W1D-032）；width == stride，无行 padding（W1D-033）
  color.rs        颜色与四种 ColorSpace（Linear/Gamma2/SimpleSRGB/FullSRGBGamma）（W1D-025）
  blend_mode.rs   29 种混合模式 → pipeline::Stage 的映射 + 覆盖率预乘表（W1D-024）
  mask.rs         裁剪/蒙版：整幅 8bit alpha（W1D-027），intersect_path 另建临时 mask（W1D-028）
  path64/         Skia PathOps 双精度辅助（cubic64/quad64/line_cubic_intersections；含 Halley 法 cbrt）
  painter.rs      公共入口：fill_rect / fill_path / stroke_path / draw_pixmap / apply_mask（735 行）
  scan/           扫描转换 4 个入口：path、path_aa、hairline、hairline_aa
  pipeline/       RasterPipeline：mod.rs（编译+调度）、highp.rs（f32x8）、lowp.rs（u16x16 标量）、blitter.rs
  shaders/        渐变（linear/radial/sweep）、pattern、gradient.rs 共享基类
  wide/           自写 SIMD 包装：f32x4/x8/x16、u16x16、i32/u32 各档
```

关键分工：**scan/* 只负责"算出每行的覆盖率"**（产出 `blit_h(x, y, width)` 或 RLE 的 `blit_anti_h`），
**pipeline/* 只负责"把颜色按覆盖率混上去"**（W1D-031/023）。二者通过 `Blitter` trait 解耦。

### 1.2 其余五者定位对比

| | 路线 | 核心数据结构 | 输出 |
| --- | --- | --- | --- |
| tiny-skia | CPU 扫描线 + 超采样 | 活动边表 + AlphaRuns | RGBA8888 位图 |
| lyon | CPU 细分（三角化），不负责光栅 | 活动边 + Span（扫描线） | 顶点/索引缓冲 |
| femtovg | GPU（GL 3.0+） | fringe 三角带 + 模板缓冲 | GL 绘制命令 |
| vello_cpu / vello_gpu | CPU/GPU 共享 Sparse Strips 前端 | 4x4 Tile + Strip（含 alpha_idx） | 位图 / GPU 命令 |
| pathfinder | GPU（面积 LUT 解析覆盖） | 16x16 Tile + 覆盖帧缓冲 | GPU 绘制 |
| raqote | CPU 扫描线 + 4x4 超采样 | 整形状 alpha mask | RGBA8888 位图 |

## 2. 关键机制

### 2.1 RasterPipeline 的"编译"（tiny-skia）

- stage 表最多 32 个，`ArrayVec` 定容、无堆分配（W1D-005）。`Stage` 是枚举，每个 stage 对应
  一个函数指针（highp 一套、lowp 一套）（W1D-002）。
- 编译产物是**两套**函数指针表：`functions`（整批 8/16 像素）与 `tail_functions`（尾部不足一批）。
  作者未能复现 Skia 的 `load_8888_/store_8888_` 性能，理由归为 "Rust 里 load/store 的分支代价"
  （`src/pipeline/mod.rs:422`/`:424`；W1D-006/007）。
- stage 内**禁止 if/match/循环**（注释原文 "strictly forbidden"），分支被提前到"选哪套函数表"
  （W1D-004）。
- 批宽：highp 8 像素（`src/pipeline/highp.rs:24`）、lowp 16 像素（`src/pipeline/lowp.rs:37`；W1D-037/118）；
  渐变上下文因此**必须补到 16 槽**（W1D-039/040）。

### 2.2 扫描转换与 AA（tiny-skia）

- 非 AA 路径：`BasicEdgeBuilder` 建边 → **按 (first_y, x) 排序**（`src/scan/path.rs:125`；W1D-116）→ 索引链表 +
  首尾哨兵边（`EDGE_HEAD_Y = i32::MIN`，W1D-030）→ `walk_edges` 按 y 推进，用 `winding_mask`
  （EvenOdd=1 / Winding=-1）统一两种填充规则（W1D-031）。排序是每帧每条路径一次 O(n log n)，不可跨帧复用。
- AA 路径：`SUPERSAMPLE_SHIFT = 2`（4x 纵向），`SuperBlitter` 把子扫描线累积进 `AlphaRuns`
  （稀疏 RLE：`runs: Vec<Option<NonZeroU16>>` + `alpha: Vec<u8>`），整行满后 flush 给真 blitter
  （W1D-008/010）。
- 曲线不打散成大量直线：`QuadraticEdge`/`CubicEdge` 用前向差分（qdx/qddx、cdx/cddx/cdddx）
  逐段走，细分数由 `diff_to_shift` 的启发式定，上限 `MAX_COEFF_SHIFT = 6`（≤64 段，受 `curve_count: i8`
  限制）（`src/edge.rs:17`；W1D-020）。
- 裁剪是**先裁再扫**：`EdgeClipper` 对每条边做 X/Y 单调化后裁剪，单边结果上限 `MAX_VERBS = 18`
  （ArrayVec 栈上）（W1D-019）。

### 2.3 覆盖率×混合的正确性分层（tiny-skia）

- `should_pre_scale_coverage()` 白名单 7 种模式（Destination / DestinationOver / Plus / DestinationOut /
  SourceAtop / SourceOver / Xor）可把覆盖率预乘进 RGB；其余（含所有需要源 alpha 项的模式）走
  `LerpU8`/`Lerp1Float` 后合成（`src/blend_mode.rs:81` 注释明确这是 `SkBlendMode_SupportsCoverageAsAlpha()` 的
  细分；W1D-024）。
- 覆盖率到 alpha 的定标：`coverage_to_partial_alpha(aa) = aa << (8 - 2*SHIFT)`（`src/scan/path_aa.rs:284`；
  W1D-111），累积时用 `catch_overflow(alpha) = alpha - (alpha >> 8)` 把 256 折回 255
  （`src/alpha_runs.rs:37`；W1D-112）。
- gamma：非 Linear 的 ColorSpace 会插入 gamma 展开/压缩 stage，**并强制整条管线走 highp**（W1D-025/026）。

### 2.4 裁剪栈的代价模型（tiny-skia）

`Mask` 只有一种表示：与 pixmap 同尺寸的 `Vec<u8>`。`Mask::intersect_path` 的实现是"新建同尺寸
`Mask::new` → `fill_path` → 逐字节预乘"，文档明说会分配临时缓冲（W1D-028/045）。也就是说**裁剪栈每深一层
就多一张全画布缓冲的工作量**，且 `apply_mask`（事后蒙版）被文档明确标为"不如绘制时传 mask 快"（W1D-113）。

### 2.5 描边的两条实现（tiny-skia）

- 变换后宽度 ≥1px：`path.stroke()` 生成描边路径再 `fill_path`（注释自述新路径可能 2-3 倍大；有 dash 时
  还要再生成一条 dash 路径）（W1D-012）。
- 变换后宽度 <1px（或 width==0）：走 hairline，不分配新路径（W1D-013）。
- hairline AA 还有一个"把细描边当调制过的粗线"的近似：覆盖率经 `scale = coverage*256` 预乘进 shader alpha
  （`src/painter.rs:569`/`:367`；W1D-114）。判定细描边用的是**廉价快长度** `|x| + |y|/2 ≤ 1.0`（`src/painter.rs:579`；
  W1D-115）——是刻意的近似判据，不是真长度。

### 2.6 细分与填充（lyon）

- 定位：CPU 三角化，产物是顶点/索引，不做光栅（W1D-063）。
- 扫描线算法（sweep line）：事件队列 + 活动边 + Span，输出填充三角形；`fill.rs:941` 注释"An iteration of
  the sweep line algorithm"（W1D-062）。
- 填充规则：`EvenOdd => winding % 2 != 0`、`NonZero => winding != 0`；**默认是 EvenOdd**
  （W1D-057/054/061）。
- 容差：API 默认 0.1（W1D-053），实现里再取一半 → 0.05（W1D-060）。曲线展平用"闭式单步步长"
  `flattening_step()`（W1D-059/058）。
- 自交处理默认开启（`handle_intersections: true`），关掉是快路径但文档警告可能 panic 或结果错误（W1D-056）。

### 2.7 GPU 侧的三种不同路线

- **femtovg（nanovg 系）**：路径边界生成宽 `fringe_width`（默认 1.0，随 DPI 缩放）的三角带做 AA（W1D-064/065）；
  单凸轮廓走 `ConvexFill`（直接填三角扇），凹/多轮廓写模板缓冲后再覆盖四边形（stencil-then-cover）（W1D-068/069）；
  关闭 AA 时 fringe 宽度取 0（几何级开关，不是混合开关）（W1D-067）。字形缓存 `GlyphAtlas` =
  `HashMap<RenderedGlyphId, Option<RenderedGlyph>>` + 纹理数组（W1D-071）。
- **pathfinder（解析面积覆盖）**：16x16 tile；覆盖率由 `computeCoverage()` 对线段射竖直射线、查面积表
  `areaLUT` 取梯形面积（W1D-091/092/089/096）。两个档位是**能力等级**不是 API：D3D9 = CPU 分箱 + GPU 填充/合成；
  D3D11 = 全 GPU 计算（W1D-093/094）。D3D9 档在 GPU 上用 `StencilFunc::Equal` 做门控（W1D-095）。
- **vello**：主线 Sparse Strips——展平 → 分箱到 4x4 tile → 稀疏水平条带；覆盖率用**解析积分**而非超采样
  （`vello_common/src/strip.rs:589` "the integral of the winding within it"；W1D-076/077/074/075/085）。
  vello_cpu 额外用**CPU 侧深度缓冲**：不透明段前→后画并更新深度，剩余段后→前画但每次都查深度跳过被覆盖的
  命令（W1D-079/080）。原 compute 路线在 `research/`，粗/细分成独立 WGSL（`coarse.wgsl:4`、`fine.wgsl:4`，
  渐变 LUT 宽 512）（W1D-082/083/084/086）。

### 2.8 字形缓存策略（三家对比）

| 库 | 缓存单位 | 键 | 备注 |
| --- | --- | --- | --- |
| tiny-skia | **无**（README 明确"不做任何资源缓存"） | — | 文本必须外部栅格化为 Mask/Pattern（W1D-047/048） |
| femtovg | `GlyphAtlas`（HashMap + 纹理数组，矩形用 bottom-left-fit 线性扫描装箱） | `RenderedGlyphId` | 灰度与彩色字形分两套命令表（`src/text.rs:480`；W1D-071/117/119） |
| vello | 由消费者提供（`glifo` 做字形运行支持） | — | vello_common 不内建字形缓存 |

## 3. 性能手段与公开读数（数字必须带锚）

### 3.1 tiny-skia 的优化清单（按命中频率排序的直觉）

1. 纯色 + Source → `memset` 整行，绕过管线（W1D-022）。
2. SourceOver + 不透明 shader + 无 mask → 强度削减为 Source（W1D-021）。
3. AA 行内按覆盖率分发：0 跳过 / 255 `blit_h` / 其余才跑管线（W1D-023）。
4. pattern 单位阵或平移 → 强制 Nearest；整数平移也归 Nearest（W1D-042）。
5. `fill_rect` 在"单位变换 + 画布 ≤8191"时走专用路径，否则退化为通用路径填充（W1D-107）。
6. AA 矩形走 `scan::hairline_aa::fill_rect`（定点 8 位逐边覆盖），不建边表（W1D-043）。

### 3.2 公开读数（全部带口径，不可外推）

| 读数 | 口径 | 锚 |
| --- | --- | --- |
| tiny-skia 比 Skia 慢 20-100%（x86-64）、100-300%（ARM） | README；无日期标注 | W1D-046 |
| 基准环境：Gentoo/AMD 3700X（x86-64）、Apple M1（ARM）；Skia v90、cairo v1.16.0、Rust 1.62、clang 13 | benches/README | W1D-051 |
| tiny-skia 约 14 KLOC、现代 CPU 编译 <5s、二进制 +约 200KiB | README（约数） | W1D-049 |
| Skia 本体 370 KLOC（含依赖约 7 MLOC）、4-8 GiB 磁盘、3-8 MiB 产物 | README（作者口径，无版本号） | W1D-050 |
| tiny-skia highp 比 Skia 慢"近 2 倍"；作者归因手写 vs clang 向量扩展 | `src/pipeline/highp.rs:14`，未标机器 | W1D-038 |
| 标量 u16x16（靠 LLVM 自动向量化）比 Skia 慢 5-10%；手写 u16x8 慢 30-40% | `src/pipeline/lowp.rs:24`，未标机器 | W1D-036 |
| "手写 SIMD 的 u16x8 比标量还慢" | `src/pipeline/lowp.rs:21` | W1D-035 |
| trait+动态分发实现比函数指针数组慢"至少 20-30%" | `src/pipeline/mod.rs:39` | W1D-003 |
| AA 扫描转换处 TODO：比 Skia 慢 15%，原因未查明 | `src/scan/path_aa.rs:106` | W1D-044 |
| pathfinder：CPU tile+cull+pack 2.2ms、GPU 填充 2.6ms、GPU 着色 2.3ms | **2017 年、1600x1600 Ghostscript tiger、i7-7920HQ、Intel HD 630** | W1D-097 |
| pathfinder 自述等效 256xAA | README（"effectively"为作者口径） | W1D-098 |

### 3.3 结构性的成本事实（不需要跑基准也能读出来的）

- **小图元有固定底价**：两套函数表（front/tail）的存在本身就说明不足一批时走另一条路径（W1D-006/007）。
- **大画布是 O(块数)**：>8191 时每块都要重做路径变换 + shader 变换（`src/painter.rs:246`-`284`；W1D-014/015）。
- **裁剪栈是 O(层数 × 画布面积)**（W1D-027/028/045）。
- **渐变有 16 槽的固定底价**（W1D-039/040）。
- **AA 的几何成本随周长增长**（femtovg 的 fringe 明确如此；W1D-064）——路径越"长"越贵，与面积无关。

## 4. 坑与反例（负面留档）

1. **静默质量降级（tiny-skia）**：路径∩裁剪框 `<<2` 溢出 short 时**无日志**地退化为非 AA
   （`src/scan/path_aa.rs:55`，W1D-108）。同文件对 `clip.right() > 32767` 是直接 `return`（连非 AA 都不画，
   `:64`，W1D-009）。→ 大坐标/大画布场景下"AA 消失"或"整条路径消失"都不报警。
2. **三次曲线裁剪打回直线**：`too_big_for_reliable_float_math` 阈值 2^22，超限就用 p0→p3 直线代替
   （W1D-017/018）。注释同时给出**已知替代方案**："用 double 重写 chop 系列几乎总能安全处理，但速度损失很大，
   所以只在必要时才换实现"（`src/edge_clipper.rs:214`；W1D-124）。
3. **`intersect_path` 的隐藏分配**：文档自己写明会新建同尺寸临时 mask（W1D-045）——裁剪层数多时这是
   O(n) 张全画布缓冲，而不是 O(1) 状态栈。
4. **无全局 alpha**（W1D-052）：README 明说只有 Pattern 能带 opacity。→ "整窗淡出"这类需求在 API 上
   没有直接表达方式。
5. **`fill_rect` 不总是快路径**：条件是"单位变换 + 画布 ≤8191"，带缩放/旋转的矩形会走完整路径光栅
   （W1D-107）。→ 用缩放做动画的 UI 会突然掉出快路径。
6. **lyon 的默认填充规则是 EvenOdd**（W1D-054/061）：与 SVG `fill-rule` 的默认值（nonzero）**不同**；
   直接用 lyon 会静默改变自交形状的填充结果。
7. **lyon 容差的实际值是 API 默认的一半**（W1D-060）：引用"容差 0.1"时必须落到 0.05，否则复算不出一致结果。
8. **raqote 的整形状 alpha mask**：作者自己在 DESIGN.md 里把它列为待改项（"the intention is to switch to
   Skia like run length representation"）（W1D-104）。→ 该实现**只 shade 有覆盖的像素**这条路径当时还没做。
9. **raqote 描边重叠不去重**：PostScript 风格描边器，每段/连接/端帽各成子路径，**不做重叠消除**
   （W1D-105）。→ 半透明描边在自交/连接处会重复叠加，颜色偏深。
10. **"手写 SIMD 必快"是错的——但只对 x86 成立**：作者实测手写 u16x8 SIMD 比标量还慢，最终采用"标量 u16x16 +
    依赖 LLVM 自动向量化"（W1D-035/036）；同一作者同文又指出 **ARM AArch64 上手写 SIMD 快 2-3 倍**
    （`src/pipeline/lowp.rs:28`；W1D-123），且 `-C target-cpu=haswell` 只再提升约 25%、相对同档 Skia 仍落后
    40-60%（`src/pipeline/lowp.rs:25`；W1D-122）。→ **结论按架构分叉，不可跨平台套用。**
11. **vello 的仓库结构已变**：本 commit 下 `vello` 包（compute-centric）已移到 `research/vello_research`，
    而 `vello_cpu`/`vello_gpu` 在根目录。按旧路径（`crates/`、`shaders/`）克隆会失败（本任务实测：
    按 `shaders/`+`crates/` sparse 后目录为空，须改用 `vello_common` 等名字）。（W1D-086）
12. **CJK / 文本全是外部责任**：tiny-skia 完全没有文本（W1D-048），femtovg 依赖外部字形栅格化
    （其 atlas 只缓存已栅格化结果，W1D-071）。→ 青简的文本成本完全落在 cosmic-text/swash 一侧。

## 5. 未验证项（写明缺什么证据）

1. **没有跑任何 benchmark**：纪律禁止 `cargo build/test/bench`。本报告所有"成本"表述都是**结构性的**
   （代码结构决定的必然成本）或**转述原作者口径**，没有一条是本机实测。→ 需要本机可复现的基准才能把
   "青简渲染器该不该用 tiny-skia 的哪条路径"落成数字。
2. **tiny-skia 的 bench 结果页未取**（`https://linebender.github.io/tiny-skia/x86_64.html`）：本机 curl 需
   `--ssl-no-revoke`，HTML 结果表未抓取 → 各子项（fill/stroke/gradients/blend/clip）具体倍数未记录。
3. **青简 × tiny-skia 的实际调用面未核**：本任务只读光栅器一侧，未读 `crates/qingjian-render`（属 A0/A1），
   → "青简画每帧用了哪些 tiny-skia 入口、命中哪些快路径"仍是推断，未验证。
4. **femtovg 的 fringe 三角带宽度与顶点数的关系未量**（只有 `fringe_width` 常量与 DPI 缩放规则；
   `path_cache.expand_fill(fringe_width, ...)` 的顶点增长曲线没读）。
5. **vello 的性能读数未取**：vello 侧只有结构文档，无任何 vello_cpu/vello_gpu 的公开 ms 数字被抓到。
6. **pathfinder 的 D3D9/D3D11 实际分箱吞吐未量**：doc/architecture.md 的 2.2/2.6/2.3 ms 是 2017 年单机
   单场景，本报告未做任何重测，**不可**用来比较 2026 年的任何实现。
7. **lyon 的 Span 生成算法细节未读全**（`monotone.rs`、`event_queue.rs` 未逐行读）→ lyon 侧只覆盖了
   配置面（默认值/容差/填充规则）与展平/扫描线的高层结构。
8. **raqote 的 gradient LUT 宽度与量化误差未核**（DESIGN.md 说"查 LUT、双线性是低精度近似"，但未取具体
   表宽与误差上界）。
9. **字形缓存的驱逐策略全部未核**：femtovg 的 `GlyphAtlas` 只见数据结构，未见淘汰/扩容策略；
   `Atlas`（bottom-left-fit 装箱器）是通用的，但"满了怎么办"没读。
10. **gamma 正确性的像素级验证为零**：tiny-skia 的四种 ColorSpace 与 vello/pathfinder 的覆盖/混合
    gamma 处理都只读了代码路径，没有做任何像素对比。
