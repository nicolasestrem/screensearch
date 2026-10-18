# 07 — Known Gaps

## 2026-10-18 — Project-note ownership identity: open product decision (owner: agent; maintainer may override)

- **A note that does not exist yet has no canonical identity.** `ReportFileOwnership` rows and the
  internal promotion intent/receipt slots for the one project note are now keyed by
  `promotion::note_identity`: the note's canonical Windows path, rendered with forward slashes and
  without verbatim/device syntax, so every equivalent respelling of an existing note (`C:/Vault`,
  `c:/vault`, `C:/Vault/.`, `C:\Vault`, `\\?\C:\Vault`) resolves to the same row and the same KV
  slot. When `std::fs::canonicalize` fails — the note is absent, or the volume cannot be opened —
  the identity falls back to the same rendering of the requested spelling, which removes duplicate
  and trailing separators and `.` components but does **not** fold case and does **not** resolve
  `..`. Two case-only spellings of a *missing* note therefore get different fallback identities.
  That is the one unspecified product decision here, recorded instead of guessed: the alternative
  (fold ASCII case in the fallback) could make two distinct files in an NTFS directory marked
  case-sensitive share one ownership row, which `03 §8c`/`§8e` forbid. No promotion can adopt bytes
  through the fallback: `prepare`/`confirm` read the note (or fail) before any ownership or intent
  decision, and a note that cannot be read is never appended to. Decide only if a real flow needs an
  identity for a note that does not exist yet.
- **Pre-identity row and record keys are read only under the unchanged spelling.** A row or record
  written by the previous build is keyed by the forward-slash path spelling, so `ownership` reads the
  identity key first and then that exact spelling, and `read_record` does the same before checking
  the record's stored path against the identity. That still finds a legacy key while the output
  folder keeps its spelling (the common upgrade), but a legacy row keyed by an *old* spelling is
  unreachable once the user respells the folder, and only the identity-keyed row this build writes
  covers the note from then on. Closing that would need a table-wide rescan of `report_files` or a
  one-time rewrite of legacy rows, neither of which this review fix introduces; it is deferred rather
  than guessed. The stale spelling-keyed row is also left in place (no store-level delete exists and
  no schema change is introduced). `owner_conflict` still compares source and live hash either way,
  so a row that does not describe the live bytes refuses an append rather than authorizing one.

## 2026-10-18 — Weekly per-item citation check and retention provenance

- **A weekly "day claim" is a top-level `- ` item naming a consumed local date.** Every
  top-level `- ` item under `Activity`, `Explicitly evidenced decisions`, `Open questions` and
  `Next steps` must cite at least one token: a consumed daily run ID, or a frame or session ID
  recorded by that day's saved report. An item naming a consumed date must cite *that* day's
  token, not another day's. Prose lines (window bounds, banners) are not items and are not
  checked. Items are the promotable unit, so a section with no evidence must say so in prose,
  not as a bullet; the weekly framing now states that. `03 §8e` does not define citation syntax,
  so a whole-number frame/session match counts (`frame 101`, `[101]`, `101, 102`) and a bare
  number is accepted even when it was not meant as a citation.
- **A retained-prose day keeps its recorded frame IDs.** `WeeklyMaterial::Saved` carries the
  saved daily's `frame_ids` into the weekly's `frame_ids`/`frames_summarized` (the frontmatter
  invariant requires `frame_ids.len() == frames_summarized`), so the note and a later
  promotion's `frames —` line keep provenance the source daily still records. Those IDs are not
  re-read evidence: `frames_sampled` still counts only Group frames, and `truncated` stays true
  when recorded evidence could not be read. The `SavedReport` field shape is unchanged; for
  weekly runs `frame_ids` reads as "frames the synthesis rests on", including a consumed day's
  recorded frames it did not re-read.

## 2026-10-18 — Saved-report export footer dates use the run's persisted zone

- **A saved run's exported footer dates its window in `run.timezone`, not the computer's
  current zone.** `MemoryPanel` builds the footer scope with `savedRunFooterScope(run)`, so
  the half-open UTC `[window_start, window_end)` is labeled in the zone persisted with the
  run (`03 §8c`: a later machine-timezone change does not move it) and the last covered
  instant stays `window_end - 1`. A Tokyo daily window therefore exports the one
  Tokyo-local date in the copied/downloaded markdown, not the two UTC-local dates the
  machine happens to be in. The persisted `local_date` stays the source for the report
  header's date, and `requested_range`/`window_local_dates` are not re-derived.
- **On-demand reports and Session recap keep the browser-local span.** Their footer scope
  carries no persisted run zone (`ReportFooterScope.timeZone` is absent), so
  `absoluteDate` without a zone is unchanged for them and for the
  Timeline/Insights/CapturesTrend labels. A valid IANA zone is assumed for a saved run: the
  backend validates the identifier before persisting it and the run header already formats
  `window_start`/`window_end` with it.

## 2026-10-18 — Post-install promotion conflict receipt

- `PromotionReceipt.hash` is populated when the promotion's bytes are confirmed
  installed. `conflict: true` plus a hash means the append landed despite a
  concurrent-swap/displaced-version verification conflict (`03 §8c`); the
  recovery copy is retained and the UI reports written, not nothing written.
  A conflict without a hash remains a no-append result. Ownership is reconciled
  before returning the installed receipt, not silently on a later call.
- A retained recovery `.md` in `Projects/` counts as a second note. The existing
  exactly-one-note guard therefore blocks another promotion until the user
  compares and moves/removes that copy; the dialog now makes this explicit.
  No automatic removal of an unmatched recovery copy is introduced.

- **`SavedReport.daily_run_ids` rides the saved-result wire surface.** `ReportDetail.result` embeds
  `SavedReport` verbatim, so the new field also appears on `GET /v1/reports` detail and the IPC
  run payload, alongside the `kind` addition `§8e` names as the deliberate migration change. It is
  report provenance about what the run consumed and no gate was needed to read it, but the
  surface change is recorded here rather than left implicit; if the projections must carry only
  the `§8e`-named fields, the fix is to strip it when building `ReportDetail`.
- **Weekly PARTIAL banner names the sampling cause even when retention caused the gap.** The
  `truncated` flag mixes context-budget cuts with recorded evidence that could no longer be read,
  so a week whose daily reports recorded evidence since pruned is told sampling was bounded while
  its status and missing-day list stay correct. The banner cause should come from the unreadable
  evidence count if this wording is worth another pass.

## 2026-10-18 — F3 G3+G4 implementation readings (owner: agent; maintainer may override)

- **`window_local_dates` describes the window, not the covered days.** A weekly note's frontmatter
  lists every local date whose day period intersects `[window_start, window_end)` — seven for a
  complete ISO week, DST weeks (167/169 h) included. A day without complete saved coverage stays in
  `missing_local_dates` and the PARTIAL banner; the field itself never claims a day was covered.
- **A `NO_DATA` week writes neither note nor index.** Both destinations get a skip receipt reading
  "A week without saved daily evidence writes no weekly note (NO_DATA)." G2-era runs that already
  carry skip receipts are not retro-written: Regenerate on such a run creates a linked revision,
  which is the only way that week acquires a note.
