# C/C++ 2D 光栅器第二组：Blend2D / ThorVG / NanoVG / Cairo（抓取 2026-10-01）

> 任务 N2（第三轮·补缺）。事实来自 `D:/KF/LSSMJ/scratch/src/<slug>` 的浅克隆，每条主张指到
> `docs/analysis/ledger/w4e.jsonl`（W4E-nnn）。**口径声明**：只覆盖下列 commit；"更快/更慢"一律带原作者
> 机器/版本/场景限定，不升格为普遍结论；A4 批（tiny-skia/lyon/femtovg/vello/pathfinder/raqote）已录结论不重复。

| 目标 | 上游 | commit（全） | 本机克隆 |
| --- | --- | --- | --- |
| Blend2D（本批最高优先） | https://github.com/blend2d/blend2d | `58ca9460b4138af6e793184682e9324e0a8053b2`（版本串 0.21.3） | `scratch/src/blend2d` |
| ThorVG | https://github.com/thorvg/thorvg | `e4594e3c44a9b0f9118e8106924a0d9c0cb42914`（meson 版本 1.2.0） | `scratch/src/thorvg` |
| NanoVG | https://github.com/memononen/nanovg | `ce3bf745eb2d2dbc14a50bf2446783f691ac4353` | `scratch/src/nanovg` |
| Cairo（可选档） | https://gitlab.freedesktop.org/cairo/cairo | `74755964edef651691d5cd36ff140108298e4e6e` | `scratch/src/cairo` |

## TL;DR（每条带锚）

1. Blend2D 是"JIT 编译管线 + 解析光栅 + 条带多线程"三件套：管线按 32-bit 签名缓存，JIT 由 asmjit 生成
   （唯一可选依赖，非 JIT 构建不需要），平台限定 x86/i386/arm64（W4E-002/003/006/118）。
2. 光栅器是 AGG 血统的**解析面积覆盖率**（A8Info `kShift = 8`，1/256 子像素；非超采样），并改造为
   自上而下可切带、稠密 cell 存储（W4E-017/018/024）。
3. 合成器靠**1 bit / 4 像素的非零比特向量**整段跳过数百像素的零覆盖区（`BL_PIPE_PIXELS_PER_ONE_BIT = 4`，
   作者实测 [4,8,16,32] 中 4 最稳）（W4E-019/020/022/023）。
4. 线程模型=条带并行：带高 64 起按 256KiB cell 缓冲与"带数 ≥ 线程数"两个条件折半，下限 8 行；
   带在 worker 间跨步交错分配（W4E-007/008/009/010/015）。
5. 反例留档：Blend2D 自测"一个 worker 连吃多条带"在 4+ 线程回退 bl_bench（注释停用该功能，W4E-014）；
   隔离线程池/隔离 JIT runtime 均被注释劝阻用于生产（W4E-041/042）。
6. ThorVG CPU 光栅=FreeType 血统的 RLE 覆盖率，`PIXEL_BITS = 8`；AA 开关落在
   覆盖率生成处（关则 `coverage = 255`），下游合成不分叉（W4E-059/060/061）。
7. ThorVG 的线程粒度=**每形状/图像一个任务**（非单形状切带），worker 用 thread_local mempool 工作区，
   线程数由应用显式传入；线程支持是编译选项（W4E-062/063/064/066）。
8. ThorVG 把局部渲染做成一等公民：`EngineOption::SmartRender` + 16×16 分区脏区表（旧+新包围盒双记），
   preRender 只清脏矩形；README 亦自述全屏级动态内容下收益趋零（W4E-068/070/071/072/073/057）。
9. NanoVG（已停维护）是 GL 几何 AA 路线的极简样本：fringe=1/ratio、展平容差 0.25/ratio、深度上限 10、
   细线用 alpha² 模拟覆盖率；无 CPU 光栅/JIT（对照面），文本仅 fontstash/stb_truetype（W4E-083/089/090/091/093/101）。
10. Cairo（经典 CPU 光栅）把图像合成整体委托 pixman：trapezoid 扫描转换（botor，cell 覆盖率累加）+
    `pixman_image_composite32`/`pixman_fill`；容差默认 0.1、AA 档 DEFAULT/NONE/GRAY/SUBPIXEL（W4E-105..115）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 1 bit / 4px 非零摘要 + 大跨步跳过零覆盖 | W4E-019/020/022 | 吸收（LSSMJ §4.1 的"0 覆盖跳过"在整行尺度的现成实现） |
