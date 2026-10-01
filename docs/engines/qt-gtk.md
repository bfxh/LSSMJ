# Qt Quick（scene graph）+ GTK4（GSK）（2026-10-01 抓取）

上游与快照（本报告全部行锚以此为可复算基准）：

| 目标 | 仓库 | commit | 本地路径 | 备注 |
| --- | --- | --- | --- | --- |
| Qt Quick scene graph | https://github.com/qt/qtdeclarative | `0be90e31adc05fd34680e9629cbc8e2372c8a54a` | `D:/KF/LSSMJ/scratch/src/qtdeclarative/src/quick/scenegraph`（sparse） | `.cmake.conf:1` = `set(QT_REPO_MODULE_VERSION "6.13.0")`（W2D-001）|
| GTK4 / GSK | https://gitlab.gnome.org/GNOME/gtk | `3b6059be883fea771f2b4784e14519c7996e6f68` | `D:/KF/LSSMJ/scratch/src/gtk/{gsk,gtk,gdk}`（sparse） | `meson.build:2` = `version: '4.24.1',`（W2D-002）|

文档快照：`D:/KF/LSSMJ/scratch/w2d/*.txt`（doc.qt.io / docs.gtk.org / blog.gtk.org 的 HTML→文本落盘，抓取 2026-10-01）。
Qt 文档页自称 “Qt 6.12”；GSK 文档页自称 “Version 4.23.4” —— **文档版本与源码版本不同轴**，引用时各自标注（W2D-003）。

## TL;DR（每条带锚）

1. **两家的“批处理”根本不是同一种东西。** Qt 是**保留式 batch 对象**（可比键 = 材质 type+viewCount+compare、drawingMode、attributes、indexType、lineWidth、inheritedOpacity、clipList、mutabilityGroup），跨帧复用 VBO/UBO 池（`QT/coreapi/qsgbatchrenderer.cpp:1800-1811` "&& gniGeometry->attributes() == gnjGeometry->attributes()"；W2D-004/005）。GTK 是**逐帧排序 + 相邻 op 合并**，没有任何跨帧可复用批（W2D-006/007/008）。
2. **Qt 不透明批不做重叠检查**（深度缓冲代劳）；**alpha 批必须做**，且有 “overlapBounds 联合矩形” 剪枝以避开 O(n²)（`QT/coreapi/qsgbatchrenderer.cpp:1842` " * To avoid the O(n^2) checkOverlap check in most cases, we have the"；W2D-009）。GTK 相反：**整套 GPU 渲染器没有深度缓冲路径**，靠节点树顺序 + 遮挡剔除（W2D-010/011）。
3. **Qt 有真正的 render 线程**（每窗口一线程、每线程一个 QRhi，事件驱动，GUI 只在 polishAndSync 处阻塞）；`QSG_RENDER_LOOP` 可强制 basic/threaded（`QT/qsgthreadedrenderloop.cpp:68` "There is one thread per window and one QRhi instance per thread."；W2D-012/013）。
4. **GSK 没有自建渲染线程**：渲染在 GDK frame clock 的 paint 相位里、主线程上执行（`G/gtk/gtknative.c:184` "gsk_renderer_render (renderer, root, region);"；W2D-014/015）。GSK 的“同步”是 Vulkan fence/semaphore 层面的（W2D-016）。
5. **脏区/局部重绘是 GSK 的强项、Qt 的空白**：GSK 把上一帧节点树与新树做 diff（Myers O(ND)）求 damage，并支持按 damage 区域裁剪（`GSK/gskrenderer.c:473`；W2D-017/018）。Qt 的官方立场是整帧重绘、**不做 CPU 侧视口裁剪与遮挡检测**（doc 引文；W2D-019）。
6. **字形缓存与子像素量化粒度可查、可调**：GTK 字形缓存键 = (font 指针, glyph, flags, scale)，flags 是 **4×4 子像素格**（1/4 px），字形按 Cairo 光栅成白色 mask 入图集（W2D-020/021）；Qt 字形格式随抗锯齿模式切换 **A8（灰度）/A32（LCD 子像素）/ARGB（彩色字体）**（`QT/qsgdefaultglyphnode.cpp:119,123`；W2D-022/023）。
7. **图集阈值差异显著**：GTK 固定 `ATLAS_SIZE 1024` + 单项 >256px 不进图集（`GSK/gpu/gskgpucache.c:27,29`；W2D-024）；Qt 图集尺寸 = max(512, nextPow2(窗口尺寸))，尺寸阈值默认 = max(w,h)/2 并可环境变量覆盖（`QT/util/qsgrhiatlastexture.cpp:39,51`；W2D-025/026）。
8. **GTK 的“批”在 GL 后端体现为相邻 shader op 合并成一次 instanced draw**（键含 op_class/flags/color_states/variation/连续 vertex_offset/纹理），由 `GSK_GPU_OPTIMIZE_MERGE` 开关（`GSK/gpu/gskgpushaderop.c:220,230-237`；W2D-006/007）。
9. **Qt 的坑是“批被打散”的三件事**：clip 会打断批、`RequiresFullMatrix` 材质完全不能批、>16 位索引不能批；官方给的目标读数是 “批数 <10，其中 3–4 个不透明”（doc；W2D-027/028/029）。
10. **GTK 的坑是“两个真实回归留档”**：4.1.2 之前“每帧强制全重绘”、4.23.0 才加 `GSK_GPU_DISABLE=damage` 关闭位（`G/NEWS:6876`、`G/NEWS:892`；W2D-030/031）。两家都有“0 墨水字形仍产生 draw”这类细节优化在近期才补（`G/NEWS:126`；W2D-032）。

