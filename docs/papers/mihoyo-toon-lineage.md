# 米哈游系卡通渲染谱系（Unite 2017 演讲转录 + 官方 console deck + 社区还原代码）

- 一手：官方 PPT "From Mobile to Console: Genshin Impact's rendering technology on Console"
  `https://image.docswell.com/slide/KWRPQ5/98VYJZ8Q/download`（2026-10-03，HTTP 200，86 页 PDF）
  快照 `D:/KF/LSSMJ/scratch/w8a/raw/genshin_console.txt`
- 二手转录（**web 深度，不能单独支撑决策**）：游戏葡萄 Unite2017 贺甲演讲实录
  `https://www.gameres.com/750798.html`；博客园转录 `https://www.cnblogs.com/nafio/p/9137010.html`
  快照 `unite2017_gameres.txt` / `unite2017_nafio.txt`
- 社区还原代码（source）：`Gaolingx/GenshinCelShaderURP`（AvatarShaderUtils.hlsl 等）、`NoiRC256/URPSimpleGenshinShaders`（SimpleGenshinFacial_LightingEquation.hlsl）、`ChiliMilk/URP_Toon`（ToonLighting.hlsl）——均为第三方拆解，非米哈游官方代码
- 账本：W8A-013..016、028..032、049..062

## 要点（带锚）

1. **多材质多通道 Ramp**：Unite2017 自述角色渲染第一特性（W8A-028）；用 2D Ramp 的多通道控制上色层，纵轴可切软硬风格（W8A-029）。
2. **Ramp 行由 lightmap 分区驱动**：社区还原里 `lightmap.a` 分 5 档（0/0.3/0.5/0.7/1.0）选 ramp 行，横轴 halfLambert 采样（W8A-054）；昼夜（冷阴影）用同一图 V 轴 +0.5 切换（W8A-055）。
3. **面部另走 SDF**：面部不用普通 cel shade（"usually very ugly"），SDF 掩码 + 前向 XZ 点积 step（W8A-058、059、060）；左右两张掩码按光照方向选择（W8A-058），LdotF 由 C# 每帧算好送材质（W8A-053）。
4. **头发**：各向异性高光随动态光变化（拒绝贴图美术，W8A-030）；高低频双层高光 + Jitter map 调粗细（W8A-031）；Kajiya-Kay→Marschner 谱系的工程化（见 hair 分析）。
5. **头发→脸投影**单独遮罩（HairShadowMaskAtten，W8A-050）——阴影贴图分辨率不够做刘海。
6. **高精度角色阴影**：单独 2K shadowmap + PCSS + 半透明阴影（转录，W8A-105）；面部另用顶点色 mask 压阴影强度（W8A-104）。
7. **后处理按风格重调**：饱和度增强 HBAO、Bloom 提亮+色相偏移（W8A-032）——物理默认参数对卡通不适用。
8. **console 档工程数字（官方 deck，paper 深度）**：clustered deferred lighting 支持视内 1024 灯（W8A-013）；局部灯阴影贴图默认精度压缩率 29.85:1（W8A-014）；角色 capsule AO（W8A-015、016）。
9. **GI 的 NPR 化**：社区实现里直接丢掉 SH 细节项只留常数项，"hide 3D feeling"（W8A-061）。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **学**：①ramp 纹理形态（横=光照、纵=风格/材质/时段）+ lightmap 通道分区；②面部 SDF 分支（几个点积 + step，实现量级极小）；③头发→脸独立遮罩；④"角色从背景拔出"的角色 AO 通道。
- **弃/慎**：①Unite2017 转录是二手（web），其中"2K shadowmap/PCSS"等数字未获官方一手复核——只作旁证；②社区代码里的具体系数（`_RampIndex * -0.1 + 1.05`）是抄写值，不构成官方口径。
- **缺口**：官方从未公开 Genshin 角色 shader 全貌；本批拿到的官方一手只有 console deck（管线级数字）。

## 与 photoreal 共核的判据贡献

- 米哈游路线在**同一 Unity 管线**里做：几何/阴影/后处理（deferred、CSM/PCSS、HBAO）都是通用设施；差异化全在 shading（ramp/SDF）与描边 pass。
- 反证点：角色专属光照与 AO 通道（capsule AO）说明"NPR 参数是 per-object 特例"，共核需要一个"按对象挂 shading 覆写"的机制。