- **An index entry links only owned, hash-verified daily notes.** `days:` resolves the frontmatter
  `daily_run_ids` to local dates oldest-first and links a date only when that run's owned note
  passed the scan's hash and provenance checks; otherwise the date stays plain text. The whole line
  reads `- [2026-W42](Weekly/2026-W42.md) — OK — <topic> — days: [2026-10-12](Daily/2026-10-12.md),
  2026-10-13`. A retention-pruned run keeps its link when its hash/provenance-verified owned
  daily note survives: the canonical `Daily/YYYY-MM-DD.md` path supplies the date before any run
  lookup. A surviving run without a verified note contributes only a plain-text date; if both
  are absent, there is no day to display. Equal-date dedup prefers a verified link over plain
  text. The weekly note's `daily_run_ids` is never rewritten to match the index.
- **Index ordering (the open `YYYY-Www` position) is chronological, newest first.** Entries sort
  descending on `{date}/{relative_path}`; a weekly row's date is the window's **last** local date, so
  the week's row opens its own block, and the `Weekly/` > `Daily/` path comparison keeps it above a
  daily entry of that same date. The week label is never a sort key.
- **The weekly note adds no banner of its own.** `weekly_note` writes the saved markdown verbatim
  under `# YYYY-Www`; `save_weekly_execution` already prepended `weekly_partial_banner`, so a second
  banner would state the week's gaps twice.
- **Promotion works at bullet granularity.** One selectable unit is a top-level `- ` bullet of the
  saved report, mapped by source heading: `Explicitly evidenced decisions` → `Decisions`,
  `Open questions` → `Open questions`, `Next steps` → `Next steps`, `Activity` → `Log`. Items are
  copied verbatim (the text after `- `); a selection that no longer matches the saved markdown is an
  error, never a silent drop. UI and backend share that reading of the markdown (headings matched
  case-insensitively, closing hashes tolerated, fenced code skipped), so backend validation cannot
  reject an item the UI offered.
- **The entry date is the promotion date, fixed at prepare.** `prepare_promotion` renders the local
  date in the run's own saved timezone and the confirmed proposal carries it; `confirm_promotion`
  rebuilds the blocks from that carried date (canonical `YYYY-MM-DD`) rather than the clock, so the
  heading written is exactly the heading previewed.
- **One `### date` block per touched section, in one atomic write.** A single promotion contributes
  one block per touched section, all items for that section sharing one provenance footer; the
  blocks are spliced by `merged_content` and written once by `write_owned` under the selection-time
  hash. A block is `### YYYY-MM-DD`, a blank line, the verbatim items, a blank line, then one italic
  footer, e.g. `*Source: weekly synthesis run R; daily runs A, B; frames 101, 102; sessions 7, 8.*`
  — the `daily runs` clause appears only for a weekly source, ids are sorted and deduped, `—` stands
  in for an empty list, and beyond 20 ids the list ends `, … (N total)`. There is no per-section
  write and no delivery row, so a stopped app leaves either the old note or the complete promotion,
  and untouched sections keep their bytes.
- **The project note is user-created and default-empty.** `create_project_note` writes a zero-byte
  `<slug>.md` (slug `^[a-z0-9][a-z0-9_-]{0,63}$`) and refuses when any note already exists; the four
  `## ` headings are created lazily at EOF in canonical order (Decisions, Open questions, Next
  steps, Log) by the first append. Zero notes is "create one first"; more than one `Projects/*.md`
  is an unsupported state naming the paths, never a silent pick.
- **Known edge, documented not fixed:** a consumed day whose daily note was later rewritten by a
  linked revision renders as a plain-text date. The scan keys owned daily notes by the note's
  frontmatter `run_id` — the revision's id — while the weekly note records the base run it consumed,
  so the lookup misses and the day is not linked. The date and the run id stay correct; only the
  link is lost.
- **`WRITE_FAILED` is the weekly writer's guard, not a reachable status.** `weekly_note` exports only
  `OK`/`PARTIAL`, so a hypothetical `BLOCKED`/`FAILED` weekly run that somehow carried a saved result
  would surface as a write failure rather than a skip receipt. The engine never saves a result for
  those outcomes, so the branch is unreachable today and kept as a guard.
- **Promotion validates the current single-note target, not a client-selected filename.**
  `confirm_promotion` re-scans `ScreenSearch/Projects`, requires exactly one existing note and
  requires the proposal's path to match it, with a nonnull selection-time hash. It never creates
  a missing target. Missing/multiple notes and stale paths are explicit errors/conflicts;
  `ReportFileOwnership` is saved only after a successful write, so a conflict leaves the previous
  row untouched. Creation and confirmation share an in-process async lock spanning scan/create
  and ownership-check/append/save respectively; the existing single-instance app is the boundary.
- **Confirmed promotion recovery uses bounded per-note metadata, not stored previews.** Before
  appending, confirmation durably records the exact path, source/run, old/new hashes and a digest
  of the selected items/date in internal settings KV. It contains no selected or note text. A
  landed append is reconciled to its original ownership identity before normal hash checks or
  loading a possibly pruned run; unchanged pre-write bytes discard an interrupted intent, while
  unrelated bytes stay a conflict. One last committed receipt per target makes an exact retry
  idempotent, including a lost response; a new explicit promotion with a fresh snapshot remains
  allowed. Folder changes cannot replace another target's pending intent. Preview may settle
  metadata from a previously confirmed append, but never appends note bytes. IPC work runs in an
  owned task so cancellation of its response does not abandon the file/SQLite critical section.
  No schema migration, public setting, HTTP/MCP surface or persistence of rejected proposals.
- **Ownership is a write guard, not a history or garbage-collection key.** An initially unowned
  note may be adopted on the first confirmed promotion. Both prepare and confirm compare an
  already-owned note's current bytes against its persisted hash and require the same source
  identity; an edit before prepare, a forged proposal hash or a source reset does not re-adopt
  it. Such cases stay `FILE_CONFLICT`. The row's `run_id` names the most recent successful
  promotion, not the only report represented in the ledger; the row outlives run pruning.
- **Section matching ignores fenced content and tolerates case/closing hashes.** Source parsing
  and destination insertion track the fence marker and opening-run length: a shorter run, a
  different marker or a marker followed by non-whitespace does not close it. Destination entries
  use the first matching real section when headings repeat; renamed sections remain distinct.
  If EOF is inside an unclosed fence, insertion is before its opener, never inside the code.
  Existing bytes, line endings and duplicate headings remain untouched. Duplicate saved bullets
  remain independently selectable and are preserved up to their persisted occurrence count;
  requesting more copies is an error, not fabrication or silent deduplication.

## 2026-10-17 — F3 G2 implementation readings (owner: agent; maintainer may override)

- **Evidence without saved dailies is refused, not reported.** `§8e` defines `NO_DATA` only for
  "no saved dailies **and** no usable evidence" and PARTIAL for a partly covered week, but says
  nothing about a week that has raw evidence and zero saved dailies. The engine stops that case
  with `GENERATION_FAILED` (before `resolve`) rather than labelling it `NO_DATA` or synthesising
  an uncited narrative. One branch to change if the contract later defines it.
