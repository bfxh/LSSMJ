# 前端→Rust 的移植与引导技术选型（19 仓上游 @ 下表 commit；抓取 2026-10-03）

> **本报告是什么**：第七轮 G-D 专项（Master Plan §1「前端语言迁移引导（Rust 内建）」）。任务=把「声明式 UI DSL /
> CSS 落地 / JS-TS 引擎与工具 / Electron→原生映射 / 引导层 DX」五块做成**可选型、可复算**的清单，每条给
> **Rust 生态可用性** 与 **可用 / 有界 / 不可用** 判定。
> **账本**：`../analysis/ledger/w8d.jsonl` **90 条 / 0 拒绝**（source 77 / doc 13；26 个 target）。
> **不重复**：Web 包装的**成本账**已在 `reports/05-web-js-wrapper-costs.md`（账本 `w3d` 75 条）——本报告只做
> **选型与映射**，成本数字一律引 `w3d`（W8D-086）。
> **抓取方式**：git 浅克隆（`--depth 1 --filter=blob:none`，必要时 sparse）共 19 仓；本轮 `raw.githubusercontent.com`
> 两次尝试均超时（curl http=000），**故零 HTTP 文档页**，doc 深度锚取克隆内官方文档 + GitHub blob URL（commit 钉死）。
> **读码范围**：`leptos_macro/src`、`leptos_hot_reload/src`、`yew-macro/src`、`sycamore-{macro,view-parser}/src`、
> `dioxus packages/{rsx,rsx-rosetta,cli}`、`slint internal/{compiler,live-preview,editor-preview}`、
> `lightningcss/src`、`stylo style/`、`servo/Cargo.toml`、`taffy/src`、`blitz-dom/src`、`boa core/engine/src`、
> `rquickjs/src`、`javy/{README,crates/plugin-api,docs}`、`wasmtime/{lib,config}.rs`、`wasmer lib/api/src`、
> `swc/{swc_ecma_parser,swc_ecma_codegen}/src`、`oxc/{oxc_parser,oxc_transformer}/src`、`tauri/{tauri,tauri-utils}/src`、
> `wry/src`、`tao/src`、`electron/docs/api/*.md`、`bevy crates/bevy_asset/src`。

## 上游快照（commit 即锚，结论都是时间戳）

| 目标 | commit | 目标 | commit | 目标 | commit |
| --- | --- | --- | --- | --- | --- |
| leptos | `ac0408a0` | stylo（独立仓） | `34d25f74` | swc | `3c46139b` |
| leptos-book | `f2271e39` | servo（钉 stylo rev） | `a6761746` | oxc | `7f65b757` |
| yew | `bfa6c19a` | boa | `39cd1121` | tauri | `30da1fd6` |
| sycamore | `48e55bb7` | rquickjs | `3d5ecfc9` | wry | `cab3eace` |
| lightningcss | `987e1bf1` | javy | `04a467bc` | tao | `d19f4c12` |
| wasmtime | `6572e8e7` | wasmer | `6c09d9e3` | electron（docs/api） | `df79406c` |
| 本仓既有克隆 | dioxus `b2ed8c32` | blitz `ff623a8c` | taffy `fb461a78` | slint `ca41829b` | bevy `52c3ec0d` |

## TL;DR（每条带锚；括号内为账本 id）

1. **四条声明式 DSL 全是“宏 → 结构化视图值”**：leptos `view!` 自述 RSX 并逐条列与 HTML 的差异（`leptos_macro/src/lib.rs:28`，W8D-001）；
   yew `html!` 固定返回 `Html`（`yew/src/lib.rs:115`，W8D-006）；sycamore 解析产物是独立 crate 的 IR `ir::Root → Vec<Node>`（`sycamore-view-parser/src/ir.rs:8`，W8D-009）；
   dioxus RSX 落在「静态模板 + 动态槽」（`rsx/src/template_body.rs:1`，W8D-010）。
2. **HTML 解析可以外借**：leptos 不自研 — 用 `rstml 0.13.1`（`leptos/Cargo.toml:85`，W8D-002）；标签命名空间（Html/Svg/Math）在编译期分类
   （`leptos_macro/src/view/mod.rs:34`，W8D-003）；yew 用静态集合把属性分监听器/布尔/普通（`yew-macro/src/props/element.rs:25`，W8D-007）。
