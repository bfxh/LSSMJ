# 26 · 补锚批（AccessKit 平台适配器 / Android / MS Raw Input / D3D12 / wgpu / Metal / Vulkan 色彩空间）

> **这份报告是什么**：把前几轮报告"未验证"节里因网络/反爬/路径失效而缺的证据**一次性补齐**。
> 不引入新结论，只补锚与更正边界。
> **账本**：`docs/analysis/ledger/w14a.jsonl` **29 条 / verify rejected=0**（source 11 / doc 18；6 target）。
> **抓取物**：`D:/KF/LSSMJ/scratch/w14a/raw/`（Android 文档经 `developer.android.google.cn` 镜像取得）；
> 复算脚本 `scratch/w14a/fetch.py`、`gen.py`。AccessKit 与 Vulkan 色彩空间用本地既有快照（source 锚）。

## 1. 补到了什么（逐项对账）

| 原缺口（出处） | 状态 | 新锚 |
| --- | --- | --- |
| w9b：AccessKit 平台适配器「只读 ARCHITECTURE，未逐读 adapters」 | ✅ 已补：Windows/macOS/Unix-AT-SPI/atspi-common 四侧源码 | W14A-001..009 |
| w10b：Android 宽色域/HDR 官方文档不可达 | ✅ 已补（google.cn 镜像） | W14A-012/013/014/019 |
| w10b：Vulkan 色彩空间 man page 403 | ✅ 已补（Vulkan WSI 规范正文，`VkColorSpaceKHR` 枚举） | W14A-010/011 |
| w11a：MS Raw Input 文档未取 | ✅ 已补（overview + 函数页） | W14A-020..023 |
| w11a：Android 输入文档未取 | ✅ 已补（input-events + MotionEvent 参考） | W14A-015..018 |
| w12a：wgpu 是否暴露内存提示未核 | ✅ 已补（`MemoryHints` 三档 + `DeviceDescriptor.memory_hints`） | W14A-026/027 |
| w12a：Metal 只到 JSON 摘要层 | ✅ 部分补（`MTLStorageMode` 四档 shared/private/managed/memoryless） | W14A-028/029 |
| w12a：D3D12 「copying and accessing resource data」404 | ⚠️ 部分补：改用「资源屏障与状态」页（初始状态硬约束 + 状态提升），copy 页仍未取 | W14A-024/025 |
| w13a：Wayland 协议原文被 Anubis 反爬 | ⚠️ 部分补：改用 wayland.app 协议镜像（w13a 已用） | W13A-036/037/038 |
| w13a/w12a：Android 显示/图形内存文档 | ⚠️ 部分补：Android 色彩/HDR/输入已取；**显示 DPI 与图形内存仍未取** | W14A-012..019 |
| w10b：BT.2100 正文 PDF | ❌ 未补：ITU 直链 404/登录墙；技术内容由 Skia 的 PQ/HLG 常数与 DXGI/wgpu 枚举支撑 | （w10b W10B-007/008） |

## 2. 补锚带来的结论修正（都不改变主结论，只加精度）

1. **AccessKit 三平台适配器确认「一套 schema + 三个薄适配器」**：Windows 走
   `UiaReturnRawElementProvider`/`UiaRaiseAutomationEvent`（`W14A-001/002`），并明确警告
   **不要在 `WM_GETOBJECT` 处理中改 a11y 树**（嵌套消息会死锁，`W14A-003`）；
   macOS 挂在 `NSView` 上用系统通知映射焦点变化（`W14A-004/005/006`）；
   Linux 走 zbus 连 AT-SPI D-Bus（`W14A-009`）；公共层以
   `TreeUpdate` 初始化 + 分族事件（`W14A-007/008`）⇒ w9b 的「a11y 不进热路径 + 增量提交」结论**升级为可执行约束**。
2. **Android 全面对齐 w10b/w11a**：sRGB 默认 + Display P3 宽色域档（`W14A-012/013/014`）、
   HDR=PQ/HLG（`W14A-019`）、输入入口 `onTouchEvent(MotionEvent)`（`W14A-015`）、
   指针 id `getPointerId`（`W14A-016`）、辅助指针 `ACTION_POINTER_DOWN/UP`（`W14A-017`）、
   历史批次 `getHistoricalX/Y` = coalesced 原始采样（`W14A-018`）。
3. **Windows 原始输入通道有了官方锚**：Raw Input 给设备相关原始数据、`WM_INPUT` 是低层、
   `WM_APPCOMMAND` 是高层（`W14A-020/021`），并有默认处理 `DefRawInputProc`（`W14A-023`）
   ⇒ w11a 的「窗口位置 vs 原始位移双通道」在 Windows 上落到具体消息。
4. **D3D12 资源状态是硬约束**：readback 堆必须以 `COPY_DEST` 起始、上传堆创建即读且不可转换
   （`W14A-024`），另有状态提升/衰减优化（`W14A-025`）⇒ w12a 的「用途→屏障」加上初始状态约束。
5. **wgpu 内存提示可用**：`MemoryHints::{Performance, MemoryUsage, Manual}` + `DeviceDescriptor.memory_hints`
   （`W14A-026/027`）⇒ w12a 的显存策略在 wgpu 档有落点（默认 Performance，压力档 MemoryUsage）。
6. **Metal 存储模式四档确认**：shared/private/managed/memoryless（`W14A-028/029`）⇒
   temporary 附件走 memoryless 是省显存的正规路径。

## 3. 仍未补到的（保留在各自报告的未验证节）

- **BT.2100 正文**：ITU 直链 404/登录墙；只有官方条目页（版本/状态）与第三方实现（Skia 常数、DXGI/wgpu 枚举）。
- **Android 图形内存与显示 DPI 文档**：本批取到色彩/HDR/输入，但「图形内存」与「显示 scaling」两页未取。
- **D3D12 copy 专页**：仍 404；用资源屏障页 + 上传页替代，读回/copy 队列细节未逐字核。
- **Wayland 协议原文**：gitlab 反爬；用 wayland.app 镜像（协议文本等价，版本/状态未逐字核）。
- **Apple 完整 API**：MTLHeap/NSScreen 的成员级页面部分 404；现有 JSON 摘要 + MTLStorageMode 足够支撑结论。

## 4. 对设计文档的影响

- 无新增判据；C18（色彩一致）、C19（输入一致）、C20（显存预算）、C21（显示拓扑）的**证据边界缩小**：
  Android 三块（色彩/HDR/输入）与 Windows 原始输入、D3D12 状态约束、wgpu 内存提示、Metal 存储模式全部有锚。
- 受影响报告的"未验证"节已就地更新并指回本批（`W14A-xxx`）。
