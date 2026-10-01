# Rust UI 框架源码分析（egui / iced / slint / makepad / dioxus+blitz）

> 上游与 commit（抓取 2026-10-01，`git rev-parse HEAD`）：
> - egui https://github.com/emilk/egui @ `6b420bc1b11cd4f37d5db6b5e0a2e369f259dd35`（浅克隆 `scratch/src/egui`）
> - iced https://github.com/iced-rs/iced @ `84f785b06ac02a49901e4113451fedd31c9c3326`（`scratch/src/iced`）
> - slint https://github.com/slint-ui/slint @ `ca41829ba5daed4328a30ebdaeddc6923ff72300`（sparse：`api/rs` `internal` `docs`，`scratch/src/slint`）
> - makepad https://github.com/makepad/makepad @ `8e82a8e695af39c582fde30e37e888f375b31d1a`（**快照**：sparse 检出 2 次遇 GitHub 502 失败，改用 `git show <sha>:<path>` 导出 17 个 UTF-8 文件到 `scratch/src/makepad-mp-snap/`，逐字节等同该 commit；锚全部指向快照路径）
> - dioxus https://github.com/DioxusLabs/dioxus @ `b2ed8c328bff51ea5d4e42225a3d1ef7675bad2c`（`scratch/src/dioxus`）
> - blitz https://github.com/DioxusLabs/blitz @ `ff623a8c39714076d77f2ac3c748b9515c7acaed`（`scratch/src/blitz`）
>
> 证据账本：`docs/analysis/ledger/w1c.jsonl`（99 条，source 93 / doc 6，0 拒绝）。本报告每条主张可指到 W1C-xxx；
> 面向 `docs/lssmj-design/README.md` 的判据 C1–C6（见文末映射）。只读分析，未运行任何 cargo。

## TL;DR（每条带锚）

1. **五个框架没有一家用 taffy 做自家 UI 布局**：iced 自研 flex（明示源自 druid，W1C-026 `flex.rs:2`）、slint 编译器生成布局调用、makepad 用 turtle/Walk 游标（W1C-071 `turtle.rs:93`）、egui 即时布局、blitz 用 taffy 但那是"HTML/CSS 引擎"场景（W1C-087 `layout/mod.rs:1` 配 Cargo.lock taffy 0.14）。⇒ 支持设计 §6「taffy 慎用」。
2. **重绘局部化四档并存**：makepad 子树级 DrawList 复用（未变即 `Redrawing::no()`，W1C-067/068）、slint 属性依赖驱动的 item 级脏区（W1C-038/039）、iced tiny-skia 逐图元 diff 且空 damage 直接跳过（W1C-020/021）、egui 完全没有（帧末连空 layer 都清理，W1C-009）。⇒ 我们 P1 的参照物是 slint+iced，不是 egui。
3. **字形缓存键是行业共识**：键=字体 blob+序号+像素尺寸+glyph id+变体坐标哈希+**子像素箱**（slint 4 箱，W1C-047；键成分 W1C-048/049）；egui 给 CJK 关亚像素分箱防图集暴涨（W1C-014）。⇒ 直接支撑 C2 与设计 §5.4"4×4 子像素格"。
4. **egui 的图集策略是无淘汰全量重置**：填充率 >80% 即清空位图+清 galley 缓存（W1C-013）——候选窗量级可用，长会话/大字符集必抖。⇒ 我们图集档必须做分代/淘汰。
5. **帧间复用发生过官方否证**：egui 放弃跨帧复用 tessellation，因为"比较 shape 已花掉重新细分的 ~50% 时间"（W1C-010 `context.rs:3297`）。⇒ 复用判据必须放在**布局产物层**（内容版本号），不能在几何层事后比。
6. **GPU 后端普遍不保证 damage**：slint femtovg 每帧整窗 `clear_rect`（W1C-045）、iced wgpu present 无 damage 分支（对照 tiny-skia 侧 W1C-020）；egui 甚至为截图测试引入"软件纹理过滤"以消除跨 GPU 差异（W1C-004 `renderer.rs:205`）。⇒ C3/C4 要在我们的执行器内自建，不能靠后端默认。
7. **draw list 是 makepad 的性能心脏**：每个子树一个 DrawList（draw_items 持久），`will_redraw` 仅比较本次 walk 矩形≠上次矩形（W1C-067/069/070）；失效判据廉价到只是一次矩形比较。⇒ 候选窗"行级复用"可整体照抄此形态。
8. **slint 把"脏"打进属性依赖图**：脏区来源=包围盒变化 ∪ 渲染属性 PropertyTracker 脏（W1C-038 `partial_renderer.rs:9`），脏传播带 `was_dirty` 去重（W1C-036），读时才求值（W1C-037）。⇒ 设计 §6「信号→订阅者标脏→汇总两级」的现成样板；但 slint 自述属性图"大量堆分配"（W1C-033 `properties.rs:7`），别复制全图。
9. **批处理下限=批键（裁剪矩形, 纹理）**：egui tessellator 只在此二者变化时开新 Mesh（W1C-005 `tessellator.rs:1521`），egui-wgpu 逐 Mesh 一次 `draw_indexed`（W1C-001）；iced 走"层内同类 run RLE 合并"（W1C-023/024）。⇒ 我们的 CPU 基线用 RLE/区间即可，SortKey 全量排序留给 GPU 档。
10. **平台差异要写进常量**：makepad 每帧新增 Slug 字形数 Windows/Linux=1、Apple 不限（W1C-065 `fonts.rs:42`，配套预算门 W1C-064）；iced 用 `NUDGE=0.001` 强制 CPU/GPU 共享同一半像素舍入约定，防 scissor 与边缘错位一像素（W1C-025）。⇒ C1/C4 的读数与实现都必须分平台、分档留档。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"，对齐 C1–C6 判据）

