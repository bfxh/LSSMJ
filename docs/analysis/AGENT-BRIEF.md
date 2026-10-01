# 分析代理作业简报（AGENT-BRIEF v1）

> 给每一个执行分析任务的子代理的标准作业书。**先读 METHOD.md，再读 targets.md 里你负责的条目，然后动手。**

## 1. 背景（3 行）

本项目 = ①对渲染引擎/UI 框架/文本栈/论文做证据锚定的大分析（≥1000 条，`tools/ledger.py` 校验）；
②交付主体 = 青简（qingjian，Rust 输入法）自绘渲染器 `crates/qingjian-render` 的完整技术文档。
你的产出 = **报告文件** + **账本条目（JSONL）**，两者都要落盘。

## 2. 硬性纪律（违反即返工）

1. **每条结论必须有证据锚**：`source` = `<路径>:<行>` + 逐字引文；`doc`/`paper` = URL + 引文。
   **不许凭记忆写行号、不许脑补引文。** 写进账本前用 Read 真读过那一行。
2. **引文必须逐字**（大小写敏感，≥10 字符），校验器会拿它去源文件 ±5 行内做子串匹配，对不上判红。
   被拒条目比少交 10 条严重得多——**宁少勿假**。
3. **主张↔账本对应**：报告里每条技术主张都要能指到你的账本行；指不到就删掉该主张。
4. 报告**头部 60 行内**必须给：TL;DR 10 条（每条带锚）+「可吸收 / 不可吸收」两张清单 + 上游 commit。
5. 只读分析：**不执行上游代码、不跑 cargo build/test**（省磁盘与时间）；不写上游目录，只在
   `D:/KF/LSSMJ/scratch/**`（克隆）与 `D:/KF/LSSMJ/docs/**`（产出）里落文件。

## 3. 产出规格

### 3.1 报告（路径由任务指定，如 `docs/engines/egui.md`）

```markdown
# <目标名>（<上游 url> @ <commit 短 SHA>, 抓取 <日期>）

## TL;DR（每条带锚）
1. …（`file.rs:123` "引文片段"）
…（10 条）

## 可吸收 / 不可吸收（对"候选窗/自绘渲染器 + 高帧率 UI"这个目标）
| 项 | 锚 | 判定 |
| --- | --- | --- |
| … | … | 吸收 / 有界吸收 / 不吸收（理由一行） |

## 1. 架构全景（模块地图，逐文件职责）
## 2. 关键机制（渲染/文本/布局/状态，按目标性质取舍）
## 3. 性能手段与公开读数（数字必须带锚：版本/机器/口径）
## 4. 坑与反例（负面留档，同样是条目）
## 5. 未验证项（写明缺什么证据）
```

### 3.2 账本（`docs/analysis/ledger/<wavefile>.jsonl`，一行一条）

```json
{"id":"W1A-001","depth":"source","target":"egui","anchor":"D:/KF/LSSMJ/scratch/src/egui/crates/egui/src/lib.rs:42","quote":"逐字引文 10–120 字符","finding":"这条观察本身（≥20 字符）","lesson":"对本项目的含义（可空）","date":"2026-10-01"}
```

- `depth`：`source`（读过源码并给行锚）/ `doc`（官方文档 URL）/ `paper`（论文/正式长文 URL）/ `web`（二手旁证，**不能单独支撑决策**）。
- 行数目标见任务；**source 行必须 ≥ 目标的 1/3**。论文任务则 `paper` 行为主体。
- 每个报告文件至少对应一条账本行（报告头部 commit 那条）。

### 3.3 自查（交活前必须跑，输出贴进你的总结）

```bash
python D:/KF/LSSMJ/tools/ledger.py verify --file D:/KF/LSSMJ/docs/analysis/ledger/<你的文件>.jsonl
python D:/KF/LSSMJ/tools/ledger.py stats  --file D:/KF/LSSMJ/docs/analysis/ledger/<你的文件>.jsonl
```
被拒条目必须当场修（引文重读、行号重定位）或删除，**不许留红**。

## 4. 抓取规范（本机环境实测）

- WebFetch 工具在本机证书校验失败——**不要用**；统一 `curl` 或 `python urllib`（都已验证可用）。
- GitHub 原始文件：`curl -sL https://raw.githubusercontent.com/<o>/<r>/<ref>/<path>`
- GitHub API：`gh api repos/<o>/<r>/commits/<sha>`（已登录）。
- 克隆（浅、省磁盘；D: 余量 ~50G）：
  ```bash
  git clone --depth 1 --filter=blob:none https://github.com/<o>/<r> D:/KF/LSSMJ/scratch/src/<slug>
  # 大仓加 --sparse 再 `git -C <dir> sparse-checkout set <dirs>`，只取需要的目录
  ```
- **不许克隆**：chromium 主体、unreal 主体、flutter engine 全量——改用 `docs/` 镜像 raw 文件与在线文档。
- 抓完记录：`git -C <dir> rev-parse HEAD`，写进报告头部。
- 反爬/超时：重试 2 次后改用其他镜像（如 `github.com/<o>/<r>/raw/...`），或降级为 `doc` 深度并注明。

## 5. 搜索纪律

- 先用 `rg`（原生，快）定位关键词，再 Read 具体文件；**不要整仓全文读**。
- 关键结构优先看：`lib.rs` / `mod.rs` / `Cargo.toml` / `README` / `docs/` / `tests/`。
- 读文件预算 ≤120 个/任务（除非任务里写明更大）；到 2/3 预算时开始收口写报告。

## 6. 总结返回格式（给主代理，≤15 行）

```text
report: <路径> (N 行)
ledger: <路径> rows=KK (source=.. doc=.. paper=.. web=..) rejected=0
commit: <上游 sha>
top5: 1) … 2) … 3) … 4) … 5) …（每条带你账本里的锚）
未验证: …
```
