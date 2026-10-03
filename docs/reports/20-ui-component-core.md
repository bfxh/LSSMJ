# 20 · UI 组件体系核心（事件/焦点/无障碍/令牌/失效边界）

> **这份报告是什么**：LSSMJ 组件层（`03-task-backlog.md` 轨 9，任务 T-UC-01..05）的**第一份证据锚定**。
> 上一轮（`rust-ui-frameworks.md`/w1c）做的是"框架级·显示面"（布局/文本/重绘局部化/批处理）；本轮做**组件级**：
> 事件路由、焦点模型、无障碍映射、主题令牌、组件重建域、文本输入组件内部。**显示面位图契约不重复**。
>
> **账本**：`docs/analysis/ledger/w9b.jsonl`（**74 条 = doc 43 / source 31，0 拒绝**，`verify` 复算 2026-10-03）；本文每条主张可指到 W9B-xxx。
> **来源与 commit**（抓取 2026-10-03）：
> - AccessKit（克隆）`D:/KF/LSSMJ/scratch/src/accesskit` @ `115887d84f0254cfcad1d9a79d93bba1fde79e75`
> - Qt 焦点源码快照（raw，钉 v6.8.1）：`scratch/w9b/qt-src/{qquickitem,qquickwindow,qqmlbinding}.cpp`
> - 复用克隆（`git rev-parse HEAD` 2026-10-03 复核）：GTK `3b6059be`、WPF `e89a851c`、Flutter `4e5a0929`、egui `6b420bc1`、iced `84f785b0`、slint `ca41829b`、RmlUi `3045e6e3`
> - 规范/文档 23 个 URL（APG、WAI-ARIA、UI Events、DTCG、Selection API、execCommand、Qt/GTK/WPF/WinUI/UIA/Flutter 官方文档、M3/Carbon/Adwaita/HIG/Fluent 令牌材料）
> **只读**：未运行任何上游代码。

## TL;DR（10 条，每条带锚）

1. **无障碍 schema 已被跨语言收敛到 AccessKit**：GTK4 构建依赖 `accesskit-c-0.18`（W9B-061）且有自家 `GtkATContext` 的 AccessKit 实现（W9B-062）；egui 每帧按开关构建 AccessKit 树（W9B-058）；slint 的属性/动作枚举与 AccessKit 同形（W9B-051/052）。⇒ 自绘引擎**只做一套 a11y schema**。
2. **AccessKit 的 TreeUpdate = 增量提交 + 焦点一等字段**：官方要求"一次更新只应包含新增/变化的节点"（W9B-046），且 `pub focus: NodeId` 是结构顶层字段（W9B-047）——与我们的 damage 三层同构，可并入同一失效账。
3. **AccessKit schema ≈ ARIA 的工程化子集**（自述"mostly matches ARIA guidelines for equivalent roles"，W9B-045）⇒ 映射表以 W3C APG/WAI-ARIA 为语义合同（W9B-002/008/009）、AccessKit Role 为落地枚举。
4. **焦点模型三家收敛**：组件边界=焦点域（Qt 向上找最近 focus scope，W9B-064；WPF `IsFocusScope` 记录域内 FocusedElement，W9B-069）；Tab 序=显式/可查询结构（Qt `nextItemInFocusChain`，W9B-065）；陷阱/循环=容器策略枚举（WPF Cycle/None/Local，W9B-070/071）。
5. **事件路由三阶段（捕获/目标/冒泡）有零成本先例**：RmlUi 用一个整数排序键 `distance×(capture?-1:1)` 同时编码阶段与顺序（W9B-053/054）；隧道段对照 WPF 三策略 direct/bubbling/tunneling（W9B-016）。
6. **键盘事件协议=从焦点项沿父链冒泡 + handled 短路**：Flutter 明写"到根 FocusScope 未处理则丢弃"（W9B-056）；slint 同构"to focused item, going up towards the window"（W9B-050）；WinUI 强调按键仅在持有焦点时产生、被控件类消费后不再上抛（W9B-020）。
7. **虚拟焦点（aria-activedescendant）是自绘 composite 的首选形态**：DOM 焦点留在容器、内部只记活跃项，但对 AT 必须"表现得像真焦点"（W9B-010/011）——对应 AccessKit 的 `focus: NodeId`。
8. **令牌分层跨系统共识 = primitive → semantic（→ component）**：DTCG 别名机制（W9B-027/028）、M3 `--md-sys-color-*` + `on-*`（W9B-029）、Carbon core/global + contextual layer（W9B-030/031）、Fluent "alias colors"（W9B-034）、HIG 按用途语义定义（W9B-033）、Adwaita 用 oklab 相对色派生（W9B-032）。
9. **失效边界有现成分类法**：WPF 依赖属性失效位 `AffectsMeasure/AffectsArrange/AffectsRender`（0x001/0x002/0x010，W9B-072/073）≈ 我们的 Hierarchy/Layout/Paint 三段；Flutter 要求"只在帧前事件处理器标脏、帧末统一重建"（W9B-057）；Qt 明确绑定重估**顺序不作保证**且有绑定环检测（W9B-036/068）。
10. **文本输入组件内部四件套各有权威**：选区=anchor/focus 对+方向（W9B-038/039）、剪贴板=命令+可拦截事件（W9B-040）、撤销=独立对象+命令压缩+clean 态（W9B-041/043、GTK 历史对象 W9B-063）、编辑操作按 inputType（用户意图）分类（W9B-042）。