## 可吸收 / 不可吸收（对“候选窗/自绘渲染器 + 高帧率 UI”这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| “可比键 = 材质比较函数 + 几何属性向量”的批键设计（把状态比较下沉到材质对象一份 `compare()`） | W2D-004/005 | **吸收** |
| 不透明批跳过重叠检查、延迟用深度/顺序解决 | W2D-009/010 | **吸收**（候选窗内容多为不透明小条，可显著省 CPU） |
| overlapBounds 联合矩形剪枝（避免 O(n²) 重叠检测） | W2D-009 | **吸收** |
| batch root/子树渲染计数阈值（节点数 64 / 顶点数 1024）触发“子树根化”，让滚动列表变便宜 | W2D-033/034 | **吸收**（候选窗列表） |
| mutabilityGroup（16 组）把高频变更内容隔离出静态批 | W2D-035 | **吸收** |
| 字形缓存键含 4×4 子像素格 + 缩放档（幂次量化） | W2D-020/036 | **吸收**（CJK 候选窗字形多，命中率关键） |
| 纹理图集 `comparisonKey = 图集指针`（同图集纹理视为相等 → 可批） | W2D-037 | **吸收** |
| 图集分配器：guillotine/BSP 分裂 + snug fit（Qt）/ 8 条空链（GTK） | W2D-038/039 | 有界吸收（候选窗图集尺寸小，简单分配器够用） |
| 每窗口一渲染线程 + 事件驱动同步（GUI 只在 sync 点阻塞） | W2D-012/013 | 有界吸收（青简是常驻小窗，线程收益与复杂度需实测） |
| 渲染节点树 diff 求 damage（Myers O(ND)） | W2D-017/018 | 有界吸收（要 candidate 变动粒度足够粗才有赚） |
| 遮挡剔除（阈值 100k px / 10% 面积才开额外 pass） | W2D-011 | 有界吸收 |
| 距离场字形（Qt `QtRendering`：可缩放但内存大、大字号有伪影） | W2D-040 | 不吸收（理由：CJK 大字符集 + 候选窗小字号，DF 图集收益不抵质量与内存风险） |
| 曲线字形（`CurveRendering`：GPU 直接光栅曲线） | W2D-041 | 不吸收（用 tiny-skia 光栅 + 图集更稳） |
| 3D 混合用的 depth post-pass | W2D-042 | 不吸收（目标无 3D 混合需求） |
| Qt 的整帧重绘模型（无 damage 区域 API） | W2D-019 | 不吸收（候选窗要求局部重绘以压功耗） |
| GTK 的整树快照（每帧新造不可变节点树） | W2D-043 | 不吸收（分配压力大；青简用增量脏标记更合适） |
| GTK 的 op 排序（uploads 全部提前） | W2D-044 | 不吸收（这是在“每帧重建 op 流”前提下的补救） |

## 1. 架构全景（模块地图）

### 1.1 Qt Quick scene graph（`qtdeclarative/src/quick/scenegraph/`，174 个 .cpp/.h，2.3MB）

- `coreapi/` —— 公共/核心 API 与默认批渲染器：
  - `qsgnode.{h,cpp}`：节点模型（`BasicNodeType/GeometryNodeType/TransformNodeType/ClipNodeType/OpacityNodeType/RootNodeType/RenderNodeType`，`coreapi/qsgnode.h:40-48`），脏位枚举（W2D-045）、`m_subtreeRenderableCount` 子树可渲染计数（`coreapi/qsgnode.h:143`；W2D-033）。
  - `qsgnodeupdater.cpp`：遍历节点树算矩阵/裁剪/不透明度并写回渲染树（W2D-046）。
  - `qsgmaterial.{h,cpp}` + `qsgmaterialshader.cpp` + `qsgmaterialtype.h`：材质 = 着色器状态封装 + `compare()` 排序键（W2D-004）。
  - `qsgbatchrenderer.cpp`（4347 行）+ `_p.h`：**默认渲染器本体**——影子树（`Node`）、元素（`Element`）、批（`Batch`）、渲染列表、批构建、上传、绘制、可视化（W2D-047/048）。
  - `qsggeometry.{h,cpp}`、`qsgtexture.{h,cpp}`、`qsgrendernode.{h,cpp}`（外部渲染注入点）、`qsgrenderer.cpp`、`qsgabstractrenderer.cpp`。
