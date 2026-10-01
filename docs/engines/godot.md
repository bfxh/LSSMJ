# Godot（https://github.com/godotengine/godot @ 084a2caa, 抓取 2026-10-01）

> 类型：C++ 源码级深读（source 主体）。浅克隆 + sparse-checkout（`scene/gui`,`scene/theme`,`scene/resources`,`scene/main`,`servers/rendering`,`servers/text`,`drivers/gles3`,`drivers/vulkan`,`modules/{text_server_adv,msdfgen,harfbuzz,freetype,icu}`,`core/{math,object,os,io}`,`doc/classes`），112 MB。
> 上游提交：`084a2caa05119b625a99b6b51d44b459a26362de`，`version.py` 声明 **4.8.0-dev**（W2A-001）。本报告所有行号均对该提交。
> 账本：`docs/analysis/ledger/w2a.jsonl` **137 条（source 129 / doc 8），verify 0 拒绝**（复算：`python tools/ledger.py verify --file docs/analysis/ledger/w2a.jsonl`）。
> 纪律：全程只读（未编译、未跑基准、未 git 提交）；WebFetch 未用，在线文档走 `python urllib` 抓 godot-docs raw（与 docs.godotengine.org 同源）。

## TL;DR（每条带锚）

1. **布局=8 个标量 + 一次乘加**：`edge_pos[i] = data.offset[i] + (data.anchor[i] * area)`（`scene/gui/control.cpp:2260`，W2A-004）——位置尺寸以 `offset[4]/anchor[4]` 存储（`control.h:231-232`，W2A-002/003），无约束求解、无盒模型传递。
2. **最小尺寸三级缓存 + 懒失效**：`minimum_size_cache`/`minimum_size_valid`（`control.h:258-259`，W2A-006/007），失效沿父链上溯到 Window 为止（`control.cpp:1977-1979`，W2A-011/012），重算被 `call_deferred` 汇到帧末（`control.cpp:2005`，W2A-013）。
3. **容器排列是帧末批处理**：`queue_sort()` → `callable_mp(&Container::_sort_children).call_deferred()`（`container.cpp:190`，W2A-019），`pending_sort` 去重；容器唯一写子几何的入口是 `fit_child_in_rect()`（`container.cpp:138`，W2A-023）。
4. **主题是四级解析链**：节点 override → 控件两级缓存 → owner 链 → 全局 context → fallback theme（`theme_owner.cpp:228-262`，W2A-037/038；`control.cpp:3982-3989`，W2A-036）；主题变更是「全清缓存 + 全子树递归通知」（`control.cpp:3785`，W2A-033；`theme_owner.cpp:172`，W2A-040）。
5. **2D 绘制命令块分配**：注释「commands are allocated in blocks of 4k to improve performance」（`renderer_canvas_render.h:169`，W2A-044），`MAX_SIZE = 4096`（W2A-045）——bump 分配 + 块只增不减。
6. **2D 也有实例化批处理**：一个四边形顶点格式 + 逐批偏移的实例缓冲（`renderer_canvas_render_rd.cpp:3074-3076`，W2A-062）；批断裂 = 命令类型/混合/MSDF 参数/LCD/纹理状态（`:2412,:2435,:2451,:2463`，W2A-058..061）。
7. **文本与图形同管线**：MSDF/LCD 文本只是带 flag 的矩形（`CANVAS_RECT_MSDF = 128`，`renderer_canvas_render.h:50`，W2A-047；`renderer_canvas_cull.cpp:1613`，W2A-048）。
8. **Godot 原生 MSDF 是完整一等公民**：msdfgen 生成（`text_server_adv.cpp:992/1056/1064`，W2A-100/101）、单源尺寸 128 服务所有字号（`font.cpp:1457`，W2A-105；`text_server_adv.cpp:1633`，W2A-099）、绘制时按 `p_size/msdf_source_size` 缩放四边形并传 px_range（`:4355`，W2A-098）。
9. **重绘粒度=整个 item**：`queue_redraw` 去重后帧末回调，先 `canvas_item_clear` 再整表重放（`canvas_item.cpp:541-552/151`，W2A-069/070/071）；全局只有「有变化」计数器 `changes++`（`rendering_server_default.h:111`，W2A-118）供宿主查询。
10. **渲染器自报记账**：对象/图元/绘制调用三元计数（`renderer_canvas_render_rd.cpp:3078-3082`，W2A-063）→ `viewport_get_render_info` → `Performance.RENDER_TOTAL_*`（`doc/classes/Performance.xml:166-173`，W2A-123）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚（账本号 + file:line） | 判定 |
| --- | --- | --- |
| 锚点+偏移 8 标量模型（含 GROW 方向回推） | W2A-004/015（`control.cpp:2260/2269`） | 吸收：与 GN sARRANGE_FMT 同族，候选窗足够 |
| 最小/期望/最大三值 + valid 位 | W2A-006/007/024（`control.h:258`；`container.cpp:147`） | 吸收：desired 单独成线是自适应宽度的关键 |
| 帧末合并 + 值比较门（等值跳过下游） | W2A-013/014（`control.cpp:2005/1964`） | 吸收：布局请求必须去重+比后再做 |
| 容器「唯一写几何入口」fit_child_in_rect | W2A-023（`container.cpp:138`） | 吸收：几何写入单点，便于审计与单测 |
| 窗口自适应=内容最小尺寸外接框 | W2A-029（`window.cpp:2004`） | 吸收：候选窗自动尺寸的直接原型 |
| 四级主题解析链 + fallback 必返回 | W2A-037/038（`theme_owner.cpp:240/261`） | 有界吸收：保留「节点 override→owner 链→fallback」三级，去掉全局 context 多域 |
| 主题项按类继承链绑定 setter 回填 | W2A-034/035（`control.cpp:3795`；`theme_db.cpp:376`） | 有界吸收：思路可用，但要换成 Rust 侧的穷举 struct 而非运行期 map |
| 命令 4KB 块分配 + 只增不减 | W2A-044/045（`renderer_canvas_render.h:169/174`） | 吸收：每帧零大分配的现成做法（判据 C6） |
| 矩形+flag 统一表达（REGION/MSDF/LCD…） | W2A-047/048/116（`renderer_canvas_render.h:50`；`style_box_flat.cpp:647`） | 吸收：文本/装饰/图片共一条批处理路径 |
| z 值域桶排序（z_range 桶数组） | W2A-049/050（`renderer_canvas_cull.h:216`；`renderer_canvas_cull.cpp:79`） | 吸收：层号值域有限时 O(n) 完胜比较排序 |
| 实例化批：单四边形 + 实例数据 + 每批偏移 | W2A-062/055/056（`renderer_canvas_render_rd.cpp:3074`） | 吸收（GPU 档）：正是我们 P3 的目标形态 |
| 批键=命令类型/混合/MSDF/纹理指针 | W2A-058..061（同文件） | 吸收：批键分层，纹理比对用去重指针 |
| shelf 装箱字形图集（per 字号） | W2A-086/087/088（`text_server_adv.h:178/192/250`） | 吸收：比 guillotine/BSP 更简单，字形同质时浪费低 |
| 字形键位打包（低位字形 / 24-26 LCD / 27-28 子像素） | W2A-089/093/097（`text_server_adv.h:252`；`text_server_adv.cpp:1294/4307`） | 吸收：一个 int32 承载全部变体，命中判定廉价 |
| 子像素 AUTO 档位阈值（>20 半像素、>16 四分之一） | W2A-094/095/096（`text_server_adv.cpp:1361/1364`；`text_server.h:172`） | 吸收：小字号才付子像素成本 |
| 图集新页初始化值（普通填白/MSDF 填黑） | W2A-092（`text_server_adv.cpp:875`） | 吸收：缩放过滤伪影的细节坑，必须记进实现清单 |
| ICU 断行 blueprint + `ubrk_clone` | W2A-083（`text_server_adv.cpp:7056/7073`） | 吸收：断行器是重对象，保留母本再克隆 |
| MSDF 单源尺寸 + 绘制期缩放 + 多线程生成 | W2A-098/099/100/101（`text_server_adv.cpp:4355/1633/992/1064`） | 有界吸收：作为「大字号/极端缩放」可选档（与 design §5.7 同结论） |
| 渲染器自报计数（对象/图元/绘制调用） | W2A-063/121/122/123 | 吸收：进我们的性能报表，口径与 Godot 可比 |
| `WRITE_ACTION` 宏「写即标脏」+ changes 计数 | W2A-118/119/120（`rendering_server_default.h:111/116`） | 吸收：脏标志做进写路径，不漏设 |
| StyleBoxFlat 的 AA 条件化 + 随 oversampling 缩放 | W2A-112/113（`style_box_flat.cpp:472/474`） | 吸收：直边直角不开 AA，既快又锐 |
| 「变换延迟到帧末、需即时读要用 force_update_transform」写进文档 | W2A-129（`doc/classes/CanvasItem.xml:461`） | 吸收：行为契约必须明写，否则用户踩坑 |

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 每帧整树重剔除（memset z 桶 + 重走全树） | W2A-050/075（`renderer_canvas_cull.cpp:79`；`viewport.cpp:1424`） | 不吸收：我们按内容版本号/tile 比较做 damage（判据 C3 优于它） |
| 重绘粒度=整 item 命令清空重放 | W2A-071（`canvas_item.cpp:151`） | 不吸收：无子项/行级局部重画；我们做行级 damage |
| 主题变更全子树递归通知 + 全清缓存 | W2A-033/040 | 有界吸收：主题域大时改「版本号+懒解析」，不为候选窗引入 |
| 命中测试每事件递归整树 + 逐点逆变换 | W2A-076（`viewport.cpp:1876`） | 不吸收：我们用命中表 O(1)/O(log n)（相对优势点，写明） |
| 全局 ThemeContext 多域嵌套（`theme_contexts` 树） | W2A-042（`theme_db.cpp:276`） | 不吸收：复杂度换多窗口主题域，候选窗不需要 |
| 光效（每图元 ≤16 盏灯）与 canvas group 离屏 | W2A-054（`renderer_canvas_render_rd.h:108`） | 不吸收：候选窗无光照需求 |
| Compatibility 后端（无管线预编译、元素上限 65536） | W2A-124/137（`ProjectSettings.xml:3263`；官方页） | 有界吸收：若做 GL 档须认这两条硬约束 |
| MultiMesh「全有或全无」可见性 | W2A-134（官方页，`article_outdated`） | 不吸收：我们需要逐行裁剪与命中 |

