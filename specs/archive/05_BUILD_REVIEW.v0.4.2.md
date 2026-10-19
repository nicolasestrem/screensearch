<!-- Archived at the v0.4.2 release sweep (2026-10-19): the complete live file as of the v0.4.2 release prep (branch release/v0.4.2). -->

# 05 — Build Review

## 2026-10-19 — PR #141 review round: incomplete all-NO_DATA week (Codex P1)

- Finding: Codex `PRRT_kwDOTAuCtM6t3zkf` (P1, at `18cc4c4`), confirmed by the `claude[bot]`
  review requested on the PR. `execute_weekly` took the empty-week path whenever every consumed
  daily was `NO_DATA`, ignoring `missing_local_dates`, so 1 `NO_DATA` daily + 6 days without a
  saved daily was saved `NO_DATA`. `03 §8e` (`PARTIAL` when any day lacks a saved daily;
  `NO_DATA` only for a week without saved dailies) settles it: valid, introduced by `c62e65a`.
- Fix `8b4f890`: the all-`NO_DATA` path requires `missing_local_dates.is_empty()`; an
  incomplete week keeps its pre-PR behavior and is saved `PARTIAL` with the missing-days banner.
  Regression `weekly_incomplete_all_no_data_week_is_not_empty`: RED on the pre-fix
  `report_service.rs` (panics at the resolver assertion), GREEN after. The `claude[bot]` minor
  note is addressed by an exhaustive `SavedReport` destructuring in
  `SavedReportView::from_saved` (a new persisted field fails to compile until classified).
- Linux (Rust 1.95.0): `cargo fmt --all -- --check` exit 0; `cargo clippy --locked -p traits
  -p store -p kernel -p api -p mcp --all-targets -- -D warnings` exit 0; `cargo test --locked
  -p traits -p kernel` exit 0 (293 passed, 0 failed).
- **Windows (tbx, Rust 1.95.0, Node 22.23.3), `04 §1c` order, detached at `8b4f890`** (commit
  transferred by git bundle, SHA asserted before the chain):

  ```text
  EXIT npm ci = 0 / npm run lint = 0 / npm run build = 0 / npm test = 0
  EXIT stage-mcp = 0 / cargo fmt = 0 / cargo clippy = 0 / cargo build = 0
  EXIT cargo test = 0        (62 suites: 1142 passed, 0 failed, 15 ignored)
  EXIT bindings diff = 0
  CHAIN_EXIT=0
  ```

  The docs-only commit on top inherits this result (no code/config/manifest path changed).
  **Not run:** `npm run dev` app verification and native F3 acceptance.

## 2026-10-19 — F3 G1–G6 review round (PR #141, pending merge)

- 17 parallel reviewers (12 on `7639b25..8b3cf95`, 5 on `8b3cf95..957b234`), read-only, findings
  by severity with file:line and scenario. 1 HIGH + ~22 lower; G5 scheduler core and kernel
  scheduling helpers clean (differential probe of `weekly_due_windows`: 1,169,040 cases across
  8 zones × 4 fire times × 12 activations × 3 date ranges × 4 cursors, 0 mismatches; another
  reviewer checked `due_at >= end` over 5M+ zone × Monday × time cases across all 486 chrono-tz
  zones). Fixes are the 5 `fix(memory|api|ui|docs)` commits on `fix/f3-review-findings`.
- Named regression tests added: `weekly_all_no_data_dailies_is_no_data`,
  `weekly_all_no_data_dailies_with_evidence_is_refused`, `weekly_no_data_day_counts_as_covered`
  (kernel), `reports_detail_never_projects_persisted_daily_run_ids_provenance` (api),
  `weekly_digest_never_counts_a_named_partial_day_as_covered` (src-tauri),
  `project_notes_skips_hidden_files_but_keeps_recovery_copies`, `promotion_conflicts_when_the_note_was_never_created`,
  `merged_content_reads_through_a_leading_utf8_bom`, `daily_note_rejects_a_weekly_synthesis_run`,
  `schema1_daily_frontmatter_is_locked_byte_for_byte` (src-tauri), `report-file-stem.test.mjs`
  cases and the weekly preview key case (ui).
- Linux verification (verbatim): `cargo fmt --all -- --check` exit 0; `cargo clippy --locked -p
  traits -p store -p kernel -p api -p mcp --all-targets -- -D warnings` exit 0;
  `cargo test --locked --no-fail-fast` over those five crates green except
  `explicit_ipv4_and_ipv6_listeners_require_authentication` (`AddrNotAvailable` — no IPv6
  loopback on this host; identical on unmodified `main`); `ui/`: tsc 0, lint 0, `npm test` 55/55,
  `npm run build` 0.
- **Not run on Linux: `src-tauri`** (Windows-only: `openssl-sys` + `std::os::windows`). The
  first Windows CI run then failed on it: clippy E0004 (`file_destination_path` missed
  `ReportDestination::Telegram`) and an unused production import of `SavedReportView` (fixed in
  `f51a1be`), then `weekly_runs_write_note_and_index_or_skip_no_data` compared the raw JSON
  `destination_snapshot` against a path (test fix in `3117aba`).
- **Windows (tbx, Rust 1.95.0, Node 22.23.3), `04 §1c` order, detached at `3117aba`:**

  ```text
  EXIT npm ci = 0
  EXIT npm run lint = 0
  EXIT npm run build = 0
  EXIT npm test = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0        (62 suites: 1141 passed, 0 failed, 15 ignored)
  EXIT bindings diff = 0
  CHAIN_EXIT=0
  ```

  Hosted CI on `3117aba`: `build & test (windows)`, all three `package & verify` targets and
  CodeQL pass. **Not run:** app verification via `npm run dev` in an isolated profile and the
  `04 §1c` native acceptance (real weekly run, Obsidian, promotion) — not claimed here.
- Known docs drift left for a follow-up: `docs/f3-g5-g6-verification.md` pins an older candidate
  commit and a UI test count (43) matching no tree; committed UI tests at `957b234` are 50 (55
  after the review-fix tests).

## 2026-10-18 — Weekly time hint (Codex `PRRT_kwDOTAuCtM6tu-Nu`, after PR #140 merged)

- PR #140 merged at `eea8184` (`957b234`) before this fix landed; this branch carries it on top
  of `main`. The weekly window always closes at Monday 00:00 local and the selected time only
  sets `due_at` (`03 §8e`: fire at the selected local time after the week closes). The hint said
  the week closed at the selected time.
- Regression in `ui/test/memory-weekly.test.mjs` was RED on the old hint (`# pass 49`,
  `# fail 1`, Node 22.20.0) and GREEN on the fix (`# pass 50`, `# fail 0`).
