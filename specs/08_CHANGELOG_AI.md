# 08 — AI Changelog

## 2026-11-04: 0.4.5 P3 synced onto main, audited and verified

- **Change:** the overnight P3 branch (`feat/0.4.5-p3-baseline-schema`) was rebased from the P1
  tip onto `origin/main` (P1 #150 + P2 #151/#152), so the baseline v14 schema and the settings v2
  model now build on the merged four-theme design system. Work landed today: `ui.appearance` as the
  26th UI key (typed `UiAppearance` enum with the six kebab tokens, `Settings` field with
  `serde(default)`, `pine-light` default, kernel load/save, rebuild carry at 26, regenerated
  bindings); a baseline DDL parity test that pins every `03 §4` declaration and the documented
  `06` #30 deltas against `BASELINE_SQL`; the rebuild fixture now seeds `ui.appearance` and asserts
  the raw value survives; the last stale retired-key reference (dev-only `/kitchen` gallery) was
  removed under a one-time maintainer grant; and the kickoff's Maintainer run was rewritten as a
  nine-step copy-pasteable PowerShell procedure with the non-destructive swap and one-command
  rollback.
- **Why:** P3 must ship one baseline schema and one honest settings model for a single user
  (0.4.5 D2, D6, D1), aligned with the maintainer's P2 pick, with the real-database rebuild left to
  the maintainer.
- **Commits:** `31f7c66` (ui.appearance), `47b1f1b` (parity test), `1407e5b` (rebuild fixture),
  `bbd8976` (kitchen gallery), `c039d5e` (maintainer run + contract text), then `3d9d89a`,
  `b53db47`, `8aca7a5`, `244638a`, `9d3a9ee` and `8833c41` from an adversarial audit (parity-test
  strictness, rebuild fidelity failures, settings doc truth, P3-authored em dash removal, restored
  coverage, guarded maintainer procedure), plus the records commits. Branch pushed and
  [PR #153](https://github.com/nicolasestrem/screensearch/pull/153) opened against `main` on the
  maintainer's explicit 2026-11-04 instruction (0.4.5 D18 validation given for this PR); not merged.
- **Review fix (`src-tauri/src/memory.rs`):** PR #153 review found that a queued or blocked report
  resumed after a restart with an edited `advanced.toml` took the new report depth and sidecar
  parameters, because P3 moved those keys out of the typed `Settings` that the durable
  `RunSnapshot` captured. `RunSnapshot` now captures the moved report and sidecar keys at enqueue,
  recovers the legacy typed fields from pre-settings-v2 snapshots, and only then falls back to the
  startup configuration, preserving the Retry/restart versus Regenerate distinction in `03 §8c`.
  Two tests cover the capture/resolve round-trip and the legacy recovery; the full gate re-ran
  green (1201 passed, 0 failed).
- **Verified:** all nine verification commands in `04` section 4 exit 0 on the final tree
  (`npm ci`, `lint`, `build`, `stage-mcp`, `fmt`, `clippy`, `cargo build`, `cargo test --workspace`
  1195 passed / 0 failed, bindings diff clean); raw output and the per-item acceptance audit are in
  `05` under 2026-11-04. No real database, model file, `.env` or secret was touched. Still open:
  the maintainer's real-DB rebuild, per-table counts, 20-frame Moment spot check, v14 boot and the
  Settings > Integrations in-app check.

## 2026-11-04: 0.4.5 P2 design system v2

- **Change:** the v2 foundation lands on `feat/0.4.5-p2-design-system`, cut from `origin/main`
  (P1 contract). Specs first: `UI_REFERENCE` §1/§2/§5 record the Logbook direction, its scales and
  its primitive inventory; `03 §8` gains `ui.appearance` and the closed UI set rises 25 -> 26;
  `07` carries the P3/P7 follow-ups. Code: bundled local variable fonts (Source Sans 3, Cascadia
  Mono, both OFL), three-layer `tokens.css` with four themes (`pine-light` default) and a density
  token, `tokens.compat.css` shim covering 46 v1 names (25 aliases, 21 kept their name), rewritten `globals.css`, Tailwind mapped onto
  the same properties, 30 primitives rebuilt with the v1 names and props intact, a 55-glyph local
  icon set, a dev-only `/kitchen` gallery outside the AppShell, and two enforcement tests (124 AA
  pairs x 4 themes; a 7-rule token fence over the primitives).
- **Commits:** `cf56222`, `8136179`, `5c9c679`, `8e1935b` (docs/specs), `75c77ba` (implementation),
  `d113a0f` (review round), then the documentation commit. Branch `feat/0.4.5-p2-design-system`, PR
  opened, not merged.
- **Verified:** lint, 90 UI tests, build, clippy, cargo build and the workspace tests all green;
  the six P2 greps at 0 (0 unresolved tokens), bindings untouched; detect 1 finding per variant,
  attributable to v1 progress bars outside this phase's ownership; AA 0 failures in all four
  themes. Raw output in `05`.

## 2026-11-03: 0.4.5 P4 automatic intelligence evaluation

- **Change:** `crates/harness` gains an `eval` subcommand and `src/eval/` (facts, generators,
  reference extractor, seeded PRNG, API client, metrics, report). It generates its own question and
  answer pairs from the database (last-seen, involvement, sequence, window recall, seen-again),
  scores evidence recall@10, a separate search recall diagnostic, citation validity, known-fact
  presence, entity recovery and invention, and latency p50/p95, and writes `report.md`,
  `report.json`, `pairs.jsonl` and `runs.jsonl`.
- **Why:** the arc must measure Ask and the work graph before changing them, with zero maintainer
  input (0.4.5 D17). The numbers freeze as the P5/P6 thresholds at Gate A.
- **Verification:** see `05` (2026-11-03) for the raw lists. `fmt`, `clippy -p harness`, and 168
  harness tests green, including determinism (same DB and seed produce identical pairs), read-only
  proof (query_only, INSERT and CREATE fail, file byte identical with a writable negative control),
  and a fixture end-to-end report. No real database was touched.

## 2026-11-03: 0.4.5 P3 baseline schema and settings v2

- **Change:** `crates/store` loses the v1 to v13 migration chain for one baseline script at
  `schema_version = 14`, with a refusal that names `ssv2c-rebuild`. New dev-only `crates/rebuild`
  carries history from a v13 file into a fresh v14 file read-only, copying by column intersection,
  repopulating FTS and `vec0`, filtering settings and refusing to rename anything. Settings v2 splits
  the typed `Settings` into the 25 UI keys plus `advanced.toml` (43 knobs, eight sections), deletes
  the nine retired keys, rewrites unknown keys away once, and keeps the reserved identity and
  allocator rows. `get_advanced_config` gives the UI a read-only projection, and the regenerated
  bindings replace the old wide `Settings.ts`.
- **Why:** the arc needs one clean schema and one honest settings model for a single user (0.4.5 D2,
  D6, D1). A migration chain nobody validates and a settings surface of 78 keys were both tax with no
  user.
- **Verification:** see `05` (2026-11-03) for the full suite and the parity proof. 1233 tests pass
  workspace-wide; the baseline was compared table by table against the as-built v13 dump (41
  objects preserved, 19 added); the rebuild tool is tested only against a synthetic fixture. The
  maintainer's real-database run and the twenty-frame spot check remain open by design.

## 2026-11-03 — 0.4.5 P1 specs contract

- **Change:** specs only. `03 §4` becomes a baseline (full DDL at `schema_version = 14`, no
  migration chain, refuse-and-name-`ssv2c-rebuild` on any other version) with 11 new tables for the
  work graph, session vectors, insights and Ask v2 threads; new `03 §7h`/`§7i`/`§7j` contracts; `§7c`
  routes and `§7f.7` MCP tools; `§8` rewritten as a 78-row disposition table (25 UI / 44 file-only /
  9 deleted; `api.read_token` shares the `api.token` row) with a rewrite-away load rule; `§12` schema failure mode; new `§13d` DoD. `01`, `02
  §5k`, `04`, `07`, `CLAUDE.md` carry the arc pointer and D1/D2/D14/D15/D16/D17/D18;
  `UI_REFERENCE` gets the v2 IA and placeholders for the P2 identity, v1 identity archived to
  `specs/archive/UI_REFERENCE_v1_identity.md`. `docs/0.4.5.md` (the roadmap) enters git.
- **Why:** roadmap P1 — every 0.4.5 decision must be normative in `specs/` before wave A, so no
  phase depends on rereading the roadmap; the single-user decision (D1) removes the migration
  chain and the retired-key tolerance.
- **Verification:** see `05` (2026-11-03) — scope, consistency and cold-read gates; no code changed,
  so no build or test run. Two unlisted `§8` keys resolved by the maintainer (`06` #27).

## 2026-10-20 — Setup visibility

- **Change:** `traits` gains `SetupOperation`/`SetupPhase`/`SetupStep`/`SetupStatus`;
  `download.rs` adds `EngineProgress` + `ensure_binary_with_progress` (streamed body, real
  per-attempt bytes, rate-limited); the kernel owns a revisioned snapshot
  (`set_setup_step`/`setup_status`, URL-query redaction, identical re-reports ignored) and
  mirrors `model_download`; `src-tauri` adds `get_setup_status`, forwards `setup_changed` and
  wires engine/embeddings steps; the UI adds `SetupIndicator`, `SetupDetails` and the
  `newerSetupStatus` revision guard. Specs: `02 §5j`, `03 §7g`/`§7`, `UI_REFERENCE §4`/`§5`,
  `04 §1`, `06` #26, `07` #121/#122.
- **Why:** the v0.4.2 field failure left inference unavailable with nothing in the app to say
  why; users need to see setup progress and a persistent, actionable failure.
- **Verification:** see `05` (2026-10-20) — full CI chain green on tbx, two mutation REDs,
  independent review fixed; interactive native run by Nico on tbx reported
  "All good!" (failed chip, live install stage, Engine/Search model Done; see `05`). PR #146
  round 1 (Codex, 3 × P2): listener-before-snapshot refetch, Settings zero presence once all
  steps are done, terminal engine steps keep the real attempt — all fixed, gate green on tbx.
  Round 2 (Codex, 3 × P2): one rail indicator (legacy download chip removed), a visible failure
  cause line, snapshot re-emit after a lagged broadcast — fixed, gate green on tbx. Final
  auto-triggered findings: vision failure copy names "Start vision tagging"; a lane's failure
  clears when that lane's model reaches Ready.

## 2026-10-19 — llama.cpp bootstrap recovery

- **Change:** `download.rs` now falls back to a pinned, SHA-256-verified Windows Vulkan asset
  when the anonymous GitHub releases lookup fails, gives the binary zip up to three attempts on
  transient statuses/transport errors, and exposes `ensure_binary_from_url_verified`
  plus two `#[ignore]`d real-network checks; `lib.rs` reports the full anyhow chain for an
  acquisition failure. Docs: `ARCHITECTURE §7.3`, `CHANGELOG [Unreleased]`, this record.
- **Why:** field evidence showed an installed 0.4.2 permanently without inference because the
  per-IP anonymous API quota was exhausted (403, remaining 0), and initialization runs once
  per session. The API-independent pin replaces only the resolution step; a working API still
  selects the newest release and no token is involved.
- **Verification:** native tbX Rust 1.95.0 — RED observed before the fix (403 panic), then
  `cargo test -p inference --lib` 110 passed / 2 ignored, clippy `-D warnings` clean for
  `inference` + `screensearch`, and both gated real-network checks downloaded the archive and
  ran `llama-server --help` (verbatim output in `05`).
- **Not done:** no merge, no release, no change to the installed app or its data.

## 2026-10-19 — Record v0.4.2 publication and field test

- **Change:** `05` gained the publication evidence (tag run, assets, updater endpoint
  redirect chain); `07` and `06` moved F3 native acceptance from PENDING to accepted on the
  maintainer's report.
- **Why:** the sweep entries said "no tag claimed" and left F3 acceptance pending; the release is
  now published and Nico reported a green field test. Recorded as reported, not as agent-observed.
- **Verification:** docs only; values copied from the observed release run output (`05`).

## 2026-10-19 — v0.4.2 release-prep archive sweep (branch `release/v0.4.2`)

- **Change:** moved the 0.4.2-arc entries of `05`/`06`/`07`/`08` verbatim into
  `specs/archive/<NN>_<NAME>.v0.4.2.md` (complete pre-sweep live files plus a one-line header)
  and the `CHANGELOG.md` `[Unreleased]` block into `CHANGELOG-ARCHIVE.md` as `[0.4.2]`, adding
  the missing external-agent memory API (#126–#130) and dependency-security (#125) bullets and
  scoping the all-NO_DATA review-fix bullet to fully covered weeks (#141, squashed in `3038a60`).
  Still-open items stay live: F3 native Windows interactive acceptance (pending),
  open weekly decisions and readings (`07`), pending `06` items.
- **Why:** CLAUDE.md "Archive on release"; modelled on the v0.4.1 sweep (`6587e84`). Release
  prep only; no tag or release is claimed here.
- **Verification:** archive bodies diffed byte-identical to the pre-sweep live files (`05`).
- **Review fix (Codex `PRRT_kwDOTAuCtM6t5thb`):** the archived docs-drift follow-up for
  `docs/f3-g5-g6-verification.md` was fixed rather than dropped: its revision pointer names the
  0.4.2 archive and a dated correction records the committed UI test counts (50 at `957b234`,
  55 at v0.4.2; observed output in `05`).

> Append-only record of what the agent changed during the build, **with reasons**. One entry per
> meaningful change set. Empty until P0 begins. (This tracks build work; the design-phase history
> lives in git.)

## <date> — <short title>
- **Change:** what was added/modified.
- **Why:** the reason, tied to a spec section.
- **Verification:** the command run + verbatim result.

---

> Pre-0.2.x (v0.1.0) history → `specs/archive/08_CHANGELOG_AI.v0.1.0.md`.
> Shipped 0.2.x history (0.2.0–0.2.2) → `specs/archive/08_CHANGELOG_AI.v0.2.x.md`.
> Shipped 0.3.0 history (PR1–PR9 + bridge fixes) → `specs/archive/08_CHANGELOG_AI.v0.3.0.md`.
> Shipped 0.3.1 history (post-0.3.0 bridge fixes + PR1–PR4) →
> `specs/archive/08_CHANGELOG_AI.v0.3.1.md`.
> Shipped 0.3.2 history (PR1–PR6) → `specs/archive/08_CHANGELOG_AI.v0.3.2.md`.
> Shipped 0.3.3 history (the browser-freeze hotfix) → `specs/archive/08_CHANGELOG_AI.v0.3.3.md`.
> Shipped 0.4.0 history (the P8 sessions arc: PR1–PR7) →
> `specs/archive/08_CHANGELOG_AI.v0.4.0.md`.
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/08_CHANGELOG_AI.v0.4.1.md`.
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141) →
> `specs/archive/08_CHANGELOG_AI.v0.4.2.md`.
> Live file holds only the current arc.

---