## 1. 架构全景（模块地图，逐文件职责）

**服务层（C++，无脚本）**

| 文件 | 职责 | 关键事实（账本号） |
| --- | --- | --- |
| `servers/rendering/rendering_server.h` / `rendering_server_default.{h,cpp}` | 渲染服务门面；写操作宏统一触发 `redraw_request()`（changes++），`has_changed()` 对外暴露「本帧有变化」 | W2A-118/119/120 |
| `servers/rendering/rendering_server_types.h` | `RenderInfo` 二维计数表（类型 × 指标）：对象/图元/绘制调用 | W2A-122 |
| `servers/rendering/renderer_viewport.cpp` | 帧流程：逐 viewport 清计数 → 绘制 → 累加计数（供 `viewport_get_render_info`） | W2A-121 |
| `servers/rendering/renderer_canvas_cull.{h,cpp}` | 2D 剔除与命令存储：z 桶排序、clip 矩形求交、`Item`/`Command*` 命令链、`canvas_item_add_*` 系列 | W2A-044/045/049/050/051/052 |
| `servers/rendering/renderer_canvas_render.{h,cpp}` | 命令结构定义（`CommandRect/NinePatch/Polygon/Primitive/Mesh/MultiMesh/Particles/Transform/ClipIgnore/AnimationSlice`）+ 包围盒惰性计算 | W2A-046/047 |
| `servers/rendering/renderer_rd/renderer_canvas_render_rd.{h,cpp}` | RD 后端 2D：实例化批记录/断裂/绘制、管线缓存（`pipeline_hash_map`）、MSDF/LCD 特化 | W2A-053..064 |
| `drivers/gles3/rasterizer_canvas_gles3.cpp` | GLES3 后端 2D：同构批处理（clip/材质/纹理断批）、整帧一次 `glMapBufferRange` 上传、scissor 裁剪 | W2A-065..068 |

