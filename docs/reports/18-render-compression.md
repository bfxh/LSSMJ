# 渲染压缩技术全线（第 8 轮 B：GPU 带宽/纹理/几何/深度/实验性，多源抓取 2026-10-03）

> **上游身份**：无单一仓——本轮是多源（AMD GPUOpen / Intel-Mesa 文档 / Khronos（Vulkan 扩展附录、KTX2 规范）/ 微软（BC 与 VRS 文档、DirectX-Specs）/ ARM astcenc / Binomial basis_universal / Google Draco 规范 / zeux meshoptimizer 源码 / crates.io 元数据 / Crossref 与 arXiv 检索）。
> **抓取物**：`D:/KF/LSSMJ/scratch/w8b/raw/`（104 个文件；语料 sha256 前缀 `2a5f024def41b561574095ba6bc01c15`）+ `txt/`（HTML/JSON→文本镜像，共 50 个）。
> **账本**：`docs/analysis/ledger/w8b.jsonl` **137 条 / verify rejected=0**（source 74 / doc 8 / paper 55，paper 占比 40.1%；不同锚 69 个）。本报告每条技术主张都指到 `W8B-xxx`。
> **抓取账目（超预算留档）**：URL 尝试 110 次（清单 88 + 临时 22），成功 ≈84、失败 ≈26（403 反爬：Khronos wiki/registry 部分路径、Intel 文档；404：GPUOpen 旧路径、Apple 文档 JSON、部分 repo README）。任务书要求 AMD 文章"多试路径"与反爬重试是超标主因——**超出 ≤90 预算 20 次，如实计入**。
> **口径**：判定分三档——**可用 / 有界（带条件，须显式标注降质或环境约束）/ 不可用**；性质分**无损 / 视觉无损 / 有损**；Rust 可用性分**纯 Rust / FFI 绑定（需 C/C++ 工具链）/ 无关（硬件侧）/ Rust 路线不可用**。

## TL;DR（10 条，每条带锚）

1. **帧缓冲压缩是硬件/驱动的透明能力，且主流是"真无损"**：AMD DCC 官方自称 lossless（`W8B-001/002`），Vulkan 明确"多数实现都支持某种帧缓冲压缩、通常对应用透明"（`W8B-021`）——引擎侧的工作是"别挡住它"，不是实现它。
2. **AMD DCC 有明确的启用载荷**：clear 用 0.0/1.0（`W8B-001..008` 段内）、别标 shader-readable（`W8B-005`）、部分写要读-改-写（`W8B-007`）、稀疏读会更糟（`W8B-008`）、D32F 可能比 D16 压得更小（`W8B-006`）；`DCC 可能被整体禁用`是既有边界的公开记录（`W8B-009`）。
3. **Intel 侧（CCS/MCS/HiZ）公开信息质量差**：Mesa 自述 CCS 是"文档化最差的部件"、内容靠逆向（`W8B-013`），但给出了统一模型——**主面 + 辅助元数据面**，元数据比主面小得多（`W8B-015/016`），读不到压缩态时要跑 resolve（`W8B-017`）。
4. **VRS 是明确的"降质换性能"**：官方原文"某些情形下降低着色率对可感知输出质量影响很小或没有"（`W8B-025/028`）；跨 DX12/Vulkan 可移植（`W8B-024/027`），但 **wgpu 未暴露**（`W8B-103`）。
5. **wgpu 档的纹理压缩"可用"已闭环**：`TEXTURE_COMPRESSION_BC/ETC2/ASTC(+ASTC_HDR)` 是正式特性位（`W8B-101/102`），ASTC 块尺寸以类型参数表达（`W8B-104`）。
6. **"视觉无损"在文献里的定义=低于感知阈值（JND）的压缩**（`W8B-052`），且 2026 年高保真区间的差异仍需**同位置交替**的更灵敏主观协议才能检出（`W8B-053`）——验收成本项，不能只报 PSNR（`W8B-107/109`）。
7. **KTX2 超压缩方案封闭为两个**：BasisLZ 与 Zstandard（`W8B-049`）；且规范明说 LZW 式无损超压缩对**已块压缩**数据收益有限（`W8B-050`）。
8. **Rust 工具链齐备（无阻塞项）**：BC7=`intel_tex_2`(MIT/Apache-2.0) 与 `ctt`（五家编码器 + `ispc-prebuilt`，MIT/Apache-2.0/Zlib，`W8B-058/059/132`）；ASTC=`ctt-astcenc`（astcenc 本体 Apache-2.0，`W8B-060/133`）；Basis=`basis-universal`(Apache-2.0) + 纯 Rust 转码器（`W8B-061/134`）；容器=`ktx2`(Apache-2.0, `W8B-057/135`)；通用无损=`zstd`(BSD-3-Clause)/`lz4_flex`(MIT)/`brotli`(BSD-3-Clause AND MIT)（`W8B-054/055/056`）。
9. **几何压缩的工程落点**：Edgebreaker/PM 谱系（`W8B-064..066/072`）→ Draco 位流（edgebreaker + rANS + sequential 三路，`W8B-067..071`）→ meshopt（最好 1 字节/三角形、典型 2 字节/索引、解码 3–6 GB/s、**编解码无损而唯一损失在量化**，`W8B-076..082`）；随机访问压缩另有专门谱系（`W8B-074/075/118`）。
10. **实验性技术四问判定**：NTC（SIGGRAPH 2023）自称随机访问+实时解压、质量优于 AVIF/JXL（`W8B-087..090`），但解码线程效率到 2026 仍是独立论文题目（`W8B-094`）、且浮点上下文推断已被指出跨平台不一致（`W8B-128`）⇒ **当前不可用（wgpu 路线不可用）**；NGLOD"快 2–3 个数量级"的分母是**先前神经方法**而非传统网格（`W8B-095`）。

