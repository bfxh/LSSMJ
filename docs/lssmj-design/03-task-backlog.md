# 03 · 任务列表（Backlog：双终局目标 + 工程 + 分析工程）

> 口径：**任务 = 可验收单元**（有判据、有依赖、有依据锚）。判据编号见 `README.md`（C1–C6）与
> `02-scene-tier.md`（C7–C10）；NPR 轨的判据 C11–C13 为本表新增**草案**（锚待 `papers/npr-toon-rendering.md` 回填）。
> 状态图例：⬜ 未开始 · 🟡 进行中 · ✅ 完成。所有任务开工前须过对应门（`04-ci-and-gates.md`）。

## 轨 1 · UI 显示面（第一层，先做）

| ID | 任务 | 依赖 | 验收判据 | 依据 |
| --- | --- | --- | --- | --- |
| T-UI-01 | 位图契约参考实现（Frame→RGBA+命中表；Windows 分层窗贴图） | — | C2（宽度对表）+ C4（逐位金丝雀） | `w5a`；设计 README §3 |
| T-UI-02 | 文本三件套：opsz 分键 / trak / 覆盖率 gamma | T-UI-01 | C2 | `w5a` W5A-036..050、W5A-056 |
| T-UI-03 | 字体清单加载（mmap、按平台清单、失败回退链） | T-UI-01 | C5 | `w5a` W5A-023/024/091 |
| T-UI-04 | 内容版本号 + 布局复用（排一次） | T-UI-01 | C1；复用命中率记账 | `w4a` W4A-042；弃 `w5a` W5A-064/065 |
| T-UI-05 | damage 三层：tile 描述符双缓冲 + 矩形封顶 8 + 空脏跳过 | T-UI-04 | C3 | `w1e` W1E-055/058/082；`w2f` W2F-028..031 |
| T-UI-06 | 状态条式命中表 + 输入焦点栈（最小交互面） | T-UI-01 | 命中金丝雀（错位=红） | `w5a` W5A-077；`w4a` W4A-036 |
| T-UI-07 | 候选窗/overlay 等价物验收（与青简位图并排） | T-UI-01..06 | 并排对拍（`09-assessment` §9.5 协议） | `renderer-qingjian/07` |
| T-UI-08 | IME 生命周期接入（preedit/commit/delete 显式建模） | T-UI-06 | 事件序单测 + 平台真机 | `w3f` W3F-045..047 |
| T-UI-09 | 字形图集档（阈值触发；4×4 子像素键） | T-UI-02 | C1（稳态）；命中率≥99% | `w2d` W2D-020/036；阈值待测 |
| T-UI-10 | 亚像素可选档 + 否决清单（学 Chromium 11 条） | T-UI-02 | 显示链色边回归 | `w2c`；`w1f` W1F-048/067/074 |

## 轨 2 · 真实画质（photoreal，第二层）