- `util/` —— 便利节点与资源：`qsgrhiatlastexture.cpp`（纹理图集）、`qsgareaallocator.cpp`（图集分配器）、`qsgtextnode.{h,cpp}`（**公开文本节点**，Qt 6.7+）、`qsgimagenode/qsgninepatchnode/qsgsimplerectnode/...`、`qsgflatcolormaterial.cpp`、`qsgvertexcolormaterial.cpp`、`qsgrhiatlastexture` 相关。
- 根目录 —— 上下文与渲染循环：`qsgcontext.cpp`/`qsgdefaultcontext.cpp`（适配层装配 + 抗锯齿模式决策，W2D-049）、`qsgrenderloop.cpp`（选择 basic/threaded，W2D-013）、`qsgthreadedrenderloop.cpp`（1843 行，渲染线程与同步协议，W2D-012）、`qsgrhisupport.cpp`（RHI 后端选择，1651 行）、`qsgbasicinternalrectanglenode.cpp`（顶点抗锯齿矩形，1101 行）、`qsgrhilayer.cpp`（item layer 离屏）、`qsgdefaultglyphnode*`（纹理字形节点）、`qsgdistancefieldglyphnode*`（距离场字形）、`qsgrhitextureglyphcache.cpp`、`qsgrhidistancefieldglyphcache.cpp`、`qsgcurve*`（曲线光栅）、`shaders_ng/`（.frag/.vert 源）、`adaptations/software/`（软件后端）。

### 1.2 GTK4 GSK（`gtk/gsk/`，本快照 11MB）

- 渲染节点体系（公共 API + 实现）：`gskrendernode.{c,h}`、`gskrendernodeprivate.h`、`gskrendernodeimpl.c`、`gskrendernodeparser.c`、每个节点一个文件（`gskcolornode.c`、`gskclipnode.c`、`gsktextnode.c`、`gskfillnode.c`、`gskstrokenode.c`、`gskisolationsnode.c`…）。
- 渲染器基类与后端：`gskrenderer.{c,h}`（公共 `gsk_renderer_render()` + damage diff，W2D-017）、`gskcairorenderer.c`（软件）、`gpu/gskgpurenderer.c`（GPU 渲染器基类，W2D-050）、`gpu/gskglrenderer.c`（GL 3.3+）、`gpu/gskvulkanrenderer.c`（Vulkan 1.2+）。
- **共用 GPU 基础设施（`gsk/gpu/`，97 个文件 —— 本快照里 GL 与 Vulkan 共享这一层，旧的 `ngl/` 目录已不存在，Vulkan 也不再单独成目录）**：
  - 帧与 op 流：`gskgpuframe.c`（op 池、seal/sort、submit，W2D-044）、`gskgpuop.c`、`gskgpuopprivate.h`（`GskGpuStage` 六态，W2D-051）、`gskgpushaderop.c`（**合并与状态省略的核心**，W2D-006/007）。
  - 节点→op 翻译：`gskgpunodeprocessor.c`（4291 行，W2D-052）。
  - 缓存与图集：`gskgpucache.c`（尺寸常量、逐出、GC，W2D-024）、`gskgpucachedglyph.c`（字形，W2D-020）、`gskgpucachedfill.c`（路径填充 mask，W2D-053）、`gskgpucachedstroke.c`、`gskgpucachedtile.c`、`gskgpucachedatlas.c`、`gskatlasallocator.c`（槽/空链表分配器，W2D-039）。
  - 局部与剔除：`gskgpuocclusion.c`（遮挡剔除，W2D-011）、`gskgpuclip.c`、`gskgpuscissorop.c`、`gskgpuimage.c`、`gskgputransform.c`。
  - 后端细节：`gskglimage.c`/`gskglpipeline.c`/`gskglframe.c`/`gskgldevice.c`、`gskvulkan*.c`（buffer/image/memory/pipeline/frame/ycbcr）、`shaders/`。
- 支撑：`gskdiff.c`（Myers 差分，W2D-018）、`gskoffload.c`（dmabuf 直通）、`gskrenderreplay.c`（4.22 新：可改写的节点重放）、`gsktransform.c`/`gskrectsnap.c`/`gskcontour.c`/`gskpath.c`（几何）。

## 2. 关键机制

### 2.1 渲染节点模型与批处理键

