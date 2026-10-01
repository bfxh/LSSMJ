#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LSSMJ 分析账本：校验 + 统计 + 报告（零依赖，Python 3.8+）。

账本 = docs/analysis/ledger/*.jsonl，一行一个分析条目（UTF-8）：
  {"id":"W1A-001","depth":"source","target":"bevy_ui",
   "anchor":"D:/KF/LSSMJ/scratch/src/bevy/crates/bevy_ui/src/lib.rs:12",
   "quote":"<该行逐字子串，>=10 字符>",
   "finding":"<= 该观察本身，>=20 字符 >","lesson":"<= 对本项目的含义，可空 >",
   "date":"2026-10-01"}

校验规则（verify）：
  1. 必填：id, depth, target, anchor, finding, date；depth ∈ {source, doc, paper, web}
  2. depth==source：anchor = <path>:<line>（路径可绝对或相对仓根）；文件必须存在；
     quote 必须逐字出现在该行 ±TOL 行内（子串匹配，大小写敏感）
  3. depth!=source：anchor 必须 http(s):// 开头；finding 长度 >= 20 字符
  4. id 全局唯一；finding 长度 >= 20 字符
退出码：有违规/被拒 ⇒ 1；--min N 未达标 ⇒ 2。

用法：
  python tools/ledger.py selftest
  python tools/ledger.py verify [--ledger-dir docs/analysis/ledger] [--file X.jsonl] [--min 1000]
  python tools/ledger.py stats  [--ledger-dir docs/analysis/ledger] [--json]
  python tools/ledger.py report --out docs/analysis/ledger-stats.md
"""
import argparse
import collections
import datetime
import glob
import json
import os
import re
import sys
from pathlib import Path

TOL = 5  # 引文容差（行）
DEPTHS = ("source", "doc", "paper", "web")
MIN_FINDING = 20
MIN_QUOTE = 10


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def default_ledger_dir():
    return os.path.join(repo_root(), "docs", "analysis", "ledger")


def norm_path(p, root):
    p = p.replace("\\", "/")
    if re.match(r"^[A-Za-z]:/", p) or p.startswith("/"):
        return p
    return os.path.join(root, p).replace("\\", "/")


def split_anchor(anchor, root):
    """返回 (path, line) 或 None。路径取最后一个 ':' 后的整数行号。"""
    if not isinstance(anchor, str) or ":" not in anchor:
        return None
    head, _, tail = anchor.rpartition(":")
    if not head or not tail.isdigit():
        return None
    return norm_path(head, root), int(tail)


def read_lines(path):
    """按行读取；UTF-8 解码失败时回退 GBK/GB2312（Windows 中文源码常见编码）。

    回退只影响「怎么把字节读成文本」，不改变校验强度：引文仍须逐字出现。
    """
    with open(path, "rb") as f:
        raw = f.read()
    try:
        text = raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        text = raw.decode("gbk", errors="replace")
    return text.splitlines(keepends=True)


def check_row(row, root, cache):
    """返回 [] 或 [违规原因, ...]"""
    errs = []
    for k in ("id", "depth", "target", "anchor", "finding", "date"):
        v = row.get(k)
        if not isinstance(v, str) or not v.strip():
            errs.append(f"缺字段/空: {k}")
    if errs:
        return errs
    if row["depth"] not in DEPTHS:
        errs.append(f"depth 非法: {row['depth']!r}（应为 {DEPTHS}）")
    if len(row["finding"].strip()) < MIN_FINDING:
        errs.append(f"finding 过短（<{MIN_FINDING} 字符）")
    if row["depth"] == "source":
        sp = split_anchor(row["anchor"], root)
        if not sp:
            errs.append("source 锚格式应为 <path>:<line>")
            return errs
        path, line = sp
        if not os.path.isfile(path):
            errs.append(f"锚文件不存在: {path}")
            return errs
        quote = row.get("quote") or ""
        if len(quote) < MIN_QUOTE:
            errs.append(f"source 条目 quote 缺失或过短（<{MIN_QUOTE} 字符）")
            return errs
        if path not in cache:
            cache[path] = read_lines(path)
        lines = cache[path]
        lo = max(0, line - 1 - TOL)
        hi = min(len(lines), line + TOL)
        if not any(quote in ln for ln in lines[lo:hi]):
            errs.append(f"引文对不上 {path}:{line}（±{TOL} 行内未找到逐字引文）")
    else:
        if not re.match(r"^https?://", row["anchor"]):
            errs.append("非 source 条目的 anchor 必须 http(s):// 开头")
    return errs


def load_rows(paths, root):
    rows, rejected = [], []
    for p in paths:
        with open(p, encoding="utf-8") as f:
            for i, raw in enumerate(f, 1):
                raw = raw.strip()
                if not raw:
                    continue
                try:
                    row = json.loads(raw)
                except Exception as e:  # noqa: BLE001
                    rejected.append((p, i, f"JSON 解析失败: {e}"))
                    continue
                if not isinstance(row, dict):
                    rejected.append((p, i, "不是 JSON 对象"))
                    continue
                row["__file"], row["__line"] = p, i
                errs = check_row(row, root, cache={})
                if errs:
                    rejected.append((p, i, "; ".join(errs)))
                else:
                    rows.append(row)
    return rows, rejected


def ledger_files(args, root):
    if args.file:
        return [os.path.abspath(args.file)]
    d = args.ledger_dir or default_ledger_dir()
    if not os.path.isabs(d):
        d = os.path.join(root, d)
    return sorted(glob.glob(os.path.join(d, "*.jsonl")))


def cmd_selftest(args):
    """金丝雀：好条目必须绿、坏条目必须红（且坏条目死在'引文对不上'上）。"""
    root = repo_root()
    good_path = os.path.join(root, "tools", "ledger.py")
    good = {
        "id": "SELFTEST-OK", "depth": "source", "target": "self",
        "anchor": f"{good_path}:1", "quote": "#!/usr/bin/env python3",
        "finding": "金丝雀正例：合法的 source 条目必须被校验器放行通过", "date": "2026-10-01",
    }
    bad = {
        "id": "SELFTEST-BAD", "depth": "source", "target": "self",
        "anchor": f"{good_path}:1",
        "quote": "这段引文故意不在源文件里-金丝雀负例-2026",
        "finding": "金丝雀负例：引文对不上的 source 条目必须被校验器判红", "date": "2026-10-01",
    }
    cache = {}
    ok_good = not check_row(good, root, cache)
    errs_bad = check_row(bad, root, cache)
    ok_bad = any("引文对不上" in e for e in errs_bad)
    print(f"selftest: good={'PASS' if ok_good else 'FAIL'} bad={'PASS(red on quote)' if ok_bad else 'FAIL'}")
    if errs_bad and not ok_bad:
        print("坏条目的拒绝原因：", errs_bad)
    if not ok_bad:
        print("!! 负例没有死在'引文对不上'上——引文门没有被证明")
    return 0 if (ok_good and ok_bad) else 1


def cmd_verify(args):
    root = repo_root()
    files = ledger_files(args, root)
    rows, rejected = load_rows(files, root)
    ids = collections.Counter(r["id"] for r in rows)
    dupes = [i for i, c in ids.items() if c > 1]
    for p, i, why in rejected:
        print(f"REJECT {os.path.relpath(p, root)}:{i}  {why}")
    for i in dupes:
        print(f"REJECT 重复 id: {i}")
        rejected.append(("(dup)", 0, f"重复 id {i}"))
    print(f"files={len(files)} rows_ok={len(rows)} rejected={len(rejected)}")
    if args.min is not None and len(rows) < args.min:
        print(f"FAIL: 账本 {len(rows)} 条 < --min {args.min}")
        return 2
    return 1 if rejected else 0


def collect_stats(root, files):
    rows, rejected = load_rows(files, root)
    by_depth = collections.Counter(r["depth"] for r in rows)
    by_target = collections.Counter(r["target"] for r in rows)
    by_file = collections.Counter(os.path.basename(r["__file"]) for r in rows)
    with_lesson = sum(1 for r in rows if (r.get("lesson") or "").strip())
    return {
        "total": len(rows), "rejected": len(rejected),
        "by_depth": dict(by_depth), "targets": len(by_target),
        "top_targets": by_target.most_common(30), "by_wave_file": dict(by_file),
        "with_lesson": with_lesson, "rejects_sample": rejected[:20],
    }


def cmd_stats(args):
    root = repo_root()
    st = collect_stats(root, ledger_files(args, root))
    if args.json:
        print(json.dumps(st, ensure_ascii=False, indent=1))
        return 0
    print(f"total={st['total']} rejected={st['rejected']} targets={st['targets']} with_lesson={st['with_lesson']}")
    print("by_depth:", st["by_depth"])
    print("by_wave_file:", st["by_wave_file"])
    print("top_targets:", st["top_targets"][:15])
    return 0


def safe_out_path(out, root):
    """报告输出路径：一律落仓根内（防路径穿越），并确保父目录存在。"""
    p = Path(out)
    if not p.is_absolute():
        p = Path(root) / p
    p = p.resolve()
    rootp = Path(root).resolve()
    if rootp not in p.parents and p.parent != rootp:
        raise SystemExit(f"拒绝写入仓根之外的路径: {p}")
    p.parent.mkdir(parents=True, exist_ok=True)
    return p


def cmd_report(args):
    root = repo_root()
    files = ledger_files(args, root)
    st = collect_stats(root, files)
    today = datetime.date.today().isoformat()
    L = []
    L.append("# 账本统计（机器生成）")
    L.append("")
    L.append(f"- 生成日期：{today}")
    L.append("- 复算命令：`python tools/ledger.py report --out docs/analysis/ledger-stats.md`")
    L.append(f"- 账本文件：{len(files)} 个 jsonl")
    L.append("")
    L.append(f"**总条目（通过校验）：{st['total']}**；被拒条目：{st['rejected']}；"
             f"覆盖目标数：{st['targets']}；带 lesson 字段：{st['with_lesson']}")
    L.append("")
    L.append("## 深度分布")
    L.append("")
    L.append("| depth | 条数 |")
    L.append("| --- | --- |")
    for d in DEPTHS:
        L.append(f"| {d} | {st['by_depth'].get(d, 0)} |")
    L.append("")
    L.append("## 波次文件分布")
    L.append("")
    L.append("| 文件 | 条数 |")
    L.append("| --- | --- |")
    for f, c in sorted(st["by_wave_file"].items()):
        L.append(f"| {f} | {c} |")
    L.append("")
    L.append("## 目标 Top 30")
    L.append("")
    L.append("| target | 条数 |")
    L.append("| --- | --- |")
    for t, c in st["top_targets"]:
        L.append(f"| {t} | {c} |")
    if st["rejects_sample"]:
        L.append("")
        L.append("## 被拒条目样本（前 20）")
        L.append("")
        for p, i, why in st["rejects_sample"]:
            L.append(f"- `{os.path.basename(p)}:{i}` {why}")
    out = safe_out_path(args.out, root)
    out.write_text("\n".join(L) + "\n", encoding="utf-8")
    print(f"written: {out}  (total={st['total']}, rejected={st['rejected']})")
    return 0


def main():
    ap = argparse.ArgumentParser(description="LSSMJ 分析账本工具")
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("selftest")

    pv = sub.add_parser("verify")
    pv.add_argument("--ledger-dir", default=None)
    pv.add_argument("--file", default=None)
    pv.add_argument("--min", type=int, default=None)

    ps = sub.add_parser("stats")
    ps.add_argument("--ledger-dir", default=None)
    ps.add_argument("--file", default=None)
    ps.add_argument("--json", action="store_true")

    pr = sub.add_parser("report")
    pr.add_argument("--ledger-dir", default=None)
    pr.add_argument("--file", default=None)
    pr.add_argument("--out", default="docs/analysis/ledger-stats.md")

    args = ap.parse_args()
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "verify":
        return cmd_verify(args)
    if args.cmd == "stats":
        return cmd_stats(args)
    if args.cmd == "report":
        return cmd_report(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
