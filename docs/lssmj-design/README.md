# LSSMJ 渲染引擎设计（v1，2026-10-01）

> **这份文档是什么**：在 1300+ 条证据锚定条目（`../analysis/ledger/`，`verify` 全绿）与 8 篇引擎深读
> （`../engines/`）、2 篇平台深读（`../platforms/`）、2 个真实样本解剖（`../targets/`、`../renderer-gn/`）
> 的基础上，给出**我们自己的**渲染引擎设计。**规则：每个关键取舍必须写明"学谁 / 弃谁 + 证据锚 +
> 否证条件"**；证据不足处一律标"待测"，不许写成结论（本仓表述纪律）。
> 决策明细见 [`../reports/10-decisions-adr.md`](../reports/10-decisions-adr.md)。

---

## 1. 定位（做什么、不做什么）

**做**：一张"显示面"渲染引擎——候选窗、悬浮条、overlay、游戏 HUD、小面板——输入"内容模型 + 主题"，
输出**位图（默认）或 GPU 帧（可选档）**，平台壳只负责贴图与回传输入坐标。
**不做**：完整控件 toolkit（表单/无障碍留给平台原生或上位框架）——这是青简对 UI 两分法的结论
（`../renderer-qingjian/01-overview.md`，`docs/design/rendering.md:13`–`:14`），延续它。

**语言**：整仓 Rust（用户口径）。文字/光栅/字体全部选 Rust 生态现成最优件（cosmic-text+swash、
tiny-skia、fontdb），不重复造轮子；**借鉴对象不限语言**——C/C++ 的 Skia/Chromium/Qt/GSK/GN SDK
与 Rust 的 gpui/WebRender/tiny-skia/vello 同权比较（每条的锚见 ADR）。

## 2. 成功判据（先量后改；全部配测量协议见 `../reports/09-performance-methodology.md`）

| # | 判据 | 锚（现状基线或参照） | 状态 |
| --- | --- | --- | --- |
| C1 | 稳态单帧 P95 满足预算（400×500 pt @2x 场景：P95 ≤ 2 ms CPU 光栅路径） | 基线：青简单帧 release 1 ms / 冷首帧 6 ms（`docs/design/rendering.md:87`；口径=macOS spike、单场景） | 待测 |
| C2 | 文本宽度对原生 ≤0.01 pt；字形归属可查 | 青简判据（`rendering.md:95`）；工具 `trace_families` | 复用 |
| C3 | **空 damage → 零重画**（无变化帧只允许一次比较+memset 级工作） | WebRender 空脏区跳过合成（`w1e` W1E-082）；GSK 树 diff（`w2d` W2D-017） | 待实现 |
| C4 | 帧间金丝雀：同一内容渲染两次逐位相同 | 本仓账本金丝雀纪律 | 待实现 |
| C5 | 资源缺失可定位 + 自动回退 | GN 的 RunLog 失败模式（`w4a` W4A-060）；青简 `NoUiFont` 回退（`w5a`） | 复用 |
| C6 | 每帧零大分配（字符串/缓冲），分配点白名单化 | ImGui FAQ 字符串警告（`w3f` W3F-060）；Mineradio 预算纪律（`w4b` W4B-019..022） | 待测 |

## 3. 总体架构

```text
        ┌────────────────────────────────────────────────────────────┐
内容模型 │ Frame { rows, preedit, highlight, columns, footer, … }     │  ← 与青简 Frame 同族
        └────────────────────────────────────────────────────────────┘
                       │  版本号（内容变才重排）
        ┌──────────────▼──────────────┐
布局与显示列表 │ Layout → DisplayList       │  学 GN sDRAW_RANGE（`w4a` W4A-035/036）
        │  每项：矩形+UV+文本run+字符映射 │  每项带"命中所需的一切"
        └──────────────┬──────────────┘
        ┌──────────────▼──────────────┐
绘制    │ Paint：字形缓存/图集 → 光栅     │  CPU：tiny-skia 式扫描线 + 2bit 超采样 + RLE（`w1d`）
        │  damage 区域重栅格（tile 比较）  │  GPU 档：SortKey 批 + 区间归并（`w1e`）
        └──────────────┬──────────────┘
        ┌──────────────▼──────────────┐
合成与贴图 │ Compose → 位图/BGRA → Present │  学青简契约：预乘 RGBA + 命中表（`w5a` W5A-003/077）
        └──────────────────────────────┘   Windows: UpdateLayeredWindow（W5A-093/094）
```

**crate 划分**（按依赖单向）：