**Qt**：渲染树是 `QSGNode` 树（无 `paint()` 虚函数；doc：节点只是数据，渲染器解释它，W2D-054）。默认渲染器维护**影子树**：为每个 `QSGNode` 建 `Node`（含 `dirtyState`、`isBatchRoot`，`coreapi/qsgbatchrenderer_p.h:539-543`），为每个几何节点建 `Element`（含 device 坐标 `bounds`、`order`、`mutabilityGroup`、RHI 管道指针，W2D-055）。每帧遍历渲染树产出两个列表（不透明/alpha，W2D-056）：

- 判定不透明：`inheritedOpacity() > 0.999f && !(material->flags() & Blending)`（`QT/coreapi/qsgbatchrenderer.cpp:80,1545`；W2D-010）。
- 批键（**逐项相等才可并批**）：`clipList`、`drawingMode`、lineWidth（仅 DrawLines 时要求相等且为 1.0）、`attributes()`、`indexType()`、`inheritedOpacity`、材质 `type()`+`viewCount()`+`compare()==0`、`mutabilityGroup`（W2D-004/005/057）。
- 不透明批**从渲染列表尾部向前扫**且不做重叠检查（W2D-058）；alpha 批向后扫，遇重叠即**中断当前批**（`W2D-009`）；材质比较不等时若已有批成员则整批作废重建（`isMaterialCompatible` 返回 `BatchBreaksOnCompare`，W2D-059）。
- 边界不在稳定浮点范围（`|coord| > 1e6`，`qsgbatchrenderer_p.h:40`）或矩阵非 2D 安全时，整批降级为 unmerged 直通（`isSafeToBatch()`；W2D-060/061）。
- 几何属性向量变化 → 只标 `needsUpload`；否则批失效（`Batch::geometryWasChanged`；W2D-062）。索引宽：RHI 不支持非 4 字节对齐偏移时自动用 32 位索引（`QSG_RHI_UINT32_INDEX` 可强制；W2D-063）。
- 管道缓存与 SRB 池：`QHash<GraphicsPipelineStateKey, QRhiGraphicsPipeline*> pipelineCache`（键含 GraphicsState 哈希 + 着色器指针 + 渲染目标/SRB 布局序列化），SRB 按布局描述串池化（阈值 1024）（W2D-064/065）。
- 内存池：`Allocator<Node,256>`、`Allocator<Element,64>`（分页槽分配，页空且位于尾部即回收），VBO/IBO 池 2MB 上限（`DEFAULT_BUFFER_POOL_SIZE_LIMIT`；W2D-066）。

**GTK**：节点是**不可变**的（doc：“All GskRenderNode s are immutable”，W2D-067）；渲染器把节点树翻译成**线性 op 流**（`GskGpuStage`：UPLOAD / PASS / COMMAND / SHADER + BEGIN_PASS/END_PASS 魔法态，W2D-051）。op 记录进一块字节数组（`gsk_gpu_ops` splice/index），提交前 `seal_ops` 串链 + `sort_ops` 重排（W2D-044）：

- 排序规则：每个 render pass 内 **uploads 全部前提**、commands 保持原序；子 pass（BEGIN_PASS 嵌套）整体提到父 pass 前（`gskgpuframe.c:424-524`；W2D-068）。
- 绘制时的“批”= **相邻 shader op 合并**：从当前 op 向后吞并满足 (同 op_class、同 node_id、同 flags/color_states/variation、`vertex_offset` 连续、0/1 号纹理与采样器相同) 的 op，合成一次 instanced draw；由 `GSK_GPU_OPTIMIZE_MERGE` 决定 `max_ops_per_draw = MAX_MERGE_OPS` 或 1（W2D-006/007）。
- GL 侧状态省略：只有 (op_class, color_states, variation, flags) 变化才换 program；纹理/采样器/裁剪 mask 逐个比较后再绑定（W2D-008）。
- 每帧从**共享顶点缓冲**预留区间（`reserve_vertex_data`），globals/storage buffer 同理，提交前 unmap（W2D-069）。

### 2.2 纹理图集与字形缓存

**Qt**

