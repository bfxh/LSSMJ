# 动画电影感（anime film look）后处理链（第七轮 W8C；抓取 2026-10-03；无上游 commit——文献/文档/规范批量）

> 口径（用户）：目标画质 = 日本泡沫时期（约 1986–1992）科幻动画的**类胶片电影感**的可复现技术面
> （赛璐璐+胶片印片+霓虹辉光+高饱和夜城）；**不评作品本身**；作品名仅作"时代气质"引用，
> **不主张任何版权素材（扫描颗粒/胶片贴图/作品画面）可内置**。
> 账本：`../analysis/ledger/w8c.jsonl`（**108 条**：paper 55 / doc 46 / web 6 / source 1；`verify` rejected=0）。
> 快照与自检：全部资料落 `D:/KF/LSSMJ/scratch/w8c/raw/`（含 PDF→text 与 HTML→text 转换，转换脚本 `mktext` 式
> 直译、非 UTF-8 快照先落 UTF-8 再比对）；`scratch/w8c/gen_ledger.py` 对**每条 quote 做逐字快照比对**
> （107/107 命中才落账；另有 1 条报告行 source 锚）。**行引文全部来自本地快照**，URL 行引文同样经该门——不是凭记忆写的。
> 关系：三渲二 shading/描边基线在 W8A（`npr-toon-rendering.md`）；后处理最小集与 TAA 纪律在 W7A
> （`post-processing-and-aa.md`）。**本轮只做"胶片感/动画电影感"的新面，不重复 W7A 的 TAA/FXAA/Bloom 结论。**

## TL;DR（10 条，每条带账本锚）

1. **颗粒是生成式物理模型，不是叠贴图**：IPOL 2017 的实现逐像素用 Monte Carlo 求值
   （W8C-001/002），颗粒集=泊松圆盘之并的 Boolean 模型（W8C-004/005），物理上"照片本质是二值函数"（W8C-003）。
2. **分辨率无关是硬需求，且"扫描贴图"路线被三处独立否证**：CGF 明确"结果被初始扫描的质量与分辨率锁死"
   （W8C-016）、样例法两次合成必同结果（W8C-018/019）；SSVM 进一步证明协方差随灰度变化 ⇒ 固定分辨率扫描
   "本质上不正确"（W8C-027）。IPOL 同口径（W8C-011 白噪声不真实、W8C-010 35GB 内存算例）。
3. **"颗粒在显示/输出域施加"有规范级共识**：ITU-T H.Sup21 定义"解码之后作为后处理阶段合成"（W8C-053/054），
   AV1 规范把颗粒写成解码器在 output 数组上的义务（W8C-058/060/061）；放大链（DLSS）同样要求
   分辨率相关效果放在放大之后（W8C-079/080）。
4. **参数面很窄（可直接抄的默认值）**：IPOL 只有 3 个主旋钮（半径均值/方差/放大倍数，W8C-008），
   滤波默认 σ=0.8 输出像素且"有理论依据"（W8C-009）；颗粒强度必须随灰度/曝光调制（W8C-005/027/043）。
5. **halation 是物理项而非滤镜**：厂商定义=高对比亮区周围的细红/橙光晕（W8C-066），可逐通道定义扩散距离
   （"红通道更远"，W8C-067），分主/次两级光晕（W8C-068）；负片侧本有专门的**防光晕底层**（W8C-072）——
   复刻 halation = 恢复被厂商特意抑制的物理效应。
6. **镜头眩光的最显著构件是镜片内反射（ghosting）**（W8C-029/036），呈色来自抗反射镀膜的残反射
   （W8C-032/033），光圈叶片数决定光斑多边形（W8C-035）；该路线 2011 年即达交互-实时（W8C-031/037）。
7. **印片/调色链有三件可对齐的一手资产**：Kodak 2383 正片的感光曲线口径（W8C-076/077/078）、
   ACES 的"Rendering Transform 与 Display Encoding 分离"（W8C-084/085）、开源 OpenDRT 的参数面
   （W8C-086/087，含"阴影颗粒均值压 0 的 offset"这类工程细节）。