3. **CSS 在 Rust 已有两条生产级链**：Lightning CSS = 基于 cssparser 的**类型化** CSS 解析/变换/压缩器（`src/lib.rs:1`、`src/properties/mod.rs:3`，W8D-016/019），
   带 `Visitor` 钩子可改写 AST（`src/visitor.rs:147`，W8D-018），**许可 MPL-2.0 须登记**（W8D-017）；
   Stylo = 浏览器级样式引擎，官方自述一份核服务 Servo 与 Firefox（README，W8D-029），Servo 主仓把它钉 rev 成外部 crate（`servo/Cargo.toml:242`，W8D-030）。
4. **Stylo 把“属性集”做成数据**：长属性写在 TOML 清单（`style/properties/longhands.toml:8`，W8D-024）再由构建期生成属性模块——但生成器要跑
   Python+Mako（`style/properties/build.py:11`，W8D-025）；失效信息压成 `RestyleHint: u16` 位域（`style/invalidation/element/restyle_hints.rs:13`，W8D-026）；
   坏声明按条上报而非炸整表（`style/error_reporting.rs:21`，W8D-027）。
5. **“CSS→自绘”的映射层有现成切法**：blitz 用 `stylo_to_parley`（文本）/`stylo_to_kurbo`（transform→2D 仿射）/`stylo_to_cursor_icon` 分文件桥接
   （`blitz-dom/src/stylo_to_parley.rs:1`，W8D-036），且“支持哪些属性”不是手抄白名单而是问引擎（`resolved_style.rs:96`，W8D-035）。
6. **布局盒只产出矩形**：taffy 的 `Layout{position(x,y), size(w,h)}` 是布局计算的全部输出（`taffy/src/lib.rs:11`，W8D-080），
   taffy 覆盖 CSS 的 Flexbox/Grid/Block 三类算法（`taffy/src/lib.rs:4`，W8D-031）并明确低层 API 给“已有自己树”的嵌入者（`:23`，W8D-032）。
7. **纯 Rust JS 引擎＝实验档，C 引擎绑定＝生产档但单线程**：Boa 自述 experimental、覆盖 ES 规范 >90%（README，W8D-037），
   嵌入面仅 `Context`+`Source`（`core/engine/src/lib.rs:1`，W8D-038）；rquickjs 锁在 mutex 后、同 runtime 不能并发（`rquickjs/src/lib.rs:13`，W8D-040），许可 MIT（W8D-041）。
8. **JS→wasm 的包体与 API 边界都写死了**：Javy 动态链接档 1–16 KB、默认静态链接**至少 869 KB**（README，W8D-043），
   默认 ES2023、NodeJS API **不支持**、TextEncoder/Decoder 只是部分支持（官方支持表，W8D-044）；宿主侧用 wasmtime fuel 可硬切执行预算（`config.rs:627`，W8D-047）。
9. **TSX→我们 DSL 的 codemod 底座两条都合格**：oxc 全支持 TS/JSX/TSX/装饰器（`oxc_parser/src/lib.rs:3`，W8D-052）且有可变遍历
   `oxc_traverse`（`oxc_transformer/src/lib.rs:15`，W8D-054）；swc 解析器几乎过全部 test262 且**可错误恢复**（`swc_ecma_parser/src/lib.rs:7`、`:21`，W8D-049/050）。
   dioxus 更直接：自带 HTML→RSX 转换器（`rsx-rosetta/src/lib.rs:17`，W8D-011）与 `dx translate`（W8D-068）。
10. **Electron 能力在 Rust 侧有对应物，但都有平台边界**：Tauri 自述“任意编译到 HTML/JS/CSS 的前端 + Rust 后端二进制”（`tauri/src/lib.rs:5`，W8D-056），
    devUrl 直连 Vite 类 HMR（`tauri-utils/src/config.rs:3836`，W8D-057），tao 管窗口/事件循环（W8D-060）、wry 仍依赖系统 WebView（W8D-059）；
    Electron 侧 autoUpdater 仅 macOS/Windows（W8D-061）、全局热键在 Wayland 走 portal（W8D-062）、contextBridge 不传 `ipcRenderer`（W8D-063）。

