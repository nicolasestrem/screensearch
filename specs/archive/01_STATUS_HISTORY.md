# 01 — Status history (pre-cleanup §8)

> This file preserves the long historical status narrative that lived in
> `specs/01_PROJECT_CONTEXT.md §8` before the post-v0.4.5 `specs/` cleanup, **moved verbatim on
> 2026-11-09**. The live `§8` is now a short current-state summary; nothing was deleted from this
> record. It also preserves the superseded current-arc pointer text from
> `specs/04_CLAUDE_CODE_BUILD_PROMPT.md §1` (the 2026-10-19 "Current arc status" paragraph and the
> per-phase 0.4.5 work pointer), which was retired when v0.4.5 shipped. The rule wording of those
> 0.4.5 constraints stays in `04 §1` and `CLAUDE.md`.

## §8 Status as it stood at the 2026-11-09 cleanup (verbatim)

## 8. Status

- **All context items decided.** License is **MIT**. Nothing outstanding.
- **Current state (2026-11-07):** latest release **v0.4.5** (2026-11-07), auto-update live since
  v0.3.2. DB **schema 15**, created from one baseline (`03 §4`, 0.4.5 D2) with **no migration
  chain**: a database at any other version is refused untouched, and the dev-only `ssv2c-rebuild`
  tool carries a v13 history into the new file read-only, keeping the old file as `*.v13.bak`
  (0.4.5 D3). **Audience is a single user** (0.4.5 D1): no forward-compat shims, no retired-key
  tolerance and no multi-environment validation. Only the Windows NSIS installer, its updater
  `.sig` and `latest.json` are release assets; standalone MCP wrapper archives are not published.
  Code-signing is the lone packaging follow-up (`03 §11b`). Active work is tracked in `05`-`08`,
  shipped history is in `CHANGELOG-ARCHIVE.md` and `specs/archive/`.