## 可吸收 / 不可吸收（对"通用 Rust 引擎 + 视觉无损优先 + wgpu 档"这个目标）

**可吸收**

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 纹理压缩格式族（BC7/BC6H/ASTC/ETC2）走 wgpu 正式特性位 | `W8B-101/102/104` | 吸收（离线编码 + 运行期硬件解码，零运行期成本） |
| KTX2 容器 + Zstd 超压缩（装载侧 Rust crate 成熟） | `W8B-048/049/050/057` | 吸收（容器级无损，可与块压缩负载叠用） |
| 资产链用 `ctt`/`intel_tex_2`/`astcenc` 绑定做离线编码 | `W8B-058/059/060/132/133` | 吸收（许可干净；`ispc-prebuilt` 免自建工具链） |
| meshopt 顶点/索引编解码（无损，唯一损失在量化） | `W8B-076/077/078` | 吸收（glTF `EXT_meshopt_compression` 生态一致） |
| 硬件帧缓冲/深度压缩的"不阻挡"纪律（clear 值、少标 shader-readable、D32F） | `W8B-002/005/006/131` | 吸收（规范级免费收益，写成资产/管线约定） |
| 压缩质量回归基线（astcenc 的 PSNR 对比基线范式） | `W8B-043` | 吸收（正是本仓"质量不得回退"门的现成范式） |

**不可吸收 / 有界吸收**

| 项 | 锚 | 判定 |
| --- | --- | --- |
| VRS（可变速率着色） | `W8B-024..029/103/105/106` | 不吸收（wgpu 无暴露；性质=降质换性能，与视觉无损优先冲突） |
| 固定速率帧缓冲压缩（VK_EXT_image_compression_control 的请求档） | `W8B-022/023` | 不吸收默认档（非 bit-exact ⇒ 与帧间逐位金丝雀 C4 互斥）；仅作可查询的观测项 |
| ETC1S / BasisLZ 档 | `W8B-045/049` | 有界吸收（官方自述 low-to-medium quality，仅体积优先场景，默认关） |
| 神经压缩（NTC/NGLOD/3DGS 系） | `W8B-087..100/126..130` | 不吸收（Rust 路线不可用：CUDA/自定义训练链；且跨平台一致性未解决） |
| 随机访问网格压缩（论文路线） | `W8B-074/075/118` | 有界吸收（概念吸收进 meshlet/流送设计；无现成 Rust 实现） |
| 硬件厂商专属能力（DCC/CCS/HiZ） | `W8B-010/013/019` | 有界吸收（只能"不阻挡 + 观测"，不可依赖；锚定厂商+世代） |

## 判定表（逐技术一行：技术 / 性质 / Rust 可用性 / 判定 / 锚）

