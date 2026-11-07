# 05 - Build Review

The 0.4.5 arc shipped in v0.4.5. Ten phases, PRs #150-#166: P1 #150 `e27f42c`, P2 #151 `7e73e30`,
P3 #153 `0dfa2f5`, P4 #155 `aceca58`, P5 #157 `722f29e` (plus the review-fix pass #159), P6 #160
`6d906a7`, P7 #161 `5d15301`, `60c60d8`, `8e0883a`, P8 #164 `eb556d1`, P9 #165 `9112087` and
P10 #166 `e459185`. The full build record, every ordered gate, raw log excerpt, acceptance walk,
measured budget and review-fix pass included, is archived at
`specs/archive/05_BUILD_REVIEW.v0.4.5.md`. New entries start below.

## Current arc

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

