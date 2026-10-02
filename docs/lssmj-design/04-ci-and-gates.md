# 04 · CI 流程与门禁（本仓现役 + 引擎仓目标态）

> 口径：本文件描述 **LSSMJ 的 CI 设计**。分两层：**A. 本仓现在就能跑的门**（文档/账本仓，已在用）；
> **B. 引擎代码仓的目标态**（crate 化之后的门集与流水线——把 `../reports/12-engineering-method-crossref.md`
> 的 P1–P12 与三仓既有实践（qingjian-gates / unified-rx / BSHSQ）落成规格）。
> 原则继承用户既有纪律：**绿=SKIP 不算绿**；**门结果必须在提交前被读到**；**先验红（金丝雀）**；
> **棘轮只准减，`--write-baseline` 需理由登记**；**PR 默认一功能一 PR，不自行合并**。

## A. 本仓（文档 + 账本）现役流水线

### A.1 门清单（全部已有实现或已规格化）

| # | 门 | 命令 | 判据 | 失败策略 |
| --- | --- | --- | --- | --- |
| A0 | 自证金丝雀 | `python tools/ledger.py selftest` | 好条目绿 + 坏条目**必须死在"引文对不上"** | 红（门本身坏了） |
| A1 | 账本门 | `python tools/ledger.py verify --min 1000` | 0 拒绝 + 总数 ≥1000 | 红 |
| A2 | 统计新鲜度 | `report` 后 `git diff --exit-code docs/analysis/ledger-stats.md` | 生成物与提交版一致（不许手改统计） | 红 |
| A3 | 引用门（规格化） | `python tools/ref_gate.py`（待实现，见 §B.5） | 文档里出现的 `W#X-nnn` 引用必须存在于账本 | 红 |
| A4 | 结构门（规格化） | `python tools/doc_gate.py`（待实现） | 每篇报告头部 60 行内含 TL;DR≥3 条锚 + 未验证节；无占位符（TODO/待补 字样白名单化） | 红 |
| A5 | 链接抽查（可选，夜间） | `python tools/link_probe.py --sample 5%` | 抽 5% 锚 URL 当日 200（失败仅告警，不阻断——外站波动不进硬门） | 黄（记录） |

### A.2 流水线（GitHub Actions 草图）

```yaml
name: lssmj-gates
on: [push, pull_request]
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with: { python-version: '3.12' }
      - run: python tools/ledger.py selftest              # A0 先验红
      - run: python tools/ledger.py verify --min 1000     # A1
      - run: python tools/ledger.py report --out docs/analysis/ledger-stats.md
      - run: git diff --exit-code docs/analysis/ledger-stats.md   # A2
      - run: python tools/ref_gate.py                     # A3（实现后启用）
      - run: python tools/doc_gate.py                     # A4（实现后启用）
```

- **提交纪律**：门在**提交之前**跑完并被读到（三仓血泪：门禁与提交压成一条命令 ⇒ 没读门就推了）。
- **代理并发**：多代理并行改本仓时沿用 qingjian-gates 的**认领协议**（`.agents/claims/<id>.json`，
  scope 交集判红；`AGENT-BRIEF.md` 已有的文件边界规则升级为机器门——见 §B.6）。

## B. 引擎代码仓（目标态）

### B.1 门的家族（按 P1–P12 落地）

| 家族 | 门 | 形式 | 来源 |
| --- | --- | --- | --- |
| 证据/文档 | 账本门（A1）、引用门（A3） | 本仓现成，迁移 | 本仓 |
| 结构质量 | god 门（文件/函数/类型成员）、dupe 门 | 棘轮 + 硬阈两档；`--write-baseline` 需披露 | reports/12 P1/P2、god-object-gate 经验 |
| 测量 | perf 门（P95 棘轮）、金丝雀（先验红）、冻结哈希（默认档） | 窗口均值 + 交错 A/B；禁在挂测态采数 | reports/09、reports/16（抓帧≠计时/HAGS 口径） |
| 一致性 | 宽度对表（C2）、逐位金丝雀（C4）、假阴性金丝雀（C7） | 与原生/期望对拍 | 判据 C1–C10 |
| 供应链 | deny/license/锁步 | cargo-deny + 移动标签钉死 | BSHSQ 经验 |
| 并发 | agent 认领门（scope 判红） | 认领文件即状态 | qingjian-gates A1–A5 |

### B.2 阶段划分（流水线分层，对应"快/全"两档）

```text
L0 快档（pre-commit，<3s）：fmt + selftest + 账本增量核 + 引用门
L1 全档（PR 必跑，<5min）：L0 + 全量账本 + 结构棘轮 + 单测 + 宽度对表(小语料)
L2 重档（main 夜间）：L1 + perf 棘轮（独占机）+ 逐位金丝雀 + 平台矩阵 + deny/审计
L3 发布档（tag）：L2 + 冻结哈希登记 + 金样全量 + 变更披露清单
```

### B.3 perf 门的硬规矩（防"数字骗门"）

1. 读数四件套：提交/版本、机器、场景、样本范围（+ HAGS 状态，`w7b`）。
2. 禁在挂测态采集（抓帧器/调试器/回放：`w7b` W7B-001/069——挂载态读数与稳态不可混）。
3. 棘轮对**P95**设上限；`--write-baseline` 必须附"为什么可以涨"一行。
4. 先拿金丝雀证明门会红（把场景放大 10× 或注入退化）。
5. 性能类门机器级独占（`perf_lock` 先例）。

### B.4 平台矩阵

| 平台 | 层 | 备注 |
| --- | --- | --- |
| Windows x64 | CPU 路径必测；GPU 档（wgpu/Vulkan·D3D12） | 分层窗贴图回归（`w5a`） |
| macOS arm64 | CPU 路径必测；Metal 后端 | 系统阴影/Retina 口径 |
| Linux | 编译 + 无窗冒烟（显示面归 Fcitx/GTK，`rendering.md:33`） | 不承诺显示面 |

### B.5 待实现的三个小门（本仓已规格化，工程 10 行级）

- `ref_gate.py`：扫描 `docs/**/*.md` 的 `\bW\d[A-Z]?-\d{3}\b` 引用 → 必须命中账本；反向孤儿条目列黄。
- `doc_gate.py`：报告头部 60 行含 TL;DR 锚；FIXME/TODO/待补 需带 `（计划：T-xx）` 才放行。
- `link_probe.py`：抽样 URL 200 检查（夜间告警档，不进硬门）。

### B.6 多代理协作门（继承 + 扩展）

- 认领：`.agents/claims/<agent-id>.json`（scope=glob 列表，expires）；**未认领动源码=红**；scope 交集=红。
- 证据边界：代理提交必须带自查输出（`verify --file` 段落）；无自查=退回。
- 合并：PR 一功能一 PR；评审主结论=能力边界表（逐条在代码里验），"质量高"只能作辅证（用户既有口径）。

## C. 现在就要做的（本仓侧，任务列表见 `03-task-backlog.md`）

- [ ] T-CI-01 实现 `tools/ref_gate.py` + 自测金丝雀（造一条悬空引用必须红）
- [ ] T-CI-02 实现 `tools/doc_gate.py`（含 TODO 白名单规则）
- [ ] T-CI-03 加 GitHub Actions（A.2 草图，含 A2 新鲜度）
- [ ] T-CI-04 夜间链接抽查（A5，告警档）
- [ ] T-CI-05 `.agents/claims/` 协议文件化（AGENT-BRIEF 升级引用）
