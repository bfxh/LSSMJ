# 01 · 引擎/框架全景矩阵（本次分析覆盖版）

> 口径：本表只列**本次真正分析过**的目标（有报告 + 账本行，报告在 `../engines|platforms|targets/`）；
> **未完成项在末尾单列（代理中途中止，不许冒充已覆盖）**。每行给"一票之见"与最能迁移给我们的那条。

## Rust 生态

| 目标 | 报告/账本 | 一票之见 | 最值得迁移的一条 |
| --- | --- | --- | --- |
| **qingjian-render**（交付主体） | `../renderer-qingjian/` + `w5a` 125 | CPU 位图候选窗：契约干净、字体/对齐工程扎实；**无脏区、measure/draw 双整形** | 位图契约 + 对齐三件套 + 字体清单加载 |
| qingjian（引擎全仓） | `../engines/qingjian.md` + `w1a` 176 | 输入法引擎：mmap 容器、增量缓存、"先打点再改"复盘 | "预算一句话挂在热路径"（每键 10ms）与缓存失效口径 |
| tiny-skia | `../engines/2d-rasterizers.md` + `w1d` | 光栅管线编译式 + 2-bit AA + RLE；**两处静默降级是反面教材** | 覆盖率三分支分发；降级必须记账 |
| lyon / femtovg / vello / pathfinder / raqote | 同上（`w1d`） | 细分/GL/计算光栅/GPU 填充/极简光栅，各占一格 | vello 已重组为 cpu/gpu 双档（参考其"按设备分档"） |
| cosmic-text / swash / harfrust / rustybuzz | `../engines/text-stack.md` + `w1b` 115 | 整形栈换代到 harfrust；**亚像素是明确缺口**（Format::Alpha 固定） | 缓存键设计 + opsz 补丁的教训（要按字号分键） |
| taffy / parley / glyphon | 同上（`w1b`） | 布局/富文本/GPU 文本 | **taffy 宽树慢 82%** 的否证数据 |
| gpui（Zed） | `../engines/gpui-webrender-skia.md` + `w1e` | 分桶+8 路归并出区间批；区间索引跨帧复用 | "Scene=区间列表"的执行器模型 |
| WebRender | 同上（`w1e` 59 条） | tile 描述符双缓冲 + damage union + 空脏跳过 | 三维脏区纪律整套照抄 |
| Skia（+Graphite） | 同上（`w1e` 41 条） | SkRecord+BBH；Ganesh 三态合并；Graphite 64 位键 | 批键实证（24B 键慢 ~30%） |

## 大引擎与平台

| 目标 | 报告/账本 | 一票之见 | 最值得迁移的一条 |
| --- | --- | --- | --- |
| Flutter + Impeller | `../engines/flutter-impeller.md` + `w2b` 129 | 不可变绘制包 + 脏标记冒泡上界；Impeller 砍掉框架光栅缓存 | 深度缓冲表达嵌套裁剪；saveLayer 窥孔；预编译着色器 |
| Chromium | `../engines/chromium.md` + `w2c` 100 | 三段管线 + tile 级 LCD 否决清单 + 帧生命周期 17 步 | 亚像素"按 tile 否决"的 11 条枚举 |
| Qt Quick / GTK4 GSK | `../engines/qt-gtk.md` + `w2d` 188 | Qt 批对象/线程；GSK 树 diff damage/无深度缓冲 | 批键下沉到材质 compare()；4×4 子像素字形缓存；"先挤进"不适用但 damage 适用 |
| Unity（UGUI/UITK） | `../platforms/unity.md` + `w2e` | 帧末统一重建 5 阶段；许可红线 | 失效域切分（多 Canvas=island） |
| Unreal（Slate/UMG） | `../platforms/unreal-slate.md` + `w2e` | 三段缓存 + Invalidation Box/Volatile/Retainer 排序 | Volatile 逃生阀与"默认不开昂贵特性" |
| **GN SDK（闭源）** | `../renderer-gn/` + `w4a` 62 | GB2312 字体图时代：SetText/Show 分离、显示列表带字符索引 | "排一次、每帧只显示"的接口形态 |
| **Mineradio（Electron）** | `../targets/mineradio-electron.md` + `w4b` 26 | 把帧预算写成 194 行纪律文档的真实产品 | 预算条目化 + 回收限流 + 去重/可取消 |

## 论文/长文（`w3f` 60 条）