**可吸收（顶部速览）**：RmlUi 事件排序键（W9B-053/054）｜Qt focus scope+焦点链（W9B-064/065）｜WPF 焦点域记忆+策略枚举+失效位（W9B-069/070/072）｜AccessKit 全套（W9B-044..048）｜DTCG/Carbon/Fluent 令牌分层（W9B-027..034）｜Flutter 标脏语义（W9B-057）｜GTK TextHistory / QUndoStack（W9B-063/043）。
**不可吸收（顶部速览）**：Qt 绑定"复杂绑定可任意嵌套"的自由度（W9B-036 反证）｜WPF 的 6 值导航枚举全量（我们只要 Cycle/None/Contained 三档）｜Flutter/GTK 各自私有 a11y 树（只取 AccessKit 一份）｜把 a11y 树放进热路径（QAccessible::isActive 门控的反面，W9B-066）｜纯插件式令牌求值（构建期解析更省，W9B-028）。

## 0. 来源地图（24 targets / 40 个独立来源 = doc URL 30 + 本地源码 10 处；账本 doc 43 / source 31）

| 域 | 来源 | 类型 | 关键锚 |
| --- | --- | --- | --- |
| 事件/焦点语义 | W3C APG（patterns 索引+dialog-modal+listbox+treeview+grid+menubar+keyboard-interface） | doc | W9B-001..009 |
| 事件/焦点语义 | W3C WAI-ARIA 1.2（aria-activedescendant 章节） | doc | W9B-010/011 |
| 事件语义 | W3C UI Events（focus/focusin 章节） | doc | W9B-012/013 |
| 事件/焦点语义 | Qt Focus 文档 | doc | W9B-014/015 |
| 事件/焦点语义 | WPF Input Overview / Focus Overview | doc | W9B-016/017 |
| 事件/焦点语义 | WinUI 3 焦点导航 / 键盘事件 | doc | W9B-018/019/020 |
| 事件/焦点语义 | Flutter Focus 类 / HardwareKeyboard 类 | doc | W9B-021/022/023 |
| 事件/焦点语义 | GTK4 Gtk.Widget 信号表（keynav-failed） | doc | W9B-024 |
| 无障碍 | MS UIA 控件类型↔模式映射表 | doc | W9B-025 |
| 无障碍 | Qt QAccessible（插件式按需加载） | doc | W9B-026 |
| 无障碍 | AccessKit：ARCHITECTURE.md + lib.rs（Role/Action/TreeUpdate） | source | W9B-044..048 |
| 令牌 | W3C DTCG 格式规范 | doc | W9B-027/028 |
| 令牌 | M3（material-web theming/color.md） | doc | W9B-029 |
| 令牌 | IBM Carbon color tokens | doc | W9B-030/031 |
| 令牌 | GNOME Adwaita CSS 变量 | doc | W9B-032 |
| 令牌 | Apple HIG（color） | doc | W9B-033 |
| 令牌 | Fluent 2（fluentui packages/tokens） | doc | W9B-034 |
| 重建域 | Qt QML 属性绑定文档 | doc | W9B-035/036/037 |
| 重建域 | Qt 绑定引擎源码（qqmlbinding.cpp, v6.8.1） | source | W9B-068 |
| 重建域 | slint 焦点失效/按键冒泡源码 | source | W9B-049/050 |
| 重建域 | Flutter Element.markNeedsBuild/framework.dart | source | W9B-057 |
| 重建域 | WPF FrameworkPropertyMetadata 失效位 | source | W9B-072/073 |
| 文本输入 | Selection API（W3C） | doc | W9B-038/039 |
| 文本输入 | execCommand（W3C editing） | doc | W9B-040/041 |
| 文本输入 | Input Events 2（inputType 意图） | doc | W9B-042 |
| 文本输入 | Qt QUndoStack | doc | W9B-043 |
| 文本输入 | GTK GtkTextHistory 源码 | source | W9B-063 |
| 组件焦点实现 | Qt qquickitem/qquickwindow（v6.8.1 快照） | source | W9B-064..067 |
| 组件焦点实现 | GTK4 gtkwidgetfocus/gtkwindow/meson/a11y 源码 | source | W9B-059..062 |
| 组件焦点实现 | WPF FocusManager/KeyboardNavigation 源码 | source | W9B-069..071 |
| 组件焦点实现 | egui AccessKit pass / iced Focusable / slint a11y 源码 | source | W9B-058/055/051/052 |
| 组件焦点实现 | RmlUi EventDispatcher 源码 | source | W9B-053/054 |

