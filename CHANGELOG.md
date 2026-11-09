# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Changed: the enrichment throttle can see memory, and level 2 now releases the vision model

**Baseline, measured on the installed 0.4.5 build before any change**, on the maintainer's box
(RTX 5060 Ti 16 GB, 32 GB RAM, screenpipe capturing alongside): while the vision sidecar was
loaded it held ~5.8 GB of available physical RAM, ~8.4 GB of commit charge and ~6.8 GB of VRAM, and
the in-process embedding model held 1233 MiB of private bytes while completely idle. Neither was
reachable by the shipped throttle, because back-to-back `vision_tag` batches pin CPU at 100 % and
would have throttled the app during its own work. These are baseline numbers, not a measurement of
this branch's build.

- **The throttle now watches host memory, not just CPU/GPU.** A third arm reads whole-machine
  physical usage and system commit charge every sample (two Windows calls, and both must succeed)
  and feeds both into the same hysteresis machine. This is the case the CPU/GPU arms cannot see: a
  machine that is out of RAM while ScreenSearch is idle — a large editor, another recorder, a wall
  of browser tabs — where `vision_tag` claims are simply not happening and nothing throttles.
  Commit charge catches the other half: a process that has committed far more memory than it is
  actively using, before physical RAM actually runs out. Either reading at its enter threshold
  raises the level; both must cool below their exit thresholds before a level can drop. Defaults are
  90 %/80 % for both. On this box that sits above the app's own measured vision peak (77 % physical,
  67 % commit), so with these defaults it does not throttle itself during its own vision batches
  here — but the arm is not promised to stay quiet on a host whose own workload crosses 90 %, which
  is exactly why the thresholds are settings rather than fixed behaviour.
- **Level 2 releases the vision model.** Pausing `vision_tag` frees no memory — the sidecar holds it.
  On the transition into Sustained the governor now asks the supervisor to unload the resident
  `llama-server` through the same gate-then-kill path as the manual Unload control, so a starved
  machine is meant to get its VRAM and commitment back instead of only its workers paused. Relief
  outranks the idle backfill's keep-warm (a drain exists to avoid cold starts, not to hold memory a
  machine needs), never an explicit user preload: a pinned model stays and the skip is logged per
  rule A. It is edge-triggered on entering the level, so a long starvation window cannot become a
  kill/reload loop. The Settings copy says the app tries to free the AI model's memory unless the user
  explicitly kept it loaded — a try can be skipped (kept loaded) or refused (nothing resident), so it
  does not claim a release happened.
- **The embedding model unloads after 15 idle minutes.** fastembed now lives behind a lazy holder that
  loads on first embed (or eagerly at startup, so a first search is not cold), reports its dimension
  and model name without loading, single-flights concurrent loads, and drops the ONNX session once
  `enrich.embed_idle_ttl_secs` (default 900, `0` = never) has passed with no embed. A session an embed
  still holds is never dropped mid-flight: the sweep and the manual unload return `false` and leave it
  resident, and the load gate spans teardown so a reload cannot overlap it. The baseline above
  was 1352 MiB private with the embedder resident and 119 MiB with embeddings disabled; the 119 MiB
  came from a different configuration. An isolated harness with the real cached model verified the
  lifecycle narrowly: 7 MiB before load → 1228 MiB loaded → 26 MiB after the idle sweep (TTL 2 s) →
  1234 MiB after reload, with an identical 768-dim query vector (max diff 0). That is the holder, not
  the whole app: it does not establish the app's idle footprint, and the reload latency is still
  untimed.
- **Settings · Advanced and the StatusRail show memory truthfully.** The throttle readout in
  Settings · Advanced gains `RAM 71% (8.3 GiB available)` and a Commit percentage from the same
  sample as the usage number, reports available memory rather than calling it "free", says "not
  monitored" rather than printing 0 % when a reading is unavailable, and its level-2 line says the app
  tries to free the AI model's memory unless the user explicitly kept it loaded. The rail tooltip
  adds the RAM and commit percentages to its CPU/GPU line. All five new keys (four memory thresholds,
  one idle TTL) are `file-only` in `advanced.toml`, so the closed 26-key UI ceiling is untouched.

