# Bevy（https://github.com/bevyengine/bevy @ 52c3ec0d, 抓取 2026-10-01）

> 抓取：`git clone --depth 1 --filter=blob:none --sparse` + `sparse-checkout set crates/{bevy_ui,bevy_text,bevy_ui_render,bevy_render,bevy_core_pipeline,bevy_sprite,bevy_sprite_render,bevy_window,bevy_image,bevy_math,bevy_color,bevy_ui_widgets}`；
> 落盘 `D:/KF/LSSMJ/scratch/src/bevy`，commit = **52c3ec0d5ecec0cdf6f4d2fdfd0895267d64740f**（0.20.0-dev，edition 2024，rust-version 1.96）。
> 根 `Cargo.toml`/`README.md` 不在 sparse 工作区，经 `git show HEAD:` 快照到 `scratch/src/bevy-snapshot/`（报告/账本引文按该快照锚）。
> 账本：`docs/analysis/ledger/w4c.jsonl`（**134 条**，source 132 / doc 2，`verify` 0 拒绝）。下文 `file:line` 均相对仓库根。

## TL;DR（每条带锚）

1. **布局内核 = taffy 0.14（特性裁剪：std/block_layout/flexbox/grid/content_size/taffy_tree，default-features=false）**；为绕开 taffy `calc` 的线程不安全，Bevy 显式不用 calc 并做 `unsafe impl Send/Sync` 断言（`bevy_ui/Cargo.toml:40`，`bevy_ui/src/layout/ui_surface.rs:46`；W4C-006/014）。
2. **增量只做一半**：样式同步是节点级变更检测驱动（`node.is_changed()` 等五路闸门，`layout/mod.rs:151`）且已有节点只 `set_style` 不重建（`ui_surface.rs:116`）；但**每个 UI 根每帧无条件调 `compute_layout`**（`layout/mod.rs:272`），且 `ui_layout_system` 注册时无 run 条件（`bevy_ui/src/lib.rs:206`；W4C-017/026/029/033）。
3. **实体树↔taffy 树映射收在 `UiSurface` 资源**：`entity_to_taffy` + `root_entity_to_viewport_node`；每个根配一个隐式 viewport 节点（Grid、100%×100%）（`ui_surface.rs:67-75`、`:204-219`；W4C-015/016/019/020）。
4. **文本栈已从 cosmic-text 换成 parley 0.11**（官方 0.19 发布说明："we've chosen to migrate to parley during this cycle"；依赖见 `bevy_text/Cargo.toml:44`），字形光栅仍是 **swash 0.2.6**（`:45`；W4C-004/010/011）——与青简（cosmic-text+swash）在整形/排版层分叉。
5. **图集键 = (字体 id, index, 字号 f32 位模式, 变体哈希, hinting, smoothing) 六元组**（`font_atlas_set.rs:12`、`pipeline.rs:379-386`），字号不取整（`pipeline.rs:382`）——每个 (字体,字号) 组合都新建图集，官方文档自认"强性能影响"（`text.rs:718`；W4C-052/053/069/070）。
6. **字形缓存键只有 `glyph_id`（无子像素偏移档）**，但同一文件文档还写着"子像素偏移分组进 bin"——文档/代码漂移；亚像素 AA 有明确 TODO 未实现（`font_atlas.rs:15`、`:26-30`、`text.rs:1547`、`:1565`；W4C-057/058/071/072）——与青简/GTK 的 4×4 子像素格路线相反。
7. **UI 渲染只有一种阶段：已排序的 `TransparentUi`，排序键 = 栈号 + 部件偏移（f32）**，偏移表 `stack_z_offsets`（阴影 -0.1 / 背景 0 / 边框 0.01 / 图 0.04 / 材质 0.05 / 文字 0.06 / 光标 0.08），用**稳定排序**（`render_pass.rs:78/134`、`lib.rs:117-130`；W4C-078/079/080/081）。
8. **UI 批 = 相邻同图合并，批结构仅 (顶点区间, image)**；顶点/索引每帧重写、批每帧重建（`lib.rs:2180-2184`、`:2322-2400`、`:2589`；W4C-089/091/093）。3D 侧的二元桶键/多绘在 UI 不适用。
9. **UI 是独立子视图（subview=1）**：正交投影原点在左上、far=1000、相机 z=999.9；画进**未采样颜色附件**、无深度；位置在 Core2d/Core3d 的 `PostProcess` 之后、`upscaling` 之前（`lib.rs:1364-1375`、`:1446-1470`、`render_pass.rs:56-63`、`lib.rs:296-303`；W4C-083/086/087/126）。
10. **渲染图已被 ECS schedule 取代**（0.19 起）：`RenderGraph`(Schedule) → `camera_driver` → 每相机 `Core2d`/`Core3d` schedule；相机按 `(order, target)` 排序、同 order 同 target 告警（`renderer/mod.rs:32-48`、`schedule.rs:136-240`、`camera.rs:805-860`；W4C-005/103/104/105/107）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| "栈号 + 部件偏移"单一排序账（阴影/背景/边框/图/文字/光标共用一阶段） | `bevy_ui_render/src/lib.rs:117-130`（W4C-080） | **吸收**：与我们 DisplayList 的"每项带命中所需一切"同构，排序键一维化 |
| 稳定排序 + 相邻同图批合并（UI/Sprite 同构） | `render_pass.rs:134`（W4C-079）、`bevy_sprite_render/src/render/mod.rs:428`（W4C-121） | **吸收**：候选窗量级最省实现；区间归并必须保序 |
| 取整/未取整双读数（布局用取整、缓存/动画用未取整） | `ui_surface.rs:319-335`（W4C-023） | **吸收**：直接抄进我们的 layout→DisplayList |
| 每帧零大分配：TextPipeline 复用缓冲、TextLayoutInfo::clear 保容量 | `pipeline.rs:36-43`（W4C-046）、`:519`（W4C-055） | **吸收**：我们的 C6 判据 |
| 写前比较（几何/变换/字号全先比后写） | `layout/mod.rs:361/410`（W4C-030/031） | **吸收**：避免变更放大 |
| 危险参数显式告警（字号>1000px） | `pipeline.rs:148`（W4C-047） | **吸收**：不许静默降级（tiny-skia 反面教材的反面） |
| 字图集分层 `HashMap<Key, Vec<FontAtlas>>` + 键 `to_bits` 浮点写法 | `font_atlas_set.rs:12/29`（W4C-062/063） | **吸收**（键的粒度要改，见下） |
| 字体双注册（内嵌族名 + asset_id 别名） | `font.rs:84-102`（W4C-067） | **吸收**：壳查清单/渲染器载的双寻址 |
| 文本度量两级：min-content → max-content + BorderBox 内缩 | `pipeline.rs:317-347`、`widget/text.rs:219-248`（W4C-051/039/040） | **吸收**：对齐原生宽度的关键式（青简 C2 上游实现） |
| 空阶段直通（UI 无项不开通道；无 clip 不裁剪） | `render_pass.rs:49`、`clipping.rs:26`（W4C-077/095） | **吸收**：零脏零成本 |
| taffy 作为布局内核 | `bevy_ui/Cargo.toml:40`、`ui_surface.rs:44-52` | **不吸收（候选窗级）**：与 ADR-09 一致；Bevy 的用法（特性裁剪/content_size 回调/不用 calc）留作大面板阶段复测的参照 |
| parley 0.11 + FontCx（排版库托管字体库） | `parley_context.rs:37`、`bevy_text/Cargo.toml:44`（W4C-074） | **有界吸收**：我们按青简路线用 cosmic-text（时间戳结论）；parley/fontique 作为"字体清单层替代"候选记录 |
| 多相机/子视图合成（UI 子视图 1、相机 order 排序） | `lib.rs:1375`、`camera.rs:821`（W4C-087/107） | **有界吸收**：我们单窗+overlay，只需"合成顺序显式"这一条 |
| 每帧全树 compute + 全量重写顶点缓冲 + 批每帧重建 | `layout/mod.rs:272`、`lib.rs:2166/2589`（W4C-029/088/093） | **不吸收**：正是我们 damage/tile 比较要替换掉的形态 |
| UiMaterial 管线无批处理（注释自认） | `ui_material_pipeline.rs:110`（W4C-098） | **不吸收（反面）**：我们 GPU 档必须建批 |
| 无子像素 AA、无子像素偏移字形缓存 | `text.rs:1547/1565`、`font_atlas.rs:15`（W4C-071/072/057） | **不吸收**：我们保留 4×4 子像素格 + 灰度 AA，锐度优先级更高 |
| RenderGraph-as-schedule 大改 | `renderer/mod.rs:48`（W4C-104） | **不吸收（架构不匹配）**：我们不是 ECS 渲染器；只借其"相机驱动子计划"的调度形状 |