8. **镜头/画幅感是纯物理量**：自然渐晕=cos⁴θ（PBRT 源码级公式，W8C-088/089）；走片抖动=整幅图像在固定
   画幅边界下的**刚性位移**（W8C-098/099/100，含可实现的 hash 偏移模型与 arXiv 的 OU 过程 W8C-038）；
   画幅感=留黑+圆角+比例预设（W8C-071）。
9. **次序纪律（与既有链的接缝）**：去条带→颗粒（AV1 明示可选处理在颗粒合成之前，W8C-059）；
   **3D LUT 不能承载空间效果**（厂商明示勾选"LUT 兼容"即移除 halation/grain/weave，W8C-063）；
   halation/film gate 假设"画面填满画幅"（W8C-069）；抖动不是动态模糊，别再叠（W8C-099）。
10. **动画侧复刻有实录可考（二手）**：MEGALO BOX（2018）为复刻赛璐璐年代感，**刻意先降采样再升采样**，
    而非叠颗粒（"叠一堆颗粒会扎眼"，W8C-101/102）；该路线被批评"混叠/并不像老动画"（W8C-103）——
    这是本轮唯一找到的在播作品级"动画胶片感"实施记录，但**全部为二手**。

## 可吸收 / 不可吸收（对"LSSMJ 场景档 + 高帧率 UI"这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 生成式颗粒（泊松/Boolean 物理模型，显示域合成） | W8C-001/002/004/005 | **吸收**（默认档，架构第一原则） |
| 颗粒参数面（半径均值/方差/σ=0.8px/按亮度调制） | W8C-008/009/027/043 | 吸收（参数表直接抄） |
| 高斯近似快速合成（白噪声×稀疏矩阵） | W8C-025/026 | 吸收（实时档实现路线） |
| 去条带(dither)→颗粒 的次序 | W8C-059/090/091 | 吸收（顺序红线） |
| halation（阈值隔离+逐通道扩散+两级光晕） | W8C-066/067/068/069 | 吸收（有界：画面填满画幅 + UI 排除） |
| film gate 画幅（留黑/圆角/比例）、gate weave（亚像素刚性位移） | W8C-071/070/098/100 | 吸收（画幅档；weave 默认亚像素） |
| 印片/调色（S 曲线+subtractive+split tone+bleach bypass） | W8C-062/063/076/084/085 | 吸收（LUT/着色器双轨，见 §3） |
| 暗角=cos⁴θ 物理项 | W8C-088/089 | 吸收（先物理项、后美术叠加） |
| 镜头眩光 ghost（物理件） | W8C-029/032/035 | 有界吸收（质量档：预烘 ghost 阵列/光线束；默认只做 halation） |
| 降采样"扫描质感"档 | W8C-101/102/103 | 有界吸收（须以混叠可见度为验收项；默认不启用） |
| 划痕/尘点 | W8C-046/047/048 | 有界吸收（默认关；做则跨帧持续+衍射型剖面） |
| 逐颗粒全帧 Monte Carlo（离线级） | W8C-010 | 不吸收到实时档（内存/时间边界；留离线预烘） |
| 扫描颗粒贴图 / 固定分辨率样例 | W8C-016/018/027 | **不吸收**（三处否证 + 版权风险） |
| 采集端烘焙（把颗粒烘进资产） | W8C-082/083 | 不吸收（不可逆；LSSMJ 保持显示端可调） |
| 无参考指标当验收 | W8C-039 | 不吸收（会奖励"去胶片化"） |

## 1. 效果链表（效果 / 物理依据（锚）/ 实现位置 / Rust 可行性 / 判定）