## 可吸收 / 不可吸收（对“UI 档 + 声明式引导层”这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 语法解析外借（rstml/HTML parser） | W8D-002/011 | **吸收**：解析不是我们的价值点，语义映射才是 |
| 编译产物形状=静态模板+动态槽 | W8D-010 | **吸收**：与“排一次 + 每帧填变量”同构，直接作为 DSL 后端目标 |
| 解析→IR→代码生成三段分离 | W8D-008/009 | **吸收**：给自家 DSL 做第二个后端时省一次重写 |
| 属性分类表（事件/布尔/样式） | W8D-007 | **吸收**：HTML 子集映射器的必备件 |
| “支持的 CSS 属性由引擎导出” | W8D-035 | **吸收**：支持表单一数据源，防文档与现实漂移 |
| 属性清单→代码生成（TOML→Rust） | W8D-024 | **有界吸收**：抄形态，生成器留纯 Rust（不引 Python/Mako，W8D-025） |
| lightningcss 作 CSS 解析/变换底座 | W8D-016/018 | **有界吸收**：MPL-2.0 需登记（W8D-017）；只用类型化 AST+Visitor，不整吞其全量语义 |
| stylo 整仓作样式引擎 | W8D-022/029/030 | **有界吸收**：语义/失效模型可学，整吞代价=属性面远超 UI 档 + 构建链（Python/Mako） |
| taffy 作布局内核 | W8D-031/032 | **不可用（已判）**：`w3b` D1 已因宽树 −82% 否证；只取其 CSS 语义对齐点 |
| 内嵌 JS（Boa）跑热路径 | W8D-037/038 | **不可用**：自述 experimental、无 JIT/隔离预算承诺 |
| 内嵌 JS（rquickjs） | W8D-040/041 | **有界**：单线程锁 ⇒ 只能放独立线程/进程，且不得进帧路径 |
| JS→wasm（Javy）+ wasmtime fuel | W8D-043/044/047 | **有界**：包体 ≥869 KB（静态档）+ API 支持表有限；沙箱可硬切预算 |
| wasm 组件作扩展单元 | W8D-046/082 | **吸收**：宿主侧无 unsafe、能力经 WIT 接口暴露 |
| oxc/swc 作 codemod 底座 | W8D-049/050/052/054 | **吸收**：许可 MIT/Apache-2.0 干净，恢复式解析 + 可变遍历 + codegen 三件套齐全 |
| Tauri/wry/tao 作**迁移目标形态** | W8D-056/059/060 | **有界吸收**：壳层语义可抄；其渲染仍交系统 WebView，与我们的位图契约分叉 |
| Electron 内置 autoUpdater / 全局热键语义 | W8D-061/062 | **有界**：平台边界与签名要求必须逐条落到我们的壳层工作项 |
| CSS 语义直接映射到命中/事件 | W8D-079/084 | **有界**：必须显式声明坐标系（滚动/子文档偏移），且命中表是唯一落点 |

## 选型判定表（技术 / 角色 / 成熟度 / Rust 性质 / 判定 / 锚）

判定口径：**可用**=可直接依赖，风险已有人替我们承担；**有界**=可用但带明确边界（许可/线程/包体/平台），边界外的用法不成立；
**不可用**=本项目目标下不采纳（含已判项）。成熟度按“官方自述 + 生产使用面”两栏读，不按 star 数。

