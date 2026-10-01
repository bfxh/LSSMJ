# 11 · 论文/来源目录学（机器生成）

- 生成日期：2026-10-01；数据源：`../analysis/ledger/*.jsonl`（逐条经 `tools/ledger.py verify` 校验）
- 规模：paper 条目 **224**（覆盖 59 个目标）· doc 条目 **643** · web 7；总条目 2420

## A. paper 轨道（按目标；锚=可点开核对的来源）

| 目标 | 条数 | 代表锚 | 一句话（取该目标首条 finding） |
| --- | --- | --- | --- |
| cassowary-tochi | 15 | https://constraints.cs.washington.edu/solvers/cassowary-tochi.pdf | TOCHI 全文：真实 UI 约束集常是环的（同时含等式与不等式），并非教科书里的无环网络。 |
| uax50 | 10 | https://www.unicode.org/reports/tr50/ | UAX#50 的竖排默认朝向：汉字/假名/谚文/缩写直立，拉丁词句侧倒——复杂文种朝向右一个属性说了算 |
| 2d-gpu | 10 | https://raphlinus.github.io/rust/graphics/gpu/2019/05/08/modern-2d.html | Levien(2019)研究结论：依赖现代 compute 能力时，2D 渲染直接做在 GPU 上可行且质量/性能都 |
| adapton | 10 | https://mhicks.me/papers/adapton-submit.pdf | Adapton PLDI 2014 摘要级数字：传统 IC 相对全量重算快 2–20×，Adapton 快 7–20 |
| valve-sdf-2007 | 8 | https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf | Green 2007（Valve, SIGGRAPH 2007）提出：把高分辨率字形图转成距离场存进低分辨率纹理通道 |
| lengyel-slug-jcgt | 8 | https://jcgt.org/published/0006/02/02/paper-lowres.pdf | Lengyel 2017（JCGT 6(2)，Slug 算法）直接在 GPU 上从轮廓数据渲染抗锯齿文本——无预计算 |
| gpu-driven | 8 | https://advances.realtimerendering.com/s2015/index.html | Ubisoft（Haar & Aaltonen, SIGGRAPH 2015）摘要：目标是“无 CPU 干预渲染高密 |
| dd-cidr | 8 | https://www.cidrdb.org/cidr2013/Papers/CIDR13_Paper111.pdf | Differential Dataflow（CIDR 2013）主张：差分计算推广增量计算，尤其适合迭代算法。 |
| oit | 7 | https://jcgt.org/published/0002/02/09/paper.pdf | McGuire & Bavoil（JCGT 2(2), 2013）Weighted Blended OIT：改写合成 |
| carmack-latency | 7 | https://raw.githubusercontent.com/QianMo/Real-Time-Rendering-4th-Bibliography-Collection/master/Chapter%201-24/%5B0228%5D%20%5BBlog%202013%5D%20Latency%20Mitigation%20Strategies.html | Carmack《Latency Mitigation Strategies》阈值口径：绝对延迟约 <20ms 人一般 |
| jlreq-github | 6 | https://w3c.github.io/jlreq/ | JLREQ 定义禁则处理（kinsokushori）：行头/行末禁则、分割禁止等的总称 |
| nanite | 6 | https://advances.realtimerendering.com/s2021/index.html | Karis/Stubbe 2021（SIGGRAPH Advances）讲座摘要：Nanite 让万亿三角形场景实时 |
| emersion-damage | 6 | https://emersion.fr/blog/2019/intro-to-damage-tracking/ | damage 追踪的第 0 级=注意到没有变化就停画（wlroots 作者、damage tracking 实现者的 |
| dobbie-vectortex | 5 | https://wdobbie.com/post/gpu-text-rendering-with-vector-textures/ | Dobbie 长文把思路从「CPU 预生成位图」翻转为「GPU 直接光栅矢量数据」——与 Slug 同族的前置论证 |
| uax9 | 5 | https://www.unicode.org/reports/tr9/ | UAX#9（本次抓取版本 Unicode 18.0.0，2026-09-01）定义内存序=逻辑序，显示序由算法推导 |
| johnnovak-gamma | 5 | https://blog.johnnovak.net/2016/09/21/what-every-coder-should-know-about-gamma/ | Novak 长文（2016-09-21）：显示系统标准 gamma≈2.2，且约等于人眼幂律灵敏度 |
| rectbinpack-pdf | 5 | https://raw.githubusercontent.com/juj/RectangleBinPack/master/RectangleBinPack.pdf | Jylänki《A Thousand Ways to Pack the Bin》：以可复现实测比较多种 2D 装箱启 |
| mesh-shading | 5 | https://www.khronos.org/blog/mesh-shading-for-vulkan | Khronos 官方（VK_EXT_mesh_shader）：mesh/task shader 走 compute  |
| vector-graphics | 5 | https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-25-rendering-vector-art-gpu | GPU Gems 3 第 25 章（Loop/Blinn 方案章节）：GPU 擅长三角形，把曲线路径转成“凸包三角  |
| uax11 | 4 | https://www.unicode.org/reports/tr11/ | UAX#11（抓取版本 Unicode 18.0.0，2026-07-31）引入「固有宽度」规范属性 |
| rastertragedy-ch1 | 4 | http://rastertragedy.com/RTRCh1.htm | Beat Stamm《The Raster Tragedy at Low-Resolution Revisited》 |
| rastertragedy-ch2 | 4 | http://rastertragedy.com/RTRCh2.htm | 作者把 ClearType/CoolType/FreeType/Quartz 归为一个通用类：亚像素抗锯齿（差异只在 |
| raph-text-layout | 4 | https://raphlinus.github.io/text/2020/10/26/text-layout.html | Levien（2020-10-26）给的文本层分层：段→样式/BiDi→字体覆盖 itemization→scrip |
| cassowary-uist97 | 4 | https://constraints.cs.washington.edu/solvers/uist97.pdf | UIST'97（Cassowary 前身）：线性等式/不等式约束在 UI 中天然出现（窗在窗左、占 1/3 宽度等） |
| kp-similarity | 4 | https://api.semanticscholar.org/graph/v1/paper/DOI:10.1145/3685650.3685666?fields=title,year,venue,abstract,openAccessPdf,externalIds | 2024 论文摘要：相邻行首尾同词/同字序属排版缺陷（相似性问题）——KP 扩展可自动检测与规避。 |
| muratori-imgui-2005 | 4 | https://caseymuratori.com/blog_0001 | Muratori 自述：2002 年为 Granny 3D 查看器写 GUI 时实验即时式设计、2002 秋在私信列 |
| nical-etagere | 3 | https://nical.github.io/posts/etagere.html | WebRender 图集分配器从 guillotine 换成定长 2 幂方 slab 分配器（nical 长文） |
| nvpr | 3 | https://api.crossref.org/works/10.1145/2366145.2366191 | Kilgard & Bolz（ACM TOG 31(6):1-10, 2012-11，DOI 10.1145/236 |
| uax14 | 3 | https://www.unicode.org/reports/tr14/ | UAX#14 对 CJK 断行类的注记：按 ID 类处理即得 CSS normal 行为 |
| cassowary | 3 | https://constraints.cs.washington.edu/cassowary/ | Cassowary：增量式线性约束求解工具箱（WPF/Enaml 等布局系统的算法源头） |
| clreq | 3 | https://w3c.github.io/clreq/ | 中文排版的基本度量：汉字与标点同为 1:1 正方形 |
| text-input-v3 | 3 | https://raw.githubusercontent.com/wayland-mirror/wayland-protocols/main/unstable/text-input/text-input-unstable-v3.xml | Wayland 文本输入协议：客户端经 preedit_string/commit_string 事件收文本 |
| gtk4-gsk | 2 | https://blog.gtk.org/2024/04/17/graphics-offload-revisited | GTK 4.14 引入新的 GSK 渲染器（GL/Vulkan），并支持 dmabuf 与 graphics off |
| overdraw | 2 | https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-23-high-speed-screen-particles | GPU Gems 3 第 23 章：全屏粒子特效的 overdraw 几乎无界，帧率问题源于填充率。 |
| performance-method | 2 | https://developer.nvidia.com/gpugems/gpugems/part-v-performance-and-practicalities/chapter-28-graphics-pipeline-performance | GPU Gems 1 第 28 章的方法论：逐阶段变化负载或频率定位瓶颈，再削减该阶段负载，循环直至达标。 |
| occlusion | 2 | https://developer.nvidia.com/gpugems/gpugems/part-v-performance-and-practicalities/chapter-29-efficient-occlusion-culling | GPU Gems 1 第 29 章：遮挡查询与 early-z 两条路都要求“对象从前到后排序”才有效。 |
| compositor | 2 | https://raphlinus.github.io/ui/graphics/2020/09/13/compositor-is-evil.html | Levien 历史考证：早期机器（即便 Atari 2600）直写 framebuffer，延迟极低——引入合成器后 |
| frame-pacing | 2 | https://raphlinus.github.io/ui/graphics/gpu/2021/10/22/swapchain-frame-pacing.html | Levien《Swapchains and frame pacing》：图形性能是吞吐/延迟/功耗的三角权衡，swa |
| pathfinder | 2 | https://pcwalton.github.io/_posts/2017-02-14-pathfinder.html | pcwalton(2017)：Pathfinder——GPU 字形光栅器，目标是任意字号下的速度与内存受控。 |
| rive | 2 | https://rive.app/blog/rive-renderer-now-open-source-and-available-on-all-platforms | Rive 官方（2023 开源公告）：自研渲染器把抗锯齿矢量路径归约为唯一三角形补丁，交给 GPU 的三角形光栅器。 |
| emersion-loop | 2 | https://emersion.fr/blog/2018/wayland-rendering-loop/ | Wayland 帧回调哲学：由合成器告知何时画（客户端不再猜测最小化/遮挡/缩略图等状态）。 |
| knuth-plass | 2 | https://doi.org/10.1002/spe.4380111102 | Knuth–Plass 1981（Software: Practice and Experience, 1981-1 |
| loop-blinn | 2 | https://doi.org/10.1145/1073204.1073303 | Loop–Blinn 2005（ACM Transactions on Graphics, 2005-07）：可编程 |
| uax29 | 2 | https://www.unicode.org/reports/tr29/ | UAX#29 定义字素簇边界（文本导航/编辑的最小单位） |
| hidden-surface | 1 | https://api.crossref.org/works/10.1145/166117.166147 | Greene, Kass & Miller 1993（SIGGRAPH '93, pp.231-238, DOI 1 |
| antialiasing | 1 | https://api.crossref.org/works?query.bibliographic=morphological+antialiasing&rows=3 | Reshetov 2009（HPG 2009, pp.109-116, DOI 10.1145/1572769.15 |
| rasterization-rules | 1 | https://pcwalton.github.io/_posts/2018-02-14-determining-triangle-geometry-in-fragment-shaders.html | pcwalton(2018)：在片元着色器里推三角形顶点位置的常规建议（几何/细分着色器传递）并非唯一解——可用非透 |
| vello | 1 | https://vello.dev/ | Vello 项目现状（2026 抓取）：拆出 vello_cpu（多线程/SIMD CPU 渲染器）与 GPU 版并 |
| popl02 | 1 | https://api.crossref.org/works/10.1145/503272.503296 | Crossref 记录（DOI 10.1145/503272.503296）：Acar/Blelloch/Harpe |
| popl08 | 1 | https://api.crossref.org/works/10.1145/1328438.1328476 | Crossref 记录：Acar/Ahmed/Blume《Imperative self-adjusting com |
| icn | 1 | https://api.crossref.org/works/10.1145/2858965.2814305 | Crossref 记录：Hammer 等《Incremental computation with names》OO |
| naiad | 1 | https://api.crossref.org/works/10.1145/2517349.2522738 | Crossref 记录：Naiad（timely/differential dataflow 的实现原型）SOSP  |
| timely | 1 | https://api.crossref.org/works/10.1145/2983551 | Crossref 记录：Murray 等 CACM 2016《Incremental, iterative data |
| dbtoster | 1 | https://api.crossref.org/works/10.14778/2336664.2336670 | Crossref 记录：DBToaster（高阶 delta 处理）VLDB 2012，DOI 10.14778/2 |
| kp-revisited | 1 | https://api.semanticscholar.org/graph/v1/paper/DOI:10.1145/2682571.2797091?fields=title,year,venue,abstract,openAccessPdf,externalIds | S2 记录：Hassan & Hunter《Knuth-Plass Revisited: Flexible Line |
| wilber | 1 | https://api.semanticscholar.org/graph/v1/paper/DOI:10.1016/0196-6774(88)90032-6?fields=title,year,venue,abstract,openAccessPdf,externalIds | S2 记录：Wilber《The Concave Least-Weight Subsequence Problem  |
| hirschberg | 1 | https://api.crossref.org/works/10.1137/0216043 | Crossref 记录：Hirschberg & Larmore《The Least Weight Subseque |
| skyblue | 1 | https://api.crossref.org/works/10.1145/192426.192485 | Crossref 记录：Sannella《Skyblue》（UIST 1994，DOI 10.1145/192426 |
| constraint-hierarchies | 1 | https://api.crossref.org/works?query.bibliographic=constraint+hierarchies+Borning&rows=4 | Crossref 检索记录：《Constraint hierarchies》（Borning/Freeman-Ben |

