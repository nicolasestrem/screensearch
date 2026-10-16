"""Local SQLite tests; live API tests are an explicitly separate, opt-in gate."""
import json
import os
from pathlib import Path
import queue
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest

import memory_consumer as consumer


# Nonprivate wire-shaped records exercise local persistence only. No HTTP server,
# mock transport or canned API response is used to claim integration acceptance.
def evidence(frame_id, captured_at, text=None):
    return dict(frame_id=frame_id, captured_at=captured_at, monitor_index=0,
                app_hint="fixture", window_title="nonprivate", browser_url=None,
                raw_text=text, content_text=text, text_source="ocr" if text is not None else None,
                filter_version=1 if text is not None else None,
                text_state="available" if text is not None else "not_available",
                vision_description=None, session_id=None, image_purged=True)


def page(items, cursor=None, maximum=100, history="fixture-history"):
    return dict(items=items, next_cursor=cursor, history_id=history,
                window={"from": 0, "to": 1000}, snapshot_max_frame_id=maximum,
                observed_at=900, consistency="bounded_membership_live_content")


class WindowTests(unittest.TestCase):
    def test_explicit_utc_milliseconds(self):
        self.assertEqual(consumer.parse_utc_ms("2026-01-01T00:00:00.001Z"), 1767225600001)
        self.assertEqual(consumer.parse_utc_ms("1767225600001"), 1767225600001)
        for invalid in ("2026-01-01", "2026-01-01T00:00:00", "1.5", "NaN",
                        "2026-01-01T00:00:00+01:00", "2026-01-01T00:00:00.0001Z"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                consumer.parse_utc_ms(invalid)

    def test_explicit_iana_timezone_handles_both_dst_transitions(self):
        spring = consumer.day_window("2026-03-29", "Europe/Paris")
        autumn = consumer.day_window("2026-10-25", "Europe/Paris")
        self.assertEqual(spring[1] - spring[0], 23 * 60 * 60 * 1000)
        self.assertEqual(autumn[1] - autumn[0], 25 * 60 * 60 * 1000)
        self.assertEqual(consumer.day_window("2026-01-01", "UTC"),
                         (1767225600000, 1767312000000))
        with self.assertRaises(ValueError):
            consumer.day_window("2026-01-01", "")

    def test_invalid_destinations_are_rejected(self):
        for url in ("http://user:secret@localhost", "http://localhost/path", "http://0.0.0.0",
                    "http://[::]", "http://host?secret=yes", "ftp://host", "http://host/#x"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                consumer.validate_api_url(url)
        self.assertEqual(consumer.validate_api_url("https://localhost:43210/"),
                         "https://localhost:43210")


class StateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR"))
        self.path = Path(self.tmp.name) / "consumer.sqlite3"
        self.state = self.open_state()

    def open_state(self):
        return consumer.State(self.path, "http://127.0.0.1:43210", 0, 1000)

    def tearDown(self):
        self.state.close()
        self.tmp.cleanup()

    def test_reference_consumer_resumes_without_losing_committed_pages(self):
        self.state.observe_high_water(100)
        self.assertTrue(self.state.commit_page(page([evidence(1, 10, "full raw text")], "next")))
        self.state.close()
        self.state = self.open_state()
        self.assertEqual(self.state.checkpoint()["cursor"], "next")
        self.assertEqual(self.state.fact("fixture-history", 1)["raw_text"], "full raw text")
        self.assertTrue(self.state.commit_page(page([evidence(2, 10, "second")], None)))
        self.assertTrue(self.state.checkpoint()["done"])
        self.assertEqual(self.state.count_facts(), 2)

    def test_page_upsert_and_checkpoint_rollback_together(self):
        self.state.commit_page(page([evidence(1, 10)], "next"))
        self.state.db.execute("""CREATE TRIGGER reject_fixture BEFORE INSERT ON facts
                              WHEN NEW.frame_id = 3 BEGIN SELECT RAISE(ABORT, 'fixture failure'); END""")
        self.state.db.commit()
        with self.assertRaises(sqlite3.IntegrityError):
            self.state.commit_page(page([evidence(2, 20), evidence(3, 30)], "later"))
        self.assertEqual(self.state.checkpoint()["cursor"], "next")
        self.assertIsNone(self.state.fact("fixture-history", 2))
        self.assertEqual(self.state.count_facts(), 1)

    def test_consumer_rereads_text_after_checkpoint(self):
        self.state.commit_page(page([evidence(1, 10)], "next"))
        self.assertEqual(self.state.textless_ids(), [1])
        original = self.state.original_fact("fixture-history", 1)
        detail = dict(frame_id=1, captured_at=10, raw_text="raw " * 5000,
                      content_text="content " * 4000, text_source="ocr", vision=None)
        self.assertTrue(self.state.commit_detail(1, detail))
        self.assertEqual(self.state.fact("fixture-history", 1)["raw_text"], detail["raw_text"])
        self.assertEqual(self.state.fact("fixture-history", 1)["content_text"], detail["content_text"])
        self.assertEqual(self.state.textless_ids(), [])
        self.assertEqual(self.state.original_fact("fixture-history", 1), original)
        self.assertEqual(self.state.checkpoint()["cursor"], "next")

    def test_available_empty_is_not_unavailable(self):
        self.state.commit_page(page([evidence(1, 10), evidence(2, 20, "")]))
        self.assertEqual(self.state.textless_ids(), [1])
        self.assertEqual(self.state.fact("fixture-history", 2)["text_state"], "available")
        self.assertEqual(self.state.fact("fixture-history", 2)["raw_text"], "")

    def test_timestamp_conflict_preserves_original_and_reconciles_bounded_window(self):
        first = evidence(1, 10, "original fact")
        self.state.commit_page(page([first], "next"))
        self.state.start_rescan()
        self.assertFalse(self.state.commit_page(page([evidence(1, 20, "different identity")], "bad")))
        checkpoint = self.state.checkpoint()
        self.assertIsNone(checkpoint["cursor"])
        self.assertIsNotNone(checkpoint["reconciliation_id"])
        self.assertEqual(self.state.fact("fixture-history", 1), first)
        self.assertTrue(self.state.commit_page(page([evidence(1, 20, "different identity")], None)))
        self.assertEqual(self.state.fact("fixture-history", 1), first)
        record = self.state.reconciliations()[-1]
        self.assertEqual(record["status"], "scanned_with_conflicts")
        self.assertEqual((record["window_from"], record["window_to"]), (0, 1000))
        conflicts = self.state.db.execute("SELECT original_json, incoming_json FROM conflicts").fetchall()
        self.assertEqual(json.loads(conflicts[0][0])["captured_at"], 10)
        self.assertEqual(json.loads(conflicts[0][1])["captured_at"], 20)

    def test_detail_timestamp_conflict_invalidates_checkpoint(self):
        self.state.commit_page(page([evidence(1, 10)], "next"))
        self.assertFalse(self.state.commit_detail(1, dict(frame_id=1, captured_at=20,
                                                        raw_text="new", content_text="new")))
        self.assertIsNone(self.state.checkpoint()["cursor"])
        self.assertEqual(self.state.fact("fixture-history", 1)["captured_at"], 10)

    def test_decreasing_high_water_invalidates_durable_checkpoint(self):
        self.state.observe_high_water(100)
        self.state.commit_page(page([evidence(1, 10)], "next"))
        self.state.close()
        self.state = self.open_state()
        self.assertTrue(self.state.observe_high_water(50))
        self.assertIsNone(self.state.checkpoint()["cursor"])
        self.assertEqual(self.state.checkpoint()["high_water"], 50)
        self.assertEqual(self.state.checkpoint()["highest_high_water"], 100)
        self.assertEqual(self.state.reconciliations()[-1]["reason"], "allocator_decreased")
        self.assertEqual(self.state.count_facts(), 1)

    def test_restored_history_requires_reconciliation_even_after_high_water_catches_up(self):
        self.state.observe_high_water(100)
        self.state.commit_page(page([evidence(1, 10)], "next"))
        self.assertFalse(self.state.observe_high_water(120))  # A restore can be invisible.
        self.assertEqual(self.state.checkpoint()["cursor"], "next")
        self.state.begin_reconciliation("operator_restore")
        self.assertIsNone(self.state.checkpoint()["cursor"])
        self.assertEqual(self.state.reconciliations()[-1]["reason"], "operator_restore")
        self.assertEqual(self.state.count_facts(), 1)

    def test_changed_history_preserves_prior_history_namespace(self):
        first = evidence(1, 10, "original")
        self.state.commit_page(page([first], "next"))
        self.assertFalse(self.state.commit_page(page([evidence(1, 20)], history="new-history")))
        self.assertTrue(self.state.commit_page(page([evidence(1, 20)], history="new-history")))
        self.assertEqual(self.state.fact("fixture-history", 1), first)
        self.assertEqual(self.state.fact("new-history", 1)["captured_at"], 20)
        self.assertEqual(self.state.reconciliations()[-1]["reason"], "history_changed")

    def test_replay_is_idempotent_and_rescan_updates_late_content(self):
        first = page([evidence(1, 10, "early")])
        self.state.commit_page(first)
        self.state.start_rescan()
        self.state.commit_page(first)
        self.assertEqual(self.state.count_facts(), 1)
        self.state.start_rescan()
        self.state.commit_page(page([evidence(1, 10, "late")]))
        self.assertEqual(self.state.fact("fixture-history", 1)["raw_text"], "late")
        self.assertEqual(self.state.original_fact("fixture-history", 1)["raw_text"], "early")

    def test_wrong_window_and_malformed_pages_never_advance_checkpoint(self):
        self.state.commit_page(page([evidence(1, 10)], "next"))
        for broken in (dict(page([evidence(2, 20)]), window={"from": 1, "to": 1000}),
                       page([evidence(3, 30), evidence(2, 20)]),
                       page([evidence(2, 1000)]),
                       page([dict(evidence(2, 20), raw_text="", text_state="not_available")])):
            with self.assertRaises(consumer.ProtocolError):
                self.state.commit_page(broken)
            self.assertEqual(self.state.checkpoint()["cursor"], "next")
            self.assertEqual(self.state.count_facts(), 1)

    def test_state_cannot_be_reused_for_different_source_or_window(self):
        with self.assertRaises(ValueError):
            consumer.State(self.path, "http://127.0.0.1:43210", 0, 2000)


class CliTests(unittest.TestCase):
    def test_connection_failure_is_not_empty_data_and_does_not_advance(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as directory:
            path = Path(directory) / "state.sqlite3"
            with socket.socket() as reserved:
                reserved.bind(("127.0.0.1", 0))  # Real refused TCP connection, no HTTP responder.
                url = f"http://127.0.0.1:{reserved.getsockname()[1]}"
                token = secrets.token_hex(32)
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("memory_consumer.py")),
                                         "--state", str(path), "--api-url", url,
                                         "--from", "0", "--to", "1000"],
                                        env=dict(os.environ, SCREENSEARCH_API_TOKEN=token,
                                                 PYTHONDONTWRITEBYTECODE="1"),
                                        capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 3)
            self.assertEqual(result.stdout, "")
            self.assertIn("NOT an empty-data result", result.stderr)
            self.assertNotIn(token, result.stderr)
            state = consumer.State(path, url, 0, 1000)
            try:
                self.assertIsNone(state.checkpoint()["cursor"])
                self.assertFalse(state.checkpoint()["done"])
                self.assertEqual(state.count_facts(), 0)
            finally:
                state.close()


