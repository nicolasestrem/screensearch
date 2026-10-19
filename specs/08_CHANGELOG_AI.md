# 08 — AI Changelog

## 2026-10-19 — F3 G1–G6 review round: truthfulness, wire contract and hardening

- 17 reviewers (12 over `7639b25..8b3cf95`, 5 over `8b3cf95..957b234`) found 1 HIGH and ~22
  lower findings; fixes are on `fix/f3-review-findings` (PR #141), not yet merged.
  HIGH: an all-NO_DATA week could be saved `OK` — it now takes the empty-week branch
  (`NO_DATA` without evidence, `GENERATION_FAILED` with evidence, model never called,
  `daily_run_ids` recorded; `weekly_all_no_data_dailies_*`, `weekly_no_data_day_counts_as_covered`).
  HIGH: `SavedReport.daily_run_ids` leaked on the §7f.6 wire — `ReportDetail.result` is now a
  `SavedReportView` projection of exactly the eleven §7f.6 fields
  (`reports_detail_never_projects_persisted_daily_run_ids_provenance`). Lower: API/MCP docs name
  `kind`; `append_entries_owned` never invents a missing project note; a leading UTF-8 BOM no
  longer defeats section matching; `daily_note` rejects non-daily runs; hidden dot-files in
  `Projects/` are not notes (recovery copies still count); `create_note` holds `NOTE_LOCK` inside
  the blocking write and commits the corrected intent hash; weekly NO_DATA skip receipts snapshot
  `Index.md`; chunk-less Telegram receipts never abort dispatch and a skip is recorded once; a
  weekly digest never counts a named partial day as covered
  (`weekly_digest_never_counts_a_named_partial_day_as_covered`); saved-report downloads name
  their run kind (`-weekly-synthesis-`, distinct from on-demand `-weekly-`); the weekly preview
  query is keyed on its whole payload; Missed-weeks copy no longer contradicts a later manual
  run; current state moved to `01 §8`; `CHANGELOG`/`docs/MEMORY.md`/`docs/ARCHITECTURE.md` updated.
- Reviewers found no HIGH/MED defects in the G5 scheduler core or the kernel scheduling helpers
  (one differential probe of `weekly_due_windows`: 1,169,040 cases, 0 mismatches).
- Linux: fmt 0, clippy `-D warnings` 0, portable tests green except the pre-existing
  `explicit_ipv4_and_ipv6_listeners_require_authentication` (no IPv6 loopback on this host);
  UI 55/55, tsc/lint/build 0. **src-tauri fixes are unverified** — the crate is Windows-only and
  the `04 §1c` chain on tbx has not run for these commits.

## 2026-10-18 — Weekly time hint describes the fire time

- The weekly local-time hint now says the ISO week closes at Monday 00:00 and the synthesis
  runs at the selected time that Monday (Codex `PRRT_kwDOTAuCtM6tu-Nu`). Recorded the
  disable-before-catch-up reading in `07`. No push, PR or merge.

## 2026-10-18 — PR #140: failed scheduled weekly syntheses are missed weeks

- Implemented Nico's decision: a scheduler-owned weekly run that settles FAILED is recorded in
  `memory.weekly.gaps` and shown under "Missed weeks". Native chain green at `ba931e84b274346c1271728fa482b02d50d1c7e8`.

## 2026-10-18 — PR #140: re-enabling weekly no longer marks the disabled period as missed

- Fixed Codex `PRRT_kwDOTAuCtM6tuHtd`: gap synthesis on weekly re-activation now requires the
  previous weekly schedule to have been enabled. Native chain green at `62c38adee440f612c6e3b956075cf68b88fd1e2a` (see `05`).

## 2026-10-18 — F3 G5/G6 rebuilt on main with review follow-ups

- Rebuilt the G5/G6 work onto `main` with PR-139 batch-3 preserved; added the future-cursor
  diagnostic, the partial-only banner gate and three regression tests. Native chain green at
  `1ce4d66bcadd2d46ccf9367be565183f9e218c5c` (see `05`). No push, PR, merge, installed-app change or real send.

## 2026-10-18 — PR #139 review batch 3: identity, citations, timezone

- Fixed Codex `r4307321378` (note path not normalized before ownership lookup: a
  respelled folder could silently adopt an edited/owned note) and `r4307321383` (export
  footer dated in the browser's timezone instead of the saved run's), and Claude's two
  `reports.rs` findings: the weekly citation check now requires a consumed
  run/frame/session token per promotable item and per named-day claim (uncited items are
  `GENERATION_FAILED`), and a retained-prose day keeps its recorded frame IDs in the
  weekly's cited set without inflating `frames_sampled`. No wire field, schema,
  dependency or generated-binding change.
- Found at integration and fixed here: since retained provenance now joins
  `frames_summarized`, it can exceed `frames_sampled`, and the coverage line's
  `N of M sampled frames summarized` could read `5 of 2`; both the report panel and the
  export footer now report the two counts separately.
- Native Windows full chain at `fe72af8`: `CHAIN_EXIT=0`, Rust 1094 passed / 0 failed /
  15 existing ignored; UI 42 passed / 0 failed. Named regressions in `05`. New hosted CI
  pending publication; no merge, installed-app change or live-history access in this
  record.
## 2026-10-18 — F3 G5/G6 weekly scheduling and saved-output digest

- Added default-off weekly schedule, time/zone, independent UUID/activation/cursor,
  Run most recent complete ISO week, separate latest status and visible disjoint
  older gaps. Catch-up scans six complete weeks, queues four most recent due weeks
  oldest-first, and safely replays an enqueue-before-cursor interruption.
- Added separately consented weekly Telegram format from the saved synthesis only:
  week label, truthful saved-daily coverage/limitations, missing dates, bounded
  source IDs, masking and existing UTF-16 chunks/intents/retries. Daily digest bytes
  are protected by a regression. No second model, raw OCR/screenshots or new API.
- Eight bounded agents; coordinator integrated findings and verified the full
  native Windows `04 §1c` chain at `7bdef55`: `CHAIN_EXIT=0`, Rust1118/0 (15 existing
  ignored), UI45/0, bindings unchanged, all 11 source hashes matched. Native owner
  branch restored clean and shipped temporary files removed. See `05` and
  `docs/f3-g5-g6-verification.md` for evidence boundaries and rollback.
- No push, PR, merge, installed-profile mutation, live-history use or real send.
## 2026-10-18 — PR #139 truthful receipt after a concurrent note swap

- Fixed Codex `r4307200508`: when the writer reports a conflict after the append
  landed, confirm checks the live hash under the note lock and records ownership
  before returning the installed hash and recovery path. The dialog reports the
  append as written with a recovery warning and Done, rather than claiming no
  write or offering to append it again. Other recovery-copy conflicts no longer
  claim the note was untouched. No receipt fields or schema were added.
- Native Windows full chain at `e69ee1f`: `CHAIN_EXIT=0`, Rust 1086 passed /
  0 failed / 15 existing ignored; UI 37 passed / 0 failed. Two real-file/store
  settlement regressions plus two UI render regressions; outputs in `05`.
- New hosted CI is pending publication; no third review request, merge,
  installed-app change or live-history access is part of this record.

## 2026-10-18 — PR #139 additional promotion/index review fixes

- **Change:** confirmed promotion intent and last receipt in existing internal per-note settings
  KV, without selected/note text, recover ownership after a landed append, restart or source-run
  pruning and answer exact lost-response retries without duplicating content. Selection/date
  identity is hashed; unrelated edits stay conflicts. IPC critical work owns its lifetime through
  filesystem and SQLite bookkeeping. Creation delegates hierarchy changes to the guarded writer
  before any junction can create external directories. Weekly indexing retains links from verified
  owned daily-note paths after daily runs are pruned, falling back to plain dates only when needed.
- **Why:** relevant new Codex findings `4305601207`, `4305709131`, `4305709134`; existing `03 §8c/8e`
  persistence and ownership guarantees. No schema/wire/public-setting change or G5/G6 scope.
  Existing workspace `rusqlite` is an app test-only dependency for actual SQLite failure injection.
- **Verification:** full native Windows `04 §1c` chain at `aba0c28`, Rust1.95.0 / Node22.23.3,
  `CHAIN_EXIT=0`: Rust1084 passed / 0 failed / 15 existing ignored; UI35 passed / 0 failed;
  fmt/clippy/build/security/binding checks all exit0. Includes a real junction, actual retention
  with a production index rebuild, owned/unowned storage-failure recovery and detached confirmation.
  Raw output in `05`. Isolated local worktree excludes concurrent G5/G6 changes; no second tbx
  checkout. Not yet pushed; fresh hosted checks and native interactive acceptance are not claimed.

## 2026-10-18 — PR #139 CodeQL test assertion

- Replaced a dynamically constructed, incompletely escaped regex in the promotion error-render
  test with literal substring containment. Resolves the source of CodeQL check `117577648747`
  without weakening the assertion, suppressing the finding or changing production code.
- Native Windows lint exited 0; all 35 UI tests passed. Copied-test SHA and exact output are in
  `05`. Publication and the new hosted scan remain pending; old successful analysis jobs do not
  certify that the failing alert gate is clear.

## 2026-10-18 — PR #139 promotion and writer review corrections

- **Change:** enforce same-source persisted ownership/hash at both promotion preparation and
  confirmation; re-resolve the one existing target and require a selection-time hash; serialize
  project-note creation and confirmed writes; reject reserved DOS device slugs without blocking
  ordinary near-misses; preserve selected duplicate occurrences with a saved-multiplicity bound.
  Source parsing and destination insertion share proper fence-marker/length tracking, tolerate
  case/closing hashes, preserve duplicate destination sections, and avoid inserting inside an
  unclosed fence. Document the ownership row as the latest-write guard, not a run attribution or
  garbage-collection index. Saving memory settings invalidates and awaits mounted project-note
  state; stale/error status no longer traps first setup. Error copy preserves user edits and does
  not promise re-preparing can authorize an owned-file edit. Retry is not disabled by a stale
  proposal's missing-note flag. Regenerated actual ts-rs output removes the interrupted WIP field.
- **Why:** all five Claude and three Codex findings at `fc39c65`, enforcing the existing `03 §8e`
  contract without changing product policy, schema, network surfaces, G5 or G6.
- **Verification:** full Windows `04 §1c` chain at `1636a4f` on tbx, Rust 1.95.0 / Node 22.23.3:
  `CHAIN_EXIT=0`; UI 35 passed / 0 failed; Rust 1069 passed / 0 failed / 15 existing ignored;
  regenerated-binding guard clean. Exact output and named regressions are in `05`, implementation
  readings in `07`, remaining native acceptance/publication boundaries in `06`. No installed-app
  restart, live-history access, Telegram send, review request or merge.

## 2026-10-18 — F3 G3+G4: weekly note writer, index entries, project-note promotion

- **Change:** weekly runs write `Weekly/YYYY-Www.md` from the persisted run (schema-2 frontmatter
  with `kind`, `daily_run_ids` and `window_local_dates`) through the daily writer's atomic/owned
  pipeline, and appear in `Index.md` with a `days:` list linking the owned daily notes they
  consumed; the index folder filter and provenance re-derivation gain weekly branches with kind
  separation both ways, and a `NO_DATA` week writes neither output. Kernel adds
  `window_local_dates` (the single date-enumeration source, also consumed by
  `weekly_missing_local_dates`), `week_label` and `parse_week_label`. Promotion adds
  `prepare_promotion` / `confirm_promotion` / `project_note_status` / `create_project_note` over
  bullet-level selections, `memory_files::merged_content` + `append_entries_owned` (one atomic
  multi-section append under the selection-time hash), `PromotionPanel` / `PromotionDialog` in the
  report view and project-note create/status in `MemoryPanel`.
- **Why:** `03 §8e` G3 (the weekly note, index entry and provenance branches under the daily
  writer's atomic, hash-tracked and ownership guarantees) and G4 (explicitly confirmed, append-only
  promotion into the single user-created project note, so generated prose never enters a project
  note unreviewed and a selection-time edit is never overwritten).
- **Verification:** Linux `cargo test -p kernel` (113 lib + 9 enrichment + 17 pipeline + 3
  sessions_intel + 22 settings + 2 throttle), `-p traits` (106 + 4) and `-p store` all 0 failed;
  `cargo clippy -p kernel -p traits -p store --all-targets -- -D warnings` clean;
  `cargo fmt --all -- --check` exit 0; `ui` lint and build exit 0, `npm test` 23 passed / 0 failed.
  Windows 04 §1c chain on tbx at `193ea68`: every step green (`CHAIN_EXIT=0`) —
  npm ci/lint/build, npm test 23/0, stage-mcp, fmt, clippy `-D warnings`, workspace
  build, workspace tests (all named G3/G4 tests passing), bindings guard clean.
  Raw output in `05`; decisions and edge cases in `06` and `07`.

## 2026-10-17 — F3 G2: weekly synthesis engine over saved dailies

- **Change:** `Store::saved_daily_runs`; kernel `execute_weekly` + weekly synthesis through the
  shared report service and model policy; truthful weekly statuses; `SavedReport.daily_run_ids`;
  weekly runs orchestrated in `src-tauri` with a missing-days PARTIAL banner.
- **Why:** `03 §8e` G2 — a weekly report is a synthesis of that week's **saved** daily reports,
  never a raw re-scan, and its status must be derivable from persisted data.
- **Verification:** full `04 §1c` Windows chain on tbx at `09d63a3`, all exit 0, all suites
  0 failed; raw output in `05`.

## 2026-10-17 — F3 G1: report-run kind, 12 → 13 migration, weekly window, frontmatter v2

- **Change:** `ReportRunKind` on runs and `/v1/reports` projections; forward-only
  `MIGRATION_V13` adds `report_runs.kind` (default `daily`); kernel ISO-week window helpers;
  note frontmatter schema v2 with schema-1 compatibility.
- **Why:** `03 §8e` G1 — freeze identity, window, kind discriminator and frontmatter before
  the weekly engine (G2).
- **Verification:** full `04 §1c` Windows chain on tbx at `7e17abf`, all exit 0, 1011 tests
  passed / 0 failed; raw output in `05`. No native app acceptance (no user-visible change).

## 2026-10-17 — F3 deeper-memory contract (02 §5i, 03 §8e)

- **Change:** recorded Nico's FINAL F3 deeper-memory decisions (GO, 2026-10-17; the F1-pilot
  contingency is waived) across the ledgers: `docs/deeper-memory-f3-plan.md` approved — kind token
  `weekly_synthesis`; `kind` as an indexed `report_runs` column (`kind TEXT NOT NULL DEFAULT
  'daily'`, forward-only 12 → 13 migration) exposed on `GET /v1/reports`; catch-up cap 4 complete
  ISO weeks; weekly headings `Activity` / `Explicitly evidenced decisions` / `Open questions` /
  `Next steps`; exactly one project note; weekly Telegram digest with its own weekly-specific
  format under `03 §8d` as increment G6. The `07` F3 open-decisions entry is marked resolved with
  the original questions preserved as history; `CHANGELOG.md` carries the contract entry.
  Normative contract text: `02 §5i` + `03 §8e`.