- **A weekly input is a single-local-day daily that owns its date.** A saved daily credits a date
  only when it is not a manual date-range run, was saved in the weekly run's own zone (persisted
  zone string equality, so `UTC` and `Etc/UTC` are not unified), its persisted `local_date` is
  that date, and its window lies entirely inside that local day. Multi-day range runs and dailies
  saved under a previous zone are not consumed and leave their dates missing; a week whose only
  dailies are of that kind is refused like a week with none.
- **Weekly prose must cite at least one consumed daily run id** (matched as a whole token),
  or the run fails with `GENERATION_FAILED`. `§8e` asks for the daily run of **every** day the
  prose describes; enforcing that per day would require knowing which days a passage
  describes, so G2 enforces only the existence of a real citation. The check also cannot detect
  additional invented ids. Per-day enforcement is a G3 review item when the note structure makes
  day sections explicit.
- **`missing_local_dates` counts a partial or truncated daily as a missing day**, since only
  complete saved coverage establishes a covered date (`§8e` PARTIAL).
- **Dailies and their revisions are all consumed** and all recorded in `daily_run_ids`; `§8e`
  does not define revision precedence for the weekly input, so none was invented. G3/G5 should
  decide whether a weekly may cite a revision rather than the base run of the same day.
- **`daily_run_ids` is persisted on `SavedReport`** (new optional field, no schema change), so a
  weekly report can be audited against the dailies it consumed.
- **Weekly runs write no note and send no Telegram digest in this increment**: the app records a
  skip receipt instead of emitting a daily-shaped note or a daily-format digest. G3 owns the
  weekly note/index and G6 the weekly digest format.
- **No user-visible weekly surface yet**: enqueue exists internally with `kind` =
  `weekly_synthesis` and the explicit UTC window; Settings controls, Run now and catch-up remain
  G5.

## 2026-10-17 — F3 G1 implementation readings (owner: agent; maintainer may override)

- **Daily notes keep frontmatter schema 1.** `03 §8e` requires weekly notes to write schema 2
  and schema-1 notes to keep parsing; it does not move daily notes. Daily output stays
  byte-identical; schema is stamped from the carried fields (any v2 field ⇒ 2).
- **First eligible weekly window** uses the daily `§8c` rule "at or after activation":
  activation exactly at Monday 00:00 local makes that week eligible. One-character switch in
  `weekly_first_window` if strictly-after was intended.
- **Weekly catch-up scan-back bound is unspecified.** `§8e` fixes oldest-first and the
  4-complete-ISO-week cap but not the lookback; deferred to G5, not guessed in G1.
- **`kind` is not a dedup dimension.** The `(schedule_id, window_start, window_end)` partial
  unique key is unchanged per `§8e`; a daily and a weekly run cannot share the same schedule
  and window. G5 must therefore give the weekly schedule its own schedule ID (weekly windows
  never coincide with daily ones, but a shared ID must not be relied on).

## 2026-10-17 — F3 deeper-memory plan: open decisions (owner: maintainer) — **Resolved (Nico, 2026-10-17)**

Source: `docs/deeper-memory-f3-plan.md` §6. Specs `02`/`03` were silent on all of these; each was
answered by Nico (2026-10-17, GO — the F1-pilot contingency is waived). The original questions are
preserved below as history; the normative contract text is `02 §5i` + `03 §8e`.

- **Weekly kind token.** `ReportKind::Weekly` already exists (`crates/traits/src/ipc.rs:158`,
  `03 §8b`: on-demand trailing 7 local days). The scheduled ISO-week synthesis needs a distinct
  token; proposal `weekly_synthesis`. **Resolved:** the token is `weekly_synthesis`.
- **Persisted `kind`.** No discriminator exists on `ReportRun` (`crates/traits/src/memory.rs:110-132`),
  `report_runs` (`crates/store/src/schema.rs:455-469`) or the `GET /v1/reports` projections. Column
  vs payload field, and its default for existing daily runs, is a 12 → 13 migration decision.
  **Resolved:** an indexed column `kind TEXT NOT NULL DEFAULT 'daily'` on `report_runs`, via the
  forward-only 12 → 13 migration, exposed on `GET /v1/reports` — listing/filtering needs it and the
  migration is already required.
- **Weekly catch-up cap:** 4 complete weeks proposed. **Resolved:** 4 complete ISO weeks.
- **Weekly note headings:** reuse daily `Activity` / `Explicitly evidenced decisions` or new names.
  **Resolved:** `Activity`, `Explicitly evidenced decisions`, `Open questions`, `Next steps`.
- **Project notes:** one user-selected note (direction-plan F3 minimum); more than one is out of scope.
  **Resolved:** exactly one project note in this slice.
- **Weekly Telegram digest** under `03 §8d`: later increment or never. **Resolved:** a weekly digest
  with its own weekly-specific format under `03 §8d` — increment G6.

## 2026-10-16 — External-agent memory API P0 decisions and gates

- Nico authorized execution of the attached plan, replacing its draft/no-GO labels.
  The new contract is `03 §7f`; it describes authorized behavior, not completed work.
- **Resolved (Nico GO), P3 only — historical record preserved:** the question was whether to
  expose saved output while an existing run is queued/running for delivery retry, or to suppress it
  under the plan's unfinished-null rule; the recommendation was to mirror immutable saved-result
  presence. **Nico approved stored-presence exposure:** an immutable saved result is exposed
  immediately whenever it exists, including while delivery is queued or retrying; absent output is
  explicit null; stored `NO_DATA` stays factual; a malformed saved payload is a read error. The
  earlier "implementation remains blocked" hold is **superseded** for P3. Evidence/contradiction:
  `06` external-agent P0 entry. P3 runtime is being implemented on
  `feat/agent-memory-saved-reports`; verification pending, no P3 result claimed.
- Wire normalization is explicit in `03 §7f.6`: signed integral timestamps, present
  nullable fields, absent versus empty text, chronological global coverage extrema,
  duplicate query rejection, safe fixed new-read errors and unknown saved error-code
  to null. These are serialization/validation decisions, not broader capabilities.
- Existing `--url` remains an alias of canonical `--api-url`; both override the
  environment. Saved-report tuple high-water does not freeze status or membership.
- Store inventory supports existing KV metadata without migration only with reserved
  single/batch/delete guards, atomic pair setup and allocator-backed production
  inserts. Established missing metadata must not be repaired on a read.
- Native UI, three native wrapper archives, real cross-machine Windows-application
  traversal, full workspace regression and measured paging profiles remain open
  until recorded execution. TBX SSH is verified; Node 22/isolation preparation and
  independent contract reviews are in progress. No installed/private history used.

## 2026-10-15 — Memory/API execution gates (owner: maintainer + agent)

- C–E policies are approved in `03 §8c/§8d`; implementation verification is tracked in `05`.
  Product acceptance must cover diverse supported user setups, not a personal usefulness score.
