# 02 — Strategic Plan

> **Question this file answers:** *"What should change, and why?"* — the strategy and the phased
> path. **Not** every table/endpoint/env var (that's `03_MASTER_PRODUCTION_SPEC.md`). Builds on
> `01_PROJECT_CONTEXT.md`.

---

## 1. Goal & user value

**Goal:** ship a local-first Windows app that turns your screen history into an instantly
searchable, question-answerable memory — **without** the resource drain and fragility that sank
the prior attempt.

**User value:**
- *Recall:* find any moment by **text or meaning** (hybrid search).
- *Answers:* ask questions about what you saw and get **grounded, reasoned** answers.
- *Control:* heavy AI work runs **when you choose** (on-demand / timed / idle), not constantly.
- *Trust:* storage and inference stay **local**, with no telemetry. Content leaves the device
  only through explicitly enabled network API access (`§5e`, `03 §7c`) or optional saved-report
  Telegram digests to a selected destination (`§5g`, `03 §8d`).

## 2. The core strategic change (the "why")

V1's engines were fine; its **shape** was the problem — a tight *real-time* streaming pipeline
(capture→OCR→embed→vision→index, all live) made it resource-hungry and brittle (one crash took
everything down). **V2c changes the shape, not the engines:**

1. **Capture-cheap, enrich-deferred.** Capture + OCR are the only always-on work and are cheap.
   Everything expensive — embeddings, vision tagging, answers — becomes **durable jobs in a
   SQLite-backed queue**, executed by workers **on-demand / on-timer / when-idle**. This is the
   single most important change: it delivers user resource-control *and* fault isolation *and*
   modularity in one move.
2. **Fault isolation by construction.** The only crash-prone, out-of-process component (the
   llama.cpp inference sidecar) is **bound to the app via a Windows Job Object** so it can never
   orphan; a failed enrichment **job retries** instead of crashing capture.
3. **Modular kernel over ad-hoc channels.** Trait-bounded modules communicate over a typed event
   bus — any module is swappable and testable in isolation.
4. **Tiered models, user-selectable.** Vision and answer each offer **Default / Quality**,
   so users trade footprint vs. quality explicitly (see `00 §E`). *(0.3.0 retired the Beta tier —
   `§5c`; the answer lane's non-Apache Nemotron went with it.)*

## 3. What stays vs. changes (vs. the V1 reference baseline)

| Area | V1 (reference) | V2c |
|---|---|---|
| Engines | SQLite+FTS5+sqlite-vec, WinRT OCR, fastembed, llama.cpp | **Keep** (proven) |
| Processing | real-time streaming pipeline | **Change → enrich-deferred job queue** |
| Vision | per-frame, real-time | **Change → on-demand / timed** |
| Shell/IPC | Axum localhost HTTP + rust-embed | **Change → Tauri 2 typed IPC** |
| Structure | monolith, ad-hoc channels | **Change → modular kernel + event bus** |
| ML runtime | flirted with Python ML sidecar (failed) | **Rust-only runtime** (no Python *ML sidecar*; Python OK for tooling) |
| Models | single defaults | **Change → 2-tier per lane** (Default / Quality; 0.3.0 retired Beta) |
| Automation | present | **Drop for v1.0** (later) |

## 4. Future-state architecture (high level)

```
Tauri 2 app  ──typed IPC──  Rust kernel (event bus + trait modules)
   │                              │
   │   always-on (cheap):  WGC capture → WinRT OCR → Store
   │   deferred (controlled):  SQLite JobQueue → workers:
   │        • fastembed (text vectors, in-process)
   │        • llama.cpp sidecar (vision tag / RAG answer, Job-Object-bound)
   │   query:  FTS5 + vec KNN → RRF → (sidecar answer, thinking) → stream to UI
```

Detailed schema, traits, command/event contracts, and sidecar protocol live in `03`.

## 5. Delivery phases (correctness-first; no deadline)

- **P0 — Scaffold:** Tauri 2 + Cargo workspace (`kernel`, `traits`, module crates), `ts-rs`
  binding gen, CI skeleton, WebView2/Vulkan/llama smoke-check.
- **P1 — Data spine:** `Store` + `JobQueue` on SQLite + sqlite-vec + FTS5; schema + migrations;
  RRF retrieval. *Everything writes here.*
- **P2 — Capture happy path:** WGC capture + WinRT OCR (STA) + event bus → frames+text stored;
  live timeline in the UI. *Proves the kernel.*
- **P3 — Deferred enrichment:** embedding worker (fastembed) + job scheduling
  (on-demand/timer/idle); hybrid search end-to-end.
- **P4 — Inference sidecar:** Job-Object lifecycle (spawn/reap/heartbeat/evict), model-agnostic
  tiered loader; on-demand/timed **vision tagging** + **RAG answers** (thinking).
- **P5 — Product:** Command-Deck UI polish; settings (model tiers, schedules, retention);
  packaging (**NSIS** installer; signing pending); first release.
- **Later (nice-to-haves):** multi-model routing UI, timeline analytics, export, sharing,
  auto-update (promoted into the 0.3.2 arc — `§8`, `docs/0.3.2.md`), OS automation.

## 5b. Post-1.0 arc — 0.2.x (attention-first text signal + recall workflows)

P0–P5 (v1.0) are complete and merged; the 0.2.x line is a **separate arc** layered on the shipped
app, **not** a retrofit of the v1.0 phases above. It is tracked in detail in `docs/0.2.0.md`
(roadmap) and `03` (contract); this section states only the strategic *what/why*.

- **The problem.** Capture indexes **raw full-screen OCR with no filtering**, so search, Ask, and
  embeddings get dominated by static chrome — taskbars, desktop icons, browser toolbars, even the
  app's own sidebar labels. Searching "Firefox" / "Steam" / "Deck" surfaces frames purely because
  those labels were on screen, not because they were the user's actual work.
- **P6 — Attention-first text signal + recall workflows.** Preserve raw text, but derive a default
  **content-text** layer (filtered OCR/UIA text — *not* vision descriptions) and make search, Ask,
  embeddings, and reports use it by default. Raw / app-chrome text stays searchable **opt-in**
  (`include_chrome`); default search stays **hybrid (FTS + vector) over content text** and the FTS
  fallback is never removed. Adds Recall **reports** (daily/weekly/custom) and premade Ask cards on
  top of the cleaned signal.
- **Ships in 0.2.0** (clean DB, no backfill): PR1 specs → PR2 data model + OCR spans → PR3
  attention-first filtering → PR6 reports → PR7 audit.
- **Deferred to 0.2.1** (highest-risk, most-invasive, not needed for the retrieval fix):
  event-driven capture, UIA text, and a smart enrichment throttle — each its own gated PR, recorded
  in `07`. **0.2.0 keeps timer/idle capture; no raw keystrokes or clipboard text are ever stored.**
- **Realized on 0.2.1.** The **smart enrichment throttle** (the roadmap's former PR5, `07` gap #49)
  now ships on the 0.2.1 line: opt-in, default-OFF, CPU/GPU-pressure-aware backpressure that pauses
  heavy enrichment (`vision_tag` — and `embed_image` until 0.3.0 PR4 removed that lane — and at the
  deepest level floors `embed_text` concurrency) under *sustained* load, while **capture / OCR /
  storage never pause** (they sit outside the worker pool). Contract in `03 §5/§7/§8`.

## 5c. Post-1.0 arc — 0.3.0 (subtraction + flow recall + open surface)

The 0.2.x line is shipped and tagged; **0.3.0 (P7)** is the next **separate arc** layered on the
shipped app, **not** a retrofit of the phases above. It is tracked in detail in `docs/0.3.0.md`
(roadmap) and `03` (contract); this section states only the strategic *what/why*.

- **The problem.** Two forces, pulling the same way. (1) **Too much surface for a solo project to
  carry** — six event-capture triggers (two riding a global `WH_MOUSE_LL` mouse hook the 0.2.0
  roadmap deliberately avoided), three model tiers per lane (one, Nemotron, the only non-Apache
  license and the only unproven hybrid arch), and a dark, flag-off image-embedding lane
  (`enrich.image_embeddings`) nobody turns on. Each is user config surface, maintainer decision
  surface, and audit surface for a privacy-first product. (2) **Recall is still friction-gated** —
  recalling means switching *to* ScreenSearch, a context switch to recover from a context switch;
  and the app is a silo with no API for the open-source audience to build on.
- **P7 — Surface reduction + flow recall + local API.** **Subtract:** cut the six triggers to
  **foreground + idle** (deleting the mouse hook, the clipboard listener, and typing-pause), retire
  the **Beta** model tier (Default/Quality only — a uniformly Apache story), and remove the
  image-embedding lane (text embeddings + vision tags already cover semantic reach). **Add** — each
  reusing infrastructure that already exists (the store, hybrid search, the sidecar, the capture
  pipeline; no new subsystem is invented): a **global-hotkey Flow overlay** (instant
  search-as-you-type / ask over whatever you're doing), a **where-was-i + mark-this-moment**
  workflow (the ADHD core — pull-based, never nagging), and an **opt-in localhost HTTP API +
  export** with a thin **stdio MCP wrapper**. Every removal deletes a decision; every addition
  reuses infrastructure.
- **Ships in 0.3.0** (build order): PR1 specs → PR2 trigger trim → PR3 Beta-tier removal → PR4
  image-lane removal → PR5 Flow overlay → PR6 where-was-i + marks → PR7 local API + export → PR8
  MCP server → PR9 audit + release. The three subtractions (PR2–PR4) are independent of each other
  and of PR5–PR8; PR5 precedes the overlay half of PR6; PR7 precedes PR8. Existing DBs are
  supported — migrations are forward-only and destroy **only** derived, re-derivable data
  (image-embedding vectors and dead jobs).
- **Deferred (recorded in `07` during PR1, not built this arc):** audio capture / transcription
  (the most-requested capability in the category — a full arc of its own; revisit 0.4.x), a
  custom-GGUF "bring-your-own-model" path (replaces the Beta tier for tinkerers), **proactive
  nudges** (0.3.0's surfaces are **pull-based only** — no notifications, no shame-analytics), marks
  inside Recall reports, and API write scopes beyond `POST /v1/marks`.

## 5d. Post-1.0 arc — 0.4.0 (sessions: frames → sessions reframe)

The 0.3.x line is shipped (0.3.0 through the 0.3.3 hotfix), and **0.4.0 (P8)** shipped as `v0.4.0`
(2026-07-11), a **separate arc** layered on the shipped app, **not** a retrofit of the phases above. It is tracked in detail in
`docs/0.4.0.md` (roadmap) and `03` (contract, `§7e`); this section states only the strategic
*what/why*.

- **The problem.** Everything shipped so far treats the **frame** as the unit of recall — the right
  foundation, but not how a person (or an AI agent) thinks about the day. Nobody asks "show me frame
  48 213"; they ask "what happened in **that meeting**", "what did **that Claude Code run** change",
  "what was I doing **before lunch**". The strategic direction — ambient memory for AI coding agents
  over the MCP layer, plus personal recall — needs the askable unit to be the **session**. Two issues
  were deliberately parked for this arc because building them on frames would be throwaway work: #54
  (app/window-title keyword intelligence, `07` #98) and **#88** (recognize Codex / Claude Code /
  terminal-vs-desktop and distinguish user input from agent output, `07` #102) — the arc's **anchor
  issue**, which closes with it.
- **P8 — Sessions.** Generalize the app's existing proto-session — where-was-i's "last sustained
  context" (`03 §7b`) — from *the one run behind me* to **every run, persisted, with a stable
  identity** (the D2 freeze rule), then make those objects **visible** in the UI (Timeline bands +
  drill-in, Moment, Deck — no new NavRail route, D13) and **queryable** over the API/MCP (`list_sessions`
  / `get_session` / `ask_session`, read-only — D12). Recognition is a versioned in-repo **taxonomy**
  (D6/D7); user-vs-agent turns land as best-effort **exchange** artifacts (D8); titles/summaries are
  **lazy** and the recap **is** the existing report engine (D3). The strategic payoff: *an agent can ask
  what happened in the user's last session with a given tool.* Hard constraints: **one** schema
  migration (PR3, 10 → 11 — D4); **additive** — zero frame-level behavior change (D10); **no audio**
  (D14); **pull-based / non-shaming** (D11). v0.4.0 is the **first auto-delivered release** (D16).
- **Ships in 0.4.0** (build order): PR1 specs → PR2 ground truth + harness → PR3 schema 10 → 11 →
  PR4 segmentation engine + recognition → **PR5 UI ∥ PR6 API/MCP** → PR7 audit + tag `v0.4.0`.
  Strictly sequential through PR4 (contract → evidence → schema → engine); PR5/PR6 are disjoint
  parallel lanes; PR7 after both. The one migration is forward-only and creates structure only —
  sessions are derived and fully recomputable from frames (D1), so no frame data is ever at stake.
- **Deferred (recorded in `07` during PR1, not built this arc):** a **user-editable taxonomy
  override** file (D6 — the in-repo file is the mechanism this arc, `07` #107); a **dedicated
  Sessions NavRail route** (D13 — a 0.4.x conversation only if the in-route surfaces prove
  insufficient, `07` #108); **audio / transcripts** (D14 — the `'transcript'` `session_artifacts`
  kind is the reserved extension point, `07` #75); and monitor hot-unplug hardening (`07` #91 —
  sessions make its silent stalls *visible*, raising it to a first-line 0.4.1 candidate).

## 5e. Network API increment — Gate A contract (2026-10-15)

Make the existing API available to users’ other devices while preserving local storage and
inference. This is a product feature for all installations, configured through the existing API
master switch and port, with no personal host, account, path or credential baked into the app.

- **Sequence:** review and commit this API-only contract amendment before implementing the
  listener, configuration, tests and accompanying active privacy/documentation copy. Historical shipped
  localhost descriptions in §5c remain historical; this section supersedes their binding rule
  for the next API increment. A contract change is not evidence that runtime behavior shipped.
- **Boundary:** default-off, authenticated plain HTTP on a configurable IPv4/IPv6 literal.
  Missing `api.bind_address` defaults to `127.0.0.1`, including existing enabled installations;
  upgrades never broaden access automatically. A token holder on any reachable host can read history. MCP destination
  default URL remains loopback; the inference sidecar remains loopback-only. A separately reviewable
  approved boundary permits only the standalone stdio HTTP-client wrapper on other OSes and
  explicit remote HTTP/HTTPS destinations, retaining `.no_proxy()` and rejecting all redirects;
  no listener/CORS changes or network-hosted MCP server. Approval does not establish implementation.
- **Scope:** listener, existing auth/configuration lifecycle, bind-address configuration and accurate copy.
  No scheduler, vault writer, schema migration, report endpoints, Telegram, firewall automation,
  public hosting or a network-hosted MCP server. The explicit remote-wrapper exception above
  supersedes the old remote-destination prohibition only. Scheduled delivery and outbound Telegram need separate reviewed
  amendments before their implementations.
- **Acceptance:** native Windows verification plus real cross-machine authenticated health
  (200 with actual version; 401 without/wrong token), local MCP, disable/enable, restart, Ask
  disconnect and loopback inference checks (`03 §7c`). Remote HTTP evidence is mandatory.

## 5f. Scheduled local memory — approved contract (2026-10-15)

The maintainer approved the report policies and execution ("Sure GO"). Build one shared durable
report service, default-off daily scheduling and user-selected Obsidian-compatible folder export
before optional Telegram delivery. This is a product-wide feature, not a personal automation.
The local schedule is an explicit exception to historical pull-only behavior; no productivity
scores, nudges, cloud inference or automatic model downloads are permitted. Existing manual
Recall/session reports retain their behavior while sharing execution and evidence collection.

Contract order: reviewed schedule/local-report amendment → separate outbound Telegram amendment
→ implementation C–E. The API increment remains independently reviewable; report HTTP/MCP
generation/retry/delivery endpoints and deeper weekly/project memory remain deferred; saved-output
reads are promoted by the approved external-agent increment (`§5h`, `03 §7f`). `03 §8c` freezes the approved
activation, DST, retry/regenerate, provenance, retention, ownership and error rules. Explicitly
selected folders/timezones/credentials are runtime setup, not another plan-approval gate.
Manual execution also supports a selected calendar date or inclusive date range, including
explicit partial-day reports for today. It uses the same local report/destination pipeline and
preserves daily scheduling semantics; manually selected output stays separate from daily notes.
Pilot/release validation remains evidence-based across supported user environments; no completion
or usefulness metrics are inferred from contract approval.

## 5g. Optional Telegram digest — separate approved exception (2026-10-15)

The maintainer approved default-off delivery of a concise text digest derived from the saved
report, to a user-selected Telegram destination. This narrow outbound-content exception is
independent of the API listener and the local schedule contract (`03 §8d`). Users explicitly
opt in to selected report content leaving the device. No raw OCR, screenshots, two-way commands,
cloud inference, implicit recipient or telemetry. Credentials live only in Windows Credential
Manager. Generation, vault writing and Telegram acceptance have separate durable receipts;
unknown acceptance never triggers automatic resend. Actual Bot API delivery requires an
explicitly approved test recipient/send and inspection, not a fabricated acknowledgement.

## 5h. External-agent memory API — approved increment (2026-10-16)

Nico approved the attached external-agent memory plan and its recommendations for execution.
The normative contract is fully recorded in `03 §7f`, not delegated back to a draft attachment.
Approval is not implementation evidence and authorizes neither merge nor release. The separate
remote-wrapper boundary amendment must receive independent review before its behavior lands.

**Outcome:** external agents enumerate a selected UTC-bounded day's chronological captured
evidence, resume checkpoints, inspect truthful capture limitations and read saved reports using
independent read-only credentials. They need no answer model, scheduler, folder export or
Telegram. Existing local capture/search/Ask/reports/schedules remain compatible with API off.
No Screenpipe adapter/endpoint parity or superiority claim; no inferred work-duration scoring.

**Frozen choices:** tuple-keyset `(captured_at,id)` ascending bounded-membership/live-content
pages; full separately labeled raw and filtered text including textless frames; per-history
identity and transactional no-reuse high-water allocator in reserved settings metadata; truthful
bounded current-process observations (1000 transitions, at most 100 returned intervals, unknown
history explicit); independent read token with explicit read/mutation/capture/inference classes;
existing Rust stdio HTTP wrapper with explicitly selected remote HTTP/HTTPS, `.no_proxy()`, no
redirects; saved-report reads preserving original prose and generation-time provenance. Frame/
report pages default 100, valid 1–1000; cursors at most 4096 bytes, literal app/title filters at
most 256 UTF-8 bytes each. These are approved contracts, not shipped APIs at the planning base.

**Order:** P0 inventory/contracts → P1 independently useful HTTP evidence/identity/auth/coverage
and credential UI plus a runnable checkpoint consumer → P2 portable remote MCP → P3 saved-report
HTTP/MCP reads → P4 integrated acceptance and active documentation. Each unit has separate
observed acceptance gates (`03 §7f.9`); review has at most two rounds before explicit blocker
escalation. No redundant blanket GO is needed. One owner for shared traits/router/auth; disjoint
MCP transport/packaging can proceed after interfaces freeze; report work follows P1 contracts
and P2 priority. No concurrent writers on shared implementation files.

**Fences:** no change to API enable/bind defaults, connection/header/shutdown security, capture
runtime or inference exposure; desktop/capture/inference remain Windows-only. Only the standalone
HTTP-client wrapper targets Windows x86_64, Linux x86_64 and macOS arm64, and native evidence is
required before calling artifacts verified. No hosted MCP server, proxy expansion, automatic
firewall changes, new NavRail route, accounts, arbitrary scopes UI, cloud inference, telemetry,
remote generation/retry/delivery, durable capture journal, tombstone replication or default
uploads. Report reads promote only saved-output deferrals, not generation permissions. Schema
stays 12; if storage protection or measured query plans require a table/index, stop for separate
migration approval rather than silently widening scope. No live-profile write/download/send.

**Evidence:** fixture drain/resume (>1000 frames), no read side effects, raw/full-text parity,
identity/rewind reconciliation, truthful unknown coverage, URL/redirect/proxy tests, native
package checks on each named target, real non-Windows-wrapper → separate Windows-app path, saved
run comparison with model unloaded/schedule off and ordinary-user regressions. Missing native/
remote evidence is blocked acceptance, never inferred from compile or loopback fixture success.
Archives/checksums publication, NSIS release/updater actions, merge and tags require separate
authorization. Rollback revokes the read token/disables API without deleting local history,
reports or notes; restoring/using older writers invalidates client checkpoints explicitly.

## 5i. Deeper memory (F3) — approved increment (2026-10-17)

Nico approved the deeper-memory F3 contract ("GO", 2026-10-17), explicitly waiving the direction
plan's F1-pilot contingency (`docs/memory-api-direction-plan.md` §5, F3). The normative contract
is fully recorded in `03 §8e`, not delegated back to the draft plan
(`docs/deeper-memory-f3-plan.md` remains context). Approval is contract-only: it authorizes
neither merge nor release, and G1–G6 each still require real per-unit verification, independent
review and separate merge authorization before landing.

**Outcome:** the daily report stays the atomic unit; F3 adds exactly two layers above it — one
index-linked weekly synthesis per source week, citing that week's saved daily reports and the
underlying evidence, and one user-created project note accumulating source-backed decisions,
open questions and next steps across days. Loop: daily reports → weekly synthesis → explicit
user-confirmed promotion → durable project note. This promotes the deeper weekly/project memory
deferral recorded in `§5f`; no second engine and no second model path — weekly runs execute
through the shared report application service.

**Frozen choices:** kind token `weekly_synthesis`, deliberately distinct from the existing
on-demand `ReportKind::Weekly` (`03 §8b`), which keeps its name, label and filename stem; the
discriminator is an indexed `kind` column on `report_runs` (`kind TEXT NOT NULL DEFAULT 'daily'`)
carried by one forward-only migration 12 → 13 with a `schema_version` bump, exposed on
`GET /v1/reports` projections as a deliberate wire-contract change; weekly run identity is
schedule ID + ISO-week UTC window (Monday 00:00 local → next Monday 00:00 local in the selected
IANA timezone; DST weeks are 167/169 hours, never 24·7 UTC hours), uniqueness
`(schedule_id, window)`, retries reuse the run and Regenerate creates a linked revision; truthful
statuses reuse the daily set (`NO_DATA`/`PARTIAL`/`BLOCKED`/`FAILED`/`OK`, `03 §8c`); the weekly
note is `ScreenSearch/Weekly/YYYY-Www.md` with entries added to the existing
`ScreenSearch/Index.md`, frontmatter v1 → v2 (`kind`, `daily_run_ids`, `window_local_dates`; v1
daily notes must still parse) under headings `Activity`, `Explicitly evidenced decisions`,
`Open questions`, `Next steps`, every item citing frame/session/daily-run IDs; exactly one
user-created project note (`ScreenSearch/Projects/<slug>.md`), written only through explicit
user-confirmed promotion as an append-only ledger (never invented, renamed or deleted; user edits
surface as conflicts, rejected proposals are not persisted); the first eligible weekly window is
the first complete ISO week after activation, and catch-up processes missed complete ISO weeks
oldest-first, capped at the 4 most recent, with older gaps left visible and never claimed covered.

**Order:** G1 contract + 12 → 13 migration (frozen in `03 §8e`) → G2 weekly engine through the
shared report service → G3 weekly note + index/path/provenance writer branches → G4
promotion/append → G5 weekly scheduler + catch-up → G6 weekly Telegram digest with its own
weekly-specific format under the existing `03 §8d` exception. G1–G6 are separately reviewable;
no repeated blanket GO.

**Fences:** no automatic edits to personal identity or existing knowledge pages; no promotion
without explicit user confirmation; no multiple project notes in this slice; no report generation
over HTTP/MCP (C3 stays deferred) and no new capture, audio, cloud inference or scoring/nags
(pull-based, D4/D11); weekly runs are local, scheduled and default-off, same as daily; `Weekly`
and `weekly_synthesis` never share a token, label or filename stem; weekly and project notes are
user-owned output and are never pruned by the 90-day run retention; no schema change beyond the
single 12 → 13 migration; no Telegram send without an explicitly approved test recipient.

**Evidence:** native acceptance mirrors the daily arc — a real weekly run over a real week of
saved dailies, the rendered note inspected in Obsidian with working links, and one promotion
performed in the running app. Per-unit checks include `weekly_window_handles_dst`,
`weekly_run_identity_reuses_schedule_window`,
`migration_12_to_13_defaults_existing_runs_to_daily`, `frontmatter_v1_still_parses`,
`promotion_requires_explicit_confirm`, `edited_weekly_note_is_preserved` and
`weekly_catchup_oldest_first`; missing native evidence is blocked acceptance, never inferred from
fixtures or compile success. Review focus: week-boundary/DST correctness, synthesis over-claiming
(uncited claims are a defect, not style), promotion races and kind separation. Rollback disables
the weekly schedule without deleting local history, reports or notes.

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Tauri 2 + WebView2 packaging friction (new) | Spike in P0; keep the shell thin; the kernel is shell-agnostic behind traits. |
| WGC capture integration (new code path) | `CaptureSource` trait + an early P2 spike; fall back to a simpler capture if WGC misbehaves. |
| Sidecar orphan/hang | Job Object KILL_ON_JOB_CLOSE + startup reap + heartbeat/restart (hard requirement). |
| Resource spikes surprising users | On-demand/timed by default; explicit schedule UI; idle-only option; per-job budgets. |
| Global hotkey conflicts (PowerToys, IMEs, games) — 0.3.0 | Non-default combos (`Ctrl+Alt+Z` / `Ctrl+Alt+M`), both configurable; a failed registration is a **visible Settings warning + toast**, never a silent no-op (`§5c`; `03 §7b`/§8). |
| API token leakage or plain-HTTP interception → full-history read from any reachable host | API **off by default**; bearer auth on every route; explicit configurable-IP/plain-HTTP threat model and loopback default for missing address; regenerate-token control; user-managed network/firewall boundary, no public hosting (`03 §7c`). |
| Scope creep | Automation + nice-to-haves are explicitly **out of v1.0**. |

## 7. Non-goals (reaffirmed)
macOS/Linux desktop/capture support (the approved standalone stdio HTTP-client wrapper exception
permits the standalone stdio HTTP-client wrapper on Windows x86_64, Linux x86_64 and macOS arm64
with explicitly selected HTTP/HTTPS remote destinations; it uses `.no_proxy()`, refuses redirects,
and leaves app/capture/inference Windows-only, inference loopback-only, and API listener/CORS
unchanged; approval is not implementation evidence) · OS automation (v1.0) · cloud/telemetry · accounts/multi-user · real-time vision ·
V1 data import · **proactive nudges / notifications** (0.3.0 recall is pull-based only; sessions stay
non-shaming — no streaks/scores, D11-of-0.4.0) · **audio capture / transcription** (*for now* — a
0.4.x candidate, `§5d`; the `session_artifacts.kind='transcript'` extension point is reserved but
unbuilt, D14).
**No telemetry — a permanent product commitment (D15-of-0.4.0).** The app never phones home beyond
the existing signed-updater manifest fetch (`03 §11b`). "Is anyone else using this?" is answered by
**passive repo-side signals** (GitHub release-asset `download_count`, Insights → Traffic, stars/
issues), never by in-app instrumentation — the reading procedure lives in `07`'s manual steps.

## 8. Status
- **License decided: MIT.** No open strategic questions.
- **v1.0 (P0–P5) shipped** (`v0.1.0`, 2026-06-24); the **0.2.x arc shipped** (attention-first text
  signal + recall workflows; `§5b`, `docs/0.2.0.md`); the **0.3.0 arc (P7) shipped** (`v0.3.0`,
  2026-07-04; `§5c`, `docs/0.3.0.md`) and its **0.3.1 triage patch shipped** (`v0.3.1`,
  2026-07-05; `docs/0.3.1.md` — the #64 vision-throughput fix + polish), and the **0.3.2 arc
  shipped** (`v0.3.2`, 2026-07-06; "P7.2 product shell mini-arc", `docs/0.3.2.md`): **lifecycle**
  (auto-update — the updater's genesis release, hard-sequenced before 0.4.0 and met; systray +
  quick actions) **and interface** (shell-layout hardening; Settings two-tier IA), under the
  **zero-DB-schema-migration** constraint (held — schema stays 10); the **0.3.3 hotfix shipped**
  (`v0.3.3`, 2026-07-07 — UIA skips Chromium/Electron windows to stop browser freezes, `07` #93).
  **The 0.4.0 sessions arc shipped** (`v0.4.0`, 2026-07-11; `§5d`, `docs/0.4.0.md`, `03 §7e`):
  sessions over frames, additive and pull-based, delivered via the auto-updater 0.3.2 shipped, the
  **first auto-delivered release** (D16, gate passed live before the tag; closes #88).

---

*Next layer:* `03_MASTER_PRODUCTION_SPEC.md` — the engineering truth: schema, traits, event/command
contracts, job-queue + sidecar protocols, config, logging, testing, CI/CD, definition of done.