| # | 项 | 锚 | 判定 |
| --- | --- | --- | --- |
| 1 | 子树级 draw list + 矩形比较判脏 | makepad W1C-067/069/070 | **吸收**——候选行内容/几何不变即零工作，实现仅一矩形比较；对 C3 |
| 2 | 属性依赖驱动 item 级脏区 + 空跳过 | slint W1C-038/039/040 + iced W1C-020 | **吸收（缩编）**——我们只取"信号标脏→item 脏矩形"，不建通用属性图（W1C-033 反证其成本）；对 C3 |
| 3 | 逐图元/逐层 damage diff（前帧 vs 本帧） | iced W1C-019/021/022 | 有界吸收——简单可靠但有 O(图元数) 每帧比较成本；我们优先 tile 描述符比较（设计 §4.3） |
| 4 | 字形缓存键含子像素箱+变体哈希+伪斜体标志 | slint W1C-047/048/049 | **吸收**——键必须覆盖一切改变位图的输入；补 opsz/trak 两键（青简全局单槽是反例） |
| 5 | CJK 关闭亚像素分箱 | egui W1C-014 | **吸收**——按字符集分策略，CJK 单槽防图集 4 倍暴涨；对 C2 |
| 6 | 图集脏区 union+上传即复位 | makepad W1C-062/063 | **吸收**——比 egui 全量重置（W1C-013）细；实现成本=一个 Rect+free list |
| 7 | 有界缓存+软上限+重建论证 | makepad Slug W1C-060/061 | 有界吸收——大字号路线才启用；候选窗不上 Slug/SDF（design §5.7） |
| 8 | 帧预算化建资源（每帧限建 N 个） | makepad W1C-064 | **吸收**——新字形/新图标按帧限流，把首帧尖峰摊平；对 C1 稳态 |
| 9 | 重光栅走 worker 通道，提交点仍在 UI 线程 | makepad W1C-066 | 有界吸收——与设计 §3 线程模型一致；仅大场景启用 |
| 10 | 显示列表抽象成 trait，多执行器（GPU/CPU/导出） | blitz W1C-092/093 | **吸收**——DisplayList→{CPU,WGPU} 双执行器同构；对 P3 |
| 11 | 层=天然批边界 + 出屏层剔除 + scissor 像素对齐 | iced W1C-030/031 | **吸收**——三条便宜优化可与 tile/damage 叠加 |
| 12 | 同层内 RLE 连续 run 分批 | iced W1C-023/024 | **吸收（CPU 档即用）**——无需排序成本的批处理下限 |
| 13 | 批键=(clip_rect, texture) 硬断点 | egui W1C-005 + W1C-001/002 | 有界吸收——作为正确性下限；性能上限用区间化归并（gpui 路线 w1e） |
| 14 | 半像素 nudge 常量 CPU/GPU 共享 | iced W1C-025 | **吸收**——凡像素对齐处必须共享同一舍入约定，否则两档分叉；对 C4 |
| 15 | 编译期常量折叠属性（set_constant） | slint W1C-050/052 | 有界吸收——我们没有 DSL；但"构建期能定的移出运行帧"原则保留（GPU 着色器预编译同理） |
| 16 | 生成代码登记绑定闭包（依赖首读登记） | slint W1C-051 | 不吸收——自研引擎里等价物是 stage 列表，不需要代码生成栈 |
| 17 | 即时模式 + 无 damage 全量重绘 | egui W1C-008/009/010 | **不吸收**——候选窗可用，大面板不可容忍（design §4.3 已裁）；记作反面教材 |
| 18 | 图集 80% 全量清空 | egui W1C-013 | 不吸收——周期性重栅格尖峰；我们做分代/淘汰 |
| 19 | taffy 做自研 UI 布局 | blitz W1C-087/088（对照 iced W1C-026） | 不吸收（候选窗级）——taffy 在 blitz 是 CSS 兼容需求；w1b 宽树慢 82%，设计 §6 已定"慎用+复测" |
| 20 | Slug/MSDF 全 GPU 曲线文本 | makepad W1C-073 + msdfer W1C-058/059 | 不吸收（候选窗级）——超大字号档再评估；MSDF 自研不可取 |
| 21 | 全栈 HTML/CSS 引擎（stylo+taffy+vello） | blitz W1C-085/086/089 | 不吸收——重量级+桥接层（stylo_taffy）自维护；语义超出"显示面"定位 |
| 22 | 响应式运行时（信号/记忆化/调度三件套） | dioxus W1C-076..082 | 有界吸收——我们只取"读时订阅、写时标脏、自根向下重跑"最小集，不引 runtime |

