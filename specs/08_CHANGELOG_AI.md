# 08 - AI Changelog

The 0.4.5 arc shipped: P1 through P10, landed as PRs #150-#166. Its full record, including every
phase's build narrative, gate output and review-fix pass, is archived at
`specs/archive/08_CHANGELOG_AI.v0.4.5.md`; that archive is frozen and this live file carries only
the current arc.

## Current arc

### 2026-11-07: v0.4.5 release preparation and QA (branch `chore/0.4.5-release`, from `main` `440d2e9`)

Phase: the maintainer's 0.4.5 release kickoff. The kickoff replaces stop-at-ambiguity with "decide,
record it in `07`, continue", so every decision this pass took without asking is recorded as
`07` QA D1-D6 with the evidence that forced it. No tag is created and no release is published by
this pass; the installer is validated through a `workflow_dispatch` dry run and an installed copy.

**Version.** `0.4.2` to `0.4.5` in all six declaration sites: workspace `Cargo.toml` (crates inherit
`version.workspace = true`), root `package.json`, `ui/package.json`, `src-tauri/tauri.conf.json`
(the four fields the release workflow's tag-vs-version guard compares), plus both npm lock roots and
the 19 local packages in `Cargo.lock`. Third-party packages that happen to sit at `0.4.2`
(`constant_time_eq`, `wasm-streams`, `windows-strings`) were not touched, and `npm ci` on both locks
still passes.

**Docs.** `CHANGELOG.md`'s `[Unreleased]` became `[0.4.5] - 2026-11-07`. `01 §8` now states the
release, schema **15** and the single-user audience. `docs/release-notes/v0.4.5.md` is the new
release body (summary, `<!-- SCREENSHOTS -->` placeholder for the maintainer's images, What's new,
Breaking with the exact P6/P8 wire changes, Agents). `README.md` gained the 0.4.5 feature section
and lost the claims that had gone stale (schema 13, forward-only migrations, six screens, thirteen
tools). The archive sweep ran per `04 §7`: `specs/archive/{05,06,07,08}_*.v0.4.5.md` are
byte-identical copies (SHA-256 recorded in `07` QA D4), and the live logs keep only what is still
open.

**Gate (on the merged tree, before the docs and after the version bump; raw logs kept outside the
repository and summarised here).** UI first, in order: `npm ci` 0, `npm run lint` 0,
`npm test` **160 passed / 0 failed**, `npm run build` 0; `node scripts/stage-mcp.mjs` 0;
`cargo fmt --all -- --check` 0; `cargo clippy --workspace --all-targets -- -D warnings` 0;
`cargo build --workspace` 0; `cargo test --workspace` **86 result lines, 1616 passed, 0 failed,
16 ignored**; `git diff --exit-code -- ui/src/bindings` prints nothing.

**Static release checks.** No `ui/src` file mentions any of the 25 names the deleted
`tokens.compat.css` mapped (`--bg-base`, `--ink-muted`, `--glow-scan`, ...), and no remote font is
referenced from `ui/src/styles`. The kickoff's literal token grep cannot be empty because three of
its names (`--accent`, `--accent-wash`, `--font-mono`) are v2 semantic tokens defined in
`ui/src/styles/tokens.css`; `07` QA D3 records both raw results and why the intent-preserving check
is the one that means anything. The dev-only `/kitchen` route is absent from the production
`ui/dist` bundle.

**P4 harness.** Regenerated the fixture at the committed seed (content digest reproduced exactly),
served it with the real embedder and answer sidecar through junctions onto the installed models (no
download), and ran the 300-pair evaluation: `300/300` turns, zero failures, entity recovery **90.3%**
(PASS against 80.0%) and p95 **2306 ms** (PASS against 3485 ms), while retrieval recall@10 37.0%,
known-fact 65.7% and citation validity 71.4% land on the frozen baseline rather than their targets.
The fixture cannot exercise Ask v2 (`crates/evalfixture/src/serve.rs` serves the legacy
`ask_context`), so those rows measure the pre-P6 pipeline; `07` QA F1 records the finding with its
raw numbers and leaves its resolution (serving Ask v2 from the fixture, or the live-database run) to
the maintainer. Nothing was re-baselined to make a number move.
