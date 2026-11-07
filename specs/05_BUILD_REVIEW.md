# 05 - Build Review

The 0.4.5 arc shipped in v0.4.5. Ten phases, PRs #150-#166: P1 #150 `e27f42c`, P2 #151 `7e73e30`,
P3 #153 `0dfa2f5`, P4 #155 `aceca58`, P5 #157 `722f29e` (plus the review-fix pass #159), P6 #160
`6d906a7`, P7 #161 `5d15301`, `60c60d8`, `8e0883a`, P8 #164 `eb556d1`, P9 #165 `9112087` and
P10 #166 `e459185`. The full build record, every ordered gate, raw log excerpt, acceptance walk,
measured budget and review-fix pass included, is archived at
`specs/archive/05_BUILD_REVIEW.v0.4.5.md`. New entries start below.

## Current arc

### 2026-11-07: v0.4.5 installed-build QA fixes (branch `fix/0.4.5-qa-bugs`, from `main` `9c6a2cb`)

Phase: three defects from hand-testing the installed 0.4.5 candidate (a spurious hotkey warning on
every settings save, literal Markdown on `/ask`, misaligned Settings controls). Decisions are
`07` QA2 D1-D4, root causes `07` QA2 F1-F3, the deliberately unfixed observations `07` QA2 U1-U3;
the work record is `08`. PR #168 merged first, and these fixes landed as PR #169 (`d98ad66`).

**What was reviewed before the gate.** Each slice was inspected rather than accepted on report:
`git diff` on `routes/Ask.tsx` shows only the two answer paragraphs plus comments replaced (the
citations row, both notes, Retry and the reserved `min-h-24` slot are byte-identical), the settings
fix is a class-string change plus one descriptor module with no primitive or category file touched,
and `overlay.rs` keeps its handlers, status bookkeeping and toast channel while replacing the two
registration paths with one `apply_hotkey` over an internal `HotkeyEnv` seam. The em-dash rule was
checked on the whole diff: the only occurrences are two pre-existing comment lines that the diff
*removes*.

**Field evidence for the hotkey defect** (gathered before any code changed): an OS probe from
another process returned Win32 `1409` for both `Ctrl+Alt+M` and `Ctrl+Alt+Z`, so the installed
process already held both chords, while the toast said nothing was registered and omitted the
"; still using the previous combination" suffix that only the "nothing registered" branch produces.

**Pre-fix demonstration** (required proof that the new tests catch the defect). The guard inside
`apply_hotkey` was temporarily reverted to the old raw-string comparison, `registered = None` on
failure and raw error text in the warning; the seam means that reverted function *is* the pre-fix
logic. Result: `4 passed; 5 failed`, including a verbatim reproduction of the field message
(`key: KeyM, id: 589855`), a duplicate registration attempt, and three repeated toasts for one
unchanged chord. The file was then restored and verified byte-identical
(SHA-256 `7eadd0dd55fa0ef046f6b971a77c35fa304f26799045ff0ccc26d20cfe66246d` both ways;
`grep -c PRE-FIX` returns 0).

**Ordered gate** (`CLAUDE.md` order: UI first, then the sidecar, then cargo), raw summary of
`gate.log` (the full log is outside the repository at
`C:\Users\nicol\screensearch-evidence\gate.log`):

````text
ui_npm_ci      EXIT=0     ui_npm_lint  EXIT=0     ui_npm_test  EXIT=0  (172 tests, 172 pass, 0 fail)
ui_npm_build   EXIT=0     stage_mcp    EXIT=0  (up to date)
cargo_fmt      EXIT=0     cargo_clippy EXIT=0     cargo_build  EXIT=0   cargo_test EXIT=0
cargo_test     EXIT=0  (86 test binaries: 1625 passed, 0 failed, 16 ignored)
bindings_diff  EXIT=0  (git diff --exit-code -- ui/src/bindings prints nothing)
OVERALL_FAIL=0
````

The UI suite grew from 160 to 172 tests (`answer-prose` 8, `settingRowColumns` 4; the five
`contrast` tests are unchanged and now share their CSS parser and WCAG math through
`test/lib/contrastTokens.mjs`). The Rust suite grew by the 9 new `overlay::tests`. `cargo clippy`
prints only the pre-existing `failed to parse serde attribute` build-script noise from `ts-rs`,
which is not a lint and does not affect the exit code.

