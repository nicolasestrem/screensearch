# ScreenSearch MCP server (`screensearch-mcp`)

`screensearch-mcp.exe` is a small [Model Context Protocol](https://modelcontextprotocol.io)
server that lets an MCP client — **Claude Desktop**, **Claude Code**, or any other — search,
ask about, and mark your ScreenSearch history. It is a thin **stdio** wrapper over the
[HTTP API](API.md): it holds no data of its own and talks only to
`127.0.0.1:<port>` with your bearer token (0.3.0 PR8; spec `03 §7c`).

> **Threat model — read this before enabling.** The application API defaults to loopback and accepts a configured IPv4/IPv6 bind address
> using plain HTTP: any reachable token holder can read your history, and network traffic is
> not encrypted. The MCP wrapper still validates loopback-only destinations and uses stdio;
> changing the application listener does not enable remote MCP. Protect MCP client configuration
> files containing the token and review the [API network boundary](API.md).

## Prerequisites

1. **Enable the HTTP API.** In ScreenSearch: **Settings → HTTP API → toggle on.** It is
   **off by default**. Enabling it binds `127.0.0.1:43210` by default (address and port configurable) and
   mints a bearer token. Keep a listener reachable on loopback to use this MCP wrapper.
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

Configuration is two values, resolved **flag > environment variable > default**:

| What | Flag | Environment variable | Default |
|---|---|---|---|
| API base URL | `--url <URL>` | `SCREENSEARCH_API_URL` | `http://127.0.0.1:43210` |
| Bearer token | `--token <TOKEN>` | `SCREENSEARCH_API_TOKEN` | *(none — required for tool calls)* |

Set `SCREENSEARCH_API_URL` only if you changed the API port in Settings.

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

The API's `health`, `export`, and mark-`resolve` surfaces are intentionally **not** exposed as
tools; the tool set is read + add-a-mark only. The three 0.4.0 session tools (`list_sessions`,
`get_session`, `ask_session`) keep that boundary: they are **read-only** over the same HTTP
surface, add no new write scope, and remain a stdio wrapper (no store access).

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
