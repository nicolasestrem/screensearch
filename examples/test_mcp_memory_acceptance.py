#!/usr/bin/env python3
"""Focused unit regressions for the saved-report validation logic in the acceptance driver.

These exercise the *pure* validators and the manifest parser in `mcp_memory_acceptance.py`
against independently spec-shaped values (03 §7f.5/§7f.6/§8c). They launch no MCP binary and
make no HTTP request, so they are **not** producer acceptance and prove nothing about the live
API/MCP surface — they only pin the driver's own field/type/null/enum/order/overlap logic.
Run: `python3 -m unittest examples/test_mcp_memory_acceptance.py` (or `python3 examples/...py`).
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import unittest

_DRIVER_PATH = Path(__file__).with_name("mcp_memory_acceptance.py")
_spec = importlib.util.spec_from_file_location("mcp_memory_acceptance", _DRIVER_PATH)
assert _spec is not None and _spec.loader is not None
driver = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(driver)

START = 1700000000000
END = 1700003600000


def _summary(**overrides):
    base = {
        "run_id": "run-1",
        "source_id": "src-1",
        "revision_of": None,
        "requested_range": None,
        "window_start": START + 1000,
        "window_end": END - 1000,
        "timezone": "Europe/Paris",
        "local_date": "2023-11-14",
        "created_at": START + 5000,
        "updated_at": START + 6000,
        "phase": "finished",
        "outcome": "OK",
        "error_code": None,
    }
    base.update(overrides)
    return base


def _saved(**overrides):
    base = {
        "markdown": "# Report\nbody text",
        "model": "qwen2.5",
        "frame_ids": [1, 2],
        "session_ids": [7],
        "cited_frame_ids": [1],
        "frames_sampled": 2,
        "frames_summarized": 2,
        "periods_total": 1,
        "periods_covered": 1,
        "passes": 1,
        "truncated": False,
    }
    base.update(overrides)
    return base


def _expected_saved(**overrides):
    saved = _saved(**overrides)
    saved["markdown_sha256"] = driver.sha256_hex(saved.pop("markdown"))
    return saved


def _detail(result, **overrides):
    base = _summary(**overrides)
    base["result"] = result
    return base


def _expected_detail(result, **overrides):
    base = _summary(**overrides)
    base["result"] = result
    return base


def _fixture_doc(**overrides):
    doc = {
        "history_id": "unit",
        "window": {"from": START, "to": END},
        "frames": [
            {
                "frame_id": 1,
                "captured_at": START + 1000,
                "raw_text_sha256": driver.sha256_hex("alpha"),
                "text_state": "available",
            }
        ],
    }
    doc.update(overrides)
    return doc


class SummaryContract(unittest.TestCase):
    def test_accepts_exact_spec_shape(self):
        normalized = driver.normalize_report_summary(_summary(), START, END)
        self.assertEqual(set(normalized), set(driver.REPORT_SUMMARY_FIELDS))

    def test_accepts_requested_range(self):
        normalized = driver.normalize_report_summary(
            _summary(requested_range={"start_date": "2023-11-14", "end_date": "2023-11-15"}),
            START,
            END,
        )
        self.assertEqual(normalized["requested_range"], {"start_date": "2023-11-14", "end_date": "2023-11-15"})

    def test_rejects_extra_field(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(_summary(markdown="# leak"), START, END)

    def test_rejects_missing_field(self):
        bad = _summary()
        del bad["outcome"]
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(bad, START, END)

    def test_rejects_bool_as_int(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(_summary(window_start=True), START, END)

    def test_rejects_unknown_phase(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(_summary(phase="draining"), START, END)

    def test_rejects_unknown_error_code(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(_summary(error_code="LEAKED_MESSAGE"), START, END)

    def test_rejects_non_date_local_date(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(_summary(local_date="2023/11/14"), START, END)

    def test_rejects_out_of_window(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_summary(
                _summary(window_start=START - 9000, window_end=START - 1000), START, END
            )

    def test_accepts_queued_without_outcome(self):
        normalized = driver.normalize_report_summary(
            _summary(phase="queued", outcome=None), START, END
        )
        self.assertIsNone(normalized["outcome"])


class PageContract(unittest.TestCase):
    def test_terminal_null_cursor(self):
        items, cursor = driver.verify_report_page(
            {"items": [_summary()], "next_cursor": None}, START, END
        )
        self.assertIsNone(cursor)
        self.assertEqual(len(items), 1)

    def test_rejects_extra_page_field(self):
        with self.assertRaises(driver.ProtocolError):
            driver.verify_report_page(
                {"items": [_summary()], "next_cursor": None, "total": 1}, START, END
            )

    def test_rejects_empty_cursor(self):
        with self.assertRaises(driver.ProtocolError):
            driver.verify_report_page({"items": [_summary()], "next_cursor": ""}, START, END)

    def test_rejects_oversized_cursor(self):
        with self.assertRaises(driver.ProtocolError):
            driver.verify_report_page(
                {"items": [_summary()], "next_cursor": "c" * 4097}, START, END
            )

    def test_enforces_descending_order(self):
        older = _summary(run_id="run-2", created_at=START + 100)
        newer = _summary(run_id="run-1", created_at=START + 200)
        with self.assertRaises(driver.ProtocolError):
            driver.verify_report_page({"items": [older, newer], "next_cursor": None}, START, END)

    def test_rejects_equal_keys(self):
        with self.assertRaises(driver.ProtocolError):
            driver.verify_report_page(
                {"items": [_summary(), _summary()], "next_cursor": None}, START, END
            )


class DetailContract(unittest.TestCase):
    def test_accepts_null_result(self):
        normalized = driver.normalize_report_detail(_detail(None), START, END)
        self.assertIsNone(normalized["result"])

    def test_accepts_full_saved_result(self):
        normalized = driver.normalize_report_detail(_detail(_saved()), START, END)
        self.assertEqual(normalized["result"]["frame_ids"], [1, 2])
        self.assertFalse(normalized["result"]["truncated"])

    def test_rejects_wrong_saved_field(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_detail(_detail(_saved(extra=1)), START, END)

    def test_rejects_negative_count(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_detail(_detail(_saved(passes=-1)), START, END)

    def test_rejects_non_bool_truncated(self):
        with self.assertRaises(driver.ProtocolError):
            driver.normalize_report_detail(_detail(_saved(truncated=0)), START, END)

    def test_expected_detail_requires_digest_not_markdown(self):
        with self.assertRaises(driver.InputError):
            driver.normalize_expected_report_detail(_detail(_saved()), START, END)
        normalized = driver.normalize_expected_report_detail(
            _expected_detail(_expected_saved()), START, END
        )
        self.assertIn("markdown_sha256", normalized["result"])


class CompareContract(unittest.TestCase):
    def _round_trip(self):
        expected = driver.normalize_expected_report_detail(_expected_detail(_expected_saved()), START, END)
        observed = driver.normalize_report_detail(_detail(_saved()), START, END)
        return observed, expected

    def test_matching_detail_has_no_mismatch(self):
        observed, expected = self._round_trip()
        mismatches = []
        driver.compare_report_detail(observed, expected, mismatches)
        self.assertEqual(mismatches, [])

    def test_digest_mismatch_detected_without_leaking_text(self):
        observed, expected = self._round_trip()
        observed["result"]["markdown"] = "# tampered"
        mismatches = []
        driver.compare_report_detail(observed, expected, mismatches)
        self.assertIn("result.markdown_sha256", {m["field"] for m in mismatches})
        self.assertNotIn("tampered", json.dumps(mismatches))

    def test_provenance_mismatch_detected(self):
        observed, expected = self._round_trip()
        observed["result"]["frame_ids"] = [1]
        mismatches = []
        driver.compare_report_detail(observed, expected, mismatches)
        self.assertIn("result.frame_ids", {m["field"] for m in mismatches})

    def test_phase_suppression_detected(self):
        expected = driver.normalize_expected_report_detail(
            _expected_detail(_expected_saved(), phase="queued", outcome=None), START, END
        )
        observed = driver.normalize_report_detail(_detail(None, phase="queued", outcome=None), START, END)
        mismatches = []
        driver.compare_report_detail(observed, expected, mismatches)
        self.assertIn("result.present", {m["field"] for m in mismatches})


class ManifestContract(unittest.TestCase):
    def _block(self):
        return {
            "expected_summaries": [_summary(run_id="run-1", created_at=START + 5000)],
            "expected_details": [
                _expected_detail(_expected_saved(frame_ids=[1]), run_id="run-1", created_at=START + 5000)
            ],
        }

    def test_accepts_corresponding_block(self):
        summaries, details = driver.normalize_expected_reports(self._block(), START, END, {1})
        self.assertEqual([s["run_id"] for s in summaries], [d["run_id"] for d in details])

    def test_rejects_run_id_mismatch(self):
        block = self._block()
        block["expected_details"][0]["run_id"] = "run-2"
        with self.assertRaises(driver.InputError):
            driver.normalize_expected_reports(block, START, END, {1})

    def test_rejects_ascending_order(self):
        block = self._block()
        block["expected_summaries"] = [
            _summary(run_id="run-1", created_at=START + 100),
            _summary(run_id="run-2", created_at=START + 200),
        ]
        block["expected_details"] = [
            _expected_detail(_expected_saved(frame_ids=[1]), run_id="run-1", created_at=START + 100),
            _expected_detail(_expected_saved(frame_ids=[1]), run_id="run-2", created_at=START + 200),
        ]
        with self.assertRaises(driver.InputError):
            driver.normalize_expected_reports(block, START, END, {1})

    def test_requires_resolvable_saved_frame(self):
        block = self._block()
        block["expected_details"][0]["result"]["frame_ids"] = [999]
        with self.assertRaises(driver.InputError):
            driver.normalize_expected_reports(block, START, END, {1})


class FixtureReportsParsing(unittest.TestCase):
    def test_absent_reports_is_none(self):
        fixture = driver.Fixture(_fixture_doc(), "unit")
        self.assertIsNone(fixture.reports)

    def test_present_reports_parsed(self):
        fixture = driver.Fixture(_fixture_doc(reports=ManifestContract()._block()), "unit")
        self.assertIsNotNone(fixture.reports)
        self.assertEqual(len(fixture.reports["expected_summaries"]), 1)

    def test_non_object_reports_rejected(self):
        with self.assertRaises(driver.InputError):
            driver.Fixture(_fixture_doc(reports=[]), "unit")

    def test_empty_summaries_rejected(self):
        with self.assertRaises(driver.InputError):
            driver.Fixture(
                _fixture_doc(reports={"expected_summaries": [], "expected_details": []}), "unit"
            )

    def test_length_mismatch_rejected(self):
        block = ManifestContract()._block()
        block["expected_details"] = []
        with self.assertRaises(driver.InputError):
            driver.Fixture(_fixture_doc(reports=block), "unit")


class SurfaceContract(unittest.TestCase):
    def test_report_tools_follow_coverage(self):
        order = driver.READ_TOOL_ORDER
        self.assertEqual(order.index("list_reports"), order.index("get_capture_coverage") + 1)
        self.assertEqual(order.index("get_report"), order.index("list_reports") + 1)

    def test_summary_field_tuple_is_frozen(self):
        self.assertEqual(
            driver.REPORT_SUMMARY_FIELDS,
            (
                "run_id",
                "source_id",
                "revision_of",
                "requested_range",
                "window_start",
                "window_end",
                "timezone",
                "local_date",
                "created_at",
                "updated_at",
                "phase",
                "outcome",
                "error_code",
            ),
        )

    def test_error_codes_match_section_8c(self):
        self.assertEqual(len(driver.REPORT_ERROR_CODES), 11)


if __name__ == "__main__":
    unittest.main(verbosity=2)
