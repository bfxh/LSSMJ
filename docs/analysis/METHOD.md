# 分析工程协议（METHOD）

> 目标：对"渲染引擎 / UI 框架 / 文本栈 / 论文"做**可审计**的大规模分析——条目 ≥1000，每条带证据锚，
> 机器校验；不造假、不脑补、不隐账（被拒条目在统计报表里公开计数）。

## 1. 什么算"一条分析"

一条 = **可指出证据的最小观察单元**。粒度参考：
- `source`：一个具体实现事实（某函数做什么、某常量为何、某算法怎么走），必须能给出 `file:line` + 逐字引文；
- `doc` / `paper`：某文档/论文里一个具体主张（含数字），必须给出 URL + 引文；
- 反例也算：一个坑、一个被否证的设计、一处性能回归留档，同样计入。

**不是一条**：读了某个 README 的总体印象（无锚）、"某某很快"（无数字无出处）、重复他人已有条目（重复 id 判红）。

## 2. 深度阶梯（`depth` 字段）

| depth | 含义 | 锚的形式 | 可否单独支撑设计决策 |
| --- | --- | --- | --- |
| `source` | 逐字读过源码 | `<路径>:<行>` + 逐字 `quote` | ✅ |
| `doc` | 官方文档/设计文档/规范 | URL + 引文 | ✅ |
| `paper` | 论文/正式技术文章（含权威长文） | URL + 引文 | ✅（注明版本/日期） |
| `web` | 二手旁证（博客、论坛、转述） | URL + 引文 | ❌ 只能做旁证，不能单独支撑决策 |

红线（源自工作区表述纪律）：**不把过程变东西**（数字带锚：版本/提交/日期/口径）、**不把局部变总体**
（样本范围写清）、**不把解释变终点**（每条目尽量给 `lesson`：对本项目的含义或下一步）。

## 3. 证据锚纪律（机器校验）

- `quote` 必须是对应行（±5 行容差）的**逐字子串**——`tools/ledger.py` 会把引文拿去源文件里找，
  找不到即该行判红。找不到就说明引文不实或行号漂移，**修好再交，不许放宽**。
- 编码：文件先按 UTF-8（含 BOM）解码，**失败自动回退 GBK/GB2312**（GN SDK 的整套头文件即 GBK；
  回退只改"怎么读文本"，不改变逐字匹配强度）。遇到双重编码问题（GBK 文件里嵌 UTF-8）时，
  先落一份 UTF-8 快照到 `scratch/` 再锚快照，并在报告里注明转换。
- 锚路径用绝对路径、正斜杠（如 `D:/KF/LSSMJ/scratch/src/bevy/...`）。
- 上游每次分析前 `git rev-parse HEAD` 记提交号，写进 `targets-lock.json` 与报告头部——
  **否证/结论都是时间戳**，锚到具体提交才可复算。

## 4. 目标与复算

- 目标总清单：`targets.md`（每个目标：类型、抓取方式、重点文件、产出文件、行数目标）。
- `targets-lock.json`：`[{target, url, commit, local_path, fetched_at, method}]`，机器可读。
- 浅克隆规范（省磁盘，D: 余量有限）：
  ```bash
  git clone --depth 1 --filter=blob:none https://github.com/<o>/<r> scratch/src/<slug>
  # 大仓加 --sparse 再 set 具体目录；超大仓（chromium/unreal 主体）不克隆，走 docs/ 镜像或在线文档
  ```
- 网络注意：本机 WebFetch（Node）证书校验失败；**统一走 `curl` 或 `python urllib`**（已验证可用：
  `curl -sL https://raw.githubusercontent.com/...`，`python -c "import urllib.request; ..."`）。
- GitHub API 走 `gh api`（已登录 bfxh 账号）。

## 5. 波次与文件命名

- 账本文件：`docs/analysis/ledger/w<波次><字母>.jsonl`，条目 id 前缀同文件名（如 `w1a.jsonl` ⇒ `W1A-001`）。
- 报告：`docs/engines/<slug>.md`、`docs/papers/<slug>.md`、`docs/platforms/<slug>.md`、`docs/targets/<slug>.md`。
- 报告头部 60 行内必须给出：TL;DR 10 条（每条带锚）+「可吸收 / 不可吸收」两张清单 + 上游 commit。
- **主张↔账本对应**：报告里的每条技术主张都要能指到账本行；指不到的改写或删掉。

## 6. 反"浑水摸鱼"的四道机器/流程判据

1. `quote` 逐字校验（引文不实 ⇒ 红）；
2. `--min 1000` 总数门 + 各波次行数目标（`targets.md` 里登记）；
3. `selftest` 金丝雀：好条目绿、坏条目红——**先证明门会判红**；
4. 统计报表公开 `rejected` 数与被拒样本（不隐账）。

## 7. 许可与回流

本仓 GPL-3.0-or-later，与青简上游一致；`docs/renderer-qingjian/` 可直接整理为上游 PR（引用片段逐字、带出处）。
