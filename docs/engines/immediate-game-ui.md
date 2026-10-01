# 即时模式与游戏 UI 群（Dear ImGui / Nuklear / RmlUi / Fyrox / O3DE LyShine / Cocos2d-x / microui）

上游（全部浅克隆到 `D:/KF/LSSMJ/scratch/src/<slug>`，`git rev-parse HEAD` 2026-10-01 实取）：

| 目标 | 仓 | commit | 备注 |
| --- | --- | --- | --- |
| Dear ImGui | ocornut/imgui | `3f00c0d0f414d45af2a0979b925ebf2e8f09936b` | 版本串 `1.93.0 WIP` / 号 19297（W4D-001）；`imgui.cpp` 18707 行 |
| Nuklear | Immediate-Mode-UI/Nuklear | `9f7750296f176e506c2b24ff55bc24495e2db750` | 单头发布 + `src/` 分文件（生成 `nuklear.h`） |
| RmlUi | mikke89/RmlUi | `3045e6e3510425ef2870f7647b3f59d3ae9970f5` | HTML/CSS 子集，保留式 |
| Fyrox | FyroxEngine/Fyrox | `a445c62352682747be85f17e2cda8331a544ad44` | sparse：`fyrox-ui`（+UI 渲染器在 `fyrox-impl/src/renderer/ui_renderer.rs`，本次未深读） |
| O3DE LyShine | o3de/o3de | `28872cbb42b4a140f92bf42d75850065135267ff` | sparse：`Gems/LyShine`（128 文件） |
| Cocos2d-x | cocos2d/ cocos2d-x | `7a5282a301a13e467bdcc3466d3cd3883862aadf` | sparse：`cocos/{ui,2d,renderer,platform}` |
| microui | rxi/microui | `0850aba860959c3e75fb3e97120ca92957f9d057` | 1208 行 `.c` + 296 行 `.h`，最小参照 |

*未做*：Stride、Flax（见 §5 未验证项；任务允许降为 doc 级，但分支/路径未在预算内命中，宁缺勿假）。

## TL;DR（每条带锚；锚=账本 id）

