# ScreenSearch MCP server (`screensearch-mcp`)

`screensearch-mcp.exe` is a small [Model Context Protocol](https://modelcontextprotocol.io)
server that lets an MCP client — **Claude Desktop**, **Claude Code**, or any other — search,
ask about, and mark your ScreenSearch history. It is a thin **stdio** wrapper over the
[HTTP API](API.md): it holds no data of its own and sends your bearer token to one
configured API root (loopback by default).

**P2 transport is implemented and committed, but not released or integrated-app accepted:**
the configuration below was first inspected at `7831e841411471700b83e1d196628ad9f0297387` in the
isolated transport worktree; that worktree's `crates/mcp/` change set is now committed at
`409d380262aae4dac2e63b85e9c1a6e474d940fe` (draft PR
[#127](https://github.com/nicolasestrem/screensearch/pull/127)). The shipped v0.4.1 binary
**and** the P1 bundled wrapper still carry the earlier baseline wrapper — nine tools, `--url`
only, loopback-only validation, no read-only mode. Do not assume installing v0.4.1 supplies these new flags, tools or remote
behavior. See [verification status](TESTING.md#external-agent-memory-api-p1p2--verification-status).
The active source repository is [nicolasestrem/screensearch](https://github.com/nicolasestrem/screensearch).

> **Threat model — read this before enabling.** The application API defaults to loopback and accepts a configured IPv4/IPv6 bind address
> using plain HTTP: any reachable token holder can read your history, and network traffic is
> not encrypted. P2 permits deliberately configured remote HTTP/HTTPS destinations, still using
> stdio to the MCP host; it does not expose a network MCP server. Protect client configuration
> containing the token and review the [API network boundary](API.md). Read-only mode is not
> authorization; use a separately created API read token for server-enforced permissions.

## Prerequisites

1. **Enable the HTTP API.** In ScreenSearch: **Settings → HTTP API → toggle on.** It is
   **off by default**. Enabling it binds `127.0.0.1:43210` by default (address and port configurable) and
   mints a legacy bearer token. The configured API root must be reachable from the wrapper host.
2. **Copy the token** from that same Settings panel (reveal / copy).

The MCP server does nothing until the API is on: with the API off, every tool call returns a
clear *"enable the API in ScreenSearch Settings"* message rather than failing silently.

## The binary

| Install kind | Path |
|---|---|
| Installed (per-user, the default) | `%LOCALAPPDATA%/ScreenSearch/screensearch-mcp.exe` |
| Installed (per-machine) | `C:/Program Files/ScreenSearch/screensearch-mcp.exe` |
| From a source build | `target/release/screensearch-mcp.exe` |

It ships **inside the ScreenSearch installer**, next to `ScreenSearch.exe` — no separate
download.

## Configuration

P2 resolves **CLI flag > environment variable > default**:

| What | Flag | Environment variable | Default |
|---|---|---|---|
| API root | `--api-url <URL>`; retained `--url <URL>` alias | `SCREENSEARCH_API_URL` | `http://127.0.0.1:43210` |
| Bearer token | `--token <TOKEN>` | `SCREENSEARCH_API_TOKEN` | None; needed for tool calls |
| Read-tool filter | `--read-only` | None | Off |

Both value flags accept `--flag value` and `--flag=value`. Both URL spellings have equal
precedence; the **last URL flag wins** if repeated/mixed. An explicitly invalid URL fails
startup rather than falling back to environment/default. There is no extra remote-enable
flag. Prefer the token environment variable over a command-line secret; protect it and do
not commit client config. For memory agents, configure the independent read credential and
`args: ["--read-only"]` in the client definition. Creating that credential does not enable API.

### URL validation and transport boundary

An explicit URL or environment value selects the destination. P2 accepts concrete HTTP/HTTPS
roots with hostname, IPv4 or bracketed IPv6 and optional port, e.g. `https://memory.example`
or `http://[::1]:43210`. Only a root trailing slash is removed; URL-parser canonicalization
also applies. Wildcards (`0.0.0.0`, `[::]`, `*`) are listeners, not destinations.

**Compatibility tightening:** formerly accepted URL shapes with userinfo/credentials
(including empty userinfo), query/fragment (including empty ones), non-root paths, dot
segments, encoded path tricks, extra slashes, whitespace/control characters or backslashes
are rejected. Missing host and non-HTTP(S) schemes are rejected. Validation runs in the shared
fallible client constructor as well as CLI/environment parsing; errors do not echo the URL.

Every HTTP path (JSON, images, POST and Ask SSE) uses `.no_proxy()` and redirect policy
**none**. HTTP(S)/ALL_PROXY environment settings are not used, even for remote destinations.
Same-origin and cross-origin redirects are refused without visiting `Location` or forwarding
the bearer. Configure the final root rather than relying on redirects. HTTPS retains normal
certificate verification; no insecure bypass is provided. The app still serves plain HTTP:
this client option does not add app TLS, change listener/CORS, or broaden inference access.

### Read-only mode

`--read-only` both filters `tools/list` and refuses non-allowlisted dispatch **before HTTP**.
The current committed transport registry (`409d380`, draft PR
[#127](https://github.com/nicolasestrem/screensearch/pull/127)) allows eight read tools:
`search_screen_history`, `get_moment`, `where_was_i`, `list_marks`, `list_sessions`,
`get_session`, `list_frames`, `get_capture_coverage`. The earlier committed HEAD (`7831e84`)
had only the first six. The stacked P3 report-tool branch
(`feat/agent-memory-saved-report-mcp`, a child of this P2 branch) adds `list_reports` and
`get_report` for **ten** read tools / **thirteen** normal once integrated; those two are **not**
in the committed transport registry, any released binary or the P1 bundled wrapper. None of the P2 additions are in the P1 bundled wrapper or any released
binary (see [Tools](#tools)). Ask tools and `add_mark` are excluded; unknown/future names
default-deny. This is tool-selection ergonomics,
**not authorization**: omitting the flag does not elevate a read token, and enabling it does
not reduce the permissions of a leaked legacy token. API 403 means insufficient permission,
not API-off; 401 means missing/invalid credential. Rotation/revocation affects subsequent
requests, not guaranteed cancellation of authorized streams.

The standalone wrapper can be built package-scoped with Rust 1.95.0 (`cargo build --locked
-p mcp --release`); it does not need Windows app crates on Linux. Intended first archive
targets are the three native wrappers — Windows x86_64, Linux x86_64 and macOS arm64 — but
archive availability and native execution are **not claimed here**: each of the three targets
needs its own packaged-archive build-and-run receipt, plus a separate release authorization,
before any portable-release or supported-platform claim. Packaging is staged, not released.
The Windows NSIS staging contract remains unchanged.

## Claude Desktop

Edit `claude_desktop_config.json` (Claude Desktop → **Settings → Developer → Edit Config**):

```json
{
  "mcpServers": {
    "screensearch": {
      "command": "C:/Users/<you>/AppData/Local/ScreenSearch/screensearch-mcp.exe",
      "env": {
        "SCREENSEARCH_API_TOKEN": "<token from Settings → HTTP API>"
      }
    }
  }
}
```

Restart Claude Desktop. If you changed the port, add
`"SCREENSEARCH_API_URL": "http://127.0.0.1:<port>"` alongside the token.

## Claude Code

```sh
claude mcp add screensearch \
  --env SCREENSEARCH_API_TOKEN=<token from Settings → HTTP API> \
  -- "C:/Users/<you>/AppData/Local/ScreenSearch/screensearch-mcp.exe"
```

Then, in a session: *"search my screen history for the invoice I saw"*, *"what was I doing
before this?"*, *"mark this moment."*

## Tools

Observed **committed** transport registry (`409d380`, draft PR
[#127](https://github.com/nicolasestrem/screensearch/pull/127)): **eleven tools in normal
mode, eight in read-only mode**. The earlier committed HEAD (`7831e84`) had nine normal / six
read-only. The two additions (`list_frames`, `get_capture_coverage`) are **not** in the P1
bundled wrapper or any released binary, and the base API on that branch does not serve
`/v1/frames` or `/v1/coverage` — the P1 routes live on `feat/agent-memory-http` — so they are
exercised only by protocol-fixture tests, not a real end-to-end read. Until they are released
and integrated, use the [HTTP frame/coverage reads](API.md#chronological-frames) and the
[reference consumer](MEMORY_CONSUMER.md) directly. Do not infer their presence from the
specification or from this page.

**P3 report tools (`list_reports`, `get_report`) are resolved** (Nico GO, stored-presence
visibility) and are being added on the **stacked report-tool branch**
`feat/agent-memory-saved-report-mcp` — a separate child of this P2 transport branch. They are
HTTP-only `Value` passthrough tools (no producer-crate import); the producer's `/v1/reports`
routes live on `feat/agent-memory-saved-reports`, so on this branch they are exercised by
protocol fixtures only, never a real end-to-end read. They are **not** in the committed transport
registry, any released binary or the P1 bundled wrapper. Remote report generation, retry,
scheduling and delivery remain deferred; the report tools add no phase/`result` logic of their own
— the producer's projection is passed through verbatim.

| Tool | Wraps | Arguments | Returns |
|---|---|---|---|
| `search_screen_history` | `GET /v1/search` | `query` (required); `from`, `to` (unix ms, both or neither); `limit` (1–100); `include_chrome` | Matching frames (id, snippet, score, time, app). |
| `ask_screen_history` | `POST /v1/ask` | `query` (required); `top_k`; `thinking`; `max_tokens` | A grounded answer, plus the frame ids it cited. |
| `get_moment` | `GET /v1/frames/{id}` | `frame_id` (required); `include_image` (default false) | Frame detail + text; with `include_image`, the screenshot too. |
| `where_was_i` | `GET /v1/context/where-was-i` | *(none)* | The last sustained context before your current activity, or a "nothing to resume" note. |
| `list_marks` | `GET /v1/marks` | *(none)* | Your marks, unresolved first then newest-first. |
| `add_mark` | `POST /v1/marks` | `frame_id` (omit to capture **now**); `note` | Creates a mark. Omitting `frame_id` captures the current screen past the change gate, then marks it. |
| `list_sessions` | `GET /v1/sessions` | `kind`, `tool`, `from`, `to`, `limit` (all optional) | The sessions overlapping the window (id, kind, tool, host, title, span, open flag); "No sessions." when none match. |
| `get_session` | `GET /v1/sessions/{id}` | `session_id` (required); `include_summary` (default false) | One session plus its exchange artifacts; `include_summary` returns the cached summary if one exists and never generates one. |
| `ask_session` | `POST /v1/ask` (`session_id`) | `session_id` (required); `query` (required); `top_k`; `thinking`; `max_tokens` | A grounded answer that cites **only** frames owned by that session. |
| `list_frames` *(committed P2 `409d380`; draft PR #127)* | `GET /v1/frames` | `from`+`to` (first page) **or** `cursor` (continuation); optional `app`, `window_title`, `limit` (1–1000) | One chronological page over the half-open UTC window, with full `raw_text`/`content_text`/`vision_description` and the exact `next_cursor`. |
| `get_capture_coverage` *(committed P2 `409d380`; draft PR #127)* | `GET /v1/coverage` | `from`, `to` (both required, unix ms) | Bounded coverage counts plus allocator/retention facts and capture observations with explicit unknowns. |
| `list_reports` *(P3, stacked report-tool branch `feat/agent-memory-saved-report-mcp`)* | `GET /v1/reports` | `from`+`to` (first page) **or** `cursor` (continuation); optional `phase`, `outcome`, `limit` (1–1000) | One page of saved-report summaries over the half-open UTC window, newest first, with the exact `next_cursor`; rows omit `markdown`/full evidence. |
| `get_report` *(P3, stacked report-tool branch `feat/agent-memory-saved-report-mcp`)* | `GET /v1/reports/{run_id}` | `run_id` (required, opaque UUID string) | One saved report's public detail — summary fields plus the saved `result` (markdown/model/coverage/provenance) or explicit `null`. |

The API's `health`, `export`, and mark-`resolve` surfaces are intentionally **not** exposed as
tools. Normal mode includes reads, Ask generation and add-a-mark. `list_sessions` and
`get_session` read stored data; **`ask_session` generates an answer** and needs
`answer.generate`, so it is excluded from read-only mode. MCP has no store access.

### Frame and coverage reads (P2, draft PR #127)

When `list_frames` / `get_capture_coverage` are released and integrated against the P1 routes,
three behaviours are the whole point — and they are the reason not to build a caller that
assumes otherwise:

- **Full text, never truncated.** Each `list_frames` item carries the complete `raw_text`,
  `content_text` and stored `vision_description`. The MCP does not snippet, elide or summarise
  them; the page `limit` bounds rows, not response bytes, and a failure is an explicit error
  rather than silent text loss (see the [frame contract](API.md#chronological-frames)).
- **Client-side page size, no auto-drain.** One call returns exactly one page and its
  `next_cursor` unchanged — including the terminal `null`. The MCP never prefetches or drains
  later pages, so **the caller sets page size and decides how many pages to read**; a large page
  is returned whole rather than shortened to fit a context budget. Continuations pass the cursor
  with `limit` only — the window and filters are frozen inside the cursor.
- **Coverage unknowns are preserved.** `get_capture_coverage` returns the response verbatim, so
  `truncated`, `unavailable_before`, open (`end: null`) intervals and any fields added later
  survive unchanged. Counts describe **stored records**, not proof of capture health or of what
  the user did; an empty window is not `NO_DATA` (see the
  [coverage contract](API.md#coverage) and the [reference consumer](MEMORY_CONSUMER.md)).

### Example: asking about a past session

*"What did I do in my last Claude Code session?"* is a two-step flow: find the session, then
ask within it.

1. `list_sessions` with `kind: "ai"`, `tool: "claude-code"`. The newest matching session is
   first (results are ordered most-recent first). Take its `id`.
2. `ask_session` with that `session_id` and `query: "what did I do?"`. The answer is grounded
   in that session's frames only, and every cited frame belongs to it.

`get_session` fills in the detail between the two: pass `include_summary: true` to read a
cached recap (it returns one only if the app already generated it, and never triggers
generation itself).

## Protocol notes

- Transport: newline-delimited JSON-RPC 2.0 over stdio (UTF-8, one message per line).
- Protocol revisions accepted: `2025-06-18`, `2025-03-26`, `2024-11-05`; a request for any
  other revision is answered with `2025-06-18` (the tool surface is identical across them).
- Capabilities: `tools` only. No resources, prompts, or server-initiated messages.
- Logs go to **stderr** (Claude Desktop collects them in its MCP logs); stdout is protocol
  traffic only.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Every tool says *"…is not responding… enable the API in ScreenSearch Settings"* | The API is off (or on a different port). Enable it in Settings → HTTP API; set `SCREENSEARCH_API_URL` if you changed the port. |
| Every tool says *"No API token is configured…"* | `SCREENSEARCH_API_TOKEN` is unset. Copy the token from Settings and set it in the client config. |
| Tools say *"The API token was rejected (401)…"* | The token was regenerated. Copy the current one from Settings and update the config. |
| `ask_screen_history` returns *"answer model not loaded"* | No answer model is loaded in ScreenSearch — open the app and let it load, or pick a model in Settings. |
| Server shows as connected but tools are missing | Confirm the `command` path points at `screensearch-mcp.exe` and restart the client. |

For the underlying HTTP contract (status codes, JSON shapes, the SSE format), see
[docs/API.md](API.md).