| 解析面积覆盖率（kShift=8）作 AA 二期备选 | W4E-024 | 有界吸收（质量上限高于 2-bit 超采样；需重写覆盖率来源，工程量另算） |
| 条带工作集按 256KiB 折半 + 带数≥线程数 | W4E-009/010 | 吸收（后台 damage 栅格化的划分规则可直接照抄，上限值待本机实测） |
| 命令按带 y 量化（对数位宽） | W4E-011 | 吸收（按带粗筛命令的零成本空间索引） |
| 渐进式质量档：渐变 LUT 256/512/1024 + 高质量翻倍封顶 | W4E-030/031 | 吸收（LSSMJ 渐变缓存按 stop 特征定档；翻倍要有硬上界） |
| DITHER 档（16×16 Bayer，布局按 16B 装载设计）+ Pattern 两档 | W4E-033/034/035/046 | 有界吸收（Bayer 只对大面积低色深渐变；Pattern=图标缩放最小档） |
| thread_count 三态（0 同步/1 用户线程/>1 取 n-1）+ 回退同步旗标 | W4E-037/038/040 | 吸收（默认=0 与青简单线程出帧兼容；回退必须可测） |
| DPI 绑定容差/带宽（tessTol、fringe=1/ratio） | W4E-089/090 | 吸收（高清屏正确性前提；LSSMJ 展平容差同样除以缩放） |
| 局部渲染选项（SmartRender 可关）+ 16×16 分区 + 脏区双包围盒（旧+新，x 有序早停） | W4E-068/070/072/074 | 吸收（LSSMJ §4.3 的 damage 带总开关；分区表与双包围盒为现成起点） |
| 每线程工作区（thread_local pool / WorkData） | W4E-049/063 | 吸收（后台栅格化免锁的具体形态） |
| "不透明色单开一条 SolidRle 路" + 覆盖率 255 二分 | W4E-075/076 | 吸收（与 w1d 的 0/255/中间三分支同构，且分派顺序即优先级） |
| 预乘内部统一、输出端一次性反预乘 | W4E-078/079 | 吸收（与青简壳的预乘 BGRA 契约同构，转换点唯一） |
| 细线 alpha² 模拟覆盖率（NanoVG 原始措辞） | W4E-093/094 | 有界吸收（w1d 已从 femtovg 侧录同族做法；此处补出处与定标理由） |
| 全局 JIT 管线缓存（函数签名→编译产物） | W4E-003/027/046 | 不吸收（LSSMJ 无 JIT；但"签名→派发"的键设计可借给 stage 分档） |
| CPU 特性的 JIT 期特化（SSE2..AVX512 生成不同机器码） | W4E-043/044 | 不吸收（Rust 路线用 cfg(target_feature) 分档即可，见 §2.4） |
| Blend2D 的"无 gamma/线性空间" | W4E-048 | 不吸收（默认与青简同；线性混合未在本批找到收益证据，先不做） |
| Cairo 的 pixman 委托 + trapezoid 中间表示 | W4E-108..112 | 不吸收（C 库绑定与抽象成本；LSSMJ 自绘光栅不引第二套像素引擎） |
| GL/stencil 路线整体（NanoVG/femtovg 族） | W4E-084/088/098/099 | 不吸收（本批为 CPU 默认路径服务；GPU 档另批已录） |
| 逐形状 AA 两级开关（全局×图元） | W4E-097 | 有界吸收（LSSMJ 候选窗暂只需全局档；字符级枚举出否决清单时再引入） |

## 1. 架构全景（模块地图）

### 1.1 Blend2D（`blend2d/` 一个 ICU 级大目录）

```
core/        公共对象：context/image/path/gradient/pattern/runtime（BL_VERSION 0.21.3 在 core/api.h:540）
pipeline/    管线定义与两种实现：reference/（C++ 参考实现 + fixedpiperuntime 的固定 SIMD 集）
             jit/（asmjit 生成：pipecompiler/pipecomposer + fetch*/compop*/fill* 各"部件"）
raster/      渲染上下文实现：analyticrasterizer（解析光栅）、edgebuilder、worker（条带并行）、
             rendercommand/renderjob（命令与作业队列）、workermanager（池与同步）、styledata（共享状态）
simd/        SIMD 抽象（simdx86_p.h / simdarm_p.h，覆盖 SSE2–AVX512、ASIMD）
threading/   原子/futex/线程池（bl_thread_pool_global 全局池单例）
opentype/    字体解析与布局（otlayout 布局、otkern 字距；字形轮廓提取有 AVX2/SSE4.2/ASIMD 变体，W4E-120/121）
其余/        codec/、compression/、unicode/、support/、tables/、geometry/（本批未逐文件读）
```