1. **ImGui 的帧契约是"指针数组 + 三个视口参数"**：`ImDrawData.CmdLists` 只是指向由 context 拥有的 `ImDrawList` 的指针，帧产物不拷贝顶点（W4D-002、W4D-003）。单条 `ImDrawCmd` 的最小状态集 = 裁剪矩形 + 纹理引用 + 顶点/索引偏移 + 索引数（W4D-004），且构造时整块 `memset`，因为后续用 memcmp 比头判合并（W4D-005）。
2. **命令生成是惰性的**：`AddLine/AddRect` 这类热路径零检查（栈上永远有一条待填命令），"该不该新开命令/能否合并"的判断全部推迟到 `_OnChanged*`（W4D-006）；合并判据是"命令头相等 **且** 索引连续"两条同时成立（W4D-007）。
3. **文本渲染用"最坏情况预留 + 事后归还"**：按剩余字符数×4 顶点一次 `PrimReserve`，画完 `PrimUnreserve` 收缩（W4D-010、W4D-011）。
4. **ImGui 1.92 是一次协议换代**：字体变成按尺寸动态烘焙（`ImFontBaked`，"a font may be rendered as any size"）（W4D-013），配套"纹理局部更新协议"（WantCreate/WantUpdates/WantDestroy/OK + Updates[] 列表）（W4D-018）；代价是**纹理可在帧内任意时刻变化，预存 UV 失效**（W4D-019）。
5. **字形查找必须 O(1)**：ImGui 用按码点直接索引的稀疏 `IndexLookup` + 专供测宽的 `IndexAdvanceX`（W4D-014、W4D-015）；Nuklear 反过来——`nk_font_find_glyph` 每次查询线性扫所有码点区间（W4D-035），这是**反面教材**。
6. **Nuklear 把命令与顶点写进同一条 `nk_buffer`**：首条命令记 `cmd_offset`，之后靠"缓冲总长−偏移"反查命令首址（W4D-027）；状态变化时若上一条命令还没吃到顶点就**改写它**而不是新增（W4D-028），纹理同理（W4D-029）。
7. **RmlUi 是彻底的保留式**：几何在 `RenderManager` 的 StableVector 里惰性编译（`CompileGeometry` 只在 handle 为空时调用）（W4D-038、W4D-039），背景/边框按类型缓存 `Geometry` 并只在 `background_dirty/border_dirty` 时重生成（W4D-042），文本是"逐字符查 `character_boxes` → 生成一个四边形"（W4D-044）。
8. **Fyrox 的量-排-视三级有效位**：`measure_valid / arrange_valid / visual_valid` + `prev_measure/prev_arrange`（W4D-059、W4D-061），官方注释直接警告"每帧对多个 widget 调 invalidate_layout **会**造成严重性能问题"（W4D-058）。
9. **LyShine 把绘制图当缓存资产**：`RenderGraph` 只在 `GetDirtyFlag()` 时重建（W4D-067），节点粒度=一次渲染状态区间（"allocate a render node for each change in render state"，W4D-062），每个节点自持合并顶点/索引缓冲（W4D-063），单节点纹理上限 16（W4D-064）。
10. **Cocos2d-x 的三处粗粒度省法**：批合并只看**相邻**命令的 `MaterialID`（W4D-073、W4D-076）；布局靠 `_doLayoutDirty` 整树早退、另留 `forceDoLayout` 逃生阀（W4D-077、W4D-078）；字形页是行式货架装箱 + 直接写 CPU 页缓冲再按页脏上传（W4D-079、W4D-080），且只栅格"本帧新出现的字符"（W4D-081）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"，即 `../lssmj-design/README.md` 的目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 命令=「裁剪矩形+纹理+索引区间」最小状态集（批键全集） | W4D-004、W4D-026 | **吸收**：直接作为我们 DisplayList→绘制项的字段定义（设计 §3/§4.2 的 SortKey 先只放这三项） |
| 状态变化时「改写尾部空命令」而非新增命令 | W4D-028、W4D-029 | **吸收**：候选窗高频 PushClip/PushColor，成本可忽略 |
| 合并判据=「状态相等 + 索引连续」 | W4D-007 | **吸收**：比"仅状态相等"更严，避免跨段错误合并 |
| 最坏情况预留 + `PrimUnreserve` 归还 | W4D-010、W4D-011 | **吸收**：支撑设计 §3 的 C6「每帧零大分配」 |
| 按码点索引的稀疏字形表 + 独立 advance 表 | W4D-015、W4D-014 | **吸收**：直接服务判据 C2（宽度对表）与复用 `trace_families` |
| 图集页的「本地脏区间上传」协议 | W4D-018、W4D-080、W4D-019 | **吸收（有界）**：先做单页 + 区域上传，多页/多纹理列表留到 P3 |
| 白像素 UV 把纯色图元并入同一纹理批 | W4D-016 | **吸收**：候选窗底色/分隔线/圆角可共批 |
| 1px 线用预烘焙线纹理代替逐段三角带 | W4D-012 | **吸收（有界）**：只对 1px 整数宽度线生效，需图集留线位 |
| 位标志级失效（geometry_dirty 1 bit / dirty_definition 隐含子树） | W4D-043、W4D-048 | **吸收**：对应设计 §6 的「三段缓存各自失效」，但先做整树版本号（Cocos 路线） |
| `RenderData/Command` 带包围盒 + 可选裁剪几何 | W4D-051、W4D-052 | **有界吸收**：矩形 scissor 必做；任意形状裁剪（模板/几何）留到圆角+异形遮罩阶段 |
| 布局异常入口单点化 + 重入保护（渲染中拒绝置脏） | W4D-068、W4D-066 | **吸收**：设计 §6「最小脏传播」的可审计前提 |
| 着色器按状态组合预置变体 | W4D-070 | **吸收**：与设计 §4.2「构建期离线编译全部着色器」同一条纪律 |
| UI 走引擎现成的动态绘制通道 | W4D-071 | **吸收**：我们 GPU 档挂在 wgpu 一条动态通道上，不接管管线 |
| 每窗口一份 DrawList、不做跨窗归并 | W4D-023、W4D-022 | **不吸收**：我们是单窗口/单显示面，一份缓冲更省（该条用于解释 ImGui 为何多列表） |
| 命令与顶点写同一条缓冲、靠长度反查命令首址 | W4D-027 | **不吸收**：可读性差且我们在 Rust 里可零成本用两个 Vec + range |
| 字形查询线性扫码点区间（Nuklear） | W4D-035 | **不吸收（反面）**：CJK 下每次查询 O(区间数)，我们一律索引表 |
| 16 位索引溢出靠 Debug 断言兜底 | W4D-032、W4D-031 | **不吸收（反面）**：容量约束必须在初始化/编译期暴露，发布路径另记账 |
| 预生成「码点区间」整套烘焙 | W4D-036 | **不吸收**：CJK 全量不现实；我们按需增量（W4D-013 路线） |
| 逐帧重排 + 逐顶点 memcpy 进大缓冲（Cocos） | W4D-075、W4D-074 | **不吸收为主**：设计 §4.3 用内容版本号+tile 比较消掉它；仅 GPU 档可选保留 |
| 不做完整 CSS/HTML 合规、只做按需子集 | W4D-049 | **吸收**：候选窗不需要完整 CSS 语义，按需子集即可（RmlUi 自陈同款取舍） |
| 把整棵 UI 布局做「整树假/真」粗粒度失效位 | W4D-077 | **吸收（P1 起步档）**：候选窗量级够用；复杂面板再上 tile/树 diff |
| 绘制图整体重建（脏即 Reset）而非增量修补 | W4D-066 | **有界吸收**：先解决悬垂指针（先清再建），增量修补留到实测出瓶颈后 |

