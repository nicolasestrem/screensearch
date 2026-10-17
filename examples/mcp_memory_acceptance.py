#!/usr/bin/env python3
"""Cross-machine **MCP -> HTTP** read-only acceptance driver for the ScreenSearch memory API.

Non-Windows (Linux/macOS) developer tool. Python 3.11+, standard library only.

WHAT THIS DOES
  Launches the *real* `screensearch-mcp` wrapper (a native build for this OS) with
  `--read-only` and drives it over the MCP stdio transport (newline-delimited JSON-RPC
  2.0). The wrapper is a pure HTTP client of the *actual, isolated* ScreenSearch app whose
  API origin the operator supplies. This driver starts no HTTP server and substitutes no
  canned response: every observation below comes from the real wrapper against the real
  app.

  Flow: `initialize` + `tools/list` -> `get_capture_coverage` -> `list_frames` pages
  (cursor) -> wrapper process restart + resume from the durable cursor -> `list_reports`
  pages -> `get_report` details -> `get_moment` source-frame detail. The driver compares the
  observed frame ids, capture timestamps, full-text hashes and preserved null/unknown coverage
  fields against an operator-supplied non-private fixture manifest, and — when that manifest
  carries a `reports` block — the report summaries, details, saved-result provenance arrays and
  Markdown digest (never the Markdown text) against the same manifest. It then verifies,
  against the same app, that an invalid token yields **401** and that the read token is
  refused a frame mark with **403**.

BOUNDARIES (why it is safe to run)
  * Stdout carries exactly one compact JSON report: status, counts, fixture ids and sha256
    hashes only. It never contains a token, OCR/text, window title, URL, cursor or image
    bytes. Diagnostics go to stderr.
  * No `--token` flag: the bearer comes only from `SCREENSEARCH_API_TOKEN`.
  * Read-only: the driver never requests capture (`"now": true`), never calls Ask, never
    loads or implies a model, and never requests an image (`include_image` is never set).
  * It makes outbound HTTP to the supplied origin only. No firewall/tunnel/live-profile
    changes; no inbound listener.
  * It touches only the private state DB and fixture manifest paths given on the CLI.

ATTESTATION BOUNDARY
  The read-only tool surface is `list_frames`, `get_capture_coverage`, `list_reports`,
  `get_report` (plus the retained `get_moment` source-frame detail). A successful run proves
  the exchange with the supplied origin and expected fixture, not that the origin belongs to a
  particular app or physical machine. The operator must independently attest native Windows
  source, profile isolation and a direct separate-machine path. A frame-only manifest reports
  the report step as `not_exercised`; a manifest with a `reports` block must pass every report
  check. This driver never claims full P4.
"""

from __future__ import annotations

import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import queue
import re
import secrets
import sqlite3
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

# --------------------------------------------------------------------------- constants

READ_TOOL_ORDER = [
    "search_screen_history",
    "get_moment",
    "list_frames",
    "get_capture_coverage",
    # P3 saved-report reads, registered immediately after coverage (03 §7f.4 line 957; the
    # MCP writer's tools.rs enumeration places them here). Both stay read-only: they only
    # read a stored projection and can never generate/queue/retry/deliver a run.
    "list_reports",
    "get_report",
    "where_was_i",
    "list_marks",
    "list_sessions",
    "get_session",
]
# Names that --read-only MUST refuse before any HTTP is entered.
FORBIDDEN_TOOLS = ("add_mark", "ask_screen_history", "ask_session")
# FrameEvidence wire fields, in the declaration order the app emits them
# (03 section 7f.6 / docs/API.md GET /v1/frames): every optional field is present as JSON
# null, and `text_state` is the only discriminator for whether a stored text row exists.
EVIDENCE_FIELDS = (
    "frame_id",
    "captured_at",
    "monitor_index",
    "app_hint",
    "window_title",
    "browser_url",
    "raw_text",
    "content_text",
    "text_source",
    "filter_version",
    "text_state",
    "vision_description",
    "session_id",
    "image_purged",
)
FRAME_CONSISTENCY = "bounded_membership_live_content"
PROTOCOL_VERSION = "2025-06-18"
SUPPORTED_PROTOCOLS = ("2025-06-18", "2025-03-26", "2024-11-05")
HASH_RE = re.compile(r"^[0-9a-f]{64}$")
# A deliberately wrong bearer for the 401 probe: random, never the real credential.
INVALID_TOKEN = "invalid-" + secrets.token_hex(32)

# --- saved-report wire contract (03 §7f.5 / §7f.6 / §8c) --------------------
# `ReportSummary` exact public fields, in declaration order (03 §7f.6). A list row is a
# summary only; the saved Markdown and evidence arrays appear only in a `get_report` detail.
REPORT_SUMMARY_FIELDS = (
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
)
# `ReportDetail` = all summary fields plus the optional saved result.
REPORT_DETAIL_FIELDS = REPORT_SUMMARY_FIELDS + ("result",)
# `ReportPage` = items + next_cursor only (no summary fields at the page level).
REPORT_PAGE_FIELDS = ("items", "next_cursor")
# `SavedReport` fields after the Markdown payload (03 §7f.6 / traits::SavedReport).
SAVED_REPORT_TAIL_FIELDS = (
    "model",
    "frame_ids",
    "session_ids",
    "cited_frame_ids",
    "frames_sampled",
    "frames_summarized",
    "periods_total",
    "periods_covered",
    "passes",
    "truncated",
)
# The live detail carries `markdown`; the operator's expected detail replaces that one field
# with its UTF-8 sha256 so no Markdown text ever has to live in the manifest or on stdout.
SAVED_REPORT_FIELDS = ("markdown",) + SAVED_REPORT_TAIL_FIELDS
EXPECTED_SAVED_REPORT_FIELDS = ("markdown_sha256",) + SAVED_REPORT_TAIL_FIELDS
REPORT_PHASES = ("queued", "running", "blocked", "finished")
REPORT_OUTCOMES = ("OK", "PARTIAL", "NO_DATA", "BLOCKED", "FAILED")
# The exact §8c `ReportError` code allowlist (03 lines 1426-1431); anything else projects null.
REPORT_ERROR_CODES = (
    "INVALID_CONFIG",
    "NOT_FOUND",
    "BUSY",
    "SOURCE_UNAVAILABLE",
    "MODEL_UNAVAILABLE",
    "STORE_ERROR",
    "GENERATION_FAILED",
    "FILE_CONFLICT",
    "WRITE_FAILED",
    "CREDENTIAL_UNAVAILABLE",
    "DELIVERY_FAILED",
)
# The frame-listing 4096-byte cursor cap also bounds report cursors (03 §7f.6).
REPORT_CURSOR_MAX_BYTES = 4096
# Strict `YYYY-MM-DD` (local date and selected-range dates are dates, never timestamps).
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

EXIT_OK = 0
EXIT_FAIL = 2
EXIT_HTTP = 3
EXIT_LOCAL = 4
EXIT_INTERRUPT = 130


class InputError(Exception):
    """Local input is missing or malformed."""


class ProtocolError(Exception):
    """The wrapper violated the MCP stdio / tool-result contract, or the app response is
    structurally invalid."""


class ApiError(Exception):
    """An HTTP request could not be completed or returned an unusable status."""

    def __init__(self, status=None, message=None):
        self.status = status
        super().__init__(message or (f"HTTP {status}" if status is not None else "API connection/read failed"))


# --------------------------------------------------------------------------- helpers


def sha256_hex(value):
    """Lowercase hex sha256 of the UTF-8 encoding of `value`, or None for None/bytes.

    This is the only place raw text is touched, and it emits a hash, never the text.
    """
    if value is None:
        return None
    if isinstance(value, bytes):
        data = value
    else:
        data = str(value).encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(value):
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False
    ).encode("utf-8")


def _reject_constant(_):
    raise ValueError("non-finite JSON constant is not allowed")


def load_json_strict(text, what):
    try:
        return json.loads(text, parse_constant=_reject_constant)
    except ValueError as exc:
        raise InputError(f"{what} is not valid JSON") from exc


def json_pointer(document, pointer):
    """RFC 6901 resolver. Raises KeyError/IndexError/'ValueError' when absent."""
    if not isinstance(pointer, str) or not pointer.startswith("/"):
        if pointer == "":
            return document
        raise ValueError("JSON pointer must be empty or start with '/'")
    current = document
    for token in pointer.split("/")[1:]:
        token = token.replace("~1", "/").replace("~0", "~")
        if isinstance(current, list):
            if not token.isdigit():
                raise KeyError(pointer)
            current = current[int(token)]
        elif isinstance(current, dict):
            if token not in current:
                raise KeyError(pointer)
            current = current[token]
        else:
            raise KeyError(pointer)
    return current