| 技术 | 性质 | Rust 可用性 | 判定 | 锚 |
| --- | --- | --- | --- | --- |
| AMD DCC（Delta Color Compression） | 无损（官方自称） | 无关（硬件/驱动侧，wgpu 无接口） | 可用（零代码；条件是别阻挡） | W8B-001/002/010 |
| AMD 深度块压缩（Depth Block，HTILE 一类的官方对照表述） | 无损 | 无关 | 可用（默认行为） | W8B-131 |
| Intel CCS（单采样色压缩，含 fast-clear 与 CCS_E） | 无损 | 无关 | 可用（代际差：IVB=clear-only，SKL+=全渲染） | W8B-011/012/014 |
| Intel MCS（多样本色压缩） | 无损 | 无关 | 可用 | W8B-015 |
| Intel HiZ（层级深度/模板压缩） | 无损 | 无关 | 可用（布局敏感：LOD 字段不支持等约束） | W8B-018/019/020 |
| 辅助面压缩统一模型（aux+主面，resolve 路径） | 无损 | 无关 | 可用（把 resolve 记进带宽账） | W8B-015/016/017 |
| VK_EXT_image_compression_control（固定速率压缩 + 查询） | 视觉无损（规范原文：一般视觉无损但非 bit-exact） | 无 Rust 需求；wgpu 未暴露查询 | 有界（默认关；与逐位金丝雀互斥） | W8B-021/022 |
| VK_EXT_image_compression_control_swapchain（展示路径） | 视觉无损 | 同上（壳层） | 有界（平台壳选型项） | W8B-023 |
| VRS（D3D12 Tier1/Tier2；VK_KHR_fragment_shading_rate） | 有损（官方："影响很小或没有"，定性非阈值） | 不可用（wgpu 无暴露） | 不可用（当前）；若启用须走感知模型（W8B-120） | W8B-024..029/103/105/106 |
| Coarse Pixel Shading（学术版，含时域超采样） | 有损（可用时域补质量） | Rust 路线不可用（论文） | 不可用（研究参考） | W8B-030/122 |
| BC7（LDR 高质量） | 视觉无损（8bpp，8 模式块） | FFI 绑定（intel_tex_2 / ctt-bc7f / ctt-bc7enc-rdo） | 可用（默认高质量档） | W8B-031/032/058/062 |
| BC6H（HDR） | 视觉无损（14 模式块） | FFI 绑定（同工具链；解码=硬件） | 可用（HDR 资产） | W8B-033 |
| BC1/BC3（低档） | 有损（BC1 alpha 差 ⇒ 实践中用 BC3） | FFI 绑定（etcpak 等） | 有界（只作体积档，默认关） | W8B-039 |
| ASTC 4x4–12x12（0.89–8bpp 连续） | 视觉无损→有损（随块尺寸） | FFI 绑定（ctt-astcenc / dashpack-astcenc-sys；astcenc=Apache-2.0） | 可用（按 bpp 分档；wgpu 类型参数表达） | W8B-035/036/060/104/133 |
| ASTC 编码六档预设（exhaustive…fastest） | 视觉无损（可调） | 同上 | 可用（离线时间预算旋钮） | W8B-035 |
| ASTC 通道去相关 | 可能降质（对相关信号） | 同上 | 有界（按内容类别实测后启用） | W8B-040/041/042 |
| ETC1S | 有损（0.3–3bpp，低到中等质量） | FFI 绑定（basis-universal）/纯 Rust 转码（basisu） | 有界（体积优先档，默认关） | W8B-045/061 |
| UASTC LDR 4x4 | 视觉无损优先（8bpp 高质量） | 同上 | 可用（跨平台默认高档） | W8B-046 |
| UASTC/Basis 转码面（ASTC/BC 等） | 无损转码（质量取决于负载档） | 同上 | 可用（一次编码多端转码） | W8B-047 |
| RDO 率失真后处理 | 有损（可控） | FFI 绑定（ctt-bc7enc-rdo / basisu RDO） | 有界（须质量度量背书） | W8B-046/062 |
| KTX2 容器（含 BasisLZ/Zstd 超压缩） | 无损（容器级） | 纯 Rust 解析（ktx2 0.5，Apache-2.0）+ zstd 绑定 | 可用 | W8B-048/049/050/057/135 |
| .astc 裸格式 | 无损（容器） | 纯 Rust 自解析（16 字节头） | 可用（离线中间物） | W8B-044 |
| Zstandard / LZ4 / brotli（通用无损） | 无损 | zstd=FFI(BSD-3-Clause)、lz4_flex=纯 Rust(MIT)、brotli=纯 Rust(BSD-3-Clause AND MIT) | 可用 | W8B-054/055/056 |
| Edgebreaker 连通性压缩 | 无损 | 无独立 crate（走 Draco 生态或自实现） | 可用（经 glTF Draco 扩展） | W8B-064/065/066 |
| Draco Edgebreaker 解码（含分裂事件表/多遍历） | 无损 | 纯 Rust 重写（draco-oxide / draco-core；亦有绑定） | 可用 | W8B-067/068/069 |
| Draco 顺序解码器 | 无损（比率低、解码快） | 同上 | 可用（随机访问/流式取舍项） | W8B-071 |
| Draco rANS 熵编码 | 无损 | 同上 | 可用 | W8B-070 |
| meshopt 顶点编解码 | 无损（编解码本身；量化是唯一有损层） | 绑定（meshopt）+纯 Rust 重实现（meshopt-rs） | 可用（默认） | W8B-077/078/081 |
| meshopt 索引编解码 | 无损（最好 1 字节/三角形，典型 ~2 字节/索引） | 同上 | 可用（默认） | W8B-076/082 |
| meshlet 压缩（3 字节/三角形，簇内随机访问） | 无损 | 同上 | 可用（大世界流送） | W8B-080 |
| Progressive Meshes（渐进流） | 无损（流可截断） | 无直接 crate（meshopt simplify 仅生成 LOD） | 有界（概念吸收；实现自研） | W8B-072 |
| 随机访问网格压缩（RAMC / RACBVH 谱系） | 无损 | Rust 路线不可用（研究） | 不可用（当前）；概念进流送设计 | W8B-074/075/118 |
| 论文级深度块压缩（平面/常量编码 2006；广义平面编码 2013） | 视觉无损（误差可控） | Rust 路线不可用（研究） | 不可用（硬件已内置同类） | W8B-083/085 |
| 随机/运动模糊下的深度压缩 | 有损/假设破裂 | 同上 | 不可用（条件外；负向留档） | W8B-084 |
| NTC（Random-Access Neural Compression of Material Textures） | 视觉无损（自称优于 AVIF/JXL） | **Rust 路线不可用**（CUDA + 自研训练链；wgpu 无协作向量） | 不可用（当前） | W8B-087/088/089/090/103 |
| NTC 后续（超网络 / 线程高效解码） | 视觉无损 | 同上 | 不可用（观察） | W8B-091/094 |
| NGLOD（神经几何 LOD） | 视觉无损（重建 SOTA；速度分母=神经方法） | 同上（tiny-cuda-nn 系） | 不可用（实时性口径不同） | W8B-095/096 |
| NeuralVDB（稀疏体表示） | 视觉无损 | 同上 | 不可用（体积线） | W8B-097 |
| 3DGS 压缩（HAC++/KISS-GS/FCGS/PCGS/HGSC/TC3DGS…） | 视觉无损~有损（高比率档） | Rust 路线不可用（无主线实现） | 不可用（观察；2026 已进综述期） | W8B-098..100/116/126/127/129 |
| 神经/熵编码的浮点上下文推断 | 有损风险（跨平台解码失败） | — | **不可用（硬红线：跨平台一致性）** | W8B-128 |
| 生成式补质量（GFix 类扩散增强） | 可能引入不可复现细节 | Rust 路线不可用 | 不可用（默认关；仅研究观察） | W8B-130 |
| 压缩与资产完整性（抗压缩水印） | — | — | 观察项（压缩链若引入须复查校验路径） | W8B-136 |

