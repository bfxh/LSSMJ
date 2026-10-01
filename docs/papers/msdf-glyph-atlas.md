# msdfgen / msdf-atlas-gen（Chlumský）：多通道距离场

> 锚：`W1F-063..066`；URL <https://raw.githubusercontent.com/Chlumsky/msdfgen/master/README.md>
> **降级说明（必读）**：任务要求引其硕士论文 *Shape Decomposition for Multi-channel Distance
> Fields*（CTU 2015）。本机对该 PDF 的三次直取（CTU DSpace bitstream 两个 URL、shachaf.net 镜像）
> 全部得到**截断流**（671500 / 978944 / 1153927 / 457495 字节，均无 `%%EOF`，pdftotext 与 pymupdf
> 均解出 0 页）；GitHub files 镜像（github.com/…/thesis.pdf）本机 TLS/超时不可达；web.archive 超时。
> 因此本批**只锚官方 README 文字**（任务允许的降级路径），论文侧结论未逐字读过。

## README 给出的四条（`W1F-063..066`）

1. **核心主张**：多通道距离场"have the ability to reproduce sharp corners almost perfectly by
   utilizing all three color channels"（`W1F-063`）——把 SDF 的圆角问题解掉。
2. **归属**：msdf 是 Chlumský 自研方法（README 明示 "using my new method"），论文为其硕士论文；
   工具链三级：thesis → `msdfgen`（生成器/库）→ `msdf-atlas-gen`（整图集）（`W1F-064`）。
3. **关键参数**：`-angle` = "maximum angle to be considered a corner"（`W1F-065`）——尖角保真依赖
   角阈值判据（默认 3.0 rad / 171.9D，工程上就是这个数域）。
4. **mtsdf 变体**：rgb=msdf（保尖角）+ alpha=真 SDF（保平滑轮廓与特效），一个文件两用（`W1F-066`）。

## 与外部证据的互证

- Lengyel 2017（`W1F-011`）对 msdf 的评价与之互补：修了圆角，但预处理"complicated analysis step"、
  且复杂字形有"difficult-to-avoid artifacts"——**选用 msdf 时构建期成本与伪影要一起评估**。
- Valve 2007（`W1F-004`）提供问题侧：单通道磨圆角。
- 三方合起来构成"尖角保真"一张表：SDF（损失圆角）/ MSDF（修角，预处理贵）/ mtsdf（两全，体积换）。

## 对 LSSMJ 文本层的取舍

- **有界吸收**：候选窗不用；"大字号/极端缩放"再评估时，首选 `msdf-atlas-gen` 工具链而不是自研生成器
  （构建期复杂性正是 Lengyel 指出的坑）。
- 若未来要 SDF 特效（描边/阴影）+ 保尖角同存，**mtsdf 是现成格式**（`W1F-066`），
  不必自己拼两种资源。
- 角阈值（`W1F-065`）要进构建参数表并写默认值——否则同一字体两处构建的尖角判定会漂移。

## 未验证（继承报告 §5）

论文内的"cut-corner 分析 / 误差修正算法 / 与 SDF 的误差量化对比"**均未核实**；引用时不要带页码。