class RealApiFixtureTests(unittest.TestCase):
    """Requires a real Rust API+SqliteStore runner, NOT supplied by this Python file.

    Missing runner is a visible skip, not acceptance. The parent API owner can
    implement the line protocol documented in docs/MEMORY_CONSUMER.md using the
    existing ApiServer/TestHost seams in crates/api/tests/http_api.rs.
    """
    def setUp(self):
        command = os.environ.get("SCREENSEARCH_CONSUMER_FIXTURE_CMD")
        if not command:
            self.skipTest("real crates/api+SqliteStore fixture runner not configured; API integration UNVERIFIED")
        argv = json.loads(command)
        self.assertTrue(isinstance(argv, list) and argv and all(isinstance(arg, str) for arg in argv),
                        "SCREENSEARCH_CONSUMER_FIXTURE_CMD must be a JSON argv array")
        self.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR"))
        self.addCleanup(self.tmp.cleanup)
        self.env = dict(os.environ, SCREENSEARCH_API_TOKEN=secrets.token_hex(32),
                        SCREENSEARCH_CONSUMER_FIXTURE_DIR=self.tmp.name,
                        PYTHONDONTWRITEBYTECODE="1")
        self.proc = subprocess.Popen(argv, cwd=Path(__file__).resolve().parent.parent,
                                     env=self.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, bufsize=1)
        assert self.proc.stdin is not None and self.proc.stdout is not None
        self.input = self.proc.stdin
        self.output = self.proc.stdout
        self.addCleanup(self.stop_fixture)
        self.events = queue.Queue()

        def read_events():
            for line in self.output:
                if line.startswith("MEMORY_FIXTURE "):
                    try:
                        self.events.put(json.loads(line[len("MEMORY_FIXTURE "):]))
                    except ValueError:
                        self.events.put({"event": "invalid_json"})
            self.events.put({"event": "eof"})

        self.reader = threading.Thread(target=read_events, daemon=True)
        self.reader.start()
        ready = self.event("ready", timeout=180)
        self.url = consumer.validate_api_url(ready["api_url"])
        # Isolated fixture must bind loopback, never a user's live network service.
        from urllib.parse import urlsplit
        self.assertIn(urlsplit(self.url).hostname, ("127.0.0.1", "::1"))
        self.start, self.end = ready["from"], ready["to"]
        self.history = ready["history_id"]
        self.expected = set(ready["expected_frame_ids"])
        self.assertGreater(len(self.expected), 1000)
        self.textless = ready["textless_frame_id"]
        self.assertIn(self.textless, self.expected)
        self.path = Path(self.tmp.name) / "consumer.sqlite3"
        self.api = consumer.Api(self.url, self.env["SCREENSEARCH_API_TOKEN"])

    def stop_fixture(self):
        if self.proc.poll() is None:
            try:
                self.input.write('{"command":"stop"}\n')
                self.input.flush()
                self.proc.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait(timeout=5)
        self.reader.join(timeout=5)
        self.input.close()
        self.output.close()

    def event(self, name, timeout=30):
        deadline = time.monotonic() + timeout
        while True:
            try:
                event = self.events.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                self.fail("Real Rust fixture did not acknowledge readiness/control; no API acceptance")
            if event.get("event") == name:
                return event
            if event.get("event") in ("eof", "error", "invalid_json"):
                self.fail("Real Rust fixture exited/failed its control protocol; no API acceptance")
            if time.monotonic() >= deadline:
                self.fail("Real Rust fixture acknowledgement deadline exceeded")

    def control(self, command, **fields):
        self.input.write(json.dumps(dict(command=command, **fields)) + "\n")
        self.input.flush()
        result = self.event("ack")
        self.assertEqual(result.get("command"), command)
        return result

    def cli(self, *extra, expected_exit=0):
        result = subprocess.run([sys.executable, str(Path(__file__).with_name("memory_consumer.py")),
                                 "--api-url", self.url, "--state", str(self.path),
                                 "--from", str(self.start), "--to", str(self.end),
                                 "--limit", "137", *extra], env=self.env,
                                capture_output=True, text=True, timeout=120)
        # Never include the environment, token or arbitrary server diagnostics.
        self.assertEqual(result.returncode, expected_exit, result.stderr)
        self.assertNotIn(self.env["SCREENSEARCH_API_TOKEN"], result.stdout + result.stderr)
        return json.loads(result.stdout.strip())

    def open_state(self):
        state = consumer.State(self.path, self.url, self.start, self.end)
        self.addCleanup(state.close)
        return state

    def test_reference_consumer_resumes_without_losing_committed_pages_real_api(self):
        summary = self.cli("--max-pages", "1")
        self.assertFalse(summary["traversal_exhausted"])
        state = consumer.State(self.path, self.url, self.start, self.end)
        try:
            checkpoint = state.checkpoint()
            self.assertIsNotNone(checkpoint["cursor"])
            original = dict(state.db.execute("SELECT frame_id,original_json FROM facts"))
            self.assertTrue(original)
        finally:
            state.close()
        summary = self.cli()
        self.assertTrue(summary["traversal_exhausted"])
        state = self.open_state()
        actual = dict(state.db.execute("SELECT frame_id,original_json FROM facts WHERE history_id=?", (self.history,)))
        self.assertEqual(set(actual), self.expected)
        for frame_id, payload in original.items():
            self.assertEqual(actual[frame_id], payload)

    def test_consumer_rereads_text_after_checkpoint_real_api(self):
        self.cli()
        state = self.open_state()
        original = state.fact(self.history, self.textless)
        assert original is not None
        self.assertEqual(original["text_state"], "not_available")
        self.control("enrich_textless", frame_id=self.textless)
        detail = self.api.get(f"/v1/frames/{self.textless}")
        self.assertIsInstance(detail["raw_text"], str)
        self.assertGreater(len(detail["raw_text"]), 4096)
        summary = self.cli("--rescan-seconds", "1000000000000")
        self.assertEqual(summary["pages_fetched"], 0)
        self.assertGreater(summary["textless_reread"], 0)
        fact = state.fact(self.history, self.textless)
        assert fact is not None
        self.assertEqual(fact["raw_text"], detail["raw_text"])
        self.assertEqual(fact["content_text"], detail["content_text"])
        self.assertEqual(fact["text_state"], "available")

    def test_restored_history_requires_reconciliation_real_api(self):
        self.cli("--max-pages", "1")
        state = self.open_state()
        old_high_water = state.checkpoint()["high_water"]
        old_count = state.count_facts()
        self.control("rewind")
        observed = self.api.coverage(self.start, self.end)
        self.assertLess(observed["frame_id_high_water"], old_high_water)
        summary = self.cli()
        self.assertTrue(summary["traversal_exhausted"])
        self.assertGreaterEqual(state.count_facts(), old_count)
        self.assertIn("allocator_decreased", [record["reason"] for record in state.reconciliations()])
        self.control("catch_up_high_water", minimum=old_high_water)
        self.cli("--reconcile")
        self.assertEqual(state.reconciliations()[-1]["reason"], "operator_restore")

    def test_timestamp_conflict_preserves_original_real_api(self):
        self.cli()
        state = self.open_state()
        frame_id = min(self.expected - {self.textless})
        original = state.fact(self.history, frame_id)
        assert original is not None
        self.control("reuse_timestamp", frame_id=frame_id)
        changed = self.api.get(f"/v1/frames/{frame_id}")
        self.assertNotEqual(changed["captured_at"], original["captured_at"])
        summary = self.cli("--rescan-seconds", "0", expected_exit=2)
        self.assertTrue(summary["traversal_exhausted"])
        self.assertGreater(summary["identity_conflicts_retained"], 0)
        self.assertEqual(state.fact(self.history, frame_id), original)
        self.assertEqual(state.reconciliations()[-1]["status"], "scanned_with_conflicts")

    def test_mid_drain_rewind_invalidates_stale_cursor_real_api(self):
        # F1: commit one page so a durable cursor is frozen at the pre-rewind snapshot
        # maximum, then fire the real backup restore on the very next page read. The
        # retained cursor now exceeds the restored allocator, so the real API answers 400
        # (`snapshot_max_frame_id > high-water`) and the consumer must re-read coverage,
        # prove the decrease, invalidate the checkpoint and rescan — never treat the 400
        # as data or assert a rewind it did not observe.
        summary = self.cli("--max-pages", "1")
        self.assertFalse(summary["traversal_exhausted"])
        state = self.open_state()
        self.assertIsNotNone(state.checkpoint()["cursor"])
        old_facts = state.count_facts()
        self.assertGreater(old_facts, 0)
        self.assertNotIn("allocator_decreased", [r["reason"] for r in state.reconciliations()])
        self.control("arm_rewind_before_page_read", count=1)
        summary = self.cli()
        self.assertTrue(summary["traversal_exhausted"])
        self.assertIn("allocator_decreased", [r["reason"] for r in state.reconciliations()])
        # The bounded rescan completed against the restored allocator, not the stale cursor.
        self.assertEqual(state.count_facts(), len(self.expected))

    def test_new_history_after_first_page_is_one_bounded_restart_real_api(self):
        # F2: commit one page, then swap the live store to a genuinely new history identity
        # (fresh DB slot via production open_path). The retained cursor carries the old
        # history_id, so the real API answers 409 and the retry page reports the new
        # history_id. That is ONE logical change: it must recover in a single bounded
        # restart while preserving the old namespace and its original facts.
        summary = self.cli("--max-pages", "1")
        self.assertFalse(summary["traversal_exhausted"])
        state = self.open_state()
        self.assertIsNotNone(state.checkpoint()["cursor"])
        old_facts = state.db.execute("SELECT COUNT(*) FROM facts WHERE history_id=?",
                                     (self.history,)).fetchone()[0]
        self.assertGreater(old_facts, 0)
        ack = self.control("rotate_history")
        new_history = ack["result"]["history_id"]
        self.assertNotEqual(new_history, self.history)
        summary = self.cli()
        self.assertTrue(summary["traversal_exhausted"])
        reasons = [r["reason"] for r in state.reconciliations()]
        self.assertEqual(reasons.count("server_history_changed"), 1)
        self.assertEqual(reasons.count("history_changed"), 1)
        # Old namespace/original facts preserved; the new namespace fully traversed.
        self.assertEqual(state.db.execute("SELECT COUNT(*) FROM facts WHERE history_id=?",
                                          (self.history,)).fetchone()[0], old_facts)
        self.assertEqual(state.db.execute("SELECT COUNT(*) FROM facts WHERE history_id=?",
                                          (new_history,)).fetchone()[0], len(self.expected))

    def test_detail_conflict_is_not_a_successful_reread_real_api(self):
        # F3: a textless detail re-read that conflicts with the preserved fact
        # (commit_detail returns False) must not be counted as a successful re-read.
        self.cli()
        state = self.open_state()
        original = state.fact(self.history, self.textless)
        assert original is not None
        self.assertEqual(original["text_state"], "not_available")
        self.control("reuse_timestamp", frame_id=self.textless)
        changed = self.api.get(f"/v1/frames/{self.textless}")
        self.assertNotEqual(changed["captured_at"], original["captured_at"])
        summary = self.cli("--rescan-seconds", "1000000000000", expected_exit=2)
        self.assertEqual(summary["pages_fetched"], 0)
        self.assertEqual(summary["textless_reread"], 0)
        self.assertGreater(summary["identity_conflicts_retained"], 0)
        self.assertEqual(state.fact(self.history, self.textless), original)


if __name__ == "__main__":
    unittest.main(verbosity=2)
