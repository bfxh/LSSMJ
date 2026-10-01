# Apple（AppKit / Core Animation / Core Text / Metal）（developer.apple.com @ 2026-10-01 抓取）

> 轨道 B7（doc 为主）。上游为闭源平台，**无 commit 可锚**——本条目的时间戳 = 抓取日 2026-10-01，
> 全部引文来自当日抓取的官方文档快照（落盘 `scratch/fetch/w2f/v/ap_*.json`，抽文本见 `ap_*.txt`）。
> 账本：`docs/analysis/ledger/w2f.jsonl` W2F-001..024（24 条，全 doc）。
> 抓取方式：Apple 文档 JSON API `developer.apple.com/tutorials/data/documentation/<path>.json`（curl/urllib）；
> 归档编程指南走 `developer.apple.com/library/archive/`。

## TL;DR（每条带锚）

1. **layer-backed 的本质是"绘制结果被缓存进 CALayer"**：`wantsLayer` 一置，视图绘制产物即落到层的 backing store，
   且整棵子树一并 layer-backed（W2F-001 `ap_nsview_wantslayer.json`："drawing done by the view is cached to the underlying layer object"）。
2. **layer-hosting 把层的管理权交给应用**，绘制与子视图责任同时转移（W2F-002）。这正是"壳只贴图"要的那一半：
   AppKit 明确区分"我替你画"与"你自己管层"。
3. **提交的原子性由 CATransaction 定义**：多次 layer-tree 修改被批成**对 render tree 的一次原子更新**——
   注意 layer tree 与 render tree 是两个东西（W2F-007 `ap_catransaction.json`："atomic updates to the render tree"）。
4. **隐式事务的提交点是"线程 runloop 下一次迭代"**，不需要应用显式 commit（W2F-008）；显式事务则用
   `[CATransaction begin]` / `commit` 手工定界（W2F-009、W2F-010）。
5. **嵌套事务里只有最外层 commit 触发动画开始**（W2F-010）——批量语义的边界由最外层界定，
   内层只是改写默认参数（时长/时间函数）。
6. **模型值与呈现值分离**：`presentationLayer` 返回"屏幕当前呈现态"的拷贝（W2F-012），
   `add(anim, forKey:)` 的文档措辞是"加到 layer 的 render tree"（W2F-011）。
7. **GPU 提交走 drawable 池**：CAMetalLayer 任一时刻只有一个 drawable 承载层内容（W2F-016），
   复用前提是"不在屏且无强引用"（W2F-017）——多缓冲后端的资源生命周期必须显式管理。
8. **MTLDrawable 是 Metal 与显示系统之间的连接体**（W2F-018）：GPU 提交与合成器解耦的关键抽象。
9. **帧率是协商结果不是常量**：`CADisplayLink.preferredFrameRateRange` 给的是"范围"，系统在**最大刷新率的因数**上取稳定值
   （W2F-019），并会因低电量模式/热状态/无障碍设置收缩可用范围（W2F-020）。
10. **Core Text 是"framesetter 切行 → typesetter 整形 → CTLine 不可变产物"三段**（W2F-021、W2F-022、W2F-023）；
    CTTypesetter 的职责边界被官方一句话写清："character-to-glyph encoding, glyph ordering, and positional operations"
    （W2F-022）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 绘制产物缓存为位图 + 层作为 backing store | W2F-001 / W2F-006 | **吸收**——与青简"预乘 RGBA 位图 + 壳贴图"契约同构，AppKit 是这条路的官方背书 |