- **Why:** maintainer decisions (Nico, 2026-10-17) after review of the corrected plan (#131);
  listing/filtering needs the persisted `kind`, and the 12 → 13 migration is already required.
- **Verification:** documentation-only edit (no runtime, schema, code or build change; no `02`/`03`
  text authored here). Edited files re-read after writing; relative links checked; no build run.

## 2026-10-17 — Fix untrusted PR checkout in on-demand Claude review (CI security)

- **Change:** `claude-code-review.yml` checks out the default branch instead of
  `refs/pull/<n>/head`; the review reads the diff via the GitHub API.
- **Why:** #134 made the review comment/dispatch-triggered, which runs with secrets
  (`CLAUDE_CODE_OAUTH_TOKEN`, `id-token: write`); checking out fork PR code there let a PR plant
  `.claude/settings.json` hooks or CLAUDE.md instructions that Claude Code would load. CodeQL flagged
  `actions/untrusted-checkout/high` + `actions/untrusted-checkout-toctou/high` (alerts #3, #4).
  #134 was merged before its checks finished — process miss; wait for checks before merging.
- **Verification:** `actionlint 1.7.7` exit 0; CodeQL re-scan on the fix PR.

## 2026-10-17 — F3 deeper-memory plan corrected after review (docs only)

- **Change:** corrected `docs/deeper-memory-f3-plan.md` (PR #131): renamed the proposed kind
  away from the existing `ReportKind::Weekly`; stated that `kind` is new persisted/wire surface,
  not an existing field; scoped the slice to one project note; described the index/path/provenance
  writer work; fixed heading names, the symlink-rule citation and the retention wording. Logged the
  open decisions in `07`.
- **Why:** two valid review threads (r4303787850, r4303788563) and a citation check against code
  and specs at `e55f2cc`. `02`/`03` untouched; no schema or code change.
- **Verification:** docs only; each cited `path:line` was re-read at the PR head. No build run.

## 2026-10-17 — Claude cost reduction: on-demand PR review + targeted spec reads (CI/docs)

- **Change:** `claude-code-review.yml` no longer runs on `pull_request`; it runs only on a
  `/claude-review` PR comment from an owner/member/collaborator or a manual `workflow_dispatch`
  (PR number input), checks out the PR head, and pins `--model claude-sonnet-5-5`. `CLAUDE.md` and
  `04 §1` replace the "read 01–04 + UI_REFERENCE end to end every session" rule with targeted
  section reads; the CLAUDE.md release-history paragraph is condensed to a pointer to
  `CHANGELOG-ARCHIVE.md` / `specs/archive/` (no history lost).
- **Why:** Nico approved (A–D) after cost review: ~50 automatic multi-agent reviews in 3 days
  (every push, Dependabot, docs-only PRs) and ~60k tokens of mandatory spec reading per session.
- **Verification:** `actionlint 1.7.7` on both Claude workflows exit 0. The comment trigger only
  takes effect once the workflow is on `main` (`issue_comment` uses the default-branch file).

## 2026-10-17 — P3 saved-report read contract and visibility resolution (docs/policy ledger)

- **Change:** resolved the stale "awaiting Nico / P3 only" saved-output visibility hold across the
  live ledgers as **approved/superseded** (historical records preserved): saved report output is
  exposed by persisted presence, including while a run is queued or retrying for delivery; absent
  output is explicit `null`; stored `NO_DATA` stays factual; a malformed saved payload is a read
  error (`03 §7f.10`, Nico GO). Documented the exact P3 report read contract — `GET /v1/reports`
  and `GET /v1/reports/{run_id}` inputs, wire DTOs (`ReportSummary`/`ReportPage`/`ReportDetail`/
  `SavedReport`), provenance, nullability, privacy exclusions, error mapping, half-open window
  overlap and the descending `(created_at, run_id)` tuple cursor with live-status semantics — in
  `docs/API.md`, and recorded the `list_reports`/`get_report` tools and stacked-branch status in
  `docs/MCP.md`. Recorded the P3 verification layers (genuine `ApiServer`+`SqliteStore` fixture vs
  raw-HTTP protocol fixtures, the independent expected manifest/driver, the model-unloaded /
  schedules-off no-mutation gate) and the pending GUI/native and cross-machine gates in
  `docs/TESTING.md`; updated `README.md` and `docs/memory-api-direction-plan.md`.
- **Why:** the earlier P3 visibility hold was answered by Nico (GO) and the contract is frozen in
  `03 §7f`; the docs must state the approved contract without claiming it is implemented. The
  runtime is being implemented on `feat/agent-memory-saved-reports`.
- **Verification:** documentation-only edit (no runtime, schema, index, build or Git-index change).
  Markdown link/fragment validation and a targeted diff check were run; **P3 runtime verification
  remains pending** and is not claimed here. No P2 old-head CI run is reused as P3 evidence.

## 2026-10-16 — External-agent memory API contract and inventories

- **Change:** explicit narrow portable/remote stdio-client boundary plus `03 §7f`
  contracts for chronological evidence, stable allocator metadata, truthful bounded
  coverage, independent read credential, safe saved-report reads and acceptance.
  Companion scope/operating/UI documents updated; schema remains 12. No runtime
  feature implementation in P0 and no listener or inference-default changes.
- **Why:** execute Nico's attached plan in independently checkable increments,
  retaining ordinary local workflows. An unfinished-run/saved-output contradiction
  is explicitly held for Nico rather than guessed; P1/P2 remain independent.
- **Verification:** independent inventories and boundary/full-contract review;
  residual OS prohibition and implicit HEAD classification corrected, verified by
  parent. Separate boundary commit `9f912e2`. Unchanged Linux MCP baseline:

  ```text
  BUILD EXIT CODE: 0
  TEST EXIT CODE: 0
  CLIPPY EXIT CODE: 0
  ```

  Parent binary/version/stdio verification and uncompleted native/remote gates in `05`.

> Append-only record of what the agent changed during the build, **with reasons**. One entry per
> meaningful change set. Empty until P0 begins. (This tracks build work; the design-phase history
> lives in git.)

## <date> — <short title>
- **Change:** what was added/modified.
- **Why:** the reason, tied to a spec section.
- **Verification:** the command run + verbatim result.

---

> Pre-0.2.x (v0.1.0) history → `specs/archive/08_CHANGELOG_AI.v0.1.0.md`.
> Shipped 0.2.x history (0.2.0–0.2.2) → `specs/archive/08_CHANGELOG_AI.v0.2.x.md`.
> Shipped 0.3.0 history (PR1–PR9 + bridge fixes) → `specs/archive/08_CHANGELOG_AI.v0.3.0.md`.
> Shipped 0.3.1 history (post-0.3.0 bridge fixes + PR1–PR4) →
> `specs/archive/08_CHANGELOG_AI.v0.3.1.md`.
> Shipped 0.3.2 history (PR1–PR6) → `specs/archive/08_CHANGELOG_AI.v0.3.2.md`.
> Shipped 0.3.3 history (the browser-freeze hotfix) → `specs/archive/08_CHANGELOG_AI.v0.3.3.md`.
> Shipped 0.4.0 history (the P8 sessions arc: PR1–PR7) →
> `specs/archive/08_CHANGELOG_AI.v0.4.0.md`.
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/08_CHANGELOG_AI.v0.4.1.md`.
> Live file holds only the current arc.

---

## 2026-10-15 — Six security alerts

- **Change:** reapplied the five dependency fixes onto v0.4.1 main: rustls
  0.23.45/webpki 0.103.15, brace-expansion 1.1.21/5.0.12, source-map-js 1.2.2,
  and selector-parser 7.1.6 via override. Dismissed CodeQL alert 2 as a false
  positive after verifying CredReadW allocation, null check, copy and free order.
- **Why:** remediate the six alerts at the maintainer's request while retaining
  all 0.4.1 changes and the Quinn fix.
- **Verification:** clean UI install/lint/build/tests and byte-identical output;
  production npm audits clean. Alert mapping and raw check excerpts in `05`.