| 技术 | 角色 | 成熟度 | Rust 性质 | 判定 | 锚 |
| --- | --- | --- | --- | --- | --- |
| rstml 0.13.1 | HTML 语法解析器（可复用） | crates.io 发布，leptos 生产在用 | 纯 Rust（proc-macro 依赖） | **可用（吸收）** | W8D-002 |
| leptos `view!` | DSL 语法/学习曲线样板 | 生产（0.13 系） | 纯 Rust 宏 | 有界吸收（只作参考，不引运行时） | W8D-001/003/077 |
| yew `html!` | 编译期控制流 + 属性分类样板 | 生产 | 纯 Rust 宏 | 有界吸收 | W8D-006/007 |
| sycamore `view!` + IR | 解析→IR→codegen 架构样板 | 生产 | 纯 Rust（宏 + parser crate） | **吸收（架构）** | W8D-008/009 |
| dioxus RSX + rsx-rosetta + `dx translate` | 编译产物形状 + 迁移工具先例 | 生产（活跃） | 纯 Rust | **吸收** | W8D-010/011/012/068 |
| slint `.slint` 编译器 + 预览 | DSL 可混写边界 + 开发环全链 | 生产（GPL-3.0-only / 商业档） | 纯 Rust 编译器库 | 有界吸收（许可；只作参考） | W8D-014/015/072/074/087 |
| Lightning CSS | CSS 解析/变换/压缩底座 | 1.0.0-alpha.72（未 1.0） | 纯 Rust；**MPL-2.0** | 有界吸收（许可登记；用其 AST/Visitor） | W8D-016..021 |
| Stylo（独立仓） | 浏览器级样式引擎（computed 值/失效模型） | 生产（Firefox + Servo 同源） | 纯 Rust 可独立 crate；**MPL-2.0**；构建需 Python+Mako | 有界吸收（学语义，不整吞） | W8D-022..030 |
| taffy | CSS 盒模型/对齐语义 | 生产（blitz 在用） | 纯 Rust（MIT） | **有界**（`w3b` D1 已否证整用） | W8D-031/032/033/080 |
| blitz 的 stylo_to_* 桥 | CSS→自绘映射的切法 | 前置（dev 阶段，同 dioxus 版本线） | 纯 Rust | **吸收（映射层）** | W8D-034/035/036 |
| Boa | 内嵌 JS（纯 Rust） | **自述 experimental** | 纯 Rust | **有界**（脚本胶水可、热路径不可） | W8D-037/038/039 |
| rquickjs | 内嵌 JS（QuickJS-NG 绑定） | 0.14，生产可用 | 绑定（C 源 + unsafe FFI）；MIT | **有界**（单线程宿主） | W8D-040/041/042 |
| Javy | JS→wasm 工具链 | 生产（Bytecode Alliance） | 工具链；Apache-2.0+LLVM 例外 | **有界**（包体 ≥869 KB + API 支持表） | W8D-043/044/045 |
| wasmtime | 插件沙箱宿主 | 生产 | 纯 Rust；Apache-2.0+LLVM 例外 | **可用** | W8D-046/047/048 |
| wasmer | 插件沙箱宿主（备选） | 生产 | 纯 Rust；MIT | **可用** | W8D-075/076 |
| swc | TSX 解析/改写/回吐 | 生产（test262 级自述） | 纯 Rust；Apache-2.0 | **可用（工具链位置）** | W8D-049/050/051 |
| oxc | TSX 全支持 + 可变遍历（codemod 底座） | 生产（活跃） | 纯 Rust；MIT | **可用（工具链位置）** | W8D-052/053/054/055 |
| Tauri | Rust 壳 + Web 前端（迁移目标形态） | 生产（2.x） | 纯 Rust；Apache-2.0 OR MIT | 有界吸收（其渲染仍交系统 WebView） | W8D-056/057/058/082/088 |
| wry | 系统 WebView 封装 | 生产 | 纯 Rust 绑定 | **有界**（渲染分叉点） | W8D-059 |
| tao | 窗口 + 事件循环 | 生产 | 纯 Rust；Apache-2.0 | **可用（壳层）** | W8D-060 |
| Electron 文档 | 能力语义源（映射表左列） | 事实标准（dev 仓，锚到 commit） | JS（只读参考） | **有界**（仅作映射来源） | W8D-061..066 |
| dioxus-cli（create/serve/hotpatch/translate） | 脚手架 / 热补丁 / 迁移命令 | 生产 | 纯 Rust | **吸收（形态）** | W8D-067/068/069/070 |
| Bevy 资产热重载 | 素材热重载语义 | 生产 | 纯 Rust | **吸收（开关化）** | W8D-071 |
| slint live/editor-preview | 预览节流 + 编辑器四件套 | 生产 | 纯 Rust（GPL/商业档） | 吸收（形态） | W8D-072/073/074 |
| 本仓目标模型（DisplayList / 命中表 / C3） | 一切映射的**落点** | 设计已定（本仓文档） | — | **约束（不可绕）** | W8D-083/084/085 |

## 1. 声明式 UI DSL：四条现成物的宏与编译流程

| 框架 | 宏/产物 | 编译流程（读到的事实） | 学习曲线（文档级） | 作“引导层参考”的用法 |
| --- | --- | --- | --- | --- |
| leptos `view!` | `impl IntoView` | `rstml` 解析 token（W8D-002）→ TagType 三分类（W8D-003）→ 生成视图构造；热重载走 `leptos_hot_reload` 的**节点 diff → Vec<Patch>**（`leptos_hot_reload/src/diff.rs:10`，W8D-004；节点表示刻意 minimal，W8D-005） | 官方书单列“控制流”章：if/match 即表达式、`Option/Result/Fn` 都实现 `IntoView`（W8D-077） | **语法与学习曲线样板**；其“控制流即表达式”应成为我们 DSL 的一等语法 |
| yew `html!` | `Html` | 自带 `html_tree`（html_if/for/while/match 皆编译期节点）+ 属性分类表（W8D-007）→ VNode | JSX 血统，Rust 侧概念多（props/组件/消息） | **属性分类与编译期控制流**参考 |
| sycamore `view!` | 视图宏 | `parse_macro_input! → ir::Root`（W8D-008）→ `codegen::Codegen`（同文件） | 小而清晰；文档指向 Book 的 view-DSL 章 | **三段分离的编译架构**样板（W8D-009） |
| dioxus RSX | `CallBody/TemplateBody` | RSX → 静态模板 + 动态节点/属性槽（W8D-010）；解析器把“首次报错必须有用”写成目标（W8D-012）；另有 HTML→RSX 转换器与 `dx translate`（W8D-011/068） | 与 React/JSX 最接近 | **迁移工具 + 产物形状**双样板（本波最强参考） |
| slint `.slint` | 自有 DSL→Rust 代码 | 编译器是独立库，管线模块逐段暴露（parser/object_tree/passes/llr/generator，W8D-087）；文档正面讨论 JSX/HTML+CSS+JS 混写的代价（W8D-014） | 声明式 + 受限表达式，但**不许混写任意代码** | **“可混写边界”的官方取舍样本**；许可 GPL-3.0-only 或商业档，只作参考不引运行时 |