## 1. 架构全景（模块地图，逐文件职责）

### 1.1 Dear ImGui（`imgui.cpp` 18707 行 / `imgui_draw.cpp` 6878 行 / `imgui.h` 4326 行）

- `imgui.h`：全部公开结构。绘制侧 `ImDrawCmd`(3233)、`ImDrawVert`(3254)、`ImDrawListSplitter`(3285)、`ImDrawList`(3348)、`ImDrawData`(3518)；字体侧 `ImFontAtlas(3751)/ImFontBaked(3904)/ImFont(3953)`。
- `imgui_draw.cpp`：绘制列表实现（`AddDrawCmd` 505、`_TryMergeDrawCmds` 577、`_OnChangedClipRect/_OnChangedTexture/_OnChangedVtxOffset` 591/613/638、`PushClipRect` 666、`PrimReserve` 733、图元与小部件绘制、`ImFont::RenderText` 5868、图集烘焙）。
- `imgui.cpp`：帧生命周期、窗口系统与命中测试（`ItemHoverable` 5058、`IsMouseHoveringRect` 10228）、`Render()` 装配 `ImDrawData`（`AddWindowToDrawData` 5955）、`ImGuiListClipper`。
- `imgui_internal.h`：内部结构，含 `ImDrawDataBuilder`（931，两层：regular / tooltip）。
- `docs/CHANGELOG.txt`、`docs/FAQ.md`、`docs/FONTS.md`：换代的官方口径（本报告用到 1.92 纹理协议与 FAQ 对照表）。

### 1.2 Nuklear（`src/` 分文件 → 生成 `nuklear.h` 5920 行）

- `src/nuklear_vertex.c`（1340 行）：绘制列表与转换器（`nk_draw_list_add_clip` 175、`nk_draw_list_push_image` 189、`nk_draw_list_alloc_vertices` 219、`nk_convert` 1181）。
- `src/nuklear_font.c`：字体烘焙与查询（`nk_font_bake_pack` 163、`nk_font_atlas_end` 1289、`nk_font_find_glyph` 507）。
- `src/nuklear_buffer.c`、`src/nuklear_pool.c`：两侧生长的缓冲与池（本报告只用到缓冲的类型/标记概念）。
- `src/nuklear_window.c` / `src/nuklear_panel.c` / `src/nuklear_layout.c`：保留窗口记录 + 立即式 widget。

### 1.3 RmlUi（`Source/Core` ~200 文件）