## §1 GPU 带宽 / 帧缓冲压缩：我们（wgpu 档）能用吗、要等什么

**能用（零代码，条件是"不阻挡"）**：
- AMD DCC 是"域特定、无损、对开发者透明"的帧缓冲压缩，自 GCN 1.2 起在独显/APU 上启用（`W8B-001/002/010`）；块内一个值全精度、其余 delta，块大小随访问模式自适应（`W8B-003`）。着色器核能直接读压缩色 ⇒ render-target→texture 的 barrier 近似 no-op（`W8B-004`）。
- **要付的代价（可写成管线约定）**：clear 用 0.0/1.0（`W8B-005` 段）；标了 shader-readable 的 RT 压缩更差、MSAA 深度目标受害最大（`W8B-005`）；部分通道写要读-改-写（`W8B-007`）；稀疏采样在压缩下更糟（`W8B-008`）；深度建议 D32F 并配 reverse-Z（`W8B-006`）；同时读写/部件不支持时会整体降级并触发解压（`W8B-009`）。
- 深度侧：AMD 官方把深度块压缩当作**既有多年的默认行为**（`W8B-131`）；Intel 侧 HiZ 自 Iron Lake 引入、按辅助面范式工作，且布局约束很硬（不支持 LOD 字段、需独立 stencil 面等，`W8B-018/019/020`）。
- **Intel 的公开知识边界**：Mesa 明说 CCS 是"文档化最差"（`W8B-013`），CCS 元数据密度被引到 16x16 组 128B 行对/1–2bit（`W8B-014`）——这类能力**只可观测、不可建模**；我们能做的判断只有"存在 + 别破坏 + 用工具读回"。

