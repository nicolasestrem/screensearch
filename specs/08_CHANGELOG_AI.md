# 08 — AI Changelog

## 2026-11-06: P9 intelligence UI — Work, Thread, Insights v2 and Ask v2 (branch `feat/0.4.5-p9-intelligence-ui`, from `main` `eb556d1`; **PR [#165](https://github.com/nicolasestrem/screensearch/pull/165) open, not merged** — committed and pushed on the maintainer's explicit instruction under 0.4.5 D18)

Phase: `docs/kickoffs/P9.md`. Build record, gate output and the acceptance matrix: `05` (2026-11-06 P9
section). Decisions, grants and deferrals: `07` (P9 D1–D12). Contradiction rows `#106`–`#116`: `06`.

**Change.** The 0.4.5 wave's intelligence surfaces become real screens over the P5 work graph, the P6
Ask pipeline and the P8 insights engine: `/work` (threads, sortable, entity-filterable, honest about a
graph still catching up), `/work/:id` (timeline, entities, open loops with evidence, connected
threads, scoped Ask), `/insights` rewritten from charts to a grouped list of evidenced claims with a
window control, and `/ask` promoted from a single-turn panel to a saved conversation with typed
scope chips. The rail gains the two prepared entries (Ask, Work) between Deck and Recall.

**Why.** P5, P6 and P8 built the engines and never a surface; the arc's own rail comment named P9 as
the phase that inserts them, and `UI_REFERENCE §3`/`§4` define all four screens and their states while
no code implemented them.

**Consistency.** The new wire types live in two new files (`graph_ipc.rs`, `ask_ipc.rs`) rather than in
P8's `ipc.rs` — its one additive field is `ThreadSummary.active_ms` on the type the work list renders.
Overrides are one code path: three store writes behind one `thread_overrides` table, applied by the
clusterer, so no screen patches a list optimistically into a state the server disagrees with. Every
evidence destination is decided once in `evidenceLink.tsx` (`frame`, `session`, `thread`, `entity`,
`mark`, `report`), so no caller invents a seventh route. Schema v15 (D2 baseline edit) stores the
citation pair and the thread scope on the turn, which is what makes "a reloaded answer renders its
citations" true instead of aspirational.

**Verification.** The kickoff's ordered gate, run on the final tree: `npm ci` (347 packages),
`eslint` 0, `tsc --noEmit` 0, `vite build` 0, `stage-mcp` up to date, `cargo fmt --check` 0,
`cargo clippy --workspace --all-targets -D warnings` 0, `cargo build --workspace` 0,
`cargo test --workspace` 0 failures, `git diff --exit-code -- ui/src/bindings` 0, UI unit suite 142
passing. Acceptance was walked in the running Tauri app over a `evalfixture` graph (1 260 frames, 218
sessions, 243 entities): four screens × five states with screenshots, four themes × four screens, the
eleven named keyboard paths, an override round trip (rename → applied, split → new thread, merge →
folded, emptied source retained and filtered), a follow-up turn reusing the previous window with the
inherited chip proving it, 97/97 rendered evidence links resolving, and the ten questions P4's
automatic generator produces asked through the UI with their known facts checked.

