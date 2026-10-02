# LSSMJ — 渲染引擎分析工程 + 青简渲染器文档

> 本仓两件事：
> 1. **证据锚定的大分析工程**——开源/闭源渲染引擎、UI 框架、文本栈与论文，条目总数 **≥1000**，
>    由 `tools/ledger.py` 机器校验（每条必须带 `file:line` + 逐字引文，或 paper/doc URL + 引文；无锚即红）。
> 2. **交付文档主体：青简（Qingjian）自绘渲染器 `crates/qingjian-render` 的完整技术文档**（`docs/renderer-qingjian/`）。

## 口径声明（先读这一段）

- 用户点名的 `qingjian-team/qingjian` **不是 Bevy 游戏引擎**，而是 **Rust 写的拼音输入法**（GPL-3.0-or-later，约 2.3k★，
  官网 qingjian.app）。它内部有一个真实的自绘渲染器 `qingjian-render`（crate 自述：
  "自绘渲染器：把候选窗一帧画成位图，各平台只负责贴图"）——**"那个渲染引擎"按此锁定**，本文档集的主体就是它。
- "1000 次分析"的机器口径：**证据锚定分析条目 ≥1000 条**（`verify --min 1000` 判定）。
  条目 = 可指出证据的最小观察单元；分四条深度轨道：`source`（逐字读源码）/ `doc`（官方文档）/
  `paper`（论文/正式技术文章）/ `web`（二手旁证，不得单独支撑设计决策）。论文轨道单列统计。
- 所有 `source` 锚指向 `scratch/src/**` 的浅克隆；`docs/analysis/targets-lock.json` 记录每个目标的
  url + commit + 本地路径（复算入口）。

## 目录结构

```text
docs/
  INDEX.md                 总导航
  analysis/                分析工程本体
    METHOD.md              协议：深度阶梯 / 证据锚纪律 / 波次命名 / 复算方法
    AGENT-BRIEF.md         分析代理作业简报（schema + 报告模板 + 自查命令）
    targets.md             目标总清单（每个目标的抓取方式与重点文件）
    targets-lock.json      url + commit + 本地路径（机器可读）
    ledger/                *.jsonl 账本（一行一条分析）
    ledger-stats.md        由 tools/ledger.py report 生成（含 rejected 数，不隐账）
    notes/                 逐目标原始笔记（可选，比报告更细）
  engines/                 逐引擎分析报告（qingjian / bevy / egui / godot / flutter / ...）
  papers/                  逐论文/文章分析（papers/INDEX.md 是述评）
  platforms/               闭源与平台（Unity / Unreal Slate / Apple / Microsoft / Android）
  targets/                 案例解剖（mineradio-electron / gn-sdk）
  reports/                 综合报告（全景 / 文本 / 布局与响应式 / GPU 技术 / 性能方法论 / ADR）
  renderer-qingjian/       ★ 交付主体：青简自绘渲染器文档（11 篇）
  renderer-gn/             GN SDK（闭源）渲染/UI 逆向文档 + 复算
  lssmj-design/            ★ 自研设计文档（两层：README 显示面 UI 档 + 02-scene-tier 场景档）
  targets/                 案例解剖（mineradio-electron）
  reports/                 综合报告（00 总览 / 01 全景 / 05 Web 成本 / 06 CJK / 09 方法论 / 10 ADR / 11 目录学 / 12 工程方法 / 13 资产 / 14 图形 API / 15 视频）
tools/
  ledger.py                账本校验 / 统计 / 报告（零依赖，含 --selftest 金丝雀）
scratch/                   浅克隆与工作区（不进 git）
```

## 复算（三条命令）

```bash
python tools/ledger.py selftest                  # 金丝雀：证明校验器真的会判红
python tools/ledger.py verify --min 1000         # 账本门：任一条目无锚/引文不实 ⇒ 非零退出
python tools/ledger.py report --out docs/analysis/ledger-stats.md
```

## 许可

**GPL-3.0-or-later**——与青简上游（`crates/*` 均 `GPL-3.0-or-later`）一致，`docs/renderer-qingjian/` 的文档
可直接整理为上游 PR 回流（引用源码片段已逐字标注出处）。