**场景/UI 层**

| 文件 | 职责 | 关键事实（账本号） |
| --- | --- | --- |
| `scene/main/canvas_item.cpp` | 保留式绘制命令的生命周期：`queue_redraw` 去重 → 帧末 `_redraw_callback` → `canvas_item_clear` + 重放 `NOTIFICATION_DRAW`/`draw`/`_draw` | W2A-069..074 |
| `scene/gui/control.{h,cpp}` | 布局（锚点/偏移/三值尺寸/valid 位）、主题取值缓存、布局模式四态、`layout_pending` 机制、输入与绘制通知 | W2A-002..018、031..036、077 |
| `scene/gui/container.{h,cpp}` | 容器基类：`queue_sort`/`_sort_children`、`fit_child_in_rect`、`as_sortable_control` | W2A-019..024 |
| `scene/gui/box_container.cpp` | 排列算法实例：间距扣除 → 期望尺寸分配 → 伸缩比分配 | W2A-025..028 |
| `scene/main/window.cpp` | 内容最小尺寸 → 窗口自适应；`child_controls_changed` 帧末归并 | W2A-029/030 |
| `scene/theme/theme_owner.cpp` | 主题解析链（owner 链 + 全局 context + fallback）、变更传播 | W2A-037..040 |
| `scene/theme/theme_db.cpp` | 默认/项目主题、ThemeContext 树、类→主题项绑定与回填 | W2A-035/041/042 |
| `scene/theme/default_theme.cpp` | 内置默认主题常量（字号 16/边距 4/圆角 3） | W2A-043 |
| `scene/resources/style_box_flat.cpp` | 样式盒绘制：AA 条件化、圆角网格（corner_detail ≤20）、三角数组提交 | W2A-112..117 |