关键分工：**光栅器只产出覆盖率**（稠密 cell + 比特向量），**管线（JIT 生成）只负责取色+混合+回填**，
两者以"签名"为键在 PipeRuntime 相遇（W4E-003/017）。

### 1.2 ThorVG（`src/` 分层）

```
renderer/        渲染器外壳：tvgCanvas（SwCanvas/GlCanvas gen(EngineOption)）、tvgTaskScheduler（可选线程池）、
                 tvgInitializer（线程数入口）、tvgRender（RenderPath/RenderShape/RenderDirtyRegion）
renderer/cpu_engine/  CPU 光栅：tvgSwRle（FreeType 血统 RLE 覆盖）、tvgSwRaster（四路分派+AVX/NEON 变体）、
                 tvgSwBlendOp（SVG 混合模式）、tvgSwMemPool（thread_local 工作区）、tvgSwRenderer（任务与脏区）
renderer/gpu_engine/  GL/WebGPU 后端；common/、loaders/（SVG/Lottie/图片）、savers/、bindings/
```

### 1.3 NanoVG / Cairo（对照面）

- NanoVG：`src/nanovg.c`（路径缓存/展平/三角形化 + fontstash 文本）+ `src/nanovg_gl.h`（GL2/GL3/GLES 实现，
  三种绘制调用 fill/convexFill/stroke）。无独立光栅器——**光栅全在 GPU 的模板/三角形里**（W4E-084/098/099）。
- Cairo：`src/` 单层大目录，图像后端=`cairo-image-surface.c`+`cairo-image-compositor.c`（整体委托 pixman，
  `pixman_image_composite32`/`pixman_fill`/`pixman_blt`），路径→梯形（trapezoid）扫描转换在
  `cairo-botor-scan-converter.c`（cell 覆盖率链）（W4E-108..112）。

## 2. 关键机制（按任务六问逐条）

### 2.1 ①光栅算法

- **Blend2D＝解析覆盖率（AGG 改进版）**：点坐标 1/256 子像素（`A8Info { kShift = 8 }`），覆盖率按面积积分；
  稠密 cell 数组 + 每扫描线固定宽比特向量（1 bit=4 px 非零摘要）；自上而下推进以支持条带（W4E-017/018/019/024/022）。
  `fp_scale=256` 在 worker 侧被硬编码（TODO 标注）（W4E-016）。
- **ThorVG＝RLE 覆盖率（FreeType 血统）**：`PIXEL_BITS = 8`；cell 面积→覆盖率（`area/(256*256*2)`）；
  EvenOdd 取 `coverage &= 511` 折返、NonZero 饱和 255；span 前向合并（同色同 y 且 x 连续）（W4E-059/060/061/122）。
  细节：HYPOT 用 `alpha max plus beta min`（α=1、β=3/8，注释自报误差 <7%）近似平方根（W4E-104）。
- **NanoVG＝不做 CPU 光栅**：递归展平（深度≤10、flatness 判据去开方）后交 GL；AA 靠几何 fringe 带
  （宽=1/ratio）+ 模板（EQUAL 0 只画边带外）（W4E-091/092/090/098）。
- **Cairo＝trapezoid 扫描转换 + pixman**：botor 转换器（cell 链累加覆盖，面积制 `AREA_TO_ALPHA`）产出
  梯形后，由 pixman 按格式合成（W4E-111/112；合成委托见 W4E-108..110）。

### 2.2 ②混合与 color management

- **Blend2D：无 gamma / 无线性空间**——全仓 gamma 字样仅命中 codec（BMP 头字段），管线里没有展开/压缩
  stage（W4E-048）；混合在像素编码空间直接做。合成算子有专门的空操作算子 `kAlwaysNop`（W4E-045）
  与实色覆盖查表（W4E-117）。