| crate | 职责 | 关键外部依赖 |
| --- | --- | --- |
| `lssmj-core` | 几何/颜色/Frame/DisplayList/命中表 | 零依赖 |
| `lssmj-text` | 字体清单加载、整形、字形缓存/图集、trak/opsz/gamma | cosmic-text(harfrust)、swash、fontdb |
| `lssmj-raster` | Path/填充/混合/阴影/图标（CPU） | tiny-skia（或内联同构实现，见 ADR-04） |
| `lssmj-render` | 布局→显示列表→绘制→damage→合成 | 上三者 |
| `lssmj-shell-{win,mac,linux}` | 贴图/输入/IME/Dpi | windows-rs / objc2 / wayland-or-x11 |
| `lssmj-gpu`（可选档） | wgpu 后端（同显示列表，换执行器） | wgpu |

**线程模型**：默认 **UI 线程同步出帧**（青简：单线程共享渲染器，"字形缓存共享"，`w5a` W5A-090）；
大场景（滚动面板）引入 **damage 后台栅格化**（学 Chromium worker 栅格 `w2c` 与 GSK 树 diff 的
"只重画 damage"），但**帧的提交点永远单点**（Qt 每窗一线程的复杂度不引入，`w2d` W2D-012 记为有界吸收）。

## 4. 渲染核心（学谁 / 弃谁）

### 4.1 光栅（CPU 基线）

- **管线"编译"式**：把"填充规则+混合模式+裁剪"编译成 stage 列表再执行（tiny-skia `w1d` W1D-002/005，
  `MAX_STAGES=32`）。我们的约束：**stage 超限必须显式报错/降级并记账**——tiny-skia 的两处静默降级
  （short 溢出退非 AA、超 2^22 长度三次曲线打回直线，`w1d` W1D-108/017/018）是**反面教材**：
  宁可慢一步也不许"悄悄改变质量"。
- **抗锯齿**：2-bit 超采样 + 稀疏 RLE 覆盖率，三分支分发（0 跳过 / 255 走矩形 / 中间跑管线，
  `w1d` W1D-008/010/023）。这是 CPU 路线的关键省法。
- **并行档（为"大面板"预留）**：学 Blend2D 的条带并行参数——**条带高从 64 起、cell 缓冲上限
  256KiB、条带数 < 线程数时折半（下限 8）**（`w4e` W4E-007..010）；**ThorVG 的 SmartRender
  16×16 分区双缓冲脏表是"CPU 端 damage"的现成范式**（`w4e` W4E-068/070/072），与 §4.3 的三层
  设计同构，可作 P1 的实现先例。
- **字形**：覆盖率遮罩 × 颜色（青简 `w5a` W5A-007/008）+ 覆盖率 gamma LUT（W5A-056）；
  彩色 emoji 直通 RGBA（W5A-008）。
- **阴影**：3 遍盒式模糊近似高斯（青简 `w5a` W5A-014..018 的成本与正确性都有测试锚）；
  **改进**：随滚动/移动的内容阴影缓存"尺寸不变即复用"，把 6 次全画布扫描降为一次贴图（青简自留的优化项，`09-assessment` §9.3）。

### 4.2 批处理与 GPU 档（预留，不当第一优先级）

- 批键：**64 位 SortKey 聚类**（Graphite 实测 24B 键慢约 30%，`w1e` W1E-116）；
  材质比较下沉到材质对象一份 `compare()`（Qt `w2d` W2D-004/005）。
- 不透明批**不做重叠检查**（Qt，`w2d` W2D-009）；需要时报 overlapBounds 联合矩形剪枝。
- 区间化：分桶 + 8 路归并出连续区间（gpui `w1e` W1E-001/005/008）。
- 纹理图集：`comparisonKey = 图集指针`（Qt `w2d` W2D-037）；图集分配器从简（guillotine/BSP 够用）。
- 着色器：**构建期离线编译全部着色器**（Impeller 的 key 卖点，`w3f` W3F-050）——GPU 档的第一条纪律。

### 4.3 脏区与 damage（本设计对青简最大的升级）

青简现状是"每帧全画、无脏区"（`../renderer-qingjian/05` §5.1、`08` §8.4）——候选窗量级可接受，
**大面板不可接受**。本设计采用三层：

1. **内容版本号**：布局产物（DisplayList）按内容 hash 复用；内容不变 → 布局零工作
   （学 GN `SetText/Show` 分离 `w4a` W4A-042；反例=青简竖/横排 measure+draw 双整形 `w5a` W5A-064/065）。