**Real-browser measurement of the alignment fix** (throwaway Vite harness rendering the real
`SettingRow`, the real `Toggle` and the real compiled stylesheet, measured in Chromium; the harness
and its server were deleted afterwards):

| Row | Hint | Switch left x (px) | Toggle root width (px) |
|---|---|---|---|
| new layout (short hint) | 42 chars | 368 | 384 |
| new layout (long hint) | 146 chars | 368 | 384 |
| old layout (short hint) | 42 chars | 224 | 240 |
| old layout (long hint) | 146 chars | 705 | 721 |

The old layout moves the switch 481 px with the hint text, which is the reported defect exactly;
the new layout puts both switches at the same x and gives both toggles the same 384 px column.

**Real-browser check of the rendered Markdown** (same harness, real `AnswerProse` inside
`prose prose-deck`, theme switched on `:root[data-theme]`, WCAG contrast of the computed colours):

| Theme | h2 / strong / li / ol li / inline code | link |
|---|---|---|
| pine-light | 15.15 | 5.76 |
| pine-dark | 16.20 | 10.26 |
| ink-light | 15.13 | 5.32 |
| ink-dark | 16.08 | 8.97 |

Every element clears AA in every theme; the tightest is a link at 5.32:1, and no token was
changed. The harness sample had no `em` or fenced block, so those two elements are covered by the
SSR assertion and the token table test rather than by this pixel run.

**Release dry run and installed build.** `gh workflow run release.yml --ref main` on `d98ad66`
(run 42374416657) finished green in 8m21s and produced `updater-bundle`: `ScreenSearch_0.4.5_x64-setup.exe`
(15,900,172 bytes), its `.sig`, and `latest.json` (`version` 0.4.5, one `windows-x86_64` platform),
downloaded to `C:\Users\nicol\screensearch-evidence\dryrun2`. No tag and no release were created.

Installed over the previous per-user install with `/S` (installer exit code 0, no wipe: no NSIS
hooks are configured and nothing touches `%APPDATA%\app.screensearchv2c.desktop`). Installed binary
59,957,248 bytes, `FileVersion`/`ProductVersion` 0.4.5, uninstall entry `DisplayVersion` 0.4.5,
launched as pid 33928.

Log since launch (22 lines, `screensearch.log.2026-11-07`):

````text
store opened db=...\screensearch.db schema_version=15
WARN screensearch_lib::overlay: global hotkey registration failed slot="marks.hotkey" chord=Ctrl+Alt+M
ERROR lines since launch: 0
````

Data intact: `frames` 385, `sessions` 2, `marks` 0; newest frame `captured_at` 13:35 local, i.e.
history from before the upgrade survived and the store opened at schema 15.

Hotkey behaviour in the installed build: **Ctrl+Alt+Z works end to end** (the chord registered, and
pressing it opened `ScreenSearch Overlay` at 640x460 and pressing it again hid it). **Ctrl+Alt+M
could not be registered**, and the log proves the failure is external rather than this app's
bookkeeping: the registration was attempted only because the plugin registry was empty, and the OS
answered 1409. The same condition produced the reported toast in the old build, whose `init_*`
cleared the stored chord and then retried it on every save. The shipped binary contains the
humanized sentence (`another app is already using that shortcut`, `the shortcut could not be
registered`, `still using the previous combination`) and no longer contains the old
`HotKey unavailable` prefixes (`grep -ac` counts 1, 2, 1 and 0). Consequence for this machine: the
mark chord stays unavailable until whichever other program holds Ctrl+Alt+M releases it, and
ScreenSearch reports that once instead of on every save.

**Observed and out of scope:** capture reported `capture started` but stored no frame after 13:35
local, before or after the upgrade (`capture.interval_ms` 3000, diff gate active, zero errors). No
line of this change touches capture; recorded for the maintainer rather than investigated here.

### 2026-11-07: v0.4.5 release QA: merged-tree gate, static release checks and the P4 harness (branch `chore/0.4.5-release`, from `main` `440d2e9`)

Phase: the maintainer's 0.4.5 release kickoff. Decisions taken without asking are `07` QA D1-D6 and
QA F1; the raw logs and the harness report live outside the repository
(`C:\Users\nicol\screensearch-evidence`). Every number below is a paste of that output, not a
summary of intent.

**Ordered gate on the merged tree** (`CLAUDE.md` order: UI first, then the sidecar, then cargo),
before the version bump and docs:

