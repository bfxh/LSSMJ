# Salvi（Intel）· An Excursion in Temporal Supersampling（GDC 2016）

> 来源：GDC2016 讲稿（NVIDIA 开发者站镜像 PDF，`salvi_gdc2016.txt` 快照；讲稿为逐页讲词版）。
> 账本：`../analysis/ledger/w7a.jsonl` W7A-044..050（全 paper，7 条）；日期 2026-10-02。

## 要点（每条带账本行）

1. **现代 TAA 的两段式**：旧方法复用"单样本"，新方法复用"解算后的颜色"（resolved color）——后者才能
   只保留上一帧、大幅降成本；用移动指数平均（EMA）连续积分（W7A-044：典型混合比=本帧 10% + 历史 90%）。
2. **邻域裁剪=主因**：讲稿明言 "Neighborhood clipping [MALAN 2012][KARIS 2014] is the main ingredient
   behind the success of recent temporal supersampling methods"（W7A-045）。
3. **纯 AABB 的缺陷**：仅用本帧局部色的 AABB 做线段裁剪，仍会产出远离凸包的色→ghosting；作者提出
   **Variance Clipping（方差裁剪）**：用本帧样本分布的一二阶矩（均值 μ、标准差 σ）构造 AABB（W7A-046）。
4. **参数与取舍**：AABB 尺寸=γ·σ，γ=1 为推荐；γ 越大越稳但 ghosting 越多，越小则失去时间积分（W7A-047/048）。
5. **VC 引发的闪烁**：细几何/光照特征落进采样缝隙→方差骤缩→历史被整体裁剪→闪烁；对策是"先降低空间
   走样"（更多采样或预滤波），而非改 VC（W7A-050）。
6. **组合纪律**：MSAA/CSAA/SSAA 与 TAA 同用时应把 TAA 放在 resolve 之前；或 post-resolve 使成本与
   样本数脱钩（W7A-049）。
7. **多速度处理**：滤窗跨到不同速度的物体时取"最长运动向量"（前景优先）在滤波中心追踪最快特征（W7A-159）。
8. **噪声复用**：TAA 可用时间随机积分降噪（W7A-160）；VC 窗口 3x3→7x7 的权重账：更大窗口更稳但
   ghosting 更多；矩可预滤波（类似 variance shadow map 的做法）。

## 对 LSSMJ 的落点

- **裁剪盒的第三代**：min/max（Lottes/Malan）→ 圆化/YCoCg（Karis）→ 方差（Salvi）。LSSMJ 的 TAA
  质量档位可直接映射三代：档 1=3x3 min/max，档 2=圆化 clip，档 3=方差裁剪（γ=1 默认）。
- **闪烁判据**：VC 分析给出"细特征+抖动"的闪烁机理——LSSMJ 的 TAA 验收用例须含细几何（栅栏/线缆）
  与静止相机（看是否自闪），与 survey 的"冻结收敛"判据互补（W7A-053/054）。
- 该讲稿大量内容与 Karis/Playdead 交叉印证（10/90、邻域裁剪、最长速度），可作为**第三方复核**。

## 未验证 / 边界

- 讲稿无性能读数（未给 ms）；VC 的增益幅度未在本材料量化（只有图示）。
- 讲稿为"逐页讲词"文本快照，公式以文字描述形式出现（γ/σ），未逐符号复算。
- 讲者属 Intel（后加入 NVIDIA），讲稿托管在 NVIDIA 开发者站镜像；引用时按 GDC2016 讲稿口径。