**判定**：上表五家都可**吸收**其“宏→视图值”范式；我们不必自研 HTML 语法解析器（W8D-002），
而应把预算放在「IR → DisplayList」与「状态变更 → damage」两跳上（对齐 W8D-010/083）。

## 2. CSS 在 Rust 的落地

- **Lightning CSS（parcel-bundler/lightningcss @987e1bf1，MPL-2.0）**：解析→类型化 AST→变换/压缩。
  类型化是本报告反复引用的一点：属性值按规范语法解析成具体类型（W8D-019），因此“CSS 文本→我们的 Style 结构体”可**一次映射**而不是逐 token 猜。
  `Visitor` 钩子（W8D-018）提供“只改我关心的那几个属性”的改写面；语法降级由显式 targets 表驱动（W8D-021）；错误类型强制带行列（W8D-020）。
  许可为 **MPL-2.0**（文件级 copyleft）——若嵌入，许可证清单要单列（W8D-017）。
- **Stylo（servo/stylo @34d25f74，MPL-2.0；Servo 主仓钉 rev 9e2238d5 使用）**：官方自述一份核服务 Servo 与 Firefox（W8D-029），
  即“一核多宿主”的官方级样本；主入口 `recalc_style_at`（W8D-023）+ 遍历器 + 缓存（W8D-028）；失效用 `RestyleHint: u16` 位域（W8D-026）；
  属性集是 TOML 数据 + 构建期生成（W8D-024），代价是 **Python+Mako 构建依赖**（W8D-025）；错误按声明粒度上报（W8D-027）。
  **判定：有界吸收**——学“属性清单化 + restyle 位域 + 逐声明错误”，不整吞（属性面与构建链都超出 UI 档需要）。
- **blitz 的桥（@ff623a8c）**：`blitz-dom` 把 DOM/CSS/布局/事件收一层（W8D-034），
  “支持属性表”由引擎启用位导出（W8D-035），样式到下游分文件映射（W8D-036：文本→parley、几何→kurbo/仿射、光标图标）。
  **这是“CSS 子集映射到自研样式模型”的现成切法**：一个属性子集，按消费者切三个映射模块，每个映射都能指回规范条款。
- **taffy 的 CSS 语义对齐点（@fb461a78）**：盒模型/尺寸/对齐/定位（Flexbox/Grid/Block 三算法，W8D-031），
  布局输出仅 position+size（W8D-080），低层 API 面向“已有自建树”的宿主（W8D-032），另有 cssparser 文本通道（W8D-033）。
  **判定：有界**（D1 已否证整用；语义对齐点照抄，实现自研）。
- **动画**：CSS `@keyframes` 在 stylo 里是**样式表规则类型**（`stylesheets/keyframes_rule.rs:5`，W8D-081）——
  引导层的“CSS 动画”应落成“规则→按时间插值出样式值→进布局/绘制”，而不是另起动画运行时。
- **不必重造 CSS 工具**：dioxus-cli 直接把 Tailwind CLI 接进构建环（钉 v3.4.15/v4.1.5 两个 tag，自动下载二进制，W8D-013）——
  “Rust 框架借 JS 生态的 CSS 生成器”是被生产验证过的务实做法；我们的引导层可留同一条外部工具通道（可选、按需拉取）。

## 3. JS/TS 引擎与工具链（Rust 侧）——“嵌进引擎当引导层”的判定

判定见上表（JS/TS 六项）；本节只补**机制要点**（这些是决定边界的实现事实）：

- **预算可被硬切**：wasmtime 的 fuel 计量在生成代码里插桩，耗尽即 trap（W8D-047）——这是“脚本不得拖垮 UI 帧”从口号变成机器判据的唯一现成手段。
- **安全靠能力面而非语言**：Javy 的插件经 WIT（WASI preview2）声明宿主能力（W8D-045），宿主侧 wasmtime 承诺无 unsafe/UB（W8D-046）——
  “给脚本什么”是接口清单问题，不是权限位问题。
