# ledger 目录说明

- 每个波次一个文件：`w<波次><字母>.jsonl`（如 `w1a.jsonl`），一行一个分析条目（UTF-8）。
- 条目 id 前缀同文件名（`w1a.jsonl` ⇒ `W1A-001`，顺序递增）。
- schema 与深度口径见 [../METHOD.md](../METHOD.md) 与 [../AGENT-BRIEF.md](../AGENT-BRIEF.md)。
- 自查（单文件）：`python tools/ledger.py verify --file docs/analysis/ledger/w1a.jsonl`
- 全量门：`python tools/ledger.py verify --min 1000`
- 统计报表：`python tools/ledger.py report --out docs/analysis/ledger-stats.md`
