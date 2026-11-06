# ScreenSearch HTTP API (v1)

The **HTTP API** exposes ScreenSearch's search, ask, frames, sessions, where-was-i, and
marks to scripts and agents, plus the read-only thread, entity and insight surfaces
([0.4.5 P8](#threads-entities-and-insights-045-p8)). It is **opt-in and off by default** (0.3.0
PR7; the sessions surface added in 0.4.0 PR6, specs `03 §7c` / `§7e`).

**External-agent memory reads (shipped in v0.4.2).** This guide also describes the
chronological frame/coverage reads and the independent read credential
(`crates/api/src/history.rs`, `crates/traits/src/history.rs`), and the saved-report reads
`GET /v1/reports` and `GET /v1/reports/{run_id}` (see [Saved reports](#saved-reports)). The
saved-report reads use **stored-presence exposure** (`spec 03 §7f.10`): an immutable saved
result is exposed immediately whenever it exists, including while delivery is queued or
retrying. Remote report generation, retry, scheduling and delivery remain out of scope; neither
credential grants `reports.generate`. The portable MCP transport is described in
[MCP configuration](MCP.md#configuration); the evidence boundary (automated, native test-chain
and packaged-client checks; native credential-UI and cross-machine acceptance not claimed) is in
the [verification status](TESTING.md#external-agent-memory-api-p1p2--verification-status). The
approved contract is [spec 03 §7f](../specs/03_MASTER_PRODUCTION_SPEC.md).

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

Inference remains loopback-only. From v0.4.2 the bundled MCP wrapper permits explicitly
selected HTTP/HTTPS API roots, still loopback by default (v0.4.1 and earlier wrappers accepted
loopback destinations only). See the [MCP guide](MCP.md#url-validation-and-transport-boundary) before
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
independent credential slots:

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

| Operation | Legacy | Read |
|---|---|---|
| `GET`/`HEAD` health, search, frame listing, coverage, frame detail/image, context, marks, sessions/detail/cached summary, export, report listing/detail | allow | allow |
| `GET`/`HEAD` `/v1/threads`, `/v1/threads/{id}`, `/v1/entities`, `/v1/insights` (`history.read`, P8) | allow | allow |
| `POST /v1/marks` for a stored frame; `POST /v1/marks/{id}/resolve` | allow (`marks.write`) | 403 |
| `POST /v1/marks` with `now:true` | allow (`marks.write` **and** `capture.request`) | 403 |
| `POST /v1/ask`, including session Ask | allow (`answer.generate`) | 403 |

Each route/method is explicitly classified; future GETs or new tools are not implicitly
safe. **HEAD has the same authentication, permission, validation and read path as GET**, but
no response body. A valid token on an unknown path gets **404**; a valid read token on a
classified write/Ask gets **403 before side effects**, not an API-off/401 error. An unsupported
method on a classified path gets **405** with `Allow`; an unclassified registered route is
denied, not inherited by either credential. The report routes (`GET /v1/reports`,
`GET /v1/reports/{run_id}`) are classified under `§7f` and shipped in v0.4.2.

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
| 400    | `bad_request`   | Malformed params/body (e.g. missing `q`, `from`/`to` not paired, `from` > `to`, invalid session `kind`, non-integer session/thread id, an unknown or duplicate query parameter or an unknown entity/insight `kind` on the P8 reads, `format` ≠ `json`, both `frame_id` and `now`). |
| 401    | `unauthorized`  | Missing or invalid bearer token, including on unknown paths. |
| 403    | `forbidden`     | Authenticated token lacks the classified permission; no write/capture/Ask has started. |
| 404    | `not_found`     | Unknown frame, mark, session or thread (or unknown endpoint with a valid token). |
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
{ "version": "0.4.2", "uptime_secs": 128, "capturing": true }
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

`GET /v1/frames` (since v0.4.2) enumerates evidence without a search term, model,
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

`GET /v1/coverage?from=<UTC-ms>&to=<UTC-ms>` (since v0.4.2) accepts **only** required
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

### Saved reports

`GET /v1/reports` and `GET /v1/reports/{run_id}` expose **already saved** report output — never
generation, retry, scheduling or delivery. Both are read routes requiring `history.read` (legacy
**and** read token allowed; the implicit `HEAD` variant is classified with `GET`). `reports.generate`
is reserved and granted to **no** credential. Both routes are **implemented**: they are registered
in the app's router (`crates/api/src/lib.rs`) and served by the desktop host
(`src-tauri/src/local_api.rs`) over the real store. See the
[P3 verification status](TESTING.md#p3-saved-report-reads--verification-status) for the evidence
boundary — the local producer, packaged-client and native checks pass; GUI and direct
cross-machine acceptance are not claimed.

#### `GET /v1/reports` — list

Two mutually-exclusive query forms:

| Input | Exact contract |
|---|---|
| Initial `from`, `to` | Both required signed `i64` UTC epoch milliseconds; `from < to`; half-open `[from,to)`. Same strict ASCII-integer rules as `/v1/frames`. A run is listed when it **overlaps** the window: `window_start < to AND window_end > from`. |
| `phase` | Optional; one of `queued` \| `running` \| `blocked` \| `finished`. An unknown value is **400**, never ignored. |
| `outcome` | Optional; one of `OK` \| `PARTIAL` \| `NO_DATA` \| `BLOCKED` \| `FAILED`. Unknown value → **400**. |
| `limit` | Default **100**, valid **1–1000**, rejected rather than clamped outside that range. |
| Continuation | `cursor` plus optional `limit` **only**; do not repeat `from`/`to`/`phase`/`outcome`. Mixing `cursor` with those filters or any unknown parameter is **400**; changing `limit` is allowed. Cursor input is capped at **4096 encoded bytes** before decoding. |

Unknown/duplicate parameter names, malformed percent escapes, invalid UTF-8, invalid integers and
invalid cursors return JSON 400. Order is **`created_at DESC, run_id DESC`**. The opaque v1 cursor
freezes the maximum ordering tuple plus the normalized window/filters; later pages admit no tuple
above that frozen maximum and seek strictly **below** the last tuple. This bounds tuples, not a
frozen membership snapshot: equal-millisecond lower UUIDs, backdated inserts and clock rollback can
admit newly created rows below the frozen tuple, and `phase`/`outcome` are **current observations**
— a filtered re-list is not a status-update feed. Re-list the window to discover transitions.

Response (`ReportPage`; illustrative, not live output):

```json
{
  "items": [{
    "run_id": "12345678-1234-4123-8123-123456789abc",
    "source_id": "9c8b7a6d-0000-4000-8000-000000000000", "revision_of": null,
    "kind": "daily",
    "requested_range": {"start_date": "2026-10-14", "end_date": "2026-10-14"},
    "window_start": 1791928800000, "window_end": 1792015200000,
    "timezone": "Europe/Paris", "local_date": "2026-10-14",
    "created_at": 1792015201000, "updated_at": 1792015205000,
    "phase": "finished", "outcome": "OK", "error_code": null
  }],
  "next_cursor": null
}
```

List rows **omit** `markdown` and the full evidence arrays; fetch the detail for those. Every
optional field is present as JSON `null`; `items` is always present (empty allowed); `next_cursor`
is an explicit `null` only at exhaustion.

Every summary row — and therefore every detail — carries the **required** `kind` field: the
persisted run-kind discriminator, one of `daily` | `weekly_synthesis` (`03 §8e`). This is the
durable report **lane**, not a format: `weekly_synthesis` is a scheduled weekly run over saved
dailies, and it is deliberately **distinct** from the on-demand report kind `weekly`
(`ReportKind`, `03 §8b`) — the on-demand token, label and filename stem never appear on this
surface, and no other value is emitted.

#### `GET /v1/reports/{run_id}` — detail

`run_id` is an **opaque UUID string** (never an integer), URL-encoded in the path. The detail is
**all** `ReportSummary` fields (including the required `kind`) **plus** `result` — the frozen
public saved-result shape, or `null`:

```
result: {                       // frozen public shape (SavedReportView, 03 §7f.6)
  markdown: String, model: String|null,
  frame_ids: [i64...], session_ids: [i64...], cited_frame_ids: [i64...],
  frames_sampled: u32, frames_summarized: u32,
  periods_total: u32, periods_covered: u32, passes: u32, truncated: bool
} | null
```

These eleven fields are the **complete** public saved-result shape (`03 §7f.6`); they are the only
keys `result` carries. The persisted `SavedReport` (`traits::SavedReport`) is an internal,
richer shape — it also stores the weekly-consumed `daily_run_ids` provenance (`03 §8e`) — and that
provenance is projected away, never hidden at the serializer. The report routes and MCP
`get_report` serve only the frozen public shape, so `daily_run_ids` never appears on this surface.

**Visibility and nullability (resolved).** `result` mirrors **actual persisted saved output**,
independent of `phase`: an immutable saved result is exposed **immediately whenever it exists**,
including while delivery is queued or retrying. A run with no saved result returns `result: null`
(any phase) — never generated replacement prose. A stored `NO_DATA` run keeps its factual saved
result (its markdown carries the no-data note). A malformed persisted saved payload is a read
error, never `null`. `frame_ids` is the full distinct summarized union (no 200-item cap);
`cited_frame_ids` may keep the display cap; `session_ids` is the generation-time ownership
snapshot, preserved even if the current frames/sessions are gone — existing frame/session reads
resolve references and may 404, and the report read never invents availability or re-segments.

**Privacy.** Neither projection exposes delivery internals, folder/recovery paths, configuration or
`config_snapshot`, Telegram chat/receipt, `schedule_id`, credentials or arbitrary stored error
messages. `error_code` is emitted **only** from the [§8c](../specs/03_MASTER_PRODUCTION_SPEC.md)
allowlist; any other code or `None` projects to `null`. Report reads serve stored error/provenance
only — they do **not** apply the in-app current-evidence overlay.

**Errors.** The same JSON envelope as the rest of the API, with fixed safe messages: malformed
query/cursor/enum/limit → **400**; missing or pruned run → **404** (never reconstructed from a
folder or regenerated); storage/decode failure → **500**; feed-readiness unavailable → **503**. A
report cursor carries no history identity, so an invalid report cursor is **400** — `409
history_changed` is frame-cursor only. Failures are explicit, never empty data or `NO_DATA`. The
saved uppercase `error_code` is distinct from HTTP errors.

**Read-only, model-free.** These reads work with schedules off and **no loaded answer model**. They
cannot claim, queue or retry a run, write an index, send a message or repair missing content.

### `POST /v1/ask`

Body: `{ "query": "...", "top_k"?: number, "thinking"?: bool, "max_tokens"?: number,
"session_id"?: number, "thread_id"?: number }`.
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

#### Ask v2 additions (0.4.5)

Four additive parts. Every one of them is optional for a client: a client that ignores the
new event and the new request field renders the answer exactly as it does today.

**`thread_id` — the conversation a question belongs to.** Omitted: the question starts a new
conversation and the turn is its first. Present: the turn is appended to that conversation at
the next ordinal, and a follow-up inherits the previous turn's resolved time window and
resolved entity ids unless the new question's own plan overrides them. An unknown
`thread_id` is a JSON **404** *before* the stream starts, like an unknown `session_id`.

**`citations` — one new terminal event**, emitted once, immediately *before* the terminal
`done` and after the last `citation`. It reports both kinds of evidence the answer was
grounded in — the sessions and the frames actually placed in the model's prompt — plus the
identifiers of the turn that produced them:

```json
{"type":"citations","sessions":[7,9],"frames":[7,41],"thread_id":3,"turn_id":2,"stage_counts":{"threads":2,"sessions":2,"frames":2},"tokens_assembled":812}
```

- `sessions` — distinct session ids carrying the cited frames, ascending. Every cited frame
  belongs to a cited session by construction.
- `frames` — the cited frame ids, in inclusion (citation) order.
- `thread_id` / `turn_id` — the persisted conversation and turn this answer belongs to.
- `stage_counts` / `tokens_assembled` — the per-turn diagnostics below.

The event is additive: `thinking`, `token`, `citation` and `done` keep their names, order and
meanings, so a client that does not know `citations` must ignore it and keep rendering the
per-`citation` frames it already received. A stream that emits no `citations` event (every
stream from a pre-0.4.5 build, and a frame-level answer with no conversation) is unchanged.

**`stage_counts` and `tokens_assembled` — per-turn diagnostics.** Both ride the `citations`
event *and* the terminal `done` event:

```json
{"type":"done","stage_counts":{"threads":2,"sessions":2,"frames":2},"tokens_assembled":812}
```

`stage_counts` gives the candidate count of each retrieval stage in order — `threads`, then
`sessions`, then `frames`, each stage only narrowing the previous one — and
`tokens_assembled` is the packed prompt cost in tokens. Both are **omitted when absent**, so
the terminal event of a stream that knows no diagnostics is still exactly
`{"type":"done"}`; a client must never require either field.

**The turn record.** Each turn is persisted before generation starts and answers are stored
when generation completes:

| Field | Meaning |
|---|---|
| question | The exact `query` text of the turn. |
| answer | The final answer text; **`null` while the turn is streaming and when it fails**. |
| window | The resolved half-open `[window_start, window_end)` in unix milliseconds, or `null` when the question carried no resolvable time intent (null means the whole history, never a fabricated bound). |
| resolved_entities | The entity ids the question resolved to, as a JSON array; empty when nothing resolved. |

Deleting a conversation removes its turns and touches no frame, session, graph row or saved
report. Reading conversations back over HTTP is a separate read surface scheduled for the
next phase; this section documents only what `POST /v1/ask` writes and emits.

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

### Threads, entities and insights (0.4.5 P8)

Four read routes over the 0.4.5 work graph and the insights table (`03 §7c` P8 block): `GET
/v1/threads`, `GET /v1/threads/{id}`, `GET /v1/entities` and `GET /v1/insights`. **Threads** are
the clusterer's persisted session clusters, **entities** the extracted repos, files, domains,
documents, references and apps, **insights** the generator's typed statements about a window. All
four are `history.read` reads — the legacy token and the read token are both allowed, and so is
the implicit `HEAD` variant, which runs the same authorized read before dropping the body.

None of them generates, writes or starts inference: each handler performs exactly one store read
and no other side effect. An empty array is **no stored row matching the query**, not "nothing
happened"; a storage failure is an explicit `500`, never an empty success. Insight generation runs
as a background job (idle, throttled, claimable only at throttle level 0) and is not reachable
over HTTP.

`insights` and `insight_evidence` are part of the v14 baseline, so these routes add no schema.
They do not change the `/v1/sessions` routes, and they do not change the in-app IPC insights
summary (`traits::InsightsSummary`), which is not served over HTTP.

#### Strict query parsing

The four routes share one strict parser. It is **deliberately stricter than `/v1/sessions`**:

| Input | Exact contract |
|---|---|
| Parameter set | Exactly the parameters the route documents. Any other name, the same name twice, or a pair without `=` → **400 `bad_request`**. |
| Encoding | Malformed percent escapes and invalid UTF-8 → **400**. Form-query `+` means space; URL-encode literal plus signs. |
| Numbers | ASCII decimal integers, optional leading `-` where the value is signed. No floats, decimal points, leading `+`, whitespace, exponents or `i64` overflow → **400**. |
| `from` / `to` | Unix epoch milliseconds, **each independently optional** (as on `/v1/sessions`), half-open `[from, to)`. Absent means unbounded on that side; both present with `from > to` → **400**. |
| `limit` | Default **1000**, clamped to **1–1000**. An out-of-range value clamps (the same semantics as `/v1/sessions`), it is not rejected — unlike `/v1/frames` and `/v1/reports`, which reject. |
| `kind` | Only the closed set the route documents. An unknown value is **400**, never ignored and never silently unfiltered. |

**`/v1/sessions` keeps its earlier laxer behaviour**: an unknown parameter there is ignored and a
duplicated one is not rejected, because that route deserializes into its parameter struct rather
than validating the parameter set. The difference is documented rather than implied — do not infer
the strict rules above for the sessions routes, and do not infer the sessions laxness for these
four.

### `GET /v1/threads`

Threads that **overlap** the requested window. Query params: `from`, `to` (unix ms, each
independently optional, half-open), `limit` (default 1000, clamped `1..=1000`). The overlap
predicate is `first_seen_at < to AND last_seen_at >= from`; with one bound the analogous half
applies (`last_seen_at >= from`, or `first_seen_at < to`). Note the asymmetry: `to` is exclusive
but `last_seen_at = from` is still an overlap, so a thread is listed from the moment of `from` up
to the instant before `to`.

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/threads?from=1720000000000&to=1720086400000&limit=50"
```

Returns a bare JSON array of thread summaries, ordered **`last_seen_at DESC, id DESC`**. Each row
carries the clusterer's own persisted figures; nothing is recomputed for the list:

```json
[
  { "id": 3, "title": "Refactor the store crate", "first_seen_at": 1719990000000,
    "last_seen_at": 1720003600000, "session_count": 2 }
]
```

`session_count` is the stored `threads.session_count`, not a recount of the detail's `sessions`.
The route has no continuation cursor: `limit` bounds the returned rows, so a window with more
threads than the limit yields the leading prefix of this order, not a page.

### `GET /v1/threads/{id}`

One thread with its member sessions and the entities those sessions touch. `id` is a decimal
integer path segment: a non-integer id is **400**, an unknown thread **404 `not_found`**. The route
takes no parameters.

```bash
curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:43210/v1/threads/3"
```

```json
{
  "thread": { "id": 3, "title": "Refactor the store crate", "first_seen_at": 1719990000000,
    "last_seen_at": 1720003600000, "session_count": 2 },
  "sessions": [
    { "session_id": 43, "started_at": 1719990000000, "ended_at": 1719993600000,
      "kind": "focus", "title": null, "confidence": 0.74 },
    { "session_id": 42, "started_at": 1720000000000, "ended_at": 1720003600000,
      "kind": "ai", "title": "Refactor the store crate", "confidence": 0.82 }
  ],
  "entities": [
    { "entity_id": 11, "kind": "repo", "canonical_name": "screensearch",
      "session_count": 2, "confidence": 0.91 }
  ]
}
```

`sessions` is ordered **`started_at ASC`**; `ended_at` is `null` for a session still open and
`title` may be `null`. `confidence` there is the **cluster edge weight** that placed the session
in the thread (`thread_sessions.confidence`), not the entity extraction's confidence. A session's
`kind` is the `sessions.kind` token (`focus` | `meeting` | `ai` | `other`).

`entities` is ordered **`confidence DESC, canonical_name`**. `session_count` counts how many of
**this thread's** member sessions touch the entity, while `confidence` is the maximum confidence
over those sessions — both are thread-scoped, so they are not comparable with the all-history
`session_count` of `GET /v1/entities`. `kind` on an entity is the `entities.kind` token.

### `GET /v1/entities`

Entities, optionally filtered. Query params: `kind`, `q`, `limit` (default 1000, clamped
`1..=1000`).

- `kind` — one of `repo` | `file` | `domain` | `document` | `ref` | `app`, the closed
  `entities.kind` set. Absent means every kind; any other value is **400**, never ignored.
- `q` — a **case-insensitive substring** of the entity's `canonical_name` **or any of its
  aliases**. Absent means no name filter. Matching is a literal substring, not a `LIKE`/glob
  pattern: `%`, `_` and `\` are ordinary characters. Folding is **ASCII-only** (the same SQLite
  `lower()` rule as the frame filters), so a name containing non-ASCII characters matches only
  when its case matches exactly — there is no Unicode casefolding and no locale collation.

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/entities?kind=repo&q=dark&limit=20"
```

Returns a bare JSON array ordered **`last_seen_at DESC, id DESC`**:

```json
[
  { "id": 11, "kind": "repo", "canonical_name": "screensearch",
    "first_seen_at": 1719000000000, "last_seen_at": 1720700000000,
    "session_count": 7, "aliases": ["screensearch-v2", "estrem/screensearch"] }
]
```

`session_count` here counts the sessions linked to the entity across the whole history
(`session_entities`), not the requested window — this route has no window. `aliases` lists the
`entity_aliases` names in insertion order and may be empty (`[]`), never `null`. Like threads,
this route has no cursor: `limit` bounds the rows returned.

### Insights

`GET /v1/insights` serves the persisted insights of the 0.4.5 insights engine (`03 §7j`). An
insight is a **typed, evidenced statement about a half-open window**: the payload is the truth and
is renderable without a model.

#### The five closed kinds and their payloads

`insight.kind` is a closed set. A sixth kind would be a **schema change** (baseline DDL edit plus a
`schema_version` bump), not an additive route change. The payload is a tagged object that repeats
the row's `kind`, so a row can never be read as a different kind; the variant and the column are
written together and always agree.

| `kind` | Payload fields (all present; nullable ones may be `null`) |
|---|---|
| `thread_activity` | `thread_id`, `title`, `session_count`, `active_ms`, `first_touched_at`, `last_touched_at` |
| `open_loop` | `source` (`unresolved_mark` \| `abandoned_thread`), `mark_id`, `frame_id`, `note`, `thread_id`, `title`, `opened_at`, `idle_ms` |
| `recurrence` | `entity_id`, `entity_kind`, `entity_name`, `first_session_id`, `first_seen_at`, `second_session_id`, `second_seen_at`, `gap_ms`, `occurrence_count` |
| `semantic_neighbour` | `session_id`, `session_title`, `session_started_at`, `neighbours[]` — each `{session_id, title, started_at, similarity}` |
| `resumption` | `thread_id`, `title`, `previous_session_id`, `previous_ended_at`, `resumed_session_id`, `resumed_at`, `gap_ms`, `last_doing` |

Per kind:

- **`thread_activity`** — time spent and sessions touched on one thread inside the window. Every
  figure is a real overlap computation over the member sessions, nothing estimated:
  `session_count` counts the member sessions overlapping the window; `active_ms` sums their
  overlap with the window (an open session contributes up to `window_end`); `first_touched_at` is
  the earliest member-session `started_at` inside the window and `last_touched_at` the latest
  `COALESCE(ended_at, window_end)` inside it.
- **`open_loop`** — something left unresolved and how long it has been open. `source` says which
  half is populated: `unresolved_mark` fills `mark_id` (and the `frame_id` it flags, plus the
  mark's optional `note`), `abandoned_thread` fills `thread_id` and `title` — a thread whose last
  activity is at least that 7-day horizon before `window_end` and whose last member session has
  actually ended. The other half's
  fields are `null`, never a fabricated value. `opened_at` is the mark's `created_at` or the
  thread's `last_seen_at`; `idle_ms` is `window_end - opened_at`. There is no third source in
  0.4.5 — the spec's "open question from a synthesis" source is deferred.
- **`recurrence`** — the same entity seen again after at least `RECURRENCE_GAP_MS` (7 days).
  `first_*`/`second_*` name the two sessions forming the recurrence — the latest qualifying pair
  inside the window — (evidence order: entity, first session, second session); `gap_ms` is
  `second_seen_at - first_seen_at` and is always at least the 7-day horizon; `occurrence_count`
  counts every session touching the entity inside the window. `entity_kind` is the `entities.kind`
  token rendered as a string.
- **`semantic_neighbour`** — sessions similar to a given session on **other calendar days** (day
  compared in UTC; localisation may come later). A session in the window without a session vector
  yields no such insight. `neighbours` is never empty, is ordered by descending `similarity`,
  holds at most the top 5, and every entry is at least 0.80 cosine similarity
  (`1 - vec0 distance`). `session_title` and a neighbour's `title` may be `null`.
- **`resumption`** — a thread picked up after at least `RESUMPTION_GAP_MS` (7 days):
  `gap_ms = resumed_at - previous_ended_at` is always at least that horizon, and `last_doing`
  is the previous session's summary, else its title, else `null`.

The thresholds named above (7-day horizons, 0.80 similarity, top 5) are **compile-time kernel
constants, not settings** — they are fixed for 0.4.5 and cannot be configured through the API,
Settings or a query parameter.

#### Evidence

```json
{ "ordinal": 0, "kind": "thread", "ref_id": 3 }
```

`ordinal` is **0-based and dense** within its insight (`0, 1, 2 …`, no gaps) and orders the
evidence for display. `kind` is the closed set `frame` | `session` | `thread` | `entity` | `mark` |
`report` and says which table `ref_id` names. `evidence` is **never empty** — an insight without
evidence is a defect (the kernel dead-letters the pass), never an empty-but-successful row.
`report` is in the closed set and in the DDL check, but **no generator emits it** in 0.4.5:
`open_loop` comes from unresolved marks and abandoned threads only.

#### Generator, staleness and replacement

`generator` is `heuristic` | `llm_phrased`. `heuristic` means the payload was produced
deterministically from stored facts; `llm_phrased` means a model only **restated** a payload that
already existed. **Phrasing is additive**: the typed payload is the truth, phrasings add no claim,
and 0.4.5 emits `heuristic` only.

`window_start`/`window_end` are unix milliseconds and half-open `[window_start, window_end)`.

`stale` (`true`/`false`) means at least one referenced row — frame, session, thread, entity or
mark — no longer exists since the insight was generated: the window, the entities or the evidence
changed. The flag is computed **when you read**, from the evidence that survives, so it turns true
as soon as the cited row goes — not at some later generation pass (a stale row is also still listed,
with the flag set, and is **replaced** by the next generation pass rather than repaired or hidden;
never treat a stale row as current truth). Rows are
not stable across passes: generation is **replace-per-window** — a pass covers one window, deletes
the rows of each kind whose window overlaps it and inserts the fresh set in one transaction — so an
`id` may name a different statement after a pass.

#### `GET /v1/insights`

Query params: `from`, `to` (unix ms, each independently optional, half-open), `kind` (one of the
five tokens above; unknown → **400**), `limit` (default **1000**, clamped **1..=1000**).

The overlap predicate is `window_start < to AND window_end > from`, with the analogous single-bound
forms. Order is **`window_start DESC, kind ASC, id DESC`**, and each row's `evidence` is ordered by
`ordinal`. Because one generation pass covers one window, rows from the same pass share
`window_start`/`window_end` and are therefore grouped by `kind` in that order.

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:43210/v1/insights?from=1791928800000&to=1794520800000&kind=recurrence&limit=50"
```

```json
[
  {
    "id": 18, "kind": "recurrence",
    "window_start": 1791928800000, "window_end": 1794520800000,
    "payload": {
      "kind": "recurrence",
      "entity_id": 11, "entity_kind": "repo", "entity_name": "screensearch",
      "first_session_id": 42, "first_seen_at": 1720000000000,
      "second_session_id": 57, "second_seen_at": 1720700000000,
      "gap_ms": 700000000, "occurrence_count": 6
    },
    "generator": "heuristic", "generated_at": 1794520805000, "stale": false,
    "evidence": [
      { "ordinal": 0, "kind": "entity", "ref_id": 11 },
      { "ordinal": 1, "kind": "session", "ref_id": 42 },
      { "ordinal": 2, "kind": "session", "ref_id": 57 }
    ]
  }
]
```

The response is a bare array; `payload` is always present and always carries its own `kind` tag;
`stale` is a JSON boolean, never `0`/`1`. These are **reads**: a `GET` never generates an insight,
never writes or repairs a row, never starts inference, and never persists a `stale` change — the
flag it reports is derived from the references that exist at that moment, so it can differ between
two reads while nothing was written. The pass window is
`[now - 30 days, now)` truncated to whole minutes, so an insight describes the generator's window,
not the window you asked for — `from`/`to` only select which stored rows you see.

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
