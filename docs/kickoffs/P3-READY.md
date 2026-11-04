# P3 ready: baseline schema v14 + settings v2

**Branch:** `feat/0.4.5-p3-baseline-schema`. Pushed to `origin` and opened as a PR against `main`
after the maintainer's explicit 2026-11-04 instruction; **not merged**. No other branch and no
`main` commit was touched.
**Base:** `origin/main` `3976551` (0.4.5 P1 #150, the P2 design system #151 and its records #152).
The branch hung off P1 `dad1eb2`; it was rebased with
`git rebase --onto origin/main dad1eb2 feat/0.4.5-p3-baseline-schema`, clean, five commits replayed.
`git diff 3d15756 e27f42c` is empty, so the P1 tip tree is exactly main's #150 squash.
**New tip:** `8833c41` for code and docs, followed by the specs/records commit that carries this
file and the PR-number commit. Old overnight tip: `48a9705`.

**Commits added today:** `31f7c66` (`ui.appearance` as the 26th UI key), `47b1f1b` (baseline DDL
parity test), `1407e5b` (rebuild fixture seeds the new key), `bbd8976` (dev-gallery stale specimen,
one-time grant), `c039d5e` (Maintainer run + contract text), `79d3976` (first records), `3d9d89a`
(parity test requires the documented extras and anchors the spec parse), `b53db47` (rebuild refuses
a missing carry table or foreign-key violation, 20-id sample test), `8aca7a5` (settings doc
comments + regenerated bindings), `244638a` (P3-authored em dashes removed), `9d3a9ee` (restored
`ctx_size` sentinel and overlay-hotkey persistence coverage), `8833c41` (hardened Maintainer run),
plus the records commits.

**Acceptance audit:** every non-maintainer item is MET with evidence in `specs/05_BUILD_REVIEW.md`
under "2026-11-04: 0.4.5 P3 baseline schema + settings v2"; the two maintainer-only items stay open.

| Check | Verdict |
|---|---|
| a. v14 DDL from `03` section 4, proven by a comparison test | MET: `crates/store/tests/p3_schema_parity.rs`, mutation-tested five ways; the documented extras are required, not merely allowed |
| b. DB not at v14 refuses, naming `ssv2c-rebuild` and the found version; file untouched | MET: `crates/store/tests/baseline.rs:174`, 8/8 pass |
| c. Rebuild tool: read-only source, bytes+mtime unchanged, carry set, derived drops, no rename | MET: rebuild lib 6 + integration 8 tests; a missing carry table and a foreign-key violation now fail loudly with the exact rows |
| d. Settings v2 per disposition, retired keys gone, `advanced.toml`, rewrite-once, round-trip, exact UI count | MET at 26 UI keys: the P2 pick added `ui.appearance` to `03` section 8, so the kickoff's pre-pick "25" is superseded by the spec and the maintainer's answer. Retired-key grep in `ui/src` is empty; the 42 remaining hits in `crates` are the allowed list/comments/tests/fixtures. 33/33 kernel settings tests include the new key |
| e. Eleven new tables empty, nothing writes them outside tests | MET: zero-row assertions plus the write grep returning only four test fixtures |
| f. Bindings regenerated and committed, diff clean | MET: `git diff --exit-code -- ui/src/bindings` exit 0 after `cargo test -p traits` |
| Real-DB rebuild, per-table counts, 20-frame Moment spot check, app boots on v14 | OPEN (maintainer only; procedure prepared) |
| In-app Settings > Integrations check from the 2026-11-04 QA pass | OPEN (maintainer; needs the real profile) |

**P2 impact line:** the literal `git diff <P1 tip> feat/0.4.5-p2-design-directions -- specs/` changes
no `03` section 8 settings key, but the P2 pick merged to main (`7e73e30`) raised the closed UI set
from 25 to 26 by adding `ui.appearance`; the maintainer answered "land it in P3" on 2026-11-04 and
P3 implements it (typed `UiAppearance` enum, `Settings.ui_appearance`, default `pine-light`, kernel
load/save, rebuild carry). The shell theme applier and the Settings select remain P7's.

**Independent adversarial validation (16 subagents, 2026-11-04):** two waves of read-only reviewers
(schema parity, rebuild, settings, maintainer procedure, records truthfulness, ownership/hygiene,
topology, clean-worktree build, plus cross-checkers) found and confirmed six should-fix defects and
several record nits. All were fixed in the commits above: the parity test no longer tolerates a
missing documented extra, the rebuild no longer skips a missing carry table or a foreign-key
violation silently, the maintainer procedure is guarded and idempotent, the two dropped tests were
restored, 83 P3-authored em dashes were removed, and the record claims were corrected. No reviewer
edited a tracked file, and no real database, model, `.env` or secret was touched.

**Pending in-app check from last night:** Settings > Integrations: turn the HTTP API on, change and
save another setting, the API must still be listening. The fix and its regression test are on this
branch: `ui/src/lib/settingsDraft.ts` and `ui/test/settingsDraft.test.mjs`, committed as `0404dd4`
(original hash `48a9705`, also present in `feat/0.4.5-p4-auto-eval`'s ancestry). The test reproduces
the exact revert scenario (6/6 pass); only the native run remains.

**Maintainer run:** `docs/kickoffs/P3.md`, section "The maintainer run (never run by an agent)": nine
numbered, copy-pasteable PowerShell steps with success criteria, the live path as the code resolves
it, a guarded non-destructive swap (no data-dir or db-path override exists), a log-gated
`*.v13.bak` rename only after the app opens on v14, a 29-day-bounded Moment spot check and an
idempotent one-command rollback. An agent did not run any of it.

**Verification:** all nine commands from `04` section 4 exit 0 on the hardened tree (npm ci, lint,
build, stage-mcp, fmt, clippy, cargo build, `cargo test --workspace` 1199 passed / 0 failed,
bindings diff clean), plus an isolated clean-worktree `fmt`/`clippy`/`test` run of `store` and
`rebuild`. Raw output is in `05` under 2026-11-04, including the first run and the post-hardening
re-run.

**Not done, and why:**
- Real-database rebuild, per-table counts before/after, the 20-frame Moment spot check and the v14
  boot: maintainer-only by the arc's rules; prepared as the procedure above, never performed here.
- Native Settings > Integrations check: needs the real profile, which agents may never open.
- `06` #30 (`03` section 4 omits ten as-built DDL details): the parity test pins the documented
  deltas, but the spec doc pass is out of P3's owned paths; it stays open for the maintainer.
- `06` #32 (confirm the wider rebuild carry set), `06` #33 (three section 8 defaults disagree with
  the as-built code) and the `07` question about importing an existing profile's demoted `file-only`
  values into `advanced.toml`: product decisions only the maintainer can make; all logged.
- `feat/0.4.5-p4-auto-eval` was deliberately not touched. Because P3 was rebased, P4 still carries
  the old P3 hashes and needs a rebase onto this tip before its turn; the Settings fix was not moved.
