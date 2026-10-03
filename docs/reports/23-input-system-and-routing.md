# 23 · 输入系统与事件路由（指针模型 / 捕获 / 命中 / 合并 / 设备 / 延迟）

> **这份报告是什么**：LSSMJ 输入系统的**证据锚定**。此前的输入面是零散的：w9b 做了事件路由/焦点/键盘
> （组件层），w9c 做了拖放与停靠热区（工作区层），w3c 有输入延迟的二手材料；**设备输入本体（指针/触控/笔/手柄）、
> 指针捕获、命中测试、事件合并与原始位移**都是空白（关键词扫描：pointer event/pointer capture/coalesced/
> gamepad/touch event/stylus/winit 全仓命中≈0）。
>
> **上游身份**：多源——W3C Pointer Events 3 / Pointer Lock 2.0 / Gamepad / Touch Events /
> Microsoft WM_POINTER、WM_TOUCH、XInput / Apple NSEvent、UITouch、PencilKit（JSON 通道）/
> Linux evdev 文档与 libinput / SDL3 文档与源码 / **winit `8b5f46d4`** / **Godot `084a2caa` core/input** /
> **Flutter `4e5a0929` pointer 管线** / **egui `6b420bc1` 输入状态与命中** / **iced `84f785b0` 鼠标/触控** /
> 青简 Windows 键输入分派 / Crossref 论文元数据。
> **抓取物**：`D:/KF/LSSMJ/scratch/w11a/raw/`；复算脚本 `scratch/w11a/fetch.py`、`gen.py`。
> **账本**：`docs/analysis/ledger/w11a.jsonl` **60 条 / verify rejected=0**（source 26 / doc 24 / paper 10；15 target）。
> **口径**：本项目 = 渲染引擎 + 前端范式（用户 2026-10-03 范围裁决）；输入系统在范围内。
> 判定三档——**可用 / 有界 / 不可用**；Rust 可得性——**纯 Rust / 平台 API / 用系统能力**。

## TL;DR（10 条，每条带锚）

1. **统一指针模型是跨平台共识**：鼠标/触摸/笔都走同一套指针事件，触控序列被规范化成
   `PointerEntered → PointerButton(Pressed, 唯一 id) → PointerMoved* → PointerButton(Released) → PointerLeft`
   （winit `W11A-005`）；指针 id 在生命周期内唯一（Flutter 单调计数器 `W11A-013`；Windows pointer id `W11A-038`）；
   Touch Events 规范自认受遗留限制，Pointer Events 是继任者（`W11A-037`）⇒ **内核只实现指针模型，Touch 兼容留在引导层**。
2. **取消路径是硬要求，不是边角**：被系统取消时只发 `PointerLeft` 不发 `Released`（`W11A-005`），
   Flutter 还会在取消时**合成一条 move 补齐轨迹**（`W11A-012`）。LSSMJ 的所有 pressed/拖拽状态必须能被取消清空并补点，
   否则会出现"永久按下"与拖拽物残留（这是自绘 UI 最常见的一类 bug）。
3. **指针捕获要显式化**：SDL 有 `SDL_CaptureMouse`（`W11A-021`），W3C 把 capture 与
   "process pending pointer capture" 的时序写进规范（`W11A-030`）；平台侧还有光标抓取模式
   Locked/Confined 的分档与降级（winit `W11A-006`）。LSSMJ 的捕获必须可查询、可取消，并进泄漏检查
   （"未释放的捕获"=红）。
4. **命中测试 = 图层可交互性 × 局部坐标变换 × 距离**：egui 按 `layer_order` 遍历、跳过不允许交互的图层、
   把位置变换到各层局部坐标后算距离（`W11A-017`）；布局/滚动变化还要补发边界事件（规范 `W11A-031`）。
   这与 w9c 的"停靠热区数据化 + damage"是同一条纪律：**命中与绘制共用一份几何数据**。
5. **窗口指针位置与原始位移是两条通道**：窗口坐标可能被系统指针加速/屏幕边缘钳制，不能用于 3D 相机类交互
   （winit `W11A-001`）；egui 把"未加速、未钳制"的 motion 单列且标注可能不可得（`W11A-015`）；
   Pointer Lock 解释为何用 `movementX/Y` 而非 `deltaX/Y`（`W11A-032`），并允许
   `requestPointerLock({unadjustedMovement:true})` 请求绕过加速（可被拒绝，`W11A-033`）。
   `primary` 是语义位（鼠标/第一根手指/未知来源，`W11A-002`），命中与激活由它驱动。
