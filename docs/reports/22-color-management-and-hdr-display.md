# 22 · 色彩管理与 HDR 显示链（ICC / 色域 / 传递函数 / 平台色彩管理 / wgpu 输出色彩空间）

> **这份报告是什么**：LSSMJ 显示链的**色彩管理证据锚定**。此前 w6c 给了光照/材质里的
> ACES 与 HDR 渲染口径、w9a 给了显示/合成链的阴影与后处理次序，但**「像素是什么颜色」这件事本身
> 没有专门分析**：ICC/PCS、色域映射、PQ/HLG、平台色彩管理（WCS/ColorSync）、wgpu 的 surface 色彩空间
> 都是空白（英文关键词扫描 ICC=0、wide gamut=0、Display P3=0、color management=1）。
>
> **上游身份**：多源——ICC 规范 PDF（ICC.1:2010-12）与 ICC FAQ / W3C CSS Color 4 / Microsoft
> Advanced Color（HDR）与 DXGI_COLOR_SPACE_TYPE / Apple ColorSync 与 Metal HDR 文档 JSON /
> Little CMS / Skia 官方 color.md 与 `af9d1443` 源码 / ITU-R BT.709-6 与 BT.2100 / Oklab /
> wgpu-types `SurfaceColorSpace` / Crossref 论文元数据。
> **抓取物**：`D:/KF/LSSMJ/scratch/w10b/raw/`；复算脚本 `scratch/w10b/fetch.py`、`gen.py`。
> **账本**：`docs/analysis/ledger/w10b.jsonl` **64 条 / verify rejected=0**（source 12 / doc 38 / paper 14；16 target）。
> **口径**：判定三档——**可用 / 有界 / 不可用**；Rust 可得性——**纯 Rust / 需 FFI（lcms2 等）/ 平台 API / 不进本目标**。

## TL;DR（10 条，每条带锚）

1. **色彩管理的中枢是「连接空间 + 定向矩阵」，不是两两直连**：ICC 用 PCS（D50）作中间空间
   （`W10B-013/015/018`），Skia 用 XYZ D50 作 connection space（`W10B-011/049`），源与目标都只与它相连——
   跨空间转换被归约成「源→XYZ」与「XYZ→目标」两段。Skia 的六步链写得很清楚：
   解预乘→源传递函数线性化→到 XYZ D50→目标色域矩阵→目标传递函数编码→预乘（`W10B-010/012`）。
2. **色彩空间 = 四元组（原色 / 白点 / 传递函数 / 范围）**，且必须可序列化、可比较、可进缓存键：
   wgpu 的 `SurfaceColorSpace` 有八档（含 `ExtendedSrgbLinear`、`DisplayP3`、`Bt2100Pq`、`Bt2100Hlg`，
   `W10B-033/036`）；DXGI 把「RGB 全范围 + G2084(PQ) + P2020」写成显式组合（`W10B-030`）；
   CSS Color 4 把 sRGB/Display P3/Rec.2020 定为预定义空间（`W10B-020/021`）。**禁止用
   `hdr: bool` 或字符串名当色彩空间的唯一标识**。
3. **HDR 不是「把 SDR 调亮」，而是三段空间的分工**：线格式=BT.2100 ST.2084/PQ（`W10B-025/045`）；
   合成空间=scRGB 线性 FP16（Windows CCCS，可表示 [0,1] 之外的值，`W10B-026/032`）；
   平台色彩管理分两段——DWM 把各应用转到 CCCS 再混合，显示内核再把 framebuffer 转到线格式（`W10B-027`）。
   数值语义也要钉死：Windows HDR 下 `1.0f`=80 nits 名义参考白（`W10B-029`）；
   PQ 始终按 10000 nits 峰值亮度解释再除以目标参考白（Skia，`W10B-007`）；HLG 的 OOTF 按 Rec.2020 定义（`W10B-008`）。
4. **色域外处理必须有明确策略**：CSS Color 4 专章列 Clipping / Closest Color (MINDE) / Chroma Reduction
   等算法族及各自的偏差（`W10B-022/023`）；Windows 的选择是「数值截断」（`W10B-028`）；
   色域映射的算法谱系与工作空间选择有专著级材料（`W10B-055/056/057`）。LSSMJ 默认对齐系统（截断），
   更好的映射作显式档并逐算法金样。
5. **平台色彩管理的分工模型是「系统管显示、引擎管标签」**：Windows 有 WCS/Advanced Color（`W10B-024/027/048`）；
   Apple 有 ColorSync 引擎 + EDR 显示（`W10B-037/038/039`）；完整 ICC CMM 有 lcms2 现成实现（`W10B-040/041`）。
   LSSMJ 的壳层应把内容色彩空间标签交给系统，自绘链只在需要自管理帧缓冲时做手动转换。
