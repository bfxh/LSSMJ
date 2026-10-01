# Android：HWUI / RenderThread / Choreographer / Compose（aosp-mirror/platform_frameworks_base @ 1cdfff555f4a21f71ccc978290e2e212e2f8b168，2026-10-01 抓取）

> 轨道 B9。**source 深度来自定点抓取**（不是全仓克隆）：`scratch/src/aosp-hwui/` 下 16 个文件
> （`core/java/android/view/{Choreographer,ThreadedRenderer,ViewRootImpl}.java`、`libs/hwui/**`），
> 抓取方式 = `raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/<sha>/<path>`（
> `android.googlesource.com` 在本机不可达，故用 GitHub 镜像；sha 取自该镜像 `main` 的 HEAD）。
> Compose 侧 = androidx/androidx @ 7b4bec74646d0928c8862e1ed2932b1ebb87b144（`androidx-main`），
> 经 `gh api .../contents/...` 探索清单后定点取 3 个文件（`scratch/src/androidx-compose/...`）。
> 账本：`w2f.jsonl` W2F-050..069（source 20 条）+ W2F-080..088（doc 9 条）。

## TL;DR（每条带锚）

1. **帧的驱动是 Choreographer 的五档回调，顺序固定且输入最先**：`CALLBACK_INPUT = 0` … `CALLBACK_COMMIT = 4`
   （W2F-050、W2F-053 `Choreographer.java:280/322`），`doFrame` 内按 INPUT → ANIMATION → INSETS_ANIMATION →
   TRAVERSAL → COMMIT 串行执行（W2F-051 `:1097`、W2F-052 `:1106`）。**提交是帧内最末的独立阶段。**
2. **帧间隔由运行时刷新率整除得到**：`mFrameIntervalNanos = (long)(1000000000 / getRefreshRate())`（W2F-054 `:338`），
   且每档回调都把同一个 `frameIntervalNanos` 作为预算传下去（W2F-051）。
3. **掉帧是"整帧作废"**：官方口径 "your app must render frames in under 16ms to achieve 60 frames per second"（W2F-082），
   超时后 Choreographer "drops the frame entirely"（W2F-083）——不是延后显示。
4. **一次帧有一次明确的跨线程同步点**：UI 线程 `post` 绘制任务给渲染线程后 `mSignal.wait(mLock)`（W2F-057、W2F-058
   `DrawFrameTask.cpp:85/86`）。
5. **RenderNode 用双份属性做线程隔离**：`mProperties`（渲染/动画侧）与 `mStagingProperties`（UI 侧 staging）（W2F-059、W2F-060）。
6. **damage 是一等公民且直接变成画布裁剪**：`androidFramework_setDeviceClipRestriction(layerDamage.toSkIRect())`（W2F-061 `SkiaPipeline.cpp:94`）；
   未脏的层不参与本帧录制（W2F-062 `:207`）。
7. **damage 有独立的累加器与坐标系归一**：`DamageAccumulator` 维护栈式变换，弹栈时把子脏区映射到父空间（W2F-064），
   根节点无变换故栈底即窗口空间脏矩形（W2F-065）。
8. **FrameInfo 用不同哨兵区分不同语义**：`FrameDeadline` 的未知值 = `INT64_MAX`（W2F-055 `FrameInfo.h:95`），
   而 `INVALID_VSYNC_ID = -1`（W2F-056 `:83`）。
9. **刷新率是运行时订阅量**：渲染线程有 `refreshRateCallback(int64_t vsyncPeriod, ...)`（W2F-066 `RenderThread.cpp:158`）——
   与 Apple 的 CADisplayLink 范围协商同源问题。
