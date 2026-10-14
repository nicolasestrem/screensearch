# 08 — AI Changelog

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
> Live file holds only the current arc.

---

## 2026-09-09 — Remove confirmed unused code and dependency declarations

- **Change:** after explicit approval, deleted `ScreenScaffold.tsx`, `IconSearch`,
  `startOfLocalDay`, and `useSetMarkNote`; removed unused workspace `thiserror` and
  OCR's direct `tracing` declaration. Cargo removed only OCR's `tracing` lockfile
  edge. Updated `CHANGELOG.md` and the existing `05`–`07` records.
- **Why:** reference analysis confirmed the UI items have no consumers; dependency
  searches confirmed unused declarations. Prefer deletion over new abstractions;
  retain the `03 §2` module boundary and `UI_REFERENCE §6` typed IPC contract.
- **Verification:** unchanged and post-edit workspace results match by test identity
  and outcome (759 pass, 15 ignored); UI lint/build and its two tests pass. The real
  WinRT OCR test was separately run before/after and passed. Full commands, raw
  result excerpts, parity explanation and local evidence paths are in `05` Pass 1.
  The initial strict UI-byte comparison failed because Tailwind dropped the dead
  scaffold's sole-use utility; the subsequent content comparison reports:

```text
PASS: 32 baseline files and 32 rebuilt files map one-to-one
PASS: 31 files identical after asset-filename reference normalization
PASS: only CSS rule removed: .max-w-5xl{max-width:64rem}
```

- **Not included:** dependency upgrades, new tests/stubs, active hook or layout
  refactors, architecture/schema changes, generated-binding edits, commits, PR,
  remote CI, release or merge. Local verification is not a claim of those actions.

## 2026-09-09 — Package the reviewed cleanup for local testing

- **Change:** produced the optimized x64 application and unsigned NSIS installer
  from the reviewed cleanup branch. Documented the reproducible command, temporary
  signing override, installer size/SHA-256 and verification in `05` Pass 2; updated
  the root changelog. No production source or checked-in build configuration changed.
- **Why:** maintainer requested a real build to test, then explicitly requested
  documentation, commit, push and an open PR. No merge or release authorization.
- **Verification:** root `npm run build -- --ci --no-sign --config` with the external
  local-build JSON configuration exited 0. Verbatim build/artifact excerpts:

```text
    Finished `release` profile [optimized] target(s) in 4m 16s
PASS: installer PE and x64 application artifact verified
PASS: tracked repository diff unchanged by packaging
```

- **Limits:** no installation or application launch. The installer retains the
  existing application identity/version; it is unsigned and is not a published
  updater artifact. Existing local regression and static-review evidence remains
  separate from any subsequent remote CI or PR-review result.


## 2026-10-14 — Remediate PR #112 and guard the Quinn advisory

- Kept quinn-proto 0.11.16 to fix remote memory exhaustion
  (GHSA-4w2j-m93h-cj5j; first patched version 0.11.15).
- Addressed both Claude and Codex's MSRV finding: documented MSRV 1.95 (required
  by the locked libsqlite3-sys build script), pinned local/CI/release toolchains to 1.95.0,
  and made Cargo CI and MCP staging use the committed lockfile.
- Added an advisory-specific gate to CI and release to prevent vulnerable Quinn
  versions from returning, without disabling Dependabot alerts or lint gates.
- Synced the PR with main through a regular merge; no force-push or history rewrite.
- Initial guard verification accepted 0.11.16 and rejected main's actual 0.11.14
  lockfile. Full verification and alert closure are recorded in Build Review.