**文本栈**

| 文件 | 职责 | 关键事实（账本号） |
| --- | --- | --- |
| `servers/text/text_server.h` | 抽象：`Glyph`/`ShapedTextData` 结构、断行 flag、子像素枚举与阈值 | W2A-096/108/109 |
| `modules/text_server_adv/text_server_adv.{h,cpp}` | 实现：FreeType 栅格 + HarfBuzz 整形 + ICU 断行/双向 + 字形图集（shelf）+ MSDF 生成（msdfgen） | W2A-078..102、105..111 |
| `scene/resources/font.cpp` | 资源层：`FontFile` 加载、缓存按 size 展开、MSDF 开关与参数、system fallback | W2A-105/106/107 |
| `modules/msdfgen/` | 仅注册 thirdparty（实际 MSDF 逻辑在 `text_server_adv.cpp` 的 `rasterize_msdf`） | W2A-100 |

## 2. 关键机制

### 2.1 布局：锚点解析 + 三值尺寸 + 懒失效
- **几何表达**：`offset[4] + anchor[4]`（W2A-002/003）；解析式 `edge_pos[i] = offset[i] + anchor[i] × 父边尺寸`（W2A-004）——布局阶段没有约束求解器，只有一次乘加；`LAYOUT_MODE_*` 四态显式声明「谁拥有布局权」（W2A-005，容器模式下控件自身写位置会被忽略）。
- **尺寸协商**：`get_minimum_size()` 由控件自报（虚函数，可被脚本/GDExtension 覆盖），与 `custom_minimum_size` 取分量最大值入 `minimum_size_cache`（W2A-009/010）；`desired_size` 是独立曲线，`fit_child_in_rect` 用 `clamp(desired, 可用宽度, 最小尺寸)`（W2A-024）。
- **失效协议**：内容变化 → `update_minimum_size()` 沿父链清 valid 位（已脏则提前停），到 top_level/Window 为止（W2A-011/012）；实际重算由 `call_deferred` 排到帧末、`updating_last_minimum_size` 去重（W2A-008/013）；重算结果与 `last_minimum_size` 逐值比较，相等则整条下游重排被跳过（W2A-014）。
- **落位与通知**：`_size_changed()` 用锚点算尺寸，小于最小尺寸时按 `h_grow/v_grow`（END/BEGIN/BOTH）回推位置（W2A-015）；变化分「精确/近似」两级判定，近似变化才发 `item_rect_changed`/`RESIZED`（W2A-016），且先修变换再发通知（W2A-017）。
- **容器**：`queue_sort` → 帧末 `NOTIFICATION_SORT_CHILDREN`（W2A-019/021）；缺省容器最小尺寸=各子项最大分量（W2A-022）；BoxContainer 顺序=扣间距 → 期望尺寸阶段 → 伸缩比阶段（W2A-025/026/027），内部用局部 `HashMap<Control*, _MinSizeCache>` 缓存子项属性（W2A-028）。
- **窗口自适应**：`Window::_get_contents_minimum_size()` = 遍历直属 Control 取 `max(位置+最小尺寸)`（W2A-029），子控件变化同样帧末归并（W2A-030）——候选窗「尺寸跟内容」的最小实现。
- **layout_pending**：读布局结果（测量）必须挂在 `_layout_pending_finished` 之后，否则读到中间态（W2A-018）。