- **ThorVG：内部统一预乘**（`surface->premultiplied = true`），输出到直通空间才一次性反预乘；
  SVG 混合模式（multiply/screen 等）在直通空间计算（反预乘→公式→再预乘）（W4E-078/079/080）。
- **NanoVG**：GL 状态混合（预乘约定由 `NVG_IMAGE_PREMULTIPLIED` 等旗标管理，W4E-084）；色彩处理本批未核。
- **Cairo**：图像合成（含 AA 覆盖）整体委托 pixman（W4E-108/109）；其色彩空间/混合细节在 pixman 内部，
  本批未读，未给结论（见 §5）。

### 2.3 ③线程模型

- **Blend2D＝条带并行 + 异步命令队列**：上下文 attach 时按 (画布宽, 线程数) 定带高（64 起，受 256KiB
  cell 上限与"带数≥线程数"约束折半，下限 8）；worker 跨步交错领带（consecutive=1）；命令/作业/FetchData
  分池预分配；取到池线程才进 kAsync，否则（带旗标）回退同步（W4E-007/008/009/010/013/015/047/049/050）。
  反例：连吃多带在 4+ 线程回退（W4E-014）。
- **ThorVG＝任务并行（形状粒度）**：`TaskScheduler::request(task)` 每形状/图像一个任务；任务按轮转入各线程
  队列、worker 邻队探测取活；RLE 生成在任务内串行、每线程 thread_local mempool；线程数是显式入参且
  线程支持为编译选项（W4E-062/063/064/065/066/058）。
- **NanoVG**：无线程（GL 上下文单线程语义）。
- **Cairo**：全局共享缓存各有具名互斥（字体映射/字形缓存/实色表面缓存等，W4E-113）——即"库线程安全、
  绘制上下文不加锁"的经典契约；"cairo_t 不跨线程"的官方文档逐字锚本批未取到（见 §5）。

### 2.4 ④JIT / SIMD 的启用条件与实测主张

- **Blend2D JIT**：默认全局 `PipeDynamicRuntime::_global`；`BL_BUILD_NO_JIT` 在非 x86_64/i386/arm64 目标
  自动生效；可被上下文旗标关闭；隔离 JIT runtime 会丢全局缓存（注释劝阻生产）（W4E-006/039/041/042/046）。
  管线缓存：进程级函数表（签名 32-bit：dst 4b/src 4b/comp 6b/fill 2b/fetch 5b/flag 1b，W4E-116），并发首次编译有
  竞态去重路径；本地查找缓存 x86 N=16（"SSE2 friendly"），SIMD 并行比较签名（W4E-027/004/005/028）。
- **Blend2D SIMD**：运行期 CPU 特性位（SSE2..AVX512、ASIMD/CRC32/PMULL）；JIT 合成按覆盖率批量选位宽
  （4 覆盖=128-bit，≥8=256-bit）（W4E-043/044/025）。
- **ThorVG SIMD**：构建期选项（AVX2/NEON 二选一），实现为 cRaster*/avxRaster*/neonRaster* 替换函数族，
  **无运行期分派**（W4E-081）。
- **NanoVG/Cairo**：无 JIT；NanoVG 无 SIMD（GL 顶点路径）；Cairo 委托 pixman（其 SIMD 属 pixman 仓，本批未取）。
- 实测定性主张（带口径）：ThorVG README 自报 CPU 基准对"某广泛使用的矢量引擎"平均约 2.9×（条件：
  5k 半透明图元、Apple M2 Pro/macOS 26、2560×1440、ThorVG v1.1 vs Skia v148；本仓版本串是 1.2.0）
  （W4E-052/053/054/055）。

### 2.5 ⑤质量 / 性能旋钮