6. **位图契约要区分「像素格式」与「色彩空间解释」**：wgpu 明说 `SurfaceColorSpace` 不改变 texel 格式，
   只改变呈现引擎如何解释数值（`W10B-034`）。LSSMJ 的 Frame/位图描述因此要带两个独立字段
   （format + color space），且显示链路两端都带标签（对齐跨平台像素一致）。
7. **实现纪律：能证明不用做就不做**。Skia 的 sRGB/sRGB-linear 是进程单例（`W10B-003`）；
   源目标 hash 相同且 alpha 类型相同 → 变换步骤直接返回零操作（`W10B-006`）；空源按 sRGB、空目标按源
   （`W10B-005`）；六步链里有可判定的跳步条件（`W10B-009/050`）；坏矩阵惰性求逆失败回退 sRGB（`W10B-004`）。
   这是 C17「优化等价性」在色彩链上的直接形态：**跳步必须可证明、可单测**。
8. **感知域是质量档，不是默认档**：Oklab 给出线性 sRGB⇄Oklab 的官方实现（`W10B-042/043/044`）；
   CIECAM02 是色彩管理的标准色貌模型（`W10B-051/052/053`）；色差度量有 CIEDE2000 的标准测试集
   （`W10B-054`）。默认插值/混合仍在编码或线性光域，感知域用于渐变色插值与色域映射的质量档。
9. **D50 与 D65 是两条白点体系，不能混**：ICC 的 PCS 白点是 D50（`W10B-015`），
   显示/合成链（sRGB、Display P3、Windows CCCS）是 D65（`W10B-036/032`），Oklab 也是 D65（`W10B-043`）。
   色适应按 ICC 规定用线性 Bradford（`W10B-014`）。LSSMJ 的每一步变换必须标注白点，转换点显式。
10. **动态 HDR 元数据（Dolby Vision / HDR10+）列观察项**：Dolby Vision 的卖点是按服务/设备动态优化
    （`W10B-047`），需要授权与专门管线；LSSMJ 的 HDR 最小集=静态 HDR10/PQ + HLG，动态元数据不阻塞主链。

## 1. 建议的显示链模型（由证据推出的三段）

```text
内容空间（应用/资产）   sRGB / linear sRGB / Display P3 / Rec.2020 / PQ / HLG     W10B-020/021/033
        │  ① 到工作空间（线性光域，D65；ICC 输入走 D50 PCS + Bradford）            W10B-010/011/014/015
        ▼
合成空间（工作空间）     线性、BT.709 原色、FP16（= Windows CCCS / scRGB 等价）      W10B-026/032
        │  ② 色域映射（超显示色域：截断或 chroma reduction）+ 可选感知域处理         W10B-022/023/028
        ▼
显示/线格式              sRGB（SDR）/ Display P3 / BT.2100 PQ / HLG              W10B-025/030/033
        │  ③ 平台色彩管理（Windows DWM/显示内核、macOS ColorSync/EDR）              W10B-027/037/038
        ▼
显示器
```

三段各自的门：① 变换矩阵/传递函数正确（逐点对拍）；② 色域外策略确定（逐算法金样）；
③ 输出色彩空间标签正确（能力探测 + 降级金丝雀）。**任一段缺标签=红**（这是本报告主张的新判据 C18 的雏形）。

## 2. 技术判定表（逐项：技术 / 性质 / Rust 可得性 / 判定 / 锚）

