#!/usr/bin/env python3
"""Opt-in, bounded, retryable HTTP memory reader. Python 3.11+; stdlib only.

This is a developer/reference tool, not an app ML/runtime component. The SQLite
file contains private evidence. See docs/MEMORY_CONSUMER.md before using it.
"""
import argparse
from datetime import date, datetime, time as day_time, timedelta, timezone
import http.client
import ipaddress
import json
import os
from pathlib import Path
import re
import sqlite3
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


class ProtocolError(Exception):
    """An invalid server response; never an empty-data result."""


class ApiError(Exception):
    def __init__(self, status: int | None = None):
        self.status = status
        super().__init__(f"HTTP {status}" if status is not None else "API connection/read failed")


def integer(value, minimum=-(2**63), maximum=2**63 - 1):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ProtocolError("Expected an integral value in range")
    return value


def encode(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)


def utc_ms(value):
    delta = value.astimezone(timezone.utc) - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return delta.days * 86400000 + delta.seconds * 1000 + delta.microseconds // 1000


def parse_utc_ms(value):
    """Signed i64 UTC milliseconds or ISO UTC timestamp, at most millisecond precision."""
    if re.fullmatch(r"-?[0-9]+", value):
        result = int(value)
    elif re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,3})?(?:Z|\+00:00)", value):
        result = utc_ms(datetime.fromisoformat(value.replace("Z", "+00:00")))
    else:
        raise ValueError("Use integral UTC milliseconds or an explicit UTC ISO timestamp")
    if not -(2**63) <= result < 2**63:
        raise ValueError("UTC milliseconds exceed i64 range")
    return result


def day_window(day, timezone_name):
    if not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", day):
        raise ValueError("Use a calendar date in YYYY-MM-DD format")
    if not timezone_name or (timezone_name != "UTC" and "/" not in timezone_name):
        raise ValueError("Select an explicit IANA timezone (for example Europe/Paris or UTC)")
    try:
        zone = ZoneInfo(timezone_name)
        selected = date.fromisoformat(day)
        starts = [datetime.combine(selected + timedelta(days=n), day_time(), zone) for n in (0, 1)]
        # A few historical zones skip midnight or an entire date. Do not silently
        # normalize those boundaries; the operator can select explicit UTC bounds.
        for boundary in starts:
            if boundary.astimezone(timezone.utc).astimezone(zone).replace(fold=0) != boundary:
                raise ValueError("Local midnight is nonexistent; choose explicit UTC bounds")
        bounds = tuple(utc_ms(boundary) for boundary in starts)
        if bounds[0] >= bounds[1]:
            raise ValueError("Selected local date has no positive UTC window")
        return bounds
    except (ZoneInfoNotFoundError, OverflowError) as exc:
        raise ValueError("Timezone data or selected date is unavailable") from exc