| # | 效果 | 物理依据（锚） | 实现位置（相对既有链） | Rust 生态可行性 | 判定 |
| --- | --- | --- | --- | --- | --- |
| E1 | 胶片颗粒 grain | 泊松圆盘 Boolean 模型；二值→滤波（W8C-003/004/005/007）；蓝噪声/信号相关（W8C-043/044） | 后处理链末端（tonemap/AA/print 之后、UI 合成之前） | **可用**：全屏 pass（wgpu 既有，W8C-096）；随机源 rand_pcg（W8C-093）；可选 noise（W8C-092）；CPU 位图档 image（W8C-095） | 默认档（弱） |
| E2 | 去条带 dither/deband | 低精度缓冲产生可见条带（W8C-090）；全屏 debanding"非常便宜"（W8C-091）；颗粒前处理次序（W8C-059） | 在 E1 之前（同一 pass 可合并随机数） | 可用（纯 shader） | 默认档 |
| E3 | halation（红橙光晕） | 染料层反射/散射（W8C-066）；逐通道扩散距离（W8C-067）；主/次两级（W8C-068）；负片防光晕层（W8C-072） | print/grade 之后、grain 附近；阈值隔离→分通道模糊→加回 | 可用（阈值 LUT + 3 次分离模糊；无新依赖） | 默认档（弱-中） |
| E4 | Bloom（阈值辉光） | W7A 已有（多尺度 mip+Karis average）；本链只补"霓虹触发"侧：少量极亮源最突出（W8C-030） | 与 E3 同段，可共享降采样链 | 已有（W7A） | 可选（默认极弱，URP 默认 0 的先例见 W7A-139） |
| E5 | 镜头眩光/ghost | 内反射=ghosting（W8C-029/036）；镀膜残反射呈色（W8C-032/033）；光圈多边形（W8C-035） | 质量档（独立 pass；物理件或预烘 ghost 阵列） | 有界：自研光线束追踪 or 预烘；R(λ,θ) 可存 2D 纹理（W8C-033） | 质量档（默认关） |
| E6 | print/grade（印片+调色） | 感光曲线（W8C-076）；rendering/display 分离（W8C-084/085）；S 曲线量化例（W8C-104）；bleach bypass/split tone（W8C-050/062） | tonemap 之后的"第二层映射"（print 层） | 可用：shader 解析式或 3D LUT；CPU 侧 palette/image（W8C-094/095）；互操作可走 ocio-rs（有界，W8C-097） | 默认档（弱；做成"动画印片"预设） |
| E7 | 暗角 vignette | cos⁴θ + 出瞳几何（W8C-088/089）；厂商暗角模块同页（W8C-062） | print 层内 | 可用（纯 shader，极便宜） | 默认档（极弱） |
| E8 | 色差 CA | 本轮只到"镜头系统/镀膜/像差"程度（W8C-031/037）；**未取到 Kolb 1995 的正文化差系数**（见未验证） | print 层内（径向通道缩放） | 可用（3 次采样重映射） | 有界（仅弱档；强档需先补一手） |
| E9 | 走片抖动 gate weave | 整幅刚性位移、非模糊（W8C-098/099）；hash 偏移模型（W8C-100）；OU 过程（W8C-038） | 显示域最后一级（UI 之前），亚像素 | 可用（全屏重映射，一次采样） | 默认档（亚像素；可在设置里关） |
| E10 | 闪烁 flicker | 厂商：投影闪烁的温和脉冲（Amount/Rate，W8C-105）；arXiv 剪辑级参数（W8C-048） | 与 E6/E9 同段 | 可用（逐帧标量乘） | 有界（默认关；极弱档） |
| E11 | 画幅 film gate | 留黑/圆角/比例预设（W8C-071） | 链末端几何（应在 UI 之下、与窗口布局解耦） | 可用（几何裁切） | 有界（做成"画幅档"设置） |
| E12 | 划痕/尘点 | 衍射型剖面（W8C-047）、跨帧寿命（W8C-046） | 质量档/离线 | 可用 | 默认关 |
| E13 | 降采样"扫描质感" | 在播作品实录（W8C-101/102）；混叠批评（W8C-103）；MTF 是系统量（W8C-075） | 整链前端（场景渲染分辨率档） | 可用（分辨率档） | 有界（默认关；须混叠验收） |

