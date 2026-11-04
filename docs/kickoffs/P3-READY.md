# P3 ready: baseline schema v14 + settings v2

**Branch:** `feat/0.4.5-p3-baseline-schema`, local only. Nothing was pushed, no PR was opened, and
nothing was merged.
**New base:** `origin/main` `3976551` (0.4.5 P1 #150 plus the merged P2 design system #151/#152).
Before today the branch hung off P1 `dad1eb2`; it was rebased with
`git rebase --onto origin/main dad1eb2 feat/0.4.5-p3-baseline-schema`, which replayed five commits
with no conflict. `git diff 3d15756 e27f42c` is empty, so the P1 tip tree is exactly main's #150
squash.
**New tip:** `c039d5e`, plus the records commit that contains this file. Old tip was `48a9705`.

**Commits added today:**

| Commit | What |
|---|---|
| `31f7c66` | Land the P2 pick's `ui.appearance` as the 26th UI setting (typed enum, Settings field, kernel load/save, rebuild count/carry, regenerated bindings) |
| `47b1f1b` | Add the v14 baseline DDL parity test against `03` section 4 (mutation-checked) |
| `1407e5b` | Seed `ui.appearance` in the rebuild fixture and assert it survives |
| `bbd8976` | Drop the stale retired-key specimen from the dev-only kitchen gallery (one-time maintainer grant; outside P3 paths) |
| `c039d5e` | Rewrite the Maintainer run as a nine-step PowerShell procedure; correct the kickoff's settings contract text |

**Acceptance audit:** every item is met with pasted evidence in `specs/05_BUILD_REVIEW.md` under
"2026-11-04: 0.4.5 P3 baseline schema + settings v2"; the two maintainer-only items stay open.

| Check | Verdict |
|---|---|
| a. v14 DDL from `03` section 4, proven by a comparison test | MET: `crates/store/tests/p3_schema_parity.rs`, mutation-tested four ways |
| b. DB not at v14 refuses, naming `ssv2c-rebuild` and the found version; file untouched | MET: `crates/store/tests/baseline.rs:174`, 8/8 pass |
| c. Rebuild tool: read-only source, bytes+mtime unchanged, carry set, derived drops, no rename | MET: `cargo test -p rebuild` 6+5 pass; source `SQLITE_OPEN_READ_ONLY`; no rename flag exists |
| d. Settings v2 per disposition, retired keys gone, `advanced.toml`, rewrite-once, round-trip, exact UI count | MET at 26 UI keys: the P2 pick added `ui.appearance` to `03` section 8, so the kickoff's pre-pick "25" is superseded by the spec and the maintainer's answer. Retired-key grep in `ui/src` is empty; 42 remaining hits in `crates` are the allowed list/comments/test/fixture places |
| e. Eleven new tables empty, nothing writes them outside tests | MET: zero-row assertions plus write grep returning only four test fixtures |
| f. Bindings regenerated and committed, diff clean | MET: `git diff --exit-code -- ui/src/bindings` exit 0 after `cargo test -p traits` |
| Real-DB rebuild, per-table counts, 20-frame Moment spot check, app boots on v14 | OPEN (maintainer only; procedure prepared below) |
| In-app Settings > Integrations check from the 2026-11-04 QA pass | OPEN (maintainer; needs the real profile) |

**P2 impact line:** the literal `git diff <P1 tip> feat/0.4.5-p2-design-directions -- specs/` changes
no `03` section 8 settings key, but the P2 pick merged to main (`7e73e30`) raised the closed UI set
from 25 to 26 by adding `ui.appearance`; the maintainer answered "land it in P3" on 2026-11-04 and
P3 implements it (typed `UiAppearance` enum, `Settings.ui_appearance`, default `pine-light`, kernel
load/save, rebuild carry). The shell theme applier and the Settings select remain P7's.

**Pending in-app check from last night:** Settings > Integrations: turn the HTTP API on, change and
save another setting, the API must still be listening. The fix and its regression test are on this
branch: `ui/src/lib/settingsDraft.ts` and `ui/test/settingsDraft.test.mjs`, committed as `0404dd4`
(original hash `48a9705`, also present in `feat/0.4.5-p4-auto-eval`'s ancestry). The test reproduces
the exact revert scenario (6/6 pass); only the native run remains.

**Maintainer run:** `docs/kickoffs/P3.md`, section "The maintainer run (never run by an agent)": nine
numbered, copy-pasteable PowerShell steps with success criteria, the live path as the code resolves
it, the non-destructive swap (no data-dir or db-path override exists), the Moment spot check and a
one-command rollback. No step deletes anything. An agent did not run any of it.

**Not done, and why:**
- Real-database rebuild, per-table counts before/after, the 20-frame Moment spot check and the v14
  boot: maintainer-only by the arc's rules; prepared as the procedure above, never performed here.
- Native Settings > Integrations check: needs the real profile, which agents may never open.
- `06` #30 (`03` section 4 omits ten as-built DDL details): the new parity test pins the documented
  deltas, but the spec doc pass is out of P3's owned paths; it stays open for the maintainer.
- `06` #32 (confirm the wider rebuild carry set), `06` #33 (three section 8 defaults disagree with
  the as-built code) and the `07` question about importing an existing profile's demoted `file-only`
  values into `advanced.toml`: product decisions only the maintainer can make; all logged.
- `feat/0.4.5-p4-auto-eval` was deliberately not touched. Because P3 was rebased, P4 still carries
  the old P3 hashes, so it needs a rebase onto this tip before its turn; the Settings fix stays
  where it is and was not moved.