## 1. 重点来源短分析（8 份）

### 1.1 AccessKit（Rust，`115887d8`）——自绘引擎 a11y 的**唯一候选 schema**
- 数据模型一句话：树节点=UI 元素或元素簇，含整数 ID、Role、可选属性；动作（Action）由 AT 侧发起、应用侧响应（W9B-044）。
- **增量语义**：`TreeUpdate.nodes` 只装新增/变化节点；未变节点重复提交有成本（适配器会去重事件）（W9B-046）。`focus: NodeId` 是顶层字段（W9B-047）——焦点不是某节点的属性，而是整树状态。
- 与 ARIA 的关系：官方承认属性/动作×角色的对应"基本对齐 ARIA 指南"（W9B-045）⇒ 规范来源应以 W3C 为准，AccessKit 是工程化子集。
- 文本动作面已备好：`ReplaceSelectedText`（删除选区并插入，等价打字/粘贴）、`SetTextSelection`、`SetSequentialFocusNavigationStartingPoint`（W9B-048）——文本组件远程操作面直接照抄。
- **采纳证据链**：GTK4 依赖 `accesskit-c-0.18`（W9B-061）+ 有 AccessKit 版 GtkATContext（W9B-062）；egui 集成（W9B-058）；slint 动作/属性枚举同形（W9B-051/052）。四个独立工具箱（含 C 与 Rust 两侧）共用一个 schema = 我们不必自造。

### 1.2 W3C APG + WAI-ARIA + UI Events——组件行为的**验收语料库**
- APG 模式页即"组件清单+键盘合同"：Tabs（W9B-001）、dialog 焦点陷阱（W9B-002）、打开即移焦（W9B-003）、listbox/treeview/grid/menubar 各自按键表（W9B-004..007）。
- 组件抽象基元：composite widget（多可聚焦元素的离散组件，W9B-008）；Tab 只进一次、内部用方向键（W9B-009）——这就是 roving tabindex / activedescendant 两条实现路线的规范依据。
- **虚拟焦点合法**：焦点可留在容器而 AT 按被指代子项处理（W9B-010/011）；UI Events 又要求"先给焦点、后发事件"（W9B-012/013）⇒ 自绘层"内部焦点"变化必须翻译成正确的焦点事件序。
- 用法：轨 9 的行为用例集（T-UC-02）以 APG 页面为期望值；不自创交互语汇。

