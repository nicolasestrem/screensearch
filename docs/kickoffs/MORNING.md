# Morning report, 0.4.5 wave A night run

Local commits only: nothing was pushed, no PR was opened, nothing was merged. One exception from
before the rules took effect: PR #150 (P1) is open from an earlier turn.

## Tasks

| Task | Status | Branch | Commit |
|---|---|---|---|
| 1 P1 specs contract | done | `feat/0.4.5-p1-specs-contract` | 3446d9b |
| 2 P1 self-test | done | `feat/0.4.5-p1-specs-contract` | 83e281a |
| 3 P2 design directions | done, stopped at the checkpoint | `feat/0.4.5-p2-design-directions` | cdf20ba |
| 4 Phase kickoff files P2 to P10 | done | `feat/0.4.5-p1-specs-contract` | dad1eb2 |
| 5 P3 baseline schema and settings v2 | done, maintainer run open | `feat/0.4.5-p3-baseline-schema` | b431bc8 |
| 6 P4 evaluation harness | done, baseline run open | `feat/0.4.5-p4-auto-eval` | cc1d916 |

Full suite on the P4 tip, which contains every P3 change: `npm ci`, `npm run lint`, `npm run
build`, `npm test`, `stage-mcp`, `cargo fmt --check`, and `clippy`, `build` and `test` on the whole
workspace with `--locked`, all exit 0: **1241 tests passed, 0 failed**, 82 UI tests. The P3 branch
was also verified on its own. Raw output: `specs/05_BUILD_REVIEW.md`, 2026-11-04 QA entry.

## QA pass (2026-11-04): what it found

An independent review plus per-branch testing found real defects in my overnight work. All are
fixed, each with a test confirmed to fail on the old code:

- **P3 would not have merged.** Its `Cargo.lock` carried P4's dependencies, so the locked MCP
  build failed on P3 alone. It only passed on the combined tree.
- **Blocker: a Settings save could switch the HTTP API off.** Enabling the API in the panel and
  then saving any other setting wrote the stale "disabled" value back and stopped the listener.
- **The rebuild would have re-segmented your history,** because it dropped the session backfill
  checkpoint and four other internal markers. It now carries them, and a test fails if a future
  marker is added without being classified.
- **A failed first run would have bricked the app,** leaving an empty file the app refused forever.
- **Four P4 metrics did not match the definitions the report prints.** They are fixed before they
  freeze as Gate A thresholds.

**One check only you can do** (I cannot run the app: it would open your real profile): in
Settings · Integrations, turn the HTTP API on, then change any other setting and save. The API
must stay listening.

## The three design directions

Open all three side by side:

```
start docs\kickoffs\P2-directions\compare.html
```

Or one full screen: `start docs\kickoffs\P2-directions\direction-a.html` (also `b`, `c`).

- **A, Command Deck refined.** The v1 console kept and refined: warm graphite, one signal orange,
  hairline discipline, new type scale and density system, light theme added.
- **B, Paper Ledger.** Screen history as a working paper: light-led, serif display voice, printed
  tables, footnote citations, no cards.
- **C, Signal Console.** An instrument panel: dense, cool slate, cyan for action and violet only for
  relationships, monospace measurements, context rail on every screen.

Each is one self-contained HTML file, system fonts, no network request, two themes, the same five
screens. Pick one or name a merge; the choice goes into `specs/UI_REFERENCE.md` §1 (0.4.5 D4).

## P1 self-test final question list

`docs/kickoffs/P1-selftest.md`. Four questions in round 1: one answered by your P1 decision, one
fixed in `03 §7h.5` (merge override semantics), two logged as phase-level choices (P6 candidate
budgets, P8 per-kind payload shape). Round 2: zero new unanswered questions.

## Questions logged tonight

In `specs/07_KNOWN_GAPS.md`:

1. An existing profile loses customised file-only values, because `advanced.toml` is now the single
   source. One-time import, or accept the reset and put it in the release notes?
2. P3 outcome notes: the rebuild carry set, the three stale defaults, and the hot-apply change for
   `api.*`.
3. P4 metric conventions to confirm at Gate A (involvement on title and URL only, NULL session end
   as open, nearest-rank percentiles, `app` not a reference kind).
4. Earlier P1 residue: P6 stage budgets and the P8 payload shape.

In `specs/06_PATCH_PLAN.md`:

- **#29** Density is a Settings choice with no settings key (P2/P7 blocker).
- **#30** `03 §4` does not describe ten as-built details of the v13 baseline (documentation gap,
  code is correct).
- **#31** Rebuild tool home: spec said `crates/harness`, built as `crates/rebuild`, spec updated.
  Resolved.
- **#32** Rebuild carry set wider than the kickoff list: `text_spans`, `vision_analysis`,
  `tags`/`frame_tags`, `report_deliveries`.
- **#33** Three `03 §8` defaults disagree with the as-built code (`storage.max_width`,
  `storage.retention_days`, `text.chrome_suppress_min_seen`).

## Maintainer actions at Gate A, in order

1. Read the three directions: `start docs\kickoffs\P2-directions\compare.html`.
2. Push and open the PRs yourself, in this order (P4 already contains the P3 commit):

```
git push -u origin feat/0.4.5-p2-design-directions
git push -u origin feat/0.4.5-p3-baseline-schema
git push -u origin feat/0.4.5-p4-auto-eval
```

3. Merge P1 (#150) if the spec changes are still to your liking, then P1's follow-up commits
   (`83e281a`, `dad1eb2`) are already on that branch.
4. Pick a direction and write it into `specs/UI_REFERENCE.md` §1.
5. Run the P4 baseline against your real history, with the app running:

```
cargo run -p harness --bin harness -- eval --db "%APPDATA%\app.screensearchv2c.desktop\screensearch.db" --api http://127.0.0.1:43210 --out harness-data/eval-baseline
```

6. Run the rebuild sequence in `docs/kickoffs/P3.md`, section "The maintainer run": copy the profile,
   `--counts`, rebuild into a new file, compare, park the original, boot, open the twenty sampled
   frames, and only then the `*.v13.bak` rename.
7. Answer the open questions above, which unblock P7 (density) and confirm the rebuild carry set.
8. Dogfood v14 for three working days, then tag v0.4.3.

## Deliberately not done

- No push, no PR, no merge, no branch delete.
- The rebuild tool was never run against a real or copied real database, and the twenty frame spot
  check was not performed: those are yours (the tool's only agent evidence is the synthetic fixture
  test). Same for the P4 baseline report, which needs your database and a running app.
- No token file, Tailwind config, primitive component or app route: P2 stops at the direction
  checkpoint by design.
- `Settings.tsx` was trimmed by 40 controls so the app builds after the settings cut. P7 rebuilds
  that page by category; the `file-only` values are not yet displayed anywhere, though the read-only
  `get_advanced_config` projection they will use exists.
- Nothing from P5 onward was started.
- I did not correct the three stale `03 §8` defaults, the ten `03 §4` omissions, or the density key:
  each is a contract change and needs your call.