## 1. 架构全景（模块地图）

本检出把 Bevy 拆成了更细的 crate（未在 `targets.md` 预告）：`bevy_ui`（布局/组件/系统）与 `bevy_ui_render`（渲染）分离，`bevy_sprite` 与 `bevy_sprite_render` 分离；`bevy_render` 已委托 `bevy_camera`/`bevy_material`/`bevy_mesh`/`bevy_shader`/`bevy_extract` 等。

| 模块 | 职责 | 关键文件（行数） |
| --- | --- | --- |
| `crates/bevy_ui` | UI 组件/布局/焦点/拾取/无障碍/栈序/文本测量；28 文件 ≈16.5k 行 | `layout/mod.rs`(2937)、`layout/ui_surface.rs`(592)、`layout/convert.rs`(778)、`ui_node.rs`(3821)、`widget/text.rs`(475)、`stack.rs`(470)、`measurement.rs`(164) |
| `crates/bevy_text` | 文本栈：字体资产、parley 集成、TextPipeline、字形图集、编辑/滚动 | `text.rs`(1782)、`pipeline.rs`(635)、`scroll.rs`(831)、`font_atlas.rs`(318)、`parley_context.rs`(174)、`font.rs`(414) |
| `crates/bevy_ui_render` | UI 提取/排队/批/通道/管线；盒阴影/渐变/九宫格/材质 | `lib.rs`(2599)、`gradient.rs`(1062)、`ui_texture_slice_pipeline.rs`(871)、`ui_material_pipeline.rs`(741)、`box_shadow.rs`(638)、`render_pass.rs`(244)、`clipping.rs`(199) |
| `crates/bevy_render` | 渲染世界/提取/阶段/批框架/视图/资源 | `batching/gpu_preprocessing.rs`(3198)、`render_phase/mod.rs`(2833)、`camera.rs`(1533)、`view/mod.rs`(1523)、`batching/mod.rs`(244)、`renderer/mod.rs`(388) |
| `crates/bevy_core_pipeline` | 2D/3D 相机 schedule 与 pass 编排 | `core_3d/mod.rs`(1054)、`core_2d/mod.rs`(468)、`schedule.rs`(340) |
| `crates/bevy_sprite(_render)` | Sprite 与 Text2d 的提取/批/渲染（与 UI 对照） | `text2d/mod.rs`、`render/mod.rs` |