Status: implementation on `feat/memory-pressure-throttle`. The final ordered gate ran every build and
test stage green — `ui_ci/lint/build`, the staged MCP sidecar, `cargo fmt`, clippy with `-D warnings`,
`cargo build` and `cargo test` all exited 0, with the full workspace reporting 86 suites / 1650 passed
/ 0 failed / 17 ignored and the four regenerated bindings unchanged across the test run. The app
itself was not run for memory: the harness above covers only the embedder lifecycle, so no
changed-build footprint, reload latency or native acceptance is claimed. At gate3 time the binding
guard exited non-zero only because nothing was committed; the user has since authorized the commit on
the new branch `feat/system-memory-relief`, with the generated bindings staged and
`git diff --exit-code -- ui/src/bindings` now exiting 0 with no output, so the guard is satisfied by
the staged tree — nothing is committed yet.

### Fixed: a settings save stops warning about hotkeys, Ask shows its answers as Markdown, Settings rows line up

- **Saving any setting no longer re-registers an unchanged global hotkey, and no longer shows a
  spurious warning.** Every save in the shipped 0.4.5 build produced a second toast, `Could not
  register mark hotkey: HotKey already registered: HotKey { mods: Modifiers(ALT | CONTROL), ... }`,
  because the registration path treated a chord this process already held as a conflict, cleared
  the stored chord, and then retried the same doomed registration on every later save. Registration
  is now idempotent and ownership aware: a chord the app already holds is never registered twice,
  chords are compared canonically instead of as raw strings, and a failed registration can no longer
  discard a chord that is still active. Both the mark hotkey (Ctrl+Alt+M) and the Flow overlay
  hotkey (Ctrl+Alt+Z) go through the same path, and no Rust `Debug` output can reach a toast: a
  genuine conflict now reads as plain language, for example "another app is already using that
  shortcut".
- **Ask v2 renders its answers as Markdown.** `/ask` printed the model's Markdown literally, with
  visible `**bold**` and `- ` list markers, while the Recall Ask tab rendered it correctly. Both
  surfaces now render through one shared component (react-markdown with GFM and the `prose-deck`
  typography), so they look identical; the citations row is unchanged, and react-markdown still
  loads with the route chunk instead of the initial bundle.
- **Settings controls sit in one fixed column.** In Settings > General the `Launch at startup`
  toggle sat further left than the `Close to tray` toggle, because the shared row let the control
  slot size itself to the hint text. The control column is now fixed in the shared row, so every
  control in a category starts at the same position.

## [0.4.5] - 2026-11-07

### Changed: every screen you own is drawn on one token set, and the old one is gone

- **The v1 token shim is deleted, and its absence is enforced.** `tokens.compat.css` mapped 25
  legacy names (`--bg-base`, `--ink-muted`, `--glow-scan`, `--radius-ribbon`, …) onto v2 tokens so
  the old screens could keep compiling while they were rebuilt. Every screen P10 owns is now on v2
  directly, the file is gone, and nothing in `ui/src` mentions any of its 25 names. Two fences keep
  it that way: the token test walks all of `ui/src` (not just the routes) and a lint rule refuses a
  raw token literal at the point it is written — no new dependency, and neither fence was loosened.
- **Deck, Recall (search and reports), Timeline, Session, Moment, and Settings · Data were rebuilt
  on those tokens**, with their five states intact. The timeline's scan-head and glow are replaced
  by a density ribbon: four reserved lanes, ribbon heights that are the real capture counts, and a
  hover card that shows the bucket instead of decorating it.
- **Session and Moment now show what the session is about.** Both gained the entity chips and the
  connections panel the Work and Thread screens already had, fed by one additive read
  (`get_session_graph`) rather than by four screens querying four different ways; a session that was
  never clustered shows an honest empty arm instead of an error.