| 修改批量化为"帧末一次原子提交" | W2F-007 / W2F-008 | **吸收**——对应 LSSMJ"帧的提交点永远单点"（`lssmj-design` §3） |
| 嵌套批量的"只有最外层生效"语义 | W2F-010 | **吸收**——我们做两层缓冲（damage 累积 / 帧提交）时照此定边界 |
| 排版产物不可变（CTLine） | W2F-023 | **吸收**——显示列表与排版产物一律 immutable，用版本号换新而非原地改 |
| 排版分层（切行 / 整形 / 行对象） | W2F-021、W2F-022 | **有界吸收**——我们只需短行排版，取"整形输出=字形序列+位置量"的边界，不引入 framesetter 的形状填充 |
| 帧率按"最大刷新率的因数"取档 | W2F-019 / W2F-020 | **有界吸收**——小窗默认跟随平台刷新率即可；低功耗档位（如 30Hz）需要协商时按此规则 |
| drawable 多缓冲 + 显式生命周期 | W2F-016 / W2F-017 | **有界吸收**——仅 GPU 档（P3）需要，且必须按"无强引用才复用"写资源管理 |
| 后台线程绘制开关（drawsAsynchronously） | W2F-015 | **有界吸收**——同 LSSMJ 的 damage 后台栅格化，但注意提交点仍在主线 |
| 逐 run 的 gamma 索引 | W2F-033（WPF 侧） | 不吸收（无源）——Apple 文档未给等价细节，改由 WPF/青简侧锚 |
| 隐式动画（改属性即动） | W2F-008 | **不吸收**——小窗要的是确定性出帧，隐式动画会让"帧内容"不确定 |
| contents 参与动画（Animatable） | W2F-013 | **不吸收**（理由一行：我们不做内容插值，位图只做整体替换） |
| 双写者写 contents 的模式 | W2F-014 | **不吸收**——反面教材：我们只允许渲染器单写者 |

## 1. 架构全景（AppKit → Core Animation → 显示）

```text
应用（AppKit NSView 子类）
  ├─ 传统路径：drawRect: → AppKit 把绘制结果收进 layer 的 backing store（layer-backed，W2F-001/003）
  └─ 快路径：wantsUpdateLayer + updateLayer() → 直接改 layer 属性，绕过 drawRect（W2F-005）
        ↓
Core Animation（CALayer 树 = 模型层）
  ├─ 隐式事务：runloop 迭代时自动 commit（W2F-008）
  └─ 显式事务：CATransaction begin/commit，嵌套只有最外层生效（W2F-009/010）
        ↓  提交到 render tree（W2F-007/011）
渲染服务器（系统进程）+ 显示硬件
        ↑
Metal 路径：CAMetalLayer ← nextDrawable（池）← MTLCommandBuffer.present(drawable)（W2F-016/018）
```

- **层的默认类型**是 `CALayer`；`CATiledLayer` 等为特定用途的分支（大图分块，见 `ap_caguide_settingup`
  第 45 行："CATiledLayer class is optimized for displaying large images"）。
- **layer-backed 与 layer-hosting 是本报告最重要的分界**：前者系统管层、后者应用管层（W2F-002/003）。
  我们的平台壳 ≈ layer-hosting：壳提供显示面，渲染器产出位图。

## 2. 关键机制

### 2.1 提交事务（Core Animation 的"帧边界"）

- `CATransaction` = "batching multiple layer-tree operations into atomic updates to the render tree"（W2F-007）。
- **隐式事务**在"无活动事务的线程修改 layer 树"时自动创建，并在"该线程 runloop 下一次迭代"自动提交（W2F-008）。
- **显式事务**用 begin/commit 手工定界（W2F-009）；嵌套时内层只改默认动画参数，
  "Only after you commit the changes for the outermost transaction does Core Animation begin the associated animations"（W2F-010）。

对我们：这条契约给"帧"提供了第二个参照系——**修改可以在任意时刻发生，但只有提交点之后的修改属于下一帧**。
LSSMJ 的做法是把提交点固定在帧末（`lssmj-design` §3 线程模型），语义等价且更简单。

### 2.2 模型层 / 呈现层

- `presentationLayer` 返回"representing the state of the layer as it currently appears onscreen"的拷贝（W2F-012）。
- 动画对象被加入的是 **render tree**（W2F-011），不是模型层。

对我们：hit-test 与布局复算要以"模型值"为准（呈现值对我们是同一值，因为我们不做动画）——
这条的价值是**提醒别在动画语义下复用同一套坐标**，我们的简化是显式的。

### 2.3 内容与绘制

- `contents` 承载位图且**是 Animatable**（W2F-013）；但 layer 绑定 view 时不应直接写 contents，
  否则 "the view replacing the contents of this property during a subsequent update"（W2F-014）。
- `drawsAsynchronously` = "drawing commands are deferred and processed asynchronously in a background thread"（W2F-015）。
- 视图重绘策略：文档明确记载，在老版本里"不常重绘的区域拆成独立 sublayer"曾被用来解决过度重绘，
  而 10.6 起改为推荐 per-view redraw policy（W2F-004）。