### 2.2 主题/样式：四级链 + 两级缓存 + 全清失效
- 每个 Control 持 `ThemeOwner`（W2A-031），并对 7 类主题项各持 `HashMap<类型名, HashMap<项名,值>>` 缓存（W2A-032）。
- 取值顺序：本控件 override → 缓存 → `ThemeOwner::get_theme_item_in_types`（owner 链逐类型 → 全局 context 的 themes → fallback theme）（W2A-036/037/038）。继承链只沿 Control/Window 传播，其他节点截断（W2A-039）。
- 变更：`_invalidate_theme_cache()` 整体 clear 7 张表（W2A-033），`propagate_theme_changed` 沿子树递归下发 notification（W2A-040），收到 `NOTIFICATION_THEME_CHANGED` 的控件按固定序列处理（发信号 → 清缓存 → 重填 → queue_redraw → update_minimum_size → _size_changed，`control.cpp:4902-4910`，W2A-138）。
- 回填机制：主题项在类注册期绑定 setter（`ThemeDB::bind_class_item`），实例化/主题变化时沿 `get_parent_class_nocheck` 上溯逐类调用 setter（W2A-034/035）。
- 全局主题是**有序列表** `[project_theme, default_theme]`（W2A-041）；`ThemeContext` 可嵌套成树，遇到子 context 根即停止下行（W2A-042）。
- 默认主题是一套常量（字号 16/边距 4/圆角 3，按 scale 缩放）（W2A-043）。
- 样式盒绘制：AA 只在「有圆角或有斜切」时开启（W2A-112/113），圆角按 `corner_detail ∈ [1,20]` 网格化、顶点数 `(detail+1)×(画边框?8:4)`（W2A-114/115），最终经 `canvas_item_add_triangle_array` 与图片/文本共用管线（W2A-116）；空样式盒直接返回（W2A-117）。

### 2.3 2D 渲染：命令块 → z 桶 → 每帧实例批
- **命令存储**：`Item::Command` 链按 4KB 块分配、块只增不减（W2A-044/045）；矩形命令带 flag（REGION/TILE/FLIP/TRANSPOSE/CLIP_UV/IS_GROUP/MSDF/LCD）（W2A-047）；包围盒 `get_rect()` 有 dirty 快路径（W2A-046）。
- **剔除**：每帧 `memset` 两个 z 桶数组再单遍挂链（W2A-049/050）；判据=「有命令或有 visibility notifier」且「item 全局矩形 ∩ clip 矩形」（W2A-051）；`update_when_visible` 的 item 每帧强制 `redraw_request()`（W2A-052）。
- **批处理（RD）**：`MAX_RENDER_ITEMS = 256K` 分块（W2A-053）；批断裂条件=命令类型（W2A-058）→ 混合/调制色（W2A-059）→ MSDF 参数 → LCD → 纹理状态指针（W2A-060/061）；实例数据经中转结构体 `memcpy` 写入（W2A-056，注释警告不要读写 write-combined 页，W2A-057）；绘制=绑定同一四边形顶点格式 + 每批偏移实例缓冲 + `draw_list_draw(…, instance_count)`（W2A-062）。
- **批处理（GLES3）**：同构断批条件（clip 变化 W2A-066、纹理/命令类型 W2A-067），整帧实例数据一次 `glMapBufferRange`+`memcpy`（UNSYNCHRONIZED）（W2A-065），裁剪用 `glScissor`（W2A-066 内含），空 item 集合只清屏（W2A-068）。
- **记账**：一次实例批 = 对象 +n、图元 +2n、绘制调用 +1（W2A-063），帧末按 viewport 聚合（W2A-121/122），对外是 `Performance.RENDER_TOTAL_*`（W2A-123）。
- 纹理状态先去重成唯一对象（`tex_info`），断批比指针而非 RID（W2A-061）；`texture_info_map` 每帧清空（W2A-064）。