- **Five dead panels were deleted** (`ApiPanel`, `AppPanel`, `RetentionControl`, `InsightsBars`,
  `CapturesTrend`) after a grep proved nothing imported them.
- **Budgets improved rather than merely holding**: initial JavaScript is 112.9 KB gzip in the main
  window and 75.4 KB in the overlay window (45 % and 30 % of the 250 KB budget), every route is its
  own chunk, the rail click lands in 27.9 ms median, search results in 16.4 ms median, and the
  overlay is on screen 13 ms after the hotkey with its first paint acknowledged at 24 ms.

### Added: Work, Thread and Ask screens, and Insights rewritten as claims you can check

- **Your history now has a place to look for what is in flight.** `/work` lists the threads the
  clusterer found — last touched, session count, time spent — sortable by either, filterable by
  entity, and honest about a graph that is still catching up ("217 of 218 sessions clustered").
  `/work/:id` opens one: its session timeline, the entities those sessions touch, the open loops it
  still holds (each with its evidence), the threads it connects to through shared subjects, and Ask
  scoped to it. A thread clustered only on weak links says so — one muted line, "Low confidence — few
  shared entities" — rather than a badge, count or score.
- **Insights are a list of claims, grouped by kind, each with the evidence that supports it.** The
  window control (today / 7 days / 30 days / custom) reads real rows; a card whose evidence is gone
  says so instead of asserting a fact; a window still being computed says how many passes are left;
  and an empty window says it is empty rather than rendering nothing.
- **Ask is a conversation.** Turns are saved, the list is titled by the question that started it, a
  failed or stopped turn keeps its question with Retry, and a follow-up keeps the previous turn's
  window and entities — with a chip that says "Inherited from the previous turn" so the reuse is
  visible rather than implied. Answers carry both kinds of citation the answer read (sessions and
  moments) as links.
- **Four screens' worth of new IPC**: 13 graph/Ask read-and-write commands (`list_threads`,
  `get_thread_page`, `list_entities`, `list_insights`, the three override writes, `ask_turn` and the
  conversation reads), wire types in `crates/traits/src/graph_ipc.rs` and `ask_ipc.rs`, and the
  regenerated `ui/src/bindings` in the same commit. Every override — rename, merge, split — is
  written to `thread_overrides` with a receipt and applied by the clusterer, so a user's edit keeps
  beating the automatic grouping.
- **Schema v15** adds `ask_turns.citations` and `ask_turns.scope_thread_id`: a reloaded turn renders
  its citations from the row rather than from a replayed stream, and a turn's thread scope is
  readable after a restart. Rows written before v15 keep `NULL` citations and render without
  citation chips rather than with invented ones.
- `specs/UI_REFERENCE.md §5` gains a "Composed components" table (`ConnectionsPanel`, `EntityChips`,
  `InsightCard`, `ScopeChips`, `EvidenceLink`), so the composed pieces P10 places in Session and
  Moment are inventoried beside the primitives without changing the primitive count.

### Fixed: two things the P10 review found in the new panels

- **A Session or Moment left open while the graph caught up stayed stale.** Clustering finishing in
  the background refreshed Work, Thread and the insight list but not the session's own graph — and
  nothing else would have corrected it, because that read has no polling and the app does not
  refetch on window focus. The session graph is now part of the same refresh, so a screen that was
  open across the clustering pass fills in instead of keeping its half-empty panels.
- **A connection named its evidence `Entity 123` when that entity came from another session in the
  thread.** The shared entity between two threads is often touched by a *sibling* session, which the
  opened session's own entity list never contains; the panel then fell back to the bare id.
  Connections now resolve those names, so the line that proves a connection reads
  "Project Atlas · Sunrise" rather than two numbers.

### Fixed: two things the acceptance walk found in the new screens

- **Renaming a thread appended to its name instead of replacing it.** The inline field opened
  pre-filled with the caret at the end, so typing produced `Payroll importPayroll ops`. The field now
  selects its value on first focus; typing replaces the name.