**要等（wgpu 档现状）**：
- 固定速率压缩（可请求 bitrate、可查询实际速率）由 `VK_EXT_image_compression_control(+swapchain)` 提供，规范口径是"一般视觉无损但非 bit-exact"（`W8B-021/022/023`）——**wgpu 未暴露该类查询**（`W8B-103` 的负向核查方法同款：特性表无相关条目）。
- VRS：三档设置路径（逐 draw / 逐图元 / 着色率图），Tier1 全档支持 1x1·1x2·2x1·2x2、Tier2 才有 8x8/16x16 tile 的着色率图（`W8B-026/027/105/106`）；**wgpu 无暴露**（`W8B-103`）。判定=不可用（当前），且其性质是降质换性能（`W8B-025/028`）——即使将来可用，也只能进"显式开关 + 默认关"的档位，并配感知模型（`W8B-120`）。

**旁证与学术侧（不单独支撑决策）**：Coarse Pixel Shading（`W8B-030/122`）、低延迟移动缓冲压缩（`W8B-086`）、无损+可伸缩色图压缩（`W8B-119`）。

## §2 高质量纹理压缩：质量、定义与 Rust 工具链

**质量档位（规范级事实）**：BC7=微软官方"高质量 RGB/RGBA"格式、16B/4x4=8bpp、每块 8 模式（`W8B-031/032`）；BC6H（HDR）=14 模式（`W8B-033`）；容器默认 DDS（`W8B-034`）。ASTC 侧：块尺寸把 bpp 变成连续选项（0.89–8bpp，6x6=3.56bpp；`W8B-035/036/047`），编码器提供六档时间-质量预设与内置质量测量模式（`W8B-035/037`）；ARM 自述 ASTC 在"给定码率的质量"与灵活性上均前进一大步（`W8B-038`，实现方口径=旁证）。**同 bpp 内的格式选择有真实差异**：BC1 因 alpha 质量差被 BC3 取代（`W8B-039`）；ASTC 的通道去相关对相关信号会**降质**（`W8B-040`）；少通道数据塞进多通道格式是浪费（`W8B-041`）；彩色数据用 sRGB 输入几乎总是感知胜利（`W8B-042`）。

**"视觉无损"的定义与验收成本**：
- 学术定义轴=感知相关性/JND（恰可察觉差异）阈值，传统量化未计入 ⇒ "视觉无损"是**相对阈值的声明**，不是"无差异"（`W8B-052`）。
- 高端（高保真）区间的差异越来越难被现有主观协议检出，需同位置交替看参考/失真的新方法（`W8B-053`）；HDR 链需要更细的评估刻度（`W8B-124`）；色度掩蔽是可压空间（`W8B-125`）；屏幕内容（4:4:4，文字/UI 类）是独立研究线（`W8B-123`）。
- 度量侧：感知质量度量是活跃研究域（`W8B-107/109/110/111/112`），窗口图像上的视觉无损研究是最接近 UI 面板的样本类（`W8B-051`）。**结论**：我们的"视觉无损"判据必须写明测量装置/观看条件/度量与主观协议，且要自建语料库（`W8B-111`）。

**超压缩与容器**：KTX2 的 `supercompressionScheme` 只有 BasisLZ 与 Zstandard 两个官方选项（`W8B-048/049`）；LZW 式无损超压缩对**已块压缩**数据收益有限（`W8B-050`）；`.astc` 是"16B 头+负载"的最简形态（`W8B-044`）。

**Rust 可用性（含许可，2026-10-03 元数据）**：

| 环节 | crate / 仓库 | 类型 | 许可 | 锚 |
| --- | --- | --- | --- | --- |
| BC7 编码 | `intel_tex_2` 0.5（ISPC 绑定） | FFI（构建期 ISPC） | MIT/Apache-2.0 | W8B-058 |
| 多编码器聚合 | `ctt` 0.6（intel/bc7enc/bc7f/etcpak/astcenc/amd + `ispc-prebuilt`） | FFI（预编译可免工具链） | MIT OR Apache-2.0 OR Zlib | W8B-059/132 |
| BC7（备选） | `ctt-bc7f` / `ctt-bc7enc-rdo` / `rusty_dds` | FFI / 纯 Rust（部分） | 随上游 | W8B-062 |
| ASTC 编码 | `ctt-astcenc` / `dashpack-astcenc-sys`（astcenc 本体 Apache-2.0） | FFI | Apache-2.0（本体） | W8B-060/133 |
| Basis 编解码/转码 | `basis-universal` 0.3（绑定）/ `basisu` 0.1（纯 Rust 转码，bit-exact） | FFI / 纯 Rust | Apache-2.0 | W8B-061/134 |
| KTX2 容器 | `ktx2` 0.5 | 纯 Rust（解析） | Apache-2.0 | W8B-057/135 |
| 通用无损 | `zstd` 0.14 / `lz4_flex` 0.14 / `brotli` 9.0 | FFI / 纯 Rust / 纯 Rust | BSD-3-Clause / MIT / BSD-3-Clause AND MIT | W8B-054/055/056 |
| ISPC 参考实现许可 | `GameTechDev/ISPCTextureCompressor` | （C++，供绑定） | MIT（仓库元数据） | W8B-063 |