### 2.4 文本栈：run 级整形 + ICU 断行 + per-size 图集 + MSDF
- **整形**：`_shape_run(p_sd, start, end, language, script, direction, FontPriorityList, …)`（W2A-078），底层 `hb_shape` 带 OpenType 覆盖与 span 特性（W2A-079）；字体回退三级：列表 → 系统字体（`_find_sys_font_for_text`）→ 原始字符/hex 框（W2A-080）。
- **结果对象** `ShapedTextDataAdvanced`：源数据（text/spans/objects）+ 整形数据（runs/glyphs/glyphs_logical）+ 中间数据（utf16/hb_buffer/breaks），自带 Mutex（W2A-111/081）；断行点位有独立 valid 位（W2A-082）；`Glyph` 结构含字符区间/偏移/advance/字体 RID/字号/span 索引（W2A-108/109）。
- **断行**：ICU `UBRK_LINE`，硬/软断点按 rule status 分流（W2A-084）；迭代器创建「surprisingly costly」，按语言留 blueprint 再 `ubrk_clone`（W2A-083）。
- **CJK 特性**：OpenType tag 具名暴露，含 `centered_cjk_punctuation` (cpct)（W2A-085）——标点可用字体特性而非自绘。
- **图集**：`FontForSizeAdvanced.textures` 是 per-size 的 `ShelfPackTexture` 数组（W2A-088/086），shelf 取「高度完全匹配否则浪费最小」（W2A-087）；新页边长 `max(字号×0.125, 256)` → 2 的幂 → 封顶 2048(MSDF)/1024(普通)（W2A-090/091）；新页初始值普通填白/MSDF 填黑（防缩放过滤伪影）（W2A-092）。
- **字形键**：低 24 位字形索引 + 高位变体（掩码 0xffffff，W2A-093）；LCD 布局 bits 24-26（`index | (layout << 24)`，W2A-097 区域）；子像素 X bits 27-28，4 档（ONE_QUARTER，`<<4`）/2 档（ONE_HALF，`<<5`）（W2A-094/095），档位由字号阈值自动选择（>16 四分之一、>20 半像素）（W2A-096）；绘制端按实际 x 计算档位（W2A-097）。
- **MSDF**：`rasterize_msdf` 吃 FT_Outline（W2A-100），`MSDFGeneratorConfig(true, ErrorCorrectionConfig())` + `msdfErrorCorrection`（W2A-101 内含），按扫描行交 `WorkerThreadPool` 组任务并行（W2A-101）；MSDF 模式下 FreeType 面固定按 `msdf_source_size`（默认 128，W2A-105）栅格化（W2A-099）；绘制时目标矩形与 UV 按 `p_size/msdf_source_size` 缩放并传 px_range/scale（W2A-098）；上游注释记录「4.8 之前除数 60 有误，可经 `gui/fonts/compatibility/msdf_legacy_scaling` 恢复」（W2A-102）。
- **彩色字体**：`FT_LOAD_COLOR` 与 `FT_LOAD_NO_BITMAP` 按字体能力与策略分流（W2A-110）。
- **资源层**：`FontFile` 的 MSDF 开关逐缓存下发（W2A-106），开关变化是全量重建事件（W2A-107）；官方类参考确认 MSDF 语义与代价（无 hinting、小字号不锐）（W2A-103）。