| 库 | 旋钮 | 取值/默认 | 锚 |
| --- | --- | --- | --- |
| Blend2D | 渲染质量 | 仅 ANTIALIAS（MAX_VALUE=0，本 commit 无全局关 AA） | W4E-036 |
| Blend2D | 渐变质量 | NEAREST / SMOOTH（注释：当前永不可用）/ DITHER | W4E-032/033 |
| Blend2D | 渐变 LUT | 256/512/1024 分档；高质量档翻倍封顶 1024 | W4E-030/031 |
| Blend2D | 图案质量 | NEAREST/BILINEAR（默认 BILINEAR） | W4E-035/046 |
| Blend2D | 上下文提示 | hints 三档（rendering/gradient/pattern quality） | W4E-037 |
| Blend2D | 线程 | 0=同步 / 1=异步(用户线程) / n=取 n-1 池线程；FALLBACK_TO_SYNC | W4E-037/038/040 |
| ThorVG | 引擎档 | Default / SmartRender（局部渲染）/ Aliased（关 AA，实验性） | W4E-068/069 |
| ThorVG | 形状级 | `antialiasing()`：细线/带 dash/半透明描边强制 AA 例外清单 | W4E-082 |
| NanoVG | 上下文旗标 | NVG_ANTIALIAS（几何 AA）/ NVG_STENCIL_STROKES（描边质量） | W4E-085/086/087 |
| NanoVG | 图元级 | `nvgShapeAntiAlias` 与全局相乘 | W4E-097 |
| Cairo | AA 档 | DEFAULT/NONE/GRAY/SUBPIXEL（另有 FAST/GOOD/BEST 提示） | W4E-107 |
| Cairo | 容差 | `cairo_set_tolerance`，默认 0.1（"converting paths into trapezoids"） | W4E-105/106 |
| Cairo | 字形缓存 | 512 页 × 32 字形/页（淘汰单位=整页） | W4E-114/115 |

### 2.6 ⑥对 LSSMJ §4.1 光栅内核的具体可借鉴项

1. **覆盖率摘要位图**：1 bit/4px 的非零摘要 + "整段跳数百像素"（W4E-019/020/022/023）。LSSMJ 的
   `lssmj-raster` 可在扫描线产物上加一层同粒度摘要，合成循环以字为单位跳过零区——这是 §4.1"0 覆盖跳过"
   在行内的推广，与 tiny-skia 的 run 级跳过（w1d W1D-010/023）互补。
2. **条带工作集公式**：带高 = f(画布宽, 256KiB cell 上限, 线程数)（W4E-007..010）。LSSMJ 的 damage
   后台栅格化按同式定 tile 行高；"带数≥线程数"作为硬约束写进配置校验。
3. **命令 y 量化索引**（W4E-011）：DisplayList → 按带分桶的粗索引，零拷贝、每带扫描列表变短。
4. **质量档三件套**：渐变 LUT 分档（W4E-030/031）、pattern 两档（W4E-035）、AA 档落点=覆盖率来源处
   （W4E-061）；全部要有硬上界与"未实现档"的诚实标注（W4E-032）。
5. **预乘契约**：内部统一预乘、输出端一次性反预乘（W4E-078/079）——与青简壳契约（w5a W5A-093/094）同构，
   直接作为 §4.4 的合成层纪律。
6. **线程三态与回退**（W4E-037/038/040/041）：默认同步（对齐青简单线程），大面板再开异步；回退路径
   必须旗标化、可测；全局池/全局缓存不进 per-context 隔离档。
7. **不吸收项及理由**：JIT（Rust 无等价收益面，W4E-003/027）、线性空间混合（无收益证据，W4E-048）、
   pixman 委托（引第二套像素引擎，W4E-104）、GL 几何 AA（非 CPU 路径，W4E-084/098）。

## 3. 性能手段与公开读数（数字必须带锚）

### 3.1 结构性手段（读代码可得，非实测）

- Blend2D：签名缓存 + 本地 16 槽 + SIMD 查签名；PipeProvider 复制函数指针消除虚调用（W4E-004/005/028）；
  实色覆盖表与 kAlwaysNop 在派发期化简（W4E-045/117）；状态/取数池预分配（W4E-050）。
- ThorVG：RLE 四路分派（合成蒙版→混合→不透明→半透明），不透明色走 `_rasterSolidRle`、其内再按
  覆盖率 255 二分（W4E-075/076）；半透明路径"先混后盖"（W4E-077）；AVX2/NEON 替换最内层循环（W4E-081）。
- NanoVG：凸路径直接 triangle fan，凹/多轮廓才用 stencil-then-cover（并发 AA 边带）；细线 alpha² 近似
  省掉真实覆盖率（W4E-093/094）；三角数/批数计数器常驻（W4E-096）。
- Cairo：字形缓存按 512 页×32 字形分页淘汰（W4E-114/115）；拷贝/填充全委 pixman
  （`pixman_image_composite32`/`pixman_fill`，W4E-109）——cairo 自身不含逐像素循环。

### 3.2 公开读数（全部带口径，不可外推）