- 图集管理器：`QSG_ATLAS_WIDTH/HEIGHT` 默认 `max(512, nextPow2(窗口尺寸))`，`QSG_ATLAS_SIZE_LIMIT` 默认 `max(w,h)/2`；`CoverWindow` 类型窗口图集减半（省内存优先）（W2D-025/026/070）。
- 入集条件：宽高**都**小于 size_limit（W2D-071）；分配 `image + 2px` 边框，上传时补一圈 padding（1px）与四角，避免采样渗色；`QSG_ATLAS_TRANSIENT_IMAGE_THRESHOLD`（默认 0 = 不留原图）控制是否为“快速移出图集”保留 QImage（W2D-072/073/074）。
- 分配器 `QSGAreaAllocator`：二叉树 guillotine 分裂，选择“剩余面积更大”的方向切；`size + maxMargin >= rect` 时判 snug fit 直接占用整块（W2D-038）。
- 批友好性：`TextureBase::comparisonKey()` 返回**图集指针**，使同一图集的纹理在批键比较中视为相等（W2D-037）。
- 字形缓存（纹理字形）：`QSGRhiTextureGlyphCache` 管理一张 glyph cache 纹理（最小 128×32，向上扩；OpenGLES2 之外走 `copyTexture` 扩容，否则 CPU 侧扩图后整块上传），**新建纹理必须清零**否则字形周围出伪影（W2D-075/076/077）。子像素 AA 的字形在**半透明目标**上时把 alpha 设为 RGB 均值（W2D-078）。
- 距离场字形：独立缓存（`qsgrhidistancefieldglyphcache.cpp`，611 行）支持从字体内嵌 `qtdf` 表直接装载预烘焙 DF 纹理，含尺寸/纹理大小合法性校验（W2D-079）。
- 字形节点装配（`qsgdefaultglyphnode_p.cpp`）：`supportsHorizontalSubPixelPositions()` 决定是否向缓存请求子像素位置；顶点位置按 **glyphCacheScale 缩放后再取整（floor/round）**，即设备像素网格上的整数定位（W2D-080/081）。

**GTK**

- 图集尺寸固定 `ATLAS_SIZE 1024`，单项 >`MAX_ATLAS_ITEM_SIZE 256` 直接不走图集（返回 NULL → 改用独立上传图像）（W2D-024/082）。
- 分配器 `GskAtlasAllocator`：槽数组 + 5 态（FREE/FREELIST/EMPTY/CONTAINER/USED）+ **8 条空链表**（按尺寸分级），支持分裂/合并/容器父子合并（W2D-039/083）。
- 缓存统一走 `GskGpuCache`：全局双向链表 + 时间戳（`gsk_gpu_cached_use` 刷时间戳，`is_old` 按 `cache_timeout` 判老），陈旧项在有图集时只标 stale、**字形仅随图集释放而回收**（W2D-084/085）。
- 字形：一个字形一个 `GskGpuCachedGlyph`，键 = (font 指针, glyph, flags, scale)，哈希 = `font ^ glyph ^ (flags<<24) ^ (guint)scale*PANGO_SCALE`；光栅方式 = 用 `gsk_reload_font(font, scale, hint metrics/style, antialias default)` 后 `pango_cairo_show_glyph_string` 画成**白色 mask** 入图集，绘制时按颜色 colorize（W2D-020/086/087）。
- 字形 padding = 1px；图集内位置与 `origin` 记录子像素余量（`cache->origin = (-origin.x + subpixel_x, ...)`；W2D-088/089）。
- 路径填充/描边也有缓存：`gskgpucachedfill`/`cachedstroke`，形状以 **32 阶子像素网格**量化（`determine_scale_and_subpixel_grid` 在缩放为 2 的幂时把子像素网格退化为 1）（W2D-053/090）。

### 2.3 文本节点与子像素/灰度

**Qt**：`QSGTextNode`（6.7+）把“已排版好的文本”加入渲染树，渲染类型三选一（doc）：`QtRendering`=可缩放距离场、`NativeRendering`=平台原生、`CurveRendering`=GPU 曲线光栅（W2D-040/041）。灰度/子像素由**抗锯齿模式 → 字形格式**决定：`GrayAntialiasing → Format_A8`、`High/LowQualitySubPixelAntialiasing → Format_A32`（LCD 子像素）、彩色字体强制 `Format_ARGB` 且不被覆盖（W2D-022/023）。模式决策在 `QSGDefaultContext`：环境变量 `QSG_DISTANCEFIELD_ANTIALIASING`（`subpixel`/`subpixel-lowq`/其它）可强制，否则按平台/字体启发式（默认 `HighQualitySubPixelAntialiasing`）（W2D-049/091/092）。字形材质按 glyphFormat 分为 argb/rgb/gray 三种 `QSGMaterialType` —— 即**灰度与子像素字形天然落在不同批**（W2D-093）。

**GTK**：`GskTextNode` 持有 `PangoFont + PangoGlyphInfo 数组 + GdkColor + baseline offset`（W2D-094）。构造时：跳过空字形（`PANGO_GLYPH_EMPTY`）、空 ink 框直接返回 NULL 节点、记录 `hint_style`；**`needs_blending = n > 1`**、**`bilevel_opacity = !has_color_glyphs && color 不透明`**（W2D-095/096/097/098）。`bilevel_opacity` 的语义是“每个坐标要么全不透明要么全透明”，直接喂给遮挡剔除与 `BLEND_NONE` 路径（W2D-099/100）。diff 时逐字形比较 (glyph, width, x/y_offset, cluster_start, is_color)，任一不同即“不可能 diff” → damage 取两节点 bounds 并集（W2D-101/102）。

