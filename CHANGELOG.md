# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

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
