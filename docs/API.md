# ScreenSearch HTTP API (v1)

The **HTTP API** exposes ScreenSearch's search, ask, frames, sessions, where-was-i, and
marks to scripts and agents. It is **opt-in and off by default** (0.3.0
PR7; the sessions surface added in 0.4.0 PR6, specs `03 §7c` / `§7e`).

**External-agent increment (P1/P2, unreleased):** this guide also describes the implemented
chronological frame/coverage adapters and independent read credential on
`feat/agent-memory-http`, against `crates/api/src/history.rs` and `crates/traits/src/history.rs`.
Source presence and focused component checks are not integrated Windows-app acceptance. The
portable MCP transport is a separate change; see [MCP configuration](MCP.md#configuration)
and the [verification status](TESTING.md#external-agent-memory-api-p1p2--verification-status).
The approved contract is [spec 03 §7f](../specs/03_MASTER_PRODUCTION_SPEC.md).

P3 saved-report reads are **blocked pending Nico's retry-result visibility decision**: a
queued/running delivery retry can already retain immutable saved output. Neither hiding that
output by phase nor exposing it by stored presence has been approved. `/v1/reports` and
`/v1/reports/{run_id}` are not registered in the inspected router. Remote report generation,
retry, scheduling and delivery remain deferred; neither credential grants `reports.generate`.

> **Threat model — read this before enabling.** The API uses plain HTTP without transport
> encryption. The default bind is `127.0.0.1` (local access). If you choose a network address,
> any token holder on a reachable host can read your entire screen history. Network observers
> may intercept tokens and responses. Wildcard binding does not enforce LAN-only access.
> ScreenSearch does not configure your firewall or router or provide public hosting.

## Enabling

Settings → **HTTP API** → choose a bind address and port, then toggle on.

- Bind address accepts any IPv4/IPv6 literal: `127.0.0.1`, `::1`, a specific interface address,
  or `0.0.0.0` / `::`. Hostnames, URLs and embedded ports are rejected.
- Missing address defaults to `127.0.0.1`, including existing enabled installations. Upgrading
  never broadens access automatically. Enabled state, token and port are preserved.
- Port defaults to **43210**. Address/port changes persist and apply immediately.
- A bearer token is generated on first enable; reveal, copy or regenerate it in Settings.
  Rotation takes effect immediately without restart.
- A bind failure is visible in Settings with guidance to correct the address/port.

Inference remains loopback-only. The shipped v0.4.1 MCP wrapper validates loopback destinations;
the separately implemented P2 wrapper permits explicitly selected HTTP/HTTPS API roots, still
loopback by default. See the [MCP guide](MCP.md#url-validation-and-transport-boundary) before
using a remote destination. This does not add TLS to the application's HTTP listener, change
CORS or open a firewall.

### Connecting from another computer

First select a network-capable bind address. Use the Windows computer's actual reachable address
as `SCREENSEARCH_HOST`; wildcard addresses are never client destinations. For IPv6, bracket the
address in URLs, for example `http://[::1]:43210/v1/health`. Local examples below use `127.0.0.1`.

```bash
curl -s -H "Authorization: Bearer $TOKEN" "http://$SCREENSEARCH_HOST:43210/v1/health"
```

If necessary, manually open **Windows Defender Firewall with Advanced Security → Inbound Rules
→ New Rule → Custom**, choose this ScreenSearch executable and TCP port, scope remote addresses
to the intended devices, and select only the trusted Private profile. Review existing rules
before adding one. ScreenSearch never adds a rule or forwards router ports automatically.
Do not expose this plain-HTTP endpoint to the public Internet.

`200` confirms a successful authenticated request; `401` confirms an HTTP response but rejected
credentials. A timeout is not an authentication failure: check the app, address, routing and
firewall. Connection refusal can mean the API is disabled or listening on another port.

## Connection bounds

All listeners, including loopback, allow at most **64 active connections**. Excess sockets close
immediately, before HTTP authentication, rather than receiving an HTTP error response. Clients
may retry after another connection closes. Each request has **30 seconds to send its headers**;
idle keep-alive connections close on the same next-header deadline. This does not limit the
duration of an active Ask SSE or export response. These limits are fixed, not configurable.

## Authentication

Every request to every route (health included) must send:

```
Authorization: Bearer <token>
```

A missing, malformed, revoked or wrong token returns **401**, including on unknown paths.
Use the exact `Bearer ` prefix. Credentials are compared in constant time. There are two
independent credential slots in P1:

| Credential | Server-enforced permissions |
|---|---|
| Existing/legacy token | `history.read`, `marks.write`, `capture.request`, `answer.generate`; existing clients retain these supported operations, not a wildcard grant. |
| Independently created read token | `history.read` only. It can read full raw/filtered text and stored images; it is not a redacted-history token. |

Upgrades start with **no read token**. Local Settings create/reveal/copy/rotate/revoke it
independently of the legacy token; these actions do not enable the API or alter bind/port.
Credential changes persist successfully before the live credential is swapped. Failed
persistence preserves the old credential/listener; legacy rotation leaves the read token
unchanged. Rotation/revocation affects subsequent requests without restart, not guaranteed
cancellation of a request or stream already authorized. No credential-management HTTP route
is exposed. Keep credentials out of URLs, logs and ordinary settings/export/report payloads.

| Registered operation | Legacy | Read |
|---|---|---|
| `GET`/`HEAD` health, search, frame listing, coverage, frame detail/image, context, marks, sessions/detail/cached summary, export | allow | allow |
| `POST /v1/marks` for a stored frame; `POST /v1/marks/{id}/resolve` | allow (`marks.write`) | 403 |
| `POST /v1/marks` with `now:true` | allow (`marks.write` **and** `capture.request`) | 403 |
| `POST /v1/ask`, including session Ask | allow (`answer.generate`) | 403 |

Each route/method is explicitly classified; future GETs or new tools are not implicitly
safe. **HEAD has the same authentication, permission, validation and read path as GET**, but
no response body. A valid token on an unknown path gets **404**; a valid read token on a
classified write/Ask gets **403 before side effects**, not an API-off/401 error. An unsupported
method on a classified path gets **405** with `Allow`; an unclassified registered route is
denied, not inherited by either credential. P3 report routes are not yet registered.

`--read-only` in MCP only filters listed/accepted tools. Use the independent API read token
for authorization, even when calling raw HTTP or running MCP without that flag.

```bash
curl -s -H "Authorization: Bearer ***" http://127.0.0.1:43210/v1/health
```

## Errors

API handler/authentication errors use this JSON envelope, including malformed query strings,
bodies and path segments mapped to `400 bad_request`. Exceptions: HEAD has no body, the
route/method gate's 405 has an `Allow` header and no JSON envelope, and connection-limit/header
failures can close a socket without an HTTP response. Do not parse these as empty data:

```json
{ "error": "not_found", "message": "frame 42 not found" }
```

| Status | `error`         | When |
|--------|-----------------|------|
| 400    | `bad_request`   | Malformed params/body (e.g. missing `q`, `from`/`to` not paired, `from` > `to`, invalid session `kind`, non-integer session id, `format` ≠ `json`, both `frame_id` and `now`). |
| 401    | `unauthorized`  | Missing or invalid bearer token, including on unknown paths. |
| 403    | `forbidden`     | Authenticated token lacks the classified permission; no write/capture/Ask has started. |
| 404    | `not_found`     | Unknown frame, mark, or session (or unknown endpoint with a valid token). |
| 404    | `image_purged`  | The frame exists but its screenshot was retention-purged (text is preserved). |
| 405    | *(no envelope)* | Unsupported method on a classified route; inspect `Allow`. |
| 409    | `history_changed` | Frame cursor belongs to another initialized history; restart/reconcile traversal. |
| 503    | `unavailable`   | A dependency is unavailable (answer model not loaded; capture off for `POST /v1/marks {"now":true}`; missing/invalid feed identity or allocator metadata; host without feed support). |
| 500    | `internal`      | Storage/decode failure, never an empty successful result. |

New frame/coverage errors have fixed safe messages: 400 `invalid history query or cursor`,
409 `history changed; restart the traversal`, 503 `history feed unavailable`, and 500
`history storage error`. They do not echo filters/cursors, private paths or underlying storage
diagnostics. A cursor maximum above the current allocator high-water is 400, not automatically
409; the client must also compare coverage and reconcile known restores. These stricter
validation/error rules do not redefine older endpoint query contracts.

## Endpoints

All paths are under `http://<computer-address>:<port>`; use `127.0.0.1` locally.

### `GET /v1/health`

```json
{ "version": "0.4.1", "uptime_secs": 128, "capturing": true }
```

### `GET /v1/search`

Query params: `q` (required), `from`, `to` (unix ms, half-open `[from, to)` — provide both
or neither), `limit` (default 20, clamped `1..=100`), `include_chrome` (`1`/`0` or
`true`/`false`, case-insensitive; default = your `text.include_chrome_default`).

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/search?q=invoice&limit=10"
```

Returns an array of `SearchHit` (`frame_id`, `captured_at`, `snippet`, `score`,
`image_path`, `image_purged`, `app_hint`) — the same shape and ranking as the UI.

### Chronological frames

`GET /v1/frames` (P1, unreleased) enumerates evidence without a search term, model,
OCR run or generation. This is not the ranked `/v1/search` surface.

| Input | Exact contract |
|---|---|
| Initial `from`, `to` | Both required signed `i64` UTC epoch milliseconds; `from < to`; half-open `[from,to)`. ASCII decimal integers, optional leading minus; no floats, plus sign, whitespace or overflow. |
| `app` | Optional literal exact match, ASCII-case-insensitive SQLite NOCASE. |
| `window_title` | Optional literal substring, ASCII-case-insensitive; `%`, `_` and backslash are not patterns. |
| Filter limits | Each decoded filter is at most **256 UTF-8 bytes**, before ASCII lowercase normalization. No trimming or Unicode casefold. Absent and supplied empty string are distinct. |
| `limit` | Default **100**, valid **1–1000**, rejected rather than clamped outside that range. |
| Continuation | `cursor` plus optional `limit` **only**; do not repeat bounds or filters. Limit may change between pages. Cursor input is capped at **4096 encoded bytes** before decoding. |

Unknown parameters, duplicate decoded names, malformed percent escapes, invalid UTF-8,
missing `=`, invalid integers and invalid/unsupported cursors return JSON 400. Form-query
`+` means space; URL-encode literal plus signs. App/title filters run before the limit and
use target-window metadata when nonnull, otherwise stored frame metadata. A present empty
target value is preserved; null metadata never matches a supplied filter.

Response (`FramePage`; illustrative, not live capture output):

```json
{
  "items": [{
    "frame_id": 41, "captured_at": 1000, "monitor_index": 0,
    "app_hint": null, "window_title": null, "browser_url": null,
    "raw_text": "", "content_text": "", "text_source": "ocr",
    "filter_version": 1, "text_state": "available",
    "vision_description": null, "session_id": null, "image_purged": false
  }],
  "next_cursor": null,
  "history_id": "12345678-1234-4123-8123-123456789abc",
  "window": {"from": 0, "to": 2000},
  "snapshot_max_frame_id": 41,
  "observed_at": 3000,
  "consistency": "bounded_membership_live_content"
}
```

Optional fields are present as JSON null, arrays remain arrays. Each item has the exact
fields above; `text_source` is `ocr`, `uia` or null. Raw/content text and stored vision
are **complete**, not snippets. `not_available` means no stored text row: raw/content,
source and filter version are null. `available` includes stored empty strings; absence
is not empty OCR or permanent failure. URL/session are stored observations, not inference.
Pages contain neither filesystem paths nor image bytes; use frame detail/image reads.
The page limit bounds rows, not response bytes; lower it for large text. Failures are
explicit errors, never silent text truncation.

Order is **`captured_at ASC, frame_id ASC`**. The first page reads global current maximum
frame ID and hydrated membership in one database read transaction; empty storage has
maximum 0. Later pages freeze that maximum and seek strictly after the last tuple.
`next_cursor: null` alone marks exhaustion; an empty terminal page is valid after deletion.
The opaque v1 cursor freezes history/window/normalized filters/maximum/last tuple. Current
encoding is base64url without padding, at most 791 characters for accepted filters; the
4096-byte public bound is not permission to construct cursors. Cursors are not credentials.

With supported allocator-active writers, later/new/backdated inserts cannot extend an
active traversal. Mutable text, metadata and session references remain live across pages;
retention/deletion may remove members. This is **bounded membership, live content**, not a
frozen database snapshot, deletion log, exactly-once feed or capture-completeness proof.
Re-list the same selected window to discover new frames and late enrichment.

#### Identity and reconciliation

`api.history_id` is distinct from resettable report identity and credentials. Setup creates
it and `api.frame_id_high_water` together for a new/pre-upgrade history, preserving existing
frame IDs. Supported inserts transactionally allocate above **both** persisted high-water
and current maximum; deletion does not lower the counter. Reserved metadata is excluded
from user settings import/export/reset/retired-key cleanup. Reads never initialize/repair
missing or invalid metadata: unavailable prerequisites return 503, not empty success.
A cursor for another history returns 409 `history_changed`.

These guarantees do not repair previously reused/deleted IDs or cover old binaries, direct
SQL, unsupported imports, independently active clones or backup rewind. Moving/restoring a
history preserves its identity; that is not global uniqueness across clones/restores.
Persist coverage high-water with the checkpoint: a decrease invalidates progress and requires
a fresh bounded reconciliation. A restore can catch up before it is observed, so the operator
must invalidate checkpoints after any known restore/old-writer use even without a decrease.
Same-ID/same-timestamp reuse can remain undetected.

The [reference consumer](MEMORY_CONSUMER.md) and
[`examples/memory_consumer.py`](../examples/memory_consumer.py) process facts and checkpoint
in one durable local SQLite transaction, saving the cursor **after** page writes. They upsert
by `(history_id,frame_id)` only when the recorded `captured_at` agrees. On conflict, retain
old facts, record competing versions, invalidate traversal and reconcile the selected window;
do not silently overwrite identity. The example re-reads textless IDs and periodically
re-lists that window; `--reconcile` explicitly invalidates progress after operator restore.
One traversal cannot promise eventual completeness or prove inactivity.

Calendar-day consumers must choose an explicit IANA timezone, convert **both** local date
boundaries independently to UTC, and preserve the exclusive end. Do not assume the host
zone or add 24 UTC hours: DST days may have 23 or 25 hours. The developer example accepts
`--day 2026-10-25 --timezone Europe/Paris`, or explicit UTC `--from`/`--to`; it rejects naive
bounds and nonexistent historical local dates rather than guessing.

### Coverage

`GET /v1/coverage?from=<UTC-ms>&to=<UTC-ms>` (P1, unreleased) accepts **only** required
`from`/`to`, with the same strict integer, half-open and duplicate/unknown-key rules above.
It returns `CaptureCoverage`, not an activity score:

| Response field | Meaning |
|---|---|
| `window: {from,to}`, `observed_at` | Requested UTC window and observation time in integral epoch ms. Store facts share one read transaction; controller/settings observations are composed separately. |
| `capture_enabled: bool` | Current controller **intent**, not proof of capture health. |
| `frame_id_high_water: i64` | Current persisted allocator counter, not current `MAX(id)` or the page's frozen maximum. Coverage has no `history_id`; page responses supply it. |
| `oldest_frame`, `newest_frame` | Global chronological extrema as `{frame_id,captured_at}`, ordered by timestamp then ID, not restricted to the window; null when storage is empty. |
| `newest_captured_at` | Global newest timestamp or null. |
| `frames_stored`, `frames_text_available` | Nonnegative in-window counts; available includes empty stored text, excludes missing rows. Store failures are errors, never zero-data success. |
| `image_retention_days`, `retention_scope` | Configured image policy (`0` = keep), scope `"images_only"`; not proof a purge completed. Text is preserved by image retention. |
| `saved_report_retention` | `{terminal_days:90, exempt_phases:["queued","running","blocked"], exempt_delivery_states:["pending","sending","unknown_delivery","conflict"], failed_delivery_with_pending_retry_exempt:true}`. Days count from last terminal update; policy, not per-run expiry or a report-read endpoint. |
| `capture_observations` | `{observation_started_at, unavailable_before, intervals, truncated}`; times are integral UTC ms. |

Each interval is `{start,end,reason}` with actual observed endpoints (not clipped to query
bounds), nullable `end`, and reason `capture_stopped` or `worker_error`. A null end means an
unresolved observation, not proof of continuous failure. Capture start closes a known pause.
The process-local record retains at most **1000 transitions** and returns at most **100
intersecting intervals**, chronologically. Restart/overflow makes earlier observations unknown;
backwards-clock loss remains unknown until re-observed. `truncated` marks unavailable/lost
observations or excess intersecting intervals. An empty interval array is not a complete
health record. Unobserved sleep, lock, privacy exclusion, crash or internal capture errors
are not reconstructed from frame gaps. Policy, counts and observations cannot establish
work duration or percentage of a person's day. Empty success means **no recorded frames in
this query**, not “nothing happened”; HTTP/storage failure is never `NO_DATA`.

### `POST /v1/ask`

Body: `{ "query": "...", "top_k"?: number, "thinking"?: bool, "max_tokens"?: number,
"session_id"?: number }`.
Returns a **Server-Sent Events** stream (`Content-Type: text/event-stream`); each `data:`
line is a JSON `AnswerDelta`:

```
data: {"type":"thinking","text":"…"}
data: {"type":"token","text":"The invoice total was …"}
data: {"type":"citation","frame_id":41}
data: {"type":"done"}
```

`{"type":"error","message":"…"}` may replace the terminal `done`. A keep-alive comment is
sent every 15 s. **Disconnecting the client cancels generation** — a dropped connection
stops the model rather than leaving it generating into a closed socket. Returns **503** if
no answer model is loaded.

With `session_id` set, retrieval is restricted to that session's **own** frames and the answer
cites **only** in-session frames (session ownership is exclusive, so concurrent sessions that
overlap in wall-clock time never leak into each other). An unknown `session_id` returns a JSON
**404** *before* the stream starts, and that check takes precedence over the 503-no-model case.
Absent `session_id`, behavior is unchanged.

```bash
curl -N -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"what did I read about rustls?"}' \
  http://127.0.0.1:43210/v1/ask
```

### `GET /v1/frames/{id}`

Default: the frame's metadata + text (`FrameDetail`). `?image=1`: the stored screenshot
bytes (`image/webp`, or `image/jpeg` for pre-WebP frames). Unknown id → 404; purged image
→ 404 `image_purged`.

```bash
curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:43210/v1/frames/41"
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/frames/41?image=1" --output frame.webp
```

### `GET /v1/context/where-was-i`

Returns the last sustained context before your current detour (`ResumeContext`), or `null`
when nothing qualifies.

### `GET /v1/sessions`

Lists the sessions that **overlap** the requested window. Query params: `kind`
(`focus` | `meeting` | `ai` | `other`), `tool` (a taxonomy id, one of `claude-code`, `codex`,
`claude-desktop`, `browser-ai`, `cursor`, `vscode`, `zoom`, `teams`, `meet`, `webex`,
`discord`), `from`, `to` (unix ms, **each independently optional**, unlike `/v1/search`
where they pair), `limit` (default 1000, clamped `1..=1000`). The overlap predicate is
`started_at < to AND COALESCE(ended_at, now) > from`, so an open or long-running session that
began before `from` stays visible (open sessions use request-time now). An unknown `kind` →
400 naming the valid values; `from` > `to` (both present) → 400.

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/sessions?kind=ai&tool=claude-code&limit=50"
```

Returns a bare JSON array of session objects, ordered `started_at DESC, id DESC`. `summary`
and `summary_model` are always `null` on this list surface (fetch a single session for them):

```json
[
  { "id": 42, "started_at": 1720000000000, "ended_at": 1720003600000, "open": false,
    "kind": "ai", "tool": "claude-code", "host": "terminal", "context_key": "claude-code",
    "title": "Refactor the store crate", "summary": null, "summary_model": null,
    "confidence": 0.82, "frozen": true, "created_at": 1720003600000,
    "updated_at": 1720003600000 }
]
```

`open` is the non-final marker (`true` iff `ended_at` is `null`). `title` is always present
(may be `null`); `host` is `terminal` | `desktop` | `browser` | `ide` or `null`.

### `GET /v1/sessions/{id}`

One session's detail plus its **exchange** artifacts. Query param: `include_summary`, set to
`1` to reveal the session's **cached** `summary` + `summary_model` (any other value, or absent,
serves both as `null`). It **never generates** a summary: a GET starts no inference and writes
no row (D12); lazy summary generation is an in-app-only action. Unknown id → 404; a non-integer
id → 400.

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/sessions/42?include_summary=1"
```

```json
{
  "session": { "id": 42, "started_at": 1720000000000, "ended_at": 1720003600000,
    "open": false, "kind": "ai", "tool": "claude-code", "host": "terminal",
    "context_key": "claude-code", "title": "Refactor the store crate",
    "summary": "Reworked the store crate's job queue …", "summary_model": "qwen2.5-7b",
    "confidence": 0.82, "frozen": true, "created_at": 1720003600000,
    "updated_at": 1720003600000 },
  "exchanges": [
    { "id": 7, "session_id": 42, "kind": "exchange", "role": "user", "frame_id": 2651,
      "content": "how does the worker pool claim jobs?", "created_at": 1720000100000 }
  ]
}
```

Only `kind = exchange` artifacts are returned (transcript / note are reserved and never
served). Each artifact's `role` is `user` | `agent` | `null` and `frame_id` may be `null`.

### Marks — the only write surface

- `GET /v1/marks` → all marks, unresolved first then newest-first (`Mark[]`).
- `POST /v1/marks` → create one. Body sets **exactly one** of:
  - `{ "frame_id": <id>, "note"?: "..." }` — mark an existing frame;
  - `{ "now": true, "note"?: "..." }` — capture the current screen past the diff gate,
    then mark it (503 if capture is off / a privacy gate denies).

  Returns `201 { "mark_id": <id> }`.
- `POST /v1/marks/{id}/resolve` → resolve (or dismiss) a mark → `200 { "resolved": true }`.
  Idempotent; unknown id → 404.

### `GET /v1/export`

Query params: `from`, `to` (unix ms, optional window), `format` (only `json` in v1; anything
else → 400). Streams a single JSON document:

```json
{
  "schema": "screensearch.export.v1",
  "exported_at": 1720000000000,
  "from": null,
  "to": null,
  "frames": [
    { "frame_id": 1, "captured_at": 1719999990000, "app_hint": "VS Code",
      "window_title": "main.rs", "browser_url": null, "activity_type": null,
      "content_text": "…" }
  ],
  "marks": [
    { "mark_id": 1, "frame_id": 1, "created_at": 1719999991000, "note": null,
      "resolved_at": null }
  ]
}
```

Frames carry **content text only** — never raw OCR, never image bytes (D12). The response
is streamed (paged internally), so exporting months of history stays memory-flat. If a store
error occurs mid-stream, the body is truncated and the connection closes (an honest partial
failure). The Settings **Export…** button calls the same code path to write a file to your
Downloads folder, so export works with the API disabled.

## Versioning

This is **v1**, read-only except the marks writes above. The MCP server (`docs/MCP.md`,
0.3.0 PR8) wraps this same HTTP surface for MCP clients.