## 2. 关键机制

### 2.1 布局（taffy 集成；对表轴"布局增量"）

- **资源与映射**：`UiSurface`（Resource）= `entity_to_taffy: EntityHashMap<LayoutNode>` + `root_entity_to_viewport_node` + `taffy: UiTree<NodeMeasure>` + scratch 缓冲；`LayoutNode` 带可选 `viewport_id`（根节点才有）（`ui_surface.rs:20-33`、`:67-75`）。
- **隐式 viewport 节点**：每个 UI 根在 taffy 里包一层 Grid（`size=percent(1.0)`、`align/justify=START`），把"根节点百分比/绝对定位"语义固定在容器上（`ui_surface.rs:192-223`）。
- **增量路径**：`ui_layout_system` 每帧跑；对每个节点做五路变更检测（渲染目标/Node/内容尺寸/rem/em）后 `upsert_node`——已存在节点仅 `set_style`（可选更新 state/context），不存在才 `new_leaf[_with_context]`；子级仅在其自身变化（或新增/幽灵/固定节点变化）时 `set_children`（`layout/mod.rs:149-170`、`:216-256`、`ui_surface.rs:116-175`）。
- **计算与回写**：每根每帧 `compute_layout_with_measure(viewport_node, Definite(物理尺寸), 测量闭包)`；随后 `update_uinode_geometry_recursive` 递归读 `taffy.layout(id)`（rounding 开关逐节点可控）写 `ComputedNode`（size/未取整 size/content/border/padding/滚动条）+ 全局变换（先比后写）。滚动位置在 clamp 后 `floor()`（`layout/mod.rs:272-293`、`:296-521`、`ui_surface.rs:226-287`）。
- **测量回调**：taffy 的 measure 闭包经 `get_text_buffer`（`needs_buffer` 为前提，否则跳过查询）拿到 `ComputedTextBlock`，调 `NodeMeasure::measure`；`NodeMeasure` 是枚举（Fixed/Text/Image/Custom）以避免装箱（`ui_surface.rs:240-287`、`:409-425`、`measurement.rs:97-135`）。
- **文本测量两级闸门**：度量在 `measure_text_system`（`create_text_measure`，重排一次断行得 min/max 内容宽），布局再生在 `text_system`（`update_text_layout_info`）；NoWrap 文本换 `FixedMeasure` 缓存、颜色变化会误触重测（文档明说"代价高、建议 bypass"）（`widget/text.rs:289-382`、`:392-460`、pipeline 注释 `lssmj: see W4C-038..043`）。
- **taffy 用法小结（供我们决策表）**：特性只开 std/block_layout/flexbox/grid/content_size/taffy_tree；不用 calc（线程安全）；节点级增量同步；每帧全量 compute 依赖库内缓存；测量函数挂在叶节点 context 上；rounding 逐节点可控且可"取两次"。

