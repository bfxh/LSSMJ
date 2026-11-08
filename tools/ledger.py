#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""LSSMJ 分析账本：校验 + 统计 + 报告（零依赖，Python 3.8+）。

账本 = docs/analysis/ledger/*.jsonl，一行一个分析条目（UTF-8）：
  {"id":"W1A-001","depth":"source","target":"bevy_ui",
   "anchor":"scratch/src/bevy/crates/bevy_ui/src/lib.rs:12",
   "quote":"<该行逐字子串，>=10 字符>",
   "finding":"<= 该观察本身，>=20 字符 >","lesson":"<= 对本项目的含义，可空 >",
   "date":"2026-10-01"}

基础契约（verify 默认档）：
  1. 必填：id, depth, target, anchor, finding, date；depth ∈ {source, doc, paper, web}
  2. date 必须 ISO 日历日（YYYY-MM-DD，2026-13-01 这类格式对日期错的也拒）；
     字符串字段必须真是字符串（非字符串给逐条拒绝原因，不再 TypeError）
  3. depth==source：anchor = <path>:<line>，line ≥ 1 且 ≤ 文件实际行数（锚必须指向
     存在的行）；路径禁 `..` 穿越段；文件必须存在；quote 逐字出现在该行 ±TOL 行内
  4. 读路径安全：锚解析符号链接后必须落在**允许证据根**内——仓根 + 例外文件登记的
     `evidence_roots` + `--evidence-root` 旗标（全部显式可见）
  5. 解码策略：**无损**——utf-8-sig → GBK 严格解码；皆败 ⇒ 拒绝
     （不再 errors="replace" 用 U+FFFD 替换字符伪造逐字基线）
  6. id 全局唯一；无账本文件 ⇒ fail-closed（exit 1，不把空输入当成功）
目标契约（verify --strict，追加）：
  7. 非 source 条目也须带 ≥10 字符 quote
例外（--exceptions，与 ref_gate 同 schema + evidence_roots 元数据）：
  {"schema_version":1,"evidence_roots":["D:/KF/..."],"exceptions":[{"id","reason","expires"}]}
  只豁免迁移期三类（DECODE / PATH_ESCAPE / NONSOURCE_QUOTE），被豁免条目**不计入
  "通过校验"总数**（EXCEPT 行可见、可数）；文件不存在 = 无例外 + 仅仓根；过期 = 不豁免。
退出码（verify / report --strict）：违规 ⇒ 1；--min N 未达标 ⇒ 2。

用法：
  python tools/ledger.py selftest
  python tools/ledger.py verify [--ledger-dir D] [--file X.jsonl] [--min N]
         [--strict] [--exceptions ci/ledger-exceptions.json] [--evidence-root R ...]
  python tools/ledger.py gaps  [...] [--json]   # 未达目标契约的条目（人工补全队列）
  python tools/ledger.py stats  [...] [--json]
  python tools/ledger.py report --out docs/analysis/ledger-stats.md
         [--as-of YYYY-MM-DD] [--strict] [--min N]
         （报告不含墙上时钟日期：同输入跨日期/跨目录字节一致，F06；--as-of 显式给则原样打印）
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
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
MAX_CACHE_FILES = 256  # 共享只读缓存上限（限内存；超限清表只影响速度不影响正确性）

# 可被例外豁免的违规码（F07/F02 迁移期；2026-10-11 存量实测 DECODE=9 / PATH_ESCAPE=1
# / NONSOURCE_QUOTE=21 / ANCHOR_EXISTS=67（GN_SDK1e 已压缩为 zip）/ QUOTE_MISMATCH=3
# （BSHSQ docs 漂移），登记于 ci/ledger-exceptions.json。
# ⚠️ ANCHOR_EXISTS 与 QUOTE_MISMATCH 仅对**仓外锚**豁免（check_row 内强制）——
# 仓内锚文件缺失/引文失配 = 仓已损坏，永不豁免。
WAIVABLE = ("DECODE", "PATH_ESCAPE", "NONSOURCE_QUOTE", "ANCHOR_EXISTS", "QUOTE_MISMATCH")


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def default_ledger_dir():
    return os.path.join(repo_root(), "docs", "analysis", "ledger")


def safe_input_path(p, what):
    """输入路径防线：拒绝含 `..` 穿越段的路径，规范化后使用。"""
    if p and ".." in Path(p).parts:
        raise SystemExit(f"拒绝含 .. 的{what}路径: {p}")
    return os.path.abspath(p) if p else p


def norm_path(p, root):
    p = p.replace("\\", "/")
    if re.match(r"^[A-Za-z]:/", p) or p.startswith("/"):
        return p
    return os.path.join(root, p).replace("\\", "/")


def split_anchor(anchor, root):
    """返回 (path, line) 或 None。路径取最后一个 ':' 后的整数行号（0/负由调用方判）。"""
    if not isinstance(anchor, str) or ":" not in anchor:
        return None
    head, _, tail = anchor.rpartition(":")
    if not head or not tail.isdigit():
        return None
    return norm_path(head, root), int(tail)


def anchor_inside_roots(path, roots):
    """锚解析符号链接后必须落在任一允许证据根内（normcase 前缀匹配，防同前缀目录名）。"""
    try:
        rp = os.path.normcase(os.path.realpath(path))
    except OSError:
        return False
    for r in roots:
        try:
            rr = os.path.normcase(os.path.realpath(r))
        except OSError:
            continue
        if rp == rr or rp.startswith(rr + os.sep):
            return True
    return False


def read_lines(path):
    """无损解码策略（F07）：utf-8-sig → GBK 严格；皆败 ⇒ ValueError（转逐条拒绝原因）。

    回退只影响「怎么把字节读成文本」，不改变校验强度：引文仍须逐字出现；
    errors="replace" 会用 U+FFFD 伪造可匹配文本，故废弃。
    """
    with open(path, "rb") as f:
        raw = f.read()
    try:
        text = raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        try:
            text = raw.decode("gbk")
        except UnicodeDecodeError as e:
            raise ValueError(f"锚文件无法无损解码（非 UTF-8/GBK）: {path}") from e
    return text.splitlines(keepends=True)


def display_rel(p, root):
    """报错定位用相对路径；Windows 跨盘符（如 TEMP 在 C:、仓在 D:）relpath 会抛
    ValueError ⇒ 退回绝对路径（同 ref_gate.display_rel，#14 判例）。"""
    try:
        return os.path.relpath(p, root)
    except ValueError:
        return p


def check_date(v):
    """ISO 格式 + 真实日历日。"""
    if not DATE_RE.match(v):
        return False
    try:
        datetime.date.fromisoformat(v)
        return True
    except ValueError:
        return False


def check_row(row, root, cache, strict=False, exceptions=None, evidence_roots=None):
    """返回 [(code, 消息), ...]。strict=True 追加目标契约（非 source 强制 quote）。

    exceptions：{id: (reason, expires)}；仅豁免 WAIVABLE 中的码，被豁免违规以
    EXEMPT_<码> 返回（调用方计为 EXCEPT，不计入"通过校验"）。
    evidence_roots：允许证据根列表；None = 仅仓根。
    """
    errs = []
    roots = evidence_roots if evidence_roots is not None else [root]

    def add(code, msg):
        errs.append((code, msg))

    def waive(code, msg):
        if exceptions and row.get("id") in exceptions:
            errs.append(("EXEMPT_" + code, msg + f"  [例外: {exceptions[row['id']][0]}]"))
            return True
        return False

    for k in ("id", "depth", "target", "anchor", "finding", "date"):
        v = row.get(k)
        if not isinstance(v, str) or not v.strip():
            add("MISSING_FIELD", f"缺字段/空: {k}")
    if errs:
        return errs
    if row["depth"] not in DEPTHS:
        add("DEPTH", f"depth 非法: {row['depth']!r}（应为 {DEPTHS}）")
    if len(row["finding"].strip()) < MIN_FINDING:
        add("FINDING_SHORT", f"finding 过短（<{MIN_FINDING} 字符）")
    if not check_date(row["date"]):
        add("DATE_FORMAT", f"date 非 ISO 日历日: {row['date']!r}")

    quote = row.get("quote")
    if quote is not None and not isinstance(quote, str):
        add("QUOTE_TYPE", "quote 必须是字符串（逐字引文）")
        quote = None

    if row["depth"] == "source":
        sp = split_anchor(row["anchor"], root)
        if not sp:
            add("ANCHOR_FORMAT", "source 锚格式应为 <path>:<line>")
            return errs
        path, line = sp
        if line < 1:
            add("LINE_RANGE", f"行号必须 ≥ 1（实测 {line}）: {path}")
            return errs
        if ".." in Path(path).parts:
            msg = f"锚路径含 .. 穿越段: {path}"
            if not waive("PATH_ESCAPE", msg):
                add("PATH_ESCAPE", msg)
            return errs
        if not os.path.isfile(path):
            msg = f"锚文件不存在: {path}"
            if anchor_inside_roots(path, [root]):
                add("ANCHOR_EXISTS", msg)  # 仓内锚文件缺失 = 仓已损坏，永不豁免
            elif not waive("ANCHOR_EXISTS", msg):  # 仓外 = 登记过的作者机依赖，可豁免
                add("ANCHOR_EXISTS", msg)
            return errs
        if not anchor_inside_roots(path, roots):
            msg = f"source 锚不在允许证据根内（解析 symlink 后）: {path}"
            if not waive("PATH_ESCAPE", msg):
                add("PATH_ESCAPE", msg)
            return errs
        if quote is None or len(quote) < MIN_QUOTE:
            add("QUOTE_SHORT", f"source 条目 quote 缺失或过短（<{MIN_QUOTE} 字符）")
            return errs
        try:
            if path not in cache:
                if len(cache) >= MAX_CACHE_FILES:
                    cache.clear()
                cache[path] = read_lines(path)
            lines = cache[path]
        except ValueError as e:
            if not waive("DECODE", str(e)):
                add("DECODE", str(e))
            return errs
        if line > len(lines):
            add("LINE_RANGE", f"行号 {line} 超出文件长度 {len(lines)}: {path}")
            return errs
        lo = max(0, line - 1 - TOL)
        hi = min(len(lines), line + TOL)
        if not any(quote in ln for ln in lines[lo:hi]):
            msg = f"引文对不上 {path}:{line}（±{TOL} 行内未找到逐字引文）"
            if anchor_inside_roots(path, [root]):
                add("QUOTE_MISMATCH", msg)  # 仓内引文无损可复核，永不豁免
            elif not waive("QUOTE_MISMATCH", msg):  # 仓外证据会随上游演进，可豁免
                add("QUOTE_MISMATCH", msg)
    else:
        if not re.match(r"^https?://", row["anchor"]):
            add("URL_SCHEME", "非 source 条目的 anchor 必须 http(s):// 开头")
        if strict and (quote is None or len(quote) < MIN_QUOTE):
            msg = f"非 source 条目缺 ≥{MIN_QUOTE} 字符 quote（目标契约）"
            if not waive("NONSOURCE_QUOTE", msg):
                add("NONSOURCE_QUOTE", msg)
    return errs


def load_exceptions(path, as_of):
    """{"schema_version":1,"evidence_roots":[...],"exceptions":[{"id","reason","expires"}]}
    （ref_gate schema + 证据根元数据）。过期例外不豁免；文件不存在 = 无例外 + 仅仓根。
    返回 (exceptions: {id: (reason, expires)}, evidence_roots: [str])。"""
    if not path:
        return {}, []
    path = safe_input_path(path, "--exceptions")
    if not os.path.isfile(path):
        return {}, []
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    roots = []
    for r in data.get("evidence_roots", []):
        if not isinstance(r, str) or not r or ".." in Path(r).parts:
            raise SystemExit(f"evidence_roots 条目非法（须绝对路径、无 ..）: {r!r}")
        roots.append(os.path.abspath(r))
    out = {}
    for entry in data.get("exceptions", []):
        rid, reason, expires = entry.get("id"), entry.get("reason"), entry.get("expires")
        if not (isinstance(rid, str) and rid and isinstance(reason, str) and reason
                and isinstance(expires, str)):
            raise SystemExit(f"例外条目缺 id/reason/expires: {entry!r}")
        try:
            exp = datetime.date.fromisoformat(expires)
        except ValueError as e:
            raise SystemExit(f"例外 {rid} expires 非 ISO 日期: {expires}") from e
        if exp >= as_of:
            out[rid] = (reason, expires)
    return out, roots


def load_rows(paths, root, strict=False, exceptions=None, evidence_roots=None):
    """共享文件缓存（F07：跨条目复用，不再每条一个 cache={}）。"""
    rows, rejected, excepted = [], [], []
    cache = {}
    for p in paths:
        try:
            f = open(p, encoding="utf-8")
        except OSError as e:
            rejected.append((p, 0, f"账本文件无法读取: {e}"))
            continue
        with f:
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
                errs = check_row(row, root, cache, strict=strict, exceptions=exceptions,
                                 evidence_roots=evidence_roots)
                if not errs:
                    rows.append(row)
                elif all(code.startswith("EXEMPT_") for code, _ in errs):
                    excepted.append((p, i, "; ".join(m for _, m in errs)))
                else:
                    rejected.append((p, i, "; ".join(m for _, m in errs)))
    return rows, rejected, excepted


def find_dupes(rows):
    ids = collections.Counter(r["id"] for r in rows)
    return [i for i, c in sorted(ids.items()) if c > 1]


def ledger_files(args, root):
    if getattr(args, "file", None):
        return [safe_input_path(args.file, "--file")]
    d = args.ledger_dir or default_ledger_dir()
    if not os.path.isabs(d):
        d = os.path.join(root, d)
    return sorted(glob.glob(os.path.join(safe_input_path(d, "--ledger-dir"), "*.jsonl")))


def cmd_selftest(args):
    """金丝雀电池：每条规则都要证明自己会红——好条目绿、坏条目死在正确原因上。"""
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

    def died(row, needle, **kw):
        return any(needle in m for _, m in check_row(row, root, cache, **kw))

    bad_quote = died(bad, "引文对不上")
    bad_date = died({**bad, "date": "2026-13-01"}, "date 非 ISO 日历日")
    bad_line0 = died({**good, "anchor": f"{good_path}:0"}, "行号必须 ≥ 1")
    n_lines = len(read_lines(good_path))
    bad_eof = died({**good, "anchor": f"{good_path}:{n_lines + 1}"}, "超出文件长度")
    bad_type = died({**good, "quote": {"k": "v"}}, "quote 必须是字符串")
    bad_short = died({**good, "quote": "short"}, "quote 缺失或过短")
    bad_dotdot = died({**good, "anchor": f"{root}/../LSSMJ/tools/ledger.py:1"}, ".. 穿越段")

    # 读路径安全 + 无损解码：仓根外临时文件（非 UTF-8/GBK 的无效字节序列）
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        lossy = os.path.join(td, "lossy.py")
        Path(lossy).write_bytes(b"# \xff\xfe\x81\x81 not utf8 not gbk\n")
        # 解码门在证据根门之后 ⇒ 解码金丝雀要把临时目录登记进证据根才能走到
        bad_dec = died({**good, "anchor": f"{lossy}:1"}, "无法无损解码",
                       evidence_roots=[td])
        escape = died({**good, "anchor": f"{lossy}:1"}, "不在允许证据根内")
        allowed = not died({**good, "anchor": f"{lossy}:1"}, "不在允许证据根内",
                           evidence_roots=[td])
        # 例外：登记后豁免（EXEMPT_*）；过期例外由 load_exceptions 过滤 ⇒ 照红
        row_esc = {**good, "id": "SELFTEST-ESC", "anchor": f"{lossy}:1"}
        exc_ok = any(c.startswith("EXEMPT_PATH_ESCAPE") for c, _ in
                     check_row(row_esc, root, cache, exceptions={
                         "SELFTEST-ESC": ("金丝雀测试例外", "2099-01-01")}))
        exp_file = os.path.join(td, "exp.json")
        Path(exp_file).write_text(json.dumps({"schema_version": 1, "exceptions": [
            {"id": "SELFTEST-ESC2", "reason": "已过期", "expires": "2020-01-01"},
            {"id": "SELFTEST-ESC3", "reason": "未过期", "expires": "2099-01-01"},
        ]}), encoding="utf-8")
        exc_loaded, _ = load_exceptions(exp_file, datetime.date.today())
        # 过期例外被过滤 ⇒ 该 id 不豁免 ⇒ 照常红（"不在允许证据根内" 出现在拒绝原因里）
        exc_expired = ("SELFTEST-ESC2" not in exc_loaded and "SELFTEST-ESC3" in exc_loaded
                       and any("不在允许证据根内" in m for _, m in check_row(
                           {**row_esc, "id": "SELFTEST-ESC2"}, root, cache,
                           exceptions=exc_loaded)))
        # ANCHOR_EXISTS 仅仓外锚可豁免：仓外缺文件 + 例外 ⇒ EXEMPT；仓内缺文件 + 例外 ⇒ 照红
        gone_ext = os.path.join(td, "gone.py")  # 不存在的仓外锚
        ext_miss = any(c.startswith("EXEMPT_ANCHOR_EXISTS") for c, _ in check_row(
            {**good, "id": "SELFTEST-GONE", "anchor": f"{gone_ext}:1"}, root, cache,
            exceptions={"SELFTEST-GONE": ("证据已归档", "2099-01-01")}))
        gone_in = os.path.join(root, "tools", "no-such-file-ledger-selftest.py")
        int_miss = any(c == "ANCHOR_EXISTS" for c, _ in check_row(
            {**good, "id": "SELFTEST-GONE2", "anchor": f"{gone_in}:1"}, root, cache,
            exceptions={"SELFTEST-GONE2": ("x", "2099-01-01")}))

    checks = [
        ("good", ok_good),
        ("bad-red-on-quote", bad_quote),
        ("bad-red-on-date", bad_date),
        ("bad-red-on-line0", bad_line0),
        ("bad-red-on-past-eof", bad_eof),
        ("bad-red-on-quote-type", bad_type),
        ("bad-red-on-quote-short", bad_short),
        ("bad-red-on-dotdot", bad_dotdot),
        ("bad-red-on-evidence-root", escape),
        ("evidence-root-allowlist-honored", allowed),
        ("exception-waives-and-counts", exc_ok),
        ("expired-exception-still-red", exc_expired),
        ("bad-red-on-lossless-decode", bad_dec),
        ("external-missing-waivable", ext_miss),
        ("internal-missing-never-waived", int_miss),
    ]
    print("selftest: " + " ".join(f"{n}={'ok' if ok else 'FAIL'}" for n, ok in checks))
    return 0 if all(ok for _, ok in checks) else 1


def cmd_verify(args):
    root = repo_root()
    files = ledger_files(args, root)
    if not files:
        print("FAIL: 无账本文件（fail-closed，不把空输入当成功）")
        return 1
    exceptions, roots = load_exceptions(args.exceptions, datetime.date.today())
    # 仓根永远是允许证据根（文件登记 + 旗标是追加，不是替换）
    roots = ([root] + roots
             + [safe_input_path(r, "--evidence-root")
                for r in getattr(args, "evidence_root", None) or []])
    rows, rejected, excepted = load_rows(files, root, strict=args.strict,
                                         exceptions=exceptions, evidence_roots=roots)
    dupes = find_dupes(rows)
    for p, i, why in rejected:
        print(f"REJECT {display_rel(p, root)}:{i}  {why}")
    for p, i, why in excepted:
        print(f"EXCEPT {display_rel(p, root)}:{i}  {why}")
    for i in dupes:
        print(f"REJECT 重复 id: {i}")
        rejected.append(("(dup)", 0, f"重复 id {i}"))
    print(f"files={len(files)} rows_ok={len(rows)} rejected={len(rejected)} "
          f"excepted={len(excepted)} evidence_roots={len(roots)}"
          f"{' strict' if args.strict else ''}")
    if args.min is not None and len(rows) < args.min:
        print(f"FAIL: 账本 {len(rows)} 条 < --min {args.min}")
        return 2
    return 1 if rejected else 0


def collect_stats(root, files, strict=False, exceptions=None, evidence_roots=None):
    rows, rejected, excepted = load_rows(files, root, strict=strict,
                                         exceptions=exceptions, evidence_roots=evidence_roots)
    dupes = find_dupes(rows)
    by_depth = collections.Counter(r["depth"] for r in rows)
    by_target = collections.Counter(r["target"] for r in rows)
    by_file = collections.Counter(os.path.basename(r["__file"]) for r in rows)
    with_lesson = sum(1 for r in rows if (r.get("lesson") or "").strip())
    return {
        "total": len(rows), "rejected": len(rejected), "excepted": len(excepted),
        "dupes": dupes, "by_depth": dict(by_depth), "targets": len(by_target),
        "top_targets": by_target.most_common(30), "by_wave_file": dict(by_file),
        "with_lesson": with_lesson, "rejects_sample": rejected[:20],
    }


def cmd_stats(args):
    root = repo_root()
    exceptions, roots = load_exceptions(args.exceptions, datetime.date.today())
    roots = roots + [safe_input_path(r, "--evidence-root")
                     for r in getattr(args, "evidence_root", None) or []]
    st = collect_stats(root, ledger_files(args, root),
                       exceptions=exceptions, evidence_roots=[root] + roots)
    if args.json:
        print(json.dumps(st, ensure_ascii=False, indent=1))
        return 0
    print(f"total={st['total']} rejected={st['rejected']} excepted={st['excepted']} "
          f"dupes={len(st['dupes'])} targets={st['targets']} with_lesson={st['with_lesson']}")
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


def render_report(files, st, as_of=None):
    """纯函数渲染（F06）：不含墙上时钟——同输入跨日期/跨目录字节一致。"""
    L = []
    L.append("# 账本统计（机器生成）")
    L.append("")
    L.append("- 性质：展示报告，非门禁（判定用 `ledger.py verify`）")
    if as_of:
        L.append(f"- as-of：{as_of}（显式给定）")
    L.append("- 复算命令：`python tools/ledger.py report --out docs/analysis/ledger-stats.md`")
    L.append(f"- 账本文件：{len(files)} 个 jsonl")
    L.append("")
    L.append(f"**总条目（通过校验）：{st['total']}**；被拒条目：{st['rejected']}；"
             f"例外豁免条目：{st['excepted']}；重复 id：{len(st['dupes'])}；"
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
    return "\n".join(L) + "\n"


def cmd_report(args):
    root = repo_root()
    files = ledger_files(args, root)
    if args.as_of and not check_date(args.as_of):
        print(f"FAIL: --as-of 非 ISO 日历日: {args.as_of}")
        return 1
    exceptions, roots = load_exceptions(args.exceptions, datetime.date.today())
    roots = ([root] + roots
             + [safe_input_path(r, "--evidence-root")
                for r in args.evidence_root or []])
    st = collect_stats(root, files, strict=args.strict, exceptions=exceptions,
                       evidence_roots=roots)
    out = safe_out_path(args.out, root)
    out.write_text(render_report(files, st, as_of=args.as_of), encoding="utf-8")
    print(f"written: {out}  (total={st['total']}, rejected={st['rejected']}, "
          f"excepted={st['excepted']}, dupes={len(st['dupes'])})")
    if args.strict:
        if args.min is not None and st["total"] < args.min:
            print(f"FAIL: 账本 {st['total']} 条 < --min {args.min}")
            return 2
        if st["rejected"] or st["dupes"]:
            print("FAIL: --strict 门禁口径下存在被拒条目或重复 id")
            return 1
    return 0


def cmd_gaps(args):
    """F07 迁移口径：基础契约已过、但未达目标契约的条目清单（人工补全队列，不编造证据）。"""
    root = repo_root()
    files = ledger_files(args, root)
    if not files:
        print("FAIL: 无账本文件")
        return 1
    _, roots = load_exceptions(args.exceptions, datetime.date.today())
    roots = [root] + roots + [safe_input_path(r, "--evidence-root")
                              for r in getattr(args, "evidence_root", None) or []]
    rows, rejected, _ = load_rows(files, root, evidence_roots=roots)
    gaps = collections.defaultdict(list)
    cache = {}
    for r in rows:
        for code, msg in check_row(r, root, cache, strict=True, exceptions=None,
                                   evidence_roots=roots):
            if code in WAIVABLE:
                gaps[code].append((r["id"], msg))
    total = sum(len(v) for v in gaps.values())
    print(f"gap entries: {total}（基础契约被拒 {len(rejected)} 条另计；按类："
          f"{', '.join(f'{k}={len(v)}' for k, v in sorted(gaps.items()))}）")
    if args.json:
        print(json.dumps({k: [i for i, _ in v] for k, v in gaps.items()},
                         ensure_ascii=False, indent=1))
    else:
        for code in sorted(gaps):
            print(f"--- {code}（{len(gaps[code])} 条）")
            for rid, msg in gaps[code][:10]:
                print(f"  {rid}  {msg[:100]}")
            if len(gaps[code]) > 10:
                print(f"  … 其余 {len(gaps[code]) - 10} 条见 --json")
    print("迁移口径：缺口走「例外登记（ci/ledger-exceptions.json：id/reason/expires）+ "
          "人工补全」，不许编造 quote、不许降门槛换绿。")
    return 0


def main():
    ap = argparse.ArgumentParser(description="LSSMJ 分析账本工具")
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("selftest")

    def common(p):
        p.add_argument("--ledger-dir", default=None)
        p.add_argument("--file", default=None)
        p.add_argument("--exceptions", default="ci/ledger-exceptions.json")
        p.add_argument("--evidence-root", dest="evidence_root", action="append", default=None)

    common(sub.add_parser("verify"))
    sub.choices["verify"].add_argument("--min", type=int, default=None)
    sub.choices["verify"].add_argument("--strict", action="store_true")

    common(sub.add_parser("gaps"))
    sub.choices["gaps"].add_argument("--json", action="store_true")

    common(sub.add_parser("stats"))
    sub.choices["stats"].add_argument("--json", action="store_true")

    pr = sub.add_parser("report")
    common(pr)
    pr.add_argument("--out", default="docs/analysis/ledger-stats.md")
    pr.add_argument("--as-of", dest="as_of", default=None)
    pr.add_argument("--strict", action="store_true")
    pr.add_argument("--min", type=int, default=None)

    args = ap.parse_args()
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "verify":
        return cmd_verify(args)
    if args.cmd == "gaps":
        return cmd_gaps(args)
    if args.cmd == "stats":
        return cmd_stats(args)
    if args.cmd == "report":
        return cmd_report(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