2. **tile 内容描述符双缓冲比较**：逐 tile 比较描述符决定重栅格；damage 单调 union 成矩形；
   **空 damage 直接跳过合成**（WebRender `w1e` W1E-055/058/082 全套照抄口径）。
3. **树 diff（二级手段）**：结构变化大时用 Myers 风格差分求 damage（GSK `w2d` W2D-017）——
   在"候选行增删"这种粗粒度变化下其实 1 就能兜住，diff 留给复杂面板。
   落地形态（ADR G3）：**图块 union + 矩形数封顶（≤8，学 WPF 的贪心合并，`w2f` W2F-028..031）
   + 超限回退全窗**；Android 把 damage 做成一等公民（`w2f` W2F-061/062），
   slint 的"脏矩形上限 3"是更严的同族做法（`w1c` W1C-041/042）。
   **最强旁证（第三轮）**：Android 是唯一公开 damage API 的合成器——`eSurfaceDamageRegionChanged`
   + small dirty 门控 + `forceFullDamage`（`w3g` W3G-046/047/048）；Windows/Apple 的"局部提交"
   在公开面**不存在**（属平台自建能力，W3G-047/057）——这反证了在应用层自建 damage 的必要性。

### 4.4 合成与"省掉画"

- 铺满目标的**纯色吸收进 clear**；不透明内容自动 `SrcOver→Src`（Impeller `w2b` W2B-062/069）。
- `saveLayer` 透明度"窥孔"快路径（条件可枚举，不满足才建离屏，`w2b` W2B-070/072）。
- 阴影/圆角/模糊：全部按"内容尺寸不变即复用"缓存。

## 5. 文本与字体（对齐原生的完整清单）

1. **字体清单加载**（青简路线，`../renderer-qingjian/03` §3.7）：平台清单 + mmap 惰性读页，
   不扫系统目录（`w5a` W5A-023/024）；用户字体=族名+文件清单（壳查、渲染器载，W5A-040）。
2. **整形**：cosmic-text（0.19 起用 **harfrust**，`w1b`）；整形等级 Advanced（`w5a` W5A-054）。
3. **对齐系统排版的三件套**（缺一宽度就对不上，`docs/design/rendering.md:91`–`:96`）：
   - `opsz` 光学字号——**按字号分键**（修掉青简"全局单槽"限制，`w5a` W5A-050）；
   - `trak` 字距——按字形所用字体查表、逐点插值、坏表防 NaN（`w5a` W5A-036..039）；
   - 覆盖率 `gamma`——按极性/主题给值（浅 0.85/深 0.75，`w5a` 06.3）。
4. **字形缓存**：缓存键含 **4×4 子像素格** + 缩放档（GTK `w2d` W2D-020/036）；
   图集与字形位图二选一的判据：**窗口小、字形少（<2k）用 SwashCache 直存**（青简现状，零图集管理成本）；
   **字形多/跨帧稳定（大面板、编辑框）上 2D 图集**（Qt/GTK 路线）。
5. **亚像素**：默认灰度 AA（青简的明确决策：与 GDI/AppKit 并排无可感差异，`rendering.md:71/85`）；
   LCD 亚像素作为"可选档"——**启用条件比照 Chromium 的 tile 级否决清单**（`w2c` W2C 的 11 项枚举），
   而不是全窗一刀切。
6. **CJK 专项**：断行用 UAX#14 的 ID 类语义（`w3f` W3F-012）；竖排=字形替换表而非旋转
   （UAX#50 `w3f` W3F-017/018）；禁则"先挤进后推出"（clreq `w3f` W3F-040）；
   注音/标注不得与所注汉字分离（`w3f` W3F-041）。
7. **SDF/矢量字**：候选窗规模**不用**（青简评估与 GTK/Qt 的图集路线更稳，`../renderer-qingjian/09` §9.4）；
   大字号/极端缩放的 UI 再评估 msdf-atlas-gen（`w3f` W3F-005/007）。
   图集档参考 **Godot per-size shelf atlas**（单源 MSDF 服务全字号是另一档；`w2a` W2A-086..092/098/099）。
8. **实现纪律（第二轮回填）**：gamma 空间不做混合/缩放（`w1f` W1F-039）；亚像素启用须过三条硬约束
   （仅竖条纹 RGB LCD、纵向无 AA、FreeType 五点权重，W1F-048/067/074）；FreeType 版本号即渲染行为
   （2.7 默认 v40，W1F-069）；**整形栈对表项=parley**（Bevy 0.19 起已从 cosmic-text 换过去，
   `w4c` W4C-004/010）。

## 6. 布局与状态