6. **事件合并/缓冲是内核职责**：Godot 的累积输入把新事件与队尾 `accumulate`（合并失败才入队，`W11A-007`），
   另有 agile flushing 档允许帧中 flush（`W11A-008`），并立下"同一帧不得投递同一事件对象两次"的纪律（`W11A-009`）；
   Linux evdev 用 `SYN_REPORT` 保证一次硬件报告的原子性（`W11A-041`），并提供设备侧时间戳 `MSC_TIMESTAMP`（`W11A-042`）。
   LSSMJ 的输入层因此要有：可合并事件表 + 原子报告 + 设备时间戳 + 单次投递标记。
7. **高频采样的无损版本要保留**：Pointer Events 3 定义 coalesced events（同帧合并前的原始采样，按序保留，`W11A-027`）
   与 `pointerrawupdate`（派发前原始更新，`W11A-029`）。默认只用合并位置；绘图/高速笔迹场景可开原始采样通道。
8. **设备能力面按"逐设备查询 + 缺失即降级"**：笔有压力/倾斜/旋转/橡皮端（SDL `W11A-022/023/045`，
   Apple UITouch 的位置/大小/力度 `W11A-049`、PencilKit `W11A-050`）；手柄有标准布局 + 能力查询
   （Gamepad 规范 `W11A-034/035`、SDL `W11A-046`、XInput packet number `W11A-040`）；
   触控要支持多设备多指（SDL `W11A-025/047`）。平台侧还有笔-触摸仲裁与掌拒（libinput `W11A-044`）——
   **这些交给平台，内核只接收仲裁后的指针事件**。
9. **手势层输出语义增量，不输出原始触点数组**：egui 的多点状态直接给 zoom/zoom2d/rotation/translation/force
   五个量（`W11A-018`）；libinput 已识别 Pinch/Swipe/Hold 手势时直接消费（`W11A-043`）。
   LSSMJ 的组件只声明"要哪种手势"，不自己拼触点；手势识别要么用平台结果，要么在输入层做一次（不重复识别）。
10. **延迟是性能问题，且有「预测+纠正」的成熟框架**：MacKenzie & Ware 1993 奠基（`W11A-051`）、
    Jota 2013 与 Deber 2015 给出"多快才够快"及输入延迟/帧率的分离贡献（`W11A-052/053`）、
    Ng 2012 给出低延迟直接触摸与预测纠正方法（`W11A-054`）。规范侧的 predicted events（`W11A-028`）
    与该框架一致。LSSMJ 分两层：先做 coalesced 原始采样（零风险），再做可开关的预测（误差可测）；
    学习式补偿（`W11A-057/059`）列观察项。

## 1. 建议的输入管线（四层 + 一条时间轴）

```text
平台原始层     WM_POINTER/WM_TOUCH/NSEvent/UITouch/evdev/libinput/XInput/SDL    W11A-038/039/048/041/043/040
    │  ① 归一化为统一指针事件（id 生命周期唯一）+ 设备能力查询
    ▼
归一化层       指针{move/button/wheel/enter/leave/cancel} + 笔轴 + 手柄模拟量 + 键盘   W11A-005/013/034/022
    │  ② 合并/缓冲（accumulate + 原子报告 + 设备时间戳 + 单次投递）
    ▼
命中与捕获层   图层可交互性 × 局部变换 × 距离；capture 定向；边界事件补发            W11A-017/030/031/021
    │  ③ 语义化（点击/拖拽/长按/手势/指针锁定位移）
    ▼
语义动作层     组件只收语义事件；动作/命令表（与 w9c 的命令系统同一张表）             W11A-016/018/026
```

时间轴贯穿四层：设备时间戳（缺失才用到达时间）→ 事件合并保留原始采样时间 → 命中/手势按时间轴求导 →
延迟测量可分段归因（`W11A-042`）。

## 2. 技术判定表（逐项：技术 / 性质 / Rust 可得性 / 判定 / 锚）