## B. doc 轨道（按目标计数，前 80）

| 目标 | 条数 |
| --- | --- |
| chromium | 55 |
| qt-quick-scenegraph | 38 |
| unreal-slate-umg | 28 |
| apple | 24 |
| qingjian-design | 23 |
| unity-ugui | 19 |
| webrender | 16 |
| flutter | 16 |
| clreq | 13 |
| unity-uitoolkit | 11 |
| coherent-gameface | 10 |
| tessera-ui | 10 |
| electron | 10 |
| android | 9 |
| windows-uia | 9 |
| jlreq | 9 |
| css-text-4 | 8 |
| godot | 8 |
| windows-tsf | 8 |
| tiny-skia | 7 |
| gtk4-gsk | 7 |
| salsa | 7 |
| wayland-xml | 7 |
| apple-vrr | 7 |
| text-input-v3 | 7 |
| vello | 6 |
| gpu-driven | 6 |
| react | 6 |
| svelte | 6 |
| imgui-faq | 6 |
| wayland-book | 6 |
| mdn | 6 |
| pathfinder | 5 |
| impeller | 5 |
| solid | 5 |
| android-frame-pacing | 5 |
| js-framework-benchmark | 5 |
| tauri | 5 |
| vue | 5 |
| webassembly | 5 |
| harfbuzz | 4 |
| raqote | 4 |
| msdfgen-readme | 4 |
| noesisgui | 4 |
| microsoft | 4 |
| microsoft-wpf | 4 |
| meshoptimizer | 4 |
| x-damage | 4 |
| wlroots | 4 |
| android-choreographer | 4 |
| android-jank | 4 |
| css-writing-modes | 4 |
| fcitx5 | 4 |
| slug | 4 |
| imgui | 4 |
| taffy | 3 |
| ft-lcd-rendering | 3 |
| nanite | 3 |
| bindless | 3 |
| elm-architecture | 3 |
| at-spi | 3 |
| android-recyclerview | 3 |
| ivd | 3 |
| noto-cjk | 3 |
| rime | 3 |
| msdfgen | 3 |
| react-fiber | 3 |
| cosmic-text-fork | 2 |
| ft-changes-raw | 2 |
| ft-patents | 2 |
| ms-cleartype | 2 |
| ms-gpos | 2 |
| rustybuzz-readme | 2 |
| icu-boundary | 2 |
| microsoft-dcomp | 2 |
| mesh-shading | 2 |
| nanovg | 2 |
| wpf | 2 |
| css-flexbox | 2 |
| qt-scenegraph | 2 |

