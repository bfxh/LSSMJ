# 短分析 · clustered / tiled / Forward+：多光源选型

> 上游：Olsson & Assarsson 2012《Clustered Deferred and Forward Shading》（HPG'12）+
> Harada 2012《Forward+》+ GPU Pro 7《Tiled Forward Shading》+ SIGGRAPH 2012 课程。
> 账本：`../analysis/ledger/w9a.jsonl` **W9A-060..073 / W9A-117/118/119**。只读；抓取 2026-10-03。

## 结论（带锚）

- **结构**：clustered 用 3D 簇替代 2D tile，光-样本映射更优（`W9A-060`）；规模上界 10^4–10^6 光源
  （`W9A-061`），分层光分配在二叉深度 5 时约 3200 万光源（极值口径，`W9A-064`）。
- **实测（2012 硬件，同内容）**：2400 光源场景 clustered 17ms（clustering 2.3ms + 分配 1.5ms + 着色 5.6ms，`W9A-062`）；
  tiled 26ms（含 17.7ms 着色，`W9A-063`）——**≈1.5× 而非 10×**，且是 2012 口径，倍数不可外推。
- **结构优势**：簇有固定最大 3D 尺寸⇒无 tiled 的视角相关退化（深度不连续把剔除退化到纯 2D，`W9A-065/066`）；
  可用簇法线做逐簇光源背面剔除（tiled 无此维度，`W9A-067`）。
- **tiled 不是陪跑**：tiled forward 相对 deferred 天然支持 MSAA 与半透明（`W9A-070`），
  tiled deferred 最坏情况波动最小、随 GPU 升级扩展最好（`W9A-071`）；论文实测最坏情况性能最优，
  作者明写**最坏情况是实时应用最重要的指标**（`W9A-073`）。
- **谱系**：Forward+ 是把 deferred 光管理搬进 forward 的先例（`W9A-118`）；该技术已进 SIGGRAPH 课程与 GPU Pro 书章
  （`W9A-068/119`）——**属成熟工程整合，研究风险低，风险在 wgpu 侧实现细节**。
- **对照基线**：Lightcuts 的"光源树"与 clustered 是同一问题的两种解（`W9A-103`）。

## 对 LSSMJ（T-PH-07 的取舍）

1. **主线**：forward + 光列表（tiled forward 起步，clustered 目标档）；半透明/ MSAA 需求支持这个方向（`W9A-070`）。
2. **验收**：P95 与最坏情况（非均值），聚类开销单列预算（`W9A-062/071/073`，对齐 C1）。
3. **数据结构**：簇存法线（背面剔除，`W9A-067`）；性能不随视角抖动是 clustered 的结构性动机（`W9A-065`）。
4. **对照**：tiled 保作退化路径；Lightcuts 只作多光源结构选型的对照（`W9A-103`）。