**结论**：纹理压缩的 Rust 路线**无不可用项**；唯一需要注意的形态差异是"FFI 绑定（功能全、需 C 工具链）vs 纯 Rust（部署简单、功能窄）"，按"离线编码用绑定、运行期解码靠硬件"分层即可。

## §3 几何 / 网格压缩

- **奠基谱系**：Edgebreaker（Rossignac 1999, DOI 10.1109/2945.764870）先压连通性（`W8B-064`），Wrap&Zip 给解压算法（`W8B-065`），SwingWrapper 是后续改良（`W8B-066`）；Progressive Meshes（Hoppe 1996, DOI 10.1145/237170.237216）把"渐进流=可截断压缩流"的同一性立起来（`W8B-072`）；综述入口=Maglo 等 2015（`W8B-073`）与 Rossignac 2005 手册章（`W8B-115`）。
- **工业形态（Draco 位流规范）**：连通性数据带 `edgebreaker_traversal_type` 与符号计数（`W8B-067`）；解码流程 Parse→分裂事件→TraversalStart→连通性重建（`W8B-068`）；支持 STANDARD 等多种遍历，各带独立缓冲（`W8B-069`）；属性走 rANS 熵编码（`W8B-070`）；另有"顺序解码器"路径（`W8B-071`）。
- **工程口径（meshoptimizer）**：索引最好 1B/三角形（比 16 位原始小 6 倍）、典型 ~2B/索引（`W8B-076`）；顶点解码 3–6 GB/s（官方未给机型/版本，须带限定，`W8B-077`）；**编解码本身无损，唯一有损层是量化**（`W8B-078`），且语义编码后的数据仍可再通用压缩（`W8B-079`）；meshlet 三角形可按 3B/三角形解码（`W8B-080`）；SIMD 要求 SSSE3+POPCNT 并有标量回退（`W8B-081`）；格式版本写在头字节（`W8B-082`）。
- **随机访问**：Yoon & Lindstrom 2007 的 RAMC（`W8B-074`）、RACBVH 2009（`W8B-075`）、"随机访问+算术编码"1999（`W8B-118`）——概念并入流送/meshlet 设计，**当前无 Rust 实现可用**。
- **Rust 路线**：`meshopt` 绑定（0.6）与 `meshopt-rs`（纯 Rust 重实现）均存在（`W8B-062` 同族检索）；Draco 侧有 `draco-oxide`（重写）、`draco-core`（纯 Rust 编解码）、`bevy_gltf_draco`（`W8B-062` 检索列表）——**几何压缩在 Rust 侧同样无不可用项**，代价是 Draco 系重写项目的成熟度需另行评估（未验证，见 §7）。

## §4 深度 / 模板压缩

- **硬件侧**：Intel HiZ（`W8B-018/019/020`）、AMD 深度块（`W8B-131`）——均属"主面+辅助面"范式（`W8B-015`），应用侧不可编程，只能遵守布局与访问约定。
- **学术侧**：Hasselgren & Akenine-Möller 2006 的"高效深度缓冲压缩"（平面/常量假设，`W8B-083`）是硬件设计的论文对应物；2011 年指出**运动模糊/随机光栅化下深度不再平面**（假设破裂，`W8B-084`）；2013 年广义平面编码把收益口径做成"降低总带宽"（`W8B-085`）。
- **判定**：自研深度压缩在本项目**不可用**（硬件已内置同类；自研无收益且与驱动压缩冲突），价值在于"理解边界+在深度附件布局上别踩约束"。

## §5 实验性技术：四项判定（实时解码成本 / 显存 / 质量 / 可复现 / Rust 可行性）