| 技术 | 性质 | Rust 可得性 | 判定 | 锚 |
| --- | --- | --- | --- | --- |
| 统一指针事件（mouse/touch/pen 同模型） | 跨平台共识 | 纯 Rust（自建结构） | **可用（内核基线）** | W11A-005/013/037 |
| 指针 id 生命周期唯一 | 语义要求 | 纯 Rust | **可用（必做）** | W11A-005/013/038 |
| 取消路径（PointerLeft 无 Released） | 状态机完整性 | 纯 Rust | **可用（必做）** | W11A-005/012 |
| 指针捕获（capture / 定向重派发） | 拖拽/画布基础 | 纯 Rust（平台映射） | **可用（必做）** | W11A-021/030 |
| 光标抓取 Locked/Confined + 降级 | 指针锁定 | 平台 API（winit 已封装） | 可用（能力探测+降级） | W11A-006 |
| 命中测试（图层×局部变换×距离） | 路由前提 | 纯 Rust | **可用（必做）** | W11A-017 |
| 布局/滚动变化补发边界事件 | 悬浮态正确性 | 纯 Rust | **可用（必做）** | W11A-031 |
| 窗口位置 vs 原始位移双通道 | 相机/指针手感 | 平台 API（可选可得） | 可用（缺失即降级记账） | W11A-001/015/032/033 |
| primary 语义位 | 激活/hover 驱动 | 纯 Rust | **可用（必做）** | W11A-002 |
| 激活点击标记（防误触） | 跨平台坑 | 纯 Rust（平台映射） | 可用（安全项） | W11A-003 |
| 事件合并（accumulate） | 高频采样压缩 | 纯 Rust | **可用（默认档）** | W11A-007 |
| 帧末 flush vs 帧中敏捷 flush | 延迟/吞吐取舍 | 纯 Rust | 可用（两档） | W11A-008 |
| 单次投递纪律（同帧不重投） | 调试金丝雀 | 纯 Rust | **可用（必做）** | W11A-009 |
| 原子报告（一次硬件报告不可拆） | 正确性 | 纯 Rust | **可用（必做）** | W11A-041 |
| 设备时间戳贯通 | 延迟测量/预测 | 纯 Rust（缺省用到达时间） | **可用（必做）** | W11A-042 |
| coalesced 原始采样（可开关） | 高频笔迹质量 | 纯 Rust | 可用（绘图档） | W11A-027/029 |
| predicted events（预测+纠正） | 感知延迟 | 纯 Rust（自研预测器） | 有界（质量档，默认关） | W11A-028/054 |
| 笔输入（压力/倾斜/旋转/橡皮端） | 设备能力 | 平台 API + 纯 Rust 结构 | 可用（能力探测） | W11A-022/023/045/049 |
| 手柄（标准布局 + 模拟量 + 能力查询） | 设备能力 | 平台 API（XInput/SDL/winit） | 可用（能力探测） | W11A-034/035/040/046 |
| 触控多设备多指 | 设备能力 | 纯 Rust 状态 + 平台映射 | 可用 | W11A-025/047 |
| 手势语义增量（zoom/rotate/translate/force） | 组件接口 | 纯 Rust | 可用（默认档） | W11A-018 |
| 平台手势/掌拒/笔-触仲裁 | 设备策略 | **用系统能力** | 可用（不重造） | W11A-043/044 |
| 触控板压力（Force Touch） | 可选能力 | 平台 API（支持面窄） | 有界（默认关） | W11A-004 |
| 指针传递函数（加速曲线） | 手感 | 平台曲线 + 参数 | 有界（不重造，跨设备差异保留） | W11A-055/056 |
| 学习式延迟补偿 | 预测质量 | 不进（训练链/权重） | **不可用（本目标）** | W11A-057/059 |
| 自研触摸板/掌拒驱动逻辑 | 平台层职责 | 不做 | **不可用（范围外）** | W11A-044 |

## 3. 重点来源短分析（5 份）

### 3.1 W3C Pointer Events 3 + Pointer Lock 2.0——统一模型的规范源

- 统一指针模型与序列（`W11A-005/037`）、coalesced/predicted/raw（`W11A-027/028/029`）、
  capture 与时序（`W11A-030`）、布局变化边界事件（`W11A-031`）；
  Pointer Lock 的 `movementX/Y` 语义（`W11A-032`）与 `unadjustedMovement` 请求（`W11A-033`）。
- 结论：**LSSMJ 的输入语义以 Pointer Events 3 为合同**，平台 API 只是它的实现来源。

### 3.2 winit `8b5f46d4`——Rust 侧的平台归一化层

- `PointerMoved` 的加速/钳制警告（`W11A-001`）、`primary` 语义（`W11A-002`）、激活点击标记（`W11A-003`）、
  `TouchpadPressure` 的平台支持面（`W11A-004`）、触控序列与取消路径（`W11A-005`）、光标抓取模式与降级（`W11A-006`）。
- 结论：**winit 是 LSSMJ 的平台输入归一化首选**（Rust、跨平台、已建模指针/触控/笔/触摸板压力）；
  自研只做上层（合并/命中/捕获/语义）。

### 3.3 Godot `084a2caa` core/input + evdev——合并、缓冲与原子报告

- accumulate 合并（`W11A-007`）、agile flush（`W11A-008`）、单次投递纪律（`W11A-009`）、
  evdev 原子报告与时间戳（`W11A-041/042`）；Godot 鼠标事件还带 tilt/pressure/inverted/screen_velocity（`W11A-010/011`）。