| 技术 | 性质 | Rust 可得性 | 判定 | 锚 |
| --- | --- | --- | --- | --- |
| sRGB / linear sRGB | 编码空间（含线性段） | 纯 Rust | **可用（默认档）** | W10B-003/031 |
| Display P3 / Rec.2020 | 宽色域原色集 | 纯 Rust | 可用（宽色域档） | W10B-020/021/033 |
| sRGB 色彩空间单例 + hash 比较 | 零分配/零转换快路径 | 纯 Rust | **可用（性能基线）** | W10B-003/006 |
| XYZ D50 连接空间 + 3x3 矩阵 | 跨空间转换归一 | 纯 Rust | **可用（默认档）** | W10B-010/011/049 |
| ICC profile 解析（显示器/输入） | 设备描述 | 纯 Rust（子集）或 lcms2 FFI | 有界（最小集自研，全量交 lcms2） | W10B-013/040/041/060 |
| ICC 渲染意图（相对色度/感知） | 映射策略 | 纯 Rust（须自定确定性实现） | 有界（perceptual 厂商自定义，须金样） | W10B-016/017/019 |
| Bradford 色适应 | 白点适配 | 纯 Rust | **可用（规范默认）** | W10B-014 |
| PQ（ST 2084）转换 | HDR 线格式 | 纯 Rust | **可用（HDR 档）** | W10B-007/025/030 |
| HLG + OOTF | HDR 线格式（广播向） | 纯 Rust | 可用（HDR 档） | W10B-008/030/033 |
| scRGB/线性 FP16 合成（CCCS 等价） | HDR 合成空间 | 纯 Rust | **可用（HDR 合成默认）** | W10B-026/032 |
| 1.0f=80nits 参考白语义 | HDR 数值语义 | 纯 Rust | **可用（显式参数）** | W10B-029 |
| 色域映射（clip / chroma reduction / MINDE） | 超色域处理 | 纯 Rust | 可用（默认 clip，质量档 reduction） | W10B-022/023/028/055 |
| 感知域插值（Oklab） | 渐变/映射质量 | 纯 Rust | 有界（质量档） | W10B-042/043/044 |
| CIECAM02 色貌模型 | 观感一致 | 纯 Rust（计算重） | 有界（研究/质量档） | W10B-051/052/053 |
| CIEDE2000 色差 | 质量度量 | 纯 Rust（必须带标准测试集） | 有界（度量/门） | W10B-054 |
| 平台色彩管理（WCS/Advanced Color） | Windows 显示侧 | 平台 API | **可用（交系统）** | W10B-024/027/048 |
| 平台色彩管理（ColorSync/EDR） | macOS 显示侧 | 平台 API | **可用（交系统）** | W10B-037/038/039 |
| wgpu SurfaceColorSpace（8 档） | 输出色彩空间 | 纯 Rust（wgpu 已有） | **可用（能力探测+降级）** | W10B-033/034/035/036 |
| DXGI 色彩空间组合 | Windows 输出 | 平台 API（wgpu 封装内） | 可用（对齐枚举语义） | W10B-030/031/032 |
| 动态 HDR 元数据（Dolby Vision/HDR10+） | 授权+专门管线 | 不进 | **不可用（本目标）** | W10B-047 |
| 自研全量 CMM | 全 profile 类 | 成本高 | 不可用（交 lcms2/系统） | W10B-040/041/060 |

## 3. 重点来源短分析（5 份）

### 3.1 ICC 规范与 FAQ——「连接空间 + 渲染意图」的规范源

- 规范定位=跨平台 profile 格式（`W10B-013`）；PCS adopted white 钉死 D50（`W10B-015`）；色适用线性 Bradford（`W10B-014`）。
- 渲染意图：media-relative（白点映射到 PCS 白）、ICC-absolute（in-gamut XYZ 不变）、perceptual/saturation（厂商自定义，`W10B-016/017`）。
- FAQ 把两次转换都归结为经 PCS 的中间空间（`W10B-018`），并定义渲染意图=色域不同时如何修改（`W10B-019`）。
- 结论：**LSSMJ 的 ICC 侧用 D50 PCS；perceptual 档必须自有确定性实现**（不依赖系统 CMM）。

### 3.2 Skia 源码 + 官方 color.md——可执行的六步链与优化规则

- 六步链（`W10B-010/012`）、XYZ D50 连接空间（`W10B-011/049`）、可判定跳步（`W10B-009/050`）、
  sRGB 单例与 hash 短路（`W10B-003/006`）、空值默认（`W10B-005`）、坏矩阵回退（`W10B-004`）、
  PQ 10000nits 与 HLG OOTF（`W10B-007/008`）。
- 结论：**LSSMJ 的色彩变换内核直接照 Skia 的「步骤表 + 跳步条件 + 单例」结构**实现；
  这是 Rust 生态外最接近的工程参照（Skia 的 C++ 实现可逐段对照）。

### 3.3 Windows Advanced Color + DXGI——HDR 的平台分工与枚举语义

- Advanced Color=HDR/宽色域/高位深三维能力（`W10B-024`）；线格式 BT.2100 ST2084（`W10B-025`）；
  CCCS=scRGB 线性 FP16 且可超 [0,1]（`W10B-026`）；两段色彩管理（`W10B-027`）；
  超色域数值截断（`W10B-028`）；HDR 1.0f=80nits（`W10B-029`）；DXGI 组合枚举与 sRGB/scRGB 定义（`W10B-030/031/032`）。
- 结论：**LSSMJ 的 HDR 路线 = 线性 FP16 合成空间 + PQ/HLG 线格式 + 平台色彩管理**；三段语义都在这里有官方原文。

### 3.4 wgpu `SurfaceColorSpace`——Rust 侧已经有的输出色彩空间模型

- 八档枚举（`W10B-033`）；只改解释不改格式（`W10B-034`）；非 Srgb=进入 HDR/WCG 的开关且支持集可查询（`W10B-035`）；
  四元组定义（`W10B-036`）。
- 结论：**LSSMJ 不需要自造显示色彩空间模型**：wgpu 档已有 HDR（PQ/HLG）与宽色域（P3）选项，
  引擎做能力探测、选择、降级与记账即可（与 C10 三级降级同构）。