- **The merge dialog claimed the emptied thread is "removed".** It is not: it is retained on record
  (deleting it would delete your own override with it) and only stops appearing in Work. The dialog
  now says exactly that.

### Fixed: two more things the PR review found

- **Your question is kept when it cannot be sent.** Asking with no answer model loaded used to clear
  the composer and then offer only a link to Settings — the words were gone when you came back. The
  question is now put back in the box (and kept when its conversation has been deleted), ready to send
  once the model is loaded.
- **Retrying an unanswered question no longer cancels an answer you are waiting for.** The Retry action
  on an older turn stayed clickable while another answer was streaming and would have restarted it in
  its place; it is disabled until the stream finishes.
- **An answer that finished while you were on another screen no longer stays looking unanswered.**
  Leaving Ask mid-answer and coming back before it finished meant neither screen refreshed the turn
  when the answer landed, so the conversation kept showing the question with no answer until it was
  reopened. The completion now refreshes the record wherever you are.
- **A scope chip no longer names a period you did not search.** "This week" asked on a Wednesday, or a
  custom range ending in the future, could be labelled "Last 7 days"; the chip now only says that for a
  window that really does end today, and otherwise shows the dates it covers.
- **Retrying a question re-asks it over the scope it originally used.** A question that failed stayed
  in the conversation offering a retry, but the retry inherited the *latest* question's date range,
  entities and thread — so a question originally asked about one day could come back answered from
  unrelated history. The retry now reads the failed turn's own recorded scope.
- **A failure no longer follows you into another conversation.** After a question failed, switching to
  a different conversation still showed that error and its Retry button, which would have resubmitted
  the question into the conversation you left. Both now stay with the conversation they belong to.
- **A thread page now shows an open marker that was left on one of its own captures.** An unresolved
  marker opened the loop on the moment it flagged, but the loop was only ever attached to that moment,
  so a thread whose history contained the marker could still say "No open loops in this thread". The
  loop is now attached to the session it interrupted and to the thread that session belongs to.
- **Work and Insights now refresh when the graph is reorganised without new captured frames.** A merge
  override's regrouping pass, a normalization pass and an insight generation run all finished without
  telling the interface, so Work could keep the old grouping and title and Insights could stay empty
  until you navigated away and back. Those passes now report completion, and the frame-scoped
  refreshes stay as narrow as before.
- **Pressing Enter again while a thread's answer was still arriving started a second conversation.**
  The first question was cancelled and its partial answer disappeared, while the cancelled turn stayed
  behind unanswered. The thread panel now ignores a submission while its turn is streaming, the same
  way the Ask screen already did.
- **The Work list could come back short.** Threads emptied by a merge are kept on record but hidden
  from the list, and that hiding happened after the page had already been cut to size — so a hidden
  row could take a slot and older live threads dropped off a page with no way to page past them. The
  list now filters before it cuts.

### Fixed: two more things the PR review found in Ask

- **A question started while the previous one was still being prepared could adopt the wrong
  conversation.** The app cancelled the earlier request, but that request's own reply — its
  conversation and turn ids — was still written into the state of the question you had just asked,
  so the screen could jump to the earlier conversation and hide the answer you were waiting for. A
  superseded reply is now ignored, the same way its streaming text already was.
- **Newly discovered entities did not appear in an Ask scope picker that was already open.** The
  entity catalogue loaded once and nothing refreshed it when extraction finished, so a session that
  opened Ask before the work graph caught up kept offering no entities until the screen was left and
  re-entered. Finishing a graph job now refreshes it, so the picker offers what has just been found.

### Fixed: four things the PR review found in Ask

- **A question scoped to a thread could answer from other threads.** When the scoped thread's
  sessions had nothing to rank — no summaries or vectors yet — the frame search fell back to the
  whole window and cited whatever else matched, while the turn still recorded the scope. A scope that
  named a thread the app could not find was worse: it searched the window too. The scope is now a
  hard constraint: the frame search stays inside it, and a scope with no sessions to search returns
  no evidence rather than unrelated evidence.