Rust 可用性总注：本轮核对 6 个件（W8C-092..097）——`noise` 0.9.0、`rand_pcg` 0.10.2、`palette` 0.7.7、
`image` 0.25.10、`wgpu` 30.0.1（均为 crates.io 元数据口径，非本机实测）、`ocio-rs` 0.2.1（OpenColorIO
v2.5.2 绑定）。**E1–E13 没有一项需要新增引擎级依赖**：wgpu 全屏 pass + `rand_pcg`/`noise` 随机源即可覆盖
默认档；`ocio-rs` 只建议进离线工具链（绑定型依赖，不进渲染热路径）。

## 2. 与既有后处理最小集（W7A）的整合

### 2.1 建议管线位置（场景档；UI 档维持无后处理，W7A-071/145 的合成纪律不变）

```
场景渲染(HDR) → [tonemap: ACES 拟合, W6C] → AA: TAA(有速度缓冲)/FXAA
   → E6 print/grade(LUT或shader) → E3 halation / E4 bloom(弱)
   → E2 deband/dither → E1 grain → E9 weave / E10 flicker / E11 film gate
   → UI 合成（此后不再有任何滤波/重映射）
```

规则（每条有锚）：
- **颗粒在 AA 之后、UI 之前**：显示域施加（W8C-053/058）；放大/时序链同样要求分辨率相关效果在其后
  （W8C-079/080）。若把 grain 放在 TAA 之前，历史缓冲会把它当噪声抹平/拖影（W7A 的 TAA 反馈回路与
  邻域裁剪机制，W7A-001/036..048）。
- **去条带在颗粒之前**：AV1 的规范注把去条带列为颗粒合成之前的可选处理（W8C-059）。
- **halation/bloom 只作用于场景层**：厂商警告小画面下 halation 会出意外（W8C-069）；UI 是贴着相机的
  alpha 内容（W7A-039），不得进入该 pass。
- **LUT 与空间效果分层**：3D LUT 不能表达 halation/grain/weave（W8C-063）——LUT 只承载 E6/E7 的逐像素
  颜色数学；E1/E3/E9 必须独立 pass。
- **动态分辨率/放大档**：后处理不得假设"只在渲染分辨率下处理"（W8C-080）；grain/halation 的像素尺度
  参数要按输出分辨率标定（IPOL 的 σ=0.8 输出像素口径，W8C-009）。

### 2.2 默认值建议（带锚；空白处为 LSSMJ 待定，不做主张）

| 参数 | 建议默认 | 锚 |
| --- | --- | --- |
| grain 滤波 σ | 0.8 输出像素 | W8C-009 |
| grain 半径分布 | log-normal（或常数） | W8C-006 |
| grain 强度 | 随亮度/密度调制，暗部收敛 | W8C-005/027/043 |
| grain 种子 | 逐帧显式种子（可复现） | W8C-060 |
| grain 开关粒度 | 逐帧/逐场景可关 | W8C-061 |
| deband | 全屏 dither，默认开、极便宜 | W8C-090/091 |
| halation 触发 | 阈值隔离（高对比亮区） | W8C-066/067 |
| halation 色相 | 红橙（逐通道扩散距离：红>绿>蓝） | W8C-066/067 |
| weave 幅度 | 亚像素（默认），速率≈片门频率档 | W8C-070/100 |
| flicker | 默认关；开则 Amount 极弱 | W8C-105/048 |
| 画幅 | 比例预设（1.85:1 或 4:3）+轻微圆角，二选一 | W8C-071 |
| vignette | cos⁴ 物理项为底，美术叠加极弱 | W8C-088/089 |

### 2.3 与 W8A（三渲二）的接缝

shading/描边/ramp 在 W8A 已定；本链**不改 shading**，只在后处理段加"显影/印片"层。Brejon 的行业类比
（拍摄:渲染 = 显影:合成，W8C-049）正是这条接缝的表述：动画电影感=在既有光路之上做"显影"。

