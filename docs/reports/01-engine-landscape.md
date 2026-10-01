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

## 未完成（代理中断，如实登记）

| 目标 | 计划 | 状态 |
| --- | --- | --- |
| Godot | `docs/engines/godot.md` | ⬜ 无产出 |
| Bevy（用户点名） | `docs/engines/bevy.md` | ⬜ 无产出 |
| egui/iced/slint/makepad/dioxus+blitz | `docs/engines/rust-ui-frameworks.md` | ⬜ 无产出（w1c 未生成） |
| 文本渲染论文批（SDF/亚像素/整形论文带 60 条） | `docs/papers/text-rendering.md` | ⬜ 无产出（w1f 未生成；部分主题由 `w3f` 覆盖） |
| Apple / 微软 / Android | `docs/platforms/{apple,microsoft,android}.md` | ⬜ 无产出（w2f 未生成） |
| 游戏中间件（Noesis/Coherent） | `docs/platforms/game-ui-middleware.md` | ⬜ 无产出 |
| GPU 技术论文批 / 布局论文批 / UI 系统论文批 / Web 成本 / CJK 专项 | w3a–w3e | 🟡 部分由 `w3f`（60 条）与 `w2c`/`w4b` 覆盖，专项报告未成 |

> 补齐路径已写好：`../analysis/targets.md` 的波次表 + `../analysis/AGENT-BRIEF.md`（恢复预算后可按简报直接续跑，
> 账本编号段（w1c/w1f/w2a/w2f/w3a–e）未占用，不会撞号）。