- **Deleting the conversation you were reading brought it back.** A finished turn keeps its
  conversation id in memory, so removing the last conversation put that id straight back into the
  address; the next question then failed because the conversation no longer existed. Deleting now
  clears the live conversation first, so the deletion sticks and the next question starts a new one.
- **Scope and window overrides followed you between conversations.** Clearing the thread scope in one
  conversation, then opening another, sent "clear the scope" for the second — and an entity or window
  override crossed the same way, including through "New conversation". Overrides now belong to the
  conversation they were set in and are dropped when you switch.
- **A custom range could lose an hour at each end on a daylight-saving day.** The range's exclusive
  end was the start plus exactly 24 hours, so in a zone with DST an end date of 1 November stopped at
  23:00 that day, and 8 March ran an hour into the next. The end is now the next local midnight, and
  the shared helper is tested against both switch days.

### Added: Insights engine and agent read surface (backend)

- **Insights are now real, evidenced statements about your history — and agents can read them.**
  Five kinds (`thread activity`, `open loops`, `recurrence`, `semantic neighbours`, `resumption`)
  are generated as typed rows whose payload renders completely without a model: every row carries at
  least one evidence reference (`frame`/`session`/`thread`/`entity`/`mark`), enforced inside the
  store's write transaction, and a row whose evidence disappears is flagged stale and regenerated
  rather than shown as truth.
- **Nothing runs on the capture path and nothing notifies.** Generation is one job under the existing
  worker pool, pressure throttle and idle gate; a failed pass logs and degrades silently. There are no
  badges, streaks, scores or targets.
- **Four read-only HTTP routes** — `/v1/threads`, `/v1/threads/{id}`, `/v1/entities`, `/v1/insights` —
  behind `history.read` for both the legacy and the read credential, with strict query validation and
  documented ordering. A GET never generates, writes or starts inference.
- **Four MCP tools over them** — `list_threads`, `get_thread`, `find_connections`, `list_insights` —
  registered as read tools, so `--read-only` dispatches them and still refuses `add_mark` and Ask.
- **Daily reports and weekly synthesis may cite the window's insights** through an additive context
  block; with no insights in the window the prompt is byte-identical to before.
- `docs/API.md` and `docs/MCP.md` document the routes, the payloads, the evidence shape and the
  ordering. Schema is unchanged (still v14): the tables were already in the baseline.

### Fixed: insights that outlived their evidence, and two ways one could distort a report

- **An insight is never reported as current once what it cites has gone.** The staleness flag was
  only written by the idle generation pass, so between a deleted session (or a frame dropped by
  retention, or a re-clustered thread) and the next pass, `/v1/insights` and the report prompt
  described the row as current. Readers now compute the flag from surviving evidence, using the same
  predicate the sweep writes, and the read path stays write-free.
- **A window full of insights can no longer push a report's grounding out of the prompt.** The
  context block is capped at roughly 1200 estimated tokens and says how many rows it left out; the
  report planner also charges that block against the model's window instead of assuming a fixed
  instruction. A thousand-row block measured ~148,000 tokens before the cap — eighteen times an 8192
  window.
- **`semantic_neighbour` no longer loses connections behind same-day clutter.** The nearest-neighbour
  read used to fetch five rows and then discard the same-day ones, so five closer sessions on one day
  hid a genuine link to another day; the fetch now widens until the rule can be satisfied or the
  candidates run out.
- Test failure messages no longer interpolate session ids (three CodeQL "cleartext logging" alerts);
  they report counts and kinds.

### Changed: App shell and Settings v2

- **The app shell is rebuilt on the v2 design system, and Settings is a searchable, autosaving
  surface instead of one long form.** The navigation rail renders one ordered list (Deck · Recall ·
  Insights · Settings) with no placeholder screens; the status rail reports readiness on v2 surface
  tokens; the command palette (⌘K / Ctrl K) navigates and jumps to any settings category; and the
  four themes of the chosen identity (`pine-light` default, `pine-dark`, `ink-light`, `ink-dark`,
  each with a follow-the-system variant) apply from one place.
