# ScreenSearch HTTP API (v1)

The **HTTP API** exposes ScreenSearch's search, ask, frames, sessions, where-was-i, and
marks to scripts and agents. It is **opt-in and off by default** (0.3.0
PR7; the sessions surface added in 0.4.0 PR6, specs `03 §7c` / `§7e`).

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

Inference and the MCP client's destination remain loopback-only. A bind that excludes loopback
may make local MCP unavailable; configure an appropriate loopback or wildcard listener to use MCP.

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

A missing or wrong token returns **401**. The token is compared in constant time.

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:43210/v1/health
```

## Errors

Every non-2xx response is a JSON body — including malformed query strings, bodies, and path
segments, which are mapped to `400 bad_request` rather than a framework plaintext rejection, so
clients can parse errors uniformly:

```json
{ "error": "not_found", "message": "frame 42 not found" }
```

| Status | `error`         | When |
|--------|-----------------|------|
| 400    | `bad_request`   | Malformed params/body (e.g. missing `q`, `from`/`to` not paired, `from` > `to`, invalid session `kind`, non-integer session id, `format` ≠ `json`, both `frame_id` and `now`). |
| 401    | `unauthorized`  | Missing or wrong bearer token. |
| 404    | `not_found`     | Unknown frame, mark, or session (or an unknown endpoint path). |
| 404    | `image_purged`  | The frame exists but its screenshot was retention-purged (text is preserved). |
| 503    | `unavailable`   | A dependency is unavailable (answer model not loaded; capture off for `POST /v1/marks {"now":true}`). |
| 500    | `internal`      | Unexpected store failure. |

## Endpoints

All paths are under `http://<computer-address>:<port>`; use `127.0.0.1` locally.

### `GET /v1/health`

```json
{ "version": "0.4.0", "uptime_secs": 128, "capturing": true }
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