### 1.3 WPF——输入路由 + 双层焦点 + 失效位（三件可直接借的零件）
- 路由：direct / bubbling / tunneling 三策略；输入事件成对（Preview 隧道 + 冒泡）（W9B-016）。
- 焦点：keyboard focus（全桌面唯一）vs logical focus（FocusScope 内记忆）；`IsFocusScope` 让 Window/Menu 这类容器"焦点落在容器即委托给域内记忆元素"（W9B-017/069）——**焦点陷阱/恢复语义的现成数据模型**。
- 导航策略：`KeyboardNavigationMode` 六值里 Cycle（回卷）/Contained（边界停）/None（整块一个停点）三种覆盖了菜单、模态、复合块的全部需求（W9B-070/071）；另有 Local（TabIndex 只在本子树内比较）。
- 失效：属性元数据失效位 Measure/Arrange/Render 独立（W9B-072/073）——"组件属性登记失效类别"的官方先例。

### 1.4 Qt——focus scope + 显式焦点链 + 绑定引擎的否定性证据
- `setFocus` 先"向上寻找最近的 focus scope"再在域内 set/clear（W9B-064）；`nextItemInFocusChain` 把焦点链当可查询结构暴露（W9B-065）——与 GTK 的"每次移动时按几何重排"（W9B-059）是两条对照路线。
- 按键入口在窗口（W9B-067，交付给 delivery agent 统一路由），a11y 事件按 `QAccessible::isActive()` 门控（W9B-066），无障碍插件按需加载（W9B-026）——**三处都在告诉我们：a11y 不进热路径**。
- QML 绑定语义（重建域）：依赖变即重估（W9B-035）；**求值顺序不保证**（W9B-036）；赋值即断链（W9B-037）；引擎对绑定环显式判否（W9B-068）。

### 1.5 GTK4——焦点序的"每帧重算"路线 + 无障碍双轨（AT-SPI & AccessKit）
- 焦点移动=`gtk_widget_focus_sort(widget, direction, focus_order)` 后逐个 `child_focus`（W9B-059）——焦点序是**计算出来的**，不维护全局链表；窗口实现 `move_focus` 虚函数做顶层导航（W9B-060）。
- a11y 现在是双实现：传统 atspi/ 目录 + AccessKit 版 GtkATContext（W9B-062），构建期开关（W9B-061）。
- 文本输入：`GtkTextHistory` 独立对象，回调函数组驱动、有 can_undo/can_redo（W9B-063）；`keynav-failed` 信号把"键盘导航失败"做成可拦截事件（W9B-024）——焦点陷阱边界行为有官方挂点。

### 1.6 Flutter——焦点树与重建域的**语义文本最完整**
- 键盘协议：primary focus 起沿父链冒泡、handled 短路、根处丢弃（W9B-056）；Focus 组件不画焦点视觉、只发 onFocusChange（W9B-021）；FocusNode 生命周期/重挂/焦点层级与 widget 层级同构（W9B-022）。
- 键模型：HardwareKeyboard 集中管键事件与键盘状态，物理键/逻辑键分离（W9B-023）。
- 重建域：`markNeedsBuild` 入全局脏表、下一帧统一重建；官方明确"同帧重复构建低效，建议只在帧前事件处理器标脏"（W9B-057）——与我们三阶段帧模型直接对齐。

### 1.7 RmlUi——事件分发器的**最小可用实现**（一个键解决三阶段）
- 监听器排序键 `sort = dom_distance_from_target * (in_capture_phase ? -1 : 1)`（W9B-053），由键直接推出阶段：负=Capture、零=Target、正=Bubble（W9B-054）。约 30 行内实现完整 DOM 式三阶段派发，是我们事件内核的最佳抄写对象。