| ID | 任务 | 依赖 | 验收判据 | 依据 |
| --- | --- | --- | --- | --- |
| T-PH-01 | wgpu 执行器（同显示列表换执行器）+ 三级降级 | T-UI-05 | C10 | `w6f` W6F-002..013；ADR I6 |
| T-PH-02 | glTF 2.0 核心 + 扩展（basisu/quantization）导入 | T-PH-01 | C8（回退测试） | `w6d` W6D-001/003/004/005 |
| T-PH-03 | 纹理链：KTX2 + Basis 转码 + BC7/ASTC 分档 + mipmap 全程 | T-PH-02 | 显存预算 + 采样对拍 | `w6a` W6A-004..009；`w6d` W6D-016/017 |
| T-PH-04 | PBR 材质基线（Metallic 二值纪律；IBL=探针+SH） | T-PH-03 | 与参考渲染器对拍 | `w6c` W6C-001/002；`w6h` W6H-006/007 |
| T-PH-05 | 阴影：CSM 默认（质量档 PCSS/SAVSM） | T-PH-04 | 走样/漏光对拍 | `w6c` W6C-010..013/021 |
| T-PH-06 | 烘焙 GI（Lightmass 式档位化）+ 探针 | T-PH-04 | 烘焙档位可切换 | `w6c` W6C-017..020 |
| T-PH-07 | 动态光：屏幕→远场等级序（Lumen 式）+ 降噪（SVGF 谱系） | T-PH-06 | 历史注入金丝雀 | `w6c` W6C-014..016；`w6j` |
| T-PH-08 | RT 混合档（可选）：DXR/Vulkan RT + 降噪成员 | T-PH-07 | 确定性条款（构建输入固定） | `w6b` W6B-001..005；`w6f` |
| T-PH-09 | 后处理最小集：TAA（速度缓冲）/FXAA 二选一；bloom 可选默认弱 | T-PH-01 | 后处理止于 UI 合成前 | `w7a` W7A-071/145；ADR J5/J6 |
| T-PH-10 | tonemap=ACES 双层 + HDR 能力查询/降级 | T-PH-09 | 显示链回归 | `w6c` W6C-004/006/008 |
| T-PH-11 | 可见性栈：距离→视锥→层级遮挡（上帧复用）→GPU 早退 | T-PH-01 | C7（假阴性金丝雀） | `w6e` W6E-001..018 |
| T-PH-12 | 资产管线：Draco/meshopt + ACL（体积即性能/折叠冗余） | T-PH-02 | C9（动画预算门） | `w6d` W6D-006..014 |
| T-PH-13 | 大世界：SVT/流送 + HLOD/World Partition 式单元 | T-PH-11 | 流送预算门 | `w6a` W6A-001..003/010；`w6e` W6E-009/010 |
| T-PH-14 | 场景内容：天空（Hillaire 优先）/froxel 透视/水（Gerstner·FFT）/粒子（离屏分数尺寸）/地形（clipmap+morph） | T-PH-11 | 各子系统独立对拍 | `w7c` W7C-001/010/019/028/031/045/058 |
| T-PH-15 | 虚拟几何（Nanite 式簇树）——最后立项，不许跳步 | T-PH-13 | 成本随屏幕分辨率 | `papers/gpu-rendering-techniques.md` |

## 轨 3 · 三渲二（NPR，第二终局目标；判据 C11–C13，锚已回填 `w8a`）

| ID | 任务 | 依赖 | 验收判据 | 依据 |
| --- | --- | --- | --- | --- |
| T-NPR-01 | 风格化 shading 基线：ramp/阶梯阴影 + 双色光照（与 PBR 共几何/可见性/纹理层） | T-PH-01..04 | C13（两管线同场景几何/轮廓不漂移） | `w8a` W8A-063/064（阶梯化 BSDF）+ W8A-040/044（UTS3 共核样本） |
| T-NPR-02 | 描边系统：inverted hull + 后处理边缘双路 + 线宽控制 | T-NPR-01 | C11（线宽分辨率无关对拍） | `w8a` W8A-006；`papers/outline-techniques.md` |
| T-NPR-03 | 法线编辑管线（Xrd 式：弃法线贴图+手工法线+逐角色专光） | T-NPR-01 | 资产管线回归（CI 内样例模型） | `w8a` W8A-002/004/010 |
| T-NPR-04 | 面部 SDF 阴影（`step(LdotF, SDF 掩码)`+左右双掩码+逐帧 LdotF） | T-NPR-02 | C12（面部阴影方向对拍） | `w8a` W8A-053/058/059 |
| T-NPR-05 | 头发 NPR（Kajiya-Kay→Marschner 谱系；工程双层高光） | T-NPR-01 | 高光带位置对拍 | `w8a` W8A-021/031 |
| T-NPR-06 | matcap/替换贴图档 + ramp 资产热更新（lightmap.a 五档分区式） | T-NPR-01 | 资产热更回归 | `w8a` W8A-054/055 |
| T-NPR-07 | 风格化 PBR 混合（PBR 底 + 艺术化压缩；沿用共享核） | T-NPR-01 | 双档切换对拍 | `papers/stylized-pbr-shared-core.md`（w8a） |
| T-NPR-08 | 2D-in-3D：billboard/纸片工作流 + 帧驱动动画记录 | T-NPR-02 | 与 2D 参考并排 | `papers/2d-in-3d-workflow.md`（w8a） |
| T-NPR-09 | NPR 后处理：描边/速度线/网点可选档 | T-NPR-02 | 关档逐位=基础档 | `w8a`（outline/风格化后处理条目） |
| T-NPR-10 | 风格一致性门：同资产在 NPR/PBR 双管线回归金样 | T-NPR-01..07 | 金样冻结哈希 | 冻结哈希惯例（BSHSQ 先例） |
| T-NPR-11 | **光照双模否证实验**：逐角色专光 × 全局时变光/GI 同开的小场景实测 | T-NPR-01、T-PH-07 | G3 实验判据（`w8a` 报告 §5） | 最强否证风险（`w8a` 共核结论） |

