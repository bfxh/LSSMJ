# 视频管线研究（第四轮·补：用户点名）

> 范围：硬件解码 API（DXVA/D3D11VA、NVDEC、VideoToolbox、VA-API、MediaCodec）、浏览器/应用侧视频管线、视频进 UI 的契约。
> 账本：`../analysis/ledger/w6g.jsonl`（12 条；可引一手仅 DXVA2/NVDEC/Chromium/GN SDK，其余登记为缺口）。
> 定位：视频**不是渲染引擎内核**，而是"宿主能力 + 零拷贝进纹理"的壳层议题——本报告按此边界写。

## TL;DR（每条带锚）

1. **硬件解码的官方定义口径**：DXVA="API + DDI 用硬件加速视频编解码处理"（W6G-001）；NVDEC=片上硬件解码器、与 CUDA 核分离（W6G-002/003）。
2. **"解码不占算力"要写清是哪种硬件**：NVDEC 与计算核分离（W6G-003）——功耗/吞吐是另一本账。
3. **浏览器把解码器抽象成管线节点**：Chromium 设计文档的职责定义"Convert encoded streams into raw audio/video frames"（W6G-004/005）——Electron/Mineradio 的视频能力即此管线的中转（w4b）。
4. **闭源 SDK 的视频契约（GN SDK）**：`cVIDEO` 类（★★★，W6G-006）；初始化含"循环（负数）/时间同步开关/是否处理声音"（W6G-007）；**每帧必须推**——"这需挂在循环里面不断调用 否侧会中断没有显示"（W6G-008）。
5. **GN 音频的边界句**：`cBGM_MUSIC`"只播放大于频率22050的音频流"（W6G-009）——闭源 SDK 的能力边界常写在注释的"注意"里。
6. **平台解码器全景**（本批状态）：Windows=DXVA2/D3D11VA（D3D11VA 页 404，以 DXVA2 为锚，W6G-012）；NVIDIA=NVDEC；Apple=VideoToolbox（页面脚本化，无静态正文，W6G-011）；Linux=VA-API（wiki 反爬 418）；Android=MediaCodec（未取）。
7. **FFmpeg 侧缺口如实登记**：HWAccelIntro 页抓到但无可挖正文（W6G-010）。
8. **对 LSSMJ UI 档：不吸收**——候选窗不播视频（青简产品线无此需求，`../renderer-qingjian/01`）。
9. **对应用/场景档**：视频=壳层能力，接口按 GN 的三要素设计——**帧驱动更新（推式）+ 时间同步开关 + 音频支路**（W6G-006..008）；零拷贝路径（解码→纹理）是平台相关的**壳实现细节**，不进渲染内核。
10. **对"替代 JS 包装"（Mineradio）的含义**：Electron 把视频/音频/网络都交给 Chromium 与 Node 生态；原生替代时这些是**独立子系统**，本体量不在渲染引擎——迁移清单要按子系统切（w4b §5 同结论）。

## 管线形状（三层，边界写死）

```text
[壳层/宿主] 平台解码（DXVA/NVDEC/VideoToolbox/...）或软件解码（FFmpeg 类）
      │  零拷贝目标：解码输出 → GPU 纹理（平台相关，不进内核）
[引擎层]   VideoTexture 句柄：帧驱动 update(dt) → 输出纹理/位图 + 时间码（学 GN cVIDEO 契约）
      │
[渲染层]   纹理/位图作为普通素材参与合成（UI 或场景，两侧都用同一"素材"概念）
```

## 附：平台解码 API（第五轮补，2026-10-02）

| 平台 | API | 状态 | 锚 |
| --- | --- | --- | --- |
| Windows | DXVA2 / D3D11VA | DXVA2 ✅ 一手；**D3D11VA 页 404**（medfound 多路径与 GitHub 镜像列举均未定位，W6K-005） | W6G-001 |
| NVIDIA | NVDEC | ✅ 一手（片上解码器、与 CUDA 核分离） | W6G-002/003 |
| Apple | VideoToolbox | ✅ 一手（**Apple 文档 JSON 通道**：压缩/解压服务句 + `VTCreateCGImageFromCVPixelBuffer` 符号） | W6K-001/002 |
| Linux | libva / VA-API | ✅ 一手（"open-source library and API specification"） | W6K-003/004 |
| Android | MediaCodec | ❌ 未达（域不可达） | — |

**通道发现**：Apple 文档抓不到时，可走 `developer.apple.com/tutorials/data/documentation/<framework>.json`（本批由此拿到一手定义句）。

## 未验证 / 缺口

- D3D11VA/HDR 视频（MS 404，W6K-005）、MediaCodec（域不可达）、FFmpeg 文档正文（W6G-010）——**平台侧仍有缺口**（编号段 `w7f` 备）。
- 未做任何解码实测；未读 Mineradio 的 `public/**`（快照不含）——它在渲染侧如何播视频仍未核（w4b 已登记同一缺口）。