子像素定位在节点处理器里做（`gskgpunodeprocessor.c:2643-2677`）：缩放取 `max(scale.x, scale.y)`；若变换 ≤2D 类别则**缩放到 2 的幂且 `flags_mask = 0`（不做子像素偏移）**；否则若 hint_style≠NONE → x 轴 1/4 px、y 轴整像素（`align_scale_x = scale*4, align_scale_y = scale`）；无 hinting → 两轴都 1/4 px；`flags = (x&3) | ((y&3)<<2) & flags_mask` 参与缓存键（W2D-036/103/104/105）。彩色字形走 `texture_op`，普通字形走 `colorize_op`（白 mask × 颜色）（W2D-106）。带不透明度且是彩色字形时先下离屏（`add_with_offscreen`）（W2D-052）。

### 2.4 脏区 / 局部重绘

**GTK（强）**：`gsk_renderer_render(renderer, root, region)` 的语义：调用方给 region，渲染器**必须**保证该区域被重绘，但**可以**不重绘区域外未变像素（doc 引文 W2D-107）。实现上保留 `prev_node`，把 `gsk_render_node_diff(prev, root, {region, ...})` 的 damage 并进 clip；`region==NULL`、无 prev_node、或调试 `FULL_REDRAW` 时才整屏（W2D-017/018/108）。diff 框架：同类型节点走各自的 `node_class->diff()`，否则若一侧是容器则让容器做 diff，再否则“不可能 diff” → 取两 bounds 并集（W2D-109/110）。差分算法是 Myers 的 O(ND) 实现（`gskdiff.c` 头注直接点名 Libenzi/Myers；W2D-018）。GPU 渲染器侧：`GSK_GPU_OPTIMIZE_DAMAGE` 打开时才按多矩形 damage 裁剪，否则退化为 damage 的外接矩形；空 region 直接 `gdk_draw_context_empty_frame()` 跳过（W2D-111/112）。每帧前后做缓存 GC 判定与排队（`maybe_gc`/`queue_gc`，W2D-113）。`GSK_GPU_DISABLE=damage` 是 4.23.0 新增的关闭位（W2D-031）。

**Qt（弱，且是有意的）**：`QSGNode::markDirty` 把脏位沿父链上抛时，只对 `DirtyNodeAdded/Removed` 等调整 `m_subtreeRenderableCount`（`coreapi/qsgnode.cpp:652-667`），`DirtyPropagationMask` 明确**只含** `DirtyMatrix|DirtyNodeAdded|DirtyOpacity|DirtyForceUpdate` —— **几何与材质变更不向上传播**（W2D-045/114）。批渲染器在影子树上分档响应：`DirtyMatrix` 且非批根 → 按子树计数/顶点数决定是否把该变换节点“根化”；`DirtyGeometry` → 改 mutability 组要全量重建、否则单批 needsUpload 或作废；`DirtyMaterial` → 混合状态翻转要全量、比较失败作废批；上抛链只带 `DirtyNodeAdded|DirtyOpacity|DirtyMatrix|DirtySubtreeBlocked|DirtyForceUpdate`（W2D-115/116/117/118）。渲染侧没有任何 damage/部分重绘 API，官方文档亦把“不做 CPU 侧裁剪与遮挡检测”写成设计取舍（W2D-019）；窗口仍是每帧呈现整帧。

### 2.5 渲染线程与主线程同步（render loop）

**Qt**：`QSGThreadedRenderLoop` + `QSGRenderThread`，全部通信走自定义事件（`WM_RequestSync`/`WM_TryRelease`/`WM_Grab`/`WM_Exposed`/`WM_Obscure`…）；设计注释写明：**渲染线程从不被阻塞，GUI 在 polishAndSync 单点阻塞**，等待渲染线程取走并完成 sync 后释放（W2D-012/119/120）。每窗口一线程、每线程一个 QRhi（W2D-121）。`m_lockedForSync` 作为“GUI 已阻塞”的门闩（W2D-122）。线程亲和：窗口显示前线程属于 GUI 线程，显示后归属渲染线程，退出时迁回（W2D-123）。动画：threaded 循环装**两个**动画驱动（GUI 线程 + 渲染线程），以 vsync 节流为准；6.5 起可用 `QSG_USE_SIMPLE_ANIMATION_DRIVER` 换成纯耗时驱动；窗口数 ≠1 或检测到 vsync 失效（“broken vsync throttling”）时回退系统定时器（W2D-124/125/126/127）。循环选择：非 OpenGLES2 后端默认 threaded，OpenGLES2 看 `ThreadedOpenGL` 能力，`QSG_RENDER_LOOP` 可强制；`windows` 值已废弃（W2D-013/128）。