### 2.5 重绘模型：item 粒度 + 两个状态位
- `queue_redraw()`：非树内直接返回；`pending_update` 去重；`call_deferred(_redraw_callback)`（W2A-069/070）。
- `_redraw_callback()`：不可见也把 `pending_update` 清掉；`draw_commands_dirty` 时先 `canvas_item_clear`（W2A-071）再重放；顺序 `NOTIFICATION_DRAW` → `draw` 信号 → `_draw`（W2A-074）；完成后 `draw_commands_dirty = true`（W2A-072）；`pending_update` 必须等绘制完才清零（防重入，W2A-073）。
- `Control` 在 `NOTIFICATION_DRAW` 里把自身矩形注册为 RS 的 custom rect（裁剪/剔除用）——布局→渲染的交接点就在此刻（W2A-077 同族，`control.cpp:4888`）。
- 全局：`redraw_request()` 只 `changes++`（W2A-118），`has_changed()` = `changes > 0`（W2A-119），所有写操作经 `WRITE_ACTION` 宏自动标脏（W2A-120）。
- 官方语义与实现互证：`queue_redraw` 每帧最多一次（W2A-127）、canvas item 不必每帧重画（W2A-126）、变换帧末统一应用（W2A-129）。

## 3. 性能手段与公开读数（数字必须带锚：版本/机器/口径）

**（a）能够自证的引擎内读数（口径明确）**

| 读数 | 值/形态 | 锚 |
| --- | --- | --- |
| 2D 帧内对象数 | `RENDER_TOTAL_OBJECTS_IN_FRAME`（canvas 段按实例数累计） | W2A-063/123 |
| 2D 帧内图元数 | 实例化矩形批按 `2 × instance_count` 计 | W2A-063 |
| 2D 帧内绘制调用数 | 每批 +1 | W2A-063/123 |
| 聚合口径 | 帧末按 viewport 累加后经 `viewport_get_render_info` 暴露 | W2A-121/122 |
| GLES3 硬上限 | `max_renderable_elements=65536`、`max_renderable_lights=32`、`max_lights_per_object=8` | W2A-124 |
| 每图元光照上限 | `MAX_LIGHTS_PER_ITEM = 16`（实际截断到 15） | W2A-054 |
| 单次渲染提交的 item 上限 | `MAX_RENDER_ITEMS = 256 × 1024`（超出分块提交） | W2A-053 |
| MSDF 参数默认 | `msdf_source_size=128`、`msdf_pixel_range=14`（代码；文档写 16/48，见 §4） | W2A-105/104 |
| 图集页尺寸 | `max(字号×0.125,256)` → 2 的幂 → ≤2048(MSDF)/≤1024 | W2A-090/091 |
| 子像素档位阈值 | ONE_QUARTER_MAX_SIZE=16、ONE_HALF_MAX_SIZE=20（单位为 pt×64 比较） | W2A-096 |
| 圆角细分上限 | `corner_detail ∈ [1,20]` | W2A-114 |
| 顶点预算公式 | `(detail+1)×(画边框?8:4)` 个顶点/角 | W2A-115 |

**（b）官方文档里的数字（均为教程/论证口径，非基准）**

| 数字 | 出处 | 口径说明 |
| --- | --- | --- |
| 2D benchmark 下「渲染 canvas 的函数」占 66%；约 50% 时间在 `libglapi`/`i965_dri` 驱动内 | godot-docs `cpu_optimization.rst`（抓取 2026-10-01，URL 见 W2A-132） | Callgrind 示例截图；页面未标注 Godot 版本、机器、帧率 |
| 帧时分解示例 9ms/1ms、50ms 对比 | `general_optimization.rst`（W2A-131） | 纯示意算术（A 优化 9× 而总帧时 5×；GPU 瓶颈时 CPU 优化无收益） |
| 4K 面积 8,294,400 px vs 640×480 的 307,200（27×） | `gpu_optimization.rst`（W2A-136） | 通用填充率论证，非 Godot 实测 |
| MultiMesh「single draw primitive … up to millions」 | `using_multimesh.rst`（W2A-134） | 该页顶部标 `:article_outdated: True` |
| 三种后端取舍（Mobile「特性更少但简单场景更快」） | `renderers.rst`（W2A-135） | 定性；无数字 |
| 官方性能哲学：默认后端优先「平衡与灵活」而非性能 | 性能文档首页（W2A-130） | 与 Godot 比数字时必须带此前提 |

**未找到**：带机器/版本/帧率三方口径的官方 2D UI 基准数字（godot-docs 性能章节无此类页；见 §5）。

## 4. 坑与反例（负面留档，同样是条目）