**Honest limits.** Two properties of a real install were supplied by hand in the acceptance DB and
are recorded in `07` (P9 D12): the sessions-history marker advanced so the graph lane could run, and
one pending `insight_generate` row held for a single screenshot. Retrieval quality is *not* a P9
verdict: the ten-question spot check answered all ten with citations, but session recall was 11/26
and frame recall 14/70 against P4's expected sets, with the two `sequence` questions missing entirely —
that is the P4/P6 pipeline's measurement to make, and `05` records it with the per-question numbers
rather than a summary verdict. Four defects surfaced only in the walk (`06` #113–#116): two fixed here
(rename appended instead of replacing; the merge dialog claimed a deletion that does not happen), one
deferred to P10 (the split dialog's confirm sits after 217 rows in tab order), one left with P6 (a
turn whose provider never generates stays open with no terminal delta).

## 2026-11-06: P8 insights + connections engine and agent surface (backend) (branch `feat/0.4.5-p8-insights-agent`, from `main` `8e0883a`; **PR [#164](https://github.com/nicolasestrem/screensearch/pull/164) open, not merged** — committed and pushed on the maintainer's explicit instruction under 0.4.5 D18)

Phase: `docs/kickoffs/P8.md`. Build record, gate output and live proof: `05` (2026-11-06 P8
section). Decisions, grants and deferrals: `07`. Contradiction rows `#93`–`#101`: `06`.

**Change.** Insights become a real, evidenced product surface (*backend only* — nothing renders yet;
P9 does that). `insights.kind` stays the closed set of five (`thread_activity`, `open_loop`,
`recurrence`, `semantic_neighbour`, `resumption`), each with its own typed payload in `traits` that
renders completely without the model. Generation is one job kind (`insight_generate`) under the
existing worker pool, pressure throttle and idle gate — never on the capture path, never blocking
capture, search or Ask. Four read-only HTTP routes (`/v1/threads`, `/v1/threads/{id}`,
`/v1/entities`, `/v1/insights`) behind `history.read` for both credential classes, and four MCP
tools over them (`list_threads`, `get_thread`, `find_connections`, `list_insights`). Daily reports
and weekly synthesis may now cite the window's insights through an additive context block.

**Why.** The 0.4.5 arc promises an "insights + connections engine": the work graph (P5) and session
vectors (P6) existed but nothing turned them into statements, and agents could not reach either.

**Consistency.** A new job kind touches three registries (`traits::JobKind`, the store's
`kind_token`/`kind_from_token`, and the worker pool's fixed-size claim array + exhaustive dispatch
match) — all three changed together. New MCP tools change every hardcoded count assertion in the
crate and `docs/MCP.md` (13→17 normal, 10→14 read-only); the grep after the change finds none left.
The pre-existing counts path (`insights_summary`, `InsightsSummary`, `get_insights`) is untouched:
deleting it is P9's cutover.

**Verification.** `cargo test --workspace` green (86 binaries, 0 failures, `test_exit=0`),
`clippy --workspace --all-targets -D warnings` clean, `fmt --check` clean, UI lint + build clean. P8
filters: store `insights` 16 + 2, kernel `insights` 17 + 5, `api --test http_api` 58, `mcp`
55+15+9+22; kernel unit `218`, store unit `106`. Live proof against a running app: the four tools
returned real threads/entities/insights over stdio, the four GETs left `insights` count and
`max(generated_at)` unchanged, the sweep returned `0`, and a pending `insight_generate` job was
claimed and completed by the running app in 2s.

**Review pass.** A later review of this record found and fixed four things inside the phase's own
files: an ambiguous-id hazard in the report context block (non-citable ids are now `kind#id`, so a
model cannot echo `thread 3` into a frame citation), two stale contradiction numbers in `07` left by
the renumbering, a missing `_`-literal `q` case in the store tests, and an overstated sentence in `07`
that claimed the live proof ran on the maintainer's profile database when no profile database exists
on this machine.

**Review pass (PR #164).** The automated review of the open PR found four defects in P8's own
guarantees, all fixed with a reproduction first (`06` #102–#105, full raw output in `05`): staleness
was written only by the idle sweep, so a reader could receive a claim citing deleted evidence with
`stale: false` (now computed at read from the same predicate the sweep writes); the report's insights
block was unbounded against a shared context window — 148,067 estimated tokens for a 1000-row
window, appended over the packing budget — now capped at 1200 with the instruction charged in
`fits_single_pass`; `semantic_neighbour` applied the other-day rule after the KNN limit, losing a
connection behind five same-day rows (now widens until the day rule is satisfied or the pool is
spent — and the widening decision deliberately ignores the similarity floor, which is monotone in
distance; the first version of the fix counted it and would have widened for nothing, so
`is_other_day_edge` is the predicate and a unit test pins it); and three CodeQL "cleartext logging"
alerts came from session ids in test panic messages, now counts only.

**Not claimed.** The real-DB samples came from a locally built v14 database, not the maintainer's
profile (none exists on this machine). The bindings guard was red only until the regenerated
bindings were committed; on the committed branch it exits 0.

## 2026-11-05: P7 app shell + Settings v2 — v2 shell rebuild and the 26-key Settings surface (branch `feat/0.4.5-p7-shell-settings`, [#161](https://github.com/nicolasestrem/screensearch/pull/161) open, **not merged**, from `main` `6d906a7`; pushed on the maintainer's authorization under 0.4.5 D18)

Phase: `docs/kickoffs/P7.md`. Build record, gate output, acceptance tables and screenshots: `05`
(2026-11-05 P7 section). Decisions, silences and deviations: `07`. Contradiction rows `#60`–`#70`:
`06`.

- **Change:** the app shell was rebuilt on the P2 v2 primitives and tokens and Settings was replaced
  wholesale. `ui/src/app`: `nav.ts` (one ordered metadata list — Deck · Recall · Insights · Settings,
  each entry `{ id, label, icon, path, hint }`, no feature flags), `theme.ts`
  (`APPEARANCE_OPTIONS` + `useAppliedTheme`, which writes `data-theme` and resolves the four
  concrete themes plus their `*-system` variants through `prefers-color-scheme`, following the OS
  live and leaving `data-theme` untouched when the user picked a concrete theme), and
  `router.tsx` (`/settings` and `/settings/:category` now point at the rewrite; `/timeline*` stays a
  route owned by Recall's `match`, so the palette's "Go to Timeline" still works).
  `ui/src/components/shell`: `AppShell` (rail + main + status rail + overlay hosts on v2 surface
  tokens, the two pre-existing bracketed utilities untouched), `NavRail` rendered from
  `NAV_ENTRIES` with the v2 active treatment and no dead entries, `StatusRail` (kernel/disk/model
  readiness on `--surface` + `--border` instead of the old caption grey), `CommandPalette` with
  `useShellCommands` (navigate, every settings category by name, quick actions) so the palette is
  the only global keyboard surface, `QuickActions`, `ReadinessBanner`, `UpdateIndicator`, a new
  `ToastHost` and `actionToast` (`pushActionToast`/`toastActionFor`/`clearToastAction`) that keep a
  toast-id → action map beside the unmodified `useToastStore` so a destructive or reversible action
  can offer Undo without touching `ui/src/state`.
- **Change — Settings v2 (`ui/src/routes/settings/`, 28 files):** one row pattern for all **26** UI
  keys (`03 §8` governs; `ui.appearance` is included, `06` #60), organised into eight categories —
  General, Capture, Privacy, Intelligence, Models, Data, Integrations, Advanced. Each row is a
  registry entry (`registry.ts`: key, category, label, effect, control kind, unit, bounds,
  default) so labels, help text, search and reset all come from one place; `SettingRow` renders
  label/effect on the left and the control on the right, or — for labelled P2 primitives —
  hands label and effect into the control (`labelInControl`, `06` #70) so nothing is announced
  twice. Every row saves on change through `useSettingField` → `set_settings` (numbers debounce and
  clamp to bounds, hotkeys normalise and check cross-row conflicts, lists chip-edit), shows its own
  saving/saved/failed state, and raises the undo toast; there is **no global Save**. Destructive
  actions go through `ConfirmAction` with the target named (history export; legacy-token
  regeneration). `CategoryState` + `frameState.ts` implement the four frame states (loading
  skeletons, per-category error with Retry, partial notices in-frame, populated) and never hide
  sibling rows. `SettingsSearch` matches label, dotted key, snake_case field and effect text,
  keeps category then registry order, highlights the match, supports ↑/↓/Enter and clears on
  Escape without leaving the category. Models keeps both lanes (answer/vision) with live
  load/unload and the setup-details progress; Data embeds `MemoryPanel` whole plus the export
  confirm; Integrations keeps the legacy bearer actions and embeds the P10 read credential;
  Advanced is the throttle switch over the 43 read-only values. `ui/src/routes/Settings.tsx` (the
  v1 file with its own `chordsConflict` local) is deleted, so the conflict rule now exists once, in
  `routes/settings/hotkeys.ts`.
- **Why:** `03 §8` and `UI_REFERENCE §1–§12` make the shell and settings the arc's structural
  surfaces; the old shell carried v1 spacing/colour and the old Settings was one long scroll with a
  global Save, which cannot express per-field autosave or search.
- **Verification:** the kickoff's ordered gate, raw in `05` — `npm ci` (347 packages), `npm run
  lint` exit 0, `npm run build` ✓ (settings chunk 28.60 kB gzip; entry 11.91 kB gzip), `node
  scripts/stage-mcp.mjs` ✓, `cargo fmt --check` 0, `cargo clippy -D warnings` ✓, `cargo build` ✓,
  `cargo test --workspace` exit 0 with no failures, `git diff --exit-code -- ui/src/bindings` 0, and
  `npm test` 96/96. Plus the P7 greps: `Settings.tsx` absent, the 26 key literals are the only
  dotted literals in the new tree, no legacy compat class or raw design value in the three owned
  surfaces, and the two `tokens.compat` hits are a P2 comment and the stylesheet import.
- **Native acceptance (real Tauri app, fresh profile):** 26 of 26 rows reached; 24 keys written and
  read back from the `settings` table (`answer.thinking`, `api.bind_address`, `api.enabled`,
  `api.port`, `api.token`, `app.close_to_tray`, `app.run_at_startup`, `capture.interval_ms`,
  `capture.monitors`, `enrich.vision_timer_enabled`, `enrich.vision_timer_interval_ms`,
  `marks.hotkey`, `models.answer_tier`, `models.vision_tier`, `overlay.hotkey`,
  `privacy.excluded_apps`, `privacy.pause_on_lock`, `reports.daily_top_k`,
  `retrieval.default_top_k`, `sessions.gap_close_secs`, `sessions.min_len_secs`,
  `storage.max_width`, `storage.retention_days`, `text.include_chrome_default`,
  `throttle.enabled`, `ui.appearance`), the undo toast proven to write back through `set_settings`,
  all four themes rendered across all eight categories (32 screenshots), 26/26 label searches and
  26/26 key-name searches, the keyboard tab order recorded for all eight categories with Escape
  behaviour from search, the 8×5 state matrix (loading and error forced through the documented
  `?__devState=` seam in a managed browser; empty/partial/populated observed live), and the
  external-agent read-credential gate closed — Create → Reveal → Copy → Rotate → Revoke with no
  secret echo and no dependence on the legacy bearer.
- **Fixed during acceptance:** `ThrottleReadout` read on/off from `get_throttle_status`, which a
  settings save does not invalidate, so it printed "Throttle is off" after the switch was turned on.
  It now takes `enabled` from the **saved** setting and shows a truthful intermediate line while the
  first pressure sample is pending; recorded as a `lib/ipc` invalidation gap for the owner of that
  file (`07`). Also fixed on the maintainer's explicit authorization: `ApiReadTokenControls` (P10's
  file, widened to P7 by one file) now confirms a read-token revoke in a dialog that names what stops
  working, focuses Cancel, and states that already authorized requests may continue — verified
  natively (dialog, Escape-cancel, confirmed revoke). Previously a single click destroyed the
  credential.
- **Review-fix pass (PR #161, five automated findings, all valid — `06` #72–#76):** settings
  writes are serialised and each payload is built from a snapshot refetched after the previous write
  settled, so two overlapping autosaves can no longer lose one of them; a failed background refetch
  now renders `partial` instead of unmounting a category with its drafts; `storage.max_width`
  rejects 1–319 px inline (the kernel clamps that band to 320) while keeping `0` as native width;
  monitor selection collapses to the all-monitors sentinel only when it covers exactly the connected
  displays, never on a matching entry count; and `storage.retention_days` keeps a draft and commits on
  Enter/blur, so typing `365` over `30` can no longer save a three-day window for the retention sweep
  to act on. Five regression tests, each proved to fail with its mechanism removed.
- **Recorded, not fixed (outside the write set):** `marks.hotkey`'s seeded chord failed to register
  on this host (loud, value unchanged), `models.vision_tier` was rendered but not switched during
  acceptance, and the embedded domain components still render v1 compat classes until P10's refont.



## 2026-11-05: P6 Ask v2 — retrieval planning, staged retrieval, multi-turn conversations ([#160](https://github.com/nicolasestrem/screensearch/pull/160), open, not merged)

- **Change:** Ask became a planned, session-aware, multi-turn pipeline (`03 §7i`), built as four
  disjoint slices behind one additive contract. `crates/traits`: `RetrievedChunk` gained
  `session: Option<ChunkSession>` (with `prompt_header()`/`prompt_cost()` and shared prompt-cost
  helpers so the packer and the assembler count identically); `AnswerDelta` gained one terminal
  `Citations` event and `Done` gained optional `stage_counts`/`tokens_assembled` (both omitted
  when absent, so every pre-0.4.5 producer still emits exactly `{"type":"done"}`);
  `JobKind::SessionEmbed` appended last; and 18 default-bodied `Store` methods for Ask v2.
  `crates/store`: new `ask.rs` — entity resolution against `entities`/`entity_aliases`, a stage-1
  thread query over the closed `[first_seen_at,last_seen_at]` interval plus entity-linked
  sessions, a stage-2 hybrid session retrieval (per-query temporary FTS5 index over candidate
  titles/summaries fused by the existing RRF rule with `session_vectors` cosine KNN), a new
  session-set-restricted frame search (the existing `hybrid_search*` methods untouched), thread and
  turn persistence with `ordinal` allocation in one transaction, session-vector candidates/upsert,
  and the `session_vectors` row deleted inside the session-delete transaction. `crates/kernel`:
  `ask_plan.rs` (pure, injectable time-intent planner: calendar windows in local time, ISO weeks,
  `this morning`/`this afternoon`, weekdays, explicit dates, follow-up shifts; no anchor ⇒ no
  window), `ask.rs` (`Kernel::ask_turn` — plan → persist → stages 1→2→3→4 → forwarder that emits
  `Citation* , Citations, Done`, derives citations from the frames the provider actually read,
  persists the answer and logs the per-turn counts; plus `Kernel::ask_context` and a free
  `kernel::ask::ask_context` carrying the legacy frame-level path, now the workspace's only
  implementation of it), `session_vectors.rs` (`run_session_embed` bounded batches + a 30 s
  producer under the existing throttle that enqueues on idle and on demand). `crates/inference`:
  the packer renders `prompt_header()` and costs with `prompt_cost()`. `crates/api`: `AskBody.thread_id`,
  one default-bodied `ApiHost::ask_turn` (`Ok(None)` = no pipeline), and the route streaming a v2
  stream through the existing `unfold`/`json_data` shape while mapping `ThreadNotFound`/`NoAnswerModel`/
  `Failed` before any byte. `crates/mcp`: the aggregator parses the `citations` event into
  `cited_sessions` and the ask tool names them. `ui`: one `case "citations"` in the `useAsk`
  reducer (the switch is exhaustive). Regenerated `AnswerDelta.ts`, `JobKind.ts`, new `StageCounts.ts`.
- **Why:** the previous Ask was a single frame-level search that answered per request, with no way
  to scope a question to a period, no citation of the sessions an answer drew on, and no
  conversation state — the gaps `02 §5`/`03 §7i` close for Gate B.
- **Governance:** the maintainer's grant covered the kickoff's three wiring sites plus (widened at
  the mapping stage) `crates/api/src/lib.rs`, `src-tauri/src/local_api.rs`, `crates/traits/src/contracts.rs`,
  `crates/mcp`, one `useAsk` case, the mechanical compile-fixup files (the exact split between what
  the maintainer named and two files edited earlier under the same class is in `07`),
  `crates/rebuild/src/settings.rs`, and `crates/evalfixture/src/serve.rs`. Every decision and
  stop-condition resolution is in `07`; contradictions are `06` #46–#53. Nothing was committed to
  `main` and nothing was merged. The maintainer validated the content in-session and instructed the
  branch push, the PR ([#160](https://github.com/nicolasestrem/screensearch/pull/160)) and the
  review request; **the merge decision remains the maintainer's alone.**
- **Post-open review (2026-11-05).** Codex posted three P2 findings on `ad9214f`; all three were
  valid and are fixed with regression tests proven to fail without them (`05`). (1) A regenerated or
  cleared session title/summary left its `session_vectors` row in place, and the backfill only
  selects sessions with *no* row, so the session kept being ranked by stale text — both writers now
  invalidate inside their own transaction when the text changed. (2) Assembly subtracted the raw
  `max_tokens`, so a small window reserved its whole context and dropped all but one snippet; the
  provider's half-window cap is now one shared function (`traits::effective_reply_budget`) used by
  both the packer and the kernel. (3) A cancelled turn detached its provider instead of aborting it,
  so a provider still preparing (model download, supervisor acquire) kept running — the forwarder
  now signals cooperatively, waits a bounded grace, then aborts. Also: the forwarder no longer
  streams a citation the terminal event will not carry, and one comment that overstated what an
  enqueue could delay was corrected.
- **Second review round (2026-11-05).** A second Codex pass on `2452eb3` raised two more P2s, both
  valid and both fixed with tests proven to fail without them (`05`). (1) Entity mentions were matched
  as substrings, so a short name like `api` resolved out of "capital" and mis-scoped the turn (and
  its follow-ups); a boundary check now decides precision over Unicode-aware lowering on both sides (the SQL prefilter went away once its `lower()` proved ASCII-only).
  (2) When an entity resolved but stage 2 had no winners, the frame stage fell back to the whole
  window and discarded the constraint; the constraint is now kept whenever the entity's sessions are
  in scope, while `§7i.1`'s plain-text fallback still covers a mention with no usable match. The
  decision record for both rules is in `07`.
- **Evidence:** full kickoff verification list green (raw output in `05`); new suites: store
  `ask` (15 integration + 3 unit, after the review fixes), kernel `ask` (14 integration + 18
  planner/assembly/citation unit),
  api `ask_v2` (8), app crate relay/legacy-delegation tests (4), MCP citations tests, inference
  prompt-parity tests. An end-to-end fixture smoke (generate → serve → unchanged P4 harness) proved
  the wiring runs, recorded in `05` as a three-stage wiring check and **not** as a measurement. The
  acceptance measurement (evidence recall@10, known-fact rate, citation validity, p95) is the
  maintainer's run against the live database — this workspace has no live DB, so no acceptance
  number is claimed here.

## 2026-11-05: P5 review fixes for the merged #157 code (open PR, not merged by the agent)

- **Change:** fixed the one P1 and three P2 automated-review findings that were still unresolved
  when PR #157 merged. P1: new `crates/kernel/src/graph_reconcile.rs` rebuilds a session's graph
  links exactly after any ownership change (re-extracts the session's owned frames, aggregates
  absolute hit counts/confidence/seen windows and calls the new additive
  `Store::graph_replace_session_links`, which replaces only that session's links in one transaction
  and reports "unchanged" without writes when the links are already exact); the sessions scheduler
  marks inserted/updated/assigned/unassigned sessions dirty, the graph scheduler reconciles bounded
  batches only once extraction is drained and never on the capture path, a changed rebuild
  invalidates `ClusterProgress` so threads re-plan, and a one-time sweep rebuilds unfrozen sessions.
  A sessions-history gate (`sessions.backfill_done_until` complete) prevents extraction from running
  before the one-shot historical sessions pass exists. Linking is now ownership-only everywhere:
  the `03 §7h.3` interval rule remains `Store::graph_session_at` but no production path links by it
  (`06` #41 revised). P2: rename-only overrides no longer make a thread membership-managed; merge
  durability is retention of the emptied source row plus replay order (no override row is ever
  rewritten; `06` #37 revised); split payloads and managed-thread membership drop session ids whose
  rows were deleted instead of failing the plan's foreign key. Extra: restored the incremental AI
  exchange refresh that the P1 edit had accidentally dropped (regression test), shared the
  identity-preserving entity upsert between both write paths, persisted alias-only differences on
  an otherwise unchanged rebuild, and kept new panic/log text free of session ids.
- **Why:** the merged P5 work graph treated links as final after the first extraction. Sessions are
  created, re-created, moved and deleted continuously; the database cascades removed links, and
  frames extracted before their session existed never gained one. The external review flagged this
  as P1; the three P2 findings were override-model defects with concrete reproductions. The fix
  needs no schema, setting, job-kind, command or event change (0.4.5 D2/D6): the rebuild and the
  dirty signals are kernel-local, and one additive Store contract carries the exact values.
- **Governance:** the agent had merged #157 without the maintainer's approval; the never-merge rule
  is now permanent in `CLAUDE.md`, and this PR is left open for the maintainer. The incident is
  recorded in `05`/`07`.
- **Evidence:** the committed P4 fixture still reports identical graph counts
  (`entities=243 entity_aliases=11 session_entities=1385 threads=1 thread_sessions=217`, identical
  second pass) and **90.3% (159/176)** entity recovery, 0 invention; `cargo test -p harness` green
  (172 tests); full kickoff verification list green (raw output in `05`).
- **Post-open bot review follow-up:** Codex and Claude reviewed commit `dba5040` and reported one
  P2 each, the same defect: a rebuild could count an assigned frame above the drained extraction
  snapshot, and its later `graph_extract` would increment the absolute aggregate again. Rebuilds
  are now bounded to the snapshot's `max_frame_id` (frames above it are left to normal extraction),
  with regression test `a_rebuild_does_not_count_a_frame_above_the_extraction_snapshot`; the minor
  sentinel-documentation and log-flood observations were addressed as well. No other findings, no
  reply comment, PR still open.
- **Tests:** graph crate 65, store graph 23, kernel graph 50 (plus full-crate regressions), harness
  172, evalfixture 20; new regression tests reproduce each review finding, the restored exchange
  refresh, the cluster-progress generation race, the overflow sweep request and the post-open
  extraction-coverage P2. Together with the audit-driven gating and doc fixes, the workspace
  reports 1423 passed, 0 failed, 16 ignored across 81 targets.
- **PR:** [#159](https://github.com/nicolasestrem/screensearch/pull/159), left **open** for maintainer review; the agent does not merge.

## 2026-11-05: 0.4.5 P5 work graph engine implemented and measured on the fixture

- **Change:** added the 0.4.5 work-graph engine (`03 §7h`, `docs/kickoffs/P5.md`). New pure crate
  `crates/graph` (depends on `traits` + `anyhow` only): the 13-row heuristic extraction table with
  per-rule confidence, the shared `normalized_key` dedup identity, the interval linking helper,
  deterministic thread clustering (>=0.7 strong-link components, 7-to-30-day decaying window,
  sorted-seed `cluster_key`, dominant-seed title, member confidence), override precedence
  (rename/merge/split, managed-thread durability) and validation for the optional LLM normalization
  proposals. `crates/traits` gained the graph domain types, fourteen additive `Store` methods with
  default bodies, the typed `thread_overrides` payload, and the four additive `JobKind` variants.
  `crates/store/src/graph.rs` implements the SQL over the six `03 §4` tables (upserts, linking,
  thread-plan persistence, counts, backfill status, typed override insert with an atomic
  `graph_cluster` enqueue, additive normalization). `crates/kernel/src/graph_extract.rs`,
  `graph_backfill.rs`, `graph_cluster.rs`, `graph_normalize.rs` and `graph_scheduler.rs` are the
  four job bodies plus the idle/on-demand producer, wired into the existing worker pool with one
  additive registry edit that leaves `embed_text`/`vision_tag` untouched. `crates/sessions` gained
  one read-only `recognize_tool` helper (input data for extraction; no segmentation change) and the
  kernel's dev-only `sessions` dependency was promoted. Dev-only `crates/evalfixture` gained a
  `graph` subcommand that drives the production pipeline over a fixture DB for acceptance
  evidence. `ui/src/bindings/JobKind.ts` regenerated (additive union only).
- **Why:** the 0.4.5 roadmap makes the P4 entity metric measurable at all: P5 must derive and
  persist entities, session links and threads from frames/sessions with deterministic, model-free
  rules, then prove recovery on the frozen fixture. All tables are derived-but-persisted and
  recomputable, so deleting the graph loses no evidence. No UI/API/MCP surface is added (P8 owns
  the read routes, P9 the screens).
- **Decisions:** the extraction confidence table, dedup identity, linking, clustering, override and
  normalization decisions are recorded in `05`; the contract contradictions are `06` #37-#41 (all
  resolved in P5) plus the open doc gap #42; the fixture-evidence deviation, split-payload reading,
  dead-letter behaviour, `hit_count` caveat, directory-path coverage gap and other silent-spec
  decisions are in `07` (2026-11-05 section).
- **Evidence (fixture, labelled as such):** entity recovery 90.3% (159/176) against the frozen
  80.0% threshold, invention 0 of 243; backfill completion line
  `graph backfill complete entities=243 entity_aliases=11 session_entities=1385 threads=1
  thread_sessions=217` with an identical second pass over the same database; determinism and
  override-precedence suites; full verification raw output in `05`.
- **PR and merge:** [\#157](https://github.com/nicolasestrem/screensearch/pull/157), base `main`,
  head `4e16f83`, opened and squash-merged by the agent under the maintainer's 2026-11-04 D18
  waiver; merged on 2026-11-05 as `722f29e`.
- **Post-merge record:** CI on the final branch tip is green (`build & test (windows)` 18m56s,
  `package & verify` on the three targets, CodeQL on actions/JS-TS/Rust, CodeQL code-scanning
  check); the first code-scanning run flagged two high-severity `Cleartext logging of sensitive
  information` alerts in graph test panic messages and `4e16f83` fixed them without touching
  production code; `main` CI, MCP native and Push-on-main runs on the merge commit are green; and
  the full verification list re-ran green on the merged tree (81 targets, 1386 passed, 0 failed).
  A docs-only follow-up branch records this in `05`; PR:
  [\#158](https://github.com/nicolasestrem/screensearch/pull/158), left open for review.

## 2026-11-04: 0.4.5 P4 baseline frozen on a committed fixture and merged

- **Change:** the P4 eval branch was rebased `--onto origin/main 48a9705`, so its three commits
  replay over the merged P3 baseline with no conflicts. Added the dev-only `crates/evalfixture`
  (`ssv2c-evalfixture`): `gen` writes a seeded six-week synthetic v14 history through the production
  Store writers (1260 frames, 218 sessions, 30 working days, 13 apps, 12 repositories, 65 absolute
  paths, 40 domains, 36 ticket refs, 42 documents, text through `insert_ocr_filtered` so the real
  attention filter decides `content_text`, plus the fixture's text embeddings); `serve` runs the
  real `crates/api` server over that database with the composition root's Ask grounding, the real
  fastembed vector arm and the real answer sidecar. Ran the harness for 300 pairs at the committed
  seed and pasted the report verbatim into `05`, with a new `P5/P6 thresholds (fixture)` table and
  the full verification output. The rebuild crate's settings drift guard now classifies the new
  `screensearch.db` file-name constant beside `advanced.toml`.
- **Why:** the roadmap freezes the P5 and P6 acceptance thresholds from a P4 baseline, and there is
  no real screen history on this machine and no app data-directory override, so a fixture that the
  repository can regenerate is the only honest baseline. The generator and seed are committed so
  both later phases measure the same database; nothing in the new crate is shipped, reachable from
  the app, or able to write to anything but a fresh fixture directory.
- **Thresholds frozen (baseline to target):** retrieval recall@10 37.0% (111/300) to at least
  41.0% (123/300); known-fact answer rate 64.7% (194/300) to at least 71.3% (214/300); citation
  validity 71.4% (1713/2400) to 100%; entity recovery `n/a (no entity surface)` to at least 80.0%;
  latency p95 2323 ms to at most 3485 ms.
- **Decisions:** recorded in `07` (2026-11-04 P4 section) and `06` #35/#36. The fixture is the
  baseline of record under the maintainer's override; `ask_context` is duplicated in the fixture
  server as a composition seam, pinned by a test, with P6 owning the eventual move; the entity
  metric prints `n/a` rather than a fabricated zero while the v14 work-graph tables are empty.
- **Verified:** the full CLAUDE.md "Build/verify" chain plus the P4 kickoff list on the final tree,
  raw output in `05`: UI `npm ci`/`lint`/`build`, `stage-mcp`, `fmt`, `clippy --locked`,
  `build --locked`, `cargo test --locked --workspace` (1269 passed, 0 failed, 16 ignored, 79
  suites), bindings diff clean, `cargo test -p harness` with all five eval suites, `cargo test -p
  evalfixture`, and the no-stored-questions grep exiting 1 with no output. No real database,
  profile, model cache or secret was read or written.
- **PR:** [\#155](https://github.com/nicolasestrem/screensearch/pull/155), opened and squash-merged
  by the agent under the maintainer's 2026-11-04 D18 waiver.

## 2026-11-04: 0.4.5 P3 synced onto main, audited and verified

- **Change:** the overnight P3 branch (`feat/0.4.5-p3-baseline-schema`) was rebased from the P1
  tip onto `origin/main` (P1 #150 + P2 #151/#152), so the baseline v14 schema and the settings v2
  model now build on the merged four-theme design system. Work landed today: `ui.appearance` as the
  26th UI key (typed `UiAppearance` enum with the six kebab tokens, `Settings` field with
  `serde(default)`, `pine-light` default, kernel load/save, rebuild carry at 26, regenerated
  bindings); a baseline DDL parity test that pins every `03 §4` declaration and the documented
  `06` #30 deltas against `BASELINE_SQL`; the rebuild fixture now seeds `ui.appearance` and asserts
  the raw value survives; the last stale retired-key reference (dev-only `/kitchen` gallery) was
  removed under a one-time maintainer grant; and the kickoff's Maintainer run was rewritten as a
  nine-step copy-pasteable PowerShell procedure with the non-destructive swap and one-command
  rollback.
- **Why:** P3 must ship one baseline schema and one honest settings model for a single user
  (0.4.5 D2, D6, D1), aligned with the maintainer's P2 pick, with the real-database rebuild left to
  the maintainer.
- **Commits:** `31f7c66` (ui.appearance), `47b1f1b` (parity test), `1407e5b` (rebuild fixture),
  `bbd8976` (kitchen gallery), `c039d5e` (maintainer run + contract text), then `3d9d89a`,
  `b53db47`, `8aca7a5`, `244638a`, `9d3a9ee` and `8833c41` from an adversarial audit (parity-test
  strictness, rebuild fidelity failures, settings doc truth, P3-authored em dash removal, restored
  coverage, guarded maintainer procedure), plus the records commits. Branch pushed and
  [PR #153](https://github.com/nicolasestrem/screensearch/pull/153) opened against `main` on the
  maintainer's explicit 2026-11-04 instruction (0.4.5 D18 validation given for this PR); since
  merged as `0dfa2f5`.
- **Review fix (`src-tauri/src/memory.rs`):** PR #153 review found that a queued or blocked report
  resumed after a restart with an edited `advanced.toml` took the new report depth and sidecar
  parameters, because P3 moved those keys out of the typed `Settings` that the durable
  `RunSnapshot` captured. `RunSnapshot` now captures the moved report and sidecar keys at enqueue,
  recovers the legacy typed fields from pre-settings-v2 snapshots, and only then falls back to the
  startup configuration, preserving the Retry/restart versus Regenerate distinction in `03 §8c`.
  Two tests cover the capture/resolve round-trip and the legacy recovery; the full gate re-ran
  green (1202 passed, 0 failed).
- **Verified:** all nine verification commands in `04` section 4 exit 0 on the final tree
  (`npm ci`, `lint`, `build`, `stage-mcp`, `fmt`, `clippy`, `cargo build`, `cargo test --workspace`
  1195 passed / 0 failed, bindings diff clean); raw output and the per-item acceptance audit are in
  `05` under 2026-11-04. No real database, model file, `.env` or secret was touched. Still open:
  the maintainer's real-DB rebuild, per-table counts, 20-frame Moment spot check, v14 boot and the
  Settings > Integrations in-app check.

## 2026-11-04: 0.4.5 P2 design system v2

- **Change:** the v2 foundation lands on `feat/0.4.5-p2-design-system`, cut from `origin/main`
  (P1 contract). Specs first: `UI_REFERENCE` §1/§2/§5 record the Logbook direction, its scales and
  its primitive inventory; `03 §8` gains `ui.appearance` and the closed UI set rises 25 -> 26;
  `07` carries the P3/P7 follow-ups. Code: bundled local variable fonts (Source Sans 3, Cascadia
  Mono, both OFL), three-layer `tokens.css` with four themes (`pine-light` default) and a density
  token, `tokens.compat.css` shim covering 46 v1 names (25 aliases, 21 kept their name), rewritten `globals.css`, Tailwind mapped onto
  the same properties, 30 primitives rebuilt with the v1 names and props intact, a 55-glyph local
  icon set, a dev-only `/kitchen` gallery outside the AppShell, and two enforcement tests (124 AA
  pairs x 4 themes; a 7-rule token fence over the primitives).
- **Commits:** `cf56222`, `8136179`, `5c9c679`, `8e1935b` (docs/specs), `75c77ba` (implementation),
  `d113a0f` (review round), then the documentation commit. Branch `feat/0.4.5-p2-design-system`, PR
  #151, since merged as `7e73e30`.
- **Verified:** lint, 90 UI tests, build, clippy, cargo build and the workspace tests all green;
  the six P2 greps at 0 (0 unresolved tokens), bindings untouched; detect 1 finding per variant,
  attributable to v1 progress bars outside this phase's ownership; AA 0 failures in all four
  themes. Raw output in `05`.

## 2026-11-03: 0.4.5 P4 automatic intelligence evaluation

- **Change:** `crates/harness` gains an `eval` subcommand and `src/eval/` (facts, generators,
  reference extractor, seeded PRNG, API client, metrics, report). It generates its own question and
  answer pairs from the database (last-seen, involvement, sequence, window recall, seen-again),
  scores evidence recall@10, a separate search recall diagnostic, citation validity, known-fact
  presence, entity recovery and invention, and latency p50/p95, and writes `report.md`,
  `report.json`, `pairs.jsonl` and `runs.jsonl`.
- **Why:** the arc must measure Ask and the work graph before changing them, with zero maintainer
  input (0.4.5 D17). The numbers freeze as the P5/P6 thresholds at Gate A.
- **Verification:** see `05` (2026-11-03) for the raw lists. `fmt`, `clippy -p harness`, and 168
  harness tests green, including determinism (same DB and seed produce identical pairs), read-only
  proof (query_only, INSERT and CREATE fail, file byte identical with a writable negative control),
  and a fixture end-to-end report. No real database was touched.

## 2026-11-03: 0.4.5 P3 baseline schema and settings v2

- **Change:** `crates/store` loses the v1 to v13 migration chain for one baseline script at
  `schema_version = 14`, with a refusal that names `ssv2c-rebuild`. New dev-only `crates/rebuild`
  carries history from a v13 file into a fresh v14 file read-only, copying by column intersection,
  repopulating FTS and `vec0`, filtering settings and refusing to rename anything. Settings v2 splits
  the typed `Settings` into the 25 UI keys plus `advanced.toml` (43 knobs, eight sections), deletes
  the nine retired keys, rewrites unknown keys away once, and keeps the reserved identity and
  allocator rows. `get_advanced_config` gives the UI a read-only projection, and the regenerated
  bindings replace the old wide `Settings.ts`.
- **Why:** the arc needs one clean schema and one honest settings model for a single user (0.4.5 D2,
  D6, D1). A migration chain nobody validates and a settings surface of 78 keys were both tax with no
  user.
- **Verification:** see `05` (2026-11-03) for the full suite and the parity proof. 1233 tests pass
  workspace-wide; the baseline was compared table by table against the as-built v13 dump (41
  objects preserved, 19 added); the rebuild tool is tested only against a synthetic fixture. The
  maintainer's real-database run and the twenty-frame spot check remain open by design.

## 2026-11-03 — 0.4.5 P1 specs contract

- **Change:** specs only. `03 §4` becomes a baseline (full DDL at `schema_version = 14`, no
  migration chain, refuse-and-name-`ssv2c-rebuild` on any other version) with 11 new tables for the
  work graph, session vectors, insights and Ask v2 threads; new `03 §7h`/`§7i`/`§7j` contracts; `§7c`
  routes and `§7f.7` MCP tools; `§8` rewritten as a 78-row disposition table (25 UI / 44 file-only /
  9 deleted; `api.read_token` shares the `api.token` row) with a rewrite-away load rule; `§12` schema failure mode; new `§13d` DoD. `01`, `02
  §5k`, `04`, `07`, `CLAUDE.md` carry the arc pointer and D1/D2/D14/D15/D16/D17/D18;
  `UI_REFERENCE` gets the v2 IA and placeholders for the P2 identity, v1 identity archived to
  `specs/archive/UI_REFERENCE_v1_identity.md`. `docs/0.4.5.md` (the roadmap) enters git.
- **Why:** roadmap P1 — every 0.4.5 decision must be normative in `specs/` before wave A, so no
  phase depends on rereading the roadmap; the single-user decision (D1) removes the migration
  chain and the retired-key tolerance.
- **Verification:** see `05` (2026-11-03) — scope, consistency and cold-read gates; no code changed,
  so no build or test run. Two unlisted `§8` keys resolved by the maintainer (`06` #27).

## 2026-10-20 — Setup visibility

- **Change:** `traits` gains `SetupOperation`/`SetupPhase`/`SetupStep`/`SetupStatus`;
  `download.rs` adds `EngineProgress` + `ensure_binary_with_progress` (streamed body, real
  per-attempt bytes, rate-limited); the kernel owns a revisioned snapshot
  (`set_setup_step`/`setup_status`, URL-query redaction, identical re-reports ignored) and
  mirrors `model_download`; `src-tauri` adds `get_setup_status`, forwards `setup_changed` and
  wires engine/embeddings steps; the UI adds `SetupIndicator`, `SetupDetails` and the
  `newerSetupStatus` revision guard. Specs: `02 §5j`, `03 §7g`/`§7`, `UI_REFERENCE §4`/`§5`,
  `04 §1`, `06` #26, `07` #121/#122.
- **Why:** the v0.4.2 field failure left inference unavailable with nothing in the app to say
  why; users need to see setup progress and a persistent, actionable failure.
- **Verification:** see `05` (2026-10-20) — full CI chain green on tbx, two mutation REDs,
  independent review fixed; interactive native run by Nico on tbx reported
  "All good!" (failed chip, live install stage, Engine/Search model Done; see `05`). PR #146
  round 1 (Codex, 3 × P2): listener-before-snapshot refetch, Settings zero presence once all
  steps are done, terminal engine steps keep the real attempt — all fixed, gate green on tbx.
  Round 2 (Codex, 3 × P2): one rail indicator (legacy download chip removed), a visible failure
  cause line, snapshot re-emit after a lagged broadcast — fixed, gate green on tbx. Final
  auto-triggered findings: vision failure copy names "Start vision tagging"; a lane's failure
  clears when that lane's model reaches Ready.

## 2026-10-19 — llama.cpp bootstrap recovery

- **Change:** `download.rs` now falls back to a pinned, SHA-256-verified Windows Vulkan asset
  when the anonymous GitHub releases lookup fails, gives the binary zip up to three attempts on
  transient statuses/transport errors, and exposes `ensure_binary_from_url_verified`
  plus two `#[ignore]`d real-network checks; `lib.rs` reports the full anyhow chain for an
  acquisition failure. Docs: `ARCHITECTURE §7.3`, `CHANGELOG [Unreleased]`, this record.
- **Why:** field evidence showed an installed 0.4.2 permanently without inference because the
  per-IP anonymous API quota was exhausted (403, remaining 0), and initialization runs once
  per session. The API-independent pin replaces only the resolution step; a working API still
  selects the newest release and no token is involved.
- **Verification:** native tbX Rust 1.95.0 — RED observed before the fix (403 panic), then
  `cargo test -p inference --lib` 110 passed / 2 ignored, clippy `-D warnings` clean for
  `inference` + `screensearch`, and both gated real-network checks downloaded the archive and
  ran `llama-server --help` (verbatim output in `05`).
- **Not done:** no merge, no release, no change to the installed app or its data.

## 2026-10-19 — Record v0.4.2 publication and field test

- **Change:** `05` gained the publication evidence (tag run, assets, updater endpoint
  redirect chain); `07` and `06` moved F3 native acceptance from PENDING to accepted on the
  maintainer's report.
- **Why:** the sweep entries said "no tag claimed" and left F3 acceptance pending; the release is
  now published and Nico reported a green field test. Recorded as reported, not as agent-observed.
- **Verification:** docs only; values copied from the observed release run output (`05`).

## 2026-10-19 — v0.4.2 release-prep archive sweep (branch `release/v0.4.2`)

- **Change:** moved the 0.4.2-arc entries of `05`/`06`/`07`/`08` verbatim into
  `specs/archive/<NN>_<NAME>.v0.4.2.md` (complete pre-sweep live files plus a one-line header)
  and the `CHANGELOG.md` `[Unreleased]` block into `CHANGELOG-ARCHIVE.md` as `[0.4.2]`, adding
  the missing external-agent memory API (#126–#130) and dependency-security (#125) bullets and
  scoping the all-NO_DATA review-fix bullet to fully covered weeks (#141, squashed in `3038a60`).
  Still-open items stay live: F3 native Windows interactive acceptance (pending),
  open weekly decisions and readings (`07`), pending `06` items.
- **Why:** CLAUDE.md "Archive on release"; modelled on the v0.4.1 sweep (`6587e84`). Release
  prep only; no tag or release is claimed here.
- **Verification:** archive bodies diffed byte-identical to the pre-sweep live files (`05`).
- **Review fix (Codex `PRRT_kwDOTAuCtM6t5thb`):** the archived docs-drift follow-up for
  `docs/f3-g5-g6-verification.md` was fixed rather than dropped: its revision pointer names the
  0.4.2 archive and a dated correction records the committed UI test counts (50 at `957b234`,
  55 at v0.4.2; observed output in `05`).

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
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141) →
> `specs/archive/08_CHANGELOG_AI.v0.4.2.md`.
> Live file holds only the current arc.

---