## 1. 架构全景（模块地图）

### 1.1 egui（即时模式 + 细分成三角形）
- `crates/egui/src`：`context.rs`（帧循环/`Context::tessellate` 入口，W1C-010）、`layers.rs`（`GraphicLayers[IdMap<PaintList>;5]`，W1C-008/009）、`memory/mod.rs`（跨帧状态桶 `Memory.data: IdTypeMap`，W1C-011）、`id.rs`（64 位哈希 Id，W1C-012）、`ui.rs/painter.rs`（每帧重建 UI）。
- `crates/epaint/src`：`tessellator.rs`（Shape→Mesh，羽化 AA，批键=clip+texture，W1C-005/006/007）、`text/`（`fonts.rs` 视图/图集管理、`font_face.rs` skrifa 轮廓+vello_cpu 光栅，W1C-015、`glyph_atlas.rs` 子像素分箱，W1C-014、`galley_cache.rs` 段落级缓存，W1C-016/017）。
- `crates/egui-wgpu/src/renderer.rs`：单管线 20B 顶点（W1C-003），逐 Mesh `draw_indexed`+逐图元 scissor（W1C-001/002）；`PREDICTABLE` 选项为截图确定性服务（W1C-004）。
- 官方 `ARCHITECTURE.md`：收敛到"2D 形状+文本→带纹理三角形"（W1C-018）。

### 1.2 iced（Elm 风格保留模式 + 双光栅后端）
- 应用态→视图树（保留），`core/src/layout.rs` + `layout/flex.rs`（druid 派 flex，W1C-026）、`core/src/widget`（Tree 保留树）。
- `graphics/src/{layer.rs,damage.rs}`：层栈（W1C-029）+ 前后帧 damage diff（W1C-019/021/022）。
- `wgpu/`：`Engine` 三管线（quad/text/triangle/image），逐层下发（W1C-030），层内 RLE run 合并（W1C-023/024），半像素 nudge 约定（W1C-025）；文本经 cryoglyph（W1C-028）与 cosmic-text fork 整形。
- `tiny_skia/`：CPU 后端**有** damage 跳过（W1C-020）——同一框架内 GPU/CPU 两后端的局部化能力不一致。

