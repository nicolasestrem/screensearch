# 08 — AI Changelog

## 2026-10-15 — Restore project-context privacy statement

Maintainer explicitly scoped this correction to `specs/01_PROJECT_CONTEXT.md`.
Restored its original privacy statement verbatim from `main` and removed the API/delivery
additions from that paragraph. Feature contracts and runtime behavior are unchanged.
Verification output:

```text
PASS: privacy statement matches main verbatim
PASS: API and delivery additions removed from the privacy paragraph
```

## 2026-10-15 — Separate Telegram permission amendment

Approved report policies now explicitly permit opt-in saved-report digests via Telegram HTTPS.
Parent reviewed network/credential/privacy rules; independent persistence review found no
blockers. `03 §8d` freezes receipt/retry/unknown-delivery semantics before outbound code.
No real credential or destination has been accessed and no send performed. Documentation
`git diff --check` passed; implementation verification follows separately.

## 2026-10-15 — Local report contract before code

Recorded maintainer approval, froze daily windows/DST/activation, durable run/destination
states, file ownership, complete evidence IDs, clone identity reset and retention rules.
Reviewed the contract independently before implementation; Telegram exception follows in
its own amendment. No runtime changes in this commit. `git diff --check`: exit 0.

## 2026-10-15 — Network API implementation and product-wide plan

- Implemented all-IPv4 API with native Windows exclusive port ownership; kept auth on all
  routes, local MCP validation and inference isolation. Corrected fixture destination URLs.
- Added persisted upgrade notice and acknowledgement IPC, generated `ApiStatus`, complete
  notice/error/current-state UI and startup invalidation without broadcasting tokens.
- Updated README/API/MCP/architecture/testing docs and CHANGELOG; preserved historical release
  statements. Added canonical execution plan and removed personal pilot acceptance framing.
- Full verification passed after fixing the real Windows mixed-address bind conflict.
  Raw output excerpts and command/log references are in `05`; live acceptance remains open.

## 2026-10-15 — Separate API contract amendment

- Updated active network rules in AGENTS/CLAUDE and specs 01–04/UI_REFERENCE before code.
  Preserved historical release assertions as history and the MCP/inference loopback boundaries.
- Defined persisted, explicit visible-window acknowledgement for enabled-install upgrades.
- Added the report-contract proposal and product-wide pilot direction; open decisions are
  not represented as implementation or measured acceptance.
- Independent review found the missing notice marker contract; it was added and re-reviewed.
- Verification: `git diff --check` returned exit 0 with empty raw output.

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

- Follow-up Windows verification on Rust 1.95 exposed two existing UIA
  manual_is_multiple_of lint errors. Replaced only those warning-counter
  predicates with Clippy's equivalent u64 method; kept the warnings-as-errors gate.

- Applied the same unsigned is_multiple_of predicate to the store perf fixture
  while auditing the remaining modulo checks under the new compiler.

## 2026-10-15 — Daily memory implementation

Added shared report execution, complete summarized frame provenance, durable schema-12 run and
receipt persistence, fixed-IANA calendar scheduling, installed-only automatic inference, safe
owned Markdown output/index and opt-in Telegram digests. Settings exposes preview, history,
coverage, independent delivery outcomes, retry/regenerate and reviewed/intentional-resend actions.
Documentation: docs/MEMORY.md, architecture, native testing checklist, strategic trust exception,
CHANGELOG and build-loop records. The requested original specs/01 privacy paragraph is preserved.
Full verification/package receipts are recorded in 05; no merge, tag, installation replacement,
private-history acceptance or real Telegram send is claimed.


## 2026-10-15 — configurable bind and input corrections (supersedes earlier Gate A defaults)

The maintainer requested configurable IPv4/IPv6 literal binding, persisted as `api.bind_address`.
Missing values default to `127.0.0.1` for fresh and existing enabled profiles; no automatic
broadening occurs. Address/port edits apply immediately, with validation before mutation.
The earlier hard-coded wildcard and automatic-broadening notice contracts above are historical
build evidence, superseded by this correction. Bearer authentication, exclusive Windows binding,
visible bind errors and MCP/inference loopback restrictions remain.

Schedule input uses `HH:mm` hour/minute precision with no seconds field. New memory settings,
delivery destination snapshots and receipt/recovery paths use forward slashes. Legacy ownership
lookup accepts earlier separators; canonical ownership keys are saved transactionally to avoid
duplicates/conflicts. This is path normalization within the Windows-only app, with no schema
migration or cross-platform expansion. Native atomic file operations remain Windows operations.

