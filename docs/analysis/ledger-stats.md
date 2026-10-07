# 账本统计（机器生成）

- 生成日期：2026-10-07
- 复算命令：`python tools/ledger.py report --out docs/analysis/ledger-stats.md`
- 账本文件：56 个 jsonl

**总条目（通过校验）：4522**；被拒条目：25；覆盖目标数：596；带 lesson 字段：4508

## 深度分布

| depth | 条数 |
| --- | --- |
| source | 2410 |
| doc | 1375 |
| paper | 699 |
| web | 38 |

## 波次文件分布

| 文件 | 条数 |
| --- | --- |
| w10a.jsonl | 81 |
| w10b.jsonl | 64 |
| w11a.jsonl | 60 |
| w12a.jsonl | 54 |
| w13a.jsonl | 49 |
| w14a.jsonl | 29 |
| w15a.jsonl | 48 |
| w1a.jsonl | 176 |
| w1b.jsonl | 115 |
| w1c.jsonl | 99 |
| w1d.jsonl | 124 |
| w1e.jsonl | 143 |
| w1f.jsonl | 93 |
| w2a.jsonl | 138 |
| w2b.jsonl | 129 |
| w2c.jsonl | 100 |
| w2d.jsonl | 188 |
| w2e.jsonl | 102 |
| w2f.jsonl | 88 |
| w2g.jsonl | 89 |
| w3a.jsonl | 85 |
| w3b.jsonl | 83 |
| w3c.jsonl | 117 |
| w3d.jsonl | 75 |
| w3e.jsonl | 68 |
| w3f.jsonl | 60 |
| w3g.jsonl | 81 |
| w3h.jsonl | 69 |
| w4a.jsonl | 62 |
| w4b.jsonl | 26 |
| w4c.jsonl | 134 |
| w4d.jsonl | 90 |
| w4e.jsonl | 122 |
| w4f.jsonl | 86 |
| w5a.jsonl | 125 |
| w6a.jsonl | 13 |
| w6b.jsonl | 14 |
| w6c.jsonl | 21 |
| w6d.jsonl | 17 |
| w6e.jsonl | 18 |
| w6f.jsonl | 13 |
| w6g.jsonl | 12 |
| w6h.jsonl | 13 |
| w6j.jsonl | 8 |
| w6k.jsonl | 6 |
| w7a.jsonl | 161 |
| w7b.jsonl | 72 |
| w7c.jsonl | 85 |
| w7d.jsonl | 87 |
| w8a.jsonl | 109 |
| w8b.jsonl | 137 |
| w8c.jsonl | 108 |
| w8d.jsonl | 90 |
| w9a.jsonl | 128 |
| w9b.jsonl | 74 |
| w9c.jsonl | 84 |

## 目标 Top 30

| target | 条数 |
| --- | --- |
| godot | 177 |
| qingjian | 177 |
| bevy | 153 |
| qt-quick-scenegraph | 106 |
| chromium | 100 |
| qingjian-render | 98 |
| avalonia | 86 |
| gtk4-gsk | 82 |
| flutter | 78 |
| tiny-skia | 68 |
| impeller | 67 |
| gn-sdk | 67 |
| skia | 62 |
| webrender | 59 |
| blend2d | 55 |
| gpui | 43 |
| cosmic-text | 42 |
| film-grain | 42 |
| unity-ugui | 37 |
| apple | 36 |
| coherent-gameface | 36 |
| imgui | 36 |
| thorvg | 35 |
| slint | 34 |
| microsoft-wpf | 29 |
| unreal-slate-umg | 28 |
| qingjian-gates | 28 |
| taa-playdead | 27 |
| egui | 26 |
| mineradio | 26 |

## 被拒条目样本（前 20）

- `w3h.jsonl:28` 锚文件不存在: D:/KF/unified-rx-mcp/README.md
- `w3h.jsonl:29` 锚文件不存在: D:/KF/unified-rx-mcp/README.md
- `w3h.jsonl:30` 锚文件不存在: D:/KF/unified-rx-mcp/README.md
- `w3h.jsonl:31` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/stat_judge.py
- `w3h.jsonl:32` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/stat_judge.py
- `w3h.jsonl:33` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/local_gate.py
- `w3h.jsonl:34` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/local_gate.py
- `w3h.jsonl:35` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/claim_gate.py
- `w3h.jsonl:36` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/claim_gate.py
- `w3h.jsonl:37` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/bench_anchor_gate.py
- `w3h.jsonl:38` 锚文件不存在: D:/KF/unified-rx-mcp/docs/STRESS-AND-PR-GATES.md
- `w3h.jsonl:39` 锚文件不存在: D:/KF/unified-rx-mcp/docs/STRESS-AND-PR-GATES.md
- `w3h.jsonl:40` 锚文件不存在: D:/KF/unified-rx-mcp/docs/STRESS-AND-PR-GATES.md
- `w3h.jsonl:41` 锚文件不存在: D:/KF/unified-rx-mcp/scripts/quality_pact_gate.py
- `w3h.jsonl:42` 锚文件不存在: D:/KF/unified-rx-mcp/spec/cli-golden.json
- `w3h.jsonl:43` 锚文件不存在: D:/KF/unified-rx-mcp/LEGACY_FREEZE.md
- `w3h.jsonl:49` 引文对不上 D:/KF/BSHSQ/scripts/gate_all.sh:89（±5 行内未找到逐字引文）
- `w3h.jsonl:50` 引文对不上 D:/KF/BSHSQ/scripts/gate_all.sh:109（±5 行内未找到逐字引文）
- `w3h.jsonl:51` 引文对不上 D:/KF/BSHSQ/scripts/gate_all.sh:135（±5 行内未找到逐字引文）
- `w3h.jsonl:52` 引文对不上 D:/KF/BSHSQ/scripts/gate_all.sh:147（±5 行内未找到逐字引文）