## 轨 4 · 工程与 CI（`04-ci-and-gates.md` 的落地）

| ID | 任务 | 依赖 | 验收判据 | 依据 |
| --- | --- | --- | --- | --- |
| T-CI-01 | `tools/ref_gate.py`（悬空 `W#X-nnn` 引用=红）+ 金丝雀 | — | 造一条悬空引用必须红 | `04` §B.5 |
| T-CI-02 | `tools/doc_gate.py`（TL;DR 锚/未验证节/TODO 白名单） | — | 缺节样例必红 | 同上 |
| T-CI-03 | GitHub Actions（A.2 YAML；含 A2 统计新鲜度） | T-CI-01/02 | 首跑全绿 + 注入坏账必红 | `04` §A.2 |
| T-CI-04 | 夜间链接抽查（告警档） | T-CI-03 | 抽样报告留档 | `04` §A.5 |
| T-CI-05 | `.agents/claims/` 并发认领协议文件化（scope 判红） | — | 交集样例必红 | qingjian-gates A1–A5 |
| T-CI-06 | perf 门模板（P95 棘轮 + 先验红 + 挂测态禁用） | 引擎代码起步 | 金丝雀证明门会红 | `reports/09`、`reports/16` |
| T-CI-07 | 平台矩阵（win/mac/linux 三档冒烟） | T-CI-03 | 三平台首跑记录 | `04` §B.4 |
| T-CI-08 | 冻结哈希纪律（默认档金样；换代须披露） | T-PH-01 起 | 换代清单机制生效 | BSHSQ 惯例 |
| T-CI-09 | 结构棘轮（god/dupe 门移植） | 引擎代码起步 | `--write-baseline` 带理由 | `reports/12` P1–P12 |
| T-CI-10 | 发布档流水线（tag→L3：全量+金样+披露） | T-CI-08 | 一次演练通过 | `04` §B.2 |

## 轨 5 · 分析工程自身（剩余自愿缺口 + 工具）

| ID | 任务 | 依赖 | 验收判据 | 依据 |
| --- | --- | --- | --- | --- |
| T-AN-01 | Karis/Frostbite/Hoffman course notes 全文（PDF 截断重取） | — | 每条带逐字锚 | `w6h` 缺口、`w7e` |
| T-AN-02 | RTG II 全文与 ReSTIR PDF（Open Access 可续） | — | 同上 | `w6j` 缺口 |
| T-AN-03 | MediaCodec/FFmpeg 文档正文（平台视频补全） | — | 同上 | `w6k` 缺口、`w7f` |
| T-AN-04 | Noesis 官网（403；换存档/二手→标注） | — | 只收 200 锚 | `w2g` W2G-009 |
| T-AN-05 | PBRT 章余量深挖（reflection/lights 全文） | — | 同上 | `w6h` |
| T-AN-06 | 论文目录学再生成（`reports/11` 随账本自动刷新） | 新增批次 | 与 ledger-stats 一致 | `reports/11` |

## 里程碑映射

| 里程碑 | 内容 | 包含任务 |
| --- | --- | --- |
| **M1 位图核心** | UI 档等价物 + 门基建 | T-UI-01..08、T-CI-01..05 |
| **M2 场景闭环** | wgpu + 资产/纹理/光照/阴影/可见性 | T-PH-01..06、T-PH-11/12 |
| **M3 画质双轨** | photoreal 动态光/后处理 + NPR 基线/描边/脸 | T-PH-07..10、T-NPR-01..06 |
| **M4 大世界与风格化收口** | 大世界/虚拟几何 + NPR 全量 + 发布档 | T-PH-13..15、T-NPR-07..10、T-CI-06..10 |

> 依赖原则：**先量后改**（每轨第一件事是把判据跑起来）；**M4 的虚拟几何不许跳步**；
> NPR 轨判据 C11–C13 在 w8a 报告落盘后由主代理回填锚与阈值。