**GTK**：GSK 自身不建线程；GTK 在 `surface_render_cb`（GdkSurface 的 render 信号）里 `gtk_snapshot_new()` → `gtk_widget_snapshot()` → `gsk_renderer_render()`（W2D-014/129）。驱动来自 `GdkFrameClock`：空闲直到有人 `request_phase()`，然后按相位发信号 `flush-events / before-paint / update / layout / paint / after-paint`（`gdkframeclock.c` 枚举 W2D-130），帧时间在绘制期间不推进（用于动画同步）（W2D-131）；idle 时钟用 GSource 挂主上下文（`g_source_attach (priv->source, NULL)`，W2D-015）。Vulkan 后端在帧层面做 fence/semaphore 同步（`vkWaitForFences` 等，W2D-016），即“同步在 GPU 后端而非线程模型”。**核对：task 简报里写的 `gsk/ngl`、`gsk/vulkan` 两个目录在本快照不存在**（见 §4 坑与反例，W2D-132）。

### 2.6 命令行/环境变量开关（可复算的调参面）

Qt：`QSG_RENDER_LOOP`、`QSG_RHI_BACKEND`、`QSG_RHI_DEBUG_LAYER`、`QSG_RHI_PREFER_SOFTWARE_RENDERER`、`QSG_RENDERER_DEBUG=render`（批统计）、`QSG_RENDER_TIMING=1`、`QSG_VISUALIZE=batches|clip|changes|overdraw`、`QSG_ATLAS_WIDTH/HEIGHT/SIZE_LIMIT/OVERLAY/TRANSIENT_IMAGE_THRESHOLD`、`QSG_RENDERER_BATCH_NODE_THRESHOLD`(64)/`QSG_RENDERER_BATCH_VERTEX_THRESHOLD`(1024)/`QSG_RENDERER_SRB_POOL_THRESHOLD`(1024)/`QSG_RENDERER_BUFFER_POOL_LIMIT`、`QSG_RENDERER_BUFFER_STRATEGY`（static/stream/dynamic）、`QSG_ANTIALIASING_METHOD=vertex|msaa`、`QSG_DISTANCEFIELD_ANTIALIASING`、`QSG_BATCHRENDERER_MINIMUM_ORDER_PADDING`、`QSG_RHI_UINT32_INDEX`、`QSG_NO_VSYNC`（W2D-133/134/135/136/137）。

GTK：`GSK_RENDERER=gl|vulkan|cairo|ngl`（选后端，`gsk_renderer_new_for_name`，W2D-138）、`GSK_GPU_DISABLE=damage|...`（W2D-031）、`GSK_RENDERER_DEBUG=verbose|diff|full-redraw|...`（W2D-108）、`GSK_GPU_OPTIMIZE_*`（merge/damage/occlusion culling/dual blend/profile；GL 后端按扩展能力裁剪 `GSK_GPU_OPTIMIZE_DUAL_BLEND`，W2D-139）。

## 3. 性能手段与公开读数（数字必须带锚）

| 手段 | 锚 | 口径/数字 |
| --- | --- | --- |
| Qt 官方批数目标 | W2D-029（doc: qtquick-visualcanvas-scenegraph-renderer.html） | “batches should be fewer than 10 and at least 3-4 of them should be opaque”；用 `QSG_RENDERER_DEBUG=render` 读 |
| Qt 批根阈值 | W2D-033/034（源码默认值） | 节点数 64 / 顶点数 1024 |
| Qt 不透明阈值 | W2D-010 | `inheritedOpacity() > 0.999f` |
| Qt 坐标安全界 | W2D-060 | `±1000000.0f`，超出即 unmerged |
| Qt 缓冲池上限 | W2D-066 | 2MB（VBO/IBO 各一） |
| Qt 图集尺寸 | W2D-025/026 | `≥512` 且按窗口尺寸取 2 的幂；单项阈值 `max(w,h)/2` |
| Qt 字形缓存纹理下限 | W2D-075 | 宽 ≥128、高 ≥32 |
| Qt 动画节流 | W2D-124/126 | 60Hz 视作 16.67ms；GUI 侧 `requestUpdate` 由 5ms 定时器兜底（文档口径） |
| GTK 图集 | W2D-024/082 | 1024×1024，单项 >256px 不入集 |
| GTK 填充子像素格 | W2D-090 | 32 阶；缩放为 2 的幂时降为 1 |
| GTK 字形子像素格 | W2D-036 | 4×4（1/4 px） |
| GTK 遮挡剔除阈值 | W2D-011/140 | 额外 pass 需 ≥ max(100000 px, 目标面积 10%) |
| GTK 零墨水字形 | W2D-032（NEWS 4.23.4） | “avoid GPU draw ops for zero-ink glyphs”，即此前空字形也会出 draw |
| GTK 历史回归 | W2D-030/031（NEWS 4.1.2 / 4.23.0） | “Don't force a full redraw for every frame”（4.1.2）；`GSK_GPU_DISABLE=damage`（4.23.0） |
| GTK 性能监控 | W2D-141（NEWS 4.21.5） | “Add rendernode performance monitoring”；遮挡剔除在 4.15.4/4.15.5 两轮增强 |
| Qt 官方性能指南 | W2D-142（doc: qtquick-performance.html） | 该页为**建议清单**（如“Batch up backend operations…”），未给实测数字——引用时不得当读数用 |

