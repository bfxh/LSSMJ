#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ref_gate.py —— 文档→账本引用完整性门（LSSMJ CI 阶段 1）。

职责（刻意收窄，不冒充其它门）：
  1. 从 docs/analysis/ledger/*.jsonl 加载全部条目 ID（JSON 解析失败 = 门失败，fail-closed）。
  2. 检查账本内 ID 全局唯一（重复 = 失败）。
  3. 扫描 docs/**/*.md 中形如 W<数字>+[字母]?-<三位数字> 的引用（正则允许多位波次，
     修复 v3 审核发现 F08：规格正则 \\bW\\d[A-Z]?-\\d{3}\\b 漏检 W15C 等两位数波次）。
  4. 每个引用必须能解析到账本中的真实 ID；悬空引用 = 失败。
  5. 例外必须显式登记在 --exceptions 文件中（含 id/reason/expires），过期例外 = 失败。

本门 **不** 验证：锚文件存在性/逐字引文（那是 ledger.py verify + 证据恢复的职责）、
Markdown 相对链接与标题锚（另设 doc_gate/link_probe）、外部 URL 存活。

退出码：0 = 通过；1 = 发现违规（悬空/重复/解析失败/过期例外）；2 = 用法或配置错误。
"""

from __future__ import annotations

import argparse
import datetime
import glob
import json
import os
import re
import sys
import tempfile

# 允许多位波次数字（W1A-001 与 W15C-001 都必须命中）；
# 编号当前固定三位，与账本既有 schema 一致。若未来放宽，需同步版本化本正则与账本规则。
REF_PATTERN = re.compile(r"\bW\d+[A-Z]?-\d{3}\b")

MD_SUFFIX = ".md"


def repo_root() -> str:
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def default_ledger_dir() -> str:
    return os.path.join(repo_root(), "docs", "analysis", "ledger")


def default_docs_dir() -> str:
    return os.path.join(repo_root(), "docs")


class GateResult:
    """统一收集违规，保证输出与退出码一致、顺序确定。"""

    def __init__(self) -> None:
        self.violations: list[str] = []
        self.stats: dict[str, int] = {
            "ledger_files": 0,
            "ledger_ids": 0,
            "doc_files": 0,
            "refs_total": 0,
            "refs_unique": 0,
            "dangling_unique": 0,
            "duplicates": 0,
            "parse_errors": 0,
            "exceptions_used": 0,
        }

    def add(self, kind: str, message: str) -> None:
        self.violations.append(f"{kind}: {message}")

    @property
    def ok(self) -> bool:
        return not self.violations


def load_ledger_ids(ledger_dir: str, result: GateResult) -> set[str]:
    ids: set[str] = set()
    files = sorted(glob.glob(os.path.join(ledger_dir, "*.jsonl")))
    if not files:
        result.add("EMPTY_LEDGER", f"账本目录无 .jsonl 文件: {ledger_dir}")
        return ids
    result.stats["ledger_files"] = len(files)
    for path in files:
        rel = os.path.relpath(path, repo_root()).replace("\\", "/")
        try:
            with open(path, encoding="utf-8") as fh:
                for lineno, line in enumerate(fh, 1):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        row = json.loads(line)
                    except json.JSONDecodeError as exc:
                        result.stats["parse_errors"] += 1
                        result.add("PARSE_ERROR", f"{rel}:{lineno} JSON 解析失败: {exc}")
                        continue
                    rid = row.get("id")
                    if not isinstance(rid, str) or not rid:
                        result.stats["parse_errors"] += 1
                        result.add("PARSE_ERROR", f"{rel}:{lineno} 缺少字符串 id 字段")
                        continue
                    if rid in ids:
                        result.stats["duplicates"] += 1
                        result.add("DUPLICATE_ID", f"{rel}:{lineno} 重复 ID: {rid}")
                        continue
                    ids.add(rid)
        except UnicodeDecodeError as exc:
            result.stats["parse_errors"] += 1
            result.add("PARSE_ERROR", f"{rel} 非 UTF-8，拒绝有损解码: {exc}")
    result.stats["ledger_ids"] = len(ids)
    return ids


def load_exceptions(path: str | None, as_of: datetime.date,
                    result: GateResult) -> set[str]:
    """例外文件格式：{"schema_version":1,"exceptions":[{"id","reason","expires"}]}。
    缺字段/过期 = 违规。文件不存在且未显式指定 = 无例外。"""
    exempt: set[str] = set()
    if path is None:
        return exempt
    if not os.path.exists(path):
        result.add("CONFIG_ERROR", f"例外文件不存在: {path}")
        return exempt
    try:
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        result.add("CONFIG_ERROR", f"例外文件解析失败: {path}: {exc}")
        return exempt
    for i, entry in enumerate(data.get("exceptions", [])):
        rid = entry.get("id")
        reason = entry.get("reason")
        expires = entry.get("expires")
        if not (isinstance(rid, str) and rid and isinstance(reason, str) and reason
                and isinstance(expires, str)):
            result.add("CONFIG_ERROR", f"例外[{i}] 缺 id/reason/expires 字段")
            continue
        try:
            exp = datetime.date.fromisoformat(expires)
        except ValueError:
            result.add("CONFIG_ERROR", f"例外[{i}] expires 非 ISO 日期: {expires}")
            continue
        if exp < as_of:
            result.add("EXPIRED_EXCEPTION", f"例外 {rid} 已于 {expires} 过期（owner 需续期或清理）")
            continue
        exempt.add(rid)
    return exempt


def scan_docs(docs_dir: str, ids: set[str], exempt: set[str],
              result: GateResult) -> None:
    dangling: dict[str, list[str]] = {}
    seen: set[str] = set()
    doc_files = 0
    refs_total = 0
    for root, dirs, files in os.walk(docs_dir):
        dirs.sort()
        for fn in sorted(files):
            if not fn.endswith(MD_SUFFIX):
                continue
            doc_files += 1
            path = os.path.join(root, fn)
            rel = os.path.relpath(path, repo_root()).replace("\\", "/")
            try:
                with open(path, encoding="utf-8") as fh:
                    for lineno, line in enumerate(fh, 1):
                        for ref in REF_PATTERN.findall(line):
                            refs_total += 1
                            seen.add(ref)
                            if ref in ids or ref in exempt:
                                if ref in exempt and ref not in ids:
                                    result.stats["exceptions_used"] += 1
                                continue
                            dangling.setdefault(ref, []).append(f"{rel}:{lineno}")
            except UnicodeDecodeError as exc:
                result.add("PARSE_ERROR", f"{rel} 非 UTF-8，拒绝有损解码: {exc}")
    result.stats["doc_files"] = doc_files
    result.stats["refs_total"] = refs_total
    result.stats["refs_unique"] = len(seen)
    result.stats["dangling_unique"] = len(dangling)
    for ref in sorted(dangling):
        locs = dangling[ref]
        shown = ", ".join(locs[:3]) + (f" …(+{len(locs) - 3})" if len(locs) > 3 else "")
        result.add("DANGLING_REF", f"{ref} 无对应账本条目: {shown}")


def cmd_check(args: argparse.Namespace) -> int:
    result = GateResult()
    as_of = (datetime.date.fromisoformat(args.as_of)
             if args.as_of else datetime.date.today())
    ids = load_ledger_ids(args.ledger_dir, result)
    exempt = load_exceptions(args.exceptions, as_of, result)
    if not os.path.isdir(args.docs_dir):
        result.add("CONFIG_ERROR", f"docs 目录不存在: {args.docs_dir}")
    else:
        scan_docs(args.docs_dir, ids, exempt, result)
    for v in result.violations:
        print(f"REJECT {v}")
    s = result.stats
    print(
        "ref_gate: ledger_files={ledger_files} ledger_ids={ledger_ids} "
        "doc_files={doc_files} refs_total={refs_total} refs_unique={refs_unique} "
        "dangling={dangling_unique} duplicates={duplicates} "
        "parse_errors={parse_errors} exceptions_used={exceptions_used}".format(**s)
    )
    if result.ok:
        print("ref_gate: PASS")
        return 0
    print(f"ref_gate: FAIL ({len(result.violations)} violations)")
    return 1


def _write(path: str, content: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(content)


def cmd_selftest(_args: argparse.Namespace) -> int:
    """内置正/负例：好仓库必须绿，坏引用/重复 ID 必须红，且红在正确原因上。"""
    failures: list[str] = []

    def run_case(name: str, ledger_rows: list[str], doc_text: str,
                 expect_exit: int, expect_marker: str | None) -> None:
        with tempfile.TemporaryDirectory() as td:
            ldir = os.path.join(td, "ledger")
            ddir = os.path.join(td, "docs")
            _write(os.path.join(ldir, "w.jsonl"), "\n".join(ledger_rows) + "\n")
            _write(os.path.join(ddir, "a.md"), doc_text)
            result = GateResult()
            ids = load_ledger_ids(ldir, result)
            scan_docs(ddir, ids, set(), result)
            code = 0 if result.ok else 1
            joined = "\n".join(result.violations)
            if code != expect_exit:
                failures.append(f"{name}: exit={code} 期望={expect_exit}")
            elif expect_marker and expect_marker not in joined:
                failures.append(f"{name}: 失败原因不含 {expect_marker}: {joined!r}")

    row1 = json.dumps({"id": "W1A-001", "depth": "doc"})
    row15 = json.dumps({"id": "W15C-001", "depth": "source"})
    # 正例：单数字与两位数字波次都能命中并解析（F08 回归）。
    run_case("good-two-digit-wave", [row1, row15],
             "引 W1A-001 与 W15C-001。\n", 0, None)
    # 负例：悬空引用必须红，且红因是 DANGLING_REF。
    run_case("dangling-ref", [row1], "引 W9Z-999 不存在。\n", 1, "DANGLING_REF")
    # 负例：重复 ID 必须红。
    run_case("duplicate-id", [row1, row1], "引 W1A-001。\n", 1, "DUPLICATE_ID")
    # 负例：账本 JSON 坏行必须红（fail-closed），不得静默跳过。
    run_case("broken-jsonl", [row1, "{not json"], "引 W1A-001。\n", 1, "PARSE_ERROR")

    if failures:
        for f in failures:
            print(f"selftest FAIL: {f}")
        return 1
    print("selftest: good=PASS bad=PASS(red on dangling/duplicate/parse)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    pc = sub.add_parser("check", help="检查 docs 引用均能解析到账本 ID")
    pc.add_argument("--docs-dir", default=default_docs_dir())
    pc.add_argument("--ledger-dir", default=default_ledger_dir())
    pc.add_argument("--exceptions", default=None,
                    help="显式例外登记文件（JSON）；不提供则无例外")
    pc.add_argument("--as-of", default=None,
                    help="例外过期判定日期 YYYY-MM-DD（默认今天；仅影响例外，不影响扫描）")

    sub.add_parser("selftest", help="内置正负例自测（坏引用必须红在正确原因上）")

    args = parser.parse_args(argv)
    if args.cmd == "check":
        return cmd_check(args)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