- `Source/Core/RenderManager.cpp`：渲染资源中枢（`InsertGeometry` 192、`GetCompiledGeometryHandle` 197、`Render` 214、析构泄漏对账 19–42 → W4D-047）。
- `Include/RmlUi/Core/RenderInterface.h`：后端契约（`CompileGeometry` 40）。
- `Source/Core/ElementBackgroundBorder.{h,cpp}`、`GeometryBackgroundBorder.{h,cpp}`：背景/边框几何的生成与缓存（含三种 clip 变体）。
- `Source/Core/FontEngineDefault/FontFaceLayer.{h,cpp}`、`TextureLayout*.cpp`：字形纹理页的装箱与逐字几何。
- `Source/Core/Layout/`：`BlockFormattingContext`、`FlexFormattingContext`、`TableFormattingContext`、`InlineContainer`、`LineBox` 等（HTML/CSS 布局子集）。
- `Include/RmlUi/Core/Element.h`：失效位与元素树（`dirty_definition` 720 起）。

### 1.4 Fyrox（`fyrox-ui/src`，本次 sparse 取到 60+ 文件）

- `draw.rs`（1192 行）：`Vertex`(52)、`CommandTexture`(62)、`ClippingGeometry`(74)、`Command`(130)、`RenderData`(810)、`DrawingContext`(915)、`commit()`(1057)、`draw_text()`(1086)。
- `widget.rs`（2000+ 行）：widget 基类（有效位 727–741、`invalidate_*` 884–938、`WidgetRenderDataSet` 525）。
- `font/mod.rs`（807 行）：`FontGlyph`(66)、`Atlas`(127，`char_map` FxHashMap 133、`render_glyph` 140)。
- `control.rs`、`border.rs`、`brush.rs`、`formatted_text.rs`：控件、边框九宫格、画刷与富文本。

### 1.5 O3DE LyShine（`Gems/LyShine/Code/Source`，128 文件）

- `RenderGraph.{h,cpp}`：`RenderNodeType{PrimitiveList,Mask,RenderTarget}`(31)、`PrimitiveListRenderNode`(77)、`MaskRenderNode`(142)、`RenderTargetRenderNode`(198)、`RenderGraph`(263，`AddPrimitive` 722、`Render` 985、`SetDirtyFlag` 996)。
- `UiCanvasComponent.{h,cpp}`（4363 行 cpp）：画布（`MarkRenderGraphDirty` 1909、`RenderCanvas` 2025、`AddPrimitive` 路径、布局重算入口）。
- `UiRenderer.{h,cpp}`：Atom RPI 对接（`UiShaderData` 变体、`DynamicDrawContext`、模板引用计数）。
- `UiElementComponent.cpp`、`UiTextComponent.cpp`、`UiLayout*Component.cpp`、`UiLayoutManager.cpp`：元素/文本/各类布局与布局管理器。
- `Code/Include/LyShine/UiRenderFormats.h`：`UiPrimitiveVertex`/`UiPrimitive`（顶点格式与图元链表）。

### 1.6 Cocos2d-x（sparse：`cocos/ui`、`cocos/2d`、`cocos/renderer`）

- `cocos/renderer/CCRenderer.cpp`（1040 行）：命令队列与批合并（`render()` 359、`fillVerticesAndIndices` 510 起、`drawBatchedTriangles` 552）。
- `cocos/ui/UILayout.cpp`（1910 行）+ `UILayoutManager.cpp`：脏位布局与布局管理器（`doLayout` 955、`forceDoLayout` 913）。
- `cocos/2d/CCFontAtlas.cpp` + `CCFontFreeType.cpp`：FreeType 字形 → 多页 A8 图集（`prepareLetterDefinitions` 360）。
- `cocos/2d/CCLabel.cpp`：标签与 `FontAtlasCache` 共享图集（1246）。

### 1.7 microui（`src/microui.{h,c}`）

- `microui.h`(296 行)：命令类型（38–43）、`mu_Font`=不透明句柄(112)、裁剪栈 API(227)。
- `microui.c`(1208 行)：命令录制与容器（`mu_push_command` 428、`mu_push_clip_rect` 249、容器 jump 回填 207/1076）。

## 2. 关键机制（按任务给定的六条对比轴）

### 2.1 轴①「帧内重建 vs 保留」的成本结构

