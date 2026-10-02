# Playdead · Temporal Reprojection AA in INSIDE（GDC 2016 + 开源实现）

> 来源：GDC2016 讲稿 PDF（`playdead_gdc_pdf.txt`，985KB，pdftotext 快照）+ 仓库源码
> `github.com/playdeadgames/temporal` @ `4795aa0007d464371abe60b7b28a1cf893a4e349`（MIT，Unity 5.0+）。
> 账本：`../analysis/ledger/w7a.jsonl` W7A-001..026（paper 17 / source 6 / doc 3）；日期 2026-10-02。

## 为什么要给这份材料单独一篇

它是本波唯一"讲稿 + 出货源码"双份齐备的 TAA 材料：讲稿给动机/取舍/数字，源码给默认值与实现细节，
两者可互相复核（讲稿说 halton(2,3)x16，源码里确实有 `points_Halton_2_3_x16`）。

## 要点（每条带账本行）

1. **结构=反馈回路**：输出即下一帧历史（W7A-001）；目的=时间上回收子像素信息（W7A-002）。
2. **jitter 三步骤**：取采样偏移→算投影偏移→剪切视锥（W7A-003）；采样序列演进 4-tap/uniform4-helix
   → `halton(2,3)` 前 16 点（W7A-014）。
3. **速度缓冲**：动态物体单独 pass 写速度，重投影=读取+减法（W7A-004）；边缘运动要在 3x3 内取
   最近深度片元的速度（速度膨胀，W7A-005）；接入靠逐物体标注，蒙皮网格打标昂贵（W7A-025）。
4. **ghosting 对策的演化**：深度拒绝+速度加权被否（"太脆弱"、阈值本身在产生 ghosting，W7A-006）；
   转 4-tap min-max 邻域钳制的动态版（W7A-007）→ Karis 式圆化 3x3 + clip（W7A-008/009）。
5. **混合与参数**：`c_feedback = lerp(c_in', c_hist', k_feedback)`（W7A-010）；开源默认
   feedbackMin=0.88 / feedbackMax=0.97（W7A-017）；反馈要高于 0.9 才有留存，但要防可见循环（W7A-012）。
6. **拖尾机理**：历史片元若没有邻居把它挤出去就会滞留（W7A-011）；兜底=输出端按速度切换运动模糊，
   `k_trust = invlerp(15, 2, ||v||)`（W7A-013）。
7. **性能**：整条 temporal pass 在 Xbox One、1920x1080 下约 1.7ms（W7A-015，自述口径）。
8. **谱系**：直接受 Yang09 Amortized Supersampling（W7A-016）与 CryENGINE3（Sousa/Jimenez）启发；
   README 致谢四条线：Sousa（邻域钳制）、Karis（YCoCg clip+圆化）、Lottes（亮度差加权）、McGuire（运动模糊）（W7A-026）。
9. **源码细节**：`clip_aabb` 只朝 AABB 中心裁剪（注释自述"fast!"，W7A-019）；默认
   `useClipping=true`、`useYCoCg=false`（W7A-018）；速度缓冲内置 tile(20) 邻域最大速度（W7A-022）。
10. **MSAA 互斥**：接入说明明确关 MSAA（W7A-024）。

## 对 LSSMJ 的落点

- 场景档若上 TAA：**实现清单** = jitter（Halton(2,3)x16）+ 速度缓冲（含膨胀）+ 3x3/圆化剪裁盒
  + EMA（反馈 0.88–0.97）+ 可选运动模糊兜底；全部参数有出货默认值可抄（W7A-013/017/018/021）。
- 成本账：1080p 全屏 TAA ≈ 1.7ms（2016 主机口径）——LSSMJ 的档位表先按此数量级建模，本机待测。
- 速度缓冲是**管线契约**而非 pass：需要每物体打标/引擎内建，LSSMJ 显示列表若扩网格项，
  要么内建速度输出（推荐），要么承担 Playdead 的标注成本（W7A-024）。

## 未验证 / 边界

- 1.7ms 是 Playdead 自述、机器=XB1、分辨率=1080p，未注明是否含速度缓冲生成；本机无复测。
- 讲稿的 `k_trust`/反馈数值是 2016 主机 30fps 语境，直接搬到高帧率 UI/场景档需重调（假设待测）。
- YCoCg 裁剪在 INSIDE 最终未启用（讲稿 bonus slide 自述）；本仓按 RGB 记录默认。