| 读数 | 口径（版本/机器/场景） | 锚 |
| --- | --- | --- |
| ThorVG CPU 平均约 2.9×于"某广泛使用引擎" | 5k 半透明图元；Apple M2 Pro (macOS 26)；2560×1440；ThorVG v1.1 vs Skia v148（README 自报） | W4E-052/053/054/055 |
| ThorVG WebGPU 平均约 1.8×于其 GL 后端 | 内部基准；同页 NOTE 声明基准机 Apple M1（非 CPU 结论） | W4E-119 |
| ThorVG 线程词条 | "task scheduler…其使用是可选的"（无数字） | W4E-056 |
| Blend2D | README 无性能数字；本批仅结构事实 | W4E-002 |

**引用纪律**：以上两条 ThorVG 倍数是作者自报、单平台、且读数版本(1.1)与克隆版本(1.2.0)不同——只可作
旁证，不得写入 LSSMJ 的判据表。

## 4. 坑与反例（负面留档）

1. **注释≠代码**：Blend2D 条带注释写"decrease to 16"，实际 `kMinBandHeight = 8`（W4E-007/008）。
2. **"优化"回退**：Blend2D 连吃多带在 4+ 线程回退 bl_bench，功能被 TODO 停用（W4E-014）——并行划分
   改动必须实测。
3. **隔离档陷阱**：隔离线程池（每次建毁全池）与隔离 JIT runtime（丢全局缓存）都被注释劝阻生产
   （W4E-041/042）。
4. **API 表面比实现宽**：Blend2D SMOOTH 档"currently never available"（W4E-032）；ThorVG Aliased 标注
   Experimental（W4E-069）——引用能力面必须先看实现/标注。
5. **缓冲复用契约**：Blend2D 稠密 cell 必须零初始化且由合成器负责清零复用（W4E-021）——复用缓冲的
   清零责任不写清就是脏覆盖率事故。
6. **damage 的边界**：ThorVG 自述全屏级动态内容下局部渲染收益趋零甚至略增开销（W4E-057）——LSSMJ
   判据 C3 需并列"全屏变化"对照场景。
7. **GL 路线宿主约束**：NanoVG 要求渲染目标带模板缓冲（W4E-088）；细线质量例外清单是刻意的
   （W4E-082/093）——近似与例外都要写在档位定义处。
8. **NanoVG 已停维护**（README 首行，W4E-083）——只可作对照不可作依赖。
9. **"自带整形"是进行时**：Blend2D OpenType 布局模块源码自称未完成（TODO + 诊断抑制，W4E-121）——引用
   其文本能力面必须带此限定。

## 5. 未验证项（写明缺什么证据）

1. **未跑任何基准**（纪律：只读/不编译）：除 ThorVG README 自报数字外，本报告无本机实测；所有"成本"
   表述均为结构性事实。
2. **Cairo 的"cairo_t 不跨线程"缺逐字锚**：本批取到的是互斥清单（全局缓存加锁，W4E-113）这一结构证据；
   cairo.h/cairo.c/doc/*.md 内均无 thread 字样——官方 FAQ 的表述在仓库外，未取。引用时按"结构证据"口径。
3. **Cairo 的色彩/线性空间处理未核**：pixman 内部（sRGB 非线性和混合、覆盖合成公式）未读，未取到
   行锚；本报告未给结论。
4. **Blend2D 的 JIT 收益量级未核**：本批未取到"JIT vs 静态管线"的公开数字；DISABLE_JIT 档的实际差距
   无锚（须等官方基准页或自行实测）。
5. **Cairo 克隆状态**：工作树恢复（`git restore --source=HEAD :/`）已于 2026-10-01 完成，W4E-105..115 的
   行锚按恢复后的工作树复读（此前部分行号经 `git show HEAD:<path>` 预读，两者一致）。
6. **ThorVG 的 AVX2/NEON 增益未核**：构建期向量化档没有公开倍数；代码只给出替换函数族，收益待测。
7. **NanoVG 计数器（fillTriCount 等）与真实批次的对应未核**：只读到累加点，未见消费者。
8. **各库 CJK/文本能力面只到"有无整形栈"层级**：Blend2D 自带 opentype 目录（未逐文件读）、ThorVG 文本
   任务、NanoVG=fontstash+stb_truetype（W4E-101）——与 LSSMJ 文本栈（w1b）的对照留到文本批。