| 技术 | 实时解码成本 | 显存/存储 | 质量 | 可复现性 | Rust 可行性 | 判定 |
| --- | --- | --- | --- | --- | --- | --- |
| **NTC**（SIGGRAPH 2023, TOG 42(4) Art.88；arXiv 2305.17105） | 自称按需实时随机访问（类 GPU 块压缩形态，`W8B-088`）；但解码线程效率到 2026 仍是独立论文题目（`W8B-094`） | 省磁盘+显存（联合压多纹理与 mip 链，`W8B-088`）；多解锁两级细节=16× 纹素（`W8B-087`） | 自称低码率下优于 AVIF/JPEG XL（`W8B-087`，作者口径未独立复现） | 训练须自研实现（比 PyTorch 快一个数量级，`W8B-089`）⇒ 复现门槛高 | **不可用**（CUDA/自研链；wgpu 无协作向量，`W8B-103`） | **不可用（当前）** |
| **NGLOD**（CVPR 2021, arXiv 2101.10994） | 稀疏八叉树只查必要 LOD，比先前神经 SDF 快 2–3 个数量级（`W8B-095`）——**分母是神经方法** | 八叉树特征体 + 每形状网络 | 重建质量 SOTA（几何+图像空间指标，`W8B-095`） | 需 CUDA 与专用训练；无引擎级集成样本 | **不可用** | **不可用（当前）** |
| **神经场/体积压缩**（NeuralVDB，arXiv 2208.04448） | 层级网络查询（未给实时口径） | 稀疏体压缩 | 高分辨率稀疏体表示（`W8B-097`） | 同组路线 | 不可用 | 不可用（观察） |
| **3DGS 压缩线**（HAC++/KISS-GS/FCGS/PCGS/HGSC/TC3DGS；综述 2026，`W8B-116`） | 渲染快（splat 光栅），压缩成本在离线（`W8B-100` 免优化前馈=进展信号） | 剪枝 15.7×（KISS-GS 口径，`W8B-098`）、"100X"（HAC++ 自称，基线须核对，`W8B-099`） | 视觉无损~有损随比率 | 逐场景优化是常态（FCGS 针对此，`W8B-100`）；浮点上下文已致跨平台熵解码失败（`W8B-128`） | 不可用（无 Rust 主线） | **不可用（观察；2026 已进综述期）** |

**对"通用引擎"的净结论**：神经压缩的共同结构是"离线贵、运行期推理 + 跨平台一致性风险 + 工具链绑定 CUDA"；且"图像压缩"不能直接当"纹理压缩"用——纹理要按需实时随机访问、要 mip 多分辨率、要联合压多通道组（`W8B-092/093`）。与本仓"跨平台像素级一致 + 引擎整用 Rust"两条底线冲突。**须清楚意识到不行**：现在引入任何一条都会把可复现性风险转移到运行期（`W8B-128` 是已被文献记录的实例）。保留的观察窗口只有两条：NTC 的"随机访问+实时"能否在 Vulkan 协作向量（`W8B-103` 未暴露）落地后进入 wgpu；3DGS 压缩若出现纯 CPU/Rust 的可复现编解码栈。

## §6 坑与反例（负面留档）

1. **"压缩=更快"是错的**：稀疏采样在压缩下 cache 双重抖动，shadow map 过滤可能"从很差到更差"（`W8B-008`）。
2. **压缩会被静默降级**：部件不支持/同时读写时 DCC 整体禁用并触发解压（`W8B-009`）——性能账必须含"压缩是否仍启用"的观测。
3. **部分写与压缩互斥**：需读-改-写，成本转移到隐藏读回（`W8B-007`）。
4. **跨平台数值不一致会直接毁解码**：浮点上下文推断致熵解码失败（3DGS 侧的 2026 年实例，`W8B-128`）——对本仓"跨平台像素级一致"是硬红线。
5. **"2–3 个数量级更快"的分母陷阱**：NGLOD 的分母是神经方法（`W8B-095`）；"100X 压缩"的基线是未压缩 3DGS 且叠加了剪枝/量化/熵编码（`W8B-099`）。
6. **同名能力有代际差**：Intel 颜色压缩 IVB=仅 clear，SKL+=全渲染（`W8B-011/012`）；DCC 限定 GCN 1.2+（`W8B-010`）。
7. **本文档自身的否证留档**：初稿凭记忆写的 Edgebreaker DOI（10.1109/2945.817351）经 Crossref 实查是另一篇（ball-pivoting），正确值 10.1109/2945.764870（`W8B-108`）——**锚错毁整条主张**。
8. **"视觉无损"宣称的层级**：规范级（非 bit-exact，`W8B-022`）、实现方级（ASTC"前进一大步"，`W8B-038`）、论文级（NTC"优于 AVIF"，`W8B-087`）三者证据强度递减，不可混用。

