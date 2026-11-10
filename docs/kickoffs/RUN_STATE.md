# Run state

Run: off-arc stabilisation toward v0.4.6 (unattended). Not an arc, not a phase.
Started: 2026-11-10
Last updated: 2026-11-10 (before item 1)
Branch: `fix/0.4.6-stabilisation`, created from `origin/main` at `93a7219`. Local commits only.

| Item | Status | Last commit | Notes |
|---|---|---|---|
| 1 `06` #150 launch panic (`UpdaterState` ordering) | in progress | none yet | Decides whether v0.4.6 exists. |
| 2 `06` #116 Ask turn whose provider never answers | pending | none yet | |
| 3 `TODO-4` `throttleStatus` invalidation | pending | none yet | |
| 4 `TODO-6` settings defaults over IPC | pending | none yet | |
| 5 `06` #143 `get_insights` removal (only if no importer) | pending | none yet | |
| 6 `06` #137 and #138 `Expander` and the v1 icon set | pending | none yet | |
| 7 `06` #42 stale `jobs.kind` comment in `03 §4` | pending | none yet | |
| 8 `07` #91 monitor hot-unplug stalls capture | pending | none yet | |
| 9 Release preparation for v0.4.6 | pending | none yet | Only if item 1 is done and verified. |
| 10 `docs/kickoffs/MORNING.md` | pending | none yet | Last commit of the run. |

Rules in force: no merge, no push, no pull request, no commit to `main`, no review trigger. One
commit per item, staged by path by the lead only; subagents never run git. Nothing touches a real
database, a model file, a token, a secret or the installed app's profile. No em dash in new text.

Resume rule: read this table and continue at the first item that is not `done` or `blocked`.

Environment note: this machine has Node v26.12.1 only (CI pins Node 22); Rust is 1.95.0. Gate logs
live outside the repository under `C:/Users/nicol/screensearch-evidence/0.4.6/`.
