# 05 — Build Review

## 2026-11-04: QA pass on P2, P3 and P4 (branches `feat/0.4.5-p3-baseline-schema`, `feat/0.4.5-p4-auto-eval`)

- **Method.** Each branch was verified on its own (not only the combined tree), an independent
  read-only reviewer audited the P3 and P4 diff against the kickoffs and `03 §4`/`§8`, and every
  finding was reproduced in code before it was fixed. Every code fix has a regression test that
  was **mutation-checked**: the old behaviour was put back and the test was confirmed to fail,
  then the fix was restored and confirmed byte-identical.
- **Found by testing the P3 branch alone (merge blocker).** `node scripts/stage-mcp.mjs` failed
  on P3 with `cannot update the lock file ... because --locked was passed`. The P3 commit's
  `Cargo.lock` had been written while P4's harness work shared the tree, so it listed five
  harness dependencies P3 does not use. It passed only on the combined tree, which is why the
  overnight run missed it. Fixed on P3 (`d68a6f5`); P4 re-adds its own edges (`40dee0a`).
- **Blocker (review, verified).** A Settings save could undo an API change made in the HTTP API
  panel and stop the listener: `api.enabled/port/bind_address` became typed settings that
  `set_settings` hot-applies, but the Settings draft never refreshed them and `set_api_config`
  never invalidated the `settings` query, so a stale `api_enabled=false` was written back. Fixed:
  the draft mirrors `api_*` like it already mirrored `app_*`, `set_api_config` invalidates
  `settings`, `set_settings` invalidates `apiStatus`. **Not run live:** the app has no data-dir
  override, so running it would open the real profile, which the run rules forbid. The behavioural
  proof is the maintainer's native check (enable the API in Integrations, change any other
  setting, confirm the listener stays up).
- **Should-fix, all fixed (`b431bc8`, `cc1d916`):**
  - `ssv2c-rebuild` dropped five live internal markers. Losing `sessions.backfill_done_until`
    re-segments the carried history. Now carried, plus a drift-guard test that scans the shipped
    source for `settings` key constants and fails on any key the rebuild neither carries nor drops
    on purpose. `text.catalog_filter_version` is dropped on purpose: it watermarks
    `chrome_text_catalog`, which is not carried.
  - The allocator test could not tell copy from recompute (fixture high-water 7 = max frame id 7).
    Now 42; a recompute mutant fails with `source Some("42"), destination Some("7")`.
  - Store open decided fresh vs existing by `path.exists()`, so a first run that failed mid-baseline
    left an empty file refused forever, and the check raced a file appearing. It now decides by
    contents: zero schema objects is initialised, anything else takes the strict v14 refusal, a
    non-SQLite file is refused untouched. A simulated crashed first run now recovers to v14.
  - `advanced.toml` rewrite is atomic (temp file then rename).
  - Four P4 metrics did not match their printed definitions: `sequence` ties, `window_recall`
    expecting out-of-window frames, recall@10 never reaching sessions, and the seen-again probe
    varying with `--pairs`. Fixed, report text updated to match.
- **Accepted, not changed:** the reviewer's nits on vacuous older kernel tests, the rebuild's
  image check resolving paths against the copied DB's folder, and harness edge cases (empty
  terminal citations event, non-ASCII needles). Recorded here for P5/P6; none changes a result
  tonight's fixture or the maintainer's run depends on.
- **P2 deliverables.** All four HTML files parse with zero structural errors and no duplicate ids,
  make no network request, and the three directions use no raw colour or px font size outside their
  token blocks. Contrast was **computed from the token values**, not read from the files' own
  comments: every text-on-background pair the CSS actually renders, and every text token on every
  page background, clears WCAG AA 4.5:1 in every theme of every direction (lowest 4.52, direction A
  light, faint text on the document background).
- **Final gate, P4 tip (contains all P3 fixes), raw:**
  ```
  === npm ci                                                             exit=0
  === npm run lint                                                       exit=0
  === npm run build                                                      exit=0
  === npm test               tests 82 / pass 82 / fail 0                 exit=0
  === node scripts/stage-mcp.mjs                                         exit=0
  === cargo fmt --all -- --check                                         exit=0
  === cargo clippy --locked --workspace --all-targets -- -D warnings     exit=0
  === cargo build --locked --workspace                                   exit=0
  === cargo test --locked --workspace                                    exit=0
  === git diff --exit-code -- ui/src/bindings                            exit=0
  passed: 1241  failed: 0
  ```
  1241 = the earlier 1233 plus 8 new regression tests. The P3 branch alone was also run green.

