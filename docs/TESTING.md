# Testing — quick guide

## Network API increment (unreleased): native acceptance checklist

These are required checks, not claims of completed acceptance. Use an isolated test profile
and fixture history; never serve personal history with a fixture token. Record raw outputs
without tokens or screen content. Preserve the everyday installation.

1. Fresh profile: API disabled, no listener. Enable in Settings; inspect the real socket
   (`Get-NetTCPConnection`) and confirm default `127.0.0.1:<configured-port>`.
2. Configure IPv4/IPv6 loopback, a specific interface and wildcard addresses. Verify actual
   sockets, immediate address/port apply and restart persistence. Invalid literals must leave
   prior configuration unchanged. On a network bind, a separate computer's authenticated
   `/v1/health` returns 200/version; missing/wrong token returns 401. Unknown paths return 401
   unauthenticated and 404 authenticated. Record manual firewall changes separately.
3. Existing enabled profile missing `api.bind_address`: restart and confirm loopback exposure,
   with token, enabled state and port preserved; no automatic broadening or upgrade notice.
4. Rotate token: old rejected, new accepted. Disable/re-enable preserves configuration.
   Port conflicts, including wildcard/specific listeners, retain enabled intent with visible error.
5. Local MCP handshakes via loopback when the selected bind accepts it. Shipped v0.4.1 rejects
   remote destinations; verify the separate P2 wrapper's explicit URL/redirect/proxy boundary
   with its focused fixtures, then a separately scoped cross-machine app run. Cancel a real
   Ask stream only in an authorized native acceptance run. Inference remains loopback-only.
6. Check address field validation, keyboard access and loading/error/off/listening states;
   distinguish wildcard listen addresses from client URLs and bracket IPv6 client addresses.
7. Schedule input exposes hours/minutes only. New memory path settings, snapshots and receipts
   use forward slashes; legacy backslash ownership records remain recognized. Verify native
   Windows atomic file creation/replacement and preservation of edited-file conflicts.

Unit/integration fixtures and a successful installer build do not replace these observations.
Rollback: disable the API or select `127.0.0.1` to restore loopback-only serving. No schema
migration or token reset is part of this increment. Schedules and Telegram are separate work.

Run from the repo root. Each command stands alone. Green = good.

## External-agent memory API P1/P2 — verification status