- Before network release: real separate-machine HTTP authentication, local MCP,
  restart, disable/enable, Ask cancellation and inference bind evidence.
- Before automation acceptance: user-selected test folder, installed model, and explicitly
  approved Telegram test destination; no fabricated receipts or private-history defaults.
- Website source is not present in this repository; external privacy-copy review remains open.
- **Carried forward from the 0.4.1 arc** (resolved decisions and verification detail:
  `specs/archive/07_KNOWN_GAPS.v0.4.1.md`; owner: maintainer unless stated). Still open:
  separate-machine HTTP authentication and interactive Settings acceptance for the configurable
  bind; real installed-model report quality; an explicitly authorized live Telegram send;
  slow/unavailable-folder behaviour (OS file operations can finish after async cancellation, durable
  intents recover the outcome); rate limiting remains deferred (#89(b) — the 64-connection cap and
  30 s header deadline shipped); sparse-content sampling can need a larger frame budget and must never
  be read as proof of no data; the Windows credential read CodeQL alert was dismissed as a false positive on
  2026-10-15 (evidence in `05`); the remaining dev-tree braces advisory is tracked below.
- Schema 11 → 12 (0.4.1) is forward-only and there is no automatic pre-migration backup in the app;
  the 0.4.1 release notes tell users to back up the data folder first.

> Where the **spec is silent** and a decision was needed, plus **manual interventions** still
> required (with owner + deadline). The agent appends here instead of guessing (`04 §5`). Empty
> until the build hits a gap.