**读数缺口（诚实记录）**：本次未取到任何“同机同场景 Qt vs GTK 帧时间/批数”的一手数字；上面所有带数字的条目都是**设计常量或官方目标值**，不是实测（W2D-143）。要判定“谁更快”必须自建基准，禁止用本报告横比。

## 4. 坑与反例（负面留档）

1. **任务简报里的路径已过时**：`gsk/ngl`、`gsk/vulkan` 在本快照均不存在；GL/Vulkan 渲染器与共用 op 基础设施都在 `gsk/gpu/`，旧 `gl/` 目录只剩 fp16 辅助（`gl/fp16.c` 等）。按简报路径 clone 会 `sparse-checkout set` 出空目录（W2D-132）。
2. **Qt 的批会被三件事打断**：clip（每个 clip 子树需要独立状态）、`RequiresFullMatrix`（“prevents all batching”）、>16 位索引；且**多像素重叠的复合项无法并批**（W2D-027/028/029）。
3. **Qt 顶点抗锯齿会把整块图元变成需要混合**，低端硬件上大面积圆角/图片会显著掉帧；官方建议优先 MSAA（W2D-144/145）。
4. **Qt `Batching only works for 16-bit indices`**：自绘几何用 32 位索引会掉出合并路径（W2D-028）。
5. **Qt 的 mipmap 图片不进图集、也不会被批**（W2D-146）。
6. **Qt 字形缓存驱动 bug 的妥协**：`QML_USE_GLYPHCACHE_WORKAROUND` 保留 RAM 副本，代价是首次绘制更慢 + 字形缓存内存翻倍（文档自述）（W2D-147）。
7. **Qt mutabilityGroup 只 16 组**，滥用会把本可合并的静态内容拆散（W2D-035）。
8. **GTK 文档与源码版本不同轴**（docs 4.23.4 vs repo 4.24.1），按文档描述实现细节有踩空风险（W2D-003）。
9. **GTK 的 `GskGLRenderer` 需要 GL 3.3**，低于此直接报错（W2D-148）；dual-blend 等优化按扩展能力动态关闭（W2D-139）。
10. **GSK 节点不可变 → 每帧重建整棵树**（GTK 在 render 回调里 snapshot 出新树），分配与 diff 成本是常态开销；这也是它要做 op 排序与 upload 提前的原因（W2D-043/044/067）。
11. **GTK 字形仅随图集释放而回收**（`cached_glyph_should_collect`），长期运行的大字符集场景要盯图集数量而非字形条目数（W2D-085）。

## 5. 未验证项（写明缺什么证据）

- **无一手性能测量**：两家的帧时间、批数、图集命中率、字形首次光栅耗时均未实测（本任务只读、不编译）。判据缺口 = 缺“同机同分辨率同内容”的对照基准（W2D-143）。
- **Qt 距离场路径细节未读全**：`qsgrhidistancefieldglyphcache.cpp` 只读了 texInfo/合法性与 `qtdf` 装载部分，双分辨率（`m_doubleGlyphResolution`）、半径/缩放常量（`QT_DISTANCEFIELD_RADIUS/SCALE`）只在 `qsgadaptationlayer_p.h` 出现引用，未追其定义（可能在 qtbase）。
- **Qt 软件后端（`adaptations/software/`）未读**：本报告全部 Qt 结论均默认 RHI 硬件路径。
- **GTK op 合并的实测收益未知**：`MAX_MERGE_OPS` 取值、`GSK_GPU_OPTIMIZE_*` 的默认装配（哪些后端默认开）未在本次范围内确认。
- **GTK 的 GDK 侧帧调度（`GdkFrameClockIdle` 的 vsync 对齐算法、`gdksurface.c` 何时发 render 信号）只读到片段**，未读全；本报告只声明“渲染在主线程 frame clock 相位内”，未声明具体调度精度。
- **`gsk/gskoffload.c` 的 dmabuf 直通**只读到文件与 blog 描述，未读实现。
