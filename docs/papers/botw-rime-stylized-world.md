# 风格化世界：BotW（CEDEC 2017 报道）与 Rime（GDC 摘要）

## BotW（web 二手：IGN Japan 对 CEDEC 2017 讲演的报道）

- 快照：`D:/KF/LSSMJ/scratch/w8a/raw/botw_ign.txt`；URL `https://jp.ign.com/the-legend-of-zelda-hd/17049/botw3d`（2026-10-03，HTTP 200）
- 讲者（报道自述）：任天堂 滝澤智（美术）与 堂田卓宏（程序），CEDEC 2017
- 账本：W8A-066..068（web 深度——**只能作旁证**）

要点：
1. **自我定义为"风格化=记号化"**（「スタイライズド」は「記号化された」と訳せる）——先定设计思想（响应可读性），再推画面（W8A-066、067）。
2. **技术的服务目标**：玩家动作→世界立刻给出可辨识反馈（W8A-067）。
3. **三层空间结构 + 参数精简**：远景=大气散射、中景=雾（Y 向）、近景=大气散射遮蔽；200+ 参数砍到 50 个交美术（W8A-068）。
4. **拒绝预计算**：为"边看边画"，排除不适合实时预览的方案，选实时渲染路线（报道原文）。

对 LSSMJ 取舍：①"记号化程度"可作为风格档的显式参数（可读性优先）；②参数暴露面要有上限（美术可用性=功能）；③三层纵深（远/中/近）与 LSSMJ 场景档的 LOD/剔除柱同构，可作背景合成规范。**慎**：全部为报道二手，不支撑精确技术决策。

## Rime（doc 级：官方 GDC session 摘要；+二手 VFX 讲演页）

- 官方 session 页：`https://gdcvault.com/play/1020812/Rime-A-Symphony-of-Images`（W8A-034）
- 二手：`https://simonschreibt.de/gat/stylized-vfx-in-rime-water-edition/`（Unreal Fest 2018 讲演，VFX 用水面/火/烟的手绘化材质）

要点：
1. 官方摘要只讲**美术方向**："forcing a minimalist style"（去语言/NPC/UI）（W8A-034）——**rasterization/shading 技术细节在官方摘要里没有**。
2. 该 GDC 讲是 Art Director 的 Visual Arts 场；技术侧公开材料（Stylized VFX in RIME）为**第三方 VFX 讲演**：手绘纹理+UV 扭曲做风格火/水（二手）。

对 LSSMJ 取舍：**Rime 只能作为"美式卡通=极简大色块+手绘纹理"的美术参照**，其渲染管线主张（如"PBR 材质改造成卡通"）本批**未获一手**——网络上常见转述不可入账（见未验证）。

## 未验证（重要）

- Rime 的"非 cel-shaded、是 UE4 PBR 改造"说法在检索摘要中出现，但其原始出处（80.lv 访谈）本批未抓取核对 → **不立账**。
- BotW 无官方幻灯片（CEDEC 资料付费墙）；本文全部为报道二手。