10. **Compose 把帧拆成 composition→layout→drawing 三段，并靠"各阶段内的状态读集"跳过阶段**（W2F-080、W2F-081）；
    其帧时钟就是 Choreographer（W2F-068 `AndroidUiFrameClock.android.kt:52`），层的绘制最终录成平台 RenderNode 显示列表
    （W2F-067 `DeviceRenderNode.android.kt:70`）。

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| damage 直接下沉为画布裁剪 | W2F-061 | **吸收**——LSSMJ tile 重栅格必须"算出脏区就真的只画脏区" |
| 脏层收集（未脏层不重录显示列表） | W2F-062 | **吸收**——脏要贯通到显示列表层，不只在光栅层 |
| DamageAccumulator 的栈式变换 + 坐标归一 | W2F-064/065 | **吸收**——damage 必须在窗口空间归并，否则跨层不可比较 |
| 帧内阶段命名 + 提交作为最末独立阶段 | W2F-050/051/052 | **吸收**——LSSMJ 出帧循环应显式命名阶段，提交单点且可测 |
| 本帧预算贯穿各阶段（frameIntervalNanos 作参数） | W2F-051/054 | **吸收**——预算用参数穿，不用全局时钟；刷新率运行时读 |
| FrameInfo 的哨兵 discipline | W2F-055/056 | **吸收**——计时字段的"未设置"与"合法零"必须可区分 |
| 帧间隔 = 刷新率整除（整数纳秒） | W2F-054 | **吸收**——掉帧判定基于运行时刷新率，不许硬编码 16.6ms |
| RenderNode 属性双份（staging/properties） | W2F-059/060 | **有界吸收**——单线程出帧时不需要；引入跨线程 damage 栅格化时再照抄 |
| UI 线程等渲染线程（postAndWait） | W2F-057/058 | **有界吸收**——单线程模型下不存在；跨线程时这是必须显式建模的等待点（含超时策略） |
| Compose 的"阶段内状态读集"失效粒度 | W2F-081 | **有界吸收**——我们只取"信号→两级标脏"的最小版；按阶段记读集记为后续可选 |
| 稳定 key 避免重组 / derivedStateOf | W2F-085/086 | **有界吸收**——候选行按稳定 id 复用布局与字形；不引入响应式运行时 |
| Compose 三阶段模型（composition 单独成段） | W2F-080 | **有界吸收**——我们的等价物是"内容版本号 + build"，不引入独立 composition 段 |
| 层录制成 RenderNode 显示列表 | W2F-067 | 吸收（同层定位）——与 LSSMJ DisplayList 同层；但**不抄 Android 的 API 形态** |
| Compose 的 benchmark 结论（1.9 追平 Views） | W2F-084 | 不吸收（样本单点）——Pokedex 单 app、官方口径；只能作旁证 |
| SurfaceFlinger 的 FPS 节流（帧率须为刷新率除数） | W2F-087 | 有界吸收——与 Apple 同一约束；小窗不需要按应用节流 |

## 1. 架构全景（分层与职责）

```text
应用（View 体系 / Compose）
  ├─ View：ViewRootImpl.performTraversals → ThreadedRenderer → DrawFrameTask（W2F-057）
  └─ Compose：AndroidComposeView → LayoutNode → GraphicsLayer（record 成 RenderNode 显示列表，W2F-067/069）
        │  帧时钟 = AndroidUiFrameClock → Choreographer.postFrameCallback（W2F-068）
        ▼
UI 线程（Choreographer 五档回调，W2F-050..052）
        │  postAndWait：投递 + 等待（W2F-057/058）
        ▼
RenderThread（hwui）
  ├─ CanvasContext：damageAccumulator、waitOnFences、prepareAndDraw（W2F-063）
  ├─ DrawFrameTask：run() 读 FrameInfo（vsyncId）并同步
  ├─ SkiaPipeline：按层重绘，damage → 裁剪（W2F-061/062）
  └─ DamageAccumulator：栈式变换下的脏区归并（W2F-064/065）
        ▼
GPU / Skia → SurfaceFlinger（系统合成器）→ HWC → 显示
```

- **`libs/hwui/` 的关键文件**（`scratch/src/aosp-hwui/libs/hwui/`）：
  `RenderNode.{h,cpp}`（节点 + 双份属性）、`DisplayList.h`、`DamageAccumulator.{h,cpp}`、
  `FrameInfo.h`、`Properties.cpp`（系统属性开关）、`pipeline/skia/SkiaPipeline.cpp`、
  `renderthread/{RenderThread,CanvasContext,DrawFrameTask}`。
- **`core/java/android/view/`**：`Choreographer.java`（帧驱动）、`ThreadedRenderer.java`（渲染器门面）、
  `ViewRootImpl.java`（遍历入口，13.5k 行）。

## 2. 关键机制

### 2.1 帧调度（Choreographer）

- 五档：`CALLBACK_INPUT=0`、`CALLBACK_ANIMATION=1`、`CALLBACK_INSETS_ANIMATION=2`、`CALLBACK_TRAVERSAL=3`、`CALLBACK_COMMIT=4`（W2F-050/053）。
- `doFrame` 内逐档 `doCallbacks(..., frameIntervalNanos)`（W2F-051/052）：**每档都拿到同一份本帧预算**。
- 帧间隔 = `1000000000 / getRefreshRate()`（W2F-054）——刷新率是运行时属性（可变刷新率下会变，见 W2F-066）。
- 超时后果是丢帧（W2F-083），官方基准是 60Hz 下的 16ms（W2F-082）。

### 2.2 同步与线程（DrawFrameTask）

- `postAndWait()`：`queue().post(...)` 之后 `mSignal.wait(mLock)`（W2F-057/058）——UI 线程**阻塞等**渲染线程完成一次 sync。
- 属性双份：`mProperties` / `mStagingProperties`（W2F-059/060），staging 提交后成为渲染侧可见版本。

### 2.3 damage 与显示列表（对 LSSMJ 最有用）

- `CanvasContext` 每帧把 `mDamageAccumulator` 挂进绘制信息（W2F-063）。
- `DamageAccumulator` 维护 `DirtyStack`，弹栈时按当前变换把子脏区映射到父空间；
  文档注释明确"*NOT* transformed by pushed transforms"是指 `peekAtDirty` 的返回语义（W2F-064）；
  根节点无变换，故最终得到的是**已完全映射**的脏矩形（W2F-065）。