- **单线程是硬约束**：QuickJS 系 runtime 锁在互斥量后，同 runtime 不能并发（W8D-040）；Boa 的 `Context` 同样是单上下文执行（W8D-038）。
  兼容回退也要留档：Boa 默认 NaN-boxing，另备 `jsvalue-enum` 特性（性能略降、平台兼容更好，W8D-039）。
- **包体税要按档算**：Javy 动态链接 1–16 KB、静态链接 ≥869 KB（W8D-043）；QuickJS 类的小体积数字（210 KiB/300µs）是上游自述（W8D-042）。
- **codemod 三件套齐全**：恢复式解析（W8D-050）+ 可变遍历（`oxc_traverse`，W8D-054）+ 代码回吐（swc codegen，W8D-051）——
  “改 AST 再打回源码”的链路无需自建。

**共同纪律（三条，都有锚）**：① 任何脚本层都不得在主线程帧路径执行（W8D-040/047）；
② 给脚本的宿主能力必须经受限接口（Javy 的 WIT 插件，W8D-045；tauri 的 Plugin trait，W8D-082）；
③ “可用”档一律宽松许可（MIT/Apache-2.0），MPL 档（lightningcss/stylo）单独记账（W8D-017/041/048/055/058/076）。

## 4. 迁移路径与映射

### 4.1 Electron 能力 → 原生壳映射表（每条一锚；左列源=`electron/docs/api` @df79406c）

| Electron 能力 | 语义（官方原文） | 原生壳对应物 | 迁移注意事项 |
| --- | --- | --- | --- |
| BrowserWindow / `-webkit-app-region: drag` | 无边框窗拖拽区用 **CSS 私有属性**声明（`browser-window.md:458`，W8D-066） | 窗口属性 + 拖拽区声明（tao 的 WindowBuilder/EventLoop，W8D-060） | “CSS 影响窗口行为”的属性要单列映射，不能当普通样式 |
| Tray | “Add icons and context menus to the system's notification area.”（`tray.md`，W8D-065） | 平台托盘 API（Tauri 侧 `tray-icon` feature，W8D-088） | 三平台行为差异（菜单重建/模板图命名）逐平台实现 |
| globalShortcut | “works even if the app does not have keyboard focus”（`global-shortcut.md`，W8D-062） | 平台全局热键注册 | Wayland 走桌面 portal —— 不能假设 X11 语义 |
| ipcMain / ipcRenderer | “Communicate asynchronously from the main process to renderer processes.”（`ipc-main.md`，W8D-064） | 宿主函数/事件回调（Tauri Plugin，W8D-082） | 按“请求-响应 / 单向通知 / 同步查询”三类拆；Mineradio 的 74 handler 不可一比一搬（`w4b`） |
| contextBridge | “safe, bi-directional, synchronous bridge across isolated contexts”（`context-bridge.md`，W8D-063） | 白名单桥面（能力显式暴露） | `ipcRenderer` 已不可经桥传递——桥上只放能力，不放原语 |
| autoUpdater | “only macOS and Windows are supported”（`auto-updater.md`，W8D-061） | 平台更新器 + 服务端 | Linux 无内置；macOS 必须签名；续传依赖服务端 Range/ETag |
| 渲染（Web 栈） | 一切视觉被迫“预算+降级”（`w4b`、`reports/05`） | **显示面位图化**（我们的渲染器接管）+ 控件面暂缓（`docs/targets/mineradio-electron.md` §5） | 这是迁移的真实分界线（`w4b` W4B-010/016 的点击穿透/抓屏复杂度可直接消除） |

### 4.2 Web 概念 → 我们显示列表 / 失效模型的映射（一处一锚）

| Web 概念 | 现成映射样本（锚） | 我们的落点（锚） |
| --- | --- | --- |
| 布局盒 | taffy `Layout{position,size}` 即布局全部输出（W8D-080） | `Layout → DisplayList`（本仓设计已钉，W8D-083） |
| 样式计算 | stylo `recalc_style_at` 逐元素入口 + 遍历器（W8D-023） | 样式重算按元素入口驱动，只走脏子树（配合 C3，W8D-085） |
| 失效/重算 | `RestyleHint: u16` 位域（W8D-026） | damage 三层（版本号→tile→空脏跳过）：任何前端状态更新必须先汇聚成 damage，否则 C3 红（W8D-085） |
| DOM 事件（pointer/mouse/keyboard/ime/…） | dioxus 语义分类 + 可插拔事件转换器（W8D-078） | 平台事件 → 统一事件语义 → 命中表查询（W8D-084） |
| 事件坐标/滚动 | blitz `adjust_coords_for_subdocument`（文档坐标 = 视口 − offset − scroll，W8D-079） | 命中坐标必须显式声明坐标系；命中表是唯一目标解析路径（W8D-084） |
| CSS 动画 | `@keyframes` = 样式表规则类型（W8D-081） | 规则→按时间插值样式值→布局/绘制；不另起动画运行时 |
| “去 Web 栈”的等价落点 | slint 官方：UI 渲进 HTML `<canvas>`（WebGL），不用 DOM/CSS，代价是文本与无障碍自担（W8D-015） | 与位图契约同理——canvas/位图作为交付面成立，但文本/无障碍必须自己兜（同构先例） |