### 1.8 令牌五家——分层共识与分歧点
- 共识：**语义层按用途/角色命名，组件只读语义令牌**。DTCG 别名（W9B-028）、M3 `--md-sys-color-*`/`on-*`（W9B-029）、Carbon core（全局）→ contextual layer（W9B-031）、Fluent "alias colors"（W9B-034）、HIG"按用途而非外观值"（W9B-033）。
- 分歧：派生方式——Adwaita 用 CSS 相对色在 oklab 里算（W9B-032，运行时求值）；Carbon 的 `$layer-*` 值"自动匹配上下文"（W9B-031，上下文相关）；DTCG 只给格式不管求值顺序（W9B-027）。⇒ 我们取"构建期把 primitive→semantic 解析成常量表 + 少量运行期派生（hover/强调）"。

## 2. 组件层蓝图输入（五段，各给建议与锚）

### 2.1 事件路由（主锚：RmlUi W9B-053/054）
- **形态**：焦点项→根的有序路径，节点上挂（阶段×距离）排序键；capture 段可选（我们只有菜单/快捷键拦截两处需要，WPF 的隧道先例 W9B-016）。
- **协议**：`handled` 短路（Flutter W9B-056 / slint W9B-050）；被组件内建消费的按键要在事件里显式标记（WinUI W9B-020）；"导航失败"类边界事件做成可拦截信号（GTK keynav-failed，W9B-024）。
- **成本纪律**：路径与排序在帧内零分配（排序键=u64：`depth<<2|phase`，RmlUi 的整数编码思想）；事件对象池化。
- **建议**：事件内核照 RmlUi 三阶段键实现（~1 文件），协议语义照 Flutter/slint 冒泡+handled。

### 2.2 焦点（主锚：Qt focus scope+链 W9B-064/065；策略枚举锚：WPF W9B-070/071）
- **数据模型**：FocusScope 栈（域=组件边界，Qt W9B-064 / WPF W9B-069）；每域记忆逻辑焦点（恢复用，WPF W9B-017）；跨域键盘焦点唯一（全局一格）。
- **Tab 序**：显式可查询焦点链（Qt `nextItemInFocusChain` W9B-065）；Tab 只把焦点送进 composite 一次（APG W9B-009），内部方向键（WinUI directional area W9B-019 / APG W9B-008）。
- **陷阱**：容器级策略枚举取 Cycle/Contained/None 三档（WPF W9B-070/071）；模态对话框=陷阱+Tab 不逃（APG W9B-002）+打开移焦/关闭归还（W9B-003/007）。
- **虚拟焦点**：composite 内部用活跃项 id，DOM/引擎焦点留在组件根（W9B-010/011）——直接映射 AccessKit 的 `focus: NodeId`。
- **失效联动**：焦点项不可见必须自动移焦（slint W9B-049 的规则，Flickable 例外照抄）。

### 2.3 无障碍（主锚：AccessKit W9B-044..048；门控锚 QAccessible::isActive W9B-066）
- **schema 唯一**：只做 AccessKit（GTK/egui/slint 三证 W9B-061/062/058/051）；Role/属性/动作清单以 ARIA/APG 校验（W9B-045 + W9B-002/008/009）。
- **构建期**：布局遍历顺路登记 a11y 节点（egui 的 `accesskit_node_builder` 模式 W9B-058），节点 id 复用显示列表 id。
- **提交期**：帧末把"只含变化节点 + focus"的 TreeUpdate 交给适配器（W9B-046/047）；平台适配器用 accesskit 上游（winit/unix/windows/macos 已存在，本轮未逐读）。
- **成本纪律**：树与事件按"AT 是否在跑"门控（Qt W9B-066 / egui 开关 W9B-058 / Qt 插件按需 W9B-026）；跨进程调用是成本主项（`w3c` W3C-091/092 的 UIA 证据）。
- **判定表**：UIA 的"Must support/Conditional"表（W9B-025）=每个 Role 的必选模式/属性集，直接作为映射表骨架。