- Runtime verified at `418c163afef0eb59d7cff4dc23be78d7a2afaa93` (same tree as this branch's code commit `05190d5`; `git diff
  --stat 418c163 05190d5` is empty) in the existing native Windows checkout:

```text
head=418c163afef0eb59d7cff4dc23be78d7a2afaa93
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
```

- Parsed Rust totals: 1131 passed, 0 failed, 15 ignored across 62 suites. UI: 50 passed,
  0 failed. Hosted checks pending; nothing pushed. The changed Settings hint has not been run
  and screenshotted in the app (`04` UI guardrails); SSR test output is not that evidence.

## 2026-10-18 — PR #140: failed scheduled weeks recorded as missed (Nico's decision)

- Runtime verified: `ba931e84b274346c1271728fa482b02d50d1c7e8` in the existing native Windows checkout (Rust 1.95.0, Node
  22.23.3). This docs-only commit inherits that result.
- Regression `weekly_failed_scheduled_week_is_recorded_as_missed` was RED against a no-op sweep
  (local probe `69a8fa9`: `left: []` vs the failed window, `test result: FAILED. 0 passed;
  1 failed`) and GREEN on the implementation.
- Verbatim native output:

```text
head=ba931e84b274346c1271728fa482b02d50d1c7e8
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
test memory::weekly::tests::weekly_failed_scheduled_week_is_recorded_as_missed ... ok
test memory::weekly::tests::weekly_reenable_does_not_record_disabled_weeks_as_missed ... ok
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
```

- Parsed Rust totals: 1131 passed, 0 failed, 15 ignored across 62 suites. UI: 50 passed,
  0 failed. Hosted checks at this head are pending.

## 2026-10-18 — PR #140 review: disabled weekly periods recorded as missed (Codex + Claude)

- Runtime verified: `62c38adee440f612c6e3b956075cf68b88fd1e2a` in the existing native Windows checkout (Rust 1.95.0, Node
  22.23.3). This docs-only commit inherits that result.
- Fixed Codex `PRRT_kwDOTAuCtM6tuHtd` (confirmed by Claude's review): a disable keeps the weekly
  state, time and zone, so re-enabling synthesized a gap over the whole disabled interval.
  `activation_values` now records prior-lane gaps only when the old weekly schedule was
  enabled (`03 §8c`: re-enable starts a future activation period; disabled periods are not
  backfilled), matching the daily lane. Time/zone changes while enabled still record overdue
  weeks.
- Regression `weekly_reenable_does_not_record_disabled_weeks_as_missed` was RED on the unfixed
  code (`8c4bbef`, test only: panicked at `src-tauri\src\memory_weekly.rs:682:9`,
  `test result: FAILED. 35 passed; 1 failed`) and GREEN on the fix.
- Verbatim native output:

```text
head=62c38adee440f612c6e3b956075cf68b88fd1e2a
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
test memory::weekly::tests::weekly_reenable_does_not_record_disabled_weeks_as_missed ... ok
test memory::weekly::tests::weekly_timing_change_records_skipped_closed_weeks_as_gap ... ok
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
```

- Parsed Rust totals: 1130 passed, 0 failed, 15 ignored across 62 suites. UI: 50 passed,
  0 failed. Hosted checks at this head are pending; green checks at `34c7372` do not verify it.

## 2026-10-18 — F3 G5/G6 review-or-redo, rebuilt on main

- Runtime verified: `1ce4d66bcadd2d46ccf9367be565183f9e218c5c` in the existing native Windows checkout (Rust 1.95.0, Node
  22.23.3), branch `feat/f3-g5-g6-weekly-scheduler-digest-v2`. This docs-only commit inherits
  that result; it is not a claim of another runtime run.
- Candidate `421ee84` was rebuilt on `main` by applying its net delta from its own base
  `176b567` (not a merge/rebase: `176b567` is not an ancestor of `main`). PR-139 batch-3 files
  are byte-identical to `main`; the four ledgers keep both entries.
- Independent review: G5 and G6 both conform to the plan and `03 §8e/§8d`. Follow-ups: the
  future-cursor diagnostic now reaches the user; missing-day disclosure parses the producer
  banner only on partial weeks; long-ID abbreviation gained a regression test.
- A first native run at `f633312` failed (`CHAIN_EXIT=101`): the new OK-week test asserted
  over the whole digest, where Highlights reproduce the saved markdown verbatim. The assertion
  was narrowed to the Coverage line (the actual invariant); production code was unchanged.
- Verbatim native output:

```text
head=1ce4d66bcadd2d46ccf9367be565183f9e218c5c
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
test memory_schedule::tests::weekly_catchup_oldest_first ... ok
test memory_schedule::tests::weekly_backfill_cap_reports_gap ... ok
test memory_telegram::tests::weekly_digest_uses_saved_weekly_report ... ok
test memory_telegram::tests::weekly_digest_renders_weekly_format ... ok
test memory_telegram::tests::weekly_digest_chunks_fit_telegram_limit ... ok
```

- Parsed Rust totals: 1129 passed, 0 failed, 15 ignored across 62 suites. UI: 50 passed,
  0 failed. Native interactive acceptance (installed app, real week, Obsidian, promotion,
  live weekly Telegram send) remains open.

## 2026-10-18 — PR #139 review batch 3: note identity, weekly citations, export timezone (Claude + Codex)

- Runtime verified: `fe72af8f83bf43007567703ac4ad152b336af608` in the existing native
  Windows checkout; Rust 1.95.0, Node 22.23.3. A subsequent documentation-only commit
  inherits this result, not a claim of another runtime run.
- Findings fixed: Codex "Normalize the note path before ownership lookup"
  (`r4307321378`) and "Format exported dates in the saved run timezone"
  (`r4307321383`); Claude's two `reports.rs` findings — the weekly citation check was
  document-level rather than per item, and the retained-prose fallback dropped the day's
  frame IDs. One unreported defect found and fixed at integration: the widened
  `frames_summarized` (retained provenance) could exceed `frames_sampled`, so the
  `N of M sampled frames summarized` coverage line could read `5 of 2`; coverage is now
  reported as two independent counts.
- Verbatim native output:

```text
head=fe72af8f83bf43007567703ac4ad152b336af608
rust=rustc 1.95.0 (59807616e 2026-04-14)
node=v22.23.3
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
# tests 42
# pass 42
# fail 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
test reports::tests::weekly_citation_check_accepts_each_item_carrying_day_provenance ... ok
test reports::tests::weekly_citation_check_matches_whole_run_ids_only ... ok
test reports::tests::weekly_saved_prose_carries_frame_ids_without_sampling_them ... ok
test reports::tests::weekly_uncited_item_is_generation_failed ... ok
test promotion::tests::different_project_notes_do_not_collide ... ok
test promotion::tests::respelled_folder_finds_the_committed_receipt_without_appending ... ok
test promotion::tests::respelled_folder_finds_the_ownership_row_and_still_conflicts ... ok
test promotion::tests::unresolved_note_identity_falls_back_to_the_lexical_spelling ... ok
test promotion::tests::writer_conflict_without_the_append_installed_appends_nothing ... ok
test promotion::tests::writer_conflict_after_install_reports_the_append_and_records_it ... ok
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
```

- Parsed Rust totals: 1094 passed, 0 failed, 15 existing ignored. UI: 42 passed, 0 failed.
  Note-identity tests exercise real respelled paths (case, `/.`, separators,
  `\\?\`-style prefixes) against a real store; the identity rule is Windows-resolved
  canonicalization with a documented lexical fallback for a note that does not exist yet.
  Hosted checks at this head are pending publication; green checks at `176b567` do not
  verify this batch. Native interactive acceptance remains open.
## 2026-10-18 — F3 G5/G6 weekly scheduler and Telegram digest

- Code candidate: `7bdef552c5916f6ffcdd4d5f0fb14f264724fcab`; G3/G4 baseline
  PR #139 through `176b567`. Eight bounded agents plus coordinator integration.
- Actual Windows app suite (no transport/credential stubs):

```text
test result: ok. 146 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
EXIT cargo clippy = 0
EXIT cargo build = 0
EXIT cargo test = 0
EXIT bindings diff = 0
CHAIN_EXIT=0
```

- Real in-memory SQLite catch-up/replay with daily disabled verifies four persisted
  weekly windows, separate schedule identity, durable disjoint gaps, cursor-after-
  enqueue recovery and unchanged daily state/snapshots. Timing changes record only
  previously eligible closed weeks. All digest review regressions pass, including
  source caps/dedup, partial causes, banner provenance and terminal retry settlement.
- Coordinator controls: kernel scheduling 22 passed / 0 failed; UI 43 passed / 0
  failed; kernel clippy `-D warnings`, UI lint/typecheck/build exit 0.
- Full native Windows `04 §1c` chain passed: Rust **1118 passed / 0 failed /
  15 existing ignored**, UI **45 passed / 0 failed**. All five spec-named G5/G6
  gates occur as `... ok` in the raw log. SHA256 of all 11 changed source files
  matched before/after; Node22 is per-process. Restored owner branch at `176b567`
  clean; 19 shipped temporary files verified absent; installed PID21660 unchanged.
- Real installed-app/private-history/Obsidian/promotion and approved live Telegram
  sign-off remain open. No installed profile/credential/note or app was changed;
  no push/PR/merge. Detailed evidence and rollback: `docs/f3-g5-g6-verification.md`.
## 2026-10-18 — PR #139 post-install conflict receipt (Codex r4307200508)

- Runtime verified: `e69ee1f9d9702693e71e1085b2c457f03fcc5ad3` in the existing
  native Windows checkout; Rust 1.95.0, Node 22.23.3. A subsequent documentation-only
  commit inherits this result, not a claim of another runtime run.
- `write_owned` can report a conflict after installing the append when the displaced
  version differs from the prepared hash. Confirm now settles it under `NOTE_LOCK`
  from the live hash before constructing the receipt: installed promotion bytes get
  an ownership update and a receipt with hash, conflict and recovery path; otherwise
  the no-append conflict and pending intent remain. The dialog distinguishes these
  cases and never says an installed append wrote nothing.
- Verbatim native output:

```text
head=e69ee1f9d9702693e71e1085b2c457f03fcc5ad3
EXIT npm ci = 0
EXIT npm run lint = 0
EXIT npm run build = 0
# tests 37
# pass 37
# fail 0
EXIT npm test = 0
EXIT quinn security = 0
EXIT stage-mcp = 0
EXIT cargo fmt = 0
EXIT cargo clippy = 0
EXIT cargo build = 0
test promotion::tests::writer_conflict_after_install_reports_the_append_and_records_it ... ok
test promotion::tests::writer_conflict_without_the_append_installed_appends_nothing ... ok
EXIT cargo test = 0
EXIT bindings = 0
CHAIN_EXIT=0
```

- Parsed Rust totals: 1086 passed, 0 failed, 15 existing ignored. UI: 37 passed,
  0 failed. Two Rust settlement regressions use real files and a real store,
  representing the writer's post-swap states; they do not inject a concurrent
  editor into `ReplaceFileW`. Two UI render regressions check installed-append
  and recovery-copy conflict wording. Native interactive acceptance remains open.
- Hosted checks for the new head are pending publication; green checks at
  `0b621ca` do not verify this fix. No new review round or merge is claimed.

## 2026-10-18 — PR #139 additional review batch, native Windows verified

- **Frozen runtime:** `aba0c28587dab9346dd869bc8a1dea75ca0bc4db`, isolated local branch
  `fix/pr139-review-followup`; ONE existing tbx checkout on `feat/f3-g3-g4-writer-promotion`.
  Concurrent G5/G6 edits in the shared coordinator checkout were excluded, not reverted.
- **Findings:** `4305601207` — persist a confirmed-write intent before append and reconcile
  original ownership/retry identity after a storage failure; `4305709131` — no hierarchy creation
  before the guarded writer checks reparse ancestors; `4305709134` — use verified surviving
  daily-note paths to retain weekly index links after run pruning. No schema/wire/public-setting
  change. `rusqlite` is an existing workspace dependency added only to app dev-dependencies for
  real SQLite-trigger failure injection; the lockfile changes only that dependency edge.
- **Native full chain:** Rust `1.95.0`, Node `22.23.3`; fresh npm ci → lint → build → tests →
  Quinn security → stage-mcp → fmt → clippy `-D warnings` → workspace build → workspace tests →
  generated-bindings guard. Every exit was 0. Parsed across the actual Rust suite summaries:
  **1084 passed, 0 failed, 15 existing ignored**. UI **35 passed, 0 failed**. No new ignored test.
  Native and isolated coordinator worktrees were clean after the run.
- **Verbatim excerpts:**

  ```text
  head=aba0c28587dab9346dd869bc8a1dea75ca0bc4db
  # tests 35
  # pass 35
  # fail 0
  test memory::tests::weekly_index_keeps_consumed_link_after_daily_run_pruned ... ok
  test promotion::tests::owned_note_recovers_after_append_when_ownership_update_fails ... ok
  test promotion::tests::detached_confirmation_finishes_its_file_and_metadata ... ok
  test promotion::tests::create_project_note_rejects_a_junction_without_external_mutation ... ok
  test promotion::tests::lost_success_response_retry_returns_the_committed_receipt_without_appending ... ok
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0
  EXIT bindings = 0
  CHAIN_EXIT=0
  ```

- **Discriminating coverage:** real SQLite failure before intent insertion and after append;
  previously owned and initially unowned notes; exact retry vs changed date; lost
  response; restart, pruned source and folder switch; unrelated edits remain conflicts; detached
  confirmed work finishes file + ownership + receipt. The Windows junction test actually creates
  a junction and asserts no external `Projects` directory is created. The index test uses real
  retention, asserts two daily runs were deleted while their notes/ownership survive, and rebuilds
  through a persisted linked weekly revision (not a reused scheduled identity).
- **Raw log:** `/home/hermes/.hermes/profiles/brian/checkpoints/pr139-followup-20261018/windows-aba0c28.log`.
  Targeted promotion run separately passed all 26 tests; the full app suite passed 122 tests.
- **Boundary:** no push/hosted verification of this batch yet; PR #139 remains open/unmerged at
  `42cf13d`. Real-week synthesis, rendered Obsidian links and interactive promotion remain separate
  acceptance gates. No live-history/installed-app access, Telegram send, bot reply or thread resolution.

## 2026-10-18 — PR #139 CodeQL test-only follow-up

- Hosted Windows build/tests and all three wrapper package jobs passed at `66980b3`, as did all
  three CodeQL analysis jobs. The separate CodeQL alert gate failed: check `117577648747`
  annotated `ui/test/promotion.test.mjs:283`, "Incomplete string escaping or encoding" /
  "This does not escape backslash characters in the input."
- The assertion built a regex after escaping only periods. Replaced it with literal
  `html.includes(backendError)`: this test needs literal containment, not regex matching. No
  production source, generated type or dependency changed; no alert suppression/dismissal.
- Real Windows follow-up: base `66980b30d350978c16c61c38874f65e313b3e3b4` plus the copied test
  file, SHA-256 `cf21c4624ea92a144e9edd057cfb4623852488a83b03277994807a339b6b2fcf`, matched
  on both hosts. `npm run lint` exited 0; observed output:

  ```text
  # tests 35
  # pass 35
  # fail 0
  TEST_ONLY_FOLLOWUP_EXIT=0
  ```

- Hosted CodeQL verification of this follow-up is pending publication and a fresh scan. The
  earlier full native Rust/runtime verification remains applicable to unchanged production bytes;
  this focused follow-up does not claim a new workspace run or native GUI acceptance.

## 2026-10-18 — PR #139 review corrections (G3+G4)

- **All eight findings addressed locally:** reject reserved Windows device slugs (without
  rejecting ordinary `com0`/`lpt0` near-misses); confirm re-resolves exactly one existing note and
  rejects absent hashes/forged targets; source parsing and destination insertion share fence
  marker/length rules and tolerate case/closing hashes; preserve duplicate selected occurrences;
  document the latest-write ownership row; enforce persisted source/hash guards at prepare and
  confirm; refresh mounted project-note state after saving settings; serialize scan/create.
  Conflict copy distinguishes rechecking a stale preview from authorizing an owned-file edit,
  without parsing backend message strings or changing the wire schema.
- **Exact tested revision:** `1636a4fbae0b73d76f49b92091385ecb98b01f12`, branch
  `feat/f3-g3-g4-writer-promotion`, ONE native checkout on tbx. Checkpoint `c71e87a` was partial,
  not a verified revision. The spurious generated `note_edited` field was removed by actual
  Windows ts-rs generation, not hand editing. Windows worktree was clean before and after the run.
- **Observed Windows output (Rust 1.95.0, Node 22.23.3, CI order):**

  ```text
  head=1636a4fbae0b73d76f49b92091385ecb98b01f12
  rust=rustc 1.95.0 (59807616e 2026-04-14)
  node=v22.23.3
  EXIT npm ci = 0
  EXIT npm run lint = 0
  EXIT npm run build = 0
  # tests 35
  # pass 35
  # fail 0
  EXIT npm test = 0
  EXIT quinn security = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0
  EXIT bindings = 0
  CHAIN_EXIT=0
  ```

- **Named regressions observed in that same workspace test log:**

  ```text
  test promotion::tests::concurrent_create_project_note_yields_exactly_one_note ... ok
  test promotion::tests::create_project_note_rejects_reserved_slug_names ... ok
  test promotion::tests::confirm_refuses_a_forged_or_stale_note_target ... ok
  test promotion::tests::duplicate_selected_items_are_all_promoted ... ok
  test promotion::tests::excess_duplicate_selected_items_are_rejected ... ok
  test promotion::tests::ownership_blocks_prepare_after_a_user_edit ... ok
  test promotion::tests::ownership_blocks_confirm_after_an_edit_or_forged_hash ... ok
  test promotion::tests::ownership_requires_same_source_and_allows_a_new_run ... ok
  test promotion::tests::promotion_requires_explicit_confirm ... ok
  test promotion::tests::promotion_to_edited_note_surfaces_conflict ... ok
  test memory_files::tests::promotion_appends_without_rewrite ... ok
  test memory_files::tests::merged_content_ignores_fake_headings_in_mixed_fences ... ok
  test memory_files::tests::merged_content_handles_four_backtick_fences ... ok
  test memory_files::tests::merged_content_uses_the_first_duplicate_section ... ok
  test memory_files::tests::merged_content_preserves_crlf_and_a_missing_final_newline ... ok
  test memory_files::tests::merged_content_does_not_insert_inside_an_unclosed_fence ... ok
  test memory_files::tests::weekly_note_contains_provenance ... ok
  test memory::tests::weekly_index_links_daily_and_weekly ... ok
  test memory::tests::weekly_note_path_passes_provenance_check ... ok
  test memory::tests::edited_weekly_note_is_preserved ... ok
  ```

- **Totals and limits:** parsed Rust results: 1069 passed, 0 failed, 15 existing ignored checks
  (interactive desktop/WGC/UIA, OCR language pack, model downloads, live migration, perf/demo and
  manual server). UI: 35 passed, 0 failed, including real QueryClient/QueryObserver settings
  invalidation and pending-state tests; rendering tests use the unit harness, not the running
  WebView2 bridge. A preliminary UI run caught a test-only strict object-identity assertion:
  TanStack structural sharing preserves values, not necessarily object identity; fixed to deep
  equality and rerun in the full chain. No Linux test/build results substitute for this run.
- **Still not claimed:** a real-week synthesis, Obsidian link inspection, or an interactive
  promotion in a freshly built isolated app. The installed app and live history were untouched;
  no schedule, Telegram send, model download, push, merge or review-bot request was performed.
  Documentation-only follow-up records this tested revision; it does not change runtime inputs.

## 2026-10-18 — F3 G3+G4 weekly note writer and project-note promotion (`03 §8e`)

- **Scope:** G3 writes `Weekly/YYYY-Www.md` from the persisted run (frontmatter schema 2 with
  `kind`, `daily_run_ids` and every `window_local_dates` date) through the daily writer's
  atomic/owned pipeline, adds weekly `Index.md` entries linking the week's note and the owned
  daily notes it consumed, gives the index folder filter and provenance re-derivation weekly
  branches with kind separation both ways, and keeps honest skip receipts for a `NO_DATA` week.
  Kernel gains `window_local_dates` (the single date-enumeration source, consumed by
  `weekly_missing_local_dates`), `week_label` and `parse_week_label`. G4 adds
  `project_note_status` / `create_project_note` / `prepare_promotion` / `confirm_promotion`,
  `memory_files::merged_content` + `append_entries_owned`, and the `PromotionPanel` /
  `PromotionDialog` surface in `ReportView` with project-note state in `MemoryPanel`. No
  scheduler, catch-up or weekly Telegram digest (G5–G6); promotion has no HTTP/MCP surface.
- **Windows (tbx) — branch `feat/f3-g3-g4-writer-promotion` at `193ea68`, full `04 §1c`
  chain in order, `CHAIN_EXIT=0` (all 10 steps green):**

  ```text
  npm ci / npm run lint / npm run build     exit 0 (vite build 3.3 s)
  npm test                                  23 passed, 0 failed
  node scripts/stage-mcp.mjs                exit 0
  cargo fmt --all -- --check                exit 0
  cargo clippy --locked --workspace --all-targets -- -D warnings   exit 0
  cargo build --locked --workspace          exit 0
  cargo test --locked --workspace           every suite 0 failed
  git diff --exit-code -- ui/src/bindings   clean
  ```

  Named app tests observed passing on Windows (`screensearch` lib):
  `weekly_note_contains_provenance ok`, `weekly_index_links_daily_and_weekly ok`,
  `weekly_note_path_passes_provenance_check ok`, `edited_weekly_note_is_preserved ok`,
  `promotion_requires_explicit_confirm ok`, `promotion_appends_without_rewrite ok`,
  `promotion_to_edited_note_surfaces_conflict ok`,
  `weekly_runs_write_note_and_index_or_skip_no_data ok`.
  Two integration fixes were required during the chain runs and are part of this branch:
  `5771b16` (promotion fixtures must match the store's provenance invariant — the first
  chain run failed on a fixture-only duplicate `frame_ids`) and `193ea68`
  (`clippy::unnecessary_sort_by` in the section splice).
- **Linux (this host, same branch and head) — completed:**

  ```text
  cargo test -p kernel    113 lib + 9 enrichment + 17 pipeline + 3 sessions_intel
                          + 22 settings + 2 throttle, 0 failed
  cargo test -p traits    106 + 4, 0 failed
  cargo test -p store     all suites 0 failed
  cargo clippy -p kernel -p traits -p store --all-targets -- -D warnings   clean
  cargo fmt --all -- --check                                               exit 0
  ui npm run lint                                                          exit 0
  ui npm run build                                                         exit 0 (3.3 s)
  ui npm test                                                              23 passed, 0 failed
  ```

- **New kernel checks observed passing (inside the 113 lib count):**
  `window_local_dates_lists_every_local_day_through_dst`,
  `window_local_dates_handles_normal_partial_and_iso_year_windows`,
  `week_label_and_parse_week_label_roundtrip`,
  `parse_week_label_rejects_malformed_and_missing_weeks`,
  `weekly_missing_local_dates_uses_the_shared_window_enumeration`. The 23 UI checks include the new
  `ui/test/promotion.test.mjs` suite (parser selection and ordering, command names and argument
  keys, populated/empty/conflict/success panel states).
- **Named app-layer checks (Windows chain only):** `weekly_runs_write_note_and_index_or_skip_no_data`
  (replaces the G2-era `weekly_runs_skip_note_and_index_with_receipts`),
  `weekly_index_links_daily_and_weekly`, `weekly_note_path_passes_provenance_check`,
  `edited_weekly_note_is_preserved`, `weekly_note_contains_provenance`,
  `promotion_requires_explicit_confirm`, `promotion_appends_without_rewrite`,
  `promotion_to_edited_note_surfaces_conflict`.
- **Not run / not claimed here:** `src-tauri` does not compile on Linux by design, so its unit
  tests and the `04 §1c` native acceptance (a real weekly run over a real week of saved dailies, the
  note inspected with working links, and one promotion in the running app) are not claimed. `crates/api`
  and `crates/mcp` are untouched — no new network surface.

## 2026-10-17 — F3 G2 weekly synthesis engine (`03 §8e`)

- **Scope:** weekly run through the shared report service over **saved daily** reports:
  `Store::saved_daily_runs`, kernel `execute_weekly` + synthesis, truthful
  `NO_DATA`/`PARTIAL`/`OK`/`BLOCKED`/`FAILED` statuses, `SavedReport.daily_run_ids`.
  No note writer, promotion, scheduler or Telegram digest (G3–G6).
- **Windows (tbx), `04 §1c` order, branch `feat/f3-g2-weekly-engine` at `09d63a3`:**

  ```text
  head=09d63a3
  EXIT ui npm ci = 0
  EXIT ui npm run lint = 0
  EXIT ui npm run build = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0          (all suites 0 failed)
  EXIT bindings guard = 0
  ```

  Named checks observed `ok`: `weekly_requires_saved_dailies` (kernel and app layers),
  `partial_week_reports_missing_days` (both layers), `weekly_no_model_is_blocked` (both),
  `weekly_statuses_are_truthful`, `daily_run_partial_wording_is_unchanged`,
  `weekly_runs_skip_note_and_index_with_receipts`,
  `saved_daily_runs_returns_only_finished_saved_dailies_inside_the_window_oldest_first`,
  `saved_daily_runs_empty_window_is_an_empty_vec_not_an_error`.
- **Linux:** everything except `crates/api/tests/http_api.rs::explicit_ipv4_and_ipv6_listeners_require_authentication`,
  which cannot bind `::1` on this host (no IPv6 loopback); it passes on Windows above.
- **Not run:** native weekly acceptance (a real weekly run over a real week of saved dailies) —
  it needs the G3 writer to produce an inspectable note.

## 2026-10-17 — F3 G1 contract + migration (`03 §8e`)

- **Scope:** `ReportRunKind` (`daily` | `weekly_synthesis`) on `ReportRun`,
  `ReportSummary`, `ReportDetail`; forward-only `MIGRATION_V13` (`report_runs.kind TEXT NOT
  NULL DEFAULT 'daily'`, CHECK, `(kind, created_at)` index), `LATEST_SCHEMA_VERSION = 13`;
  `list_report_runs` kind filter; kernel `WeeklyWindow` + `weekly_window` /
  `weekly_first_window` / `weekly_previous`; note frontmatter schema v2. No weekly engine,
  writer, promotion, scheduler or Telegram behavior (G2–G6).
- **Windows (tbx, Rust 1.95.0, Node 26.11.1), `04 §1c` order, branch
  `feat/f3-g1-contract-migration` at `7e17abf`:**

  ```text
  EXIT npm ci = 0
  EXIT npm run lint = 0
  EXIT npm run build = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0   (62 suites, passed=1011 failed=0)
  EXIT bindings diff = 0
  ```

  Named G1 checks observed `ok`: `migration_12_to_13_defaults_existing_runs_to_daily`,
  `weekly_run_identity_reuses_schedule_window`, `weekly_window_handles_dst`,
  `frontmatter_v1_still_parses`, plus `frontmatter_v2_roundtrips_for_weekly_synthesis`,
  `unknown_frontmatter_schema_is_rejected`, `weekly_kind_reaches_summary_and_detail_projections`.
- **Findings fixed during the pass:** first Windows run failed to compile `screensearch`
  (daily writer missed the v2 frontmatter fields; test-only `ReportRunKind` imports tripped
  `-D warnings`); `preserves_existing_ids` hard-coded schema 12. Fixed in `7e17abf` / `d4234e4`.
- **Not run:** native app acceptance (`npm run dev`, isolated profile) — G1 adds no
  user-visible behavior; native weekly acceptance belongs to G2–G5.

## 2026-10-16 — External-agent memory API P0 inventory and baseline

- **Contract work:** six delegated inventories/contracts covered production inserts,
  settings/reset boundaries, every HTTP route/MCP tool, saved-report projections,
  wrapper portability and native Windows readiness at `cd9f5fe` (0.4.1, schema 12).
  New normative contract: `03 §7f`. No runtime feature implementation in this pass.
- **Review:** independent boundary review found the leftover bare macOS/Linux
  non-goal in `02 §7`; corrected and parent-verified before runtime implementation.
  Full P1/P2 review found implicit HEAD classification missing; made explicit along
  with persisted-before-live read-token lifecycle. No P1/P2 product contradiction.
  P3 visibility contradiction remains open in `06`/`07`. Separately reviewed boundary
  committed as `9f912e2`; exact parent checks:

  ```text
  boundary_staged_non_goal_carveout=PASS
  boundary_only_staging=PASS
  boundary_secret_scan=PASS
  ```
- **Baseline:** unchanged Linux MCP package, Rust 1.95.0. Actual process output:

  ```text
  BUILD EXIT CODE: 0
  TEST EXIT CODE: 0
  CLIPPY EXIT CODE: 0
  ```

  Commands: `cargo +1.95.0 build --locked -p mcp`, `cargo +1.95.0 test --locked -p mcp`,
  `cargo +1.95.0 clippy --locked -p mcp --all-targets -- -D warnings`.
  Parent independently exercised the compiled binary and stdio protocol:

  ```text
  screensearch-mcp 0.4.1
  parent_verified_stdio_exit=0
  parent_verified_tools=9
  parent_verified_server={"name": "screensearch-mcp", "title": "ScreenSearch screen history", "version": "0.4.1"}
  ```

- **Windows:** `ssh nicol@tbx` available; separate detached worktree and fixture
  tooling created, Rust 1.95.0 verified, official Node 22 extracted with checksum
  validation. Installed app/history untouched. Native baseline at `cd9f5fe` completed;
  parent read back actual return codes from both phase logs and independently checked
  the native worktree remained clean:

  ```text
  RC_UI_NPM_CI=0 elapsed_s=4.3
  RC_UI_NPM_LINT=0 elapsed_s=2.5
  RC_UI_NPM_BUILD=0 elapsed_s=5.3
  RC_STAGE_MCP=0 elapsed_s=32.9
  RC_FMT=0 elapsed_s=0.9
  RC_CLIPPY=0 elapsed_s=66.5
  RC_BUILD=0 elapsed_s=83.7
  RC_TEST=0 elapsed_s=100.7
  RC_BINDING_GUARD=0
  ```

  This verifies the unchanged base, not the concurrently developed P1 implementation.
- **Not accepted:** new API behavior, native UI, cross-machine traversal, macOS
  archive execution, allocator behavior, consumer resume and paging measurements
  still require implementation and real evidence. No merge/release/publication.

## 2026-10-16 — P1 implementation in progress (not acceptance)

- Store/traits contracts now exist. Allocator/setup/readiness/reserved-setting guards
  and real SQLite regression tests are implemented; native allocator review/tests
  are running on an immutable component snapshot, not the changing parent tree.
  Frame/coverage SQL is a separate disjoint module under implementation.
- Kernel process-local observations are wired to actual controller/worker events.
  Its isolated pure-std buffer check returned:

  ```text
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```

  Native controller tests and independent review are in progress. Unobserved WGC-
  internal readback errors stay unknown; no frame-gap-derived intervals were added.
- API explicit permissions and independent live read-token authentication are
  implemented. The worker observed clippy exit 0; its API test run exited 101
  solely on the retained IPv6 listener test because that Linux host has no `::1`
  address. The failure is not patched out or called a native pass. Native API
  verification and new frame/coverage endpoints remain open.
- Reference consumer local SQLite/UTC/resume/conflict checks were independently
  rerun by the parent:

  ```text
  Ran 20 tests in 2.788s

  OK (skipped=4)
  CONSUMER_LOCAL_TEST_EXIT=0
  ```

  Four skipped tests require the real API+Store fixture runner; no successful
  production API consumption is claimed. Token UI/composition and the isolated
  MCP transport lane are still under implementation. Full P1 acceptance remains open.

## 2026-10-16 — P1 real API consumer gate exercised

The fixture runner composes the actual API router, production SQLite Store and kernel
observations over nonprivate seeded history, with capture/scheduling off and answer
provider unavailable. It exercises more than 1000 frames, ties/out-of-order timestamps,
mid-window resume, full/textless/empty text, late enrichment, timestamp identity conflict,
and backup rewind/catch-up reconciliation. This is not a canned HTTP response server.

The parent independently reran the full consumer suite with
`SCREENSEARCH_CONSUMER_FIXTURE_CMD` pointing to the compiled
`memory_consumer_fixture` example and `PYTHONDONTWRITEBYTECODE=1`:

```text
Ran 20 tests in 50.708s

OK
PARENT_REAL_API_CONSUMER_EXIT=0
```

All four real-API cases executed; no skips in this run. The earlier local-only skipped
run above remains historical. Timing is under concurrent development load, not a release
latency budget. Native whole-workspace/app/UI and cross-machine MCP acceptance remain
separate gates; P1 is not declared complete by this fixture pass.

### P1 scoped API follow-up (Linux, not native app acceptance)

Parent-executed checks on the current HTTP foundation:

```text
cargo test --locked -p api --test http_api history_reads
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out
P1_HTTP_HISTORY_EXIT=0

cargo test --locked -p api --lib
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
P1_API_LIB_EXIT=0
```

The eight HTTP cases exercise the real Store-backed frame/coverage routes, more than
1000-frame chronology and bounded membership, literal filters/metadata/text, deletion
progress, cursor/history failures, explicit GET/HEAD authorization, truthful empty coverage
and safe errors. Library cases cover cursor bounds/codec/query rejection, future-route
default-deny, capture permission separation and retained connection/stream limits.

These are scoped package checks, not a full Linux workspace build, Windows/IPv6 or UI
acceptance. Independent consumer audit additionally found mid-traversal rewind and
single-history-change recovery bugs not covered by the earlier 20 passing consumer tests;
their fixes and real-API regressions remain required before the P1 gate closes.

### P1 native integration and consumer recovery follow-up

The frozen 41-file runtime candidate (`content_sha256`
`139392392d9299fcad21c40d7ac87a5b413a2e6b0b6d040a00e92cab7c17facf`)
passed on native Windows with Rust 1.95.0 / Node 22:

```text
P1N_fmt_EXIT=0
P1N_clippy_EXIT=0
P1N_build_EXIT=0
P1N_test_EXIT=0
P1N_RUN_DONE
```

Commands were workspace formatting, locked all-target clippy with warnings denied,
locked workspace build/test and the generated-binding/source hash guards. Unchanged UI
bytes had already passed `npm ci`, lint, build, 14 UI tests and MCP staging. Parent parsed
the raw native results: **928 passed, 0 failed, 15 ignored** across 60 result summaries;
the IPv4/IPv6 HTTP listener and IPv6 composition tests both executed successfully. The
15 existing environment/hardware-gated ignores were retained, not introduced to obtain green.
Parent independently re-read the remote native source files: `41/41 PASS`, matching this
candidate and the local worktree.

Consumer F1/F2/F3 are now fixed: a proven mid-drain allocator decrease reconciles after
the real cursor 400; one 409/history transition consumes one logical bounded restart;
a conflicting detail read is not counted as a successful textless re-read. Three new
real-API regressions failed on the old logic and pass on the fix. The full consumer suite
ran **23 tests, 0 skips**; parent verified the runner/source hashes and independently ran
all seven real-API cases:

```text
Ran 7 tests in 31.761s
OK
PARENT_CONSUMER_RECOVERY_EXIT=0
```

Native build/test acceptance is not native app/UI acceptance. Settings keyboard/clipboard/
error-state inspection and separate-machine MCP remain open. In particular, the desktop
app autostarts capture; there is no production `capture.enabled` setting. Verification
must not assume that an invented seed key disables capture or launch against a live profile.

### Follow-up: real tuple seeks and the native UI launch blocker

Measured continuation work exposed a prefix scan despite correct page membership. The
membership CTE now merges two disjoint indexed branches (equal timestamp plus rowid range,
then greater timestamps), using the existing `idx_frames_captured_at`; no DDL, index,
migration, cursor format or public contract changed. Bundled SQLite measurements cover
distinct, all-equal, grouped and shuffled timestamps. A persistent VM-step regression fails
against the old SQL and passes with the fix, alongside a 2500 equal-timestamp-frame drain.
Parent independently reran the full Store package and eight Store-backed HTTP history cases;
both commands exited 0. Native affected/full checks for these new SQL bytes are separate
from the earlier 928-pass snapshot and must be observed before counting them green.

The actual isolated native app was launched on a service window station in SSH Session 0.
It opened only the verification profile, failed WGC capture as expected and initialized the
backend, but created no WebView2 renderer/CDP endpoint. Native Settings interaction therefore
remains **blocked**, not passed. No interactive-session fallback or capture-disable bypass
was used. No model inference, report generation or delivery was exercised. The isolated
process tree was stopped; nonprivate fixtures remain available for backend-only remote proof.

### Post-seek native and actual-app backend evidence

The new seek bytes were subsequently verified on native Windows, not borrowed from the
earlier 928-pass snapshot. Formatting, locked workspace all-target clippy/build/test,
generated bindings and 41-file source guards all passed. Parent fetched and parsed the raw
native log, independently rechecked all remote source hashes and confirmed fixture cleanup:

```text
POSTSEEK_NATIVE_PASSED=930
POSTSEEK_NATIVE_FAILED=0
POSTSEEK_NATIVE_IGNORED=15
POSTSEEK_REMOTE_SOURCE_HASHES=41/41 PASS
FIXTURE_PORT45999_LISTENERS=0
```

A separately built P2 Windows wrapper then exercised the **actual isolated Windows app**
backend on Windows localhost: 1035 expected/received frames, process restart/resume,
1035 matching details, no hash mismatch, valid read credential 200, invalid credential 401,
and direct existing-frame mutation 403. The local fixture credential was seeded, not created
through the unavailable UI. Session 0 capture failed; no GGUF model, inference, report
generation or delivery was used. Private API settings/credentials were revoked/restored and
the owned app stopped after the test.

The separate-machine connection still timed out under Windows' default inbound block with
no rule for the test port. No firewall, SSH-forward or port-proxy workaround was applied.
**Windows localhost is not the cross-machine gate.** Native Settings UI remains blocked.

Independently, the checksum-verified **CI-packaged Linux client** exercised the real P1
API/SQLite/kernel fixture: 1035 frames in five pages, process restart/resume, matching full
text/nulls and 1035 details, 401/403 authorization, zero OCR/answer-provider calls. This closes
neither the Windows network nor GUI gate; P3 saved-report policy remains separately blocked.

### Token-audit corrections: affected native checks

The configured read credential is now passed into API state before the accepting serve task
starts (startup/enable/rebind), without changing legacy constructor signatures or live rotation.
`ApiStatus` now uses redacted diagnostic formatting; its serialization and generated binding
remain unchanged. Regressions cover initial real-socket authentication without a setter,
credential preservation through bind failure/rebind, and diagnostic-token redaction.

Parent copied only the three changed source files into the existing native verification
checkout, preserving their earlier bytes and verifying the full 41-file manifest before and
after the affected checks. On Windows Rust 1.95.0 / Node 22:

```text
traits:    2 passed; 0 failed; 0 ignored
api_lib:  19 passed; 0 failed; 0 ignored
api_http: 42 passed; 0 failed; 1 ignored
local_api: 23 passed; 0 failed; 0 ignored
fmt_RC=0
clippy_RC=0
bindings_RC=0
TOKEN_AUDIT_NATIVE_OVERALL_RC=0
TOKEN_AUDIT_NATIVE_SOURCE_HASHES=41/41 PASS
```

The IPv6 test that could not bind in the Linux container passes natively; it was not disabled.
These are affected native regressions, not a rerun or extension of the historical 930-test
receipt. No app was launched and no UI, clipboard, capture, live-profile or network-policy
acceptance is inferred; the existing GUI/network/P3 gates remain open.

### P3 saved-report reads: local and packaged-client evidence

Nico's GO resolved the former delivery-visibility hold: valid persisted saved output is
readable immediately, including queued/running delivery. Absent output remains explicit null;
stored NO_DATA remains factual. The policy is committed on both feature branches.

Parent independently executed the affected portable checks on the final candidate bytes:

```text
Store report_pages: 17 passed; 0 failed
API library: 23 passed; 0 failed
API report HTTP: 9 passed; 0 failed
MCP dot-ID wire regression: 1 passed; 0 failed
Python driver validators: 38 tests, OK
Existing real consumer suite: 23 tests, OK (zero skips)
```

Scoped formatting and warnings-denied clippy passed. The dot-ID regression proves exact
`.`/`..`/empty identifiers fail locally before HTTP; percent-encoding alone was not sufficient
because URL reparsing normalized those segments to another route.

The final Linux release archive was checksummed, unpacked and actually executed: version,
MCP initialization, 13 normal/10 read-only tools and an authenticated real-Store read passed.
Parent then replayed the same independent-manifest integration using that **unpacked archive
binary**, not only a debug client, against the actual `ApiServer` + `SqliteStore` + `Kernel`
fixture. Expected values were cross-checked between seed constants and direct read-only SQL,
not copied from endpoint responses:

```text
PACKAGED_REAL_DRIVER_STATUS=ok
PACKAGED_REAL_P3_REPORTS=ok
PACKAGED_REAL_CHECKS=21
STATE_CORE_UNCHANGED=TRUE
TOKEN_LEAKS=0
SOURCE_HASHES_ALL_FINAL_UNCHANGED=TRUE
PARENT_PACKAGED_NO_PROVIDER_NO_MUTATION=PASS
```

The run consumed 1035 frames over five pages with process restart/resume and zero detail
mismatches, then 11 report summaries over four pages and 11 matching report details. Eight
saved results and three null results included queued/running saved output, factual NO_DATA,
250 historical source IDs and allowlisted/null error codes. A saved-report source resolved
to a real frame; a missing historical frame returned not-found without rewriting provenance.
Report/delivery/file state fingerprints were unchanged, OCR/answer-provider calls were zero,
capture was off, and privacy canaries were absent from public projections.

Archive SHA256: `bfa24ebcc82b1740c0f3f6fecf3ed72e8fbdf0978472b74f35aacfbd97ac47a1`;
unpacked binary SHA256: `ce9658f7d3aba82edfec02aa7001922f1b8e9a0e86412066b2eba636eb623eeb`.
Source binding: Store report pages `6efd95ff…`, API reports `b7880cf2…`, MCP tools `62bceaa4…`,
driver `084bc9eb…`, producer fixture `4adb84e5…`; all 22 frozen source hashes stayed unchanged.
The local archive script ran under Node 26.8.1/Rust 1.95.0; this is not a Node22/native-CI receipt.

Native Windows Rust 1.95.0/Node22 checks subsequently passed. Parent downloaded the raw
logs, required every per-command RC to be zero, and independently read back all native inputs:

```text
PRODUCER_GATES_OVERALL_RC=0
MCP_GATES_OVERALL_RC=0
PARENT_P3_NATIVE_SOURCE_HASHES=20/20 PASS
API library: 23 passed; 0 failed; 0 ignored
API HTTP: 51 passed; 0 failed; 1 ignored
Tauri local_api: 25 passed; 0 failed
MCP: 47 library + 13 memory + 8 remote + 22 stdio; 0 failed
Workspace-unified remote TLS: 8 passed; 0 failed
NATIVE_WINDOWS_ARCHIVE_REGISTRY_AUTH_SMOKE=PASS
```

The retained IPv6 test executed. Store report/full packages, full traits, scoped formatting,
affected all-target clippy, fixture build and bindings guard passed; existing Store ignores
were retained. The actual Windows ZIP was checksummed, unpacked and executed with 13 normal/
10 read-only tools plus a genuine authenticated Store read. No app was launched. Independent
review round1 passed with no security/logic findings against the 22 frozen inputs. Its four
nonblocking hardening notes do not imply signed cursors, support for oversized non-UUID IDs,
or extra runtime-schema validation. No extra review was requested for these docs-only receipts.

**Still open:** fresh hosted checks for the published P3 revisions. These are same-host
fixture/packaged-client and native build/test results, not
GUI, direct Linux-to-Windows-app, release or merge acceptance. The queued fixture is inert
and must not be used to launch the real app. No generator/scheduler/delivery worker was started.

### Review follow-up: truthful stop observation

The claude[bot] review finding on PR #128 (bounded memory evidence / read-only credentials)
showed `Kernel::stop_capture()` recording `Interrupted(CaptureStopped)` unconditionally —
outside the `if let Some(h) = handle` branch — so the observation also fired when
`capture.take()` returned `None` and no capture task was running. When the capture source
self-terminates, the shutdown watcher in `start_capture` clears the capture slot and records an
open-ended `Interrupted(WorkerError)`; a later user stop (capture_control(Stop), tray stop,
quit path) then had no handle to stop yet still recorded `CaptureStopped`, so
`CaptureObservationBuffer::record` closed the open error interval and opened a
`capture_stopped` one. `GET /v1/coverage` / `kernel.capture_observations()` consequently
presented an unresolved worker error as a deliberate user stop, contradicting `03 §7f.2` and
`§7f.6` (reasons are observed, never inferred; actual observed endpoints are preserved; an
idempotent stop of already-stopped capture must not churn). The observation now fires only when
a live capture handle existed; a no-handle stop is a pure no-op. The regression
`stop_capture_after_source_shutdown_does_not_relabel_worker_error` first failed against the
unfixed code (`reason: CaptureStopped` on the open interval) and passes with the fix. Full
kernel suite on Linux x86_64 / Rust 1.95.0:

```text
     Running unittests src/lib.rs (target/debug/deps/kernel-ecea33f7a04539a9)
test result: ok. 93 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.79s
     Running tests/enrichment.rs (target/debug/deps/enrichment-423258413faa7b3b)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
     Running tests/pipeline.rs (target/debug/deps/pipeline-5fcaa3ae1dbb5a69)
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
     Running tests/sessions_intel.rs (target/debug/deps/sessions_intel-773b00b46a392ad5)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
     Running tests/settings.rs (target/debug/deps/settings-9c9f9acd5452a677)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
     Running tests/throttle.rs (target/debug/deps/throttle-f34053ac2968a7eb)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.58s
   Doc-tests kernel
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo fmt --all -- --check` and `cargo clippy --locked -p kernel --all-targets -- -D warnings`
both exit 0. This is kernel-crate evidence on Linux, not native Windows app acceptance; the
existing GUI/network/P3 gates remain open.

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
> Live file holds only the current arc.

---

## 2026-10-15 — Security alert remediation after PR #124

Based on main `6587e84` (v0.4.1 tag), preserving its version bump, schema 12,
network API, memory reports, Telegram and Quinn security fix. The earlier local
0.4.0 worktree is retained; only the targeted dependency changes were reapplied.

| GitHub alert | Resolution |
|---|---|
| Dependabot [36](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/36) | rustls 0.23.40 → 0.23.45; rustls-webpki 0.103.13 → 0.103.15 |
| Dependabot [39](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/39) | brace-expansion 5.0.8 → 5.0.12 |
| Dependabot [40](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/40) | brace-expansion 1.1.16 → 1.1.21 |
| Dependabot [42](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/42) | all postcss-selector-parser copies → 7.1.6 via npm override ^7.1.6 |
| Dependabot [43](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/43) | source-map-js 1.2.1 → 1.2.2 |
| CodeQL [2](https://github.com/nicolasestrem/screensearch-v2c/security/code-scanning/2) | dismissed as false positive through GitHub API at 2026-10-15T21:44:44Z |

CodeQL starts from the null pointer initializer but does not model successful
`CredReadW` allocation. `credential_read` dereferences only after success and a
null check, copies the blob before `CredFree`, and never dereferences after free.
The [Microsoft contract](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw#parameters)
guarantees a single allocated credential block and interior pointers, freed via
`CredFree`. This confirms the earlier review in the 0.4.1 archive. No broad
suppression or unsafe-code change is needed. GitHub confirmed `state: dismissed`.

The parser override replaces Tailwind 3, postcss-nested and typography's 6.x
copies without migrating Tailwind. Fresh Node 22 install, lint, TypeScript/Vite
build and existing UI tests passed. All generated assets are byte-identical to a
clean build of main (`diff -qr` exited 0 with no output). Verbatim test summary:

```text
# tests 2
# suites 0
# pass 2
# fail 0
```

Root and UI production-only npm audits returned:

```text
found 0 vulnerabilities
```

The Quinn security guard returned:

```text
PASS: GHSA-4w2j-m93h-cj5j — quinn-proto 0.11.16
```

Cargo formatting and generated-binding diff checks passed. Native Windows
inference, desktop and installer checks cannot run in this Linux environment.
Dependency alerts close after default-branch integration and graph refresh;
local fixes are not claimed to have closed the five Dependabot alerts.

Portable Rust verification used the committed Rust 1.95.0 toolchain.
`cargo test --locked -p api -p mcp` exited 0 (97 passed, one ignored):

```text
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.32s
test result: ok. 29 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 30.20s
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
```

`cargo clippy --locked -p api -p mcp --all-targets -- -D warnings` exited 0:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.86s
```

This supersedes the earlier 0.4.0 worktree result from Rust 1.99; no trait
changes or lint suppressions were needed with the pinned toolchain.