| 实现 | 帧内重建什么 | 跨帧保留什么 | 锚 |
| --- | --- | --- | --- |
| Dear ImGui | 全部 widget 逻辑 + 每窗 DrawList（顶点/命令）逐帧从零 `resize(0)` | 窗口记录、ID 状态、字体图集与烘焙字体 | W4D-006、W4D-013、W4D-023 |
| Nuklear | 全部 widget 逻辑 + 命令流（转成顶点流） | 窗口记录（按名哈希查链表）与 `property/edit/popup` 等 widget 状态；库自述定位=「最小状态」（W4D-089） | W4D-033、W4D-034、W4D-030 |
| microui | 命令流（JUMP/CLIP/RECT/TEXT/ICON）在固定内存区内重录 | 池项（id+last_update）与固定内存区容量 | W4D-084、W4D-085、W4D-090 |
| RmlUi | 只有失效元素重生成几何；未失效者复用已编译句柄 | 元素树、计算值、几何句柄（可由多元素共享，W4D-041）、字形纹理页 | W4D-039、W4D-041、W4D-042、W4D-043 |
| Fyrox | 仅失效 widget 重跑 measure/arrange/draw | 每 widget 的 prev_measure/prev_arrange + 缓存 RenderData（draw 与 post_draw 两套，W4D-060） | W4D-058、W4D-059、W4D-061 |
| O3DE LyShine | 脏时整图重建；否则零 | 整个 `RenderGraph`（节点=状态区间，含合并顶点缓冲） | W4D-066、W4D-067、W4D-062 |
| Cocos2d-x | 每帧排序命令 + 逐顶点拷进共享大缓冲 | 节点树、`_doLayoutDirty` 早退的布局结果、共享字体图集 | W4D-073、W4D-074、W4D-077、W4D-082 |

**ImGui 的官方自陈**是这条轴的分水岭：立即式为"最坏情况（频繁变化）"优化，传统 toolkit 为"什么都不变"优化、"一变就掉性能"（W4D-087）。
**对设计取向的意义**：`lssmj-design` §4.3 选的是 LyShine/Fyrox/RmlUi 一侧的"保留+damage"，代价就是"变化时更贵"。这条对价**必须**用候选窗的稳态形态来摊——判据 C3（空 damage 零重画）成立时我们比 ImGui 便宜；单帧内全内容变更（候选整列表刷新）时我们**不**比 ImGui 便宜，这是设计文档当前没写明的对价（见 §5）。
**否证条件**：若实测候选窗"每帧内容必变"（如高亮跟随鼠标逐帧移动导致整行失效），则 damage 分层的收益归零，应退回显式全量重建 + 只做几何复用（RmlUi 式）。

### 2.2 轴②顶点/命令缓冲与批合并

三种缓冲粒度谱系（本批新增的证据）：

1. **每窗口一份**（ImGui）：`AddDrawListToDrawDataEx` 把每窗 DrawList 按层追加（W4D-023），跨窗只共享 `ImDrawListSharedData`；合并只发生在**同一列表内**的相邻命令（W4D-007）。
2. **每节点一份**（LyShine）：节点=状态区间，节点内 `m_combinedVertices/m_combinedIndices` 一次性装填（W4D-063）；合并判据=状态四元组相同 + 容量够（sRGB/混合/预乘/掩码类型）+ 纹理仍在 16 张以内（W4D-065、W4D-064）。
3. **全局一份**（Fyrox `RenderData`，W4D-050；Cocos `_verts` 大缓冲，W4D-074）：命令只存区间。Cocos 的合并是**邻接式**（`prevMaterialID == currentMaterialID`，W4D-073），全局顺序由渲染前的 `renderqueue.sort()` 决定（W4D-075）——即"排序不可跨帧复用"。

命令簿记技巧：ImGui 的 `_OnChanged*` 惰性化（W4D-006）、Nuklear 的空命令改写（W4D-028、W4D-029）与同缓冲反查（W4D-027）、Fyrox 的"空批不入账"（W4D-053）。
**对设计取向的意义**：我们单窗口单层，取 Fyrox 式"全局一份 + 命令区间"最省；批键按 `lssmj-design` §4.2 的 64 位 SortKey，但**判据必须含"容量上界"**（LyShine 的 16 纹理给了先例口径，W4D-064）。
**否证条件**：若同屏纹理页 >2 且无法把候选窗图元并批，则"全局一份"收益消失，应改为按材质分桶的多缓冲。

