# -*- coding: utf-8 -*-
"""ref_gate.py 单元测试（CI 中由 `python -m unittest discover -s tools/tests` 执行）。

覆盖面与 v3 审核 F08 的验收要求对应：
  - W1A / W15C（两位数波次）正常命中；
  - 悬空引用失败；重复 ID 失败；坏 JSON 行 fail-closed；
  - 例外必须显式、未过期；过期例外失败。
"""

import datetime
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ref_gate  # noqa: E402


def write(path: str, content: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(content)


class RefPatternTest(unittest.TestCase):
    def test_one_and_two_digit_waves_match(self):
        text = "见 W1A-001 与 W15C-002，另有 W3H-120。"
        self.assertEqual(
            ref_gate.REF_PATTERN.findall(text),
            ["W1A-001", "W15C-002", "W3H-120"],
        )

    def test_non_three_digit_serial_not_matched(self):
        # 账本现行 schema 为三位编号；放宽需同步版本化 schema 与本正则。
        self.assertEqual(ref_gate.REF_PATTERN.findall("W1A-1 W1A-0001x"), [])


class GateBehaviourTest(unittest.TestCase):
    def _run(self, ledger_rows, doc_text, exceptions=None, as_of=None):
        result = ref_gate.GateResult()
        with tempfile.TemporaryDirectory() as td:
            ldir = os.path.join(td, "ledger")
            ddir = os.path.join(td, "docs")
            write(os.path.join(ldir, "w.jsonl"), "\n".join(ledger_rows) + "\n")
            write(os.path.join(ddir, "a.md"), doc_text)
            ids = ref_gate.load_ledger_ids(ldir, result)
            exempt = set()
            if exceptions is not None:
                epath = os.path.join(td, "exceptions.json")
                write(epath, json.dumps(exceptions))
                exempt = ref_gate.load_exceptions(
                    epath, as_of or datetime.date(2026, 10, 8), result)
            ref_gate.scan_docs(ddir, ids, exempt, result)
        return result

    def test_good_refs_pass(self):
        r = self._run(
            [json.dumps({"id": "W1A-001"}), json.dumps({"id": "W15C-001"})],
            "正文引用 W1A-001 和 W15C-001。\n",
        )
        self.assertTrue(r.ok, r.violations)
        self.assertEqual(r.stats["refs_total"], 2)

    def test_dangling_ref_fails_with_reason(self):
        r = self._run([json.dumps({"id": "W1A-001"})], "引 W9Z-999。\n")
        self.assertFalse(r.ok)
        self.assertTrue(any("DANGLING_REF" in v and "W9Z-999" in v
                            for v in r.violations), r.violations)

    def test_duplicate_id_fails(self):
        row = json.dumps({"id": "W1A-001"})
        r = self._run([row, row], "引 W1A-001。\n")
        self.assertFalse(r.ok)
        self.assertTrue(any("DUPLICATE_ID" in v for v in r.violations))

    def test_broken_jsonl_fails_closed(self):
        r = self._run([json.dumps({"id": "W1A-001"}), "{bad"], "引 W1A-001。\n")
        self.assertFalse(r.ok)
        self.assertTrue(any("PARSE_ERROR" in v for v in r.violations))

    def test_missing_id_field_fails_closed(self):
        r = self._run([json.dumps({"depth": "doc"})], "无引用。\n")
        self.assertFalse(r.ok)
        self.assertTrue(any("PARSE_ERROR" in v for v in r.violations))

    def test_valid_exception_allows_ref(self):
        r = self._run(
            [json.dumps({"id": "W1A-001"})],
            "引 W9Z-999（已登记例外）。\n",
            exceptions={"schema_version": 1, "exceptions": [
                {"id": "W9Z-999", "reason": "历史迁移中，owner: maintainer",
                 "expires": "2027-01-01"}]},
            as_of=datetime.date(2026, 10, 8),
        )
        self.assertTrue(r.ok, r.violations)
        self.assertEqual(r.stats["exceptions_used"], 1)

    def test_expired_exception_fails(self):
        r = self._run(
            [json.dumps({"id": "W1A-001"})],
            "引 W9Z-999。\n",
            exceptions={"schema_version": 1, "exceptions": [
                {"id": "W9Z-999", "reason": "过期示例", "expires": "2026-01-01"}]},
            as_of=datetime.date(2026, 10, 8),
        )
        self.assertFalse(r.ok)
        self.assertTrue(any("EXPIRED_EXCEPTION" in v for v in r.violations))
        # 过期例外不得继续豁免：悬空仍要单独计violation
        self.assertTrue(any("DANGLING_REF" in v for v in r.violations))

    def test_exception_missing_fields_rejected(self):
        r = self._run(
            [json.dumps({"id": "W1A-001"})],
            "无引用。\n",
            exceptions={"schema_version": 1, "exceptions": [{"id": "W9Z-999"}]},
        )
        self.assertFalse(r.ok)
        self.assertTrue(any("CONFIG_ERROR" in v for v in r.violations))

    def test_empty_ledger_dir_fails(self):
        result = ref_gate.GateResult()
        with tempfile.TemporaryDirectory() as td:
            ldir = os.path.join(td, "ledger")
            os.makedirs(ldir)
            ref_gate.load_ledger_ids(ldir, result)
        self.assertFalse(result.ok)
        self.assertTrue(any("EMPTY_LEDGER" in v for v in result.violations))


class SelftestTest(unittest.TestCase):
    def test_selftest_passes(self):
        self.assertEqual(ref_gate.cmd_selftest(None), 0)


if __name__ == "__main__":
    unittest.main()