### 2.4 主题令牌（主锚：DTCG 格式 W9B-027/028；分层实证锚 Carbon W9B-030/031）
- **三层命名**：primitive（原色/尺度）→ semantic（角色，如 bg/fg/accent/on-*）→ component（组件私有覆盖）。跨系统共识见 §1.8；M3 的 `on-*` 配对（W9B-029）与 Fluent 角色×状态命名（W9B-034）作为命名法样板。
- **别名机制**：primitive→semantic 用 DTCG 别名表达（W9B-028），**构建期解析成常量表**、运行期零查表成本（与 WPF 失效位同族：能构建期定的不进运行帧）。
- **变体**：语义令牌携带明/暗/高对比变体（HIG W9B-033 的硬要求）；派生色（hover/pressed/强调）用色彩空间公式生成（Adwaita oklab W9B-032），保一致。
- **判据对接**：T-UC-03 的"默认主题冻结哈希"打在 semantic 层解析产物上；组件只允许引用 semantic/component id（不允许 primitive 直接进绘制）。

### 2.5 组件级失效边界（主锚：WPF 失效位 W9B-072/073）
- **属性登记失效类别位**：Measure/Arrange/Paint（≈我们 Hierarchy/Layout/Paint 三段），只命中必要段（W9B-072/073）。
- **帧末统一 flush**：标脏入表、帧末排序重建；禁止重建过程中标脏（Flutter W9B-057）。
- **传播纪律**：顺序不作保证（Qt W9B-036）、重入/环路检测（Qt W9B-068）、可见性→焦点→a11y 三链联动（slint W9B-049）。
- **门**：T-UC-04"重建范围计数门"=按失效位统计每次事件触发的重建节点数与段数（有 Volatile 逃生阀时同表记账）。
- **边界**：跨组件传播只走显式信号（不做隐式属性图，slint 属性图堆分配的反证在 `w1c` W1C-033）。

### 2.6 文本输入组件内部（附加段；主锚：QUndoStack W9B-043 + GtkTextHistory W9B-063）
- 选区=anchor/focus 对+方向（W9B-038/039）；组件间共享选区单例（W9B-038）。
- 剪贴板=命令+可拦截事件对（copy/cut/paste，W9B-040）；撤销=独立对象（回调驱动、can_undo/can_redo，W9B-063）+命令压缩+clean 态（W9B-043）；undo 语义已外置为独立规范对象（W9B-041）。
- 编辑操作按 inputType 分类入库（W9B-042，撤销粒度=用户意图）；IME 已由 T-UI-08 接（`w3f`）。

## 3. 判定表（可吸收 / 有界吸收 / 不吸收；对齐轨 9 任务）