- **布局内核**：自研"锚点 + 排列格式"单文件内核（覆盖候选窗/工具栏/列表）——形态参考 GN `sARRANGE_FMT`
  （`w4a` W4A-031..037）但**弃其 256×256 上限与全局状态机**，接口=纯函数：
  `layout(content, theme, constraints) -> DisplayList`（可单测、可缓存、可 diff）。
- **要不要 taffy**：**候选窗级不用**；大面板阶段"**慎用**"——taffy 自报宽树 10 万节点比基线慢 82%
  （`w1b`，README:119）；引入前必须先按我们的真实形态（宽而浅）复测（否证条件见 ADR-09）。
- **失效模型**（学 Slate + UGUI 的共识，`w2e` W2E-032..035、W2E-058）：
  三段缓存 **Hierarchy / Layout / Paint** 各自可失效；帧末统一重建；**Volatile 逃生阀**
  （高频元素按标记退出缓存，`w2e` W2E-064）；容器语义（Z/层号）进批键账（`w2e` W2E-065/066）。
- **状态与响应式**：信号层对照 Solid/Salsa 的"按需+记忆化"（`w3f` W3F-024..028）；
  我们只取**最小脏传播**（信号→订阅者标脏→汇总到 Layout/Paint 两级），不引入完整响应式运行时。

## 7. 平台壳、输入与 IME

- 契约（青简，`w5a`）：壳只做"贴图 + 回传坐标"；渲染器输出 `{pixmap(预乘 RGBA), content_rect, hit_table}`。
- Windows：`UpdateLayeredWindow`（BGRA 预乘，逐通道换序 `w5a` W5A-093/094）；DPI=`dpi/96`（W5A-092）；
  回退链（字体失败 → 系统绘制）照抄（W5A-091）。合成层官方背书：DComp 自述
  **"works with bitmap content only; it does not support vectors or text"**（`w2f` W2F-075）——
  系统合成路径与我们的位图契约同构。
- macOS：`NSBitmapImageRep`（8bpc/4ch/预乘，按行 stride 拷，`w5a` W5A-098/100/101）；系统阴影跟 alpha（W5A-097）。
- Linux：显示面在 Fcitx5/IBus（`docs/design/rendering.md:33`），壳不实现或只做无框架调试窗。
- IME：preedit/commit/delete 生命周期显式建模（wayland text-input-v3 的 `preedit_string/commit_string`
  事件序，`w3f` W3F-045..047；TSF/DWrite 侧同构）。
- **命中表**：最小集=矩形+字符区间（GN `sDRAW_RANGE.strid` 的现代版，`w4a` W4A-036）——
  候选窗只需要行命中，输入类组件才要字符级。

## 8. 性能预算（草案，全部进 `reports/09` 的协议测）

| 层 | 预算（400×500pt@2x） | 依据 |
| --- | --- | --- |
| 布局 | 内容不变 = 0；内容变 ≤ 一次整形量 | GN/青简三方互证 |
| 字形拉取 | 稳态命中率 ≥99%（4×4 子像素格缓存） | GTK 键设计 `w2d` |
| 光栅 | 只栅格 damage；全窗重栅格 ≤2 ms | 青简 1 ms 读数（同量级场景） |
| 阴影/装饰 | 尺寸不变 = 0（缓存） | 青简自留优化项 |
| 贴图 | 每帧 1 次整窗提交（面积=damage 时可局部） | Mineradio 的"每帧 1 层"教训镜像 |

## 9. 路线图

- **P0（位图契约）**：Frame→位图→贴图全链 + 宽度对表工具（复用青简判据 C2）。产出：候选窗等价物。
- **P1（damage + 显示列表）**：内容版本号 + tile 比较 + 空脏跳过（判据 C3）。产出：滚动面板 60fps。
- **P2（文本三件套 + 图集档）**：opsz 分键 / trak / gamma + 2D 图集 + 亚像素否决清单。
- **P3（GPU 档）**：wgpu 执行器 + SortKey 批 + 预编译着色器（判据新增：CPU 占用降低幅度）。
- **P4（大面板/复杂控件）**：树 diff、tearing-free 双缓冲、可访问性对接（平台侧）。

## 10. 未决与待测（不许当结论用）

1. taffy 在"宽而浅"真实面板上的实测（否证条件=比自研内核慢或内存高）。
2. GPU 档收益拐点（我们形态下 CPU 光栅 vs GPU 批量的交叉点）。
3. 竖排/CJK 断行在本项目产品里究竟是 P2 还是 P4。
4. 图集档的触发阈值（字形数/窗口面积），需实测出"图集管理成本 vs 命中收益"的交叉。