## §7 未验证项（写明缺什么证据）

1. **AFBC（Arm Frame Buffer Compression）**：Khronos/ARM 官方页与 Mesa panfrost 文档均未在可抓取文本里给出口径（抓取物只到"Compressed texture support"标题层级）⇒ 移动侧固定速率压缩**未验证**。
2. **Apple 的 memoryless render target**：Apple 文档 JSON 路径两次 404，未取得逐字引文 ⇒ Metal/TBDR 侧省带宽机制**未验证**（同族：Metal 的 tile 内存策略）。
3. **AMD HTILE 的一手命名与参数**：本轮只拿到"DCC 文中对 Depth Block 的官方对照表述"（`W8B-131`）与论文侧对应物，**没有** AMD 白皮书/ISA 中 `HTILE` 词的逐字引文 ⇒ 该术语在本报告中标注为"HTILE 一类"。
4. **Intel CCS 的具体 bit 布局与 resolve 成本数字**：仅到"元数据密度"与"需要 resolve"的文档级陈述（`W8B-014/017`），无实测成本。
5. **DCC/VRS 的实际带宽收益**：AMD 文章给的是定性（"save more bandwidth"），**无本机可复算读数**；VRS 亦只有"free performance"的口径（`W8B-025`）。
6. **NTC/NGLOD 的质量与解码成本未独立复现**：全部为作者摘要口径（`W8B-087..096`），本报告不把其数字用于任何预算。
7. **Rust 侧 Draco 重写项目（draco-oxide/draco-core）的成熟度与位流兼容性**：仅拿到 crates.io 描述（`W8B-062`），未读源码、未跑对拍。
8. **wgpu 未来特性时间表**：VRS / 协作向量 / 图像压缩控制是否排期，无官方路线图证据（`W8B-103` 只证明"当前没有"）。
9. **抓取预算超支 20 次**（110 vs ≤90）：已如实计入；若需严格合规，应把"多路径重试"改为先查目录页再取件。

## 补充证据条目（未逐条进正文，均在账本内）

- 视觉无损阈值的**内容依赖**：立体图像（`W8B-113`）与 JPEG2000 内容（`W8B-114`，弱刊仅作旁证）——阈值不可跨内容照搬。
- 几何/点云序列的视觉质量研究线（`W8B-117`）：与纹理侧共用同一判定语言。
- NPR 与感知质量的交叉（`W8B-121`）：G-B 管线可复用同一主观协议。
- 2D 高斯泼溅做神经视频压缩（`W8B-137`）：视频轨（`15-video-pipeline`）的观察项，不进默认链。

## 复算命令

```bash
# 账本自检（应为 rejected=0）
python D:/KF/LSSMJ/tools/ledger.py verify --file D:/KF/LSSMJ/docs/analysis/ledger/w8b.jsonl
python D:/KF/LSSMJ/tools/ledger.py stats  --file D:/KF/LSSMJ/docs/analysis/ledger/w8b.jsonl
# 抓取物清单与哈希
python -c "import json,glob;[print(f, len(json.load(open(f,encoding='utf-8')))) for f in sorted(glob.glob('D:/KF/LSSMJ/scratch/w8b/raw/_manifest*.json'))]"
# 生成器（引文一律从落盘文件抽取，缺针即报错）
python D:/KF/LSSMJ/scratch/w8b/gen_w8b.py
```

**来源清单（37 组）**：AMD GPUOpen（DCC 文章、RDNA 性能指南）；Intel/Mesa（ISL: CCS、HiZ、Aux surface compression、Tiling；panfrost）；Khronos（VK_EXT_image_compression_control(+swapchain)、VK_KHR_fragment_shading_rate、VK_EXT_texture_compression_astc_hdr/3d、KTX2 规范）；微软（BC7、BC 总览、D3D12 VRS 文档、DirectX-Specs VRS 规范）；ARM（astcenc README/Encoding/FileFormat/Testing）；Binomial（basis_universal README）；Google（Draco 位流规范 8 件）；zeux（meshoptimizer README/vertexcodec/indexcodec）；gfx-rs（wgpu-types features/texture）；crates.io（zstd/lz4_flex/brotli/ktx2/intel_tex_2/ctt + 3 组检索）；GitHub API（ISPCTextureCompressor 元数据、仓库检索 3 组）；Crossref（13 组查询/记录）；arXiv（6 组 API 查询：NTC/NGLOD/神经纹理压缩后续/3DGS 压缩/视觉无损）；Linux 内核文档（i915，未产出条目）。