def validate_api_url(url):
    try:
        parts = urllib.parse.urlsplit(url)
        if (parts.scheme not in ("http", "https") or not parts.hostname
                or parts.username is not None or parts.password is not None
                or parts.path not in ("", "/") or parts.query or parts.fragment
                or "?" in url or "#" in url or any(c.isspace() for c in url)
                or "\\" in url or "%" in parts.netloc):
            raise ValueError("API URL must be an HTTP(S) origin without credentials, path, query or fragment")
        parts.port  # Validate range and syntax.
        try:
            address = ipaddress.ip_address(parts.hostname)
        except ValueError:
            if "*" in parts.hostname:
                raise ValueError("Wildcard API destinations are forbidden")
        else:
            if address.is_unspecified:
                raise ValueError("Wildcard API destinations are forbidden")
        return url.rstrip("/")
    except (ValueError, TypeError) as exc:
        raise ValueError("Invalid API URL; select a concrete HTTP(S) origin") from exc


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Api:
    def __init__(self, url, token, timeout=30):
        self.url = validate_api_url(url)
        if not token or any(ord(c) < 33 or ord(c) > 126 for c in token):
            raise ValueError("Set SCREENSEARCH_API_TOKEN to a valid bearer credential")
        self._token = token
        self.timeout = timeout
        # No proxy environment leakage, redirect following or TLS bypass.
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    def get(self, path, params=None):
        url = self.url + path
        if params:
            url += "?" + urllib.parse.urlencode(params)
        request = urllib.request.Request(url, headers={"Authorization": "Bearer " + self._token,
                                                       "Accept": "application/json"}, method="GET")
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                if response.status != 200:
                    raise ApiError(response.status)
                # Read complete JSON or fail explicitly; never shorten full text.
                payload = json.loads(response.read(), parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
                if not isinstance(payload, dict):
                    raise ProtocolError("Expected a JSON object")
                return payload
        except urllib.error.HTTPError as exc:
            status = exc.code
            exc.close()  # Never display/store arbitrary server diagnostics or Location.
            raise ApiError(status) from None
        except (urllib.error.URLError, OSError, TimeoutError, http.client.HTTPException):
            raise ApiError() from None
        except (ValueError, UnicodeError):
            raise ProtocolError("API returned invalid JSON") from None

    def coverage(self, start, end):
        result = self.get("/v1/coverage", {"from": start, "to": end})
        if result.get("window") != {"from": start, "to": end}:
            raise ProtocolError("Coverage window differs from the selected window")
        integer(result.get("observed_at"))
        integer(result.get("frame_id_high_water"), 0)
        stored = integer(result.get("frames_stored"), 0)
        integer(result.get("frames_text_available"), 0, stored)
        if type(result.get("capture_enabled")) is not bool:
            raise ProtocolError("Coverage lacks explicit capture intent")
        return result


FIELDS = ("frame_id", "captured_at", "monitor_index", "app_hint", "window_title", "browser_url",
          "raw_text", "content_text", "text_source", "filter_version", "text_state",
          "vision_description", "session_id", "image_purged")


def validate_evidence(item, start, end, maximum):
    if not isinstance(item, dict) or any(field not in item for field in FIELDS):
        raise ProtocolError("Incomplete full-evidence record")
    integer(item["frame_id"], 1, maximum)
    integer(item["captured_at"], start, end - 1)
    integer(item["monitor_index"], 0, 2**32 - 1)
    for field in ("app_hint", "window_title", "browser_url", "raw_text", "content_text", "vision_description"):
        if item[field] is not None and not isinstance(item[field], str):
            raise ProtocolError("Expected complete nullable text")
    if item["text_state"] not in ("available", "not_available"):
        raise ProtocolError("Unknown text availability")
    if item["text_state"] == "not_available":
        if any(item[field] is not None for field in ("raw_text", "content_text", "text_source", "filter_version")):
            raise ProtocolError("Unavailable text must have explicit nulls")
    elif item["raw_text"] is None or item["content_text"] is None:
        raise ProtocolError("Available text must include full raw and filtered strings")
    if item["text_source"] not in (None, "ocr", "uia"):
        raise ProtocolError("Unknown text source")
    if item["filter_version"] is not None:
        integer(item["filter_version"], -(2**31), 2**31 - 1)
    if item["session_id"] is not None:
        integer(item["session_id"])
    if type(item["image_purged"]) is not bool:
        raise ProtocolError("Invalid image retention observation")
    return {field: item[field] for field in FIELDS}


class State:
    """One selected source/window per DB; durable page work and cursor are atomic."""
    def __init__(self, path, source, start, end):
        self.path = Path(path)
        self.db = sqlite3.connect(self.path, timeout=5)
        self.db.row_factory = sqlite3.Row
        try:
            self.db.execute("PRAGMA journal_mode=WAL")
            self.db.execute("PRAGMA synchronous=FULL")
            self.db.executescript("""
                CREATE TABLE IF NOT EXISTS checkpoint (
                    id INTEGER PRIMARY KEY CHECK(id=1), source TEXT NOT NULL,
                    window_from INTEGER NOT NULL, window_to INTEGER NOT NULL,
                    history_id TEXT, high_water INTEGER, highest_high_water INTEGER,
                    cursor TEXT, done INTEGER NOT NULL DEFAULT 0, snapshot_max INTEGER,
                    last_capture INTEGER, last_frame INTEGER, reconciliation_id INTEGER,
                    last_scan_ms INTEGER, coverage_json TEXT);
                CREATE TABLE IF NOT EXISTS facts (
                    history_id TEXT NOT NULL, frame_id INTEGER NOT NULL, captured_at INTEGER NOT NULL,
                    original_json TEXT NOT NULL, current_json TEXT NOT NULL,
                    PRIMARY KEY(history_id, frame_id));
                CREATE TABLE IF NOT EXISTS versions (
                    history_id TEXT NOT NULL, frame_id INTEGER NOT NULL, captured_at INTEGER NOT NULL,
                    payload_json TEXT NOT NULL, observed_ms INTEGER NOT NULL,
                    UNIQUE(history_id,frame_id,captured_at,payload_json));
                CREATE TABLE IF NOT EXISTS reconciliations (
                    id INTEGER PRIMARY KEY, reason TEXT NOT NULL, started_ms INTEGER NOT NULL,
                    finished_ms INTEGER, window_from INTEGER NOT NULL, window_to INTEGER NOT NULL,
                    prior_history TEXT, observed_history TEXT, prior_high_water INTEGER,
                    observed_high_water INTEGER, status TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS conflicts (
                    reconciliation_id INTEGER NOT NULL, history_id TEXT NOT NULL,
                    frame_id INTEGER NOT NULL, original_json TEXT NOT NULL, incoming_json TEXT NOT NULL,
                    UNIQUE(reconciliation_id,history_id,frame_id,incoming_json));
            """)
            with self.db:
                self.db.execute("INSERT OR IGNORE INTO checkpoint(id,source,window_from,window_to) VALUES(1,?,?,?)",
                                (source, start, end))
            checkpoint = self.checkpoint()
            if (checkpoint["source"], checkpoint["window_from"], checkpoint["window_to"]) != (source, start, end):
                raise ValueError("Use a separate state DB for each API origin and selected window")
        except BaseException:
            self.db.close()
            raise

    def close(self):
        self.db.close()

    def checkpoint(self):
        return dict(self.db.execute("SELECT * FROM checkpoint WHERE id=1").fetchone())

    def fact(self, history, frame_id, original=False):
        field = "original_json" if original else "current_json"
        row = self.db.execute(f"SELECT {field} FROM facts WHERE history_id=? AND frame_id=?",
                              (history, frame_id)).fetchone()
        return json.loads(row[0]) if row else None

    def original_fact(self, history, frame_id):
        return self.fact(history, frame_id, original=True)

    def count_facts(self):
        return self.db.execute("SELECT COUNT(*) FROM facts").fetchone()[0]

    def reconciliations(self):
        return [dict(row) for row in self.db.execute("SELECT * FROM reconciliations ORDER BY id")]

    def _reset_traversal(self):
        self.db.execute("""UPDATE checkpoint SET cursor=NULL,done=0,snapshot_max=NULL,
                          last_capture=NULL,last_frame=NULL WHERE id=1""")

    def _begin_reconciliation(self, reason, observed_history=None, observed_high_water=None):
        cp = self.checkpoint()
        if cp["reconciliation_id"] is not None:
            self.db.execute("UPDATE reconciliations SET status='interrupted' WHERE id=?",
                            (cp["reconciliation_id"],))
        result = self.db.execute("""INSERT INTO reconciliations(reason,started_ms,window_from,window_to,
                    prior_history,observed_history,prior_high_water,observed_high_water,status)
                    VALUES(?,?,?,?,?,?,?,?,'scanning')""",
                    (reason, time.time_ns() // 1000000, cp["window_from"], cp["window_to"],
                     cp["history_id"], observed_history, cp["high_water"], observed_high_water))
        self._reset_traversal()
        self.db.execute("UPDATE checkpoint SET reconciliation_id=? WHERE id=1", (result.lastrowid,))
        return result.lastrowid

    def begin_reconciliation(self, reason):
        with self.db:
            return self._begin_reconciliation(reason)

    def observe_high_water(self, high_water, coverage=None):
        integer(high_water, 0)
        with self.db:
            cp = self.checkpoint()
            decreased = cp["high_water"] is not None and high_water < cp["high_water"]
            if decreased:
                self._begin_reconciliation("allocator_decreased", observed_high_water=high_water)
            highest = max(high_water, cp["highest_high_water"] or 0)
            self.db.execute("""UPDATE checkpoint SET high_water=?,highest_high_water=?,coverage_json=? WHERE id=1""",
                            (high_water, highest, encode(coverage) if coverage is not None else cp["coverage_json"]))
        return decreased

    def start_rescan(self):
        with self.db:
            self._reset_traversal()

    def _upsert(self, history, item, in_reconciliation):
        payload = encode(item)
        previous = self.db.execute("SELECT * FROM facts WHERE history_id=? AND frame_id=?",
                                   (history, item["frame_id"])).fetchone()
        self.db.execute("INSERT OR IGNORE INTO versions VALUES(?,?,?,?,?)",
                        (history, item["frame_id"], item["captured_at"], payload, time.time_ns() // 1000000))
        if previous and previous["captured_at"] != item["captured_at"]:
            rec = self.checkpoint()["reconciliation_id"]
            if rec is None:
                rec = self._begin_reconciliation("timestamp_conflict", observed_history=history)
            self.db.execute("INSERT OR IGNORE INTO conflicts VALUES(?,?,?,?,?)",
                            (rec, history, item["frame_id"], previous["original_json"], payload))
            # Original and current prior facts stay intact; the competing observation
            # is recorded separately, even while the reconciliation scan progresses.
            return in_reconciliation
        self.db.execute("""INSERT INTO facts VALUES(?,?,?,?,?) ON CONFLICT(history_id,frame_id)
                        DO UPDATE SET current_json=excluded.current_json""",
                        (history, item["frame_id"], item["captured_at"], payload, payload))
        return True

    def commit_page(self, page):
        cp = self.checkpoint()
        if (not isinstance(page, dict) or page.get("window") != {"from": cp["window_from"], "to": cp["window_to"]}
                or page.get("consistency") != "bounded_membership_live_content"
                or not isinstance(page.get("history_id"), str) or not page["history_id"]
                or not isinstance(page.get("items"), list) or "next_cursor" not in page):
            raise ProtocolError("Invalid page envelope or selected window")
        cursor = page["next_cursor"]
        if cursor is not None and (not isinstance(cursor, str) or not cursor or len(cursor.encode()) > 4096):
            raise ProtocolError("Invalid continuation cursor")
        maximum = integer(page.get("snapshot_max_frame_id"), 0)
        integer(page.get("observed_at"))
        history = page["history_id"]
        changed_history = cp["history_id"] is not None and cp["history_id"] != history
        if not changed_history and cp["snapshot_max"] is not None and maximum != cp["snapshot_max"]:
            raise ProtocolError("Snapshot membership changed during traversal")
        previous_tuple = ((cp["last_capture"], cp["last_frame"])
                          if cp["last_capture"] is not None and not changed_history else None)
        items = []
        for item in page["items"]:
            item = validate_evidence(item, cp["window_from"], cp["window_to"], maximum)
            key = item["captured_at"], item["frame_id"]
            if previous_tuple is not None and key <= previous_tuple:
                raise ProtocolError("Page did not progress in chronological tuple order")
            previous_tuple = key
            items.append(item)
        if cursor is not None and (not items or cursor == cp["cursor"]):
            raise ProtocolError("Continuation did not progress")
        with self.db:
            if changed_history:
                self._begin_reconciliation("history_changed", observed_history=history)
                self.db.execute("UPDATE checkpoint SET history_id=? WHERE id=1", (history,))
                return False
            self.db.execute("UPDATE checkpoint SET history_id=? WHERE id=1", (history,))
            in_reconciliation = cp["reconciliation_id"] is not None
            accepted = True
            for item in items:
                if not self._upsert(history, item, in_reconciliation):
                    accepted = False
            if not accepted:
                return False
            last = previous_tuple or (cp["last_capture"], cp["last_frame"])
            # This is deliberately last, in the SAME FULL-synchronous transaction as
            # every upsert/version/conflict. No durable cursor can outrun its work.
            self.db.execute("""UPDATE checkpoint SET cursor=?,done=?,snapshot_max=?,last_capture=?,last_frame=?
                               WHERE id=1""", (cursor, cursor is None, maximum, *last))
            if cursor is None:
                now = time.time_ns() // 1000000
                rec = self.checkpoint()["reconciliation_id"]
                if rec is not None:
                    count = self.db.execute("SELECT COUNT(*) FROM conflicts WHERE reconciliation_id=?", (rec,)).fetchone()[0]
                    self.db.execute("UPDATE reconciliations SET status=?,finished_ms=? WHERE id=?",
                                    ("scanned_with_conflicts" if count else "scanned", now, rec))
                self.db.execute("UPDATE checkpoint SET last_scan_ms=?,reconciliation_id=NULL WHERE id=1", (now,))
        return True

    def textless_ids(self):
        history = self.checkpoint()["history_id"]
        return [row[0] for row in self.db.execute("""SELECT frame_id FROM facts WHERE history_id=?
                    AND json_extract(current_json,'$.text_state')='not_available' ORDER BY captured_at,frame_id""", (history,))]

    def commit_detail(self, frame_id, detail):
        cp = self.checkpoint()
        old = self.fact(cp["history_id"], frame_id)
        if old is None or not isinstance(detail, dict) or detail.get("frame_id") != frame_id:
            raise ProtocolError("Detail does not identify the requested stored fact")
        item = dict(old)
        for field in ("captured_at", "raw_text", "content_text"):
            if field not in detail:
                raise ProtocolError("Detail lacks full text or capture identity")
            item[field] = detail[field]
        item["text_state"] = "not_available" if detail["raw_text"] is None else "available"
        item["text_source"] = detail.get("text_source") if item["text_state"] == "available" else None
        if item["text_state"] == "not_available":
            item["filter_version"] = None
        # Legacy detail does not expose filter_version/session; preserve the latest
        # page values, then let the periodic full traversal refresh those fields.
        if "vision" in detail:
            vision = detail["vision"]
            item["vision_description"] = vision.get("description") if isinstance(vision, dict) else None
        for field in ("app_hint", "window_title", "browser_url", "image_purged", "monitor_index"):
            if field in detail:
                item[field] = detail[field]
        # A conflicting timestamp can leave the selected window. Still retain it as
        # a conflict, not a silent discard: only the preserved fact is window-bound.
        integer(item["captured_at"])
        validate_evidence(dict(item, captured_at=old["captured_at"]), cp["window_from"], cp["window_to"], 2**63 - 1)
        with self.db:
            return self._upsert(cp["history_id"], item, cp["reconciliation_id"] is not None)


class RunLock:
    """SQLite lock file serializes CLI processes without platform-specific locking."""
    def __init__(self, state_path):
        self.db = sqlite3.connect(str(state_path) + ".lock", timeout=0)
        try:
            self.db.execute("BEGIN EXCLUSIVE")
        except BaseException:
            self.db.close()
            raise

    def close(self):
        self.db.rollback()
        self.db.close()


def consume(api, state, limit=100, max_pages=None, rescan_seconds=3600):
    cp = state.checkpoint()
    coverage = api.coverage(cp["window_from"], cp["window_to"])
    state.observe_high_water(coverage["frame_id_high_water"], coverage)
    cp = state.checkpoint()
    now = time.time_ns() // 1000000
    if cp["done"] and (cp["last_scan_ms"] is None or now - cp["last_scan_ms"] >= rescan_seconds * 1000):
        state.start_rescan()
    pages = committed = 0
    restarts = 0
    # A 409 (`server_history_changed`) and the changed-history page it triggers are the
    # SAME logical change. This flag keeps them from being counted as two restarts while
    # the old history namespace and its immutable facts stay preserved.
    recovering_history = False
    while not state.checkpoint()["done"] and (max_pages is None or pages < max_pages):
        cp = state.checkpoint()
        params = {"limit": limit}
        params.update({"cursor": cp["cursor"]} if cp["cursor"] is not None else
                      {"from": cp["window_from"], "to": cp["window_to"]})
        try:
            response = api.get("/v1/frames", params)
        except ApiError as exc:
            if exc.status == 409:
                recovering_history = True
                state.begin_reconciliation("server_history_changed")
                restarts += 1
                if restarts > 1:
                    raise ProtocolError("Repeated history changes; checkpoint invalidated, retry after source stabilizes")
                continue
            if exc.status == 400:
                # A retained cursor can 400 when the allocator high-water dropped below
                # the cursor's frozen snapshot maximum (an active mid-drain rewind). Re-read
                # coverage and start a bounded rescan ONLY when a decrease is proven; never
                # convert an unexplained 400 into data or an asserted rewind.
                coverage = api.coverage(cp["window_from"], cp["window_to"])
                if state.observe_high_water(coverage["frame_id_high_water"], coverage):
                    restarts += 1
                    if restarts > 1:
                        raise ProtocolError("Repeated allocator decreases; bounded reconciliation remains pending")
                    continue
                raise
            raise
        before_history = cp["history_id"]
        accepted = state.commit_page(response)
        pages += 1
        committed += int(accepted)
        if accepted:
            recovering_history = False
            continue
        if recovering_history and state.checkpoint()["history_id"] != before_history:
            # Same logical change as the 409 above: the retry legitimately reported the
            # new history_id. Old facts are preserved by commit_page; count one restart.
            recovering_history = False
            continue
        recovering_history = False
        restarts += 1
        if restarts > 1:
            raise ProtocolError("Repeated identity changes; bounded reconciliation remains pending")
    # Re-read every currently textless ID even if its listing cursor is already
    # committed/exhausted. 404 records disappearance, never an empty OCR result.
    reread = missing = 0
    for frame_id in state.textless_ids():
        try:
            detail = api.get(f"/v1/frames/{frame_id}")
        except ApiError as exc:
            if exc.status == 404:
                missing += 1
                continue
            raise
        if state.commit_detail(frame_id, detail):
            reread += 1
    # Catch a detectable rewind while traversing; the next run will reconcile if
    # it happened during a finite max-pages run. This is not a cross-read snapshot.
    coverage = api.coverage(cp["window_from"], cp["window_to"])
    state.observe_high_water(coverage["frame_id_high_water"], coverage)
    cp = state.checkpoint()
    conflicts = state.db.execute("SELECT COUNT(*) FROM conflicts").fetchone()[0]
    return dict(pages_fetched=pages, pages_committed=committed, facts_retained=state.count_facts(), textless_reread=reread,
                details_missing_404=missing, traversal_exhausted=bool(cp["done"]),
                reconciliation_pending=cp["reconciliation_id"] is not None,
                identity_conflicts_retained=conflicts,
                meaning="No recorded frames is not proof of inactivity; exhaustion is not capture completeness")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-url", default=os.environ.get("SCREENSEARCH_API_URL", "http://127.0.0.1:43210"))
    parser.add_argument("--state", required=True, type=Path, help="User-selected private SQLite output file")
    parser.add_argument("--from", dest="start", help="Integral UTC-ms or UTC ISO timestamp")
    parser.add_argument("--to", dest="end", help="Exclusive UTC-ms or UTC ISO timestamp")
    parser.add_argument("--day", help="Local calendar date YYYY-MM-DD; requires --timezone")
    parser.add_argument("--timezone", help="Explicit IANA timezone for --day (including UTC)")
    parser.add_argument("--limit", type=int, default=100, help="Page size 1..1000")
    parser.add_argument("--max-pages", type=int, help="Stop after this many fetched pages, retaining resume state")
    parser.add_argument("--rescan-seconds", type=int, default=3600, help="Re-list exhausted selected window after this interval")
    parser.add_argument("--watch-seconds", type=int, help="Repeat within this SAME selected window; no automatic next-day drain")
    parser.add_argument("--reconcile", action="store_true", help="Explicit checkpoint invalidation after restore/old writers")
    args = parser.parse_args(argv)
    state = lock = None
    try:
        if args.day:
            if args.start is not None or args.end is not None or not args.timezone:
                raise ValueError("Use --day with --timezone, not --from/--to")
            start, end = day_window(args.day, args.timezone)
        else:
            if args.start is None or args.end is None or args.timezone:
                raise ValueError("Supply both --from and --to, or --day with --timezone")
            start, end = parse_utc_ms(args.start), parse_utc_ms(args.end)
        if start >= end or not 1 <= args.limit <= 1000 or args.rescan_seconds < 0:
            raise ValueError("Invalid half-open window, page limit or rescan interval")
        if (args.max_pages is not None and args.max_pages < 1) or (args.watch_seconds is not None and args.watch_seconds < 1):
            raise ValueError("Page/watch limits must be positive")
        api = Api(args.api_url, os.environ.get("SCREENSEARCH_API_TOKEN", ""))
        # Create only the explicitly selected file; no default private-history path.
        if not args.state.parent.is_dir():
            raise ValueError("Create the private state directory before running")
        lock = RunLock(args.state)
        state = State(args.state, api.url, start, end)
        if args.reconcile:
            state.begin_reconciliation("operator_restore")
        while True:
            summary = consume(api, state, args.limit, args.max_pages, args.rescan_seconds)
            print(encode(summary), flush=True)  # Counts only; never token/title/OCR/URL/cursor.
            if args.watch_seconds is None:
                return 2 if summary["reconciliation_pending"] or summary["identity_conflicts_retained"] else 0
            time.sleep(args.watch_seconds)
    except (ValueError, ProtocolError) as exc:
        print(f"memory consumer: {exc}", file=sys.stderr)
        return 2
    except ApiError as exc:
        guidance_by_status = {401: "check the bearer environment variable", 403: "use a valid history.read credential",
                              404: "required API route unavailable", 503: "feed readiness unavailable"}
        guidance = guidance_by_status.get(exc.status or 0, "retry after checking the API")
        print(f"memory consumer: {exc}; {guidance}; NOT an empty-data result", file=sys.stderr)
        return 3
    except (sqlite3.Error, OSError):
        print("memory consumer: local state write/lock failed; no empty-data claim; checkpoint is retryable", file=sys.stderr)
        return 4
    except KeyboardInterrupt:
        return 130
    finally:
        if state is not None:
            state.close()
        if lock is not None:
            lock.close()


if __name__ == "__main__":
    sys.exit(main())