- 结论：**合并与缓冲的工程形态照 Godot；原子性与时间轴照 evdev**——两者互补，构成内核输入层的最小实现。

### 3.4 Flutter / egui / iced——命中、状态机与指针状态

- Flutter 的 kCancel 合成补点与指针 id 单调分配（`W11A-012/013`）；
  egui 的 latest_pos vs interact_pos（`W11A-014`）、未加速 motion（`W11A-015`）、
  点击/拖拽判定（`W11A-016`）、命中测试（`W11A-017`）、多点语义增量（`W11A-018`）；
  iced 的最小鼠标事件集与 Finger newtype（`W11A-019/020`）。
- 结论：**命中与指针状态机照 egui，取消/补点照 Flutter，事件最小集照 iced**——
  三家各取一段，都是 Rust/C++ 可逐行对照的实现。

### 3.5 SDL3 + Apple + Windows——设备能力面

- SDL：鼠标捕获（`W11A-021`）、笔轴/橡皮端（`W11A-022/023`）、手柄能力查询（`W11A-024`）、触控设备表（`W11A-025`）、
  文档侧的能力面（`W11A-045/046/047`）；Windows WM_POINTER/WM_TOUCH/XInput（`W11A-038/039/040`）；
  Apple NSEvent/UITouch/PencilKit（`W11A-048/049/050`）；libinput 手势与仲裁（`W11A-043/044`）。
- 结论：**设备能力面按"能力位 + 轴/按钮枚举 + 缺失降级"建模**；平台专有策略（掌拒/仲裁/手势识别）用系统结果。

## 4. 与既有轮的接缝

- **与 w9b（组件层事件路由/焦点）**：w9b 定的是"事件进入组件后怎么走"（捕获/冒泡排序键、焦点链、虚拟焦点）；
  本轮定的是"事件从设备到命中/捕获之前怎么来"。两者接口=统一指针事件 + 命中结果 + capture 状态。
- **与 w9c（DnD/停靠/滚动）**：w9c 的 DnD 三段语义、停靠热区、滚动物理都消费本轮的事件流与命中结果；
  本轮的原子报告/合并纪律是它们手感与正确性的前提。
- **与 w3c（延迟）**：w3c 有 Carmack/帧调度等二手材料；本轮补上学术锚（`W11A-051..054`）与规范侧的
  coalesced/predicted（`W11A-027/028`），并把"输入延迟"与"帧时间"分开记账（`W11A-053`）。
- **与 w5a/位图契约**：青简的"壳回传命中坐标"是最小输入回流；本轮把它扩展成完整指针/设备模型
  （宿主输入映射层，`W11A-026`）——位图契约不变，事件侧补全。
- **与范围边界**：物理由用户侧实现，本轮只到"根运动/位移"的输出接口（w10a M1/G9a），不涉及碰撞与刚体。

## 5. 未验证项（缺什么证据）

1. **未运行任何上游代码**：全部为文档/源码静态观察；合并正确性、取消路径、命中一致性、延迟分段
   都要在引擎代码期做金样与实测。
2. **无实测延迟数据**：没有输入→画面反馈的端到端读数（相机/绘图/点击三类场景），
   本报告的延迟结论只到"研究谱系 + 架构位置"层。
3. **Windows Raw Input 文档未取到**（`rawinput` 路径 404）；WM_POINTER/WM_TOUCH/XInput 已取。
   原始输入（Raw Input/RegisterRawInputDevices）只在本报告里由 winit 的"原始位移"与 Pointer Lock 的
   `unadjustedMovement` 间接代表，**未逐字核 Raw Input 文档**。
4. **Android 输入文档未取**（`developer.android.com`/`source.android.com` 在本机网络超时，与 w10b 同因）；
   Android 的 MotionEvent/输入分发只有二手印象，未进账本。
5. **Qt/WPF 的输入路由未新增**：w9b 已覆盖其焦点/路由语义，本轮不重复；但它们的
   "原始输入/触控板手势"部分未逐条核。
6. **平台手势识别细节未核**：libinput 的手势状态机（阈值/超时）与 Windows WM_GESTURE 的参数未逐行读；
   本轮只锚"平台已识别手势直接消费"的架构结论。
7. **游戏手柄实测缺失**：没有 XInput/SDL 手柄的实机读数与映射表（死区/扳机曲线/热插拔）；
   `W11A-040` 的 deadzone 代码片段只作工程参照。
8. **Apple 文档只到 JSON 摘要层**：NSEvent/UITouch/PencilKit 的完整 API（预测触点、合并触点、
   压力阶段）未逐节核——`W11A-049/050` 只支撑"能力字段存在"这一层结论。