````text
ui_npm_ci      EXIT=0 (3s)
ui_npm_lint    EXIT=0 (3s)          # Rules-of-Hooks at error level
ui_npm_test    EXIT=0 (2s)          # 160 tests, 160 pass, 0 fail
ui_npm_build   EXIT=0 (6s)
stage_mcp      EXIT=0 (26s)
cargo_fmt      EXIT=0 (1s)
cargo_clippy   EXIT=0 (76s)         # Finished `dev` profile, zero warnings
cargo_build    EXIT=0 (87s)         # Finished `dev` profile
cargo_test     EXIT=0 (196s)        # 86 result lines: 1616 passed, 0 failed, 16 ignored
bindings_diff  EXIT=0               # `git diff --exit-code -- ui/src/bindings` prints nothing
````

The same UI half was re-run after the version bump: `npm ci` 0, `lint` 0, `test` 160/0, `build` 0,
`stage-mcp` 0, and the staging step now reports `Compiling mcp v0.4.5`, which is the bump taking
effect in a built artifact.

**Static release checks.**

| Check | Result |
|---|---|
| No v1 token names in `ui/src` | PASS: the 25 names the deleted `tokens.compat.css` mapped (`--bg-base`, `--bg-overlay`, `--bg-surface`, `--danger`, `--ease-ui`, `--font-body`, `--font-display`, `--glow-scan`, `--ink`, `--ink-faint`, `--ink-muted`, `--line`, `--motion-*`, `--ok`, `--radius-chip`, `--radius-panel`, `--radius-ribbon`, `--scrim`, `--text-caption`, `--text-subtitle`, `--text-title`, `--tracking-eyebrow`, `--warn`) match nowhere, and `ui/src/styles/tokens.compat.css` no longer exists. The kickoff's wider regex cannot be empty because it also names three live v2 tokens (`--accent`, `--accent-wash`, `--font-mono`); both raw outputs and the reason are in `07` QA D3. |
| No `kitchen` in the production bundle | PASS: `grep -rni kitchen ui/dist` prints nothing; `ui/dist` is `assets`, `index.html`, `overlay.html`. |
| No remote fonts | PASS: `grep -rniE 'url\(http|fonts\.googleapis|fonts\.gstatic' ui/src/styles` prints nothing; the four font files under `ui/src/styles/fonts` are bundled. |

**P4 harness on the committed fixture and seed** (`crates/evalfixture gen`, then
`serve --embed` with the real answer sidecar, then `harness eval --pairs 300 --seed
6696384177732976644`). Generation reproduced the recorded content digest exactly:
`digest=69dac6e6ae417c0a6bc25590948a4dde8a6e1d500f99bb177f80a6bc794b8ab9`, `frames=1260`,
`sessions=218`, `embeddings=1260`; the answer sidecar loaded
(`Ministral-3-3B-Reasoning-2512-Q4_K_M.gguf`) and the run finished `300/300` turns with zero
failures in 345 s, using the already-installed models through junctions (no download: the models
directory is byte-identical at 5,810,899,283 bytes and no file in it has today's mtime).

| Metric | Target | Measured (overall) | Verdict |
|---|---|---|---|
| retrieval recall@10 (`GET /v1/search`) | >= 41.0% (>= 123/300) | 37.0% (111/300) | FAIL, exactly the frozen baseline |
| known-fact answer rate (`/v1/ask`) | >= 71.3% (>= 214/300) | 65.7% (197/300) | FAIL, 1.0 pt over the 64.7% baseline |
| citation validity | 100% (2400/2400) | 71.4% (1713/2400) | FAIL, 687 invalid ids, all "outside the pair window" |
| entity recovery | >= 80.0% | 90.3% (159/176) | PASS |
| latency p95 | <= 3485 ms | 2306 ms | PASS |

The three failures are explained rather than dismissed: this harness cannot exercise Ask v2 at all.
`crates/evalfixture/src/serve.rs` states it in its own header ("Ask v2 turns are not served here: the
host keeps `ApiHost::ask_turn`'s `Ok(None)`") and delegates Ask to
`kernel::ask::ask_context`, the frame-level path kept for exactly this fixture. So the two Ask rows
measure the legacy pipeline, and the `/v1/search` row is the one `06` #46 records that P6 does not
bind to. The gap, its owner and what would close it are `07` QA F1. No code, test or threshold was
changed to move these numbers.