- **Settings now has eight categories — General, Capture, Privacy, Intelligence, Models, Data,
  Integrations, Advanced — over 26 settings with one row pattern.** Every row saves on its own and
  offers Undo from a toast; there is no global Save button. Number fields clamp to their bounds,
  hotkeys normalise and warn on cross-row conflicts, monitor and exclusion lists chip-edit, and
  each row shows its own saving/saved/failed state. Destructive actions (exporting history,
  regenerating the legacy API token, revoking an agent's read token) ask first and name the
  consequence — the read-token dialog states that every agent using it stops being able to read your
  history, that the token cannot be restored, and that already authorized requests may continue.
  A category never goes blank:
  loading shows skeletons, a failure shows an error with Retry in that pane only, and partial data
  shows the rows it has.
- **Search finds a setting by what it does or by its key**, matching the label, the dotted key, the
  field name and the help text, with the match highlighted; ↑/↓/Enter select, Escape clears the
  query and leaves you in the current category. Tab order walks the categories and then that
  category's controls with no trap.
- **Saving is now strictly serialised, and two destructive edges are closed.** Overlapping edits
  could previously lose one another (each save wrote the whole settings object, so an earlier save's
  refresh could overwrite a later one); saves now queue and each is built from a freshly read
  snapshot. A failed refresh no longer replaces a category with an error panel — the rows stay.
  Store-image width rejects values between 1 and 319 px instead of accepting a number the backend
  silently raises to 320, monitor toggles no longer turn an excluded display into capture-all when an
  unplugged display is in the saved selection, and retention days save when you press Enter or leave
  the field rather than on every keystroke — typing `365` over `30` used to store a three-day window
  the retention sweep would act on.
- **Rendering fixes carried by the rebuild:** the v1 `Settings.tsx` is deleted (its local
  chord-conflict rule lives once, in the settings module), and the throttle readout no longer
  reports "off" after you switch it on — it shows the saved state immediately and the measured
  numbers as soon as the first sample arrives.


### Added: Ask v2 — session-aware, multi-turn question answering

- **Ask now plans before it retrieves, and answers over conversations instead of single frames.**
  A question is first planned: relative time phrasing ("yesterday", "last week", "this morning")
  becomes an explicit half-open window, and an entity mention resolves against the work graph so
  retrieval is scoped to the sessions linked to it. Retrieval then runs in a fixed order — threads,
  then sessions (full-text on session titles/summaries combined with session-vector similarity),
  then frames inside the winning sessions — with each stage only narrowing the previous one. Every
  answer cites both the frames it actually read and the sessions those frames belong to.
- **Follow-up questions are conversations.** A question starts a conversation and turn 1; asking
  again with the conversation returns the next ordinal and reuses the previous turn's time window
  and entity scope, so "and the day before?" moves the window and keeps the scope. Deleting a
  conversation removes its turns and touches no captured evidence.
- **The answer stream gains exactly one event and one request field.** A single `citations` event
  is emitted immediately before `done`, carrying the cited session ids, the cited frame ids, the
  conversation and turn ids, the per-stage candidate counts and the assembled token total. `done`
  carries the same counts, and both are omitted when absent, so a client that knows nothing about
  Ask v2 renders today's answer unchanged. `POST /v1/ask` accepts an optional `thread_id`.
- **Sessions with a title or summary are embedded in the background.** A new `session_embed` job
  kind keeps the session-vector index warm, in bounded batches ordered most-recently-updated
  first, on idle and on demand under the existing pressure throttle — never on the capture path.
  A session whose title and summary are both empty is never embedded, a deleted session's vector is
  removed with it, and a rewritten or cleared summary invalidates its vector so the session is
  never ranked by text that no longer describes it.
- **A library with no work graph still answers.** With empty graph tables and no session
  summaries, Ask v2 degrades to the existing frame-level search restricted to the planned window —
  the same query, depth and ranking, with the window as the only added filter — so it is never
  worse than the previous Ask.

### Fixed: work-graph links now follow session ownership changes

- **The work graph repairs its session links instead of treating the first extraction as final.**
  Sessions are created, re-created, moved and deleted while history is captured; the database
  cascades removed the graph's session links, and a frame extracted before its session existed
  never gained one. The graph now re-extracts a changed session's owned frames, computes its links
  exactly, and replaces them in one transaction when the graph is idle, so links are restored or
  moved without inflated counts and threads are re-planned afterwards. The historical backfill also
  waits for the one-shot sessions pass, so a frame is never extracted before its session exists.
  Linked evidence now always follows the authoritative frame owner, never a time-interval guess.
  Renaming a thread no longer freezes it against new sessions, merging a split-created thread stays
  merged across runs, and a split that names a since-deleted session can no longer stall thread
  updates. No schema, setting or surface change.

### Added: work graph engine

- **ScreenSearch now derives the work graph behind its history: entities, session links and
  threads.** On idle and on startup, model-free rules read what was already captured (window
  titles, browser URLs, file and repository paths, document names, ticket references and the
  session's recognized tool) and persist deduplicated entities, their links to sessions and
  deterministic threads. The optional annotation pass can add aliases when a local answer model is
  loaded; without it the graph is complete and every entity still comes from the deterministic
  rules. Threads are clusters of sessions that share a strong entity within a 30-day decaying
  window, with rename/merge/split overrides always winning once the UI writes them. Everything is
  derived and recomputable, so clearing it loses no evidence. There is no new screen in this phase:
  the read routes arrive with P8 and the Work screen with P9. Evidence: entity recovery 90.3% on
  the committed fixture against the frozen 80.0% threshold, backfill counts pasted and idempotent
  in `specs/05_BUILD_REVIEW.md`.

### Added: automatic evaluation baseline and the P5/P6 thresholds

- **The intelligence work has a measurable baseline, and P5/P6 must beat it.** A dev-only fixture
  generator (`crates/evalfixture`, never shipped) builds a seeded six-week history through the real
  store writers, and a second subcommand serves the app's own local HTTP API over it with the real
  embedder and answer model, so the evaluation harness measures shipped code. The committed seed
  lets both later phases regenerate the same database. Thresholds frozen from the run: retrieval
  recall@10 37.0% to 41.0%, known-fact answer rate 64.7% to 71.3%, citation validity to 100%,
  entity recovery to 80.0%, p95 latency to 3485 ms or lower. Evidence: `specs/05_BUILD_REVIEW.md`.

### Added — specs

- **0.4.5 arc (P9) contract: work graph, Ask v2, insights, baseline schema v14, settings v2, v2
  design-system placeholders.** Specs-only; zero code. Roadmap: `docs/0.4.5.md`; contracts:
  `specs/03_MASTER_PRODUCTION_SPEC.md` §4, §7h–§7j, §8, §13d and `specs/UI_REFERENCE.md`.

### Added: design system v2 foundation

- **The UI is rebuilt on one token system with four themes.** `ui/src/styles/tokens.css` carries
  primitives, a four-theme semantic layer (pine light and dark, ink light and dark; pine is the
  default) and a density token, with bundled local variable fonts and a compatibility shim so every
  existing screen keeps rendering. 30 primitives and one local icon set are rebuilt on it, and two
  tests enforce AA contrast in every theme and forbid raw colours or pixel font sizes in components.
  A dev-only `/kitchen` route shows every primitive in every state. The `ui.appearance` setting
  (26th UI key) is recorded for P3 and P7.

### Added

- **Setup progress and errors are visible in the app.** While ScreenSearch prepares local AI —
  downloading and installing the llama.cpp engine, preparing the search model, or downloading a
  vision/answer model — a quiet status-bar chip reads "Preparing local AI" with the current
  stage, and Settings · Models lists each step: Checking → Downloading → Verifying → Installing →
  Done. Real progress (bytes and a percentage) appears only when it can be measured; the search
  model shows honest indeterminate progress. A failed step stays visible as "Setup failed" until
  that step runs again, with a plain cause, the next step and the technical details one click away (signed
  URL parameters are stripped). Nothing blocks capture or search, and no new download, setting
  or network destination is added. Spec: `03 §7g`.

- **A dev-only evaluation harness for search and answers.** It writes its own questions from facts
  already in your history (when you last opened a file, which sessions mentioned a repo, what you
  were doing before a session, where a line appeared again), then scores retrieval, citation
  validity, latency and entity recovery. No hand-written question set to maintain and nothing to
  label, so the next improvements are measured rather than guessed.

### Changed

- **One clean database schema, no migration chain.** The database is now created from a single
  baseline at schema version 14, and any other version refuses to open with a message that names
  the `ssv2c-rebuild` tool. That dev-only tool carries an existing history into the new file
  read-only: frames, images, text, embeddings, sessions, marks, reports and the API identity are
  copied, derived indexes are rebuilt, the old file is never modified, and renaming it to
  `*.v13.bak` stays your step. Spec: `03 §4`.
- **Settings are cut down to what a person can actually decide.** The Settings screen edits 25 keys
  instead of 78. Engine tuning moves to a documented `advanced.toml` file beside the database, nine
  retired keys are gone, and an unknown key is rewritten away once instead of being silently
  tolerated. Spec: `03 §8`.

### Fixed

- **A keyboard user could select an option they could not see in the new Combobox.** The design
  system's `Combobox` keeps focus in the input and announces the active option with
  `aria-activedescendant`, but its popup caps its height, and ArrowDown past the seventh option moved
  the active row out of the visible band without scrolling. Enter then committed an option that was
  never on screen. The popup now reveals the active option itself: only its own scroll offset moves
  (measured from rects, so the popup border and padding stay visible at both ends and the page never
  jumps), covering arrow keys, Home/End, wrapping, typing a filter and opening with a selection near
  the bottom. Arrow keys and Home/End also skip disabled options instead of landing on a row that
  cannot be chosen, which is what a native select does. Verified in a browser over a twelve-option
  list: 0 of 15 keyboard steps left the active option hidden, against 7 of 15 with the fix disabled.

- **A loading button could still submit its form.** The design system's `Button` dropped its click
  handler while loading, which stops a handler but not a native form submit: with `type="submit"`,
  `aria-disabled` is announced and ignored, so clicking the spinner or pressing Enter on the focused
  button sent the form again while the first request was still pending. The button now cancels the
  activation itself while loading.

- **Tab could be swallowed at the edge of the new Dialog and Sheet.** Their focus trap wraps from the
  last control to the first and back, but it counted a control inside a collapsed group or an
  inactive tab panel as a candidate, and focusing one of those does nothing, so a Tab at the wrap
  point did nothing either: focus stayed on the panel and the next press repeated it. The trap now
  only counts controls it can actually focus. Verified in the browser: the same Shift+Tab that
  previously left focus on the panel now lands on the visible control, and Escape still returns focus
  to whatever opened the dialog.

- **Local inference could be permanently unavailable on a rate-limited network.** The app
  resolves its llama.cpp sidecar through GitHub's *anonymous* releases API, which is capped at
  60 requests/hour per IP. On a shared address the quota can be exhausted, and a single `403`
  left the app with no `llama-server` for the whole session — search still worked, but vision
  tagging and Ask never did. Resolution now falls back to a pinned, SHA-256-verified Windows
  Vulkan release asset (downloaded from GitHub's release-asset host, which is not subject to the
  API quota); a working API still picks the newest build, and `SSV2C_LLAMA_RELEASE_URL` still
  overrides both. The binary download itself gets up to three attempts on transient
  `401`/`403`/`408`/`429`/`5xx` and transport errors, and the failure message now carries the
  full error chain instead of only the outer context.

## Older versions

Releases 0.4.2 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