### 4.3 渐进迁移的工程次序（“WASM 先行、界面后移”）

1. **壳先换、渲染后换**：存量应用（Mineradio 型）先吃“任意 Web 前端 + Rust 后端二进制”的形态（W8D-056），
   devUrl 接既有 Vite HMR（W8D-057）——此时尚未动渲染，风险最小。
2. **显示面先位图化**：overlay/歌词舞台/桌面模式先交给我们渲染器（位图 + 命中表，W8D-084/083），
   控件面（设置/登录/歌单管理）暂缓——与 `mineradio-electron.md` §5 的分层结论一致（`w4b`）。
3. **扩展先进沙箱**：任何 JS/第三方逻辑先作为 wasm 插件跑（wasmtime fuel 上限，W8D-047；Javy 的 WIT 能力面，W8D-045），
   永不进内核热路径——这是“不内嵌完整浏览器”的工程化表述（Master Plan G-D 反目标）。
4. **工具链先行于语法**：迁移工具（TSX/HTML→DSL codemod）比运行时更早交付（W8D-049/050/051/052/054/068）。

## 5. 引导层设计输入（脚手架 / 热重载 / 错误 DX）

- **脚手架**：dx create 走 `cargo_generate`，默认模板托管在独立仓（`gh:dioxuslabs/dioxus-template`，W8D-067）——
  模板与主仓解耦、可换模板分支；我们照此做“模板独立仓 + 可换模板”。
- **热重载开发环（三档，从轻到重）**：
  ① **文件事件层**：slint 把 FS 事件归一成 Created/Changed/Deleted（W8D-073）；Bevy 资产热重载是资产层开关
  （`file_watcher` feature + 运行期 override，W8D-071）。
  ② **预览重建层**：slint 的 `REBUILD_DEBOUNCE = 50ms`（单一常量，击键突发合并成一次重建，W8D-072）；编辑器集成
   （文档缓存/编辑会话/版本化诊断/预览连接四件套，W8D-074）。
  ③ **二进制补丁层**：dx hotpatch 直接给已编译二进制打补丁（吃 `dx build --fat-binary` 的结构化产物 + ASLR 参考，默认开，W8D-069/070）。
  建议：我们的档位=① + ②（结构/样式变更）；逻辑变更走常规重建，不承诺③。
- **错误 DX**：三处独立共识——lightningcss 错误类型强制携带行列（W8D-020）；dioxus RSX 把“首次报错必须非常有帮助”写成设计目标（W8D-012）；
  stylo 按声明粒度上报、坏一条留一条（W8D-027）；swc/oxc 的错误恢复让半成品源码一次报多条（W8D-050、W8D-052）。
  另：属性支持表由引擎导出而非手抄（W8D-035），保证“报错/补全/文档”三处口径同源。

## 6. 「内置引导」三档方案与推荐

| 档 | 含什么 | 交付物 | 代价/风险 | 判据（怎么算成） |
| --- | --- | --- | --- | --- |
| **最小** | ① 声明式 DSL 宏（RSX 风格，解析借 rstml 或自研）→ 内容模型→DisplayList；② CSS 子集解析（lightningcss 或 cssparser 子集）→ 我们的 Style；③ 文件事件 + 50ms 去抖预览；④ CLI 脚手架（模板独立仓） | 1 个 DSL 宏 crate + 1 个样式解析 crate + 1 个 dev CLI | 无迁移工具 ⇒ 存量前端要手改；无脚本层 ⇒ 动态行为受限 | DSL 示例集全部编过；样式子集表由引擎导出（W8D-035）；改一行样式 <50ms 见画面（W8D-072） |
| **中（推荐）** | 最小 + ① TSX/HTML→DSL codemod（oxc 或 swc 底座，含 HTML→RSX 式映射表）；② 编辑器/预览协议（缓存+会话+版本化诊断+预览连接四件套）；③ 错误 DX 规范（行列 + 逐声明错误 + 支持表）；④ Electron→壳层映射表（§4.1）作为迁移文档 | 上述 + 1 个 codemod CLI + 1 套 LSP 预览服务 | 需维护 codemod 与两个前端语法（HTML/TSX）；oxc/swc 版本跟版成本 | codemod 对黄金样本仓库可复算（输入/输出落盘）；LSP 诊断与 CLI 报错同源（同一引擎导出，W8D-035）；映射表每条带 Electron 原文锚（W8D-061..066） |
| **大** | 中 + 脚本引导层：wasm 沙箱（wasmtime fuel，W8D-047）承载 Javy 或 rquickjs 单线程宿主（W8D-040），能力经 WIT/插件接口暴露（W8D-045/082） | 上述 + 1 个 wasm 插件宿主 + 插件 API | 包体 ≥869 KB（静态档，W8D-043）；沙箱/桥面/版本兼容三项长期维护；安全面扩大 | 插件超预算即 trap（fuel）；脚本层零主线程帧路径调用；禁用时不增加任何运行期成本（feature 门） |