### 3.5 感知域与色域映射（Oklab / CIECAM02 / CIEDE2000 / 映射专著）

- Oklab 的官方实现与 D65 白点（`W10B-042/043/044`）；CIECAM02 标准与述评（`W10B-051/052/053`）；
  CIEDE2000 的测试数据集（`W10B-054`）；色域映射算法综述与工作空间选择（`W10B-055/056/057`）；
  色调映射的内容类型差异（`W10B-063/064`）。
- 结论：**感知域是「质量档 + 度量工具」**：Oklab 用于渐变/映射，CIEDE2000 用于质量门，CIECAM02 留研究档。

## 4. 与既有轮的接缝

- **与 w6c（光照/材质/色彩）**：w6c 定 ACES tonemap 与 HDR 渲染口径；本轮定**输出/显示侧**的色彩空间与转换。
  接缝=ACES 输出变换（RRT/ODT）之后进入本报告的三段链；ACES 用的是自身色彩空间，需显式转到显示空间。
- **与 w9a（实时光影/显示链）**：HDR 输出与后处理顺序（tonemap→显示编码）在本报告被钉死；w9a 的显示链结论不变，
  新增「输出色彩空间标签」这一层。
- **与 w9b（组件/令牌）**：主题令牌的颜色值必须带色彩空间（token 里的 hex 默认 sRGB；宽色域值要显式标注），
  否则组件层无法参与跨平台色彩一致。
- **与 w8c（电影感/调色）**：胶片感链（grain/halation/grade）作用在「工作空间还是显示空间」直接决定观感与可复现性；
  本报告的答案是**统一在线性工作空间做、显示编码前完成**（LUT 不能承载空间效果，w8c 已给证据）。
- **与判据**：建议新增 **C18 色彩一致**——内容色彩空间标签齐全；同标签跨平台走同一条变换链；
  宽色域/HDR 默认关且开启时必须记录参考白与线格式；色域外策略显式。

## 5. 未验证项（缺什么证据）

1. **Android 宽色域/HDR 官方文档不可达**：`developer.android.com` 与 `source.android.com` 在本机网络均超时
   （两次尝试），本轮 Android 侧色彩管理**未取到官方锚**——补缺时需换网络/存档重取。
2. **Skia 官网文档不可达**：`skia.org` 与 `docs.skia.org` 超时；改用 **Skia 仓库内官方文档**
   `site/docs/user/color.md`（`W10B-010/011/012/049/050`）与 `af9d1443` 源码，内容等价但非站上版本。
3. **BT.2100 正文 PDF 未取到**：ITU 直链 404，只取到 BT.2100 的官方条目页（`W10B-045`，含版本/状态）；
   PQ/HLG 的公式细节来自 Skia 源码与 DXGI/wgpu 枚举，**未逐字核 BT.2100 正文**（PQ 标准是 SMPTE ST 2084，付费）。
4. **ICC 规范 PDF 为文本抽取结果**：抽取有断词与空格噪声（如 "measurem ent"），本轮只引用其中语义完整的行；
   规范正文的图表（色适应矩阵、profile 结构）未逐项核。
5. **wgpu `SurfaceColorSpace` 的版本未钉**：证据来自 docs.rs 的 latest 页面（2026-10-03 抓取）；
   `wgpu-types` trunk `lib.rs` 中未搜到该枚举，说明它可能在别的模块或较新分支——**落地时必须锁 wgpu 版本并写特性探测**。
6. **未做任何实际屏幕测量/对拍**：没有 ICC profile 实测、没有 HDR 显示器读数、没有跨平台同图对拍；
   C18 的门（色度计/截图对拍/逐位路径）需要真实硬件与素材。
7. **Apple 文档只到 JSON 摘要层**：ColorSync 与 Metal HDR 的 JSON 通道取到概述文字，
   `developer.apple.com` 的完整页面为 JS 渲染未逐节核；EDR headroom 的 API 细节（`W10B-038/039`）未展开。
8. **动态 HDR 元数据只取到厂商 marketing 页**（`W10B-047`）：Dolby Vision 的元数据规范（ST 2094 系列）为付费文档，
   未核；本轮结论「列观察项」不受影响。

> **补锚更新（w14a）**：第 1 项已补——Android 宽色域/HDR 文档经 `developer.android.google.cn` 镜像取得（sRGB 默认 + Display P3 档 + `ColorSpace.Named` + PQ/HLG 线格式，`W14A-012..014/019`）；Vulkan 色彩空间枚举用 WSI 规范正文补上（`W14A-010/011`）。**BT.2100 正文仍未取到**（ITU 直链 404/登录墙），PQ/HLG 技术细节仍由 Skia 常数与 DXGI/wgpu 枚举支撑。