These are evidence boundaries, not a release or app-acceptance claim. Contract/source:
[API frames](API.md#chronological-frames), [coverage](API.md#coverage),
[MCP transport](MCP.md#configuration), [consumer](MEMORY_CONSUMER.md), and spec `03 §7f`.

| Observed evidence | Scope / still pending |
|---|---|
| Native store review at `history_pages.rs` SHA256 `f71c4c21802ccaad74980ba27b0805563f81f4773cd78f2a2f37a478a40a0d9e`: focused 14/14; full store 156 passed, 0 failed, 3 ignored; all-target clippy exit 0. | Isolated store snapshot only, not integrated HTTP/UI/app acceptance. Pre-fix `u64` decoding did not compile; the checked `i64` conversion fix is present. |
| API interface/cursor checkpoint: four cursor unit tests and API-lib clippy passed; initial HTTP attempt blocked by that store compile defect. | Historical blocker fixed by store owner; these notes alone do not prove the later HTTP rerun passed. |
| P2 transport report: build/test/clippy/fmt exit 0; 66 tests (36 unit, 8 remote transport, 22 stdio). Registry at `7831e84` has nine normal/six read-only tools. | Historical (earlier commit): component evidence only. TLS-negative fixture actually ran there with OpenSSL; it can skip if OpenSSL is absent. No feed/coverage MCP tools at that commit. |
| P2 transport committed at `409d380` (draft PR [#127](https://github.com/nicolasestrem/screensearch/pull/127)): package-scoped build/test/clippy/fmt exit 0; **77 tests** (41 lib unit, 6 memory_tools, 8 remote transport, 22 stdio), 0 failed; packaged-archive verify observed `tools/list: 11 tools` and read-only `8 tools`. | Component/protocol evidence only: the two new tools drive controlled HTTP fixtures, not a real P1 end-to-end read. Not in any released binary or the P1 bundled wrapper. Windows/macOS legs of the native package workflow not yet observed. |
| Earlier local consumer run: 20 tests, 4 skipped, exit 0. | Historical baseline; the fixture runner did not yet exist. A skip is not successful API consumption or P1 acceptance. |
| Current consumer run (F1/F2/F3 fixes): **23 tests, 0 skips, 7 real-API tests executed**, exit 0, against a real `ApiServer` + `SqliteStore` (no mock HTTP). | Real-API frame/coverage consumption receipted at component scope; the runner binary + source hash were re-verified independently. Not P1 app/UI acceptance. |

Local evidence notes are under the active profile scratch directory, not repository links:
`scratch/memory-api-p1-feed-api-interface.md`,
`scratch/memory-api-p1-pages-native-review.md`, and `scratch/memory-api-p2-transport.md`.
Their timestamps/immutable snapshots matter; inspect raw outputs before advancing a gate.
Do not promote a base build, worker summary or synthetic fixture into current app acceptance.

### Focused checks (required receipts, not claimed rerun here)

On a prepared native Windows checkout, use the **unchanged CI order below first**: UI install,
lint, build → MCP staging → fmt → workspace clippy → build → test → generated-binding guard.
Keep each exit code and source/snapshot identity. Focused follow-ups do not replace that order:

```sh
cargo test --locked -p store --lib history_pages::tests -- --nocapture
cargo test --locked -p api --lib history::tests -- --nocapture
cargo test --locked -p api --test http_api history_reads -- --nocapture
cargo test --locked -p api --test http_api history_bad_queries -- --nocapture
cargo test --locked -p mcp
cargo clippy --locked -p mcp --all-targets -- -D warnings
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s examples -p test_memory_consumer.py -v
```

The last command without `SCREENSEARCH_CONSUMER_FIXTURE_CMD` visibly skips the real-API
cases. The runner exists at `crates/api/examples/memory_consumer_fixture.rs`; build it with
`cargo build --locked -p api --example memory_consumer_fixture` on the prepared native
checkout, then set that variable to a JSON argv array naming the **absolute** path of the built
executable. `CARGO_TARGET_DIR` is not stable across trees, so resolve the directory once with
`cargo metadata --format-version 1 --no-deps` (its `target_directory` field) and join
`debug/examples/memory_consumer_fixture[.exe]` — do not hardcode a private `target/debug` path
or a path from another checkout. The harness creates fresh disposable SQLite state and random
read credentials. Record all seven integration outcomes; runner presence is not an execution
receipt. The consumer guide's earlier pending-runner wording/old suggested integration-test
path is not evidence that a run has passed.

Required fixture/native checks include >1000 rows, timestamp ties/out-of-order timestamps,
filters before limit, 256-byte UTF-8 filters/4096-byte cursor bound, strict invalid input,
full/missing/empty text, frozen membership, deletion/retention during traversal, late
enrichment and re-scan, page checkpoint crash/resume, identity conflict and decreasing-high-
water reconciliation, known restore after high-water catch-up, explicit timezone/DST days,
read-token lifecycle persistence failures, HEAD/401/403/404/405, and no read side effects.
Do not skip or remove IPv6 tests because a Linux host lacks `::1`; that is a failed host
prerequisite, not a native Windows pass. Use fixtures, not personal captures or model downloads.

Receipted since the table above: the **integrated native Windows CI-order gate** now passes
on the existing native checkout at `f76dda6` (Rust 1.95.0 / Node 22): fmt, clippy
(`--all-targets -D warnings`), build and test all exit 0 — **928 passed, 0 failed, 15
hardware/model-ignored** across 60 binaries — plus the 41-file source-hash guard and the
generated-binding guard. See `scratch/memory-api-p1-integrated-native.md`. This is native
build/test evidence, **not** app/UI acceptance.

Still pending unless separately receipted: rendered credential UI states (the native UI
acceptance run is prepared but not executed — its isolated launch is blocked on the session-0
safety question), separate-machine frame traversal/consumer resume, supported native
wrapper/archive execution (Windows/Linux/macOS legs) and real paging/latency measurements.
P3 saved-report reads are **resolved** (Nico GO, stored-presence visibility — see the
[P3 verification status](#p3-saved-report-reads--verification-status)); local producer and
packaged-client and native Windows checks now pass; fresh hosted P3 gates remain open. Generation/retry/scheduling/delivery
remain deferred. No merge, publication, installer replacement or release is implied.

### P3 saved-report reads — verification status

P3's retry-result visibility question is **resolved** (Nico GO): saved output is exposed by
persisted presence even while a run is queued/retrying; absent output stays explicit `null`, stored
`NO_DATA` stays factual, and a malformed saved payload is a read error. The contract is
`03 §7f.5–7` / `§7f.10`; the endpoints are documented in
[API saved reports](API.md#saved-reports). The source branches are
`feat/agent-memory-saved-reports` and `feat/agent-memory-saved-report-mcp`. Parent observed the
following local/native results; fresh hosted gates remain open. Do not reuse P2 old-head CI
(or any base build) as P3 evidence.

The final unpacked Linux release client passed **21 checks** against the genuine API/SQLite
fixture: 1035 frames/details (zero mismatches), restart/resume, 11 report summaries/details,
queued/running saved output, null/NO_DATA distinction and full historical provenance. The
independent seed-vs-SQL manifest agreed; report/delivery/file state stayed unchanged,
provider calls and credential leaks were zero. The archive also passed checksum/unpack/version,
13 normal/10 read-only registry and an actual authenticated Store read. See `05_BUILD_REVIEW`
for the final source/archive hashes. This local archive script used Node 26.8.1/Rust 1.95.0;
Native Windows checks subsequently passed under Node22/Rust1.95: API library 23/0,
HTTP 51/0 with one retained ignore, Tauri lifecycle 25/0, the full MCP package and
workspace-unified TLS suite, plus actual Windows ZIP execution/registry/auth-read verification.
Parent verified 20/20 remote source hashes and all raw command exits. Independent review
round1 found no security/logic issues. Fresh P3 CI remains pending; GUI and direct cross-machine
acceptance are not inferred.

Three evidence layers, which must not be conflated:

- **Genuine producer fixture — `crates/api/examples/memory_consumer_fixture.rs`.** A test-only
  runner that composes the real `ApiServer` router over a real isolated `SqliteStore` (no mock
  HTTP; capture disabled, schedules off, answer providers unavailable). A P3 extension seeds real
  `ReportRun`/`SavedReport`/delivery rows through the production store writers and reads them back
  through the real routes. Because this fixture never starts the app's `MemoryRuntime` scheduler or
  worker, queued/running delivery cases are **inert** here. **Never launch the real app against a
  queued-delivery fixture** — app startup can process those rows even with schedules off. Any
  genuine-app smoke fixture, if ever prepared, must use only finished saved rows with no pending
  deliveries.
- **Raw-HTTP protocol fixtures — `crates/mcp/tests/memory_tools.rs`.** The MCP wrapper's own
  loopback fixture with a hand-authored `Responder` speaking the frozen wire. These are
  **protocol-fixture evidence, not real-product acceptance**: they prove the MCP passes the wire
  through unchanged, not that the producer routes exist on that branch.
- **Independent expected manifest — the acceptance driver.** `examples/mcp_memory_acceptance.py`
  (non-Windows, read-only MCP→HTTP) gains a `reports` block produced by the operator **from the
  app/DB, never scraped from a tool response**: `expected_summaries` (complete public
  `ReportSummary` objects, newest first for the manifest window) and `expected_details` (complete
  public `ReportDetail` objects with a present `result.markdown` replaced by `markdown_sha256`;
  absent result is literal `null`). The driver validates exact field sets/nulls/types/order and
  page exhaustion, compares each detail, Markdown digest and full provenance arrays, and adds a
  bounded `--report-page-size` to force paging without auto-draining past the manifest window.
  Driver order: coverage → frame pages/restart/resume → report pages/detail → frame source details.
  A frame-only manifest stays usable but must report P3 `not_exercised`, never full P4 acceptance.

Required P3 assertions (local fixture checks observed above; native/application boundaries remain separate):

- Read a persisted fixture run with the **model unloaded and schedules off**, assert the exact wire
  bytes, and assert **no** generator/scheduler/file-writer/Telegram call and **no** store mutation
  (phase/`updated_at`/`result` unchanged; a panicking provider/writer host is never invoked).
- SQL-side overlap/status/keyset filters before `limit`; ties and revisions; mutable statuses
  re-listed; precise `null` vs `NO_DATA`; internal-field exclusion (no
  `schedule_id`/`config_snapshot`/delivery/path/credential keys); and unchanged stored
  markdown/evidence/session provenance after source deletion/resegmentation.

Report tuple tie-group sorting and sparse-filter costs are **measured-only**: no latency budget is
promised and no index or schema change is introduced. GUI/native and separate-machine
(cross-machine) acceptance remain **blocked** until their own observed receipts; a future run on
the same host is not a cross-machine result.

## ✅ The one command

```sh
cargo test --workspace
```

That runs the default workspace tests, not ignored/manual acceptance gates. **0 failed** only
passes the tests actually executed; skipped/ignored cases and native acceptance remain separate.

## 🔁 Before you push (CI-order gates)

Run these. All must be clean:

```sh
(cd ui && npm ci && npm run lint && npm run build)
node scripts/stage-mcp.mjs   # once per clone — src-tauri's externalBin sidecar; bare cargo fails without it (0.3.0 PR8)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
git diff --exit-code -- ui/src/bindings
```

Tip: copy-paste them one at a time. Don't move on until one passes.

## 🩹 If something fails

| You see | Do this |
|---|---|
| `fmt` diff | `cargo fmt --all` (auto-fixes), then re-check |
| `clippy` error | read the `--> file:line`, fix that one spot, re-run |
| a test `FAILED` | scroll up to the test **name** + its `assert` message; that's the clue |
| `npm` error | `cd ui && npm ci` once, then `npm run build` |

## ⏭️ What gets skipped (and that's fine)

Tests marked `ignored` need a GPU, a downloaded model, or hardware. They are **not** run by `cargo test`. Only run these on purpose:

```sh
# real llama-server: downloads models + uses the GPU (slow, big)
cargo test -p inference --test smoke -- --ignored --nocapture

# event-driven capture hooks: start/drop the Win32 hook source 50× — no leak/hang (real desktop)
cargo test -p capture -- --ignored
```

The event-driven trigger logic itself runs in plain CI: the 9 `crates/capture/src/trigger.rs` unit
tests cover foreground debounce / rate-ceiling / idle-edge behavior (pure, no Win32), and the kernel
settings round-trip + sanitize + retired-key-drop tests cover the `capture.event_*` keys. Only the
hardware hook lifecycle test in `events.rs` is `#[ignore]`d. *(0.3.0 PR2 trimmed the six triggers to
foreground + idle, deleting the clipboard, click, scroll-stop, and typing-pause tests with their
triggers — `docs/0.3.0.md`.)*

## 🎯 Just one crate (faster)

```sh
cargo test -p inference     # the sidecar
cargo test -p kernel        # workers + scheduler
cargo test -p store         # database
```

## 🟢 The no-orphan gate (P4's must-pass)

Proves the sidecar can't outlive the app:

```sh
cargo test -p inference --test no_orphan
```

Want it to be `ok`. If it's not — stop and ask.

## Archived — manual acceptance checklists (pre-0.4.0)

The per-PR manual acceptance sections for the 0.2.x through 0.3.2 arcs (Manual PR7 audit,
event-driven capture, model tiers, image-lane removal, Flow overlay, where-was-i + marks,
local API + export, MCP server, polish bundle, auto-update) have shipped and moved to
[`specs/archive/TESTING.pre-0.4.0.md`](../specs/archive/TESTING.pre-0.4.0.md). The sections below
cover the current 0.4.0 sessions arc.


## Dev-only harness — segmentation ground truth + validation (0.4.0 PR2)

The `crates/harness` binary is a **dev-only, read-only** referee for the sessions arc. It is a
workspace crate, so it is built and tested by the normal `cargo` gates above, but it is **never
bundled** by the NSIS installer (only `src-tauri` + the `screensearch-mcp.exe` externalBin ship).
Run it with `cargo run -p harness -- <subcommand>`.

**Read-only guarantee.** Every query path opens the DB with `SQLITE_OPEN_READ_ONLY` + `PRAGMA
query_only`; the harness's unit tests assert a write is rejected on that connection. The only file
it writes to the DB side is the `backup` target (a `VACUUM INTO` snapshot to a fresh file). Exports
and hand labels are personal screen history and live under the **git-ignored** `harness-data/`.

**Automated tests (CI-safe, no real data).** `cargo test -p harness` runs the pure segmenter,
taxonomy, label-parsing, scoring (typed DP-optimal boundary matching), digest, and read-only export
tests against synthetic fixtures + a tempfile SQLite DB. No test touches `%APPDATA%` or a real path.

**Manual end-to-end (Phase A/B/C, maintainer-in-the-loop).**
1. **D5 backup FIRST** (release-blocker-class; before any other live-DB command):
   `cargo run -p harness -- backup --to <a dir OUTSIDE the repo and OUTSIDE %APPDATA%/app.screensearchv2c.desktop/>`.
   It writes `screensearch-YYYY-MM-DD.db`, refuses to overwrite, refuses a destination inside the
   repo tree or the app data dir, and prints `PRAGMA integrity_check` + source/copy row counts as
   the attestation. WAL note: if the app was force-killed and left an unrecovered `-wal`, a
   read-only open fails with an actionable message; start the app once (or point `--db` at the
   backup) and retry.
2. `cargo run -p harness -- suggest-days` prints a per-day survey (frames, distinct apps, coarse
   AI/meeting window-title signals, marks). Pick 5-10 representative days (a meeting-heavy day, a
   Claude Code day, a Codex day, a browser-AI day, a mixed/fragmented day, plus one contiguous
   2-3-day stretch for the stability check). June-July days avoid the DST-transition guard.
3. `cargo run -p harness -- export --days 2026-06-15,2026-06-16,...` writes each day to
   `harness-data/<day>/` (`day.json`, `frames.jsonl`, `marks.jsonl`, `digest.md`, `labels.toml`).
   Re-exporting a day refreshes the data files but preserves an existing hand-edited
   `labels.toml` (it prints `(kept existing labels.toml)`), so it is safe to re-run.
4. Hand-label each day's `labels.toml` from its `digest.md` (the readable context-run timeline;
   marks appear as anchors). Under an evening for the whole sample.
5. `cargo run -p harness -- score` (optionally `replay`, `sweep`, `stability`) scores boundary
   precision/recall/F1 (+/- tolerance) and tool-recognition accuracy against the labels.
   `sweep`/`stability` write markdown to `harness-data/reports/`.

**The segmenters — `--algo micro | grouped | concurrent | shipped`.** Pass 1 (`segment_micro`) produces
unfloored app-run micro-spans; pass 2 groups them. Three algorithms:
- `concurrent` (**default**, `06` #28 / `07` #114): the **per-identity-track** model. Sessions of
  different identities may overlap in wall-clock time (an AI track spans a meeting; two AI tools run
  at once) while a frame belongs to exactly one session. A foreign AI id opens its own track; a short
  unrecognized run absorbs into the last-touched open AI track, a leading one ramps into an opening
  track, a run over `absorb_max` is focus; an AI track emits only if its summed recognized presence
  reaches `IDENTITY_QUALIFY_MS`; meeting bands are no longer barriers and overlapping meetings are
  not merged.
- `grouped` (`06` #27): the serial two-pass segmenter (one open group, meeting bands as barriers) —
  kept as the A/B baseline.
- `micro`: the ungrouped `§7b` app-context baseline.
- `shipped` (PR4): calls the production `crates/sessions` concurrent engine with the frozen
  `merge_gap=2700s`, `absorb_max=1800s`, `meeting_gap=480s`, focus floor/density, qualification,
  and W constants. The harness `--gap-close`/`--min-len` flags still exercise the two final settings.

**`labels.toml` is v2 (`06` #28):** non-overlap is enforced **per identity track**, not globally —
`ai` sessions may not overlap another `ai` with the same `tool`; `focus`/`other` may not overlap
another of the same kind; `meeting` labels may overlap; the file is globally sorted by start.
Different identities may overlap. Serial (pre-v2) label files stay valid.

- `score` reports the **identity-partitioned** typed boundary P/R/F1 as the primary metric plus the
  pooled position-only `posF1` comparability column, at BOTH 120 s and 180 s tolerance (the D9
  evidence pair); `--tolerance <s>` overrides with a single window. Labels are snapped to the nearest
  captured frame (a disclosed policy for boundaries inside no-frame idle gaps).
- `replay` prints each session's context key, kind, tool, host, frame count, close reason, and marks
  overlapping sessions with `~` (the concurrency indicator).
- `sweep` runs the Stage-A `merge_gap x absorb_max` grid plus Stage-B 1-D knob sweeps (each FLAT ->
  named constant, or SENSITIVE -> keep as a setting), with BOTH a `micro` and a serial-`grouped`
  baseline line and predicted-session-count honesty columns.
- `stability` re-proves the freeze-lookback window (identity-partitioned boundaries: an identity swap
  at the same instant counts as drift).
- Group flags (proposed `sessions.*` names; PR4 owns the finals): `--merge-gap` `--absorb-max`
  `--meeting-gap` `--focus-min-len` `--focus-density`. Seg flags: `--gap-close` `--min-len`.
  Scoring: `--tolerance`. An unknown subcommand exits non-zero.

The approved D9 thresholds + chosen parameters land in `specs/05`/`06` (they are PR4's binding merge
gate). The exported sample and labels are never committed; specs/PR carry aggregate numbers only.

### PR4 production gate and live checks

1. **CI-runnable parity:** `cargo test -p harness --test shipped_parity`. The table-driven fixtures
   cover interleaved tools, short None absorption, meeting overlap, merge-gap equality, focus density,
   AI qualification, open projection, renamed `app_hint=chatgpt,title=Codex`, and excluded
   `ChatGPT Classic`. Boundaries/identity/host and first/last metadata match harness-concurrent;
   production additionally owns pass-1-consumed excursion frame ids, which the frozen referee only
   counted as absorbed time.
2. **Binding D9 re-run:** prepare an input directory containing only `2026-07-07`, `2026-07-08`,
   and held-out `2026-07-09` (do not include the 07-10 capture-limit demonstrator), then run:
   `cargo run -p harness -- score --algo shipped --data <three-day-dir>`, followed by the same command
   with `--algo micro` and `--algo grouped`. With no explicit tolerance, each command prints both
   ±120 s and ±180 s. Gate criteria are verbatim in `06` #26; a miss stops the PR with no retuning.
3. **D5 backup before live launch:**
   `cargo run -p harness -- backup --to <outside-repo-and-app-data-dir>`. Confirm the printed
   integrity check and frame/mark count parity, then print the backup's full path, byte size, and
   mtime. Only after this gate may `npm run dev` open the live schema-11 DB. Never launch the debug
   executable directly.
4. **Historical/incremental observation:** keep capture running while logs show
   `sessions historical backfill advanced` (`cursor_ms` increasing toward `target_ms`). Query
   `sessions`, `frames.session_id`, and `session_artifacts` to confirm old rows appear, open/recent ids
   reconcile stably, and new frames continue arriving. A segmenter error must log and leave capture
   active.
5. **Recognition + D8 qualitative check (maintainer in the loop):** foreground a real Claude Code
   session, Codex desktop session, browser-AI page, and meeting-titled window long enough to pass the
   frozen floors; inspect `kind/tool/host/context_key`. For an AI row, inspect `kind='exchange'`
   artifacts: explicit user/agent markers may produce rows; no marker must produce none, never an
   invented role.
6. **D10 regression spot-check:** exercise where-was-i, frame search, Ask, Timeline, and marks while
   capture continues. PR4 adds no commands, no NavRail route, no audio, and no notification surface.

## Native acceptance — sessions UI (0.4.0 PR5)

This is a **real Tauri/WebView2 acceptance run**, not a Vite-browser substitute. Start only from the
repo root with `npm run dev`; never launch the debug executable or use `cargo tauri dev`. Before
opening the PR5 build against the live database, confirm the D5 backup described above still exists
outside both the repo and the app-data directory. Use real schema-11 session data. A local answer
model must be ready for the Recap checks.

Record the date, Windows build, WebView2 version, monitor resolution/DPI, database frame/session
counts, and screenshots or screen recording for every observed result. Do not mark a row accepted
when the required real-data state is absent; record that state as **not available** and carry it to
the PR7 audit.

### Navigation and data truth

1. Open Timeline on a range containing multiple session kinds and overlapping sessions. Confirm the
   density ribbon remains visible, session bands use the existing neutral/ok/warn token vocabulary,
   every visible band has a usable native-button hit target, and the band layer is exactly four lanes.
   Use a fixture/range with at least five simultaneous collisions: confirm no fifth row appears, the
   omitted bands are counted by the neutral “more sessions — narrow the range” keyboard control, and
   activating that control focuses the existing range presets. Repeat during initial route skeleton,
   loading, error, empty, and populated; all five states must reserve identical four-lane geometry
   with no clipping, nested scrolling, horizontal scrolling, or layout shift.
2. With pointer input, follow **band → session drill-in → representative Moment → back**. Confirm
   Back returns to the same session drill-in and the drill-in remains on the same Timeline context.
   Open a Moment directly and use **Part of session**; confirm the link is omitted for a frame with
   no session. Also open the session URL directly and confirm its back action returns to Timeline.
3. In the drill-in, compare the displayed absolute span, kind/tool/host, confidence, frame total,
   representative first/last frames, and exchanges against the real session row/artifacts. Confirm
   exchange links open the cited Moment and no exchange roles are invented when artifacts are absent.
4. Generate **Recap** with the answer model ready. Confirm progress is visible, the result uses the
   existing report rendering/footer, every citation opens a Moment owned by this exact session, and
   no frame from an overlapping session leaks into the citations. Run it again and press **Cancel**;
   confirm generation stops and no late result replaces the idle state. Repeat once while navigating
   away mid-generation to confirm route teardown also cancels backend work.
5. Open Deck and confirm the existing Jump back action still opens its Moment while the optional
   session span truthfully frames it. In Advanced Settings, confirm the collapsed **Sessions** group
   contains only `sessions.min_len_secs` (30–3600) and `sessions.gap_close_secs` (60–3600), saves
   normally, and adds no NavRail item.

### Keyboard, state, and layout matrix

6. Keyboard-only: focus the Timeline slider and use its existing arrow/Enter behavior; then Tab to
   session bands, move through every band in DOM order, press Enter/Space to open one, traverse every
   drill-in action/link, open a Moment, and return. Confirm visible focus throughout and no interactive
   button is nested inside the ARIA slider.
7. Observe each real-data state that is available: loading skeletons; list/detail error + Retry;
   frames with **no sessions** (density ribbon remains usable); session with **no exchanges**; **open**
   session (still running/non-final boundary); **low-confidence** session (numeric confidence shown);
   missing/deleted session; summary failure/retry; Recap failure/retry. Do not synthesize database
   rows or mock IPC responses merely to make a state appear tested.
8. At 1280×720, 1920×1080, and 3440×1440, check 100%, 125%, and 150% Windows display scaling where
   the hardware permits. Cover Timeline, session drill-in, affected Moment, and Deck. At each point
   confirm fixed rails, exactly one vertical route scroll context, no route-level horizontal
   scrollbar, no nested vertical scroller, wrapped frame/citation/exchange strips, no clipped band
   lanes, and no cumulative layout shift when session/summary/Recap data arrives. Repeat with Windows
   reduced motion enabled and confirm session-band/scan ambient motion is absent.
9. Keep Timeline mounted while a real scheduler pass commits new/reconciled session rows. Confirm the
   Tauri `sessions_changed` event payload is `null`, the mounted session list refetches without route
   navigation or manual reload, and the new bands/ownership become visible. Confirm the signal creates
   no toast, notification, badge, nudge, or score. Repeat an unchanged pass and confirm it emits no
   event; repeat with a failed scheduler pass and confirm no false success is claimed (a failed
   multi-write pass may conservatively invalidate because an earlier write may already have
   committed).

### PR5 evidence status

**Observed 2026-07-10 — native acceptance passed, with the open-session variant unavailable in the
current live dataset.** The run used `npm run dev` against the live schema-11 database, with the
worktree's `target/debug/screensearch.exe` process and a real Tauri WebView
(`window.__TAURI_INTERNALS__ === true`), not browser mocks. The live status showed 515 captures / 514
tagged. Evidence:

- Deck rendered `JUMP BACK ... until 17:08 session 16:44–17:14`, preserving the existing action while
  adding truthful session framing.
- Timeline rendered eight real overlapping sessions in two lanes. At 1280×720 DPR 1, 1920×1080 DPR
  1, and emulated 3440×1440 DPR 1.5, document width equalled viewport width; every band was at least
  32×32; band overlaps, nested vertical scroll contexts, and horizontal overflow were all zero.
  Accessible band labels included kind, full date/time, and tool/host.
- Keyboard focus on the first band followed by CDP native `rawKeyDown` / `char` / `keyUp` Enter
  opened `/timeline/session/3`. The real round trip was AI band → session 3 → representative frame
  `/timeline/2651`; Moment showed back label `SESSION` and `PART OF SESSION ScreenSearch Workflow`,
  and SESSION returned exactly to `/timeline/session/3`.
- Session 3 showed AI · codex/desktop · Confidence 78.5% · Jul 10 01:21–02:50 · 41 frames / 24
  representatives, plus the honest `No exchanges captured for this session.` state. It had one
  vertical scroller and no horizontal overflow.
- Recap cancellation was observed after `Summarizing 1 of 4 · 1/4`: clicking CANCEL returned to
  GENERATE RECAP and no stale result appeared. A clean Recap completed in 15 seconds with 5 passes,
  1/1 periods, 39/39 frames summarized, and the truthful trimmed footer. All 39
  `report.cited_frame_ids` were read back through the live `get_frame` command; every frame resolved
  with `session.id === 3`.
- Live session 21 provided the low-confidence state: neutral Confidence 47.2%, 50 frames, no invented
  exchanges, one scroll context, and no horizontal overflow. A live `list_sessions` query returned
  21 sessions and open IDs `[]`; therefore the open-session visual variant was **unavailable in this
  dataset**, not passed or failed. Its implementation/test coverage remains, but no live observation
  is claimed.
- With CDP emulating `prefers-reduced-motion: reduce`, the media query matched and computed animated
  elements were `[]`. The Sessions expander showed Minimum session length 120 (min 30 / max 3600)
  and Close gap 300 (min 60 / max 3600), with labels explaining the next session pass.
- The only console/network noise was the existing `favicon.ico` 404 and an informational WebView
  lazy-image intervention; no app runtime errors were observed. The screenshot remains outside the
  repo at `%TEMP%/screensearch-pr5-timeline.png` and is not committed.

**Observed 2026-07-11 — final review-fix acceptance at code `02e5cad` / docs `67b76ce`.** The real
`npm run dev` process was again this worktree's `target/debug/screensearch.exe`, with
`window.__TAURI_INTERNALS__ === true`. Through the real typed listener in
`/src/lib/ipc/events.ts`, a startup scheduler pass produced `sessions_changed` probe `{count:1}` and
no toast or notification. Fixed geometry was then observed at 1280 px:

- Forced initial loading through the existing dev-state seam: grid
  `32px 32px 32px 32px`, grid height 140, session outer height 192, and five skeleton elements.
- Live empty Today: the same 140/192 grid/outer geometry and no horizontal overflow.
- Live populated 7-day: the same 140/192 geometry, 21 visible bands, zero band overlaps, and document
  width 1280 equal to viewport width 1280.
- Live populated dense 30-day: the same 140/192 geometry, 12 visible bands, neutral
  `9 more sessions — narrow the range`, zero overlaps, and document width 1280 equal to viewport
  width 1280. The overflow button was keyboard-focusable. CDP native
  `rawKeyDown` / `char` / `keyUp` Enter moved focus to `TODAY`, whose parent `aria-label` was
  `Time range`.

These observations close the focused dense-overflow, fixed-state-geometry, keyboard-focus, and live
scheduler-refresh steps above. No schema, API/MCP, or frame behavior changed.

**Final clean suite (Pass 12, 2026-07-11):** the required UI-first sequence passed at tip `8629e0c`,
with the additional `npm run test` session-band regression gate between `npm ci` and lint. Full
color-disabled raw output—including the 1,084-line workspace test log and empty fmt/binding-guard
outputs—is preserved verbatim in `specs/05_BUILD_REVIEW.md` Pass 12. The npm allow-scripts warning
was non-failing.

**PR #104 review follow-up (Pass 13, 2026-07-11):** all four unresolved inline threads were
applicable and are fixed in code `c360d7e`; two store threads described the same unbounded content
scan. Focused RED/GREEN evidence is preserved in `specs/05_BUILD_REVIEW.md` Pass 13. The store query
now stops in SQLite and exactly preserves Rust Unicode trim behavior; Timeline loading/error/empty/
populated all reserve four lanes (the forced error/loading render measured four 31.9965 px rows,
139.9653 px total, 1704 px document/viewport parity, and no nested scrollers); and a repeated
unchanged scheduler pass emits no `sessions_changed` event or redundant ownership/artifact writes.
This is focused review verification only; the post-review full UI-first suite is recorded separately
after it runs.

**Post-review final clean suite (Pass 14, 2026-07-11):** the required color-disabled UI-first
sequence passed after code `c360d7e` and review documentation `3f62479`: `npm ci` → `npm run test`
→ lint → UI build → MCP staging → fmt → workspace clippy/build/test → generated-binding guard. All
ten commands exited 0; the npm allow-scripts warning was non-failing. The ten fenced blocks in
`specs/05_BUILD_REVIEW.md` Pass 14 were compared directly with their captured logs; all matched,
including the complete 1,091-line workspace-test log and the empty fmt/binding-guard logs.

The automated UI-first verification below this acceptance record remains the build/test evidence;
the native observations above complete `03 §13c-5` without substituting mocks for the live app.

## Sessions API + MCP acceptance (0.4.0 PR6)

PR6 exposes the sessions surface over the local HTTP API and the MCP wrapper. It is **read-only**
and adds no schema change (the sessions tables shipped in PR3). All of the automated tests below
run in the normal `cargo test --workspace` gate against synthetic fixtures / tempfile SQLite; no
test touches `%APPDATA%` or real screen history.

### Automated tests (CI-safe, no real data)

- **`crates/api` fixture tests** (`crates/api/tests/http_api.rs`, extended): `GET /v1/sessions`
  ordering (`started_at DESC, id DESC`) and the overlap predicate
  (`started_at < to AND COALESCE(ended_at, now) > from`, so an open or before-`from` session that
  spans the window stays visible and `from`/`to` are each independently optional); validation
  (unknown `kind` → 400 naming the valid values, `from` > `to` → 400, non-integer id → 400,
  unknown id → 404); `GET /v1/sessions/{id}` detail shape (`{ session, exchanges }`, only
  `kind = exchange` artifacts returned); summary visibility (`include_summary=1` reveals the
  **cached** `summary`/`summary_model`, any other value serves them as `null`); and the **D12
  never-generate** guarantee, where a GET with `include_summary=1` on a session with no cached
  summary starts no inference and writes no row (the summary stays `null`, the row is unchanged).
- **`crates/store` scoped-search tests**: the session-scoped hybrid search restricts retrieval to a
  session's **own** frames, so a query answered under `session_id` returns and cites only in-session
  frames even when a concurrent session overlaps in wall-clock time (ownership is exclusive, not a
  time window).
- **`crates/mcp` session-tool round-trips** (`crates/mcp/tests/stdio_mcp.rs`, extended): `tools/list`
  now advertises **nine** tools; `list_sessions` / `get_session` / `ask_session` round-trip over the
  stdio wrapper against a fixture API (including the empty-result "No sessions." reply and the
  `include_summary` default of `false`), and the API-off / wrong-token states still return the guided
  errors rather than crashing.

### Live MCP round-trip (maintainer in the loop)

A real end-to-end check that the three tools drive the running app, with the citation-ownership
invariant confirmed against the live database.

1. Start the app from the repo root with `npm run dev` (never the debug executable, never
   `cargo tauri dev`). In **Settings → Local API**, toggle it on and **copy the bearer token**; in
   the model panel, **load an answer model** (`ask_session` needs it ready).
2. Run the staged binary against the live API, passing both values by environment:
   `SCREENSEARCH_API_URL=http://127.0.0.1:43210` and `SCREENSEARCH_API_TOKEN=<copied token>`, then
   launch `target/debug/screensearch-mcp.exe`.
3. Drive `tools/list` and confirm it lists **nine** tools (the six prior tools plus `list_sessions`,
   `get_session`, `ask_session`).
4. Call `list_sessions` with `kind=ai`, `tool=claude-code`; confirm the returned sessions are
   AI / Claude Code and ordered most-recent first. Take one session `id`.
5. Call `get_session` for that id **without** `include_summary` (summary/summary_model come back
   `null`), then **with** `include_summary=true` (a cached summary appears only if the app already
   generated one, which the tool never triggers); confirm the exchange artifacts match the
   session.
6. Call `ask_session` with that `session_id` and a query. For every cited frame id, confirm it
   belongs to the session with a read-only lookup:
   `SELECT session_id FROM frames WHERE id = ?` must return the same session id for each citation,
   proving no frame from an overlapping session leaked into the answer.

Record the date, Windows build, WebView2 version, and the observed session/frame ids and citation
ownership results, the same way the PR5 acceptance run above does.

## D16 first-auto-delivery gate — live update path (0.4.0 PR7)

This is the release-blocking gate for v0.4.0 (decision D16, `docs/0.4.0.md`). v0.4.0 is the first
release existing installs receive automatically, so before the tag we prove the whole update path
on a real 0.3.2-or-0.3.3 install: **auto-update check → background download → user-initiated
restart → first boot runs the 10 → 11 migration → sessions appear over pre-existing history → the
frame-level features are unchanged.** A failure anywhere here is a stop-and-report; do not tag.

The installed app's updater endpoint is baked to `releases/latest/download/latest.json`, which
only resolves for a published, non-prerelease GitHub release. Because the real v0.4.0 tag comes
after this gate, the gate is served by a **temporary** full release under a non-`v*` tag
(`pre-0.4.0`), built and signed locally, published just before the run and deleted right after.
This mirrors the 0.3.2 PR2 update E2E; the difference is that the manifest advertises the real
`0.4.0` version (so the installed 0.3.3 sees a strictly-newer update) while the assets live under
the temporary tag.

**Do not run `npm run dev`, `cargo tauri dev`, or any repo-built executable against the live data
directory during this gate.** A dev build shares the Roaming data dir and would migrate the live
DB to schema 11 out of band, destroying the test bed. Every DB read below is `sqlite3 -readonly`,
which is safe against the running app.

Paths on the maintainer machine: live DB `%APPDATA%/app.screensearchv2c.desktop/screensearch.db`;
logs `%APPDATA%/app.screensearchv2c.desktop/logs/screensearch.log.YYYY-MM-DD` (daily-rolling, the
suffix date is UTC, so an evening run can span two files; tail with
`Get-Content <file> -Wait -Tail 50`); installed exe
`%LOCALAPPDATA%/ScreenSearch/ScreenSearch.exe`.

### Precondition — D5 backup verified present

Before the run, confirm a dated copy of the live `screensearch.db` exists outside the repo and
outside `%APPDATA%` (decision D5, release-blocker-class). Verify integrity and that it predates the
migration:

```
sqlite3 -readonly "<backup>.db" "PRAGMA integrity_check; SELECT version FROM schema_version;"
```

Expect `ok` and `10`. If the intended D16 history bed is a restored historical backup, it must read
schema **10**; a schema-11 file cannot be restored under 0.3.3 (the installed app rejects a newer
schema). Take one more fresh dated backup of the actual bed with
`cargo run -p harness -- backup --to <dir outside repo and appdata>` and keep both.

### BEFORE — the running 0.3.3 install, temp release published

Capture the baseline so the after-state is comparable:

1. Version: `(Get-Item "$env:LOCALAPPDATA/ScreenSearch/ScreenSearch.exe").VersionInfo.ProductVersion`
   is `0.3.3`.
2. Schema: `SELECT version FROM schema_version;` is `10`; `SELECT COUNT(*) FROM sqlite_master WHERE
   name IN ('sessions','session_artifacts');` is `0`.
3. History baseline: `SELECT COUNT(*), MIN(captured_at), MAX(captured_at) FROM frames;` and
   `SELECT COUNT(*) FROM marks;` — record verbatim.
4. Frame-level baseline via the local API (enable **Settings → Local API**; the token persists in
   the DB and survives the update): `GET /v1/health` (`"version":"0.3.3"`, `capturing:true`) and
   `GET /v1/search?q=<a known term from the historical data>` — record the top result frame ids
   verbatim for a byte-comparable after-check.
5. Trigger: the user clicks **Check for updates** (Settings App section, NavRail footer, or tray).
   Expected: the UI shows an available `0.4.0` update; the log shows the background download begin
   and then a signature-verified download whose byte count equals the installer size; a
   restart-to-update affordance appears. No update should ever install without the user's click.

### THE UPDATE — user-initiated restart

The user clicks restart (the only install trigger). Expected: a log line recording the install on
user-initiated restart, a graceful shutdown, the passive NSIS installer running, and the app
relaunching on its own.

### AFTER — first boot of 0.4.0

1. Version: exe ProductVersion is `0.4.0`; `GET /v1/health` reports `"version":"0.4.0"` using the
   **same** token as before.
2. Migration (**gate**): `SELECT version FROM schema_version;` is `11`; both `sessions` and
   `session_artifacts` tables exist; the day's log carries the migration lines. A failure here is
   stop-and-report — do not tag. Rollback: quit the app, delete
   `screensearch.db`/`-wal`/`-shm`, copy the D5 backup back, reinstall 0.3.3 from its GitHub
   release.
3. Frames untouched (D10): re-run the BEFORE queries. The frame count is greater than or equal to
   the baseline (capture resumed), `MIN(captured_at)` is unchanged, the marks count is identical,
   and the same `/v1/search` query returns the same top frame ids.
4. Sessions over pre-existing history (**gate**): within roughly 60 s of boot and continuing, the
   log shows `sessions historical backfill advanced` with `cursor_ms` climbing toward `target_ms`
   and no `sessions ... failed; capture remains active` warnings; `SELECT COUNT(*) FROM sessions;`
   grows across polls; `SELECT COUNT(*) FROM sessions WHERE started_at < <update-epoch-ms>;` is
   greater than zero (sessions strictly over history that predates the update); `SELECT value FROM
   settings WHERE key LIKE 'sessions.%';` shows the `backfill_done_until` checkpoint advancing; and
   the Timeline on a backfilled historical date shows session bands, with one drill-in opening
   truthfully against its real row. Do **not** gate the tag on backfill completion (months of
   history at 6 h per chunk per 60 s tick takes hours and yields to capture by design); gate on a
   clean migration, an advancing scheduler, historical sessions visible, and frame features
   unchanged. Confirm completion (`cursor_ms == target_ms`) opportunistically before the PR opens.
5. Idle re-check: **Check for updates** on the 0.4.0 install reports no update (the temp manifest's
   `0.4.0` equals the running version).

Record in the build log (`specs/05`): timestamps, verbatim query outputs, verbatim log lines, and
the installer byte-count match. After the gate and the remaining live acceptance, delete the temp
release (`gh release delete pre-0.4.0 --yes --cleanup-tag`) and confirm the endpoint reverts to
`0.3.3`.

## Daily memory C–E: native acceptance (not yet certified)

Use a separate, explicitly selected test profile/folder and an installed local answer model.
Preserve the ordinary installation and take a dated database backup before the schema 11 → 12
upgrade. Do not enable Telegram or use private history merely to satisfy a test command.

1. Inspect defaults after migration: schedules and Telegram off; no timezone, folder or recipient
   inferred; existing frame/session/settings counts preserved. Open Settings → Memory reports
   and verify loading, empty, error and populated states without additional scroll containers.
2. Configure a test folder and IANA zone, inspect backend schedule preview, run a previous-day
   report over approved evidence. Compare saved frame IDs/counts with the actual summarization
   inputs, inspect model label and coverage, and distinguish sample coverage from total capture.
3. Verify successful empty inspection gives NO_DATA without loading a model. Missing installed
   model must block with a retry action; a read/inference failure must not produce NO_DATA.
4. Open the real Markdown note and index in the selected folder/Obsidian. Inspect typed
   frontmatter and dated headings. Edit the note, retry/regenerate, and confirm conflict plus
   byte preservation. Exercise unavailable folder, junction/reparse path and interrupted write.
5. Restart around enqueue/result-save/file-write boundaries; saved output must be reused and
   attempts reconciled. Verify the seven-day catch-up cap, visible older gap, DST overlap/gap,
   timezone/time changes, disabled-period exclusion and shutdown before sidecar destruction.
6. Only with an approved destination and explicit test-send authorization, save a Telegram
   token using the write-only field. Inspect the actual received message and recorded message
   IDs, partial label, Unicode chunk boundaries and masking limits. Never paste tokens or raw
   private report content into verification logs. Confirm disable/config-change suppression,
   confirmed ancestor suppression, and explicit reviewed/resend actions for uncertain attempts.
7. Record source commit, model, environment, commands/raw output and the checks actually observed.
   A service acknowledgement proves acceptance only. Keep unperformed checks open; the F1 pilot
   and F2 release decision require varied supported setups and consenting participants.


## Manual Memory report dates and ranges (2026-10-15)

Use a consented test profile and selected folder. Verify a single date and an inclusive multi-day
range in the saved timezone, invalid/reversed/future date errors, and a today-ending range with
its frozen enqueue-time cutoff and explicit PARTIAL/as-of description. The previous-day shortcut
must display its target date and retain its original behavior; selecting dates alone enqueues nothing.
Daily scheduling activation/catch-up remains unchanged. Genuine empty windows need no model,
write NO_DATA notes and skip Telegram with attempt 0. A positive evidence probe followed by blank
model output must fail generation instead of claiming NO_DATA or writing a header-only report.

Inspect separate Reports files for independent selected-date runs, regenerated revision reuse
of the original unchanged owned filename, preserved edited content and Daily notes, and correct
range links in the common index. Old schema-1 frontmatter without requested_range/base_run_id and
old run JSON without requested_range must still load. Persistence tests cover immutable UTC
windows/request metadata and legacy previous-day deduplication. No SQL migration is required.

With an explicitly authorized test recipient/send, inspect the Telegram date-range/PARTIAL text
and per-chunk receipts. Automated digest-rendering tests do not prove a live message arrived.
Do not reset live schedules or directly edit durable run/delivery rows to manufacture acceptance.