| # | 项 | 锚 | 判定 |
| --- | --- | --- | --- |
| 1 | RmlUi 事件排序键（距离×阶段整数编码） | W9B-053/054 | **吸收**——事件内核最小实现，零分配路径 |
| 2 | Flutter/slint 冒泡+handled 短路协议 | W9B-056/050 | **吸收**——组件层事件协议基线 |
| 3 | WinUI"控件内建消费要显式" | W9B-020 | **吸收**——热键层不重复触发的前提 |
| 4 | Qt focus scope + 显式焦点链 | W9B-064/065 | **吸收**——FocusScope 栈+next/prev 查询 |
| 5 | WPF 双层焦点（keyboard/logical）+ FocusScope 记忆 | W9B-017/069 | **吸收**——焦点恢复语义 |
| 6 | WPF 导航策略枚举 | W9B-070/071 | **有界吸收**——只取 Cycle/Contained/None 三档 |
| 7 | APG composite/虚拟焦点（activedescendant） | W9B-008/009/010/011 | **吸收**——composite 内部焦点模型 |
| 8 | GTK 焦点序每次重算（几何） | W9B-059 | 有界吸收——作为 Qt 显式链的对照实现；大面板再评估 |
| 9 | GTK keynav-failed 可拦截信号 | W9B-024 | **吸收**——陷阱边界行为挂点 |
| 10 | AccessKit 全套（schema/TreeUpdate/Action） | W9B-044..048 | **吸收**——a11y 唯一 schema；适配器用上游 |
| 11 | a11y 按需门控（isActive/插件/开关） | W9B-066/026/058 | **吸收**——不进热路径（对 C6/C16 同族） |
| 12 | UIA 角色→模式映射表 | W9B-025 | **吸收**——映射表骨架（与 ARIA 对表） |
| 13 | DTCG 别名 + 构建期解析 | W9B-027/028 | **吸收**——令牌分层的机器格式 |
| 14 | 语义令牌多档变体（明/暗/高对比） | W9B-033 | **吸收**——HIG 硬要求，进冻结哈希 |
| 15 | 运行期相对色派生（Adwaita oklab） | W9B-032 | 有界吸收——只用于 hover/强调等少量派生 |
| 16 | Carbon 上下文相关 layer 令牌 | W9B-031 | 有界吸收——需要层叠求值；先做常量层 |
| 17 | WPF 属性失效位三分类 | W9B-072/073 | **吸收**——重建范围计数门的刻度 |
| 18 | Flutter 帧末统一 flush + 禁帧内标脏 | W9B-057 | **吸收**——三阶段帧模型的纪律 |
| 19 | 绑定重估顺序不保证 + 环检测 | W9B-036/068 | **吸收**——测试不得断言重排次序；环路金丝雀 |
| 20 | 选区 anchor/focus+方向；剪贴板命令+事件 | W9B-038/039/040 | **吸收**——文本组件内部四件套之三 |
| 21 | QUndoStack 压缩/宏/clean 态；GtkTextHistory 回调对象 | W9B-043/063 | **吸收**——撤销栈独立可测 |
| 22 | egui 每帧重建 a11y 树（无淘汰） | W9B-058 | 有界吸收——小面可行；长会话要增量（W9B-046） |
| 23 | iced 无 a11y（对照） | W9B-055 | 不吸收（负面留档）——只有 Operation 式焦点遍历可用作最小实现对照 |
| 24 | WPF 六值枚举全量 / QML 复杂绑定自由度 | W9B-070/036 | 不吸收——超需；我们只要三档+简单信号 |

## 4. 未验证项（缺什么证据）

1. **AccessKit 平台适配器未逐读**：只读 common/ARCHITECTURE 与 atspi-common 的入口；`adapters/unix|windows|macos|android|ios` 的事件映射明细（尤其 AT-SPI 的焦点/选区事件）未核——本报告关于"适配器交给上游"的判断基于目录与 ARCHITECTURE 叙述（W9B-062 只锚 GTK 侧）。
2. **Qt 路由内核未读**：快照只有 qquickitem/qquickwindow/qqmlbinding 三文件（v6.8.1 raw），`qquickdeliveryagent.cpp`（deliverKeyEvent 实体、焦点链遍历）未取——W9B-067 只证明"窗口把按键交给 agent"。
3. **GTK 焦点序算法细节未读**：`gtk_widget_focus_sort` 的几何规则（方向优先/包裹规则）未逐行核。
4. **Flutter RebuildScope 完整状态机未读**：只锚 markNeedsBuild 语义与脏表排序（W9B-057）；构建期防重入的 assert 网络未读。
5. **令牌系统官方站不可抓**：m3.material.io 与 Fluent 2 官网为 JS 渲染（抓取仅得空壳），M3/Fluent 证据分别用 material-web 仓库文档（W9B-029）与 fluentui 仓库 types.ts（W9B-034）替代——均为官方仓库，但不是设计规范原文。
6. **Qt 文档版本**：doc.qt.io 抓取页眉显示 Qt 6.12.0（开发版）；Qt 源码快照钉 v6.8.1——两处版本不同，语义类引用不受影响，机制细节以 6.8.1 为准。
7. **无运行时验证**：全部为文档/源码静态观察；焦点金丝雀、读屏器冒烟（T-UC-01/02 的门）本轮未做（按纪律不跑上游代码）。