### 2.3 轴③字体图集与字形策略

| 维度 | ImGui（1.92+） | Nuklear | RmlUi | Fyrox | Cocos2d-x |
| --- | --- | --- | --- | --- | --- |
| 尺寸模型 | 任意尺寸→`ImFontBaked` 动态烘焙（W4D-013） | 每个 `nk_font_config` 一个烘焙尺寸+区间（W4D-036） | FontFaceHandle（字号一份）+ FontFaceLayer（效果层） | 每字号一份 `Atlas`（W4D-055） | `FontAtlas` 每 TTF 配置一份（W4D-082） |
| 字形查找 | 码点索引稀疏表 O(1)（W4D-015） | 线性扫码点区间（W4D-035，反面） | `UnorderedMap<Character,TextureBox>` 哈希（W4D-045） | `FxHashMap<char,usize>` 哈希（W4D-055） | `FontLetterDefinitionMap`（本次未逐字核，见 §5） |
| 装箱 | stb 矩形装箱 + 最小 512×128/最大 8192 上下界（W4D-017） | 预生成区间（无按需装箱） | 多页行式 `TextureLayout`（W4D-046） | 多页 `RectPacker`（W4D-056） | 多页行式货架 + `renderCharAt` 直写页缓冲（W4D-079、W4D-080） |
| 上传粒度 | 局部纹理更新协议（Updates[] + 包围矩形）（W4D-018） | 一次性烘焙上传 | 纹理回调 `CallbackTextureSource`（本次未逐字核） | 页面 `modified` 标志重传（本次未逐字核） | 按页脏区间 `updateTextureContent`（W4D-080） |
| 增量 | 按需（动态字体） | 无（区间预先烘焙） | 按需（新字符入图） | 按需（首见即栅格，W4D-057） | 按需（`findNewCharacters`，W4D-081） |

**注**：microui 干脆把字体做成不透明句柄交宿主（W4D-083），因此它不参与本表任何一列——这是我们**不**取的路线（交付主体必须自带字体栈）。

**对设计取向的意义**：`lssmj-design` §5.4 的"<2k 字形用 SwashCache 直存 / 大面板上图集"与 §5.5 的字形缓存键（4×4 子像素格+缩放档）在下表都能找到同构先例；**新增一条**：图集必须有**显式上下界**与**局部上传**（ImGui W4D-017、W4D-018，Cocos W4D-080），否则扩容即全量重传 + 全量 UV 失效（W4D-019 是明写的坑）。
**否证条件**：若实测"字形数 <2k 且窗口 <600×800"下 SwashCache 直存的命中率与图集无差异，则 P2 的图集档应整体推迟（`lssmj-design` §10.4 已列为待测）。

### 2.4 轴④裁剪栈实现

- **ImGui 两套并存**：`ImDrawList::PushClipRect` 只产出渲染级 scissor（"passed down to your render function but not used for CPU-side coarse clipping"，W4D-008），CPU 侧的粗裁剪/命中由上层 `ImGui::PushClipRect` 体系承担。
- **Nuklear 一套但双消费**：`nk_command_scissor` → `nk_draw_list_add_clip`（W4D-037、W4D-030）；CPU 侧另有 `ctx->clip_stack`（microui 同构，W4D-086）。
- **Cocos 批内裁剪**：裁剪是命令状态之一，`isSkipBatching` 可让该命令强制断开批（W4D-073 的 `batchable` 判断）。
- **LyShine 用模板/掩码节点**：不规则遮罩走 `MaskRenderNode` 的 stencil 两遍（`SetupBeforeRenderingMask/SetupAfterRenderingMask`，W4D-072）；`UiPrimitiveVertex` 甚至带第 2 个纹理索引专用于 alpha 掩码（W4D-069）。
- **Fyrox 除 scissor 外还有 `ClippingGeometry`**（三角形集合 + 变换栈，W4D-052）；`DrawingContext` 用 `transform_stack`/`opacity_stack` 维护绘制状态（W4D-054）。

**对设计取向的意义**：候选窗只需矩形裁剪，但**渲染裁剪与逻辑裁剪必须是两份**（ImGui W4D-008 + microui W4D-086 双证）；圆角/异形遮罩一旦需要，就走 LyShine 的模板路线而不是 CPU 逐像素（后者与 `lssmj-design` §4.1 的 CPU 基线冲突）。