def parse_utc_ms(value, what):
    if value is None:
        return None
    if isinstance(value, int) and not isinstance(value, bool):
        return value
    if isinstance(value, str):
        text = value.strip()
        if re.fullmatch(r"-?\d+", text):
            return int(text)
        if text.endswith("Z"):
            text = text[:-1] + "+00:00"
        try:
            import datetime

            dt = datetime.datetime.fromisoformat(text)
        except ValueError as exc:
            raise InputError(f"{what} must be integer epoch-ms or an ISO-8601 UTC instant") from exc
        if dt.tzinfo is None:
            raise InputError(f"{what} ISO timestamp must carry an explicit UTC offset")
        return int(dt.timestamp() * 1000)
    raise InputError(f"{what} must be integer epoch-ms or an ISO-8601 UTC instant")


def validate_api_url(raw):
    """Return the concrete `scheme://host[:port]` origin, rejecting anything ambiguous.

    Mirrors the wrapper's own URL strictness: no credentials, path, query, fragment,
    wildcard or whitespace; concrete IPv4/IPv6/hostname only.
    """
    if raw is None or not str(raw).strip():
        raise InputError("--api-url is required")
    text = str(raw).strip()
    if any(ch.isspace() or ord(ch) < 0x20 or ch == "\\" for ch in text):
        raise InputError("--api-url contains whitespace or control characters")
    parts = urllib.parse.urlsplit(text)
    if parts.scheme not in ("http", "https"):
        raise InputError("--api-url scheme must be http or https")
    if parts.username or parts.password or "@" in (parts.netloc or ""):
        raise InputError("--api-url must not carry credentials")
    if parts.query or parts.fragment:
        raise InputError("--api-url must not carry a query or fragment")
    if parts.path not in ("", "/"):
        raise InputError("--api-url must be a bare origin (no path)")
    host = parts.hostname
    if not host:
        raise InputError("--api-url must name a host")
    if "*" in host or host in ("0.0.0.0", "0", "::", "[::]"):
        raise InputError("--api-url must name a concrete host, not a wildcard")
    try:
        port = parts.port
    except ValueError as exc:
        raise InputError("--api-url has an invalid port") from exc
    try:
        ip = ipaddress.ip_address(host)
        literal = f"[{host}]" if ip.version == 6 else host
    except ValueError:
        literal = host
    origin = f"{parts.scheme}://{literal}"
    if port is not None:
        origin += f":{port}"
    return origin


# --------------------------------------------------------------------------- check log


class Checks:
    def __init__(self):
        self.items = {}

    def record(self, name, ok, detail=None):
        entry = {"ok": bool(ok)}
        if detail is not None:
            entry["detail"] = detail
        self.items[name] = entry
        return bool(ok)

    def all_ok(self):
        return all(v["ok"] for v in self.items.values())

    def failed(self):
        return [k for k, v in self.items.items() if not v["ok"]]


# --------------------------------------------------------------------------- fixture


class Fixture:
    """The operator-supplied non-private expected-fixture manifest."""

    def __init__(self, doc, path):
        self.path = path
        self.raw = doc
        self.history_id = doc.get("history_id")
        self.frames = {}
        frames = doc.get("frames")
        if not isinstance(frames, list) or not frames:
            raise InputError("fixture must carry a non-empty 'frames' array")
        for entry in frames:
            if not isinstance(entry, dict):
                raise InputError("each fixture frame must be an object")
            fid = entry.get("frame_id")
            if type(fid) is not int or fid < 1:
                raise InputError("fixture frame_id must be a positive integer")
            if fid in self.frames:
                raise InputError(f"fixture frame_id {fid} is duplicated")
            cap = entry.get("captured_at")
            if type(cap) is not int:
                raise InputError(f"fixture frame {fid} needs an integer captured_at")
            hashes = {}
            for key in ("raw_text_sha256", "content_text_sha256", "vision_description_sha256"):
                val = entry.get(key)
                if val is not None and (not isinstance(val, str) or not HASH_RE.fullmatch(val)):
                    raise InputError(f"fixture frame {fid} field {key} must be null or a lowercase sha256 hex")
                hashes[key] = val
            state = entry.get("text_state")
            if state not in (None, "available", "not_available"):
                raise InputError(f"fixture frame {fid} text_state must be 'available'/'not_available'/absent")
            inferred = "not_available" if hashes["raw_text_sha256"] is None else "available"
            if state is None:
                state = inferred
            if state == "not_available" and inferred == "available":
                raise InputError(f"fixture frame {fid} is text_state=not_available but has a raw_text hash")
            if state == "available" and inferred == "not_available":
                raise InputError(f"fixture frame {fid} is text_state=available but has no raw_text hash")
            self.frames[fid] = {
                "captured_at": cap,
                "text_state": state,
                **hashes,
            }

        self.textless = self._id_list(doc.get("textless_frame_ids"), "textless_frame_ids")
        self.available_empty = self._id_list(doc.get("available_empty_frame_ids"), "available_empty_frame_ids")
        empty_hash = sha256_hex("")
        for fid in self.textless:
            if self.frames[fid]["text_state"] != "not_available":
                raise InputError(f"textless_frame_ids {fid} is not text_state=not_available")
        for fid in self.available_empty:
            if self.frames[fid]["raw_text_sha256"] != empty_hash:
                raise InputError(f"available_empty_frame_ids {fid} raw_text_sha256 is not sha256('')")

        checks = doc.get("coverage_checks", [])
        if not isinstance(checks, list):
            raise InputError("coverage_checks must be an array")
        for check in checks:
            if not isinstance(check, dict) or not isinstance(check.get("pointer"), str):
                raise InputError("each coverage_check needs a 'pointer' string")
            keys = [k for k in ("equals", "is_null", "present") if k in check]
            if len(keys) != 1:
                raise InputError("each coverage_check needs exactly one of equals/is_null/present")
        self.coverage_checks = checks

        # Optional P3 saved-report extension (freeze: independent acceptance-manifest
        # extension). The expected rows are produced by the operator from production seed
        # inputs and a direct read-only SQLite projection — never scraped from an API/MCP
        # response. When the block is absent the manifest stays frame-only and the report
        # step is reported as not_exercised, never as full P4 acceptance.
        reports = doc.get("reports")
        if reports is None:
            self.reports = None
        else:
            if not isinstance(reports, dict):
                raise InputError("fixture 'reports' must be an object when present")
            summaries = reports.get("expected_summaries")
            details = reports.get("expected_details")
            if not isinstance(summaries, list) or not summaries:
                raise InputError("fixture reports.expected_summaries must be a non-empty array")
            if not isinstance(details, list) or not details:
                raise InputError("fixture reports.expected_details must be a non-empty array")
            if len(details) != len(summaries):
                raise InputError(
                    "fixture reports expected_summaries/expected_details must correspond one-to-one"
                )
            for entry in summaries:
                if not isinstance(entry, dict):
                    raise InputError("each fixture report summary must be an object")
            for entry in details:
                if not isinstance(entry, dict):
                    raise InputError("each fixture report detail must be an object")
            self.reports = {"expected_summaries": summaries, "expected_details": details}

        self.declared_window = doc.get("window")
        target = doc.get("post_target_frame_id")
        if target is not None:
            if type(target) is not int or target not in self.frames:
                raise InputError("post_target_frame_id must reference a fixture frame")
            self.post_target_frame_id = target
        else:
            self.post_target_frame_id = min(self.frames)

    def _id_list(self, value, what):
        if value is None:
            return []
        if not isinstance(value, list):
            raise InputError(f"{what} must be an array")
        for fid in value:
            if type(fid) is not int or fid not in self.frames:
                raise InputError(f"{what} references unknown frame {fid!r}")
        return list(value)

    def ordered_ids(self):
        return [
            fid
            for fid, _ in sorted(self.frames.items(), key=lambda kv: (kv[1]["captured_at"], kv[0]))
        ]

    def ordered_tuples(self):
        return sorted(((v["captured_at"], fid) for fid, v in self.frames.items()))


# --------------------------------------------------------------------------- MCP client