## C. 述评与深读文件清单

- `papers/adapton.md`
- `papers/atlas-packing.md`
- `papers/cassowary.md`
- `papers/cjk-and-linebreak.md`
- `papers/compositor-latency.md`
- `papers/css-layout-specs.md`
- `papers/differential-dataflow.md`
- `papers/freetype-hinting-lcd.md`
- `papers/gamma-and-subpixel.md`
- `papers/gpu-2d-rasterization.md`
- `papers/gpu-driven-rendering.md`
- `papers/gpu-rendering-techniques.md`
- `papers/gpu-vector-text-dobbie.md`
- `papers/harfbuzz-shaping.md`
- `papers/impeller-shader-offline.md`
- `papers/incremental-ui-state.md`
- `papers/karis-nanite-2021.md`
- `papers/layout-and-incremental.md`
- `papers/lengyel-slug-2017.md`
- `papers/line-breaking-quality.md`
- `papers/mesh-shading-meshlets.md`
- `papers/msdf-glyph-atlas.md`
- `papers/oit-and-overdraw.md`
- `papers/raph-text-layout.md`
- `papers/text-rendering.md`
- `papers/uax-text-properties.md`
- `papers/ui-systems-and-latency.md`
- `papers/valve-sdf-2007.md`
- `papers/yoga-taffy.md`

## D. 复算

```bash
python tools/ledger.py stats          # 总计数与深度分布
python tools/ledger.py verify --min 1000   # 全量门（含引文逐字校验）
```