### 2.2 文本（parley + swash；对表轴"文本缓存与图集"）

- **资源**：`FontCx`(parley::FontContext，含 generic family 备份)、`LayoutCx`(LayoutContext<TextBrush>)、`ScaleCx`(swash ScaleContext)、`TextPipeline`（sections_buffer/text_buffer 复用）、`FontAtlasSet`、`RemSize`/`TextIterScratch` 等（`parley_context.rs:33-43`、`:168-175`、`bevy_text/src/lib.rs:120-129`）。
- **整形**：`LayoutCx.ranged_builder(text, scale_factor, true)` → 逐段 push 属性（family/字号/行高/字距/粗细/宽/斜体/features/variations/**brush**）→ `build_into`；断行策略映射 parley 三枚举（AnyCharacter→WordBreak::BreakAll；WordOrCharacter→OverflowWrap::Anywhere；NoWrap→TextWrapMode::NoWrap；WordBoundary→WordBreak::Normal）（`pipeline.rs:196-289`）。
- **文本属性载体**：`TextBrush{section_index, font_smoothing}` 存在 parley 布局里，渲染按 run 拿 AA 模式（`parley_context.rs:11-31`）。
- **字形生成**：遍历 `layout.lines()/items()`；按 run 生成 `FontAtlasKey`（字体 id/index/字号位/变体哈希/hinting/smoothing）；未命中即用 swash `Scaler`（每 run 建一次、可按 hinting 开提示）光栅成 Alpha 图集块；位置 = 块尺寸/2 + 字形位置 + 图集 offset（`FontSmoothing::None` 时先 floor 且覆盖率二值化 127 阈值）（`pipeline.rs:350-483`、`font_atlas.rs:138-260`）。
- **图集管理**：`FontAtlas` = `DynamicTextureAtlasBuilder` + `texture_atlas` + `glyph_to_atlas_index: HashMap<GlyphCacheKey, GlyphAtlasLocation>`；`add_glyph_to_atlas` 依次尝试已有图集，全失败则新建（尺寸=字形最大边+2×padding 上取整 2 次幂、下限 512；padding=2）（`font_atlas.rs:11`、`:32-41`、`:138-188`）。`FontAtlasSet.total_bytes()` 提供图集内存统计（`font_atlas_set.rs:39`，W4C-130）。
- **命中率把手**：`GlyphCacheKey.glyph_id` 单字段 ⇒ 同字形跨子像素位置复用同一光栅（无子像素档）；键的粒度=内存预算（对照我们 4×4 格）。
- **Text2d 分叉**：世界空间文本仍用同一 TextPipeline/FontAtlasSet，但提取期逐字形产生 `ExtractedSprite`，走 Sprite 批（与 UI 的 `ExtractedUiItem::Glyphs` 分叉）（`bevy_sprite_render/src/text2d/mod.rs:24-100`、W4C-124）。

### 2.3 UI 渲染（提取/排队/批/通道；对表轴"UI 批处理键"）

- **提取是增量的**：`ExtractedUiNodes{uinodes: 主实体→(相机, 子项表), changed}`；`extract_uinode_changes` 用 `Changed<..>`（ComputedNode/StackIndex/UiGlobalTransform/Visibility/Clip/目标相机/颜色…）与 `RemovedComponents` 维护失效；各提取系统只扫 `changed`（`lib.rs:403-417`、`:439-505`、`text.rs:73-77`）。
- **z_order 在提取期算好**：`stack_index as f32 + stack_z_offsets::*`（背景 0、边框 0.01、渐变 0.02/0.03、图 0.04、材质 0.05、文字 0.06、光标 0.08、阴影 -0.1、选区 0.055）（`lib.rs:117-130`、`:2260`、`text.rs:167/202`、`box_shadow.rs:454`、`ui_texture_slice_pipeline.rs:466`）。
- **栈序来源**：`ui_stack_system` 每帧重建 back-to-front 列表（根按 GlobalZIndex/ZIndex 排序、子树内局部 Z 排序），写 `ComputedStackIndex`；同分区=同相机（`stack.rs:52-160`，W4C-131）。
- **排队与排序**：`queue_uinodes` 把每个提取项作为 **transient** 项加入该视图的 `SortedRenderPhase<TransparentUi>`；`sort_phase_system::<TransparentUi>` 用**稳定排序**按 z_order 排（`render_pass.rs:124-148`、`lib.rs:2206-2268`、`render_phase/mod.rs:2284-2297`）。
- **批**：`prepare_uinodes` 线性扫排序后的 items，相邻项 `image` 相同才续批；`UiBatch{range, image}`；无纹理项（default image）可被后续纹理项"升级"（因未纹理项不采样）；顶点/索引写入全局 `RawBufferVec`，批每帧清空重建（`lib.rs:2275-2599`）。
- **绘制**：单通道 "ui"（颜色附件=**未采样**目标、无深度），每批 `draw_indexed(range)`；管线固定 ALPHA_BLENDING、按 `UiAntiAlias` 生成 `ANTI_ALIAS` shader_def；顶点自带 flags 与圆角/边框/中心点参数，圆角与边框 AA 在片元做（`render_pass.rs:24-75/208-244`、`pipeline.rs:48-108`、`lib.rs:2145`；W4C-128）。
- **扩展部件**：盒阴影（采样数默认 4、>~10 收益递减；同阶段、键=栈号-0.1）、渐变（shader flags RADIAL/FILL/CONIC，W4C-129）、九宫格切片、UiMaterial（**无批**，注释自认）全部并进同一 sorted phase（`box_shadow.rs:181-206`、`lib.rs:2186-2204`、`ui_material_pipeline.rs:110`）。

### 2.4 render graph 与视图排序（对表轴"与 3D 管线的关系"）

- **结构**：`RenderGraph`(Schedule，含 Begin/Render/Submit/Finish 集合) → `camera_driver`(System) 逐根视图 `run_schedule`；根视图=相机（按 `SortedCameras` 排序）或非相机辅助视图（阴影贴图，W4C-132）；相机 schedule = `CameraRenderGraph`（`Core2d`/`Core3d`/自定义）（`renderer/mod.rs:32-64/79-138`、`schedule.rs:128-268`）。
- **相机排序**：`sort_cameras` 按 `(order, target)` 排序；同 order 同 target 记歧义并 `warn_once`；同 target 相机自底向上编号（底部替换、其上 alpha 混合）（`camera.rs:794-860`）。
- **阶段与批（3D 侧）**：`BinnedRenderPhase`（multidrawable/batchable/unbatchable/non_mesh 四桶，键=(BatchSetKey, BinKey)）、`SortedRenderPhase`（IndexMap+transient，默认不稳定排序）；批元 `BatchSetMeta=(pipeline, draw_function, dynamic_offset, user_data)`；合并=归约（相同则扩区间）；`Render` 计划集合顺序 Extract→…→Queue→PhaseSort→Prepare→Render→Cleanup（`render_phase/mod.rs:109-170/1893-1947/2189-2297`、`batching/mod.rs:42/198-244`、`bevy_render/src/lib.rs:327-347`）。
- **UI 与 3D 的关系**：UI 不在 3D 相机的 pass 里"排队"，而是同一相机派生的**第二个视图**（subview=1），有自己的 phase 账；`ui_pass` 在 Core2d/Core3d 的 PostProcess 之后、upscaling 之前执行 ⇒ UI **不经过色调映射/MSAA**，直接写主通道颜色附件（`lib.rs:1370-1375/1406-1541`、`render_pass.rs:56-63`、`lib.rs:296-303`）。相机 `order` 决定"哪个相机（连带其 UI）后画"；`ComputedUiTargetCamera` 决定 UI 挂到哪个相机（映射可缓存，W4C-133）。

## 3. 对比轴（我方决策对表）

| 轴 | Bevy 现状（锚） | 对我们的判据 |
| --- | --- | --- |
| **布局增量** | 节点级变更检测→taffy set_style；每根每帧全量 compute；几何回写先比后写；无全局脏闸门（W4C-026/029/030/033） | 学"写前比较"与"节点级同步"；**不学**无脏闸门——我们的 C3 判据（空 damage 零重画）要求总闸门 |
| **文本缓存与图集** | 键=(字体,字号位,变体,hinting,AA) + glyph_id 单键；每组合新图集；无子像素档/无亚像素 AA（W4C-052/053/057/071/072） | 我们的键=4×4 子像素格+缩放档：**比 Bevy 更重要**（锐度）；但要显式钉"缩放档"防止字号连续变化爆图集（Bevy 的坑正相反） |
| **UI 批处理键** | 单键（image）；相邻合并；每帧重建；材质路径无批（W4C-089/091/093/098） | 候选窗级：直接采用"保序+相邻同图"；大面板：升级为键聚类（Qt/GTK 路线，W2D 对照） |
| **与 3D 管线关系** | 独立子视图+独立 sorted phase；后处理之后合成、无 MSAA/无色调映射（W4C-083/087/126/127） | 对应我们"显示面叠加在宿主内容之上"的壳契约；位图契约天然无 MSAA |
| **已知性能坑** | 见下节（自留 TODO/文档警告/全量重建点） | 作为"否证清单"逐条对照我们的 P1/P2 |

## 4. 性能手段与公开读数

**本事实现手段（带锚）**

1. 复用缓冲：`TextPipeline` 的 sections/text 缓冲、`TextLayoutInfo::clear` 保容量（W4C-046/055）。
2. 查询前置判断省查询：`needs_buffer` + `get_text_buffer`（W4C-024/038）。
3. 双读数取整策略：rounding 开/关各读一次（W4C-023）。
4. 两级文本缓存：measure → fixed-measure（NoWrap）/text-measure（W4C-039/042/043）。
5. 快路径直通：无 clip 不裁剪、空 phase 不开通道（W4C-077/095）。
6. 增量提取：`changed` 集合 + 失效清理（W4C-084/085）。
7. 3D 侧先分类后排序（四桶）、批合并=归约（W4C-116/117）。
8. 质量拐点文档：阴影采样默认 4、>~10 收益递减（W4C-082）。

**公开读数**：本检出树内**未发现**可复算的基准数字（快照 README 131 行无性能/基准章节，W4C-134；未跑 cargo bench，纪律禁止）。唯一带"量化口径"的公开表述是文档中的拐点与告警（字号>1000px、阴影采样 >~10、图集键组合爆炸的定性警告）——全部按定性留档，**不得当数字结论引用**。外部发布说明亦未给出 UI/文本帧时数据（0.19/0.20 新闻页已抓，未含性能数字）。

## 5. 坑与反例（负面留档）

1. **`needs_rerender` 粒度不足（作者自留 TODO）**：结构变化（字号/对齐）与非结构变化（颜色/平滑）混在一个标志位，UI 会因此无谓重测；官方注释建议拆组件做成本收益分析（`text.rs:53-57`，W4C-068）。
2. **颜色变化触发重测**：`measure_text_system` 的文档警告"对大字块代价高"，建议调用方 `bypass_change_detection`（`widget/text.rs:286-288`，W4C-040）——把性能责任推给调用方，是设计味道。
3. **图集组合爆炸**：字体句柄 × 缩放字号每个组合新图集；字号以 f32 位模式进键且**不取整**（文档却说"取整到最近像素"）——文档/代码漂移 + 缩放缓变会持续新建图集（W4C-053/069/070）。
4. **同族漂移第二例**：`font_atlas.rs` 文档描述"子像素偏移分箱"，键中已无偏移字段；亚像素 AA 仍是 TODO（W4C-057/058/071/072）。
5. **批每帧全量重建**：`clear_batches` 注释直言"每帧从头重建"；顶点/索引整体重写（W4C-088/093）——大 UI 的 CPU 成本随节点线性增长。
6. **UiMaterial 无批**：自研材质路径 `UiMaterialBatch ≈ 单次 draw call`（W4C-098）——扩展点即性能悬崖。
7. **CPU 多边形裁剪**：每项按 Sutherland-Hodgman 裁剪并插值（旋转/多边形 clip 的通用代价）（W4C-094）。
8. **字体移除全量重建**：任一字体资产移除→清空字体集合、重映射 generic family、全量标脏 TextFont（W4C-066）。
9. **taffy calc 线程不安全**：Bevy 选择不用 calc 以保 Send/Sync（W4C-014）——"依赖特性会反噬并发设计"的实例。
10. **2D 不能 multidraw**：注释直认 2D batch set key 无意义（W4C-120）——别把 3D 多绘机制当默认模板。

## 6. 未验证项（缺什么证据）

1. **帧时数字**：未测（禁止 cargo bench）；需要"节点数 × 帧时 / 布局增量收益 / 批重建成本"三组读数才能支撑我们抄/不抄的定量判据。
2. **taffy 0.14 在"宽而浅"形态的实测**：Bevy 的用法（Grid 根 + flex/grid 混合）与我们的形态不同；ADR-09 否证条件仍未满足。
3. **parley vs cosmic-text 的宽度/字形差异**：未做同源文本对表（我们的 C2 判据必须先过这道）。
4. **`ui_layout_system` 每帧全量 compute 的真实代价**：未测；无法判断"无 run 条件"在 1k/10k 节点下的实际损失。
5. **字形图集键爆炸的实际阈值**：未测（连续缩放场景）。
6. **`gradient.rs`/`ui_material.rs` 的着色器细节**：本波只读到管线与批层面，未逐行读 wesl 数学（`ui.wesl` 只读了头 100 行）。