- 断行/排版：Knuth–Plass（DOI 复核修正）、UAX#14/#29/#50、clreq 禁则。
- 字体/矢量：msdfgen、Slug、Pathfinder、vello、parley、imgui FAQ。
- 增量/响应式：Cassowary、Adapton、Salsa、React Fiber、Svelte、JFB。
- 平台契约：TSF、DirectWrite、text-input-v3、wayland damage、Impeller、Electron 性能文档。

## 未完成 → **第二轮已全部补齐**（2026-10-01 晚）

| 目标 | 结果 |
| --- | --- |
| Godot | ✅ `../engines/godot.md`（196 行）+ `w2a` **138 条**（原生 MSDF 全链路、三级 min-size 缓存、**无通用 damage**——强化我们 damage-first 决策） |
| Bevy（用户点名） | ✅ `../engines/bevy.md`（140 行）+ `w4c` **134 条**（文本栈已换 **parley** 0.11；布局"节点级变更加每帧无条件 compute"；UI 批键=栈号+偏移） |
| Rust UI 框架（egui/iced/slint/makepad/dioxus+blitz） | ✅ `../engines/rust-ui-frameworks.md`（172 行）+ `w1c` **99 条**（**五家没人用 taffy 做自家 UI 布局**；重绘局部化四档谱系；egui 否证跨帧复用 tessellation） |
| 文本渲染论文批 | ✅ `../papers/text-rendering.md`（223 行）+ 11 篇深读 + `w1f` **93 条**（paper 66；SDF/gamma/亚像素硬约束/FreeType 版本纪律） |
| GPU 技术论文批 | ✅ `../papers/gpu-rendering-techniques.md`（231 行）+ 7 篇深读 + `w3a` **85 条**（Nanite 讲义全文、矢量填充三代路线、OIT） |
| 布局与增量论文批 | ✅ `../papers/layout-and-incremental.md`（214 行）+ 7 篇深读 + `w3b` **83 条**（taffy 口径补全：135.78 vs 247.42 ms@M1 Pro；Cassowary 最坏情形；DOI 现场纠错两条） |
| UI 系统与延迟论文批 | ✅ `../papers/ui-systems-and-latency.md`（258 行）+ `w3c` **117 条**（damage 三语义+矩形封顶；延迟 20/50ms 刻度；TSF 自绘候选窗先例；**Tessera 查证：组件树级 dirty、无像素级 dirty rect 实测——不得引用**） |
| Web 栈成本 | ✅ `../reports/05-web-js-wrapper-costs.md`（176 行）+ `w3d` **75 条**（JFB 实抄数字：swap1k solid 12.6 vs react 89.9ms；Tauri 2.84MB vs Electron 166.5MB/454MiB/84 线程） |
| CJK 与输入 | ✅ `../reports/06-cjk-text-and-ime.md`（192 行）+ `w3e` **68 条**（clreq 挤压先于禁则；jlreq 短行放宽；竖排`vert`非`vrt2`；Rime/Fcitx5 候选窗惯例） |
| Apple / 微软 / Android | ✅ `../platforms/{apple,microsoft,android}.md` + `w2f` **88 条**（WPF 脏区 ≤8 矩形贪心合并；**DComp 官方"只支持位图"**；Android damage 一等公民；CAMetalLayer drawable 池教训） |
| C/C++ 即时模式群 | ✅ `../engines/immediate-game-ui.md`（222 行）+ `w4d` **90 条**（ImGui 命令簿记/稀疏碼点表；缓冲粒度三谱系；LyShine/Fyrox/RmlUi 保留侧样本） |

**账本终值：2420 条通过 / 0 拒绝**（`verify --min 1000`；source 1546 / doc 643 / paper 224 / web 7）。

## 口径修正（对照用户初稿蓝图，逐条带锚）

1. **"Taffy 10 万节点 1.64ms"**：实测口径相反——taffy 自报宽树 10 万节点**慢 82%**（135.78 / 241.34 / 247.42 ms，0.3@71027a8、M1 Pro、criterion×10；`w3b` W3B-043）。
2. **"Tessera UI 的 dirty rect 实测"**：查证结论=其 dirty 机制在**组件树级**（BuildTreeMode 三档），**没有像素级 dirty rect 实测数字**，该出处不可引用（`w3c` W3C-114..117）。
3. **cosmic-text 栈**：0.19 起整形引擎是 **harfrust**（rustybuzz 已归档）；Bevy 则从 cosmic-text **换到了 parley**（`w4c` W4C-004/010）——整形栈有两条并存路线，选型须对表。
4. **亚像素**：ClearType 仅竖条纹 RGB LCD + FreeType 五点权重 + 纵向无 AA 三条硬约束（`w1f` W1F-067/074/048）；gamma 空间操作全错（W1F-039）。
