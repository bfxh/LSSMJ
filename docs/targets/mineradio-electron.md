# Mineradio（Electron 42 桌面音乐应用）案例解剖

> 快照：`Mineradio-paused-2.2.0`（用户提供 tar.gz，本地 `scratch/src/Mineradio-paused-2.2.0/`，只读）。
> 账本：`../analysis/ledger/w4b.jsonl`。用途：给"替代 JS 包装方案"一个**有真实工程史的样本**——
> 它把 Electron 的性能压榨做到了"写预算文档"的程度，正好用来分清**哪些成本是架构性的、哪些是实现问题**。

## TL;DR（每条带锚）

1. Electron **42.4.1**（`package.json:112`），打包**关闭 asar**（`package.json:48` `"asar": false`）——
   资源以普通文件直落盘（对启动/更新/杀软行为都有影响，见"成本账"）。
2. 主进程是**单一 6058 行 `desktop/main.js`**：6 个 BrowserWindow、**74 个 `ipcMain.handle`**
   处理器（复算：`grep -c "ipcMain.handle(" desktop/main.js`；示例锚 `main.js:4060`）。
3. 多窗口分工：主窗 + **桌面歌词**（点击穿透 `setIgnoreMouseEvents(..., { forward: true })`，`main.js:3599`）
   + 壁纸/全桌面两套 runtime（`full-desktop-mode-runtime.js:457`、`wallpaper-engine-runtime.js:1597`）。
4. 壁纸引擎集成走 **WorkerW 窗口注入**（`FindWindowEx(..., "WorkerW", ...)`，`wallpaper-engine-runtime.js:1324`）
   + `desktopCapturer` 抓场景（`wallpaper-engine-runtime.js:1591`、`:2232`）。
5. preload 用 `contextBridge` 暴露**单一 `desktopWindow` 命名空间**（`preload.js:1`、`:3`），
   桥面覆盖窗口/内存/缓存/壁纸/登录/更新等全部受控能力。
6. **性能纪律被写成文档**：`docs/LOW_SPEC_OPTIMIZATION_DOCTRINE.md`（194 行）逐条规定"帧预算"
   ——"一帧一层纹理上传仍是硬预算"（`:5`）、预热"仍受每帧 1 层预算约束"（`:6`）、
   重任务"不得在开关、切歌或动画 tick 中一次性创建"（`:18`）、回收"有时间预算的小批次队列"（`:20`）。
7. 内存有独立机械：`appMemoryTrimTimer/InFlight` 防重入的裁剪节拍（`main.js:49`、`:1969`），
   配合系统级释放开关（LOW_SPEC 规定提权/UAC 路径默认锁定）。
8. 输入与系统集成：`globalShortcut.register`（Esc/自定义热键，`main.js:1357`、`:1793`）、
   `powerMonitor` 的 resume/unlock 恢复可见性（`main.js:5959`）、GPU 诊断 IPC（`main.js:4060`）。
9. 渲染侧是 **Web 栈**：GSAP 动画、`mpg123-decoder`（WASM）解码、Canvas/WebGL 视觉（歌词纹理/粒子/3D 歌单架；
   详见 LOW_SPEC 的"每帧纹理上传预算"条目）——**一切视觉都被迫写成"预算 + 降级"结构**。
10. 服务侧 `server.js`（6717 行）= 网易云 API 代理（搜索/URL/封面/音频、扫码登录、cookie 持久化、音质探测），
    说明这类桌面包的一半复杂度在**服务代理与账号体系**，不在渲染。

## 1. 架构（进程 / 窗口 / 桥）

- 主进程依赖面（`main.js:1`）：electron 全套（含 `protocol`/`desktopCapturer`/`powerMonitor`）+
  自研 runtime 模块（wallpaper-engine / local-music / built-in-playlist / full-desktop / login-gate）。
- 两个自定义协议 scheme 注册（`wallpaper-engine-library.js` 与 `local-music-library.js` 的
  `registerXxxScheme(protocol)` 在 `main.js:27`–`:28` 调用）——**本地媒体资源走自定义协议**，
  与 Chromium 的安全模型（file:// 限制、CSP）周旋的标准做法。
