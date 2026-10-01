# 青简（qingjian，Rust 输入法）@ c08ae57cb88b6a4a46f4a5e9c1d6d11c5e69222e, 抓取 2026-10-01

> 范围：全仓（crates/* 除 `crates/qingjian-render` 本体、apps/*、tools/*、docs/*、CI）。
> 本地路径 `D:/KF/LSSMJ/scratch/src/qingjian`（只读分析，未跑 cargo/未改文件）。
> 账本：`docs/analysis/ledger/w1a.jsonl`，176 条，全部 `source` 深度（本报告每条主张指到 `W1A-###`）。
> 上游 URL：https://github.com/qingjian-team/qingjian ；许可证 GPL-3.0-or-later（`Cargo.toml:21`）。
> 版本现状：workspace 库版本 `0.1.1`、各壳 `0.1.5-dev`（`W1A-150`，壳版本各自写死是约定）。

## TL;DR（每条带锚）

1. **分层判据是可检验的一句话**：Core 平台无关、平台层只做「事件翻译 + 贴图」。判据写在 `crates/qingjian-core/src/lib.rs:5`「换掉 IMK 换成 TSF，不应该需要改这里的任何一行」（`W1A-006`；`docs/design/architecture.md:20`，`W1A-102`）。workspace 11 个库 crate + 6 个壳/工具包（`W1A-151`）。
2. **数据流**：按键 → `Composition`（缓冲/光标）→ `parser` 切分（每位置最多 8 种切分，`W1A-030`）→ 词库逐音节前缀二分收窄（`W1A-038`）→ `ranking` 排序键预计算（`W1A-028`）→ 整句 Viterbi + 束搜索（束宽 8，`W1A-163`；格子候选 6/20，`W1A-022`）→ `annotate` 补译文 → `commit` 上屏并喂 Learner。`query()` 明确**不带译文**（`engine/query/mod.rs:22`，`W1A-078`），译文由 `annotate` 后补。
3. **`.qj` 容器 = 内存布局即文件布局**（`docs/design/architecture.md:192`，`W1A-104`）：头 32 字节（魔数 `QINGJIAN`、`FORMAT_VERSION = 1`，`W1A-043/044`）+ 分节表 + 8 字节对齐正文（`W1A-042`）；打开只 mmap 与边界校验（`W1A-037/048`），启动 0.9 s → 50 ms（`W1A-105`）。大端机器编译期拒绝（`W1A-041`）。
4. **查词的代价模型被写死**：代价与匹配到的音节组合数成正比、与首音节下键数无关（`W1A-038`）；区间收窄到 48 条内改线性比对（`W1A-034/039`）；词目 12 字节、键索引 16 字节定长无填充（`W1A-035/036`）。
5. **整句 = bigram 词图 + 个人 n-gram 插值**：静态模型 λ=0.8 插值（`W1A-052/054`）；个人 bigram+trigram 与静态模型按 μ=c/(c+8) 插值、trigram 绝对折扣 0.75、总量 20 万条超限减半（`W1A-019/020/021`）；格子查词结果进 `SpanCache`（上限 8192，`W1A-023`），因为「敲键是增量的」（`W1A-024`）。
6. **神经重排是可选注入层，且只改名次不写回分数**：`rescore_paths` 用「路径分 + λ·(神经分 − 静态二元分)」排序（`engine/rescoring/mod.rs:98`，`W1A-017`），注释写明 P2C 分被拼音条件住、写回会污染跨读法比较（`W1A-016`，真实故障 `W1A-131/132`）；λ 缺省 0.5、前文 64 字（`W1A-012/014`）。
7. **热路径性能有公开读数与统一口径**：预算「每一键 10 ms」（`W1A-116`），数字口径 = release / 89 万条词库 / 300 万对 bigram / M 系列 Mac（`W1A-117`）；RSS 480→148 MB（`W1A-118`）、查词 20→0.2 ms（`W1A-119`）、纠错过滤 39→0.17 ms（`W1A-120`）、格子缓存 12→1 ms（`W1A-121`）、首键 9–10→4.5–5.6 ms（`W1A-122`）、启动 930→50 ms（`W1A-124`）。
8. **三个平台是同构的两种进程模型**：macOS 同进程 IMK 单例；Windows/Linux 是独立 Server 进程 + 薄壳（TSF DLL / Fcitx5 插件），协议帧 = 4 字节长度前缀 + JSON（`W1A-069`），协议版本 7（`W1A-067`），Linux 复用同一套（`W1A-148`）。
9. **隐私与供应链是设计项不是补丁**：TSF 侧查 `GUID_COMPARTMENT_KEYBOARD_DISABLED` 整键放行（`W1A-089`）；密钥文件 0600 原子写（`W1A-032`）；日志必经掩码写入器（`W1A-071/085`）；更新索引用内置 ed25519 公钥 `verify_strict` 验签（`W1A-081/082`）；没配签名密钥的发版门禁直接失败（`W1A-097`）。
10. **工程实践成体系**：CI 三 job（Linux 全量 / macOS 壳 / Windows 三件套，均 `--locked` + `-D warnings`，`W1A-093`）、actions 钉 commit（`W1A-092`）、每周 cargo audit（`W1A-096`）、pre-commit 拦装饰性注释（`W1A-098`）、发版门禁四条（`W1A-141`）；模型假设「没有基线不立项、准确率赢了但延迟不达标也不立项」（`W1A-170/172`）。

## 可吸收 / 不可吸收（对「候选窗/自绘渲染器 + 高帧率 UI」这个目标）

| 项 | 锚 | 判定 |
| --- | --- | --- |
| 「内存布局即文件布局」+ mmap 零拷贝容器 | `W1A-104/043/044` | 吸收：主题/字体清单/字形缓存索引都可用同一模式，免反序列化 |
| 定长结构体（12/16 字节、无填充、zerocopy 派生） | `W1A-035/036` | 吸收：命中区表、布局结果表直接落盘/共享 |
| 打开时一次校验、之后 unchecked（记 SAFETY 前置） | `W1A-037/050` | 有界吸收：前提是「文件只整体替换」；本仓渲染器数据文件同样满足 |
| 增量交互优先于单次优化（SpanCache 模式：键=输入模式、失效=数据变） | `W1A-024/023` | 吸收：字形栅格缓存、布局缓存同理；关键是列清「结果依赖什么」 |
| 排序键预计算 + `select_nth` 预选 + 去重截断 | `W1A-028` | 吸收：候选/字形/绘制项排序都适用 |
| 后端分档精度（Metal f16 / CPU f32）与「结果一致」验收 | `W1A-059` | 吸收（若有推理）：分档要配一致性判据 |
| 后置打分只改名次、不写回分数 | `W1A-016/017` | 吸收：任何「停顿后重排」功能都该照此隔离条件化分 |
| 异步 worker「只算最新任务」+ 序号作废 | `W1A-018/061` | 吸收：慢任务合并策略 |
| 位图输出契约（一帧 → 预乘 RGBA + 命中区，壳只贴图） | `W1A-084/113` | 吸收（这正是 A0 交付主体）：渲染器与壳边界收敛到两个动作 |
| 保留系统绘制退路 + 明确删除条件 | `W1A-072` | 吸收：候选窗切换期灰度手段 |
| CPU 栅格而非 GPU（像素量小） | `W1A-109` | 有界吸收：仅对候选窗尺寸成立；全屏/高帧率合成不适用 |
| 原生观感三补丁（opsz 光学字号、trak 字距、text_gamma） | `W1A-156/157/158`（+`W1A-112`） | 吸收：与系统并排对齐的必做项 |
| 中文语言学资产（拼音音节表、双拼七套、注音、五笔码表、CEFR/JLPT 等级表） | `W1A-143` | 不吸收：与本仓渲染器目标无关（本仓只做显示面） |
| 云联想 / LLM 提示词工程（云端词校验、问字、翻译选区） | `W1A-061/063` | 不吸收：本仓不联网、无 AI 功能 |
| 学习数据 TSV 落盘与减半降级 | `W1A-074/076/077` | 有界吸收：「原子写 + 坏行跳过 + 读失败不覆盖」这套纪律与数据无关，值得照搬 |

## 1. 架构全景（模块地图，逐文件职责）

### 1.1 包结构

- workspace（`Cargo.toml:1-16`）：库 `qingjian-{core,dictionary,translate,learning,platform,predict,lm,format,neural,render,update}`；
  壳/工具 `apps/{cli,linux/server,macos,windows/{server,tsf,settings}}`、`tools/{dict-convert,gloss-gen,release-sign}`（`W1A-005` 记 workspace lint 只有 clippy all=warn）。
- 依赖方向（`docs/design/architecture.md:138-150`，`W1A-152`）：dictionary ← core ← {translate, learning, predict, lm, neural} ← platform ← apps。
  `learning` 另依赖 `translate` 的 LevelTable 做词汇按级汇总。翻译/学习/联想/语言模型/神经重排全部经 trait 注入（`Translator`/`Learner`/`Predictor`/`LanguageModel`/`SentenceScorer`，缺省空实现）。
- 外壳组装示例（`apps/windows/server/src/main.rs`）：读配置 → `assembly::assemble` 装配 Engine → `set_*` 推配置 → `Router` → 管道 serve；正式词库装配失败回落样例词库（`main.rs:71-78`，`W1A-165`）。

### 1.2 逐 crate 职责（各 `lib.rs` 的一句话）

- `qingjian-core`：引擎（`lib.rs:1-49`）。20 个模块：composition/parser/correction/candidate/ranking/shortcut/english/sentence/emoji/fuzzy/shuangpin/zhuyin/engine/storage 等。
- `qingjian-dictionary`：词库（TSV 或 `.qj` mmap）+ 辅码表 `AuxCodeTable` + 形码 `CodeTable` + 英文 `WordList`（`lib.rs:1-36`）。
- `qingjian-format`：`.qj` 容器（`Container`/`Writer`/`Table`/`Text`/`hash`/`Metadata`）（`lib.rs:1-39`）。
- `qingjian-lm`：bigram 语言模型 `BigramModel`，CSR + 文件哈希索引（`lib.rs:1-19`）。
- `qingjian-neural`：candle 字级 Transformer 推理，`CharScorer`（含章·知微）与 `P2cScorer`（含章·通变），模型 `.qjm` 单文件（`lib.rs:1-25`）。
- `qingjian-predict`：云联想 `CloudPredictor` + 释义兜底 `CloudGlossFiller`（独立线程、攒批）（`lib.rs:1-22`）。
- `qingjian-learning`：`FrequencyLearner`/`InputLog`/`UsageStats`/`VocabularyBook`（`lib.rs:1-17`，`W1A-074`）。
- `qingjian-translate`：`Glossary`/`LevelTable`/`PersonalGlossary`/`LayeredTranslator`（个人表优先叠加）。
- `qingjian-platform`：`Config`（TOML，toml_edit 原地改键保留注释，`W1A-073`）+ Windows IPC 协议类型 + 日志掩码 + dirs（`lib.rs:1-24`，`W1A-065`）。
- `qingjian-update`：读官网 `releases.json`、验 ed25519 签名、按平台/渠道挑版本，只提示不安装（`lib.rs:1-19`）。
- `qingjian-render`（不在本任务范围）：候选窗「一帧 + 主题 → 位图」；本报告只引用它与壳的接口：macOS `candidates/bitmap/mod.rs:1`（`W1A-084`）、Windows `ui/painter/mod.rs:1`（`W1A-088`）、配置开关 `CandidateRenderer`（`W1A-072`）。

### 1.3 会话 API 与数据流细节

`Engine` 是唯一门面（`engine/mod.rs:82-290`，字段带注释）：`set_input/push/backspace` 喂拼音 → `query()` 出 `Query{segmentations, candidates, timings}` → `annotate(&mut CandidateList)` 补译文 → `commit(&Candidate)` 上屏并喂 Learner（词频、词转移、自动造词）。`Engine::flush_learning()` 定时落盘且不作废格子缓存；`break_chain()` 断学习链（壳停用时）。
多会话：`EngineSession` 保存可挂起状态，`swap_session` 交换组句/历史/学习链并清查询与异步缓存（`engine/session.rs:57-86`，`W1A-080`）。

### 1.4 验证工具链（apps/cli）

CLI 是 Core 的第一个壳、不依赖任何平台 API（`W1A-166/167`）：查询 / 逐键计时（`--typing`）/ 输入日志回放（`--replay`）/ 整句评测（`--eval-text`，可 `--eval-save` 冻结句子集）/ 常数扫描（`--tune`）/ 冷启动字词实验（`--eval-cold`）。Engine 的组装集中在一处，注释声明「这是 Core 之外唯一知道具体 Translator / Learner 类型的地方」（`W1A-168`）；回放按日志每条的方案切引擎，因此 CLI 自己留一份码表（`W1A-169`）。评测协议见 `docs/plan/model-eval.md`：先写指标与基线再改代码、改完在同一份**冻结**日志上比（`W1A-170/171`），并把「日志里的正确答案多是被测系统自己的输出」这类尺子偏差逐条留档（`W1A-173`），连回放器本身的口径修正（raw 也要走 `take_raw`，否则英文首选低 10 个点）都记在案（`W1A-174`）。

## 2. 关键机制

### 2.1 解析与切分（parser）

按位置 DP，每位置保留最优 8 种前缀切分（`W1A-030`），token 可为完整音节、声母（简拼）或末尾未完成前缀；排序键：音节少 > 不完整少 > 前面音节长。`is_fully_segmentable` 是无分配一维可达性 DP，专供纠错变体预过滤（`W1A-029`，性能收益见 3 节）。词库键规范化统一在 `canonical_syllable`（`v` 表 ü，`W1A-040`）。

### 2.2 词库查询（dictionary）

四段数据（词文本 arena、键 arena、KeyIndex 表、Slot 表）按字节序排好；查询逐音节位置二分收窄：「以某前缀开头的键」是连续区间，完整音节直接二分到 `前缀+音节+空格`，简拼位置按区间内实际音节跳块，区间 ≤48 条改线性（`W1A-034/038/039`）。查询接口 `lookup_pattern`（≥ 模式长）与 `lookup_exact`（正好等长）共用一套收窄实现；多写法版（模糊音/敲错边）逐位置相加不相乘，调用方保证同位置写法互不覆盖。

### 2.3 排序模型（ranking）

规则排序键（音节数一致 > 覆盖字母多 > 非末尾简拼少 > 末音节完整 > 同输入串选择次数 > 上下文 log P(词|前词) + 加分 − 罚分 > 原音节/词长短 > 字典序）（`ranking/mod.rs:1-15`，其中「非末尾简拼少」见 `W1A-175`）。选择次数加分 = 0.5·ln(1+min(次数,20))（`W1A-027`）；模糊音命中扣 ln2（`W1A-176`）。排序键先算好再排、远超上限时 `select_nth_unstable_by` 预选（`W1A-028`），候选上限 500（`W1A-010`）。

### 2.4 整句转换（sentence）与个人模型

词图：每个格子放正好覆盖那几个音节的词，默认留词频前 6、含简拼位置留 20（`W1A-022`）；Viterbi + 束宽 8（`W1A-163`）；词库无单字的音节按 −30 log 概率兜底（`W1A-025`）。
打分 = 静态 bigram（λ=0.8 插值，`W1A-052/054`，CSR 布局 `W1A-053`）与个人 n-gram 插值：μ=c/(c+8) 封顶 0.5（`W1A-019`）、trigram 绝对折扣 0.75（`W1A-020`）、在线计数上限 20 万条超限减半（`W1A-021`）。格子候选经 `SpanCache` 跨键复用（上限 8192、学习数据一变就清，`W1A-023/024/026`）。
纠错两路：整段一处编辑的变体按噪声信道挑（改完得分扣编辑代价仍高于原样才纠）；词图内敲错边（音节级变体表）管「音节都合法但整句不通」。接受的 (敲的, 要的) 记个人敲错表并给后续代价打折。

### 2.5 神经重排（neural + engine/rescoring）

- 注入面：`SentenceScorer` trait 两个实现——`CharScorer` 用「前文 + 整句」按字累加 log 概率（用 context），`P2cScorer` 用「按键 → 汉字」的 P2C 条件（不看前文）；产品端整句重排用 P2C（`W1A-145`，冻结集 8322 句 42.49% vs 41.80%、配对 McNemar p=0.003）。
- 架构：`convert_paths` 出 Viterbi 前 6 条路径（`W1A-011`）→ `rescore_paths` 按「路径分 + λ·(神经分 − 静态分)」重排（`W1A-017`），**不写回 `Conversion::score`**（`W1A-016`；理由与故障见 `W1A-131/132`）。
- 缓存与异步：`NeuralCache` 键 = (前文, 按键) + 文本，两条件任一变整张作废（`W1A-146`）；`RescoreWorker` 后台线程、排队只算最新任务（`W1A-018`）；同步打分器当场补分（CLI 评测），异步的等壳停键后 `request_rescoring` / `poll_rescoring`。
- 推理细节：前文 K/V 缓存（切点=去掉最后一个 token，`W1A-056/058`）；精度 Metal f16 / CPU f32（`W1A-059`）；模型单文件 `.qjm` 三节装 config/vocab/safetensors（`W1A-060`）。
- 生成路线（实验）：词图读不通整段时改问 `SentenceScorer::generate`（P2C 束宽 5、上限 32 字），生成的整句插在词图路径前；生成与重排互补（只有生成对 16.5%、只有重排对 6.9%，`W1A-133`），但生成是 O(生成字数) 次串行前向、延迟结构不同（`W1A-134/135/136/137`）。

### 2.6 `.qj` 容器与零拷贝（format）

头 32 字节（`QINGJIAN` + version + kind + section_count + 16 预留，`W1A-043`）+ 分节表（tag/offset/length，正文 8 字节对齐，`W1A-042`）+ 正文；第一节固定 `META`（TOML：名称/许可/署名/条数），第三方数据许可随文件走。`Container::open` 校验魔数/版本/种类/分节边界，`Table<T>`（Owned/Mapped）与 `Text`（Owned/Mapped）两种视图对查询代码同形（`W1A-050`）。文件内哈希索引 = 开放寻址 + FNV-1a 64 + fmix64 终混，容量 2 的幂、装载率 ≤0.5，输出被 golden 测试钉死（`W1A-045/046/047`）。数值小端、原生对齐，大端机器 `compile_error!`（`W1A-041`）；数据文件只整体替换（写临时再改名，`W1A-048`）。

### 2.7 平台抽象面（与 qingjian-render 的对接）

- `qingjian-platform` 只承载：`Config`（分节 general/shortcut/fuzzy/dictionaries/apps/predict/…，热加载；`set_value` 用 toml_edit 保留注释，`W1A-073`）、IPC 协议类型（serde；`W1A-065`）、日志与掩码、`dirs`/`resources`。
- 渲染器接口（跨 crate）：壳侧持 `Renderer` + `FontLibrary`，输入 `qingjian_render::Frame` + `Layout` + 主题，输出位图；macOS 在 `drawRect:` 贴 CGImage（`W1A-084`），Windows 在 `layered::present` 把预乘 RGBA 转 BGRA 后 `UpdateLayeredWindow`（`W1A-113`）；两窗口共用一份渲染器与字形缓存（`W1A-088`）。Core 侧提供展示规则（分页/矩阵视口 `Grid`、`column_ems`），壳只转发方向键（`W1A-155`）。
- 渲染器资产约定：字体不扫系统、按平台清单加载（`W1A-003`）；cosmic-text 用钉 rev 的 fork 补 `opsz`（`W1A-004`）。

### 2.8 TSF / 输入法系统集成要点（Windows）

- **进程模型**：DLL 加载进每个应用进程，Engine 必须在独立 Server 进程（`qingjian-platform::protocol` 文档头，`W1A-066`）。DLL 只做 IPC + 内联 preedit + 量光标矩形；候选窗与状态条在 Server 自绘（UI 线程 + 分层窗 + `PostThreadMessageW` marshal）。
- **协议纪律**：版本 7、一个版本周期只升一次；给线上枚举加变体会让老 DLL 整帧失败、按键放行（症状「突然只出英文」，`W1A-066`）；`SESSION_OPENED_SINCE = 6` 用能力起始版本而不是当前版本（`W1A-068`）。
- **高 z-band 覆盖**：uiAccess manifest + 代码签名 + 装 Program Files 三者齐备才盖得住商店/任务栏搜索（`W1A-106`）；没签名的 exe（含测试二进制）起不来（os error 740），CI/pre-push 用 `QINGJIAN_UIACCESS=0` 关掉（`W1A-094/099`）。
- **隐私**：组句前查 `GUID_COMPARTMENT_KEYBOARD_DISABLED`（密码框、空上下文）整键放行（`W1A-089`）；私密时 `Engine::set_private` 给学习器/日志套 `Muted*`、停联想与翻译、并 `discard_input()` 无痕清理（`W1A-080`）。
- **沙盒细节**：日志目录用 icacls 给 AppContainer 授权（SID 常量，`W1A-086/087`）；DLL 静态链接 CRT（`crt-static`，`W1A-149`；真机故障 `W1A-115`）。
- **按键分流**：单击切换键判定在 TSF 击键 sink（`OnTestKeyDown/Up`），因为线程级钩子看不到被 TSF 吃掉的键；中英模式全局一份存在 Server（架构文档 `architecture.md:411-431`，`W1A-154`）。

## 3. 性能手段与公开读数（数字带锚：版本/机器/口径）

预算与口径（`docs/notes/performance.md`）：每键候选 ≤10 ms（敲键间隔约 100 ms；>30 ms 可感），启动尽量 1 s（`W1A-116`）；所有数字 = release 构建 + 89 万条开发词库 + 300 万对 bigram + M 系列 Mac（`W1A-117`）。

| 轮次 | 手段 | 读数（改前 → 改后） | 锚 |
| --- | --- | --- | --- |
| 1 内存 | arena + 偏移（词文本/键各一 arena，索引定长） | RSS 480 MB → 148 MB | `W1A-118` |
| 2 查词 | 逐音节前缀二分收窄 + 前缀记忆化 + 排序键预计算 | 长输入 20→0.2 ms；`zhgdoima` 70→1–3 ms；单字母 30→3–4 ms | `W1A-119` |
| 3 逐键 | 纠错变体先用无分配 `is_fully_segmentable` 过滤 | parse 39→0.17 ms（16 键） | `W1A-120` |
| 3 逐键 | 词图格子缓存 `SpanCache`（增量复用） | rank 12→1 ms（`sss…` 15 键），15 键平均 2.5 ms | `W1A-121` |
| 3 逐键 | 四项常数优化（foldhash、u128 预选键、跳过模糊比对、reserve） | 首键 9–10→4.5–5.6 ms（模糊全开 8 ms） | `W1A-122`（+`W1A-001/075`） |
| 4 启动 | `.qj` 容器 mmap（内存布局=文件布局） | 词库 240→12 ms；LM 620→9 ms；Engine 就绪 930→50 ms；体积反而增大 | `W1A-124`（+`W1A-104/105`） |
| 5 启动 | 释义表进容器（TSV 24 万词） | 加载 90→8 ms；启动 170→47 ms（冷 116 ms 缺页） | `W1A-125` |
| 6 词图 | 敲错边（每完整音节约 10+ 变体） | 28 键最慢 3.5→4.2 ms、平均 0.46→0.64 ms；仍在 10 ms 内 | `W1A-159` |
| 7 句末英文 | `split_english_tail` 每次查询多遍后缀扫词表 | `--typing woxiangxuehaorust` 最慢 5.0 ms（改前 4.65），平均 0.8 ms | `W1A-160` |
| 8 神经 | 前文 KV 缓存 + 分数缓存 + 停键异步 | Metal 64 字前文×8 条 133→28 ms；评测每句 69→27 ms；8322 句平均 61→24 ms、最慢 781→101 ms | `W1A-161`（+`W1A-056/058`） |
| — | P2C 延迟门槛表 | 整句重解码 Metal 68 ms（<100 ✔）/ CPU 141 ms（勉强）；每键增量 2.1/6.7 ms（<10 ✔）；加载 66 ms | `W1A-138` |

渲染器侧读数（候选窗）：首帧 2 倍屏 266×300 pt release 1 ms（首帧 6 ms 含字形缓存冷启动）、壳内字体库 3.8 ms（`W1A-111`）；字体按需加载 5 文件 67 面 1.5–4 ms、峰值 RSS 16 MB（`W1A-110`）；Windows 真机验收 2026-09-15（`W1A-114`）。
复算与教训：读数的「翻好/翻坏」逐条对账（回放尺子有偏：整句的「正确答案」是基线自己的输出，`W1A-129/130/131/162`）；动态增量是最大加速来源（`W1A-121/127`）；先打点再改（两次猜错热点，`W1A-126`）。

## 4. 坑与反例（负面留档）

1. **跨读法比较被条件化分数污染**（`W1A-131/132`）：P2C 分写回 `score` 后，`woxiangxuexirust` 被「纠成」我想学习如斯（−16.90 vs −13.87）；贝叶斯展开说明末项只在同一串按键下是常数。修法：重排只改名次（`W1A-016`）。
2. **线上协议枚举扩张**（`W1A-066`）：给 `Candidate` 加 `kind` 变体，老 DLL 整帧失败、按键放行，症状「输入法突然只出英文」。
3. **零拷贝上的「每次校验」**（`W1A-123`）：第一版 `Text::deref` 每次 `from_utf8` 扫 30 MB arena，CLI 卡死；改为打开时校验一次。
4. **IMK 类注册顺序**（`W1A-083`）：`define_class!` 类未先注册就建 IMKServer → 静默退回基类、按键全部透传。
5. **动态 CRT 进沙盒宿主**（`W1A-115/149`）：TSF DLL 动态链 `vcruntime140.dll`，AppContainer 读不到时整个 DLL 加载失败、系统切回上一个输入法。
6. **缓存负结果**（`W1A-063`）：联想的空回复曾被缓存，导致同一问题永远无答案（`?mumumu`）。
7. **UI 覆盖与权限链**（`W1A-106`）：候选窗被高 z-band 宿主盖住 → 三条件齐备才行；uiAccess exe 不能 `CreateProcess` 拉起（740）。
8. **文档纪律的结构性缺口**（`W1A-139`）：`docs/notes/release.md` 文件头残留一段 workflow 表格片段并与标题粘行——文档也会有「脏数据」。
9. **欠账自曝**（`W1A-142`）：发版文档写明产品数据当时是滚动覆盖、同一源码 tag 重跑可能拿到不同数据，且安装包内容验证未做——直接削弱「同一 commit 可复算」。
10. **个人 n-gram 的近似**（`W1A-164`）：三元上下文的「前二词」取最优前驱回指，不扩状态——是明确的近似而非精确解。

## 5. 未验证项（缺什么证据）

1. **无外部文档锚**：本批 176 条全为仓库内 `source`（含 in-repo 设计/笔记文档的行锚），没有抓取外部规范/论文；架构文档里对微软文档、IMK、Apple CoreText 行为的引用（如 `W1A-089` 的「微软文档明说密码框应禁用文本服务」）**未经我复核原文**（本机 WebFetch 不可用，只对 2 个外链做了试抓，其中一个需登录/JS，遂全部放弃）。
2. **未跑构建/测试**：所有性能数字均为上游文档转录，未复算；`cargo test`/clippy 状态未验证（任务禁止跑构建）。非 Rust 侧的 `apps/windows/settings`（WinUI 3）、`apps/linux/fcitx5`（C++）只看了入口与文档，未读实现。
3. **未读面**：`crates/qingjian-core/src/engine/{commit,learning,marked,statistics,vocabulary,prediction}` 只读了 crate 文档与设计文档转述；`apps/macos/src/{imk,host,preferences,candidates/view,menubar}` 的实现细节、`tools/{dict-convert,gloss-gen,corpus}` 的算法、`apps/windows/{settings,installer}`、`apps/linux/fcitx5`（C++）全部未读（读文件预算 ≤120，已用约 60）。
4. **数字的适用面**：10 ms/键、480→148 MB 等全部来自「89 万条开发词库 + M 系列 Mac + release」这一单一配置（`W1A-117` 自陈）；换词库规模（现产品 8.7 万条，`W1A-153`）或换机器后**未重测**，不可外推。
5. **渲染器主体（A0）不在本报告**：`crates/qingjian-render/**` 只引用接口，未做行级分析；主题 TOML 的字段集与像素级对齐数字见 A0。
6. **时间点**：以上均为 commit `c08ae57` 的快照；`docs/notes/performance.md` 里部分数字标注 2026-09-05/09-08，与仓库当前代码（如 P2C 已取代字级模型做重排，`W1A-145`）之间存在**文档滞后**，本报告按「文档写下的当时口径」引用。