Contract and implementation corrected. Current verification is recorded below/in
`05_BUILD_REVIEW.md`; earlier raw results remain evidence of their original revisions.


Current correction checks passed: Node 22 UI install/lint/build/two existing tests; MCP staging;
Rust 1.95 fmt, Clippy with warnings denied, workspace build and 823 tests (15 existing ignored);
generated binding guard and unsigned NSIS package. Native prefix preservation was corrected during
review and covered by real atomic create/replace/read tests. The rebuilt CLI returned version
0.4.0 with exit 0; hash, size, raw excerpts and acceptance boundaries are recorded in `05`.


## 2026-10-15 — live NO_DATA investigation and manual date/range selection

Read-only inspection of the reported running Windows installation found two finished previous-day
runs with zero frames in the requested window, NO_DATA output, successful vault/index receipts
and Telegram skipped with attempt 0. No model or transport request had run. Captures existed for
the following day, so retrying the previous-day window could not include them. Schedule activation
starts with a full subsequent calendar day and fires after that day finishes; selecting a near-future
clock time on the activation day does not bypass that contract. No live configuration, credentials,
notes or run records were changed, and the running app was not stopped.

The maintainer then requested choosing a date or date range. Contract commit `eb5f16d` adds an
optional typed inclusive calendar request, today's frozen in-progress window/PARTIAL framing,
independent explicit-run identities and Reports files, range labels in the shared index/history/
digest, and legacy previous-day/schedule compatibility. Optional backward-compatible JSON fields
require no SQL schema change. Native profile replacement and an actual Telegram send remain
outside this implementation/verification run. Current build/test evidence follows after checks.


Current date-range UI/Rust gates pass: 832 Rust tests, zero failed, 15 existing ignored;
UI install/lint/build/two existing tests, MCP stage, fmt, Clippy and binding guard. Source review
also corrected empty coverage generation after a positive evidence probe: report text must be
nonblank, and all-empty map output cannot become NO_DATA. Prompt-based no-match behavior stays
supported, including normalization of empty/whitespace prompts. Raw evidence and native
acceptance boundaries are in `05_BUILD_REVIEW.md`; packaging follows separately.


Unsigned date-range NSIS packaging passed; release profile completed in 3m 09s and the executable
returned `ScreenSearch 0.4.0` with exit 0. Artifact hash/size/raw output are in `05`. The running
installation, selected vault, settings, credentials and Telegram conversation were not changed.


## 2026-10-15 — README showcase for PR #123

Expanded the README's development-feature overview into a user-facing showcase of scheduled
memory, selected dates/ranges, Obsidian Markdown/index output, opt-in Telegram digests, coverage/
recovery and configurable API binding. Added setup links and clarified the loopback default.
The maintainer plans release tonight and requested this documentation in PR #123; the README
continues to identify the additions as unreleased until publication. CHANGELOG updated.

Documentation-only verification: relative README link/image check and `git diff --check` exited 0.
Raw link-check output:

```text
README local links/images checked: 21
PASS: all relative README link/image targets exist
```

No runtime changes or repeated build/test suite; the current installer verification above remains
unchanged. This documentation update does not merge PR #123, create a tag or publish a release.


## 2026-10-15 — one shared agent instruction source

Compared AGENTS.md and CLAUDE.md: their repository-rule bodies were identical; only their
headings/agent labels differed. At the maintainer's request, AGENTS.md is now a plain sentence
requiring agents to read and follow CLAUDE.md before working. CLAUDE.md remains the full shared
guidance, with its introductory label updated for all coding agents; all existing rules are
preserved. No symlink or Codex global configuration change was made. The checkout has
core.symlinks=false, so a committed symlink would not reliably materialize as a link.

Official OpenAI documentation confirms Codex discovers AGENTS.md at repository scope:
https://developers.openai.com/codex/agent-configuration/agents-md . The sentence is an instruction
to read the target, not a special automatic-import syntax. The reference must be followed by
agents; no universal third-party auto-include behavior is assumed.

Raw local verification:

```text
PASS: existing instruction bodies matched before consolidation
PASS: all existing repository rules preserved in CLAUDE.md
PASS: AGENTS.md points to the existing canonical CLAUDE.md
```

`git diff --check` exited 0 with empty output. A fresh read-only agent began at AGENTS.md,
followed the pointer and identified CLAUDE.md as canonical. It returned the existing mandatory
spec reading order, Windows/path rules and branch/review rules. This verifies the ordinary
instruction-following workflow without claiming an automatic file-import mechanism.