### 2.4 GPU 呈现（Metal）

- `CAMetalLayer` 维护 drawable 池，任一时刻只有一个 drawable 承载层内容（W2F-016）；
  复用条件是"不在屏且无强引用"，池耗尽会让下一次申请**等待**（W2F-017）。
- `MTLDrawable` 连接 Metal 与显示系统（Core Animation）（W2F-018）。

### 2.5 帧率协商（ProMotion）

- `preferredFrameRateRange` 的类型是 `CAFrameRateRange`（`ap_caframerateRange.json`）；
  系统"typically provides a consistent frame rate by choosing one that's a factor of the display's maximum refresh rate"（W2F-019），
  并会因系统策略与用户偏好改变可用范围（W2F-020）。
- `CADisplayLink` 自述为"a timer object that allows your app to synchronize its drawing to the refresh rate of the display"
  （`ap_cadisplaylink.txt`）。

### 2.6 文本栈（Core Text）

- 三段：`CTFramesetter`（按形状切行，内部调 typesetter）→ `CTTypesetter`（行排版）→ `CTLine`（不可变行对象）
  （W2F-021、W2F-022、W2F-023）。
- `CTTypesetter` 的职责被一句话界定：字符到字形编码、字形排序、kerning/tracking/基线调整（W2F-022）。
- `CTFont` 同时提供字形相对排布度量与"绘制时的当前字体"两个角色（W2F-024）。

## 3. 性能手段与公开读数

**本节无数字。** Apple 的 API 文档页不提供基准读数，本报告也未抓到任何 Apple 官方 benchmark 数字——
按本仓纪律，**没有锚就不写数字**（见 §5 未验证项第 3 条）。

可称为"手段"的只有机制层面的三条：
1. 位图缓存（layer-backed 的前提，W2F-001）；
2. 后台线程绘制（`drawsAsynchronously`，W2F-015）；
3. 视图级重绘策略替代手工 subsublayer 拆分（W2F-004）。

## 4. 坑与反例（负面留档）

1. **双写者写 contents**：layer 与 view 绑定时直接写 contents 会被 view 后续更新覆盖（W2F-014）。
   → LSSMJ 约束：位图只由渲染器单写者产出，壳不写内容。
2. **drawable 强引用导致池耗尽**：复用前提是"不在屏且无强引用"（W2F-017），
   文档还提示 "if a drawable isn't available when you call..., the system waits"（`ap_cametallayer.txt` 第 44–45 行）。
   → 表现为"偶发无限等待"，是典型的难查性能故障。
3. **帧率不可单方面设定**：系统会因策略/偏好收缩范围（W2F-020）。
   → 性能预算必须写明口径（刷新率档位），不能把"60fps"当成可以永远要求的东西。
4. **用拆层解决过度重绘是历史包袱**：W2F-004 记载这曾是官方推荐做法，后被 per-view policy 取代。
   → 对 LSSMJ 的启示：不要要求调用者自己分层来省重绘，脏区判定要内建在渲染器里。

## 5. 未验证项（缺什么证据）

1. **无源码深度**：Apple 全栈闭源，本条目全部为 doc 深度；机制细节（如 render server 的线程模型、
   提交队列深度）无官方文档可锚。
2. **ProMotion 专文不可得**：`quartzcore/optimizing-promotion-refresh-rates-for-iphone-13-pro-and-ipad-pro`
   在文档 JSON API 上返回 HTTP 404（已留档于 `version/w2f/v/manifest.jsonl` 的 `ap_promotion` 记录），
   本报告关于可变刷新率的全部结论只用到 `CADisplayLink.preferredFrameRateRange` 与 `CAFrameRateRange` 两页。
3. **零性能数字**：Apple 文档不含 benchmark；若后续需要 Apple 侧数字，须另找官方 WWDC 材料或实测，
   本条目不预设任何读数。
4. **`NSView.updateLayer` 的完整前置条件**：文档提到与 `wantsUpdateLayer` 配合及"不该调用 super"，
   本报告只锚到其优化定位一句话（W2F-005），未逐条读该页全部条目。
5. **CATiledLayer 的 tile 尺寸/层级语义**未读全（仅确认其"为大图优化"的定位），
   若要抄它的 tile 划分策略需补读该页与归档指南对应章节。