### 1.3 slint（编译期 DSL + 属性依赖图 + 多后端）
- `internal/compiler`：`generator/rust.rs` 生成属性初始化/`set_binding` 闭包/`set_constant`（W1C-050/051），`llr/optim_passes/count_property_use.rs` 用读次数驱动折叠（W1C-052）。
- `internal/core`：`properties.rs` 属性图——惰性求值（W1C-037）、脏传播带去重（W1C-035/036）、自述堆分配现状（W1C-033）；`partial_renderer.rs` 属性驱动脏区（W1C-038/039/040）；`item_rendering.rs` 后端原语 trait（W1C-053）；`textlayout.rs` 泛型文本布局（W1C-054）。
- `internal/renderers`：`software/` 逐行+脏区（W1C-041/042/043/044），`femtovg/` 每帧整窗 clear+三类缓存（W1C-045/046），`skia/`、`anyrender/`。

### 1.4 makepad（自研 DSL + draw list 复用 + SDF/MSDF/Slug 三段字形）
- `draw/src/draw_list_2d.rs`：DrawList 持久绘制项，`begin_maybe` 判定复用返回 `Redrawing::no`（W1C-067/068）。
- `draw/src/cx_2d.rs`：`will_redraw` 以 walk 矩形比较+dirty_check_rect（W1C-069/070）。
- `draw/src/turtle.rs`：游标式布局（Walk=尺寸/对齐/Flow，`walk_turtle` 分配矩形，W1C-071/072）。
- `draw/src/text/`：`sdfer.rs`（esdt 版 SDF，W1C-056/057）、`msdfer.rs`（自研 MSDF 边着色，W1C-058/059）、`slug_atlas.rs`（大字号 Slug，曲线缓冲软上限 2^20 f32，W1C-060/061）、`font_atlas.rs`（dirty_rect+free_rects，W1C-062/063）、`fonts.rs`（MSDF worker 通道+Slug 帧预算，W1C-064/065/066）、`rasterizer.rs`（GlyphImageKind 分流，W1C-074）。
- `draw/src/shader/draw_glyph.rs`：Slug 着色器（curve/band 纹理，W1C-073）。

### 1.5 dioxus + blitz（响应式运行时 + HTML/CSS 引擎）
- dioxus：`packages/core/src/reactive_context.rs`（mark_dirty/subscribe，W1C-076/077）、`scheduler.rs`（三类队列+自根向下序，W1C-081/082）、`packages/signals/`（Signal 订阅者集 W1C-078、Memo 脏标志+惰性重算 W1C-079/080）、`packages/generational-box/`（代际句柄 W1C-083）。
- blitz：`blitz-dom`（stylo 样式→自研 stylo_taffy 桥→taffy 布局，damage 位域增量，W1C-087/088/089/090/091）、`blitz-paint`（DOM→anyrender 绘制流，parley 字形 run，W1C-092/093/094）、`blitz-shell`（重绘请求门控+iOS 时序坑，W1C-095/096）、`blitz` 胶水（默认 anyrender_vello，W1C-085/086）。

## 2. 对比轴（①帧驱动 ②布局 ③文本 ④重绘局部化 ⑤GPU 批处理/层）

### 2.1 ① 帧驱动
| 框架 | 形态 | 锚 |
| --- | --- | --- |
| egui | 即时：每帧重建 shape 列表；官方否证跨帧复用（比较=重算 50% 成本） | W1C-010 |
| iced | 保留：应用态→视图树；重绘由 window RedrawRequested 驱动 | W1C-032 |
| slint | 编译期：.slint AOT 编译成机器码+属性图；读时才求值 | W1C-055/037 |
| makepad | 保留+复用：子树 draw list 持久，未变即不录制 | W1C-067 |
| dioxus+blitz | 保留+响应式：脏 scope 重跑驱动 DOM 变更→增量布局→vello 重绘 | W1C-082/091 |

对我们：**取 makepad/slint 的"未变即不录"精神 + iced 的显式树**，落地为 Frame→(版本号)→DisplayList（设计 §3/§4.3）。

### 2.2 ② 布局
- 自研派：iced（druid 派 flex，W1C-026）、makepad（turtle 游标，W1C-071/072）、egui（即时布局）、slint（编译器生成）。
- 第三方派：blitz→taffy（W1C-087，配 stylo_taffy 桥 W1C-088——桥接层是自维护成本）。
- 结论：我们的"锚点+排列格式"自研内核与生态主流一致；taffy 仅在大面板阶段按 w1b 否证条件复测。

