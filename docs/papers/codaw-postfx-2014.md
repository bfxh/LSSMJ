# COD: Advanced Warfare · Next Generation Post Processing（SIGGRAPH 2014 课程）

> 来源：讲稿 PPTX（427MB 原件，本波用 HTTP Range 仅取 slide/notes XML 解出文本：`codaw_slides.txt`
> 166 页 + `codaw_notes.txt` 129 页备注）。讲者 Jorge Jimenez（Activision/Sledgehammer）。
> 账本：`../analysis/ledger/w7a.jsonl` W7A-104..116（全 paper，13 条）；日期 2026-10-02。

## 要点（每条带账本行）

1. **管线清单**：运动模糊、Bokeh 景深、次表面散射、Bloom/Veil（+阴影采样 bonus）（W7A-104）。
2. **根本边界**：屏幕空间方法"不可能完全准确"——背景被遮挡后需要重建（信息缺失）（W7A-105）。
3. **运动模糊**：基于 McGuire2012 重建滤波（+Sousa2013 向量化）（W7A-114）；三段式采样区
   （Tile Max 20x20 → 3x3 tile 邻域 → 全分辨率可变宽模糊）；改善点=更准的样本贡献+背景恢复。
4. **DOF**：两层（前景/背景各一个 COC）分裂-平均-alpha 混合（W7A-113）；主滤波 49-tap 跑半分辨率
   （W7A-106）；性能清单亦列"Half-res rendering"（W7A-107）；圆形 Bokeh（人眼孔径）；背景重建圆外扩张。
5. **Bloom/Veil**：不设阈值直接吃全色（引 Ward1997，指向"宽动态范围感知"）（W7A-116）；降采样
   自研 36-texel/13 fetch；升采样逐级 3x3 tent（W7A-109），"绝不直接双线性跨级放大"（W7A-108）；
   6 级 mip、R11G11B10（W7A-111）。
6. **Firefly 抑制**：Karis average（部分版=按 4 样本分块）用于 mip0→mip1（W7A-112）；firefly=极亮
   子像素（HDR 输入的固有难题）（W7A-110）。
7. **性能（Xbox One，讲稿自述）**：Veil 0.35ms / 运动模糊 0.52ms / DOF 1.27ms / SSS 0.46ms（W7A-115）。
8. **方法学**：样本权重三分条件（前景速度≥偏移/背景在像素速度范围内/速度相近）——scatter-as-you-gather
   的判据清单；随机噪声 vs dither 的取舍：重欠采样用随机噪声掩盖更好。

## 对 LSSMJ 的落点

- **"最小后处理链"的证据来源**：bloom（多尺度 mip + tent + Karis average）、DOF（两层+半分辨率）、
  运动模糊（速度缓冲 + tile max）三条都有 AAA 出货实现细节，可作为 LSSMJ 场景档的实现蓝本。
- **成本锚**：XB1 上 DOF 1.27ms / MB 0.52ms / bloom-veil 0.35ms——LSSMJ 的成本表按此数量级起算
  （口径=2014 主机开发期，非 PC；本机待测）。
- **放弃项**：卷积 bloom（FFT 能量守恒）在 UE 文档被判"高端/影视"（W7A-124）；COD 的 36-texel 自研核
  也属"可省"——LSSMJ 标准档取 UE 式 5 级高斯即可。

## 未验证 / 边界

- 讲稿原始 PPTX 427MB（含视频），本波经 Range 抽取 XML 文本；**图表内容未读**（只有文字层）。
- 性能数字为讲稿自述、XB1 平台、2014 开发期；无机器/分辨率细分（未标注分辨率，默认 1080p）。
- 备注页（speaker notes）仅 129/258 有文本；其余为空白或仅图。