## 2026-11-03: 0.4.5 P4 automatic evaluation harness (branch `feat/0.4.5-p4-auto-eval`)

- **Scope (roadmap P4, kickoff `docs/kickoffs/P4.md`):** dev-only eval runner in `crates/harness`.
  Five self-generated families (last-seen, involvement, sequence, window recall, seen-again),
  unlabeled entity check, six metrics. **No hand-written question set, no labelling, no golden
  file** (0.4.5 D17). `D17` is enforced by construction: the only inputs are the fixture database
  and a seed.
- **Metrics defined in code and printed in the report:** evidence recall@10 over cited ids (the
  only evidence a client sees, `03 §7i.3`), a separate `/v1/search` retrieval recall@10 diagnostic
  that is never mixed in, citation validity (frame exists and captured in window; session exists
  and overlaps the window, NULL end treated as open), known-fact presence after case and
  whitespace normalization, latency p50/p95 by nearest rank, entity recovery and invention with
  `n/a (no entity surface)` when the graph is empty.
- **Fixture run (not the baseline):** 87 pairs from a 12 session, 36 frame fixture. `pairs.jsonl`,
  `runs.jsonl`, `report.md`, `report.json`. A fixture yields fewer pairs than real history, and the
  report prints a pair-shortfall note rather than inventing pairs.
- **Verification (branch tip, raw):**
  ```
  cargo fmt --all -- --check                                       exit=0
  cargo clippy -p harness --all-targets -- -D warnings              exit=0
  cargo test -p harness   146 passed / 1 / 3 / 6 / 6 / 3 / 3 passed, 0 failed
  ```
  Read-only proof: `PRAGMA query_only=1`, INSERT and CREATE TABLE fail, reads succeed, the file is
  byte identical afterwards, with a writable negative control on the same file. Determinism:
  same database and seed produce identical pairs; a different seed does not; editing a frame
  changes the derived pair.
- **Maintainer run (not performed by an agent):**
  `cargo run -p harness --bin harness -- eval --db <db> --api http://127.0.0.1:43210 --out harness-data/eval-baseline`