### 2.5 轴⑤输入命中

- ImGui：命中是**按图元逐个判**（`ItemHoverable(bb, id)`，W4D-020），且复用当前窗口 ClipRect 求交（W4D-021）——命中与裁剪同源，避免"被裁掉却仍可点"。
- 大列表靠 `ImGuiListClipper`：先出 1 项量高，再按裁剪矩形算可见区间（W4D-024）——**命中只对已提交项存在**，与 ImGui"图元不剔除、剔除在高层"的分工（W4D-025）自洽。
- Fyrox：命中靠 widget 树 + `screen_to_local` 变换（本次只核到变换函数，未核命中遍历，见 §5）。
- microui：裁剪栈同时服务布局/命中（W4D-086）。

**对设计取向的意义**：`lssmj-design` §7 的命中表（矩形+字符区间）方向正确；ImGui 的额外启示是**命中必须由裁剪栈裁过再判**（W4D-021），否则我们会得到"damage 外的幽灵命中"。
**否证条件**：若候选窗需要同一帧内命中数百行之外的内容（如搜索高亮跨行选择），clip 后判命中的模型需补一层"逻辑可见但视觉裁剪"的白名单。

### 2.6 轴⑥与游戏渲染管线的对接

- **LyShine（最完整样本）**：UI 不接管管线，而是复用 Atom RPI 的 `DynamicDrawContext`（W4D-071）；状态以着色器变体 id 表达（sRGB/linear、alpha-test、gradient mask，W4D-070）；渲染目标用 `AttachmentImage`，canvas 可"渲染到纹理"（`BeginRenderToTexture`，W4D-067 上下文）；掩码用模板（W4D-072）。
- **Fyrox**：UI crate 只产出 `RenderData`（顶点+三角形+命令，W4D-050），交给引擎的 UI 渲染器（`fyrox-impl/src/renderer/ui_renderer.rs`，本次未深读）。
- **Cocos2d-x**：`Renderer` 是共享的 2D 渲染器，UI（`ui::Layout` 等）与 Sprite 走同一条命令队列（W4D-073、W4D-075）；批键 = MaterialID（W4D-076）。
- **ImGui/Nuklear/microui**：不碰管线，只交付顶点+纹理+裁剪三元组（W4D-002、W4D-004、W4D-026）。

**对设计取向的意义**：与 `lssmj-design` §4.2/§9 P3 一致——我们的 GPU 档应交付"我们的命令流"再由 wgpu 执行器消费，**不接管平台管线**；LyShine 证明这条路在真引擎里可行（同一引擎内 UI 与 3D 共用 RHI）。

## 3. 性能手段与公开读数

**本批没有取到任何上游公开的帧时/吞吐数字**（未跑基准、也没在仓内找到官方标定表）——依纪律，本节只登记**可复算的参数与门槛值**，不写"某某很快"：

| 数值 | 含义 | 锚 |
| --- | --- | --- |
| `19297` | ImGui 版本号（1.93.0 WIP），用于复算时的版本锚 | W4D-001 |
| `1 << 16`（65536） | ImGui 触发 VtxOffset 的顶点数门槛（16 位索引） | W4D-009 |
| `NK_USHORT_MAX` | Nuklear 16 位索引上界（超限仅 Debug 断言） | W4D-031、W4D-032 |
| 512/128/8192 | ImGui 图集最小宽高/最大宽高（2 的幂） | W4D-017 |
| `reserve_geometry = 256` | RmlUi 几何槽位预留（避免句柄失效） | W4D-040 |
| `MaxTextures = 16` | LyShine 单节点纹理上限 | W4D-064 |
| 19 种命令 | Nuklear 的 UI 命令类型全集（转换期展开为三角形） | W4D-034（枚举首项）+ W4D-030 |
| `border = 2`（Fyrox 字形页边距） | 字形装箱的像素间隔离 | W4D-056 上下文 |
| 库内无 std 容器 | ImGui 自述大量 UI 下 `std::string` 性能不佳，故一律裸类型 | W4D-088 |

## 4. 坑与反例（负面留档）