- **0.4.5 arc, merged and released (2026-11-07):** every phase of Waves A-C is on `main` and
  ships in v0.4.5. **P1** specs contract (#150), **P2** design system v2 (#151), **P3** baseline
  schema plus settings v2 (#153; the working tree carries one baseline instead of a migration
  chain, `03 §4`, 0.4.5 D2), **P4** automatic evaluation with the frozen P5/P6 thresholds (#155),
  **P5** work graph engine (#157, review fixes #159; pure `crates/graph`, graph persistence, four
  kernel job kinds and the idle scheduler), **P6** Ask v2 (#160; planning, the four ordered
  retrieval stages, both-kind citations, multi-turn conversations and the `session_embed` lane),
  **P7** shell plus Settings v2 (#161, #162, #163), **P8** insights and the agent read surface
  (#164; five kinds, four read routes, four MCP tools), **P9** the Work, Thread, Insights v2 and
  Ask v2 screens (#165; schema **15** adds `ask_turns.citations` and `ask_turns.scope_thread_id`,
  so a pre-P9 database is refused) and **P10** the Recall surfaces refont with the v1 token shim
  removed (#166). The executing phases ran under the maintainer's overrides (fixture evidence
  instead of the real-DB maintainer run, D18 waived per branch on the maintainer's explicit
  instruction); the release QA pass and its decisions are in `05` and `07`. Gate A (`v0.4.3`) and
  Gate B (`v0.4.4`) tags remain the maintainer's calls; **no `v0.4.3` or `v0.4.4` tag exists**. The
  full app is a Rust workspace of **18 module crates** plus `src-tauri` (the Tauri composition
  root) and a React/TS UI.

- **P8 insights + agent surface (shipped in v0.4.5, #164):** five kinds (`thread_activity`,
  `open_loop`, `recurrence`, `semantic_neighbour`, `resumption`) generate from history as job kind
  `insight_generate` under the
  existing worker pool, pressure throttle and idle gate; the store writes each insight with its
  evidence in one transaction, and a sweep dead-letters a pass that would leave one unevidenced. Four
  read routes (`GET /v1/threads`, `/v1/threads/{id}`, `/v1/entities`, `/v1/insights`, all
  `history.read` with strict parsing) and four MCP tools (`list_threads`, `get_thread`,
  `find_connections`, `list_insights`; counts 13→17 normal / 10→14 read-only). Daily reports and
  weekly synthesis may cite the window's insights through an additive prompt context block. **No
  schema change** (v14 already carried `insights`, `insight_evidence`, `session_vectors`) and **no
  UI** — P9 renders the screens. Verification and evidence are in `05` (2026-11-06), contradictions
  `06` #93–#101.

- **P6 Ask v2 (shipped in v0.4.5, #160):** Ask is now
  planned before it retrieves (time intent → half-open window in system-local calendar terms; entity
  mentions resolved through the work graph with a plain-text fallback), retrieves in the normative
  stage order (threads → sessions → frames → assembly, each stage narrowing the previous), cites both
  the frames it read and the sessions those frames belong to, and persists conversations
  (`ask_threads`/`ask_turns`) so a follow-up reuses the previous window and entity scope. One new
  terminal `citations` event precedes `done`; `done` additionally carries the per-stage counts and
  the assembled token total; `POST /v1/ask` accepts `thread_id`. A new `session_embed` job lane keeps
  `session_vectors` warm on idle and on demand, never on the capture path. The legacy frame-level Ask
  keeps its exact semantics and is now written once (`kernel::ask::ask_context`), and a database with
  an empty work graph degrades to that frame-level path with the planned window as the only added
  filter. **No schema, settings-key, table or route change.** Acceptance (evidence recall@10,
  known-fact rate, citation validity, p95) is the maintainer's live-DB run and is not yet recorded;
  the build record is `05`, the decisions `07`, the contradiction rows `06` #46–#53.
- **P5 review-fix pass (shipped in v0.4.5, #159):** the P5 work graph (`722f29e`, PR #157) still had
  one P1 and three P2 automated-review findings unresolved at merge time. The fixes landed on
  `fix/p5-157-review-findings` ([#159](https://github.com/nicolasestrem/screensearch/pull/159),
  **merged** 2026-11-05): per-session link
  reconciliation after session ownership changes, ownership-only linking, a sessions-history
  ordering gate, and the override-model corrections (rename membership, merge retention,
  stale-session filtering). No schema, settings, job-kind, command or UI change. The repository now
  carries a hard rule in `CLAUDE.md`: agents never merge pull requests; merges are maintainer-only.

## Superseded 04 §1 current-arc pointer text (verbatim, retired 2026-11-09)

**Current arc status (2026-10-19):** the 0.4.2 arc — external-agent memory API (`§1b`) and F3
deeper memory (`§1c`) — is released as v0.4.2 (2026-10-19); open native acceptance is tracked in
`05`. **The next arc is 0.4.5** (2026-11-03; pointer below and `02 §5k`), and nothing is defined
past it — stop and ask before starting any other arc work (`§5`). The per-increment pointers below
stay valid for follow-up work on those areas.

**For 0.4.5 work (P9 — the work-intelligence + design-system-v2 arc, approved 2026-11-03):** also
read `docs/0.4.5.md` (phase order, per-phase acceptance, file ownership), `02 §5k`, and the
normative `03 §4`/`§7h`/`§7i`/`§7j`/`§8` with `UI_REFERENCE` (identity, tokens, IA, components).
The P2 design system is merged to `main` (`7e73e30`): read the
identity from `UI_REFERENCE` §1/§2 and the shipped primitives from
`ui/src/components/primitives/` with `ui/src/styles/tokens.css`, do not re-derive either. Every
theme is one of `pine-light` (default), `pine-dark`, `ink-light`, `ink-dark` via `data-theme`, and
the `ui.appearance` setting selects between them (`03 §8`). The P3 baseline + settings v2 work is
merged to `main` as `0dfa2f5` (PR #153); the committed tree is the code of record for schema,
settings and the rebuild tool, and `docs/kickoffs/P3-READY.md` is its hand-off. P4 (the
fixture-backed automatic evaluation and the frozen P5/P6 thresholds) is merged as `aceca58` (PR
#155), and P5 (the work graph engine) is merged as `722f29e` (PR #157); read its acceptance
evidence and recorded decisions in `05`/`06` #37-#42/`07` before starting P7 work. P6 (Ask v2) is
merged ([#160](https://github.com/nicolasestrem/screensearch/pull/160)), as are P7 (shell + Settings
v2, [#161](https://github.com/nicolasestrem/screensearch/pull/161)) and its follow-ups (#159, #162,
#163): read P6's build record,
the fourteen maintainer decisions/grants and the eight contradiction rows in `05`/`07`/`06` #46-#53
before touching the ask path, the `session_vectors` lane or `Kernel::ask_turn`. Its acceptance
numbers are the maintainer's live-DB run and are not yet recorded.

P8 (insights + agent surface) is implemented on `feat/0.4.5-p8-insights-agent` and open as
[#164](https://github.com/nicolasestrem/screensearch/pull/164) (committed on the maintainer's
instruction, not merged): the five insight kinds, the `insight_generate` job kind, the four
`history.read` routes and the four MCP tools — read `05` (2026-11-06) and `06` #93–#101 first. P9
(the intelligence UI) has not started: `/ask` and `/work` do not exist yet.