## 3. 质量红线（已知冲突；负面留档）

1. **无参考指标会奖励"去胶片化"**：arXiv 实测"标准无参考质量指标主动奖励历史上不真实的幻觉与过度平滑"
   （W8C-039）。⇒ 胶片感档的验收必须用成对/人工判据，不能用无参考分数。
2. **halation/film gate 假设画面填满画幅**（W8C-069）；带黑边/小画面的输入会出意外——UI、字幕、窗口
   内嵌视频不得进入该 pass。
3. **3D LUT 承载不了空间效果**（W8C-063）；任何"把胶片感导出成一张 LUT"的方案必然丢掉 halation/grain/weave。
4. **固定分辨率贴图颗粒=确定性冻结**（W8C-018）且协方差错（W8C-027）——视频档会看出"纹样静止"。
5. **降采样档会混叠**（W8C-103），且"叠颗粒会扎眼"（W8C-102）——强度与分辨率感必须联动。
6. **抖动≠动态模糊**（W8C-099）——再叠运动模糊会把锐利错位变成糊。
7. **每帧独立尘点=闪烁噪点**（W8C-046 的持久化模型即为此而设）。
8. **分辨率写死假设**会破坏放大链（W8C-080）。
9. **全帧逐颗粒 Monte Carlo 的内存/时间**（35GB 算例，W8C-010）——实时档必须走高斯近似（W8C-025/026）。
10. **采集端烘焙不可逆**（W8C-083）——不要学相机内烘焙；LSSMJ 保持显示端可调。

## 4. 来源地图（22 个来源；快照 `D:/KF/LSSMJ/scratch/w8c/raw/`）

| # | 来源 | 类型 | 账本 |
| --- | --- | --- | --- |
| 1 | Newson/Faraj/Galerne/Delon《Realistic Film Grain Rendering》IPOL 7:165–183 (2017) | paper | W8C-001..014, 106 |
| 2 | Newson/Delon/Galerne《A Stochastic Film Grain Model…》CGF 36(8) (2017, HAL) | paper | W8C-015..023 |
| 3 | Newson/Faraj/Delon/Galerne《Analysis of a Physically Realistic Film Grain Model…》SSVM 2017 (HAL) | paper | W8C-024..027 |
| 4 | Hullin/Eisemann/Seidel/Lee《Physically-Based Real-Time Lens Flare Rendering》SIGGRAPH 2011（作者版 PDF） | paper | W8C-028..037 |
| 5 | Jastrzebski 等《AbsoluteDegradation》arXiv:2607.02131v1 (2026-07) | paper | W8C-038..048, 107 |
| 6 | Brejon《CG Cinematography》ch.1 / ch.9 | paper（权威长文） | W8C-049..052, 104 |
| 7 | ITU-T H.Sup21 (01/25)《Film grain synthesis technology for video applications》 | doc（规范） | W8C-053..055 |
| 8 | 3GPP TR 26.855（j00）《Study on Film Grain Synthesis》 | doc（技术报告） | W8C-056/057 |
| 9 | AV1 规范（aomediacodec.github.io，7.18.3 等） | doc（规范） | W8C-058..061 |
| 10 | Blackmagic《DaVinci Resolve 19.1 Reference Manual》（Resolve FX/Film Emulation 章） | doc（厂商） | W8C-062..071, 105 |
| 11 | Kodak VISION3 5219/7219 技术数据表（H-1-5219） | doc（厂商） | W8C-072..075 |
| 12 | Kodak VISION Color Print Film 2383/3383 技术数据表 | doc（厂商） | W8C-076..078 |
| 13 | NVIDIA《DLSS Super Resolution Programming Guide》(310.6.0, 2026-03) | doc | W8C-079/080 |
| 14 | ARRI《Textures》官方页（Image Science） | doc（厂商） | W8C-081..083 |
| 15 | ACES Documentation《Output Transforms — Implementor Notes》 | doc | W8C-084/085 |
| 16 | jedypod/open-display-transform README（OpenDRT） | doc（开源项目） | W8C-086 |
| 17 | vkdt 文档《OpenDRT》参数镜像页 | doc | W8C-087 |
| 18 | PBRT 3ed《Realistic Cameras》（pbr-book.org） | doc（书） | W8C-088/089 |
| 19 | Godot《3D rendering limitations — Color banding》 | doc | W8C-090/091 |
| 20 | crates.io API（noise/rand_pcg/palette/image/wgpu/ocio-rs） | doc（包元数据） | W8C-092..097 |
| 21 | Video Artifacts（gostrobrod.dev）frame jitter / projector gate weave | web | W8C-098..100 |
| 22 | SAKUGABOORU《Production highlights: MEGALO BOX》01/02（译文+评论） | web | W8C-101..103 |

