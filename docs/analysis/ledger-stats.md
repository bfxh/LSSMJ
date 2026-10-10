# 账本统计（机器生成）

- 性质：展示报告，非门禁（判定用 `ledger.py verify`）
- 复算命令：`python tools/ledger.py report --out docs/analysis/ledger-stats.md`
- 账本文件：58 个 jsonl

**总条目（通过校验）：4493**；被拒条目：70；例外豁免条目：10；重复 id：0；覆盖目标数：601；带 lesson 字段：4479

## 深度分布

| depth | 条数 |
| --- | --- |
| source | 2365 |
| doc | 1377 |
| paper | 713 |
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
| w15b.jsonl | 15 |
| w15c.jsonl | 11 |
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
| w3h.jsonl | 90 |
| w4b.jsonl | 26 |
| w4c.jsonl | 134 |
| w4d.jsonl | 90 |
| w4e.jsonl | 122 |
| w4f.jsonl | 86 |
| w5a.jsonl | 125 |
| w6a.jsonl | 13 |
| w6b.jsonl | 13 |
| w6c.jsonl | 19 |
| w6d.jsonl | 17 |
| w6e.jsonl | 15 |
| w6f.jsonl | 13 |
| w6g.jsonl | 8 |
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
| w9a.jsonl | 124 |
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
| qingjian-gates | 29 |
| unreal-slate-umg | 28 |
| taa-playdead | 27 |
| egui | 26 |
| mineradio | 26 |
| vscode | 25 |

## 被拒条目样本（前 20）

- `w3h.jsonl:53` 引文对不上 D:/KF/BSHSQ/docs/SPEC.md:237（±5 行内未找到逐字引文）
- `w3h.jsonl:60` 引文对不上 D:/KF/BSHSQ/docs/PERF-REVIEW-2026-09-27.md:103（±5 行内未找到逐字引文）
- `w3h.jsonl:61` 引文对不上 D:/KF/BSHSQ/docs/PERF-REVIEW-2026-09-27.md:107（±5 行内未找到逐字引文）
- `w4a.jsonl:1` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:2` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:3` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:4` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:5` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:6` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:7` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:8` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:9` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:10` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:11` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:12` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:13` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/GN.h
- `w4a.jsonl:14` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/_Graphics/GN_G2D.h
- `w4a.jsonl:15` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/_Graphics/GN_G2D.h
- `w4a.jsonl:16` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/_Graphics/GN_G2D.h
- `w4a.jsonl:17` 锚文件不存在: D:/KF/GN_SDK1e/GN_SDK1e/Include/_Graphics/GN_G2D.h