- IPC 面 74 个 handler：窗口控制、内存快照/裁剪/系统释放、缓存目录、壁纸引擎（列项目/详情/启动/截图回传）、
  GPU 诊断、登录（含彩蛋门 `login-easter-egg-gate.js`）。全在 preload 的 `desktopWindow` 命名空间下。

## 2. 桌面集成（这套应用最"重"的部分）

| 机制 | 锚 | 说明 |
| --- | --- | --- |
| 桌面歌词 | `main.js:3599` | 点击穿透（`forward: true`）；鼠标轮询/热区状态散在主进程（`desktopLyricsMousePoller` 等，`:34` 起）|
| 壁纸引擎 | `wallpaper-engine-runtime.js:1324`/`:1591`/`:2232` | WorkerW 注入 + desktopCapturer 抓源；`IntPtr/FindWindowEx` 说明走原生窗口层 |
| 全桌面模式 | `full-desktop-mode-runtime.js:457` | 独立 runtime（弹层/退出策略与 全局 Esc 协同，`main.js:1357`）|
| 电源事件 | `main.js:5959`–`:5960` | 睡眠恢复/解锁后修复窗口可见性 |

## 3. 性能工程（LOW_SPEC 文档才是这个仓最值钱的产物）

逐条摘引（行锚在账本）：

- **纹理上传预算**：一帧一层是硬预算；顺序必须先把当前窗口正文传完再放行特效层（`:5`）；
  预热受同样预算（`:6`）——"不能靠同步上传、扩大单帧预算…换连续性"。
- **重任务协作调度**：歌词 mesh/材质不得整窗一次性创建（`:18`）；回收走有时间预算的小批次（`:20`）；
  所有半成品"可取消、可去重、可分批释放"（`:17` 附近）。
- **降级而非砍功能**：低配封顶分辨率、等比缩放画布，但"不能靠砍译文、描边、辉光…换性能"（`:7` 附近）。

配套机制：`appMemoryTrim*`（`main.js:49`/`:1969`）、GPU 诊断（`:4060`）、启动开关白名单
（`app.commandLine.appendSwitch`，`main.js:494`–`:495`）。

## 4. 成本账（架构性 vs 实现性）

| 成本 | 性质 | 证据 |
| --- | --- | --- |
| Chromium + Node 常驻多进程（主/渲染/GPU/Utility） | **架构性** | 6 窗口 + Electron 42；LOW_SPEC 自述"Electron 多进程是 Chromium 架构事实" |
| 一切视觉都要"预算+降级"（否则掉帧/爆内存） | **架构性**（Web 渲染路径的固有税） | LOW_SPEC 全篇即此税的账单 |
| 主进程单线程 6058 行 + 74 IPC 面 | **实现性** | 可拆模块/worker，但 Electron 模型不改 |
| asar:false + 未签名分发 + 网盘渠道 | **实现/分发选择** | `package.json:48`、README 的下载指引 |
| 服务代理/账号/cookie 复杂度 | **业务性**（与 UI 技术栈无关） | `server.js` 6717 行 |

## 5. 对 LSSMJ 的含义（"替代 JS 包装"要替代什么）

- **可被原生等价物直接消除的**：每帧纹理上传预算、Canvas/WebGL 视觉的降级矩阵、点击穿透歌词窗的
  "轮询+热区"复杂度（原生分层窗/位图化后，穿透是窗口属性，热区是命中表）。
- **被验证**：把"每帧只做固定预算工作"当一等纪律——与青简"排一次/Show 每帧"、GN"SetText/Show"
  形成三方互证（`w5a`、`w4a`、`w4b` 各有一条）。
- **不会被替代掉、要照样做的**：服务代理与账号体系、更新分发渠道、安装器体验——这些是产品的一半。
- **迁移路径的现实性**：该应用的"显示面"（歌词舞台/桌面模式/overlay）正是可位图化/GPU 化的那一层；
  "控件面"（设置/登录/歌单管理）可以暂缓——与青简设计档对 UI 两分法的结论一致。

## 6. 未验证

- 未运行应用（无内存/启动实测）；文中"74/6/6058/6717/194"等计数为**对快照文件的复算**（命令随行）。
- `public/**` 前端资产不在本快照（未随包），WebGL/WebGPU 用法的细节未读（README 与 docs 佐证其特征）。
- 未审计安全性（不属本任务）。