## 5. 未验证（不许当结论）

1. **零本机实测**：E1–E13 全部未在 LSSMJ 上实现/测量；所有默认值与成本均来自文献/文档口径，观感阈值未标定。
2. **SMPTE RDD 5 正文未取**（SMPTE 标准文档不在公开抓取渠道；本轮只到 ITU 的引用级，W8C-055）。"SMPTE 颗粒规范"的具体算法
   （64×64 变换、LUT 结构）**不在本账本内**。
3. **Kolb 1995《A Realistic Camera Model》未取到**（三个镜像失败）——色差/渐晕的经典一手缺位；
   E8 只到"可见像差由镜头系统产生"的程度（W8C-031/037），**色差系数与实现未验证**。
4. **ARRI Textures White Paper 与 HPA 2023 演讲 PDF 403**，未读；ARRI 结论只到官方页（W8C-081..083）。
5. **3GPP TR 26.855 仅读目标章**（未通读其 100+ 页评估数据）；AV1 规范只读颗粒相关段与 note。
6. **OpenDRT 深入文档未读**（wiki 原始页取不到，000）；只到 README 与 vkdt 参数镜像（W8C-086/087）。
7. **动画侧全部二手**：MEGALO BOX 记录来自 SAKUGABOORU 译文+评论（W8C-101..103）；ja.wikipedia
   「撮影(アニメ制作)」在本机不可达（000），CGWORLD 站内检索无结果 ⇒ **"80 年代赛璐璐+胶片"的官方拆解
   一手材料本轮未获**，不立账。
8. **"夕阳/夜城渐变天空"的实时化一手材料未取到**：本轮只到"条带治理是前提"的工程证据（W8C-090/091）
   与 W8A 的 ramp/昼夜轴；背景美术实时化留缺口。
9. **5 个抓取失败留档**：Kodak H-61 LAD PDF（404）、motion.kodak.com 扫描扩展密度 PDF（000）、
   wikiwand/ja.wikipedia（000）、Blender 参考页（403，Glare 节点文档未取）、HPA 2023 演讲 PDF（403）。
10. **版权**：本文出现的作品名仅为"时代气质"引用；未主张任何扫描颗粒/胶片素材/作品画面可内置。
    grain/halation 的公开算法（IPOL/CGF/SSVM/AV1/ITU）与实现均不含版权素材。

## 6. 落点建议（阶段）

- **P-FILM-0（默认档，本链最小可用集）**：E2 deband → E1 grain（高斯近似，σ=0.8px、3 旋钮）→
  E3 halation（阈值+逐通道扩散）→ E7 暗角（cos⁴）→ E9 weave（亚像素）；全部挂在 W7A 链的 tonemap 之后、
  UI 之前。验收：无参考指标禁用（W8C-039）、UI 边界不变（W7A）、TAA 下颗粒不拖影（用 W7A 的 C12/C13 判据扩一条）。
- **P-FILM-1（画幅/调色档）**：E6 print 预设（对表 Kodak 2383 曲线语义，W8C-076）+ E11 film gate + E10 flicker 弱档。
- **P-FILM-2（质量档）**：E5 镜头眩光（先预烘 ghost 阵列，后光线束）、E12 划痕/尘点、E13 降采样档（须混叠验收）。