### 2.3 ③ 文本（图集/字形缓存）
- 键：slint 全要素键（W1C-047/048/049）；egui CJK 例外（W1C-014）。
- 图集：egui 80% 清空（W1C-013）vs makepad dirty_rect 增量上传（W1C-062/063）。
- 光栅：egui=skrifa+vello_cpu（W1C-015）；makepad=SDF(esdt)/MSDF(自研)/Slug 三档（W1C-056..059）；blitz=parley 整形+anyrender 绘制（W1C-094/097）；slint=parley 或 swash 可切（W1C-099）。
- 容量工程：makepad 的软上限+帧预算（W1C-060/061/064/065）值得抄；我们候选窗维持 SwashCache 直存（设计 §5.4），大面板再上 2D 图集。

### 2.4 ④ 重绘局部化（本设计对青简的最大升级）
从粗到细：
1. 无局部化：egui（W1C-009/010）。
2. 帧间 diff：iced tiny-skia（W1C-019/021/022）——含"阴影外扩入 damage"的必答题（W1C-022）。
3. 属性驱动 item 脏区：slint（W1C-038/039/040），脏区矩形数上限 3（W1C-041/042）。
4. 子树 draw list 复用：makepad（W1C-067/068/069/070）——最廉价判据（矩形比较）。
5. blitz：布局层 damage 位域增量（W1C-089/090/091），但绘制端整场重放（paint_scene 无 damage，W1C-093）。

我们 P1 组合：makepad 式"未变即不录/不画" + slint 式"信号标脏" + 设计 §4.3 tile 描述符比较；三者都要求**空 damage 直接跳过**（iced W1C-020 是先例，我们加进 C3 的验收口径）。

### 2.5 ⑤ GPU 批处理/层
- 批键：egui=(clip,texture) 硬断点（W1C-005）；iced=层内 RLE run（W1C-023/024）+层边界（W1C-030）+出屏剔除与 scissor 对齐（W1C-031）；顶点 20B（W1C-003）。
- 层：egui 5 个 Order 分层（W1C-008）；iced 层栈裁剪继承（W1C-029）。
- 确定性：egui PREDICTABLE（W1C-004）与 iced NUDGE（W1C-025）都说明"跨后端逐位一致"需要显式工程。
- 结论：CPU 档用 RLE/区间归并 + 层边界；GPU 档把 (clip,texture) 作为正确性下限、SortKey 全排序作为上限，预编译着色器照 w3f W3F-050。

## 3. 性能手段与公开读数（全部带锚；均为机制类，不给跨机速度断言）

| 手段 | 框架 | 锚 | 口径 |
| --- | --- | --- | --- |
| 空裁剪矩形零几何 | egui | W1C-006 | 判据：`clip_rect.is_positive()` 前置 |
| 空 damage 零重画 | iced tiny-skia | W1C-020 | `damage.is_empty()` 时仅重入栈上一帧记录 |
| damage 归并阈值 20000px² | iced | W1C-019 | 逻辑像素²，经验常量（未给推导） |
| 脏区 ≤3 矩形 | slint 软件后端 | W1C-041/042 | 编译期断言钉死（MAX_COUNT==3） |
| 图集 80% 触发清空 | egui | W1C-013 | 代价=周期性全量重栅格 |
| 每帧限建 1 个 Slug 字形（Win/Linux） | makepad | W1C-064/065 | Apple=不限；平台常量区分 |
| Slug 曲线缓冲软上限 2^20 f32（≈4MB） | makepad | W1C-060/061 | 超限整体重建，注释论证不抖动 |
| 子像素 4 箱 | slint | W1C-047 | 1/4 像素分箱，注释"够用且缓存小" |
| 半像素 nudge=0.001 | iced | W1C-025 | CPU/GPU 共享约定，防 1px 错位 |
| 全量重绘=全标脏（同管线两模式） | blitz | W1C-090/091 | 非增量=每节点 ALL_DAMAGE，读数可 A/B |

## 4. 坑与反例（负面留档）