1. **纹理帧内可变 → 预存 UV 失效**（ImGui 自陈，W4D-019）：图集扩容/新建会让旧 UV 全部作废。**我们的对策**：UV 按页版本化或"内容通过 GetCustomRect 每次现取"。
2. **字形查询线性扫区间**（Nuklear，W4D-035）：大字符集下每字一次 O(区间数) 扫描；无索引表。
3. **16 位索引只靠 Debug 断言**（Nuklear，W4D-031、W4D-032）：Release 下溢出会静默产生错误几何。**我们的对策**：容量约束在初始化期校验 + 运行期计数记账。
4. **每帧对多 widget 置脏 = 严重性能问题**（Fyrox 自陈，W4D-058）：保留式的失效传播就是它的成本中心。
5. **渲染图持元素组件指针 → 必须整体 Reset**（LyShine，W4D-066）：脏标记置位时先整图清空，避免悬垂指针；反过来说明"跨帧缓存显示列表"必须解决**所有权/生命周期**问题，否则就是 use-after-free。
6. **置脏必须挡重入**（LyShine，W4D-068）：渲染中置脏会产生"半成品图"（注释明写加载屏导致重入的现实路径）。
7. **邻接式批合并的隐含前提**：顺序必须已被 z 排序保证（Cocos，W4D-073、W4D-075）；一旦引入跨层绘制（如 ImGui 的 tooltip 层，W4D-022），邻接合并会失效。
8. **ImGui 绘制列表不剔除图元**（W4D-025）：完全依赖上层裁剪；我们的分层若做不到"上层一定裁"，会出现"便宜的路走错方向"。
9. **逐顶点 memcpy + 逐帧排序**（Cocos，W4D-074、W4D-075）：这是被 `lssmj-design` §4.3 明确要消掉的成本结构；不要因为实现简单而先抄它。
10. **按尺寸烘焙会造成"每字号一份资源"爆炸**（Nuklear/Fyrox/Cocos 都是这个模型，W4D-036、W4D-055、W4D-082）：ImGui 1.92 转向动态烘焙（W4D-013）正是因为缩放/多 DPI 场景下固定尺寸太贵——`lssmj-design` §5.3 的 opsz 分键必须与之同向。

## 5. 未验证项（缺什么证据）

1. **Stride 与 Flax 未覆盖**：按任务允许可降为 doc 级，但本次 `gh api` 列目录 + raw 拉取均在分支/路径上 404（尝试次数 2 次后按纪律停手）。二者与"游戏引擎内 UI 层"（尤其 Flax 的 `GUIRenderer`/Stride 的 `Renderers/*`）的对照留待后续；**当前报告不包含它们的任何主张**。
2. **未跑任何基准**：§3 只有参数与门槛值；"ImGui 帧成本/我们的 damage 收益"这类数字**一个都没有**。判据 C1/C3 的实测仍未做。
3. **Fyrox UI 渲染器未深读**：`fyrox-impl/src/renderer/ui_renderer.rs`（sparse 已取到，未读）——命令→GPU 的批处理与纹理上传策略缺证据。
4. **RmlUi 的纹理上传路径未逐字核**：`CallbackTextureSource`/`TextureDatabase` 的调用时机与"帧内是否重传"缺锚；文中相关表述已标注"未逐字核"。
5. **Cocos 的字形查找结构未逐字核**：`FontLetterDefinitionMap` 的实际类型与查找复杂度未读（`CCFontAtlas.h` 未读）。
6. **Nuklear 的缓冲/池容量语义未逐字核**：`nk_buffer` 的 FRONT/BACK 两侧生长与"帧末复位"只读到枚举与 API 名（W4D 未登记该条），故 §2.1 只写"命令+顶点同缓冲"这一已锚事实。
7. **命中测试的完整路径**：ImGui 只核到 `ItemHoverable` 入口与 clip 求交；`HoveredId` 的跨窗口竞争/层级判定未读；Fyrox 只核到坐标变换函数。
8. **无障碍/输入法**：本批 7 个目标**全部未涉及** IME/无障碍（ImGui 的 `PlatformImeData` 只出现在头文件注释里，未深读）——`lssmj-design` §7 的 IME 契约在本批得不到任何证据支持。
