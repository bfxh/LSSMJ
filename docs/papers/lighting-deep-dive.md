# 光照深度补挖（第五轮：各向异性 / IBL / PBR 谱系文本）

> 补齐已登记的缺口（各向异性一手材料、IBL、PBRT/Disney 文本深挖）。
> 账本：`../analysis/ledger/w6h.jsonl`（13 条；快照 `scratch/g5/{aniso,ibl}` + `scratch/w6c/raw/*.pdf.txt`）。

## TL;DR（带锚）

1. **各向异性的根因是一手规范的句子**：斜视下"像素滤波足迹又长又窄"（`ext-aniso.txt:32`，W6H-001）。
2. **扩展刻意不规定算法与质量**（W6H-003）——各向异性是"尽力而为"档，**验收必须对拍不能靠承诺**。
3. **API 语义句可直引**：D3D11_FILTER_ANISOTROPIC"对缩小/放大/mip 采样全用各向异性插值"（W6H-004）；MINIMUM 变体是采样器家族的另一支（W6H-005）。
4. **IBL 是材质模型的输入而非后处理**：Filament 材料文档中 IBL 与车漆材质层直接耦合（W6H-006）。
5. **反射探针=环境采集的工程化**：官方定义句"捕获全方向球面视图"（W6H-007）；理论底座=SH 辐照度（Ramamoorthi 2001，W6H-012）。
6. **PBR 是工程折中谱系**：Disney 笔记自问"要不要完美能量守恒"（W6H-008）；clearcoat 自认 ad-hoc 近似（W6H-010）——**引用 PBR 时必须带这个语境**。
7. **纹理预滤波的统一表述**：≤去高频（PBRT 纹理章，W6H-011）——与 Williams 1983（W6A-013）成对，mipmap=预滤波的特例。
8. **天空的解析谱系起点**：Preetham 日光模型 Crossref 锚（W6H-013；与 A3 代理批的 Bruneton/Hillaire 轮互补）。
9. **对 UI 档：无**（位图候选窗不采样环境贴图、无 mipmap 链，`texture-systems.md` 结论不变）。
10. **对场景档**：采样器矩阵按"质量对拍"验收；IBL=反射探针/环境图+SH；PBR 参数纪律照 Disney/UE 口径（W6C-001/002 已入账）。

## 本轮贯通的技术点（此前缺）

| 点 | 状态 | 锚 |
| --- | --- | --- |
| 各向异性规范文本 | ✅ 取得（Khronos 镜像） | W6H-001..003 |
| 各向异性 API 语义 | ✅ 取得 | W6H-004/005 |
| IBL 官方材料（Filament） | ✅ 取得（147KB 原文） | W6H-006 |
| 反射探针官方定义 | ✅ 取得（缓存） | W6H-007 |
| Disney PBR 全文本 | ✅ PDF 抽取 59KB，本批取 3 条 | W6H-008..010 |
| SH / Preetham 论文锚 | ✅ Crossref | W6H-012/013 |
| Karis/Frostbite/Hoffman course notes 全文 | ⚠️ **伪 PDF/截断**（自 shadow 站返回的流不完整） | 缺口（w7e 备） |
| LearnOpenGL IBL 教程 | ⚠️ 406 拒抓 | 缺口 |

## 未验证

- Disney 全文本只取 3 条（余量在快照，按需续挖）。
- PBRT 章（reflection/lights/textures）为分页缓存，本批取 1 条；Karis/Frostbite 笔记 PDF 不可用（截断）——**这三份是场景档 P5 前的关键补齐项**，登记为 `w7e`。
