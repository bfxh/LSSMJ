# Unity Toon Shader（UTS3）：官方 toon 着色层的一手材料

- 官方文档（doc）：`https://docs.unity3d.com/Packages/com.unity.toonshader@0.9/manual/index.html`（2026-10-03，HTTP 200）
- 官方仓库（source，gh api 逐字取）：`Unity-Technologies/com.unity.toonshader` @ master
  - `com.unity.toonshader/Runtime/Shaders/UTSLighting.hlsl`（跨管线光照核，34 行）
  - `.../Shaders/URP/UniversalToonOutline.hlsl`、`.../BuiltIn/UCTS_Outline.cginc`、`.../Scripts/Utilities/Toon3Das2DMaterialUtility.cs`
  - 文档 md：`Documentation~/Outline.md`、`ShadingStepAndFeather.md`
- 账本：W8A-039..048、041、042、043

## 要点（带锚）

1. **定位**：面向赛璐璐 3D-CG 动画作者（"cel-shaded 3D-CG animations"）的 toon shader 集（W8A-039）。
2. **跨管线**：官方声明兼容 Built-in / URP / HDRP（W8A-040）——**toon 外观层与渲染管线解耦的官方样本**。
3. **三色体系**：基色 / 1st 阴影色 / 2nd 阴影色为角色设计的三主角，另有 Highlight/Rim/MatCap/AngelRing 等（官方 index 文本）。
4. **步进+羽化 API**：Base Color Step 划边界、Base Shading Feather 控过渡、1st/2nd 两级阴影（W8A-043）。
5. **跨管线共用光照核**：`UTSLighting.hlsl` 以 `t = saturate(1 - dotNL)` 驱动三色线性着色，用 smoothstep 实现边界羽化（W8A-044、045）——同一份核被 URP/HDRP/Built-in 各方 include。
6. **描边两模式**：沿法线挤出 / 整体缩放（mesh scale），另有宽度图、Z 偏移、相机距离衰减参数（W8A-041、042）。
7. **描边宽度是距离函数**：`_Outline_Width * 0.001 * smoothstep(_Farthest_Distance, _Nearest_Distance, dist)`（W8A-046）；外扩方向可用烘焙法线替换（W8A-047）。
8. **描边=独立 pass**：C# 侧 `SetShaderPassEnabled(ToonConstants.SHADER_LIGHT_MODE_NAME_FOR_OUTLINE, enabled)` 即开/关（W8A-048）；仓库里还有 `Toon3Das2DMaterialUtility`（"3D 当 2D"工具）文件。

## 对 LSSMJ 三渲二轨（P-NPR）的取舍

- **学**：①shading 核与管线解耦（LSSMJ 的 WebGPU/wgpu 档直接照此分层：核心光照函数 + 各后端 pass 装配）；②Step/Feather 两参数化的阴影边界 API；③描边参数族（宽度/宽度图/Z 偏移/距离衰减/烘焙法线）——这是描边层的最小完备接口；④描边可整体开关（质量档/性能闸门）。
- **慎**：UTS 是"动画生产"取向（美术可逐场景指定光照颜色，无视场景真实光色）——LSSMJ 若同时要 photoreal，需要把"光照颜色覆写"设为可选层而非默认。

## 与 photoreal 共核的判据贡献（本份 = 最强正面证据）

- 同一官方包在三条渲染管线上提供同一 toon 外观；光照核是一份被多处 include 的纯函数。
- 结论形态：**共核可行，切面在"shading 函数 + 描边 pass + 若干美术覆写参数"**，几何/可见性/纹理/后处理链无需二套。

## 未验证

- UTS 在 URP/HDRP 下"特性差异"清单（官方 Feature Difference 页）未逐条读；差异是否触及共享核尚未核。
- 未跑 Unity 工程实测；所有结论为文档/源码口径。