| # | Date | Gap (spec was silent on…) | Resolution / decision | Owner | Needed by |
|---|---|---|---|---|---|
| 72 | 2026-06-29 | **llama.cpp multimodal host-memory leak (build 9842).** ~149 MB committed host RAM leaked **per vision inference** (VRAM flat; isolated 60-frame test 2026-06-29: 6.8 → 16.3 GB committed). Freed only on process exit. | **Open (upstream).** Local mitigation shipped 0.2.1: the **sidecar recycle valve** (`sidecar.recycle_enabled` default true; `sidecar.recycle_rss_mb` default 0=auto, explicit clamp 8192–131072 MiB; the 8 GiB floor clears the 8B vision warmup baseline so it can't recycle-per-request) recycles llama-server at a committed-RAM ceiling. **The real fix is upstream** / a newer bundled llama.cpp build — re-run the 60-frame test when it lands. Shipped-mitigation detail + small-RAM behaviour: `06` #15 and the `08`/CHANGELOG v0.2.x archives. | agent | open (upstream; local mitigation ✅ 0.2.1) |
| 46 | 2026-06-24 | **Stall-abort can't cancel hf-hub's detached chunk tasks** (PR #26, Codex P1) — the model-download stall watchdog calls `task.abort()` on the outer fetch task, but `hf-hub` 0.4.3 starts its own per-chunk `tokio::spawn`s internally; aborting the parent drops their join handles without stopping them, so a chunk task on a (briefly) revived socket could still write the `.sync.part` after the UI reports `Failed`. | **Open (bounded, deferred):** the blast radius is contained — (a) resume reads the **committed counter** in the `.sync.part` trailer as the authoritative offset, so stale writes past it are re-fetched/overwritten next attempt (committed data isn't corrupted); (b) downloads are **serialized per lane**, so a user-initiated retry never runs a second `ModelDownloader` fetch concurrently. A real fix means replacing the high-level `ApiRepo::download_with_progress` with a cancellable chunked downloader we own (or forking hf-hub) — a larger change than this PR's scope. **Update (0.2.0 PR8):** the default path is now the chunked downloader, whose chunk futures are awaited *inside* the fetch task (`buffer_unordered`, no detached `tokio::spawn`), so `task.abort()` cancels every in-flight chunk — **no orphaned writers on the chunked path**. This gap now applies only to the single-stream hf-hub **fallback** (range-less servers), where the bounded mitigations above still hold. | agent | mostly resolved (chunked path); remains for hf-hub fallback only |
| 58 | 2026-06-25 | **OPEN — live multi-DPI / secondary-monitor `target_rect` acceptance check (PR3)** — gap #54 marks the `target_rect` DPI mapping *code* resolved, but the runtime behaviour can only be confirmed on real hardware: a `npm run dev` session spanning **≥2 monitors at different DPI scales**, verifying the foreground window's normalized rect maps to the correct monitor band (background/system suppression fires on the right monitor, never the wrong one). CI cannot cover this. Tracked as its own row so it stays visible **after** PR #32 merges and #54 reads ✅. | **Open (manual, owner = user/maintainer):** run the multi-DPI check on a real desktop session; a wrong rect can only *under*-suppress (recoverable via `include_chrome` + preserved `raw_text`), never silently lose content, so it does not block merge — but the check must be done before relying on background/system suppression on multi-monitor setups. Update this row to ✅ once verified. | user | open (before multi-monitor reliance) |
| 75 | 2026-07-03 | **Audio capture / transcription — deferred (0.3.0 arc, `02 §5c`).** The single most-requested capability in this product category, and explicitly out of 0.3.0. | **Deferred to 0.4.x by user decision:** mic capture, a Whisper lane, and consent UX are a full arc of their own. Not a spec silence — a scoped deferral recorded per the roadmap. | user+agent | 0.4.x candidate |
| 76 | 2026-07-03 | **Custom GGUF "bring-your-own-model" path — deferred (0.3.0).** The tinkerer escape hatch that replaces the retired Beta tier. | **Deferred:** small, but kept out of 0.3.0 so PR3 stays a pure subtraction (`docs/0.3.0.md`). | user+agent | post-0.3.0 |
| 77 | 2026-07-03 | **Proactive nudges / notifications — deferred (0.3.0).** "You've been on X for 40 min…" style push. | **Deferred, deliberately:** 0.3.0's where-was-i / marks surfaces are **pull-based only** — no notifications, no shame-analytics. Any push mechanism is a separate, opt-in design conversation (`02 §7`). | user+agent | post-0.3.0 |
| 78 | 2026-07-03 | **Marks in Recall reports — deferred (0.3.0).** Surfacing unresolved marks inside daily/weekly reports. | **Deferred:** reports stay as shipped in 0.2.x (`03 §8b`); a later refinement. | user+agent | post-0.3.0 |
| 79 | 2026-07-03 | **API write scopes beyond marks — deferred (0.3.0).** | **Deferred:** the v1 local API is read-only except `POST /v1/marks` (`03 §7c`, D11). | user+agent | post-0.3.0 |
| 82 | 2026-07-03 | **`jobs.kind` has no `CHECK` constraint (optional hardening; PR #70 gemini review).** The live `jobs.kind` column is `TEXT NOT NULL` with only a comment enumerating values (`crates/store/src/schema.rs:132`); a reviewer suggested a `CHECK (kind IN ('embed_text','vision_tag'))`. | **Declined for the 0.3.0 arc, recorded:** adding a `CHECK` to the `§4` DDL would (a) diverge the spec from the real, un-constrained schema, and (b) if treated as contract, force an unplanned **jobs-table rebuild** in PR4 (SQLite can't add a `CHECK` in place) — scope creep beyond D5 (which only drops the image tables + deletes `embed_image` jobs). Kept as opt-in future hardening: if ever added, it lands as its own forward-only migration (+1, populated-DB test, D15) that rebuilds `jobs`, not silently folded into PR4. | agent | optional / post-0.3.0 |
| 89 | 2026-07-04 | **PR7 v1-API scope residuals (recorded, not built).** Three items sit just outside the PR7 surface. | **Deferred, recorded:** (a) **non-JSON export formats** — `format=` accepts only `json` in v1 (anything else → 400); CSV/NDJSON are future formats. (b) **No rate limiting** on the local surface — 0.4.1 added a 64-active-connection cap and a 30 s request-header deadline on every listener (`03 §7c`), but there is still no per-request rate limit; acceptable behind a bearer token on loopback by default, revisit as the configurable bind widens the surface. (c) **`captured_at`-seeded export cursor** — the keyset cursor scans by `id` from 0 even for a *late* time window, so a huge history with a recent-only `from` still walks early ids (bounded, ordered PK scan; correct, just not optimal). A future optimization can seed the cursor from the first in-window id. | agent | post-0.3.0 |
| 92 | 2026-07-04 | **UIA circuit-breaker thresholds are named constants, not settings.** `BREAKER_CONSECUTIVE_BAD` (3) and `BREAKER_COOLDOWN_MS` (30 min) in `crates/uia/src/breaker.rs` govern the per-app UIA→OCR back-off, but are not exposed as `capture.*` settings, unlike other thresholds (`03 §3b` says thresholds live in settings). | **User-decided exception, recorded:** the 0.3.0 arc is surface-*reduction*; the user chose breaker-only protection with no new settings surface. The constants are conservative and self-documenting; promote them to settings only if real usage shows the default 3/30-min back-off is wrong for some workflow. **Update (0.3.3):** since `#93` now skips Chromium/Electron windows outright, the breaker only backstops *native* apps (and any browser whose class lookup failed), so its thresholds matter even less — no change warranted. | user+agent | post-0.3.0 (only if defaults prove wrong) |
| 95 | 2026-07-04 | **UIA client spawn vs. concurrent settings-save has a vanishingly-narrow residual race (PR #79 review follow-up).** `get_or_spawn_uia` now reads the *live* `config.budget` under the `respawn_gate` (was: the caller's pre-gate snapshot), so a single settings save landing during the capture-start → first-spawn window can no longer bake a stale budget into the new client — the realistic bug the reviewer found. The only residual is a *second* settings save whose `apply_settings` teardown interleaves exactly between this spawn's fresh config read and its slot store (both saves changing the budget within the ~ms spawn) — the teardown then no-ops on the still-empty slot and the mismatch persists until the next save. | **Fixed for the reachable case; residual recorded as accepted.** The residual needs two distinct human saves within a single client's spawn window (not reachable by a person) and self-heals on the next save (which tears down the now-populated slot). A full close would need to serialize teardown against the in-flight spawn (e.g. a config generation counter checked under the slot write lock) — disproportionate to a non-reachable micro-race on a latency-budget value. | agent | post-0.3.0 (only if it ever proves reachable) |
| 91 | 2026-07-04 | **Monitor hot-unplug mid-session stalls capture silently (PR9 audit finding).** The 3440×1440 primary display disconnected (DP power-save) during the PR9 live audit: one `WARN capture::wgc: capture: readback failed monitor=1 error=no frame available from the capture device`, then **no frames stored for ~4 min** despite foreground events, while `/v1/health` kept reporting `capturing:true`; a mark-hotkey press during the transition produced a frame but **no mark row and no visible toast** (silent loss), and later presses did nothing (requests into the stalled worker). | **Open (post-0.3.0 hardening), bounded:** a manual Deck STOP → START immediately restored frame flow (verified live); no data corruption. Fix direction: the capture worker should react to display-topology changes (`WM_DISPLAYCHANGE` / WGC item `Closed`) by recreating its sessions for the surviving monitors — or at minimum flip capture readiness to Error + toast instead of continuing to report capturing=true. Not a release blocker: environmental trigger, loud only in the log today, recoverable in two clicks. **Update (0.4.0):** sessions render such a stall **visible** — a session with a silent hole — which strengthens the case; but capture-worker topology handling is not sessions work, so it stays deferred as a **first-line 0.4.1 candidate, priority raised** (`docs/0.4.0.md` §2/§6). | agent | post-0.3.0 (first-line 0.4.1 candidate) |
| 103 | 2026-07-05 | **Settings search box — deferred (0.3.2 D6).** The two-tier Settings IA (PR5) should make search unnecessary; adding one now would paper over bad structure. | **Deferred by roadmap decision:** revisit in 0.4.x only if the restructure proves insufficient in practice. | user+agent | 0.4.x, only if the two-tier IA isn't enough |
| 104 | 2026-07-05 | **Token-level visual refresh — possibility recorded, out of 0.3.2 (D12).** PR4/PR5 are structural (layout + IA); the aesthetic direction (`UI_REFERENCE §1`: warm graphite, one bold accent, scanline identity) is untouched. | **Not planned — recorded so it isn't smuggled in:** a visual refresh is its own conversation that amends `UI_REFERENCE §1` first, then implements. Anything token/palette/typography-shaped inside 0.3.2 is a scope violation. | user+agent | unscheduled (own conversation first) |
| 106 | 2026-07-06 | **NavRail ghost/duplication glitch is a WebView2 GPU-compositor stale-surface artifact — not fixable in app code (0.3.2 PR4 Phase A stop condition).** Phase A proved the DOM is invariably clean (rail anchor count constant at 6 across route-cycle/scroll/reload stress; 0 anomalies; no virtualization/remount/transform on the rail) and CDP renderer screenshots are clean; the only app-level mechanism that moves the rail is a banner mount/unmount shifting the whole nav row vertically (measured +40 px for a 40 px banner). The artifact is the rail's pre-shift painted **display surface** persisting after the shift — a WebView2/Chromium compositor class (cf. WebView2Feedback #2421, VS Code #113188: ghost renders with clean DOM), invisible to DOM inspection and CDP screenshots, intermittent (tied to slow-subsystem-init banner transitions). | **Stop condition fired (per `docs/0.3.2.md` PR4 + maintainer decision 2026-07-06): STOP on a direct fix; apply the cleanest CSS mitigation + keep this row open pending field recurrence.** Mitigation shipped in PR4 Phase B (preventive hardening): NavRail gets its own stacking/compositing boundary (`relative` + `z-rail` [consumes the previously-unused `--z-rail` token] + `isolation: isolate`) so it owns an isolated surface that repaints atomically on the shift, and `main` gets `contain: paint` (paint only, never layout) so the scrolling content layer cannot share/bleed a surface with the rail. Cannot be CI-verified (not observable via CDP); re-check on the physical display and on WebView2 runtime updates. Flip to ✅ only if a runtime update fixes it upstream or the mitigation is confirmed durable in the field. | user+agent | open (upstream class; app-side mitigation shipped 0.3.2 PR4) |
| 107 | 2026-07-07 | **User-editable taxonomy override file — deferred (0.4.0 D6, `03 §7e`).** The AI-tool / meeting recognition taxonomy ships as a versioned in-repo data file compiled into the app; adding a tool = editing that file this arc. A user-supplied override (so a user can teach it about their own tools without a rebuild) is not built. | **Deferred by roadmap decision (D6):** the in-repo file is the mechanism this arc — small, auditable, no new settings/loader surface. A user override file is a later refinement; revisit only if real use shows the built-in taxonomy is too rigid. | user+agent | 0.4.x, only if the in-repo taxonomy proves too rigid |
| 108 | 2026-07-07 | **Dedicated Sessions NavRail route — possibility recorded, out of 0.4.0 (D13, `docs/0.4.0.md`).** Sessions surface **inside** existing routes (Timeline bands + drill-in, Moment, Deck); the arc adds **no** new NavRail entry. | **Not planned — recorded so it isn't smuggled in:** a dedicated Sessions route is its own 0.4.x conversation, opened only if live use shows the in-route surfaces are insufficient. Anything route-shaped (a new NavRail item) inside 0.4.0 is a scope violation. | user+agent | 0.4.x, only if the in-route surfaces prove insufficient |
| 109 | 2026-07-07 | **`browser_url` is dormant in production capture — deferred (predates 0.4.0; `crates/kernel/src/capture_loop.rs:130`, `resume.rs`).** Capture persists `browser_url: None` (populating it needs a UIA read); `§7b` where-was-i already implements the browser-domain refinement per-contract but it never fires. The 0.4.0 `browser-ai` taxonomy seed (`03 §7e`) therefore keys on `app_hint` + window-title patterns, with the claude.ai/chatgpt.com/gemini.google.com **domain match dormant until this lands**. | **Deferred — not 0.4.0 sessions work.** Sessions recognize browser-AI from stored metadata today (title/app hint). Populating `browser_url` (a UIA URL read on the browser's address bar) is an independent capture enhancement that would sharpen both where-was-i (`§7b`) and browser-AI/domain recognition; scope it as its own item if PR2 evidence shows title/app-hint recognition is too weak. | agent | 0.4.x, only if title/app-hint browser-AI recognition proves too weak |
| 112 | 2026-07-10 | **Accepted limitations of the task-level grouping redesign (recorded as decisions, not silent defaults; `06` #27).** Three items the two-pass segmenter cannot fully solve on app metadata alone: (a) **the invisible back-to-back same-kind tool handoff** — when one AI tool hands off to another with no gap and the second tool's sustained evidence arrives long after the true boundary (the old tool's frames continue past the true boundary and the new tool's first sustained run appears much later, e.g. when the second tool runs on a remote machine the capture sees only as a remote-access client), no interior split is reachable; (b) **`HOST_PRECEDENCE` and `IDENTITY_QUALIFY_MS` are one-data-point constants** (frame-majority attribution provably picks the wrong tool when a desktop AI app out-frames its own terminal session, so terminal > ide > desktop > browser is used; correct in every decidable case here but n small); (c) **spinner-prefix specificity is untested outside claude-code** (#111). | **Maintainer decisions (2026-07-10):** (a) the handoff class emits **one merged AI session with exact outer endpoints**, attributed to the precedence-winning tool; the interior split is declared out of heuristic reach, with a user **mark-split** or within-session sub-tool attribution as the recorded future channel (which of the two to build is a later product call). This structurally caps recall + tool accuracy on every such day — accepted. (b)/(c) ship as named constants / terminal-stem-ANDed matching with a standing **re-verify-on-every-new-labeled-day** instruction; revisit with more days (a Tuesday meeting day + a desktop-agent-first day would stress them). | user+agent | 0.4.0 PR2 (accepted; revisit with more days) |
| 115 | 2026-07-10 | **Ground truth should be reconstructed from the capture, not recalled; non-work fullscreen activity is uncapturable (PR2 methodology finding).** (1) In PR2, labels drawn from recollection proved unreliable — a rough recollection of a day's shape did not match what the capture recorded (unsurprising for a recall-augmentation tool). Ground truth must be read off `digest.md` (what the capture shows), with the agent pre-drafting data-grounded labels for the maintainer to sanity-check; this also caveats the earlier recall-assisted labels. (2) A request to surface **non-work activity as its own session** cannot be met by the current pipeline: a fullscreen app (e.g. a game) is diff-gated to near-zero frames, and there is no non-work taxonomy category, so the segmenter cannot recover it. | **Open — recorded (2026-07-10).** (1) Shift the labeling protocol to digest-first (in effect going forward). (2) Non-work sessions need a taxonomy category (likely `kind='focus'` or a new kind) AND capture that survives fullscreen (a capture-side change, cf. `#91`); both out of the current PR2 harness. Fold into the #114 session-model conversation. | user+agent | 0.4.0 (labeling protocol now; non-work session = design conversation) |
| 113 | 2026-07-10 | **Anchorless-focus density gate: honesty tradeoff (`06` #27; the invisible-suppression risk).** To stop fragmented low-density blocks (email/admin scatter at ~1 frame/min) from emitting false "focus" session bands, anchorless focus sessions must additionally pass the named `SESSION_FOCUS_MIN_DENSITY_FPH` constant (frames/hour). This suppresses a class the maintainer labels **no-session** — but the flip side is invisible by design: a **sparse legitimate** session (long reading/video/monitoring under diff-gated capture, which produces few frames) would also emit nothing, and the user would never see why. The gate also does not catch a second observed FP class (a denser leisure-browsing block passes it), so precision has a known honest ceiling. | **Gate ON at 90 frames/hour by maintainer decision (2026-07-10), with guardrails:** `0` disables it; **AI/meeting sessions are exempt** (a real short high-signal AI burst must survive); the residual leisure-block FP is **accepted** and routed through the D11 low-confidence field in PR4 rather than a knife-edge density knob; and the false-suppression side is **validated on a fresh labeled day containing a sparse legitimate session before the default freezes** (Phase C fresh-day gate). Recorded so the suppression is a decision, not a silent default. **Update (v0.4.0 PR7): the density gate shipped as the named constant `focus_min_density = 90 fph` (`06` #28 demoted it from a knob to a constant); the sparse-legitimate false-suppression validation on a fresh labeled day was not evidenced, so it stays a standing honesty item carried into 0.4.x. Update (usage review 2026-08-01 §6.7): an 11-day sweep found 30 fph dominated 90 (F1 0.148 vs 0.125), so the named constant is now 30 fph; the honesty item remains because the sweep does not make suppressed sessions visible.** | user+agent | 0.4.x (shipped constant; sparse-legitimate false-suppression check still owed) |
| 116 | 2026-07-10 | **Accepted limitations of the concurrent session model (recorded as decisions, not silent defaults; `06` #28, resolves #114).** Three items the per-track segmenter cannot solve on app metadata alone: (a) **two concurrent instances of the same tool fold into one track** — the taxonomy id is the track key and app metadata cannot distinguish two `claude-code` terminals (or two `codex` runs), so simultaneous same-tool work is one session; (b) **`browser-ai` is a single taxonomy id**, so concurrent claude.ai + chatgpt.com tabs are one track, not two (sharpens only if per-tab `browser_url` capture lands, `#109`); (c) **genuinely-overlapping labeled shapes are thin** — the two tuning days (07-07/08) carry serial labels; overlap is validated on the held-out days (07-09/10) exactly once before the D9 gate. | **Maintainer decisions (2026-07-10):** (a)/(b) accepted as identity-granularity limits — instance/tab separation would need per-window or per-URL capture that does not exist today; recorded, revisit only if real use shows same-tool concurrency matters. (c) mitigated by the fresh-day gate (params frozen on the tuning days before the held-out days are scored) plus a standing **re-verify-overlap-on-every-new-labeled-day** instruction (a multi-agent Tuesday and a meeting-through-AI day would stress it most). Concurrency **removes** the #112(b) host-precedence pathology from the shipped path (kept only in the serial `--algo grouped` baseline). | user+agent | 0.4.0 PR2b (accepted; revisit with more days) |
| 117 | 2026-07-10 | **Foreground-only capture cannot represent concurrent AI agents; the taxonomy is a moving target (PR2b held-out finding — the 07-10 4-agent demonstrator).** The 2026-07-10 capture caught a genuinely concurrent session ("codex in terminal + claude in terminal + codex desktop + claude desktop, all running — a stable session focused on 4 things"). Three capture/taxonomy facts cap what any segmenter can recover from it, independent of segmenter quality: (a) **capture is foreground-only + diff-gated** — a background agent (codex-desktop) produces almost no captured frames until it is foregrounded, so true concurrency exceeds the record (cf. `#91`, `#115`(2)); (b) **spinner ambiguity** (`#112c`) — codex-CLI and claude-code are both spinner terminals, so the taxonomy recognizes both as `claude-code`, misattributing the codex-terminal work; (c) **the codex desktop app was renamed by OpenAI to "ChatGPT" ~4 h before this capture**, so it is captured under `app_hint = "chatgpt"` (not `codex`) and the `codex` taxonomy entry misses it (while `"ChatGPT Classic"` = the actual, rarely-used ChatGPT). Net: on 07-10 the segmenter recovers ~one `claude-code` track (absorbing the codex-terminal frames) plus a thin `claude-desktop` track; codex is effectively invisible. | **Recorded; shapes the D9 gate and the 07-10 disposition (maintainer, 2026-07-10).** 07-10 is used as the **concurrency-capture-limit demonstrator, not a D9 scoring day** — 07-09 is the primary held-out day; 07-10 shows the honest recall/accuracy ceiling the capture + taxonomy impose, so the segmenter is not penalized for a capture gap. The gate must not demand recall the capture cannot provide. **PR4 taxonomy decision (2026-07-10): map the exact `chatgpt` app stem to `codex`, with an explicit case-insensitive `ChatGPT Classic` title exclusion.** Both production and harness parsers honor the exclusion; the shipped-parity fixture covers the renamed and Classic cases. The codex-CLI-vs-claude-code spinner ambiguity and any capture-side background/multi-window fix remain open under `#112c`/`#91`; no new product decision was guessed here. | user+agent | 0.4.0 PR4 (rename resolved; capture/spinner limits remain) |
| 120 | 2026-07-11 | **Update-available UX is easy to miss and its link lands on a stale Settings position (0.4.0 PR7 D16 live-run finding).** The update-ready signal is a low-contrast NavRail-footer text link (for example `vX.Y.Z update`); activating it deep-links into Settings where the restart-to-update control used to live, but the 0.3.2 two-tier IA moved it, so the click lands on unrelated settings (monitors / hotkeys). | **Out of 0.4.0 scope; filed as GitHub issue #108, a 0.4.1 candidate.** Direction (non-binding): a higher-visibility update-available treatment, and a direct path to a focused update view (release notes + a single restart-to-update action) instead of a Settings deep link. Affects 0.3.2+ (`ui/src/components/shell/UpdateIndicator.tsx`, unchanged through 0.4.0). | user+agent | 0.4.1 candidate (GitHub #108) |
> Resolved v0.1.0-era rows (2026-06-21..2026-06-24) → `specs/archive/07_KNOWN_GAPS.v0.1.0.md` (ids preserved).
> Resolved + retired 0.2.x rows, and the resolved-engineering-decisions list → `specs/archive/07_KNOWN_GAPS.v0.2.x.md` (original `#N` ids preserved).
> Resolved + decided 0.3.0-arc rows → `specs/archive/07_KNOWN_GAPS.v0.3.0.md` (ids preserved).
> Resolved 0.3.1-era rows (#94/#99/#101) → `specs/archive/07_KNOWN_GAPS.v0.3.1.md` (ids preserved; #94's open remainder = live row #100). The live table holds only still-open rows and standing deferrals.
> Resolved + shipped 0.3.2-arc rows (#83/#96/#97/#100/#105) and the resolved 0.1.1 TODO block → `specs/archive/07_KNOWN_GAPS.v0.3.2.md` (ids preserved). The live table holds only still-open rows and standing deferrals.
> Resolved 0.3.3 row (#93 — first-UIA-walk-hangs-Chromium, reversed → window-class skip) → `specs/archive/07_KNOWN_GAPS.v0.3.3.md` (id preserved). Standing deferrals #92/#95 carry a 0.3.3 update note but stay live.
> Resolved 0.4.0-arc rows (#98/#102/#110/#111/#114/#118/#119) → `specs/archive/07_KNOWN_GAPS.v0.4.0.md` (ids preserved). #102 closed GitHub issue #88 at the v0.4.0 release. The live table holds only still-open rows and standing deferrals.
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/07_KNOWN_GAPS.v0.4.1.md` (resolved decisions; open gates carried forward above). Row #50 (scheduled / saved reports) shipped in 0.4.1 and moved there (id preserved).

Manual steps still required (e.g. signing certs, first-run model download, CI secrets):
- **First-run model download** — embedding models (P3) auto-download via fastembed into
  `<app-data>/models/fastembed`. **P4:** the `llama-server` Vulkan binary auto-downloads into
  `<app-data>/sidecar/llama` at launch (off-thread; `sidecar` readiness Initializing→Ready/
  Unavailable), and the vision/answer GGUF (+ mmproj) auto-download via `hf-hub` into
  `<app-data>/models/<lane>/<tier>` **lazily on first request** (gaps #14/#15). Nothing is bundled.
  No download-progress %% yet (readiness is coarse Initializing→Ready); a percentage is a P5 nicety.
- **Code-signing certificate** for the installer — P5 packaging. Recommended path for this MIT OSS
  project (cheapest → most turnkey): **SignPath Foundation** (free Authenticode signing for
  qualifying OSS) → **Azure Trusted Signing** (~US$10/mo, Microsoft-run, builds SmartScreen
  reputation) → **Certum Open Source Code Signing** (cheap annual cloud cert for OSS devs). Note:
  since 2023 all OV certs require a hardware token or cloud HSM, so a plain file-based cert is no
  longer available; budget for that. Decide owner before P5.
- **Updater signing keypair (0.3.2 PR2, D2 — RELEASE BLOCKER for `v0.3.2`):**
  - ✅ **Keypair generated (0.3.2 PR2, 2026-07-05).** minisign keypair, **fingerprint
    `27E1C773C0BDF81E`**; the **public** key is committed in `src-tauri/tauri.conf.json`
    (`plugins.updater.pubkey`). The **private** key was written **outside the repo** at
    `%USERPROFILE%\.tauri\screensearch-updater.key` (no password; the file *is* the secret) and is
    never committed (`.gitignore` also blocks `*.key`).
  - ✅ **Maintainer custody — satisfied 2026-07-06 (0.3.2 PR6):**
    1. CI secrets set on `nicolasestrem/screensearch-v2c` (maintainer delegated the command to the
       agent in-session): `TAURI_SIGNING_PRIVATE_KEY` = the full key file, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
       = empty. Verified via `gh secret list` (both names listed, updated 2026-07-06); the PR6
       release-pipeline dry-run exercised them end-to-end (signed installer + `.sig` + `latest.json`).
    2. **Offline backup: user-attested 2026-07-06** — the maintainer confirmed during the PR6
       session that the private key is copied to a maintainer-controlled location off this machine.
       (The backup was the release blocker; it is recorded here per D2.)
  - Written-down consequences: **losing the private key strands every installed copy** on manual
    downloads again. A **leak** forces **key rotation**: generate a new keypair, bump the pubkey in
    `tauri.conf.json`, and ship one **manually-downloaded** release with the new key — existing
    installs cannot auto-update past a rotation (they still trust the old key), so the rotation release
    is announced as a manual download, exactly like the v0.3.2 genesis.
  - This minisign key is the *updater* signature only — **not Authenticode**; the code-signing
    certificate bullet above stays open regardless (`03 §11b`, `03 §11`).
- **Pre-migration DB backup (0.4.0 PR3, D5 — RELEASE-BLOCKER-CLASS):** before any 0.4.0 build is
  ever pointed at the live database, a **dated copy of `screensearch.db` must exist outside the app
  data dir** (`%APPDATA%\app.screensearchv2c.desktop\`). The live DB is simultaneously the **only
  production install** and the arc's **PR2 ground-truth dataset** — losing it loses the arc's
  evidence, so this is not rollback theater but the proportionate safety net (`docs/0.4.0.md` D5).
  Owner = maintainer. Re-verified present at the PR7 **D16** first-auto-delivery gate before `v0.4.0`
  is tagged.
  - ✅ **Gate 0 satisfied at 0.4.0 PR3 (2026-07-10).** Maintainer attested in-session that a dated
    pristine copy of `screensearch.db` exists outside the app data dir. PR3's build **never opened the
    live DB** (its mtime stayed `07/10/2026 10:55:06`, 183,693,312 bytes, throughout; the app was not
    running) — all PR3 tests run on in-memory fixtures, and the migration was proven on a separate
    **throwaway** copy (live `.db` + `-wal` + `-shm` → isolated scratchpad) via the env-gated
    `live_db_copy_migrates_to_v11_fast_and_clean` test: **3036 frames migrated 10 → 11 in ~146 ms**,
    fk clean, sessions/artifacts empty, zero backfill (output in `05` Pass 4). The throwaway copy (now
    schema 11) was deleted. `npm run dev` was **not** run on the PR3 branch. Re-verify the
    pristine copy is present again at the PR7 **D16** gate.
  - ✅ **Re-verified present at the PR7 D16 gate (2026-07-11).** `screensearch-2026-07-10.db`
    (185,266,176 bytes, schema 10, `PRAGMA integrity_check` = `ok`, 3,114 frames) exists outside the
    repo and outside the app data dir; a fresh `screensearch-2026-07-11.db` was written by the harness
    `backup` subcommand (`VACUUM INTO`, source/copy 3114/3114 frames match). Today's separate fresh
    install DB was preserved aside before the D16 bed was restored. The D16 first-auto-delivery gate
    passed on that bed (`05` Pass 17): 10 → 11 migration clean, 9 sessions over pre-existing history,
    frame-level features unchanged.
- **Usage-signals reading procedure (0.4.0, D15 — no in-app telemetry, ever):** the app never phones
  home beyond the signed-updater manifest fetch (`02 §7`, `03 §11b`). To answer "is anyone else using
  this?", read **passive repo-side signals** (owner = maintainer; needs `push` access for traffic):
  - Release-asset download counts —
    `gh api repos/nicolasestrem/screensearch-v2c/releases --jq '.[] | {tag: .tag_name, assets: [.assets[] | {name, download_count}]}'`.
    The **`latest.json`** asset's count ≈ update checks across all updater-capable installs; the
    **installer `.exe`** count ≈ manual downloads.
  - Clones / views (14-day rolling window) —
    `gh api repos/nicolasestrem/screensearch-v2c/traffic/clones --jq '{count, uniques}'` and
    `.../traffic/views --jq '{count, uniques}'`; plus stars/issues as coarse interest signals.
  - **Caveat:** the maintainer's own launches (the updater checks on every launch) inflate the
    `latest.json` count, so read it against a known personal baseline. An optional convenience script
    in `scripts/` is a later follow-up — it runs on the maintainer's machine, **never** in the app
    (not built in PR1, which is specs-only).

## Maintenance follow-ups — 2026-09-09

- **Development dependency advisories (owner: maintainer; before a separately scoped
  dependency-maintenance change):** `npm audit --include=dev` reports four vulnerable
  packages: `baseline-browser-mapping` (moderate), `brace-expansion`, `browserslist`,
  and `js-yaml` (high). The production-only audit reports zero. These are observed
  audit findings, not a demonstrated exploit in the shipped app. Reassess and update
  the lockfile under a separate approval; the deletion-only cleanup did not run
  `npm audit fix` or change versions.
- **Regression coverage boundary:** the UI has two Node tests, not comprehensive
  component/interaction tests. Report/event-hook or layout consolidation needs a
  stronger behavior-specific regression net first. This cleanup instead deletes
  unreferenced code and compares the real production builds (`05` Pass 1).
- **Verification remains local:** Node 26.8.1 was used instead of CI's Node 22; no
  remote CI, interactive app-wide QA, GPU/model smoke, installer or release check
  was performed. Existing hardware/manual gates above remain open.


## 2026-10-15 — Remaining npm build-tool advisory

Owner: maintainer; reassess on upstream patch or build-input changes.
GHSA-vfj7-8cjw-p6xm affects braces 3.0.3, propagated through chokidar, micromatch,
fast-glob and Tailwind (five high package entries, one underlying advisory).
Registry latest remains 3.0.3, with no patched release. Globs are fixed in the
repository's Tailwind config; captured text and API requests do not become build
patterns. The static production UI does not execute this Node pipeline, and
`npm audit --omit=dev` is clean. Do not force a Tailwind 4 migration just to
silence propagation. Reassess before accepting untrusted build patterns.
CodeQL alert 2 was dismissed with the FFI contract evidence recorded in `05`.
