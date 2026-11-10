# Run state

Run: off-arc stabilisation toward v0.4.6 (unattended). Not an arc, not a phase.
Started: 2026-11-10
Last updated: 2026-11-10 (item 1 live proof running)
Branch: `fix/0.4.6-stabilisation`, created from `origin/main` at `93a7219`. Local commits only.

| Item | Status | Last commit | Notes |
|---|---|---|---|
| 1 `06` #150 launch panic (`UpdaterState` ordering) | in progress: live proof and Node 22 gate | 209ede8 (to be amended with evidence) | Regression fails with the production panic, then passes. Initial gate green (1653 passed); 20 cold dev launches are running. |
| 2 `06` #116 Ask turn whose provider never answers | blocked (records committed) | see item 2 commit | No turn bound exists in `03 §7i`, and `§8` forbids a hardcoded threshold with a closed 26-key UI set. Failing fast would change documented first-use downloads. Question filed in `07` under `06` #116. |
| 3 `TODO-4` `throttleStatus` invalidation | done (code + regression + gate) | pending commit | Shared `SETTINGS_SAVE_INVALIDATIONS`, readout prop removed; regression fails on the old three-key set. |
| 4 `TODO-6` settings defaults over IPC | done (code + contract test + gate) | pending commit | New `get_settings_defaults`; consumers migrated off the mirror; contract test proven by mutation. |
| 5 `06` #143 `get_insights` removal (only if no importer) | done (no code change; importer found) | pending commit | `commands.ts:211-215` to `queries.ts:303-311` to `Deck.tsx:29,72`. Removal would break the Deck. |
| 6 `06` #137 and #138 `Expander` and the v1 icon set | done (no code change; importers live) | pending commit | `Expander` has 4 references; `components/icons` has 27 importing files. |
| 7 `06` #42 stale `jobs.kind` comment in `03 §4` | code changed, gate pending | pending commit | Comment-only: both copies now list all eight `JobKind` tokens; parity test passes. |
| 8 `07` #91 monitor hot-unplug stalls capture | blocked (records written, no code change) | pending commit | No spec sentence on a topology change; `UI_REFERENCE.md:234` asks only for honest degradation. Question and seam recorded. |
| 9 Release preparation for v0.4.6 | pending | none yet | Only if item 1 is done and verified. |
| 10 `docs/kickoffs/MORNING.md` | pending | none yet | Last commit of the run. |

Rules in force: no merge, no push, no pull request, no commit to `main`, no review trigger. One
commit per item, staged by path by the lead only; subagents never run git. Nothing touches a real
database, a model file, a token, a secret or the installed app's profile. No em dash in new text.

Resume rule: read this table and continue at the first item that is not `done` or `blocked`.

Environment: Rust 1.95.0; the host default is Node v26.12.1. A checksum-verified portable
Node v22.23.4 is now available at
`C:/Users/nicol/screensearch-evidence/0.4.6/node22/node-v22.23.4-win-x64/`.
Final gates use that Node 22 with its npm 10 and an external `CARGO_TARGET_DIR`.
Logs live outside the repository under `C:/Users/nicol/screensearch-evidence/0.4.6/`.