class McpChild:
    """Drives one real `screensearch-mcp --read-only` child over stdio JSON-RPC 2.0."""

    def __init__(self, exe, api_url, token, timeout=120.0):
        self.exe = str(exe)
        self.api_url = api_url
        self.token = token
        self.timeout = timeout
        self._proc = None
        self._queue = queue.Queue()
        self._stderr_tail = []
        self._next_id = 1
        self.spawns = 0
        self._spawn()

    def _child_env(self):
        env = dict(os.environ)
        env["SCREENSEARCH_API_URL"] = self.api_url
        if self.token is None:
            env.pop("SCREENSEARCH_API_TOKEN", None)
        else:
            env["SCREENSEARCH_API_TOKEN"] = self.token
        # The wrapper ignores proxies, but strip them so the observation is unambiguous.
        for key in (
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ):
            env.pop(key, None)
        return env

    def _spawn(self):
        try:
            self._proc = subprocess.Popen(
                [self.exe, "--read-only"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self._child_env(),
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
            )
        except OSError as exc:
            raise InputError("could not launch the wrapper binary") from exc
        self.spawns += 1
        self._stderr_tail = []
        threading.Thread(target=self._pump_stdout, daemon=True).start()
        threading.Thread(target=self._pump_stderr, daemon=True).start()

    def _pump_stdout(self):
        proc = self._proc
        assert proc is not None and proc.stdout is not None
        for line in proc.stdout:
            self._queue.put(line)
        self._queue.put(None)  # EOF sentinel

    def _pump_stderr(self):
        proc = self._proc
        assert proc is not None and proc.stderr is not None
        for line in proc.stderr:
            self._stderr_tail.append(line.rstrip("\n"))
            if len(self._stderr_tail) > 20:
                self._stderr_tail.pop(0)

    def _stderr_hint(self):
        tail = [ln for ln in self._stderr_tail if ln.strip()]
        return tail[-1] if tail else ""

    def _send(self, method, params=None, notification=False):
        proc = self._proc
        if proc is None or proc.stdin is None:
            raise ProtocolError("wrapper stdin is closed")
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        request_id = None
        if not notification:
            request_id = self._next_id
            self._next_id += 1
            msg["id"] = request_id
        line = json.dumps(msg, ensure_ascii=False, separators=(",", ":"))
        try:
            proc.stdin.write(line + "\n")
            proc.stdin.flush()
        except (BrokenPipeError, OSError) as exc:
            raise ProtocolError("wrapper closed its stdin") from exc
        return request_id

    def _recv_for(self, request_id):
        deadline = time.monotonic() + self.timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProtocolError("timed out waiting for the wrapper response")
            try:
                line = self._queue.get(timeout=remaining)
            except queue.Empty:
                raise ProtocolError("timed out waiting for the wrapper response")
            if line is None:
                hint = self._stderr_hint()
                raise ProtocolError(f"wrapper exited before responding ({hint})" if hint else "wrapper exited before responding")
            text = line.strip()
            if not text:
                continue
            try:
                msg = json.loads(text, parse_constant=_reject_constant)
            except ValueError as exc:
                raise ProtocolError("wrapper emitted a non-JSON line on stdout") from exc
            if not isinstance(msg, dict):
                raise ProtocolError("wrapper emitted a non-object JSON message")
            if msg.get("id") != request_id:
                # Tolerate (and ignore) any unsolicited notification; nothing else is legal.
                if "id" not in msg and "method" in msg:
                    continue
                raise ProtocolError("wrapper response id does not match the request")
            if msg.get("jsonrpc") != "2.0":
                raise ProtocolError("wrapper response is missing jsonrpc 2.0")
            if "error" in msg:
                err = msg["error"] if isinstance(msg["error"], dict) else {}
                raise ProtocolError(f"JSON-RPC error {err.get('code')}: {err.get('message')}")
            if "result" not in msg:
                raise ProtocolError("wrapper response has neither result nor error")
            return msg["result"]

    def request(self, method, params=None):
        request_id = self._send(method, params)
        return self._recv_for(request_id)

    def notify(self, method, params=None):
        self._send(method, params, notification=True)

    def handshake(self):
        result = self.request(
            "initialize",
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "mcp-memory-acceptance", "version": "1"},
            },
        )
        if not isinstance(result, dict) or not isinstance(result.get("serverInfo"), dict):
            raise ProtocolError("initialize result is missing serverInfo")
        if result.get("protocolVersion") not in SUPPORTED_PROTOCOLS:
            raise ProtocolError("initialize returned an unsupported protocolVersion")
        caps = result.get("capabilities")
        if not isinstance(caps, dict) or "tools" not in caps:
            raise ProtocolError("initialize result does not advertise the tools capability")
        self.notify("notifications/initialized")
        return result

    def list_tools(self):
        result = self.request("tools/list", {})
        if not isinstance(result, dict) or not isinstance(result.get("tools"), list):
            raise ProtocolError("tools/list result is missing a tools array")
        names = []
        for tool in result["tools"]:
            if not isinstance(tool, dict) or not isinstance(tool.get("name"), str):
                raise ProtocolError("tools/list entry is malformed")
            names.append(tool["name"])
        return names

    def call_tool(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if not isinstance(result, dict) or not isinstance(result.get("content"), list) or not result["content"]:
            raise ProtocolError(f"tools/call {name} result is missing content")
        return result

    def tool_error_text(self, result):
        parts = []
        for block in result.get("content", []):
            if isinstance(block, dict) and isinstance(block.get("text"), str):
                parts.append(block["text"])
        return "\n".join(parts)

    def tool_json(self, name, arguments):
        result = self.call_tool(name, arguments)
        if result.get("isError") is True:
            raise ProtocolError(f"tool {name} returned isError: {self.tool_error_text(result)[:160]}")
        text = self.tool_error_text(result)
        try:
            return json.loads(text, parse_constant=_reject_constant)
        except ValueError as exc:
            raise ProtocolError(f"tool {name} did not return JSON") from exc

    def restart(self):
        self.close()
        self._queue = queue.Queue()
        self._spawn()

    def close(self):
        proc = self._proc
        self._proc = None
        if proc is None:
            return
        try:
            if proc.stdin is not None:
                proc.stdin.close()
        except OSError:
            pass
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass


# --------------------------------------------------------------------------- state


class State:
    """Durable cursor checkpoint so a traversal resumes across process restarts.

    Bound to one (origin, window) pair; a mismatch is refused rather than silently
    re-windowed (a resumed cursor is only meaningful inside its frozen window).
    """

    def __init__(self, path, origin, start, end):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.lock = self.path.with_suffix(self.path.suffix + ".lock")
        try:
            self._lock_file = os.open(str(self.lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        except FileExistsError as exc:
            raise InputError("another acceptance run holds this state DB") from exc
        os.write(self._lock_file, str(os.getpid()).encode("ascii"))
        try:
            self.conn = sqlite3.connect(str(self.path))
            self.conn.execute("PRAGMA journal_mode=WAL")
            self.conn.execute("PRAGMA synchronous=FULL")
            self.conn.execute(
                """
                CREATE TABLE IF NOT EXISTS accept (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    origin TEXT NOT NULL,
                    window_from INTEGER NOT NULL,
                    window_to INTEGER NOT NULL,
                    history_id TEXT,
                    snapshot_max INTEGER,
                    cursor TEXT,
                    done INTEGER NOT NULL DEFAULT 0,
                    pages_fetched INTEGER NOT NULL DEFAULT 0,
                    frames_seen INTEGER NOT NULL DEFAULT 0,
                    restarts INTEGER NOT NULL DEFAULT 0,
                    list_frames_calls INTEGER NOT NULL DEFAULT 0,
                    last_capacity INTEGER NOT NULL DEFAULT 0,
                    last_cursor_bytes INTEGER NOT NULL DEFAULT 0,
                    last_captured_at INTEGER,
                    last_frame_id INTEGER
                )
                """
            )
            row = self.conn.execute("SELECT origin, window_from, window_to FROM accept WHERE id=1").fetchone()
            if row is None:
                self.conn.execute(
                    "INSERT INTO accept (id, origin, window_from, window_to, cursor, done)"
                    " VALUES (1, ?, ?, ?, NULL, 0)",
                    (origin, start, end),
                )
            elif row[0] != origin or row[1] != start or row[2] != end:
                raise InputError("state DB was created for a different origin/window")
            self.conn.commit()
        except BaseException:
            self.close()
            raise

    def checkpoint(self):
        row = self.conn.execute(
            "SELECT history_id, snapshot_max, cursor, done, pages_fetched, frames_seen,"
            " restarts, list_frames_calls, last_capacity, last_cursor_bytes FROM accept WHERE id=1"
        ).fetchone()
        return {
            "history_id": row[0],
            "snapshot_max": row[1],
            "cursor": row[2],
            "done": bool(row[3]),
            "pages_fetched": row[4],
            "frames_seen": row[5],
            "restarts": row[6],
            "list_frames_calls": row[7],
            "last_capacity": row[8],
            "last_cursor_bytes": row[9],
        }

    def rescan(self):
        self.conn.execute(
            "UPDATE accept SET cursor=NULL, done=0, history_id=NULL, snapshot_max=NULL,"
            " pages_fetched=0, frames_seen=0, last_captured_at=NULL, last_frame_id=NULL WHERE id=1"
        )
        self.conn.commit()

    def record_list_call(self, capacity, cursor_bytes):
        self.conn.execute(
            "UPDATE accept SET list_frames_calls = list_frames_calls + 1,"
            " last_capacity = ?, last_cursor_bytes = ? WHERE id=1",
            (capacity, cursor_bytes),
        )
        self.conn.commit()

    def commit_page(self, *, history_id, snapshot_max, cursor, done, items):
        if cursor is not None and not isinstance(cursor, str):
            raise ProtocolError("next_cursor is neither a string nor null")
        if cursor is not None and cursor == "":
            raise ProtocolError("next_cursor is empty")
        if cursor is not None and len(cursor.encode("utf-8")) > 4096:
            raise ProtocolError("next_cursor exceeds the 4096-byte cap")
        prior = self.checkpoint()
        if cursor is not None and prior["cursor"] is not None and cursor == prior["cursor"]:
            raise ProtocolError("next_cursor did not advance (no drain progress)")
        if not done and not items and cursor is not None:
            raise ProtocolError("a non-terminal page returned no items")
        first = items[0] if items else None
        last = items[-1] if items else None
        with self.conn:
            self.conn.execute(
                "UPDATE accept SET history_id=?, snapshot_max=?, cursor=?, done=?,"
                " pages_fetched=pages_fetched+1, frames_seen=frames_seen+?,"
                " last_captured_at=?, last_frame_id=? WHERE id=1",
                (
                    history_id,
                    snapshot_max,
                    cursor,
                    1 if done else 0,
                    len(items),
                    last["captured_at"] if last else prior.get("last_captured_at"),
                    last["frame_id"] if last else None,
                ),
            )

    def bump_restart(self):
        with self.conn:
            self.conn.execute("UPDATE accept SET restarts = restarts + 1 WHERE id=1")

    def close(self):
        conn = getattr(self, "conn", None)
        if conn is not None:
            try:
                conn.close()
            except sqlite3.Error:
                pass
            self.conn = None
        fd = getattr(self, "_lock_file", None)
        if fd is not None:
            try:
                os.close(fd)
            except OSError:
                pass
            self._lock_file = None
        try:
            if self.lock.exists():
                self.lock.unlink()
        except OSError:
            pass


# --------------------------------------------------------------------------- validation


def normalize_evidence(item):
    if not isinstance(item, dict) or set(item) != set(EVIDENCE_FIELDS):
        raise ProtocolError("frame evidence has unexpected/missing fields")
    fid = item.get("frame_id")
    captured_at = item.get("captured_at")
    if type(fid) is not int or fid < 1 or type(captured_at) is not int:
        raise ProtocolError("frame evidence has a non-integer frame_id/captured_at")
    monitor_index = item.get("monitor_index")
    if type(monitor_index) is not int or monitor_index < 0:
        raise ProtocolError("frame evidence has a non-integer monitor_index")
    state = item.get("text_state")
    if state not in ("available", "not_available"):
        raise ProtocolError("frame evidence has an invalid text_state")
    filter_version = item.get("filter_version")
    if state == "not_available":
        # No stored text record: raw/content/source/version are all null (03 section 7f.6).
        for field in ("raw_text", "content_text", "text_source", "filter_version"):
            if item.get(field) is not None:
                raise ProtocolError("a textless frame carries text, a text_source or a filter_version")
    else:
        for field in ("raw_text", "content_text"):
            if not isinstance(item.get(field), str):
                raise ProtocolError("an available frame is missing raw/content text")
        if item.get("text_source") not in ("ocr", "uia"):
            raise ProtocolError("an available frame has an invalid text_source")
        if type(filter_version) is not int:
            raise ProtocolError("an available frame is missing its filter_version")
    if not isinstance(item.get("image_purged"), bool):
        raise ProtocolError("frame evidence has a non-boolean image_purged")
    for field in ("app_hint", "window_title", "browser_url", "vision_description"):
        if item.get(field) is not None and not isinstance(item[field], str):
            raise ProtocolError(f"frame evidence {field} must be a string or null")
    if item.get("session_id") is not None and type(item["session_id"]) is not int:
        raise ProtocolError("frame evidence session_id must be an integer or null")
    return {
        "frame_id": fid,
        "captured_at": captured_at,
        "monitor_index": monitor_index,
        "text_state": state,
        "text_source": item.get("text_source"),
        "filter_version": filter_version,
        "raw_text": item.get("raw_text"),
        "content_text": item.get("content_text"),
        "vision_description": item.get("vision_description"),
        "image_purged": item["image_purged"],
        "session_id": item.get("session_id"),
        "app_hint": item.get("app_hint"),
        "window_title": item.get("window_title"),
        "browser_url": item.get("browser_url"),
    }


def verify_page(page, start, end):
    if not isinstance(page, dict):
        raise ProtocolError("frame page is not an object")
    for field in ("items", "next_cursor", "window", "history_id", "snapshot_max_frame_id", "observed_at", "consistency"):
        if field not in page:
            raise ProtocolError(f"frame page is missing {field}")
    if page.get("consistency") != FRAME_CONSISTENCY:
        raise ProtocolError("frame page consistency is not bounded_membership_live_content")
    window = page.get("window")
    if not isinstance(window, dict) or window.get("from") != start or window.get("to") != end:
        raise ProtocolError("frame page window does not match the selected UTC window")
    if not isinstance(page.get("history_id"), str) or not page["history_id"]:
        raise ProtocolError("frame page history_id is not a non-empty string")
    if type(page.get("snapshot_max_frame_id")) is not int or type(page.get("observed_at")) is not int:
        raise ProtocolError("frame page snapshot_max_frame_id/observed_at must be integers")
    if page.get("next_cursor") is not None and not isinstance(page["next_cursor"], str):
        raise ProtocolError("frame page next_cursor must be a string or null")
    items = page.get("items")
    if not isinstance(items, list):
        raise ProtocolError("frame page items is not an array")
    normalized = [normalize_evidence(it) for it in items]
    previous = None
    for item in normalized:
        if item["captured_at"] < start or item["captured_at"] >= end:
            raise ProtocolError("a served frame lies outside the selected half-open window")
        key = (item["captured_at"], item["frame_id"])
        if previous is not None and key <= previous:
            raise ProtocolError("frame page items are not strictly ascending")
        previous = key
    return normalized


def compare_to_fixture(items, fixture, mismatches):
    for item in items:
        fid = item["frame_id"]
        expected = fixture.frames.get(fid)
        if expected is None:
            mismatches.append({"frame_id": fid, "field": "present", "reason": "served frame not in fixture"})
            continue
        if expected["captured_at"] != item["captured_at"]:
            mismatches.append({"frame_id": fid, "field": "captured_at"})
        for field, key in (
            ("raw_text_sha256", "raw_text"),
            ("content_text_sha256", "content_text"),
            ("vision_description_sha256", "vision_description"),
        ):
            want = expected[field]
            got = sha256_hex(item[key])
            if want != got:
                mismatches.append(
                    {"frame_id": fid, "field": field, "expected_sha256": want, "observed_sha256": got}
                )


def compare_detail(detail, fixture, mismatches):
    if not isinstance(detail, dict):
        raise ProtocolError("frame detail is not an object")
    fid = detail.get("frame_id")
    expected = fixture.frames.get(fid)
    if expected is None:
        mismatches.append({"frame_id": fid, "field": "present", "reason": "detail frame not in fixture"})
        return fid
    if detail.get("captured_at") != expected["captured_at"]:
        mismatches.append({"frame_id": fid, "field": "captured_at"})
    vision = detail.get("vision")
    vision_description = vision.get("description") if isinstance(vision, dict) else None
    for field, got in (
        ("raw_text_sha256", sha256_hex(detail.get("raw_text"))),
        ("content_text_sha256", sha256_hex(detail.get("content_text"))),
        ("vision_description_sha256", sha256_hex(vision_description)),
    ):
        if expected[field] != got:
            mismatches.append(
                {"frame_id": fid, "field": field, "expected_sha256": expected[field], "observed_sha256": got}
            )
    if "image_purged" not in detail or not isinstance(detail["image_purged"], bool):
        raise ProtocolError("frame detail is missing a boolean image_purged")
    return fid


def verify_coverage(coverage, start, end):
    if not isinstance(coverage, dict):
        raise ProtocolError("coverage is not an object")
    for field in ("window", "observed_at", "frame_id_high_water", "capture_enabled", "frames_stored", "frames_text_available", "capture_observations"):
        if field not in coverage:
            raise ProtocolError(f"coverage is missing {field}")
    window = coverage.get("window")
    if not isinstance(window, dict) or window.get("from") != start or window.get("to") != end:
        raise ProtocolError("coverage window does not match the selected UTC window")
    for field in ("observed_at", "frame_id_high_water", "frames_stored", "frames_text_available"):
        if type(coverage.get(field)) is not int or coverage[field] < 0:
            raise ProtocolError(f"coverage {field} must be a non-negative integer")
    if coverage["frames_text_available"] > coverage["frames_stored"]:
        raise ProtocolError("coverage frames_text_available exceeds frames_stored")
    if not isinstance(coverage.get("capture_enabled"), bool):
        raise ProtocolError("coverage capture_enabled must be a boolean")
    obs = coverage.get("capture_observations")
    if not isinstance(obs, dict):
        raise ProtocolError("coverage capture_observations must be an object")
    for field in ("observation_started_at", "unavailable_before", "truncated", "intervals"):
        if field not in obs:
            raise ProtocolError(f"coverage capture_observations is missing {field}")
    if not isinstance(obs["truncated"], bool):
        raise ProtocolError("coverage capture_observations.truncated must be a boolean")
    if not isinstance(obs["intervals"], list):
        raise ProtocolError("coverage capture_observations.intervals must be an array")
    for interval in obs["intervals"]:
        if not isinstance(interval, dict):
            raise ProtocolError("a capture interruption is not an object")
        if "end" not in interval or "reason" not in interval:
            raise ProtocolError("a capture interruption is missing end/reason")
        if interval["end"] is not None and not isinstance(interval["end"], int):
            raise ProtocolError("a capture interruption end must be an integer or null")
        if interval["reason"] not in ("capture_stopped", "worker_error"):
            raise ProtocolError("a capture interruption has an unknown reason")
    return {
        "observed_at": coverage["observed_at"],
        "frame_id_high_water": coverage["frame_id_high_water"],
        "capture_enabled": coverage["capture_enabled"],
        "frames_stored": coverage["frames_stored"],
        "frames_text_available": coverage["frames_text_available"],
        "open_intervals": sum(1 for i in obs["intervals"] if i["end"] is None),
        "intervals": len(obs["intervals"]),
        "truncated": obs["truncated"],
        "unavailable_before": obs["unavailable_before"],
    }


def apply_coverage_checks(coverage, checks):
    """Return (results, failures). Each result proves a null/unknown field survived."""
    results = []
    failures = []
    for check in checks:
        pointer = check["pointer"]
        entry = {"pointer": pointer}
        try:
            value = json_pointer(coverage, pointer)
            present = True
        except (KeyError, IndexError, ValueError):
            value = None
            present = False
        if "present" in check:
            ok = present == bool(check["present"])
            entry["present"] = present
        elif "is_null" in check:
            # Honor the boolean: is_null true => the field must be null; false => present,
            # not null. This is how an operator asserts a null/unknown field was preserved.
            ok = present and ((value is None) == bool(check["is_null"]))
            entry["present"] = present
        else:
            ok = present and value == check["equals"]
            entry["present"] = present
        entry["ok"] = bool(ok)
        results.append(entry)
        if not ok:
            failures.append({"pointer": pointer})
    return results, failures


# --------------------------------------------------------------------------- report validation


def _fail(exc, message):
    raise exc(message)


def _require_exact_fields(obj, fields, what, exc: type[Exception] = ProtocolError):
    if not isinstance(obj, dict):
        _fail(exc, f"{what} is not an object")
    keys = set(obj.keys())
    want = set(fields)
    if keys != want:
        missing = sorted(want - keys)
        extra = sorted(keys - want)
        _fail(exc, f"{what} has the wrong field set (missing={missing}, extra={extra})")


def _require_int(value, what, exc: type[Exception] = ProtocolError, minimum=None):
    # `type(...) is int` (not isinstance) so JSON true/false is never accepted as 1/0.
    if type(value) is not int:
        _fail(exc, f"{what} must be an integer")
    if minimum is not None and value < minimum:
        _fail(exc, f"{what} must be >= {minimum}")
    return value


def _require_nonempty_str(value, what, exc: type[Exception] = ProtocolError):
    if not isinstance(value, str) or value == "":
        _fail(exc, f"{what} must be a non-empty string")
    return value


def _require_nullable_str(value, what, exc: type[Exception] = ProtocolError):
    if value is not None and not isinstance(value, str):
        _fail(exc, f"{what} must be a string or null")
    return value


def _normalize_requested_range(value, what, exc):
    if value is None:
        return None
    _require_exact_fields(value, ("start_date", "end_date"), f"{what}.requested_range", exc)
    for key in ("start_date", "end_date"):
        if not isinstance(value[key], str) or not DATE_RE.fullmatch(value[key]):
            _fail(exc, f"{what}.requested_range.{key} must be a strict YYYY-MM-DD date")
    return {"start_date": value["start_date"], "end_date": value["end_date"]}


def normalize_report_summary(item, start, end, exc: type[Exception] = ProtocolError):
    """Validate one `ReportSummary` (list row) against the frozen wire contract and return it.

    Exact field set, JSON types, explicit nulls, enum membership, the §8c error-code allowlist
    and half-open window overlap (`window_start < to AND window_end > from`) are enforced. A
    list row is a summary only, so a saved `result`/`markdown` key is rejected as a bad field.
    """
    _require_exact_fields(item, REPORT_SUMMARY_FIELDS, "report summary", exc)
    run_id = _require_nonempty_str(item["run_id"], "report summary run_id", exc)
    source_id = _require_nonempty_str(item["source_id"], "report summary source_id", exc)
    revision_of = _require_nullable_str(item["revision_of"], "report summary revision_of", exc)
    requested_range = _normalize_requested_range(item["requested_range"], "report summary", exc)
    window_start = _require_int(item["window_start"], "report summary window_start", exc)
    window_end = _require_int(item["window_end"], "report summary window_end", exc)
    if window_start >= window_end:
        _fail(exc, "report summary window_start must be strictly before window_end")
    if not (window_start < end and window_end > start):
        _fail(exc, "report summary window does not overlap the selected [from, to) window")
    timezone = _require_nonempty_str(item["timezone"], "report summary timezone", exc)
    local_date = item["local_date"]
    if not isinstance(local_date, str) or not DATE_RE.fullmatch(local_date):
        _fail(exc, "report summary local_date must be a strict YYYY-MM-DD date")
    created_at = _require_int(item["created_at"], "report summary created_at", exc)
    updated_at = _require_int(item["updated_at"], "report summary updated_at", exc)
    phase = item["phase"]
    if phase not in REPORT_PHASES:
        _fail(exc, "report summary phase is not one of queued|running|blocked|finished")
    outcome = item["outcome"]
    if outcome is not None and outcome not in REPORT_OUTCOMES:
        _fail(exc, "report summary outcome is not a known ReportOutcome")
    error_code = item["error_code"]
    if error_code is not None and error_code not in REPORT_ERROR_CODES:
        _fail(exc, "report summary error_code is not in the §8c allowlist")
    return {
        "run_id": run_id,
        "source_id": source_id,
        "revision_of": revision_of,
        "requested_range": requested_range,
        "window_start": window_start,
        "window_end": window_end,
        "timezone": timezone,
        "local_date": local_date,
        "created_at": created_at,
        "updated_at": updated_at,
        "phase": phase,
        "outcome": outcome,
        "error_code": error_code,
    }


def _normalize_saved_report(result, markdown_key, what, exc):
    _require_exact_fields(result, (markdown_key,) + SAVED_REPORT_TAIL_FIELDS, what, exc)
    payload = result[markdown_key]
    if not isinstance(payload, str):
        _fail(exc, f"{what} {markdown_key} must be a string")
    model = _require_nullable_str(result["model"], f"{what} model", exc)
    arrays = {}
    for key in ("frame_ids", "session_ids", "cited_frame_ids"):
        value = result[key]
        if not isinstance(value, list) or any(type(x) is not int for x in value):
            _fail(exc, f"{what} {key} must be an array of integers")
        arrays[key] = list(value)
    counts = {
        key: _require_int(result[key], f"{what} {key}", exc, minimum=0)
        for key in (
            "frames_sampled",
            "frames_summarized",
            "periods_total",
            "periods_covered",
            "passes",
        )
    }
    truncated = result["truncated"]
    if not isinstance(truncated, bool):
        _fail(exc, f"{what} truncated must be a boolean")
    return {markdown_key: payload, "model": model, **arrays, **counts, "truncated": truncated}


def normalize_saved_report(result, exc: type[Exception] = ProtocolError):
    """Validate the live `SavedReport` (a detail `result`) and return it with the real Markdown."""
    return _normalize_saved_report(result, "markdown", "saved report", exc)


def normalize_report_detail(detail, start, end, exc: type[Exception] = ProtocolError):
    """Validate one live `ReportDetail` (all summary fields + `result`) and return it."""
    _require_exact_fields(detail, REPORT_DETAIL_FIELDS, "report detail", exc)
    summary = normalize_report_summary(
        {key: detail[key] for key in REPORT_SUMMARY_FIELDS}, start, end, exc
    )
    result = detail["result"]
    normalized = dict(summary)
    normalized["result"] = None if result is None else normalize_saved_report(result, exc)
    return normalized


def normalize_expected_report_detail(detail, start, end, exc: type[Exception] = InputError):
    """Validate one manifest `expected_details` entry (its result uses `markdown_sha256`)."""
    _require_exact_fields(detail, REPORT_DETAIL_FIELDS, "expected report detail", exc)
    summary = normalize_report_summary(
        {key: detail[key] for key in REPORT_SUMMARY_FIELDS}, start, end, exc
    )
    result = detail["result"]
    normalized = dict(summary)
    if result is None:
        normalized["result"] = None
    else:
        normalized["result"] = _normalize_saved_report(
            result, "markdown_sha256", "expected saved report", exc
        )
    return normalized


def verify_report_page(page, start, end):
    """Validate a `ReportPage` (`items` + explicit `next_cursor`) and return (items, cursor).

    Items must be strictly descending by `(created_at, run_id)`; a non-terminal cursor must be
    a non-empty string within the 4096-byte cap.
    """
    _require_exact_fields(page, REPORT_PAGE_FIELDS, "report page")
    next_cursor = page["next_cursor"]
    if next_cursor is not None:
        if not isinstance(next_cursor, str) or next_cursor == "":
            raise ProtocolError("report page next_cursor must be a non-empty string or null")
        if len(next_cursor.encode("utf-8")) > REPORT_CURSOR_MAX_BYTES:
            raise ProtocolError("report page next_cursor exceeds the 4096-byte cap")
    items = page["items"]
    if not isinstance(items, list):
        raise ProtocolError("report page items is not an array")
    normalized = [normalize_report_summary(item, start, end) for item in items]
    previous = None
    for summary in normalized:
        key = (summary["created_at"], summary["run_id"])
        if previous is not None and key >= previous:
            raise ProtocolError(
                "report page items are not strictly descending by (created_at, run_id)"
            )
        previous = key
    return normalized, next_cursor


def normalize_expected_reports(block, start, end, known_frame_ids):
    """Validate the manifest `reports` block and return `(expected_summaries, expected_details)`.

    Both arrays are validated against the frozen public DTOs; they must correspond one-to-one by
    `run_id`, be unique and descending by `(created_at, run_id)`, and at least one saved result
    must reference a frame also present in the frame fixture so the report -> source-frame-detail
    chain can be resolved.
    """
    expected_summaries = [
        normalize_report_summary(entry, start, end, exc=InputError)
        for entry in block["expected_summaries"]
    ]
    expected_details = [
        normalize_expected_report_detail(entry, start, end) for entry in block["expected_details"]
    ]
    summary_ids = [s["run_id"] for s in expected_summaries]
    detail_ids = [d["run_id"] for d in expected_details]
    if summary_ids != detail_ids:
        raise InputError(
            "fixture reports expected_details must correspond one-to-one (same run_id order) as "
            "expected_summaries"
        )
    if len(set(summary_ids)) != len(summary_ids):
        raise InputError("fixture reports expected_summaries has a duplicate run_id")
    keys = [(s["created_at"], s["run_id"]) for s in expected_summaries]
    if any(keys[i] <= keys[i + 1] for i in range(len(keys) - 1)):
        raise InputError(
            "fixture reports expected_summaries must be in descending (created_at, run_id) order"
        )
    saved_frames = [
        fid
        for detail in expected_details
        if detail["result"] is not None
        for fid in detail["result"]["frame_ids"]
    ]
    if not any(fid in known_frame_ids for fid in saved_frames):
        raise InputError(
            "fixture reports must reference at least one saved frame that is also present in "
            "'frames', so the report -> source-frame-detail chain can be resolved"
        )
    return expected_summaries, expected_details


def compare_report_detail(observed, expected, mismatches):
    """Compare one observed `ReportDetail` to its manifest expectation (Markdown via digest).

    Only field names, ids, counts and hashes are ever recorded — never the Markdown text.
    """
    run_id = expected["run_id"]
    for field in REPORT_SUMMARY_FIELDS:
        if observed[field] != expected[field]:
            mismatches.append(
                {
                    "run_id": run_id,
                    "field": field,
                    "expected": expected[field],
                    "observed": observed[field],
                }
            )
    expected_result = expected["result"]
    observed_result = observed["result"]
    if (expected_result is None) != (observed_result is None):
        mismatches.append(
            {
                "run_id": run_id,
                "field": "result.present",
                "expected_present": expected_result is not None,
                "observed_present": observed_result is not None,
            }
        )
        return
    if expected_result is None:
        return
    for field in SAVED_REPORT_TAIL_FIELDS:
        if observed_result[field] != expected_result[field]:
            mismatches.append(
                {
                    "run_id": run_id,
                    "field": f"result.{field}",
                    "expected": expected_result[field],
                    "observed": observed_result[field],
                }
            )
    observed_digest = sha256_hex(observed_result["markdown"])
    if observed_digest != expected_result["markdown_sha256"]:
        mismatches.append(
            {
                "run_id": run_id,
                "field": "result.markdown_sha256",
                "expected_sha256": expected_result["markdown_sha256"],
                "observed_sha256": observed_digest,
            }
        )


# --------------------------------------------------------------------------- HTTP probes


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_a, **_k):  # pragma: no cover - direct requests only
        return None


def _opener():
    return urllib.request.build_opener(_NoRedirect, urllib.request.ProxyHandler({}))


def http_probe(origin, token, method, path, body=None, timeout=30.0):
    """One real HTTP request to the actual app. Returns the status code (never raises on
    a 4xx — those are the expected probe outcomes). Connection/transport failure raises
    ApiError. No image is ever fetched and no capture/Ask endpoint is ever used."""
    data = None
    headers = {"Authorization": f"Bearer {token}", "Accept": "application/json"}
    if body is not None:
        data = json.dumps(body, separators=(",", ":")).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(origin + path, data=data, headers=headers, method=method)
    try:
        with _opener().open(request, timeout=timeout) as response:
            response.read()
            return response.status
    except urllib.error.HTTPError as exc:
        exc.close()
        return exc.code
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise ApiError(message="the isolated app API could not be reached") from exc


def probe_invalid_token(origin):
    """A GET with a knowingly-wrong bearer must be refused with 401 (read-only, safe)."""
    return http_probe(origin, INVALID_TOKEN, "GET", "/v1/marks")


def probe_read_token_frame_mark(origin, token, frame_id):
    """A read token POSTing a mark for an *existing* frame must be refused with 403.

    This never sends `"now": true` (no capture), never targets Ask, and never loads a
    model; the body names an existing non-private fixture frame only."""
    return http_probe(origin, token, "POST", "/v1/marks", body={"frame_id": frame_id})


# --------------------------------------------------------------------------- report


def new_report():
    return {
        "driver": "mcp_memory_acceptance",
        "driver_version": 1,
        "mode": "read-only MCP -> HTTP",
        "status": "unknown",
        "window": None,
        "counts": {},
        "ids": {},
        "hashes": {"coverage_sha256": None, "pages": [], "details": []},
        "reports": {
            "pages": 0,
            "summaries": 0,
            "details": 0,
            "saved_results": 0,
            "null_results": 0,
            "queued_running_with_saved": 0,
            "phases": {},
            "outcomes": {},
            "summary_sha256": None,
            "page_sha256": [],
            "detail_sha256": [],
            "chain_source_frame_id": None,
        },
        "checks": {},
        "p3_reports": "unknown",
        "p4_full": "not_claimed",
        "native_app_attestation": "operator-supplied origin (driver cannot itself attest the target)",
        "actual_cross_machine_run": "not_performed",
        "pending": [],
    }


def _emit(report):
    sys.stdout.write(
        json.dumps(report, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n"
    )
    sys.stdout.flush()


# --------------------------------------------------------------------------- orchestration


def _require(condition, message):
    if not condition:
        raise InputError(message)


def run(args, report):
    checks = Checks()
    report["checks"] = checks.items

    # -- 1. explicit inputs only -------------------------------------------------
    origin = validate_api_url(args.api_url)
    start = parse_utc_ms(args.start, "--from")
    end = parse_utc_ms(args.end, "--to")
    _require(start is not None and end is not None, "--from and --to are required")
    _require(start < end, "--from must be strictly before --to")
    report["window"] = {"from": start, "to": end}

    token = os.environ.get("SCREENSEARCH_API_TOKEN", "")
    _require(bool(token) and len(token) >= 16, "set SCREENSEARCH_API_TOKEN to the read-only bearer")

    exe = Path(args.exe)
    _require(exe.is_file(), "--exe does not point at an existing screensearch-mcp binary")

    fixture = Fixture(load_json_strict(Path(args.fixture).read_text(encoding="utf-8"), "fixture manifest"), str(args.fixture))
    if isinstance(fixture.declared_window, dict):
        _require(
            fixture.declared_window.get("from") == start and fixture.declared_window.get("to") == end,
            "fixture window does not match --from/--to",
        )
    _require(1 <= args.limit <= 1000, "--limit must be within 1..=1000")
    _require(1 <= args.report_page_size <= 1000, "--report-page-size must be within 1..=1000")

    # The optional report extension is validated up front against the same window and frame
    # fixture, so a malformed manifest fails as invalid_input before any process starts.
    reports_present = fixture.reports is not None
    expected_summaries = []
    expected_details = []
    if reports_present:
        expected_summaries, expected_details = normalize_expected_reports(
            fixture.reports, start, end, set(fixture.frames)
        )
    # Set the report-step status up front so an early failure still tells the truth about the
    # manifest: the final roll-up overwrites this with ok/failed once the step has run.
    report["p3_reports"] = (
        "pending: reports block present; report step not yet executed"
        if reports_present
        else "not_exercised: manifest has no reports block"
    )

    expected_ids = fixture.ordered_ids()
    expected_tuples = fixture.ordered_tuples()
    report["counts"].update(
        {
            "expected_frames": len(expected_ids),
            "expected_textless": len(fixture.textless),
            "expected_available_empty": len(fixture.available_empty),
        }
    )

    # -- 2. state (durable cursor) ------------------------------------------------
    state = State(args.state, origin, start, end)
    if args.rescan:
        state.rescan()

    client = McpChild(exe, origin, token, timeout=args.timeout)
    received = []
    page_hashes = []
    mismatches = []
    resumed_from_cursor = False
    exchange_observed = False
    try:
        # -- 3. handshake + tool surface ------------------------------------------
        init = client.handshake()
        server_info = init.get("serverInfo", {})
        checks.record(
            "initialize",
            True,
            {
                "protocol_version": init.get("protocolVersion"),
                "server_name": server_info.get("name"),
                "server_version": server_info.get("version"),
            },
        )
        names = client.list_tools()
        strict = names == READ_TOOL_ORDER
        if strict or args.allow_tool_drift:
            checks.record("read_only_tool_set", True, {"tools": names, "strict_equal": strict})
        else:
            checks.record("read_only_tool_set", False, {"tools": names, "strict_equal": False})

        for bad in FORBIDDEN_TOOLS:
            result = client.call_tool(
                bad, {"query": "acceptance", "frame_id": fixture.post_target_frame_id}
            )
            text = client.tool_error_text(result)
            ok = result.get("isError") is True and "read-only" in text.lower()
            checks.record(f"refuse_{bad}", ok)

        # -- 4. coverage ----------------------------------------------------------
        coverage = client.tool_json("get_capture_coverage", {"from": start, "to": end})
        exchange_observed = True
        coverage_counts = verify_coverage(coverage, start, end)
        report["hashes"]["coverage_sha256"] = sha256_hex(canonical_json_bytes(coverage))
        checks.record("coverage_shape", True, coverage_counts)
        results, failures = apply_coverage_checks(coverage, fixture.coverage_checks)
        checks.record(
            "coverage_null_unknown_preserved",
            not failures,
            {"checks": len(results), "failed": failures},
        )

        # -- 5. frame pages with cursor, restart + resume --------------------------
        cp = state.checkpoint()
        if cp["done"] and not args.rescan:
            received = []
        pages_this_run = 0
        restarted = False
        while not state.checkpoint()["done"] and pages_this_run < args.max_pages:
            cp = state.checkpoint()
            if cp["cursor"] is None:
                call_args = {"from": start, "to": end, "limit": args.limit}
            else:
                call_args = {"cursor": cp["cursor"], "limit": args.limit}
                if restarted:
                    resumed_from_cursor = True
            state.record_list_call(
                len(call_args), len(cp["cursor"].encode("utf-8")) if cp["cursor"] else 0
            )
            page = client.tool_json("list_frames", call_args)
            items = verify_page(page, start, end)
            compare_to_fixture(items, fixture, mismatches)
            state.commit_page(
                history_id=page["history_id"],
                snapshot_max=page["snapshot_max_frame_id"],
                cursor=page["next_cursor"],
                done=page["next_cursor"] is None,
                items=items,
            )
            received.extend(items)
            page_hashes.append(sha256_hex(canonical_json_bytes(page)))
            pages_this_run += 1
            if args.restart_after and pages_this_run == args.restart_after and not restarted:
                client.restart()
                state.bump_restart()
                restarted = True
                client.handshake()
                post_names = client.list_tools()
                checks.record(
                    "tools_preserved_across_restart",
                    post_names == READ_TOOL_ORDER or args.allow_tool_drift,
                    {"tools": post_names},
                )

        final = state.checkpoint()
        drained = final["done"] or pages_this_run >= args.max_pages
        checks.record(
            "frame_pages",
            drained and final["list_frames_calls"] == final["pages_fetched"],
            {
                "pages_fetched": final["pages_fetched"],
                "frames_seen": final["frames_seen"],
                "list_frames_calls": final["list_frames_calls"],
                "cursor_terminal": final["done"],
                "max_pages_hit": bool(final["done"] is False and pages_this_run >= args.max_pages),
            },
        )
        checks.record(
            "process_restart_resume",
            restarted and (resumed_from_cursor or final["done"]),
            {"restarts": final["restarts"], "resumed_from_cursor": resumed_from_cursor},
        )
        checks.record("frame_matches_fixture", not mismatches, {"mismatches": mismatches[:20], "count": len(mismatches)})

        # -- 6. saved report pages + details --------------------------------------
        # Optional P3 read surface: the frozen `list_reports`/`get_report` tools over the
        # manifest's own UTC window. Every report check runs when the manifest carries a
        # `reports` block; a frame-only manifest leaves the step not_exercised, never P4.
        report_page_hashes = []
        report_detail_hashes = []
        report_mismatches = []
        report_source_ids = []
        report_phase_counts = {}
        report_outcome_counts = {}
        report_state = {"saved_results": 0, "null_results": 0, "queued_running_with_saved": 0}
        report_chain_source_id = None
        report_chain_source = None
        if reports_present:
            limit = args.report_page_size
            call_args = {"from": start, "to": end, "limit": limit}
            observed_summaries = []
            report_pages = 0
            terminal = False
            last_key = None
            previous_cursor = None
            while True:
                page = client.tool_json("list_reports", call_args)
                items, next_cursor = verify_report_page(page, start, end)
                for summary in items:
                    key = (summary["created_at"], summary["run_id"])
                    if last_key is not None and key >= last_key:
                        raise ProtocolError("report rows are not strictly descending across pages")
                    last_key = key
                observed_summaries.extend(items)
                report_page_hashes.append(sha256_hex(canonical_json_bytes(page)))
                report_pages += 1
                if next_cursor is None:
                    terminal = True
                    break
                if next_cursor == previous_cursor:
                    raise ProtocolError("report next_cursor did not advance between pages")
                if report_pages >= args.max_pages:
                    break
                previous_cursor = next_cursor
                call_args = {"cursor": next_cursor, "limit": limit}
            checks.record(
                "report_pages",
                terminal and report_pages >= 1,
                {
                    "pages": report_pages,
                    "summaries": len(observed_summaries),
                    "cursor_terminal": terminal,
                    "page_size": limit,
                    "max_pages_hit": bool(not terminal and report_pages >= args.max_pages),
                },
            )
            checks.record(
                "report_list_matches_manifest",
                observed_summaries == expected_summaries,
                {"expected": len(expected_summaries), "observed": len(observed_summaries)},
            )
            for expected in expected_details:
                detail = client.tool_json("get_report", {"run_id": expected["run_id"]})
                observed = normalize_report_detail(detail, start, end)
                compare_report_detail(observed, expected, report_mismatches)
                report_detail_hashes.append(sha256_hex(canonical_json_bytes(detail)))
                if observed["result"] is not None:
                    report_state["saved_results"] += 1
                    report_source_ids.extend(observed["result"]["frame_ids"])
                    if observed["phase"] in ("queued", "running"):
                        report_state["queued_running_with_saved"] += 1
                else:
                    report_state["null_results"] += 1
                report_phase_counts[observed["phase"]] = (
                    report_phase_counts.get(observed["phase"], 0) + 1
                )
                outcome_key = "null" if observed["outcome"] is None else observed["outcome"]
                report_outcome_counts[outcome_key] = report_outcome_counts.get(outcome_key, 0) + 1
            checks.record(
                "report_details",
                not report_mismatches,
                {
                    "checked": len(expected_details),
                    "mismatches": report_mismatches[:20],
                    "count": len(report_mismatches),
                },
            )
            # A stored immutable result must stay readable regardless of delivery phase
            # (§7f.10 resolved for acceptance). A `result.present` mismatch is exactly the
            # "phase suppressed the stored result" failure this check guards against.
            checks.record(
                "report_phase_never_suppresses_result",
                not any(m.get("field") == "result.present" for m in report_mismatches),
                {"queued_running_with_saved": report_state["queued_running_with_saved"]},
            )

        # -- 7. detail (source frame resolved from the saved report, then the window) --
        seen_ids = [item["frame_id"] for item in received] or [int(x) for x in []]
        detail_ids = [fid for fid in expected_ids if fid in set(seen_ids)]
        if not detail_ids:
            detail_ids = expected_ids
        if reports_present:
            # Prefer a frame the saved result actually returned; fall back to the expected
            # saved frames (normalize_expected_reports guarantees at least one is in the
            # fixture) so the report -> source-frame-detail chain always resolves.
            observed_frames = [fid for fid in report_source_ids if fid in fixture.frames]
            expected_frames = [
                fid
                for detail in expected_details
                if detail["result"] is not None
                for fid in detail["result"]["frame_ids"]
                if fid in fixture.frames
            ]
            resolvable = observed_frames or expected_frames
            report_chain_source_id = resolvable[0]
            report_chain_source = "observed" if observed_frames else "expected"
            if report_chain_source_id not in detail_ids:
                detail_ids = [report_chain_source_id] + detail_ids
        if args.detail_limit:
            detail_ids = detail_ids[: args.detail_limit]
        detail_mismatches = []
        detail_hashes = []
        image_requests = 0
        chain_ok = False
        for fid in detail_ids:
            detail = client.tool_json("get_moment", {"frame_id": fid})
            compare_detail(detail, fixture, detail_mismatches)
            detail_hashes.append(sha256_hex(canonical_json_bytes(detail)))
            if report_chain_source_id is not None and fid == report_chain_source_id:
                chain_ok = True
        checks.record(
            "detail_hashes",
            not detail_mismatches,
            {"checked": len(detail_ids), "mismatches": detail_mismatches[:20], "count": len(detail_mismatches), "image_requests": image_requests},
        )
        report["hashes"]["details"] = detail_hashes
        if reports_present:
            checks.record(
                "report_source_frame_detail",
                chain_ok,
                {
                    "source_frame_id": report_chain_source_id,
                    "resolved_from_saved_report": True,
                    "source": report_chain_source,
                },
            )

        # -- 8. permission probes -------------------------------------------------
        if args.probes:
            status = probe_invalid_token(origin)
            checks.record("direct_invalid_token_401", status == 401, {"status": status})
            status = probe_read_token_frame_mark(origin, token, fixture.post_target_frame_id)
            checks.record(
                "direct_read_token_post_403",
                status == 403,
                {"status": status, "sent_now_true": False, "invoked_model": False},
            )
        else:
            checks.record("direct_invalid_token_401", False, {"skipped": True})
            checks.record("direct_read_token_post_403", False, {"skipped": True})

    finally:
        client.close()
        state.close()

    # -- 9. report ----------------------------------------------------------------
    received_ids = [item["frame_id"] for item in received]
    received_tuples = [(item["captured_at"], item["frame_id"]) for item in received]
    report["ids"] = {
        "expected": expected_ids,
        "received": received_ids,
        "expected_sha256": sha256_hex(canonical_json_bytes(expected_tuples)),
        "received_sha256": sha256_hex(canonical_json_bytes(received_tuples)),
        "ordered_equal": expected_tuples == received_tuples,
        "set_equal": set(expected_ids) == set(received_ids),
    }
    checks.record("frame_id_set", set(expected_ids) == set(received_ids) if received_ids else False)
    checks.record("frame_order", expected_tuples == received_tuples if received_ids else False)
    report["hashes"]["pages"] = page_hashes
    report["counts"].update(
        {
            "frames_received": len(received_ids),
            "pages_fetched": len(page_hashes),
            "details_checked": len(report["hashes"]["details"]),
            "hash_mismatches": len(mismatches),
        }
    )

    # Record the report step's own roll-up and hashes (sanitized: ids/counts/hashes only).
    report["reports"]["pages"] = len(report_page_hashes)
    report["reports"]["page_sha256"] = report_page_hashes
    report["reports"]["detail_sha256"] = report_detail_hashes
    report["reports"]["summaries"] = len(expected_summaries)
    report["reports"]["details"] = len(expected_details)
    report["reports"]["saved_results"] = report_state["saved_results"]
    report["reports"]["null_results"] = report_state["null_results"]
    report["reports"]["queued_running_with_saved"] = report_state["queued_running_with_saved"]
    report["reports"]["phases"] = report_phase_counts
    report["reports"]["outcomes"] = report_outcome_counts
    report["reports"]["chain_source_frame_id"] = report_chain_source_id
    if reports_present:
        report["reports"]["summary_sha256"] = sha256_hex(
            canonical_json_bytes(
                [{key: s[key] for key in REPORT_SUMMARY_FIELDS} for s in expected_summaries]
            )
        )
        report_checks = [name for name in checks.items if name.startswith("report_")]
        report["p3_reports"] = (
            "ok"
            if report_checks and all(checks.items[name]["ok"] for name in report_checks)
            else "failed"
        )
    else:
        report["p3_reports"] = "not_exercised: manifest has no reports block"

    # The driver performed a real MCP -> HTTP run against the operator-supplied origin, but it
    # cannot itself attest that origin is the isolated cross-machine Windows app. The
    # "exchange observed" wording is emitted only when the coverage read actually succeeded, so
    # an unavailable origin never inherits a task-progress claim.
    if exchange_observed:
        report["actual_cross_machine_run"] = "not_attested_by_driver"
        report["native_app_attestation"] = (
            "origin exchange observed by the driver; physical host and native-app identity "
            "require independent operator evidence"
        )
    else:
        report["actual_cross_machine_run"] = "not_performed"
        report["native_app_attestation"] = (
            "not_attested: the driver did not complete an origin exchange"
        )
    report["pending"] = [
        "Native-app isolation and a direct separate-machine path are not attested by this driver.",
    ]
    if not reports_present:
        report["pending"].append(
            "Saved-report tools are not exercised; their acceptance requires a separate gate."
        )
    if checks.all_ok():
        report["status"] = "ok"
        return EXIT_OK
    report["status"] = "failed"
    return EXIT_FAIL


# --------------------------------------------------------------------------- entrypoint


def build_parser():
    parser = argparse.ArgumentParser(
        prog="mcp_memory_acceptance.py",
        description="Cross-machine read-only MCP -> HTTP acceptance driver (no listener, no canned HTTP).",
    )
    parser.add_argument("--exe", required=True, help="path to the native screensearch-mcp binary")
    parser.add_argument("--api-url", required=True, help="explicit origin of the isolated app API")
    parser.add_argument("--from", dest="start", required=True, help="UTC window start (epoch-ms or ISO-8601 Z)")
    parser.add_argument("--to", dest="end", required=True, help="UTC window end (exclusive)")
    parser.add_argument("--fixture", required=True, help="non-private expected-fixture manifest (JSON)")
    parser.add_argument("--state", required=True, help="private state DB path (cursor checkpoint)")
    parser.add_argument("--limit", type=int, default=100, help="page size 1..1000 (default 100)")
    parser.add_argument(
        "--report-page-size",
        dest="report_page_size",
        type=int,
        default=100,
        help="saved-report page size 1..1000 (default 100); a small value forces report pagination",
    )
    parser.add_argument("--max-pages", type=int, default=0, help="stop after N pages (0 = drain fully)")
    parser.add_argument("--restart-after", type=int, default=1, help="restart the wrapper after N pages (0 = never)")
    parser.add_argument("--detail-limit", type=int, default=0, help="verify detail for at most N frames (0 = all)")
    parser.add_argument("--timeout", type=float, default=120.0, help="per-request timeout seconds")
    parser.add_argument("--rescan", action="store_true", help="discard a stored cursor and re-traverse")
    parser.add_argument("--allow-tool-drift", action="store_true", help="downgrade an unexpected read-only tool set to a note")
    parser.add_argument("--no-probes", dest="probes", action="store_false", help="skip the 401/403 permission probes")
    parser.set_defaults(probes=True)
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.max_pages == 0:
        args.max_pages = 1 << 60
    report = new_report()
    try:
        code = run(args, report)
    except InputError as exc:
        report["status"] = "invalid_input"
        report["error"] = str(exc)
        code = EXIT_FAIL
    except ProtocolError as exc:
        report["status"] = "protocol_error"
        report["error"] = str(exc)
        code = EXIT_FAIL
    except ApiError as exc:
        report["status"] = "api_error"
        report["error"] = str(exc)
        code = EXIT_HTTP
    except (sqlite3.Error, OSError):
        report["status"] = "local_error"
        report["error"] = "local state/lock failure"
        code = EXIT_LOCAL
    except KeyboardInterrupt:  # pragma: no cover - interactive
        return EXIT_INTERRUPT
    _emit(report)
    return code


if __name__ == "__main__":
    sys.exit(main())