**推荐：中档**。理由三条（都有锚）：① 价值最高的一环是**迁移工具**（存量 HTML/TSX 手改成 Rust DSL 的成本会把引导层废掉，W8D-011/068）；
② 脚本层是**可选需求**且必须走沙箱，等出现真实插件诉求再上（W8D-043 的包体税 + W8D-040 的单线程约束都是长期成本）；
③ 三档都以“不内嵌完整浏览器、不进内核热路径”为硬边界（Master Plan G-D 反目标；W8D-047 是可机器化的那条）。

## 6.1 交付对接（C16 与 T-GD-01..05）

| 判据 / 任务 | 本报告供给 | 锚 |
| --- | --- | --- |
| **C16 引导层不进热路径**（草案，锚待本报告） | 三档方案的硬边界：宏与 codemod 在编译期/构建期；脚本层只许帧外沙箱且带执行预算 | W8D-089、W8D-047 |
| T-GD-01 三档选型拍板 | §6 三档表 + 推荐（中档）+ 每档判据 | W8D-090 |
| T-GD-02 声明式 DSL 原型 | §1 五家宏/编译流程 + 产物形状（静态模板+动态槽）+ 三段分离架构样板 | W8D-010/008/009 |
| T-GD-03 样式子集（CSS→自研样式模型） | §2 三条链的取舍 + blitz 的映射切法 + “支持表由引擎导出” | W8D-035/036/019 |
| T-GD-04（可选档）WASM/JS 引导层评估 | §3 的逐引擎判决素材（Boa/rquickjs/Javy + wasmtime/wasmer 两条宿主） | W8D-037..048、W8D-075/076 |
| T-GD-05 Electron→原生迁移指南 | §4.1 映射表（逐条带 Electron 官方原文锚）+ §4.3 工程次序 | W8D-061..066 |

## 7. 未验证

- **未跑任何构建/测试**（本工程纪律：只读）：wasmer/wasmtime 的实际嵌入、javy 产物体积、oxc/swc codemod 原型、
  lightningcss Visitor 改写路径，均为**读码/读文档级**结论，无本机复算。
- **包体与性能数字全为上游自述**：Javy 1–16 KB / ≥869 KB（W8D-043）与 QuickJS 的 210 KiB/300µs/75000 测试（W8D-042）
  都是 README 转录（后者还是 rquickjs 转抄 QuickJS），未按“数字带口径”复算。
- **`raw.githubusercontent.com` 本轮两次超时**（curl http=000），故零 HTTP 文档页；doc 锚改用克隆内官方文档 + GitHub blob URL，
  引文可在对应 commit 的克隆物上逐字复算。
- **宏展开产物未读**：leptos/yew/sycamore/dioxus 只读到宏入口与 crate 结构（cargo expand 类输出未看），
  “学习曲线”一列为**文档级判断**（W8D-077 等），非实测。
- **未读**：`oxc_traverse` 源码（只见 transformer 的 use，W8D-054）、rquickjs 的 `sys`/`core` 绑定层、
  swc 的 Node 绑定、wasmer 的编译器后端、stylo 的构建实际可行性（Python/Mako 链，W8D-025）。
- **Electron 版本号未能钉死**：docs 仓 `package.json` 为 `0.0.0-development`（dev 仓通例），故 API 语义锚到
  **commit `df79406c`** 而非版本号；与 `w4b` 的 Electron 42.4.1 样本存在代差风险（未逐条比对）。
- **许可判定只做了“读 Cargo.toml/license 字段”一级**：MPL-2.0 与 GPL-3.0-or-later 的组合结论（W8D-017/030）
  未走法律复核，属工程口径的登记项而非结论。