- **Not claimed:** the baseline report against real history (maintainer's run), and the P5/P6
  thresholds, which freeze from that report at Gate A.

## 2026-11-03: 0.4.5 P3 baseline schema and settings v2 (branch `feat/0.4.5-p3-baseline-schema`)

- **Scope (roadmap P3, kickoff `docs/kickoffs/P3.md`):** `crates/store` baseline v14 with the
  v1 to v13 chain deleted, the dev-only `crates/rebuild` tool (D3), settings v2 in
  `crates/traits`/`crates/kernel`/`src-tauri` (D6), and the regenerated bindings.
- **Baseline fidelity.** The chain was dumped before deletion: a throwaway test opened an
  in-memory database through the old migrations and wrote `sqlite_master` to `target/v13_dump.sql`,
  then a second throwaway harness rebuilt v13 from that dump and compared it with `create_baseline`
  table by table (`PRAGMA table_info`, `foreign_key_list`, `index_list`, index and trigger SQL).
  Result: `parity OK: 41 v13 objects preserved`, `missing (0)`, `added (19)` = the eleven new tables
  and eight indexes. 31 tables (27 plain plus 4 virtual), 22 indexes, 7 triggers.
- **Open refusal.** A database that is not v14 fails before any pragma runs, so a refused file is
  never header rewritten; the message names the found version and `ssv2c-rebuild`. Tests assert the
  file bytes and mtime are unchanged after a refusal, for a v13 and a v15 fixture.
- **Settings v2 counts (grep, not assertion):** 25 UI rows = 24 typed fields plus `api.token`;
  44 file-only = 43 `AdvancedConfig` knobs plus `app.tray_toast_done`; 9 deleted. Retired-key grep
  hits only the retired list, its doc comments, the retirement test, the rebuild tool's own copy of
  the retired list and fixtures, and two `uia` doc comments that explain the retirement.
- **Deleted-key and moved-key greps (raw):**
  ```
  grep -rn MIGRATIONS crates                                    -> 0 lines
  grep -rn ssv2c-rebuild crates/store/src                       -> 4 lines
  grep -rn advanced.toml crates                                 -> 28 lines
  capture_diff_threshold / sidecar_device / enrich_embed_text /
    overlay_max_results in crates/traits/src/ipc.rs             -> 0 each
  ```
- **Per-crate gates (raw, from the lanes' logs):**
  ```
  cargo fmt --all -- --check                                    exit=0
  cargo clippy -p store --all-targets -- -D warnings             exit=0
  cargo test -p store        171 passed (80 lib + 6 baseline + 65 store + others), 0 failed
  cargo test -p rebuild      4 + 5 passed, 0 failed
  cargo test -p traits       123 + 4 passed, 0 failed
  cargo test -p kernel       126 + 9 + 17 + 3 + 31 + 10 + 2 passed, 0 failed
  cargo test -p harness      146 + 1 + 3 + 6 + 6 + 3 + 3 passed, 0 failed
  ```
- **Full suite on the integrated tree (verbatim, one command per line):**
  ```
  === npm ci                                                   exit=0
  === npm run lint                                             exit=0
  === npm run build                                            exit=0
  === node scripts/stage-mcp.mjs                               exit=0
  === cargo fmt --all -- --check                               exit=0
  === cargo clippy --workspace --all-targets -- -D warnings     exit=0
  === cargo build --workspace                                  exit=0
  === cargo test --workspace                                   exit=0
  ```
  `cargo test --workspace` summary: **1233 passed, 0 failed**; the four `1 ignored` are the
  pre-existing `perf` and `seed_demo` gates. Full log kept at `%TEMP%\p3-full-verify.txt`.
- **UI build was red mid-phase and was repaired.** Removing 44 file-only and 9 deleted keys from
  the typed `Settings` broke `Settings.tsx`, `ScheduleControl.tsx` and `FlowOverlay.tsx` (157
  type errors). Repair: 40 controls removed from `Settings.tsx` (that is the D6 subtraction P7 will
  re-home by category), 3 from `ScheduleControl.tsx`, and the overlay now reads `overlay.max_results`
  from a new read-only `get_advanced_config` projection instead of a removed field. No hardcoded
  fallback. `npm test` 82 passed.
- **Not claimed:** the rebuild run against the maintainer's real database, the per-table counts
  before and after, and the twenty frame spot check. Those are the maintainer run in
  `docs/kickoffs/P3.md`; the agent evidence is the synthetic fixture test only.
- **Gaps recorded:** `06` #30 (the spec's §4 does not describe ten as-built details), #31 (the
  rebuild tool's home reconciled from `crates/harness` to `crates/rebuild`), #32 (carry set wider
  than the kickoff list), #33 (three §8 defaults disagree with the as-built code). `07` carries the
  product questions, including the `advanced.toml` reset for an existing profile.

## 2026-11-03: 0.4.5 P1 specs contract (branch `feat/0.4.5-p1-specs-contract`, specs-only)

- **Scope (roadmap `docs/0.4.5.md` P1):** zero code. D1–D18 normalized into the specs so a fresh
  agent can start any wave-A phase (P2/P3/P4) from `specs/` alone. Landing sites: `01 §2/§6/§7/§8`
  (single user, baseline schema, rebuild tool, local-only, next arc); `02 §5k` (arc), `§5d`/`§7`
  (audio not a priority); `03 §4` (baseline rule at `schema_version = 14`, 11 new tables, migration
  prose kept as history), `§7c`/`§7f.7` (4 read routes, 4 MCP tools), new `§7h`/`§7i`/`§7j` (work
  graph, Ask v2, insights), `§8` (settings v2 table, load rule), `§12` (schema failure mode), new
  `§13d` (gates + per-phase DoD); `04 §1/§2/§3/§4` (0.4.5 pointer, ownership, D2/D18 guardrails);
  `UI_REFERENCE §1/§2/§5` (placeholders filled at P2), `§3` (v2 IA), `§4` (12 state rows), `§7`,
  `§8` (v2 shell contract), `§10.7`; v1 identity archived verbatim to
  `specs/archive/UI_REFERENCE_v1_identity.md`; `07` (0.4.5 section, #75, #82, #104); `CLAUDE.md`
  hard rule; one-line amendments in `docs/ARCHITECTURE.md` and `docs/memory-api-direction-plan.md`.
- **Implemented by four disjoint writer lanes** (`03` core sections, `03 §8`, `UI_REFERENCE`, the
  other specs/docs), integrated by the parent, which also fixed survivors the lanes did not own:
  `03 §12` still stated forward-only migrations as the current rule, `02` closing status still said
  "no next arc", `UI_REFERENCE §7` still said "dark-only by design", `07` #82/#104 pointed forward
  at the superseded rules.
- **`03 §8` conversion:** 78 key rows — 25 `UI` (the closed list, each with a live consumer;
  `api.read_token` shares the `api.token` row), 44 `file-only` (live consumer, engine tuning), 9 `deleted` (hits only in the
  `RETIRED_SETTINGS_KEYS` sweep/tests/doc comments, or none). Two backticked tokens in the old list
  were not keys (`frames.capture_trigger`, `set_api_config.bind_address`) and stay in prose.
- **Gates 1–2 (raw output goes in the PR body):** scope — 13 tracked files modified plus the new
  archive file and `docs/0.4.5.md`, every changed path `*.md`, no `crates/`/`ui/`/`src-tauri/`
  path; UI rows `25`; tolerance grep over `03` empty (`§8b` byte-identical to `main`); `D1`–`D18`
  each cited ≥1 file; `CREATE (VIRTUAL) TABLE` count `29` with each of the 11 new names once; every
  surviving `forward-only` hit is historical or amended; `git diff --check` clean. No build/test
  run — no code changed.
- **Gate 3 cold reads:** P3 from `03 §4` alone — **yes** (typed columns, PKs/FKs/CHECKs/indexes for
  all 11 tables, `schema_version = 14`, refusal naming `ssv2c-rebuild`, no normative migration
  function); `06` #27 resolved by the maintainer. P2 from `UI_REFERENCE §1/§2/§5` alone —
  **yes** (each placeholder names who fills it, when, and what unblocks it; `§3`/`§4`/`§8` already
  authoritative). P5/P8 from `03 §7h` + `§7j` alone — **yes** (six entity kinds, linking rule,
  ≥0.7 confidence with 7 d → 30 d decay, override precedence, five insight types; `TBD` count `0`).
- **Resolved (`06` #27, maintainer 2026-11-03):** `api.read_token` shares the `api.token` UI row;
  `sidecar.device` added as `file-only`. Committed and pushed after the maintainer validated the
  content and named `nicolasestrem/screensearch` (0.4.5 D18).

## 2026-10-20 — Setup visibility (branch `feat/setup-progress-visibility`, `06` #26)

- **Scope (approved contract `02 §5j`, `03 §7g`):** additive `SetupStatus` snapshot +
  `setup_changed` event + `get_setup_status`; engine acquisition reports real per-attempt
  bytes and Checking/Downloading/Verifying/Installing; embeddings report Preparing → Done/
  Failed (indeterminate); model-lane downloads are mirrored from `model_download`;
  StatusRail `SetupIndicator` + Settings · Models `SetupDetails`. No schema, setting, network
  destination or downloader-policy change.
- **Implemented by three disjoint writer lanes** (kernel+Tauri, `download.rs`, UI), integrated
  and verified by the parent. All compilation and tests ran on tbx (Windows, Rust 1.95.0,
  Node 26.12.0 — documented Node is 22); nothing was compiled on the Linux coordinator.
- **Native gate at the pre-review tree** (`44398dd`): `npm ci` 0 · `npm run lint` 0 ·
  `npm run build` 0 · `cargo fmt --all -- --check` 0 · `cargo clippy --workspace
  --all-targets -- -D warnings` 0 · `cargo test --workspace` 0 (inference `117 passed;
  2 ignored`, kernel setup `8 passed`) · `git diff --exit-code -- ui/src/bindings` 0. The first
  UI test run failed (`actual: '12 MB', expected: '12.4 MB'`): the test assumed one decimal,
  the code follows the existing rail/ModelPanel rule (whole numbers from 10 up) — the test was
  corrected, not the code; rerun `tests 80 / pass 80 / fail 0`.
- **RED proofs (mutation on tbx):** making `newerSetupStatus` accept unconditionally failed
  exactly the four ordering tests (`pass 76 / fail 4`); removing the kernel identical-step
  guard failed `setup_identical_re_report_neither_bumps_revision_nor_broadcasts`
  (`assertion left == right failed`, exit 101). Both files restored and verified.
- **Independent review** (read-only): `clean_with_minor_findings`, no P0/P1. Fixed: P2 empty
  snapshot announced "Local AI setup complete" → now silent; P3 Verifying/Installing reset the
  attempt to 1 → carry the delivering attempt; P3 identical re-report bumped the revision → no
  bump, no broadcast (new test); P3 chip had no width cap → `max-w-80` + truncated step text.
- **Native gate at the post-review tree** (`a36504b`): lint 0 · UI tests 0 · build 0 · fmt 0 ·
  workspace clippy 0 · `cargo test -p kernel --test setup` `9 passed` · `cargo test -p
  inference --lib` `117 passed; 2 ignored` · `cargo test -p screensearch` 0 (`162 passed`) ·
  bindings clean.
- **Native interactive run (Nico, tbx, 2026-10-20, `npm run dev` of the `a36504b` tree, sidecar
  folder emptied):** reported "All good!" with four screenshots. Observed in them:
  (1) `SSV2C_LLAMA_RELEASE_URL` pointed at a missing asset → StatusRail chip **"Setup failed"**
  plus the existing readiness banner "Inference: Unavailable — llama-server unavailable:
  download …/b0/nope.zip returned HTTP 404 Not Found"; (2) a real first-run acquisition →
  chip "Preparing local AI · Installing e…" (step text truncated by the new width cap) beside
  the "Embeddings: Starting" / "Inference: Starting — resolving llama-server" banner; (3) the
  console log shows the anonymous API lookup **failing with HTTP 403 rate limit and the pinned
  fallback being used** (the #145 path, live); (4) Settings · Models afterwards: **Engine —
  Done, "Engine available"; Search model — Done, "Search model ready"**, no chip.
  Not captured as screenshots (covered only by Nico's "All good!" report): the failed row in
  Settings details with its next step, the determinate download percentage, and the Ctrl+R
  reload reconciliation. Polish noted: at the rail's width the active chip truncates the stage
  word ("Installing e…"); a shorter active label is a candidate follow-up, not a defect against
  `03 §7g`. Not pushed; no PR; no merge.
- **PR #146 review round 1 (Codex, `e0503e3`, 3 inline P2 threads; all valid, fixed):**
  (1) `PRRT_kwDOTAuCtM6uUbG1` — the snapshot query mounts in the child `StatusRail`, whose fetch
  can resolve before `useLiveEvents`' async `setup_changed` listener exists, so a terminal event
  in that gap was lost → the hook now invalidates the setup query once the listener is live (the
  revision guard makes the refetch safe). Timing inside the WebView; no node-runner test can
  reproduce it — verified by reading, lint/build. (2) `PRRT_kwDOTAuCtM6uUbG7` — Settings kept
  listing all-`done` rows forever, contrary to `UI_REFERENCE §4` ("details list nothing") → new
  pure `setupDetailsSteps` returns nothing once no operation is active or failed; test
  `setupDetailsSteps: zero presence once every operation is done…` proven RED on tbx (guard
  removed → `pass 80 / fail 1`, file restored). This also corrects the native-run observation
  above: the "Engine available / Search model ready" rows after setup were that defect, not the
  intended idle state. (3) `PRRT_kwDOTAuCtM6uUbHC` — terminal engine `Done`/`Failed` reset
  `attempt` to 1 → the last download attempt is shared (`Arc<AtomicU32>`) with the terminal
  steps. Gate on tbx at the fixed tree: lint 0 · UI `tests 81 / pass 81 / fail 0` · build 0 ·
  fmt 0 · workspace clippy `-D warnings` 0 · `cargo test -p screensearch` `162 passed` ·
  kernel setup `9 passed` · bindings clean.
- **PR #146 review round 2 (Codex, `b500176`, auto-triggered on push; 3 inline P2, all valid,
  fixed):** (1) `PRRT_kwDOTAuCtM6uUowo` — a vision/answer download showed the legacy
  `model_download` rail chip *and* the setup chip; `UI_REFERENCE §4` wants one summarized
  indicator → the legacy rail chip is removed (the kernel already mirrors both lanes into the
  setup snapshot with real bytes; ModelPanel keeps its own `model_download` view). (2)
  `PRRT_kwDOTAuCtM6uUowu` — a failed row showed no cause outside the collapsed diagnostic →
  pure `setupFailureCause` renders a visible cause line derived only from the error text (HTTP
  status, digest mismatch, else the innermost chain segment; never an inferred "offline");
  test proven RED on tbx (`pass 81 / fail 1`, file restored). (3) `PRRT_kwDOTAuCtM6uUowy` — a
  lagged kernel broadcast could drop the terminal `SetupChanged` → `forward_events` re-emits
  `kernel.setup_status()` on `RecvError::Lagged` (the UI revision guard keeps it safe). Gate on
  tbx: lint 0 · UI `tests 82 / pass 82 / fail 0` · build 0 · fmt 0 · workspace clippy
  `-D warnings` 0 · `cargo test -p screensearch` `162 passed` · kernel setup `9 passed` ·
  bindings clean. Independent read-only review of the round-2 diff: no defects. Merge
  authorized by Nico for #146 once checks on the pushed head pass.
- **Auto-triggered Codex review on `1291529` (1 inline P2, valid, fixed):**
  `PRRT_kwDOTAuCtM6uU02n` — the vision-model failure copy said "Use Load", but Load only loads
  the answer model, so it pointed at an action that cannot retry the vision download → each
  lane now names its own existing action ("Load answer model" / "Start vision tagging"); `03
  §7g` and the `UI_REFERENCE §4` row corrected to match (no new retry mechanism). Copy-only
  change (UI + specs only — no Rust path changed since `1291529`, whose Windows CI passed). tbx:
  lint 0 · UI `tests 82 / pass 82 / fail 0` · build 0; the old "Use Load" copy fails the updated
  test (`pass 81 / fail 1`, file restored).
- **Auto-triggered Codex review on `fa186f5` (1 inline P2, valid, fixed — final round, Nico:
  "merge after addressing this round"):** `PRRT_kwDOTAuCtM6uVC5e` — after a model download
  failed, switching that lane to a tier already on disk resolves without the downloader, so the
  lane's `failed` step stayed until restart while the model served fine → the kernel clears a
  lane's `failed` setup step when the sidecar reports that lane `Ready` (other lane and non-ready
  transitions untouched). Test `setup_lane_failure_clears_when_that_lane_model_is_ready`, proven
  RED on tbx with the call removed (exit 101, file restored). tbx: UI build 0 · fmt 0 · workspace
  clippy `-D warnings` 0 · `cargo test -p kernel` all green (setup `10 passed`); tbx restored to
  clean `main`.

## 2026-10-19 — llama.cpp bootstrap recovery (branch `fix/llama-bootstrap-recovery`)

- **Trigger (field evidence):** installed 0.4.2 on tbx logged
  `ERROR screensearch_lib: llama-server unavailable error=llama.cpp release query returned an
  error status` at every start (19:43:37Z and 19:55:24Z), with `sidecar/` empty, no
  `llama-server*` anywhere and zero `.gguf` files. The app's exact anonymous request
  (`GET https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10`, UA
  `screensearch-v2c`) returned `HTTP 403` with `x-ratelimit-remaining: 0` / `used: 60`; the
  same host's authenticated probe still resolves assets, so naming/matcher/repo move were
  ruled out. `init_inference` returns on that error and is spawned once, so the session stays
  without inference.
- **Change (`crates/inference/src/download.rs`, `src-tauri/src/lib.rs`):** resolve through the
  API as before, and on any lookup failure fall back to a pinned Windows Vulkan asset
  (`b11739`, digest `2a26916f…aba249`, verified before install); bounded three-attempt retry
  for the binary zip on `401`/`403`/`408`/`429`/`5xx`/transport errors; `init_inference` logs
  and reports the full `{e:#}` chain. `SSV2C_LLAMA_RELEASE_URL` and an existing install still
  short-circuit before any network call.
- **RED (native tbX, Rust 1.95.0, before the fix):**

  ```text
  thread 'download::tests::rate_limited_release_lookup_uses_direct_fallback' panicked at
  crates\inference\src\download.rs:1802:34:
  a rate-limited API must not prevent first-run bootstrap: llama.cpp release query returned an
  error status
  Caused by: HTTP status client error (403 Forbidden) for url (http://127.0.0.1:54571/releases)
  test result: FAILED. 0 passed; 1 failed
  ```

- **GREEN (same host, after the fix):**

  ```text
  test result: ok. 110 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
  TEST_EXIT=0
  CLIPPY_EXIT=0            # cargo clippy -p inference -p screensearch --all-targets -D warnings
  BINARY OK: C:/Users/nicol/Documents/GitHub/ssv2c-bin-smoke\api-path\llama\llama-server.exe
             (91841309 bytes total install)
  BINARY OK: C:/Users/nicol/Documents/GitHub/ssv2c-bin-smoke\pinned-fallback\llama\llama-server.exe
             (91841309 bytes total install)
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 110 filtered out   # --ignored
  CARGO_EXIT=0
  ```

  The two `#[ignore]`d real-network checks download the actual archive: one through the live
  API, one through a deliberately failing API URL (nonexistent repo) that exercises the pin,
  digest check and extraction; both then run `llama-server --help` so the Windows loader and
  the DLL set are proven, not assumed. Note: since the b10xxx builds `llama-server.exe` is a
  ~9 KB launcher stub with the code in `llama-server-impl.dll`, so the install directory — not
  the `.exe` — is what the check measures.
- **Independent review (read-only subagent on `b845f7a`):** no P0/P1. Three P2s, all handled:
  the retry prose said "retried three times" where the code makes three *attempts* (fixed in
  `CHANGELOG`, `ARCHITECTURE`, `08`); the comment claiming the API has no digest was replaced
  with the accurate statement (GitHub does publish one; this resolver does not parse it yet —
  now gap `07` #96); and the `SSV2C_LLAMA_RELEASE_URL` path now also verifies the digest when
  the override URL *is* the pin. One clippy regression from that last edit (the now-unused
  non-verifying wrapper) was removed before the re-run. Re-verified on tbx at the final tree:
  `cargo test -p inference --lib` → `110 passed; 0 failed; 2 ignored` (TEST_EXIT=0) and
  `cargo clippy -p inference -p screensearch --all-targets -- -D warnings` → `Finished`
  (CLIPPY_EXIT=0).
- **Review round 1 (PR #145):** Codex reviewed `fd35571` with **no findings** (0 inline
  threads, 0 review submissions); `@claude review` returned six non-blocking suggestions as a
  PR conversation comment. Dispositions: (1) broad fallback trigger — intentional and now
  covered by `release_lookup_without_a_matching_asset_uses_direct_fallback`; (2) pin staleness
  — already `07` #95; (3) `verify_sha256` wording — reworded; (4) env-var race in the ignored
  test — now takes `ENV_LOCK` like the other env-reading test; (5) 32 MB double copy —
  pre-existing and unchanged (would need a `Bytes`-based signature), left as is; (6) offline
  start — already `07` #94. Only (1), (3) and (4) changed code/docs; no product contract moved.
- **Not run / open:** the live installed app was never restarted and its data untouched (the
  single-instance plugin means an isolated first run would require stopping it); no full
  `cargo test --workspace` / UI chain was run locally on this branch (CI's Windows job passed
  on `fd35571`), and no merge/release was made.

## 2026-10-19 — v0.4.2 published and field-tested

- **Release:** tag `v0.4.2` (annotated, on squash commit `611a4b4`, PR #142); release
  <https://github.com/nicolasestrem/screensearch/releases/tag/v0.4.2>, non-draft,
  non-prerelease, Latest, published 2026-10-19T12:22:34Z. Tag run 39249860802 passed every step
  including "Verify version consistency" and "Publish GitHub Release"; the earlier dispatch dry
  run 39249417516 on `611a4b4` also passed (tree identical to the audited `7d5133d`).
- **Assets (observed):**

  ```text
  latest.json 773
  ScreenSearch_0.4.2_x64-setup.exe 14608705
  ScreenSearch_0.4.2_x64-setup.exe.sig 424
  ```

- **Updater endpoint (observed):** `latest.json` reports `version` 0.4.2, installer URL
  `.../screensearch-v2c/releases/download/v0.4.2/ScreenSearch_0.4.2_x64-setup.exe`, signature key
  ID `27E1C773C0BDF81E` (matches the shipped pubkey). The old-name endpoint resolves:

  ```text
  HTTP/2 301 -> .../nicolasestrem/screensearch/releases/latest/download/latest.json
  HTTP/2 302 -> .../releases/download/v0.4.2/latest.json
  HTTP/2 302 -> release-assets.githubusercontent.com/...
  HTTP/2 200
  endpoint version: 0.4.2
  ```

- **Field test (maintainer-reported, 2026-10-19):** Nico: "Auto update works. Tested everything
  green and field tested 0 bugs." Recorded as the maintainer's statement; this agent did not
  observe the run and holds no per-item output, screenshot or Telegram send log from it. It is
  taken as the F3 native interactive acceptance (`04 §1c`) and the auto-update check. The
  entry above it is superseded on "no tag claimed": the tag and release now exist.

## 2026-10-19 — v0.4.2 release-prep archive sweep (branch `release/v0.4.2`)

- **Moved, verbatim:** every 0.4.2-arc entry recorded since the v0.4.1 sweep (security alert
  remediation after PR #124, #125; external-agent memory API P0–P3 and review follow-ups,
  #126–#130; F3 G1–G6 and the PR #139/#140/#141 review rounds) →
  `specs/archive/05_BUILD_REVIEW.v0.4.2.md`,
  which is the complete pre-sweep live file plus a one-line header. Same for `06`/`07`/`08`
  (`specs/archive/*.v0.4.2.md`); the `CHANGELOG.md` `[Unreleased]` block became `[0.4.2]` in
  `CHANGELOG-ARCHIVE.md`. Release prep only: no tag or release is claimed by this entry.
- **Verification (docs only):** each archive minus its two header lines is byte-identical to
  the pre-sweep live file:

  ```text
  05_BUILD_REVIEW archive identical
  06_PATCH_PLAN archive identical
  07_KNOWN_GAPS archive identical
  08_CHANGELOG_AI archive identical
  ```

- **Version bump `98b1466`** (`Cargo.toml`/`Cargo.lock`, `tauri.conf.json`, root and UI
  `package.json` + lockfiles → 0.4.2). Native `04 §1c` chain on tbx (Windows, detached at the
  pushed branch SHA, Rust 1.95.0, Node 22):

  ```text
  HEAD=98b146619c0ae496a2e0c9bba61e6805d1f98870
  rust=rustc 1.95.0 (59807616e 2026-04-14) node=v22.23.3
  EXIT npm ci = 0
  EXIT npm run lint = 0
  EXIT npm run build = 0
  EXIT npm test = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0
  EXIT bindings diff = 0
  CHAIN_EXIT=0
  ```

  `cargo test` totals: 62 suites, 1142 passed, 0 failed, 15 ignored. Later commits on the
  branch are docs/specs only. Updater compatibility audited read-only: `plugins.updater`
  pubkey, endpoint and `installMode` are byte-identical to v0.3.2–v0.4.1; the shipped
  `screensearch-v2c` endpoint 301-redirects to `nicolasestrem/screensearch` and resolves to
  the current `latest.json`; `scripts/make-latest-json.mjs` strips the tag `v`; nothing but
  `release.yml` triggers on `v*` tags; the 12 → 13 migration tests pass
  (`cargo test -p store --lib migration_`: 13 passed).
- **Codex P2 on PR #142 (`PRRT_kwDOTAuCtM6t5thb`):** the archived 05 entry "Known docs drift
  left for a follow-up" (`docs/f3-g5-g6-verification.md`: stale candidate, UI count 43) was not
  carried forward. Fixed in the PR instead of carrying it: the doc's revision pointer now names
  `specs/archive/05_BUILD_REVIEW.v0.4.2.md` and a dated correction records the committed UI
  counts, observed on Linux with `node --experimental-strip-types --test test/*.test.mjs`
  (Node v26.8.1) in a temporary detached worktree at `957b234` and in `ui/` at the PR tree:

  ```text
  957b234: ℹ tests 50 / ℹ pass 50 / ℹ fail 0
  a634e8c: ℹ tests 55 / ℹ pass 55 / ℹ fail 0
  ```

  The tbx chain at `98b1466` reported the same `# tests 55` / `# pass 55` / `# fail 0`.
- **Carried forward, still PENDING (not archived away):** F3 native Windows interactive
  acceptance (`04 §1c`) has not been run — no `npm run dev` isolated-profile session, no real
  weekly run over a real week of saved dailies, no Obsidian inspection of the weekly note and
  index links, no promotion in the running app, and no running-app screenshot of the weekly
  Settings hint. The G6 weekly Telegram test send needs an explicitly approved recipient.
  External-agent memory API: native Settings interaction (read-token controls) stayed blocked
  in SSH Session 0, and separate-machine MCP → Windows-app traversal is unproven (default
  inbound block; no firewall workaround applied). Open decisions and readings: `07`.

> **Populated during the build**, after each meaningful pass (`04 §7`). Record what actually
> happened — honestly. Empty until P0 begins.

For each build pass, append an entry:

## Pass <n> — <date> — <phase, e.g. P0 Scaffold>
- **Implemented:** what now works (with the verbatim verification output that proves it).
- **Skipped / deferred:** what was intentionally not done, and why.
- **Hallucinated / corrected:** anything the agent assumed that turned out wrong.
- **Broke / regressed:** what stopped working, and the fix.
- **Still risky:** areas that compile/pass but warrant scrutiny.

---

> Pre-0.2.x (v0.1.0) history → `specs/archive/05_BUILD_REVIEW.v0.1.0.md`.
> Shipped 0.2.x history (0.2.0–0.2.2) → `specs/archive/05_BUILD_REVIEW.v0.2.x.md`.
> Shipped 0.3.0 history (the whole arc: PR1–PR9 + post-0.2.2 bridge fixes) →
> `specs/archive/05_BUILD_REVIEW.v0.3.0.md`.
> Shipped 0.3.1 history (the P7.1 triage patch: post-0.3.0 bridge fixes PR #79/#80 + PR1–PR4) →
> `specs/archive/05_BUILD_REVIEW.v0.3.1.md`.
> Shipped 0.3.2 history (the P7.2 product-shell mini-arc: PR1–PR6) →
> `specs/archive/05_BUILD_REVIEW.v0.3.2.md`.
> Shipped 0.3.3 history (the browser-freeze hotfix: UIA skips Chromium/Electron windows) →
> `specs/archive/05_BUILD_REVIEW.v0.3.3.md`.
> Shipped 0.4.0 history (the P8 sessions arc: PR1–PR7, Passes 1–17) →
> `specs/archive/05_BUILD_REVIEW.v0.4.0.md`.
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/05_BUILD_REVIEW.v0.4.1.md`.
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141) →
> `specs/archive/05_BUILD_REVIEW.v0.4.2.md`.
> Live file holds only the current arc.

---
