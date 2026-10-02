# SDF 面部阴影（原神式）：三份社区还原代码 + 一手约束

> 说明：面部 SDF 无官方一手材料（米哈游未公开）；本文用**三份独立社区还原**（source 深度）交叉核对同一算法形态。三份代码来自不同作者、同构实现——"独立实现趋同"是本文能给出的最强证据级别。

- `ChiliMilk/URP_Toon` @ master：`Assets/ChiliMilkToonShader/Include/ToonLighting.hlsl`、`Script/SetSDFShadowProperty.cs`（W8A-049、050、053）
- `Gaolingx/GenshinCelShaderURP` @ main：`Shaders/V5.0Beta/ShaderLibrary/AvatarShaderUtils.hlsl`、`AvatarGenshinOutlinePass.hlsl`（W8A-054、055、056、057）
- `NoiRC256/URPSimpleGenshinShaders` @ master：`SimpleGenshinFacial_LightingEquation.hlsl`、`NiloOutlineUtil.hlsl`（W8A-058..062）
- 账本：npr-sdf-face 目标 9 条

## 算法形态（三份实现共同点）

1. **掩码**：预生成面部 SDF/阈值贴图（左/右两份），按光照方向选一份：`RdotL > 0 ? faceShadowMapR.r : faceShadowMapL.r`（W8A-058）。
2. **二维光照量**：用光照方向的 XZ 平面投影与角色 forward/left 的点积（LdotF），而非 N·L（W8A-049、W8A-053）。
3. **判光**：`step(normalizedFdotL, faceShadowMap)` —— 归一化前向点积与掩码比较出一条极简判光式（W8A-059）。
4. **步进+羽化**：`radiance = 1 - saturate((1 - sdfShadowMask - LdotF - (shadowStep*2-1)) / shadowFeather + 1)` —— 阈值与羽化由 shadowStep/shadowFeather 两参数控制（W8A-049）。
5. **faces 是显式特例**：`_IsFace? lerp(0.5,1,litOrShadowArea)`，注释直言 N·L 在脸上"usually very ugly"（W8A-060）。
6. **CPU 契约**：`_LdotFL` 由 C# 每帧计算送入材质（W8A-053）——SDF 脸需要一个逐帧 CPU→GPU 参数通道。
7. **头发投影**：非面部 path 需乘 `HairShadowMaskAtten`（头发在脸上的投影独立遮罩，W8A-050）。
8. **rim 与深度复用**：附近还有一条用**深度图横向偏移**采样的 rim light（W8A-057）——同一角色 pass 里深度缓冲被风格光复用。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **学**：面部 SDF 分支（实现量级≈几个点积+一次 step）；L/R 双掩码 + 光照方向选择；Step/Feather 参数对；逐帧 LdotF 的上传通道（顺带说明渲染图需要一个"角色面部参数"节点）。
- **代价**：需要 SDF 掩码生成工具（从多角度阈值图平均/插值，社区教程为 Zhihu/CSDN 二手，本批未采）；掩码是"角色专属资产"，不入通用纹理链。
- **慎**：三份实现都是**第三方还原**（source 级代码证据，但非官方规格）；系数（0.5 lerp、阈值 0.9 等）属还原者选择，不应视为原神实际参数。

## 与 photoreal 共核的判据贡献

- 面部 shading 在实现上就是"同一条光照函数里按材质标签走不同分支"——支撑"共核 + shading 分支表"。
- 面部特例化也说明：共核需要"按材质/部位派发 shading 变体"的机制（而非全局统一 BRDF）。
