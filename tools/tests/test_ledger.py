# -*- coding: utf-8 -*-
"""ledger.py 单元测试（CI 中由 `python -m unittest discover -s tools/tests` 执行）。

对应 v5 审核 F06（报告可复现）与 F07（校验器契约）的验收要求：
类型严格 / ISO 日期 / 行号范围 / `..` 拒绝 / 证据根限定 / 无损解码 /
例外（含过期）/ 空输入 fail-closed / 报告无墙上时钟。
"""

import datetime
import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ledger  # noqa: E402


def write(path: str, content: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    Path(path).write_text(content, encoding="utf-8")


def write_bytes(path: str, content: bytes) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    Path(path).write_bytes(content)


class DateCheckTest(unittest.TestCase):
    def test_valid_iso(self):
        self.assertTrue(ledger.check_date("2026-10-01"))

    def test_bad_format_rejected(self):
        self.assertFalse(ledger.check_date("2026/10/01"))
        self.assertFalse(ledger.check_date("20261001"))
        self.assertFalse(ledger.check_date(""))

    def test_bad_calendar_rejected(self):
        # 格式对但日历错：2026-13-01 / 02-30
        self.assertFalse(ledger.check_date("2026-13-01"))
        self.assertFalse(ledger.check_date("2026-02-30"))


class CheckRowTest(unittest.TestCase):
    def setUp(self):
        self.root = ledger.repo_root()
        self.cache = {}
        # 锚目标 = 本测试文件自身（行数与内容确定）
        self.self_path = os.path.abspath(__file__)
        self.n_lines = len(ledger.read_lines(self.self_path))
        self.good = {
            "id": "T-1", "depth": "source", "target": "t",
            "anchor": f"{self.self_path}:1",
            "quote": "# -*- coding: utf-8 -*-",
            "finding": "测试正例：合法 source 条目应通过校验" + "x" * 10,
            "date": "2026-10-01",
        }

    def test_good_passes(self):
        self.assertEqual(ledger.check_row(self.good, self.root, self.cache), [])

    def test_quote_non_string_clean_reject(self):
        # F07：quote 非字符串给逐条拒绝原因，不再 TypeError
        errs = ledger.check_row({**self.good, "quote": {"k": 1}}, self.root, self.cache)
        self.assertTrue(any("quote 必须是字符串" in m for _, m in errs))

    def test_line_zero_rejected(self):
        errs = ledger.check_row({**self.good, "anchor": f"{self.self_path}:0"},
                                self.root, self.cache)
        self.assertTrue(any("行号必须 ≥ 1" in m for _, m in errs))

    def test_line_past_eof_rejected(self):
        anchor = f"{self.self_path}:{self.n_lines + 1}"
        errs = ledger.check_row({**self.good, "anchor": anchor}, self.root, self.cache)
        self.assertTrue(any("超出文件长度" in m for _, m in errs))

    def test_dotdot_rejected_even_if_resolves_inside(self):
        anchor = f"{self.root}/../LSSMJ/tools/ledger.py:1"
        errs = ledger.check_row({**self.good, "anchor": anchor}, self.root, self.cache)
        self.assertTrue(any("穿越段" in m for _, m in errs))

    def test_outside_evidence_roots_rejected_and_allowlist_honored(self):
        with tempfile.TemporaryDirectory() as td:
            anchor_file = os.path.join(td, "a.py")
            write_bytes(anchor_file, b"hello_anchor_line\n")
            row = {**self.good, "anchor": f"{anchor_file}:1",
                   "quote": "hello_anchor_line"}
            errs = ledger.check_row(row, self.root, self.cache)
            self.assertTrue(any("不在允许证据根内" in m for _, m in errs))
            errs2 = ledger.check_row(row, self.root, self.cache, evidence_roots=[td])
            self.assertEqual(errs2, [])

    def test_lossless_decode_required(self):
        with tempfile.TemporaryDirectory() as td:
            lossy = os.path.join(td, "lossy.py")
            write_bytes(lossy, b"# \xff\xfe\x81\x81 not utf8 not gbk\n")
            errs = ledger.check_row({**self.good, "anchor": f"{lossy}:1"},
                                    self.root, self.cache, evidence_roots=[td])
            self.assertTrue(any("无法无损解码" in m for _, m in errs))

    def test_nonsource_quote_only_in_strict(self):
        row = {**self.good, "depth": "doc",
               "anchor": "https://example.com/docs", "quote": None}
        base = ledger.check_row(row, self.root, self.cache)
        self.assertEqual(base, [])
        strict = ledger.check_row(row, self.root, self.cache, strict=True)
        self.assertTrue(any("NONSOURCE" in c or "quote" in m for c, m in strict))

    def test_exception_waives_and_marks(self):
        with tempfile.TemporaryDirectory() as td:
            anchor_file = os.path.join(td, "a.py")
            write_bytes(anchor_file, b"hello_anchor_line\n")
            row = {**self.good, "id": "X-1", "anchor": f"{anchor_file}:1"}
            errs = ledger.check_row(row, self.root, self.cache, strict=True,
                                    exceptions={"X-1": ("迁移期例外", "2099-01-01")})
            self.assertTrue(all(c.startswith("EXEMPT_") for c, _ in errs))
            # 例外只豁免迁移类：基础违规（引文对不上）不豁免
            bad = {**self.good, "id": "X-2",
                   "quote": "这段引文不在文件里-负例-2026"}
            errs2 = ledger.check_row(bad, self.root, self.cache,
                                     exceptions={"X-2": ("x", "2099-01-01")})
            self.assertTrue(any(c == "QUOTE_MISMATCH" for c, _ in errs2))


class ExceptionsTest(unittest.TestCase):
    def test_expired_filtered(self):
        with tempfile.TemporaryDirectory() as td:
            p = os.path.join(td, "e.json")
            Path(p).write_text(json.dumps({"schema_version": 1, "exceptions": [
                {"id": "A", "reason": "过期", "expires": "2020-01-01"},
                {"id": "B", "reason": "有效", "expires": "2099-01-01"},
            ]}), encoding="utf-8")
            exc, _ = ledger.load_exceptions(p, datetime.date(2026, 10, 11))
            self.assertNotIn("A", exc)
            self.assertIn("B", exc)

    def test_malformed_raises(self):
        with tempfile.TemporaryDirectory() as td:
            p = os.path.join(td, "e.json")
            Path(p).write_text(json.dumps({"exceptions": [{"id": "A"}]}), encoding="utf-8")
            with self.assertRaises(SystemExit):
                ledger.load_exceptions(p, datetime.date(2026, 10, 11))

    def test_evidence_roots_parsed(self):
        with tempfile.TemporaryDirectory() as td:
            p = os.path.join(td, "e.json")
            Path(p).write_text(json.dumps(
                {"schema_version": 1, "evidence_roots": ["D:/KF/BSHSQ"],
                 "exceptions": []}), encoding="utf-8")
            _, roots = ledger.load_exceptions(p, datetime.date(2026, 10, 11))
            self.assertEqual(roots, [os.path.abspath("D:/KF/BSHSQ")])


class FailClosedTest(unittest.TestCase):
    def test_empty_ledger_dir_verify_fails(self):
        with tempfile.TemporaryDirectory() as td:
            args = types.SimpleNamespace(ledger_dir=td, file=None, min=None,
                                         strict=False, exceptions=None,
                                         evidence_root=None)
            self.assertEqual(ledger.cmd_verify(args), 1)

    def test_missing_file_arg_fails_clean(self):
        args = types.SimpleNamespace(ledger_dir=None, file="Z:/no/such.jsonl",
                                     min=None, strict=False, exceptions=None,
                                     evidence_root=None)
        # 不存在的 --file：明确失败而非 traceback（FileNotFoundError 属于失败路径）
        try:
            rc = ledger.cmd_verify(args)
            self.assertEqual(rc, 1)
        except OSError:
            pass  # 允许 OSError 形式的明确失败，但不允许静默 0


class ReportDeterminismTest(unittest.TestCase):
    def test_render_has_no_wall_clock_and_is_stable(self):
        st = {"total": 2, "rejected": 0, "excepted": 0, "dupes": [],
              "by_depth": {"source": 2}, "targets": 1,
              "top_targets": [("t", 2)], "by_wave_file": {"w.jsonl": 2},
              "with_lesson": 1, "rejects_sample": []}
        r1 = ledger.render_report(["a.jsonl"], st)
        r2 = ledger.render_report(["a.jsonl"], st)
        self.assertEqual(r1, r2)
        today = datetime.date.today().isoformat()
        self.assertNotIn(today, r1)  # F06：不含墙上时钟日期
        self.assertNotIn("生成日期", r1)
        r3 = ledger.render_report(["a.jsonl"], st, as_of="2026-10-01")
        self.assertIn("as-of：2026-10-01", r3)


class SafeInputPathTest(unittest.TestCase):
    def test_dotdot_rejected(self):
        with self.assertRaises(SystemExit):
            ledger.safe_input_path("../evil.jsonl", "--file")

    def test_normal_path_passthrough(self):
        self.assertEqual(ledger.safe_input_path("tools/ledger.py", "--file"),
                         os.path.abspath("tools/ledger.py"))


class DupesTest(unittest.TestCase):
    def test_find_dupes_sorted(self):
        rows = [{"id": "B"}, {"id": "A"}, {"id": "B"}, {"id": "B"}]
        self.assertEqual(ledger.find_dupes(rows), ["B"])


if __name__ == "__main__":
    unittest.main()