- `SkiaPipeline::renderLayerImpl(RenderNode*, const Rect& layerDamage)`：把 damage 设为设备裁剪（W2F-061）；
  `collectLayers` 只收集带 damage 的层（W2F-062）。

### 2.4 文本/字形

本报告**不对 Android 文本栈下结论**（见 §5 未验证项第 3 条）：hwui 的字形路径在
`libs/hwui/glyph/` 与 `pipeline/skia/SkiaPipeline` 之外的字体缓存层，本次未读。
可用的间接锚只有 DWrite 侧的 glyph run 定义（WPF 报告 W2F-077/078），**不跨平台套用**。

### 2.5 Compose 侧（重组与层）

- 三阶段 composition → layout(measure+place) → drawing（W2F-080）；跳过某阶段的依据是**该阶段内的状态读集**（W2F-081）。
- 官方建议：列表项给稳定 key 避免不必要的重组（W2F-085）；用 `derivedStateOf` 限制重组范围（W2F-086）。
- 帧时钟 = `Choreographer.postFrameCallback`（W2F-068）；层绘制录成 RenderNode 显示列表
  （`DeviceRenderNode.record(canvasHolder, clipPath, drawBlock)`，W2F-067），
  层属性变更走 `invalidate` 而非重录，且 `density/layoutDirection/size` 是 record 的输入（W2F-069）。
- 探索得到的 `androidx/compose/ui` 源码分布（`gh api` 清单，@7b4bec74）：
  `compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/{node,graphics,layout,text,platform,viewinterop,window,...}`；
  `platform/` 下有 `DeviceRenderNode.android.kt`、`GraphicsLayerOwnerLayer.android.kt`、`AndroidUiFrameClock.android.kt` 等。

## 3. 性能手段与公开读数

| 数字 | 口径 | 锚 |
| --- | --- | --- |
| 16 ms/帧（60fps 基准） | developer.android.com 的 vitals/render 页，明确写"under 16ms to achieve 60 fps" | W2F-082 |
| Compose 1.9 起"match the Views performance" | 官方 Pokedex 基准（开源 app，单样本；页面自述口径） | W2F-084 |

**除上述两条官方口径外，本报告不含任何自测数字。** AOSP 源码里 `Properties.cpp` 等虽含各类 `debug.hwui.*`
开关（本次抓到文件），但本报告未从中提取性能读数——**没有实测就不写数字**。

## 4. 坑与反例（负面留档）

1. **一帧一次阻塞等待**（W2F-057/058）：UI 线程 post 后 `wait`，渲染线程慢即 UI 线程卡。
   → LSSMJ 单线程出帧时不存在此点；一旦引入后台栅格化，必须把"等待点 + 超时/降级"显式建模，否则症状与
   "卡死"无法区分。
2. **帧的代价是整帧作废**（W2F-083）：超预算不是"晚 1ms 显示"，而是丢帧。
   → 支持把"稳态单帧 P95"作为硬判据（`lssmj-design` C1），而不是看平均帧时。
3. **刷新率不是常数**（W2F-054/066）：帧间隔从运行时刷新率算出，且渲染线程订阅其变化。
   → 任何"16.6ms"写死的预算表都是错的；LSSMJ 预算须写明刷新率档位。
4. **双份属性不是免费的**（W2F-059/060）：staging 提交语义带来额外一份属性内存与一次拷贝，
   换来跨线程安全。→ 单线程场景引入它是纯开销。
5. **本地化文档不能冒充英文原文**（W2F-087/088）：`source.android.com` 在本机不可达，抓到的副本是官网 zh-CN
   本地化版，故这两条只作有限旁证；英文口径一律改由 AOSP 源码锚支撑。

## 5. 未验证项（缺什么证据）

1. **SurfaceFlinger 无 source 锚**：`platform_frameworks_native` 在 GitHub **无镜像**（`gh api` 返回 404，
   已实测），`android.googlesource.com` 本机不可达，故本报告对 SurfaceFlinger/HWC/BufferQueue 只有
   W2F-087/088 两条 zh-CN 文档旁证，**合成器细节未验证**。
2. **source.android.com 英文原站不可达**：`an_*` 系列抓取全部 `WinError 10060`（URL 连接超时），
   已留档于 `scratch/fetch/w2f/v/manifest.jsonl`；因此图形架构专页（HWUI/RenderThread/vsync/frame-pacing）
   的英文原文未取得。
3. **Android 文本栈未读**：hwui 的字形/字体缓存层（`libs/hwui/` 字形相关文件、`pipeline/skia` 中字体部分）
   本次未逐行读，故本报告不对 Android 的整形/字形缓存下任何结论。
4. **Compose 侧只取了 3 个文件**：`compose/ui/ui` 的其余部分（`layout/`、`text/`、`node/` 等）只有目录清单，
   未取源码；"重组/RenderNode/帧调度"的结论以官方文档页（W2F-080/081/085/086）与这 3 个文件为准。
5. **`ViewRootImpl.java`（13.5k 行）只做定点抓取未细读**，本报告未引用其任何行锚。
6. **无端到端实测**：本报告无任何“在真机上跑出来的”帧时数字，全部机制结论来自源码与官方文档。