1. **即时模式无跨帧复用余地**：egui 连空 layer 都在帧末清理（W1C-009），且官方算过复用 tessellation 不划算（W1C-010）——证明"事后再比"路线死路，必须前置版本号。
2. **图集全量重置的尖峰**：egui 80% 阈值（W1C-013）会在长会话周期性触发全量重栅格；我们不做。
3. **CJK×亚像素=图集爆炸**：egui 明写 CJK 会 hog 图集（W1C-014）；任何"4 箱对所有字符"的方案在 CJK 上都错。
4. **宽高量化断振荡**：egui 把 wrap 宽度 round 到整数像素以斩断宽度↔换行反馈回路（W1C-016）——候选窗宽度回传必须带稳定化。
5. **阴影是 damage 的隐藏项**：iced 必须把 shadow offset+blur 外扩进 damage（W1C-022）；我们的阴影缓存失效判据必须同样覆盖外扩。
6. **CPU/GPU 舍入分叉**：iced 记录半像素浮点噪声导致 snap 分叉，必须共享 nudge（W1C-025）。
7. **平台时序坑**：iOS 在 WindowEvent 内 request_redraw 无效（W1C-095）；blitz 用"事件外补发"防御。
8. **GPU 后端不保证 damage**：slint femtovg 整窗 clear（W1C-045），iced wgpu 侧无 damage 分支（对照 W1C-020）——C3 要在我们执行器内实现。
9. **桥接层自维护**：blitz 的 stylo→taffy 桥（W1C-088）说明"复用样式引擎"≠"复用布局"；我们的依赖同样只在同质栈内复用（cosmic-text+swash+tiny-skia 均 Rust 同族）。
10. **属性图分配成本**：slint 自述属性引擎"大量堆分配、待优化"（W1C-033）——我们的最小脏传播不建通用属性图。
11. **自研 MSDF 复杂**：makepad 自写边着色 MSDF（W1C-058/059）代价高；要 MSDF 用现成件（设计 §5.7 结论）。
12. **响应式运行时是系统工程**：dioxus 三类队列+顺序约束（W1C-081/082）+代际句柄（W1C-083）有其必要复杂度；我们只取最小集（W1C-079/080）。

## 5. 对本项目判据 C1–C6 的映射（lssmj-design/README §2）

| 判据 | 本报告的直接输入 | 锚 |
| --- | --- | --- |
| C1 稳态单帧 P95 ≤2ms（CPU 光栅） | 可抄：行区间脏区（slint W1C-043/044）、帧限流摊峰（makepad W1C-064）、RLE 分批（iced W1C-024）；反例：图集全清尖峰（W1C-013） | W1C-043/044/064/024/013 |
| C2 文本宽度对原生 ≤0.01pt、字形归属可查 | 全要素缓存键（slint W1C-047/048/049）+ CJK 分策略（W1C-014）+ 宽度量化稳定化（W1C-016） | W1C-047/048/049/014/016 |
| C3 空 damage→零重画 | 现成先例：iced 空跳过（W1C-020）、slint 属性驱动脏区（W1C-038/039/040）；必要判据：廉价矩形比较（makepad W1C-069/070） | W1C-020/038/039/040/069/070 |
| C4 帧间金丝雀逐位相同 | 先例与代价：egui PREDICTABLE（W1C-004）、iced NUDGE 共享约定（W1C-025）——逐位一致需显式确定性档 | W1C-004/025 |
| C5 资源缺失可定位+回退 | 机制证据：makepad Slug 预算门返回 `Deferred`（W1C-064）、blitz 重绘请求门控（W1C-096） | W1C-064/096 |
| C6 每帧零大分配、分配点白名单 | 反例驱动：slint 属性图堆分配自述（W1C-033）、egui Memory 读即克隆（W1C-011）——我们不引入二者形态 | W1C-033/011 |

## 6. 未验证项（缺什么证据）

1. **makepad 的 GPU 提交层未读**：只取到 `draw/` 与 `widgets/`；draw item→instance buffer 的合并/上传在 `platform/`（未检出，502 阻断）。第 2.5 节关于 makepad 批处理的说法仅限 draw list 层。
2. **iced 布局性能无读数**：flex 是自研，但本仓未做基准（未跑代码）；"taffy 慎用"不因此条改变。
3. **slint 编译器→渲染调用链只取两点**：`set_binding`/`set_constant` 两行；生成代码如何驱动 `ItemRenderer`（render_item_children 的调用点）未逐一核。
4. **blitz 绘制端是否真无 damage**：仅确认 `blitz-paint` 无 damage 引用（本轮 grep 范围）；anyrender_vello 内部是否做 tile 复用未查（外部 crate，未读）。
5. **各框架性能数字**：刻意不抄任何博客读数（web 深度不能单独支撑决策）；机制结论全部来自源码锚。
6. **makepad 快照完整性**：17 个文件为 `git show` 导出，未做 `git status` 对账（sparse 检出未完成）；引用行号仅对 8e82a8e6 快照成立。