1. **官方文档默认值与代码不符**：`FontFile.xml` 声明 `msdf_pixel_range` 默认 16、`msdf_size` 默认 48（W2A-104），`font.cpp` 实际 14 / 128（W2A-105）——引用「默认值」必须读代码。
2. **MSDF 除数历史错误**：4.8 之前 FreeType 26.6 定点用了除数 60（应 64），上游留了 `gui/fonts/compatibility/msdf_legacy_scaling` 兼容开关（W2A-102）——行为修复带显式兼容项，值得照抄流程。
3. **MSDF 的代价是硬代价**：官方明说「font hinting 不可用」、小字号清晰度下降（W2A-103）；引擎项目级默认 `default_font_multichannel_signed_distance_field = false`（W2A-125）——上游默认档不用 MSDF。
4. **无通用 2D damage**：局部重画的唯一补偿是 `draw_animation_slice` 时间片技巧（官方称「比持续重画更快的背景动画实现」W2A-128），且重绘粒度是整 item（W2A-071）——需要行级/区级更新时 Godot 2D 没有现成机制。
5. **`update_when_visible` 是每帧重画的开关**（W2A-052）：误用即退化为每帧全量命令重放。
6. **主题变更成本是全子树**：全清缓存（W2A-033）+ 递归 notification（W2A-040）+ 每控件 `update_minimum_size`+`_size_changed`；主题域大时是热点。
7. **命中测试 O(树) 且每事件重走**：`_gui_find_control_at_pos` 递归子节点（逆序）+ 逐点逆变换（W2A-076）。
8. **AA 不是默认安全的**：仅圆角/斜切才开（W2A-112），且羽化宽度必须按 oversampling 反向缩放（W2A-113），否则高 DPI 下发虚。
9. **图集新页初始值会影响缩放过滤结果**（填白/填黑各有原因，W2A-092）——空白区的值不是无关紧要的。
10. **Compatibility 后端没有管线预编译**：官方要求「加载时把材质/着色器/粒子在视锥内显示至少一帧」来预热（W2A-137），否则首帧卡顿；同后端还有 65536 元素上限（W2A-124）。
11. **实例数据不能直接读写 write-combined 内存页**（注释：huge performance implications，W2A-057）——GPU 档实现必须走中转结构体。
12. **加宽/加高会误报的风险已被上游显式处理**：精确/近似两级比较（W2A-016）说明浮点噪声若不处理会产生 resized 通知风暴。

## 5. 未验证项（写明缺什么证据）

1. **无官方 2D UI 基准数字**：本轮只抓到教程级示例（W2A-131/132/136），无「机器+版本+帧率」三方口径的官方 2D 渲染基准；未抓取第三方基准（纪律：本轮不做二手证据）。
2. **未做任何实测**：所有数字均为上游源码常量与官方文本转述；本仓纪律禁止执行上游代码/跑基准（BRIEF §2.5），故「Godot 2D 一帧多少 ms」在本轮**无锚**。
3. **未读的模块（影响面已标注但未考察）**：`renderer_scene_*`（3D）、`renderer_rd` 的 canvas group/skeleton/particles 细节、其余容器实现（GridContainer/FlowContainer/ScrollContainer/Window 复数面板）、`TextServer` 的 justification/overrun trim 细节、`Font` 变体与可变字体轴路径、编辑器主题生成脚本（`default_theme_icons.gen.h` 来源）。
4. **GLES3 与 RD 后端的 2D 实际性能差**：只有代码形态差异（纹理键比较方式、上传方式，W2A-065/067），无实测数字支撑「哪个更快」。
5. **文档在线页面未直接抓取**：本报告 doc 条目的 anchor 是 `raw.githubusercontent.com/godotengine/godot-docs/master/...`（2026-10-01 抓取）；未逐一核验 docs.godotengine.org 渲染页与 raw 源是否逐字一致（同源但版本滚动策略不同）。
6. **`doc/classes/*.xml` 属「文档深度」但按源码锚记**：这些 XML 是随引擎提交的官方类参考（W2A-103/104/123/124/125/126/127/128/129），本报告把它们按 `source` 记（可机器复算引文），如需 `doc` 深度需补在线 URL。
