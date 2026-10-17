# Deeper memory — F3 planning increment (weekly synthesis + accumulating project note)

Status (2026-10-17): **PROPOSED.** This is the planning artifact for the F3 gate named in
[memory-api-direction-plan.md](memory-api-direction-plan.md) §5 ("only after F1 passes and F2 is
reviewed, separately plan a minimum deeper-memory slice …"). It grants **no** implementation,
spec-amendment, schema-migration or merge authorization. On maintainer GO its §3 becomes the
contract entry (`02 §5i` + `03 §8e`) reviewed per Gate A before any code.

Author provenance: drafted by the maintainer's second-brain librarian agent from the external
consumer side, after live second-machine verification of the P1–P3 read surfaces (frames, coverage,
saved-report HTTP reads) on 2026-10-17. It has not observed the Windows build loop.

## 1. Goal and scope

Turn saved daily evidence into durable cross-day memory. The daily report is the atomic unit; the
deeper-memory slice adds exactly two layers above it and nothing else, matching the direction-plan
F3 minimum (one weekly synthesis, one user-selected project note):

1. **Weekly synthesis** — one index-linked weekly report per source that references that week's
   saved daily reports and the underlying evidence, with citations.
2. **Accumulating project note** — one user-created, user-named note that collects source-backed
   decisions, open questions and next steps across days, appended only through explicit
   user-confirmed promotion from a synthesis or daily report.

Loop: `daily reports → weekly synthesis (cites dailies + evidence) → user-confirmed promotion →
durable project note`.

## 2. Non-goals

- No automatic edits to personal identity or existing knowledge pages (direction-plan F3 rule).
- No promotion without explicit user confirmation; generated prose never enters a project note
  unreviewed, and rejected proposals are not persisted anywhere.
- No report generation over HTTP/MCP (C3 stays deferred); weekly runs are local, scheduled,
  default-off, same as daily.
- No new capture, no audio, no cross-platform capture, no cloud inference.
- No Telegram change in this slice; a weekly digest would reuse the `03 §8d` exception as a
  separate later increment, separately flagged.
- No scoring, nags, or unsolicited automation (pull-based, D4/D11).
- No multiple project notes in the first slice (see §6).
- Schema changes only as one forward-only migration in the increment that needs it (12 → 13), never
  guessed or downgraded.

## 3. Contract surfaces (to become normative on GO)

### 3.1 Weekly synthesis runs

- New report kind, proposed token `weekly_synthesis` (pending the §6 naming decision; logged in
  `specs/07_KNOWN_GAPS.md`), executed through the shared report application service (C1) —
  no second engine, no second model path. It is deliberately distinct from the existing on-demand
  `ReportKind::Weekly` (`crates/traits/src/ipc.rs:158`, `03 §8b`: trailing 7 local days, frame-based
  map-reduce), which keeps its name, label ("Weekly") and `screensearch-report-weekly-…` filenames
  unchanged.
- Run identity: schedule ID + explicit UTC window `[week_start, week_end)`. The window is the ISO
  week (Monday 00:00 local → next Monday 00:00 local) computed in the user's selected IANA
  timezone; DST-shortened/lengthened weeks produce 167/169-hour windows, never 24·7 UTC hours.
- Uniqueness is `(schedule_id, window)`; retries reuse the run; Regenerate creates a linked
  revision, mirroring the daily contract (`03 §8c`).
- Truthful statuses reuse C4: `NO_DATA` for a week with no saved dailies and no evidence;
  `PARTIAL` when any day in the window lacks a saved daily report or its coverage is marked
  partial/truncated; `BLOCKED` when prerequisites (model) are missing; `FAILED` on execution
  errors; `OK` only with complete measured coverage of the window.
- Inputs: the saved daily reports in the window, their stored evidence IDs and coverage records.
  A weekly run may re-read evidence but **must** record the `daily_run_ids` it consumed in its
  provenance; a weekly report citing no daily run for a day it describes is a defect.
- Persistence: weekly runs live in the same report-run store. No kind discriminator exists today —
  not on `ReportRun` (`crates/traits/src/memory.rs:110-132`), not on the `report_runs` table
  (`crates/store/src/schema.rs:455-469`). Adding an explicit `kind` (daily vs the new kind)
  and window granularity is new persisted surface, carried by the single forward-only 12 → 13
  migration with a `schema_version` bump (§2); the maintainer decides at G1 (§6) whether `kind` is a column (needed if uniqueness or listing filters on
  it) or a payload field with a default for existing daily runs.

### 3.2 Weekly note and index

- Writes `<folder>/ScreenSearch/Weekly/YYYY-Www.md` (e.g. `2026-W42.md`) and adds weekly entries to
  the existing `ScreenSearch/Index.md`, linking week → daily notes. This is new writer work, not an
  append to an existing mechanism: today the index is a flat newest-first list rebuilt from
  `Daily/` and `Reports/` only (`src-tauri/src/memory.rs:1219`, `:1291-1308`), and note paths are
  re-derived through `note_relative_path` (`src-tauri/src/memory_files.rs:95-120`), whose provenance
  check would reject a `YYYY-Www` name (`memory.rs:1261-1276`). The index builder, path derivation
  and provenance check each need a weekly branch.
- Same writer guarantees as daily notes: stored content hash, replace/append only unchanged owned
  files, user edits surface as conflicts and are never overwritten, interrupted writes recover with
  the previous file intact (D3/D3a); symlinks/reparse points rejected (`03 §8c`).
- Frontmatter: reuse the daily typed fields and add new fields `kind: weekly_synthesis`,
  `daily_run_ids`, `window_local_dates`, with the same coverage counts. Daily frontmatter has no
  `kind` field today (`memory_files.rs:62-83`, schema 1), so note-frontmatter schema bumps v1 → v2
  with a typed parse/serialization roundtrip test; v1 daily notes must still parse.
- Fixed headings (proposed; §6): reuse the daily headings `Activity` and `Explicitly evidenced decisions`
  (`crates/kernel/src/reports.rs:786`) and add `Open questions` and `Next steps`. Every item cites
  frame/session/daily-run IDs. Tentative screen text is never stated as commitment.

### 3.3 Project note (accumulation)

- The user creates one project note in-app (`<folder>/ScreenSearch/Projects/<slug>.md`). The app
  never invents, renames or deletes projects; creation is manual and default-empty.
- **Promotion is the only write path**: the user selects items from a synthesis or daily report and
  confirms; the app appends one dated, provenance-carrying entry in one atomic write.
- After first write the note is user-owned under the same ownership rules: appends only to unchanged
  owned files; user edits surface as conflicts; existing lines are never rewritten.
- Fixed sections: `## Decisions`, `## Open questions`, `## Next steps`, `## Log`. Each appended
  entry carries its date, source `run_id`(s) and frame/session IDs, and is additive forever — the
  note is an append-only ledger, not a summary the app maintains.
- No HTTP/MCP surface in the first slice. `GET /v1/reports` projections (`ReportSummary` /
  `ReportDetail`, `crates/traits/src/reports.rs:125-181`) carry no `kind` today; if weekly runs
  appear in that listing, exposing `kind` is a deliberate wire-contract change reviewed with the
  §3.1 persistence change, not an existing field that surfaces automatically.

### 3.4 Scheduling

- Default-off weekly schedule beside the daily one; Settings controls mirror D4 (schedule, time,
  timezone, folder implied by the daily choice, Run now, last-run status).
- First eligible window = the first complete ISO week after activation. Restart catch-up processes
  missed weeks oldest-first, capped at the 4 most recent complete weeks (proposed; §6);
  older gaps stay visible and are never claimed as covered.
- Timezone selection is fixed while travelling; DST transitions follow the daily rules (missing
  local time → first valid instant, overlapping → earlier instant; `03 §8c`).

## 4. Increments and proposed checks

Each increment is separately reviewable; names are proposals unless marked existing.

- **G1 Contract + migration:** freeze run identity, window, statuses, the `weekly_synthesis` kind,
  the persisted `kind` discriminator (12 → 13), promotion and note-frontmatter schema v2 in
  `03 §8e`. `weekly_window_handles_dst`, `weekly_run_identity_reuses_schedule_window`,
  `migration_12_to_13_defaults_existing_runs_to_daily`, `frontmatter_v1_still_parses`.
- **G2 Engine:** weekly run through the shared service consuming saved dailies; truthful statuses.
  `weekly_requires_saved_dailies`, `partial_week_reports_missing_days`, `weekly_no_model_is_blocked`.
- **G3 Writer:** atomic weekly note + index/path/provenance branches.
  `weekly_note_contains_provenance`, `weekly_index_links_daily_and_weekly`,
  `weekly_note_path_passes_provenance_check`, `edited_weekly_note_is_preserved`.
- **G4 Promotion:** select → confirm → append; conflicts preserved.
  `promotion_requires_explicit_confirm`, `promotion_appends_without_rewrite`,
  `promotion_to_edited_note_surfaces_conflict`.
- **G5 Scheduler:** weekly schedule, catch-up, visible gaps. `weekly_catchup_oldest_first`,
  `weekly_backfill_cap_reports_gap`.

Native acceptance mirrors the daily arc: a real weekly run over a real week of saved dailies, the
rendered note inspected in Obsidian with working links, and one promotion performed in the running
app. No Telegram step in this slice.

## 5. Review focus

1. **Week boundaries / DST / timezone change** — no duplicate or skipped weekly windows (G5).
2. **Synthesis over-claiming** — weekly prose must cite; uncited claims are a defect, not style (G2/G3).
3. **Promotion races** — user edits the project note between select and confirm (G4).
4. **Retention interplay** — finished report runs (any kind) prune 90 days after their last terminal
   update, with the existing queued/running/blocked and active-delivery exemptions
   (`src-tauri/src/memory.rs:20`, `03 §8c`). Weekly and project notes are user-owned output and
   are never pruned by run retention. Provenance IDs may outlive their sources; existing
   frame/session reads already 404 honestly and report reads must not invent availability.
5. **Kind separation** — the on-demand `Weekly` report and the scheduled `weekly_synthesis` must
   never share a token, label or filename stem (G1/G3).
6. **Index growth** — `Index.md` grows by design; flag a split rule (e.g. per-month sections) at
   review instead of silently bounding history.

## 6. Decisions needed from the maintainer

The specs are silent on every item below. Per `04 §5` each is an open question for the maintainer,
recorded in `specs/07_KNOWN_GAPS.md`; nothing here resolves it, and implementation stops until it is
answered.


- GO/NO-GO, contingent on F1 pilot outcome review per the direction plan.
- Weekly kind naming: `weekly_synthesis` proposed, kept distinct from the on-demand
  `ReportKind::Weekly` in the typed model, run store, note frontmatter, labels and filenames.
- `kind` placement: indexed column vs payload field with default (G1).
- Weekly note headings: reuse daily `Activity` / `Explicitly evidenced decisions` plus
  `Open questions` / `Next steps`, or new names.
- Weekly catch-up cap: 4 complete weeks proposed.
- Whether a weekly Telegram digest rides the existing `03 §8d` exception as a later increment.
- Project-note location and naming: `ScreenSearch/Projects/<slug>.md` proposed.
- Multiple project notes: out of the first slice per the direction-plan minimum; a later increment
  only if the single-note pilot holds up.

## 7. Verification strategy

Reuse the repository's declared order (UI install/lint/build, stage MCP, `cargo fmt --all --
--check`, clippy `-D warnings`, build, test, bindings guard). Native acceptance requires the real
Windows build environment and the evidence listed in §4; unit fixtures do not substitute. Raw
output is recorded in `specs/05_BUILD_REVIEW.md` per the build-loop rules.
