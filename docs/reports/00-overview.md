# 00 · 总览：这个仓在做什么、怎么读、怎么复算

## 一句话

本仓（LSSMJ）做两件事：**①** 对"渲染引擎 / UI 框架 / 文本栈 / 论文"做**证据锚定**的大规模分析
（每条结论可指到 `file:line` 或 URL；机器校验）；**②** 交付**青简自绘渲染器
（`qingjian-render`）的完整技术文档**，并把分析结果落成对它的**对标评估**。

## 给谁读

| 读者 | 从哪开始 |
| --- | --- |
| 想看**渲染器怎么工作**（含代码导航、坑、性能口径） | [`renderer-qingjian/README.md`](../renderer-qingjian/README.md) → 01–10 |
| 想看**全行业怎么做的**（引擎/平台/论文） | [`../analysis/targets.md`](../analysis/targets.md)（目标总清单）→ `engines/**`、`platforms/**`、`papers/**` |
| 想**核账**（这条结论凭什么） | [`../analysis/METHOD.md`](../analysis/METHOD.md) → `analysis/ledger/*.jsonl` → `tools/ledger.py verify` |
| 想看**两个用户提供样本**的解剖 | [`../targets/mineradio-electron.md`](../targets/mineradio-electron.md)、`renderer-gn/**` |

## 口径声明（必须带读）

1. **`qingjian-team/qingjian` 不是 Bevy 游戏引擎**，是 Rust 写的**拼音输入法**（GPL-3.0-or-later）；
   "那个渲染引擎" = 它内部的 `crates/qingjian-render`（"把候选窗一帧画成位图，各平台只负责贴图"）。
   本文档集以此为主体；Bevy 本身另按"用户点名的引擎"单列分析（`engines/bevy.md`，见 targets.md D3）。
2. **"≥1000 次分析"的机器口径**：证据锚定条目总数 ≥1000（`tools/ledger.py verify --min 1000`）。
   深度四条轨道：`source`（逐字读源码）/`doc`（官方文档）/`paper`（论文与正式长文）/`web`（旁证）。
   **in-repo 设计文档按 `targets.md` C6 约定计入 `source` 深度——构成在 `ledger-stats.md` 中披露。**
3. **所有数字带口径**（版本/提交、机器、场景、样本范围）；跨口径不可互套。
4. **不隐账**：被拒条目数与被拒样本在 `ledger-stats.md` 公开；金丝雀（`selftest`）证明门会判红。

## 目录与产物（哪些已就绪）

- 交付主体：`../renderer-qingjian/` —— README + 01 总览 + 02 架构 + 03 文本管线 + 04 帧与排布 +
  05 光栅画布 + 06 主题 + 07 平台后端 + 08 性能 + 09 对标评估 + 10 术语表。
- 分析工程：`../analysis/`（协议/简报/目标/账本/统计）+ `../engines|papers|platforms|targets/`。
- 综合报告（本目录）：00 总览（本篇）、01 全景、02–08 主题综述、09 性能方法论、10 决策 ADR、11 目录学。
- 工具：`tools/ledger.py`（校验/统计/报告 + selftest 金丝雀）。

## 复算（三条命令 + 一个指针）

```bash
python tools/ledger.py selftest                    # 金丝雀：门会判红
python tools/ledger.py verify --min 1000           # 全量账本门（引文逐字校验）
python tools/ledger.py report --out docs/analysis/ledger-stats.md
```

**当前计数不写在本页**（避免"过程变东西"）：以 `../analysis/ledger-stats.md`（机器生成、含
生成日期与被拒样本）为准；目标清单与各波次状态见 `../analysis/targets.md`。

## 波次与目标（结构）

| 波次 | 内容 | 账本文件 |
| --- | --- | --- |
| W1 | qingjian 全仓 / 文本栈 / Rust UI 框架 / 2D 光栅器 / gpui·WebRender·Skia / 文本渲染论文 | `w1a`–`w1f` |
| W2 | Godot / Flutter+Impeller / Chromium / Qt+GTK4 / Unity+Unreal+中间件 / Apple+MS+Android | `w2a`–`w2f` |
| W3 | GPU 技术论文 / 布局与增量论文 / UI 系统与延迟论文 / Web 栈成本 / CJK 与输入 | `w3a`–`w3e` |
| W4 | GN SDK（闭源）逆向 / Mineradio（Electron）解剖 / Bevy / 即时模式群 | `w4a`–`w4d` |
| W5 | 主代理自读（渲染器 38 文件 + 平台壳 + 设计档） | `w5a` |

## 许可与回流

GPL-3.0-or-later（与青简上游一致）。`../renderer-qingjian/**` 与逐条账本可直接整理为上游
issue/PR 素材；引用源码片段逐字、带出处。
