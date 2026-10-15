# 05 — Build Review

## 2026-10-15 — Report policy approval and local scheduling contract

The maintainer explicitly approved the proposed report policies with "Sure GO" after the
scope clarification. Local schedules, durable runs, source identity, retry/regenerate, DST,
file ownership, coverage and product-wide validation policies are frozen in `03 §8c` before
implementation. A second agent reviewed persistence/provenance; parent reviewed lifecycle,
calendar and output rules. Telegram outbound permission is a separate following amendment.
Documentation check: `git diff --check` returned no errors (line-ending normalization warnings).

## 2026-10-15 — Network API increment B implementation verification

### Local installer result

The final source was packaged with Node 22 using
`npm run build -- --ci --no-sign --config <temporary-build-config>` (exit 0).
The temporary config contained only `{"bundle":{"createUpdaterArtifacts":false}}`.
Production signing/updater configuration, application identity and version were not changed.
This is a local unsigned 0.4.0 test installer, not an updater release. Raw final output:

```text
    Finished `release` profile [optimized] target(s) in 3m 14s
       Built application at: C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\screensearch.exe
        Info Patching C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\screensearch.exe with bundle type information: nsis
        Info Target: x64
     Running makensis to produce C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\bundle\nsis\ScreenSearch_0.4.0_x64-setup.exe
    Finished 1 bundle at:
        C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\bundle\nsis\ScreenSearch_0.4.0_x64-setup.exe
```

Complete raw package output: `target/verification/package.log`. Runnable workspace tests:
766 passed, 15 ignored across all test binaries; the full raw per-binary results are in
`target/verification/tests.log`. Ignored native/model tests remain unverified.

Installer size: 13,754,211 bytes. SHA-256:
`6B170F04DEA6FC661C89287E823120DA8EDA88F65F32ACE709A1A656ED601BAB`.

### Implementation and verification detail

- Changed only the application API listener to all IPv4; retained MCP destination validation,
  inference loopback, bearer authentication, token rotation and default-off configuration.
- Added persisted enabled-upgrade notice, explicit visible-main-window acknowledgement,
  generated status binding and payload-free startup cache invalidation. Reviewed startup
  failures must not overwrite enabled intent; early UI reads refresh after startup.
- Initial full tests exposed Windows' default ability to bind a wildcard listener beside an
  existing loopback listener on the same port. Fixed the actual server with native
  `SO_EXCLUSIVEADDRUSE` before binding; retained the loopback conflict test and added wildcard
  coverage. This prevents local MCP reaching another service despite a successful API startup.
- Removed private-database input from the ignored fixed-token manual fixture server.
- Independent agents reviewed contract, backend/UI startup fixes and exclusive socket ownership.
- Verification used Node 22.23.3 and pinned Rust 1.95.0. UI lint/build, UI tests, fmt, locked
  workspace clippy/build/test and the regenerated-binding guard exited 0. Commands match CI;
  MCP was staged first. Cargo regenerated lockfile format 4 and added the existing Windows
  dependency to API; no dependency versions changed.
- Raw excerpts from the final `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo build --locked --workspace` and `cargo test --locked --workspace` outputs:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 20.97s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 41.87s
test binds_all_ipv4_interfaces ... ok
test loopback_client_still_connects ... ok
test all_routes_require_bearer ... ok
test local_api::tests::wildcard_bind_failure_keeps_enabled_with_error ... ok
test local_api::tests::bind_failure_keeps_enabled_with_error ... ok
test local_api::tests::disabled_api_has_no_listener ... ok
test local_api::tests::enabled_upgrade_shows_bind_change_notice_once ... ok
```

- Complete local raw logs: `target/verification/{clippy,build,tests,ui-lint,ui-build}.log`.
  Initial failure is retained in `tests-initial.log`. `git diff --exit-code -- ui/src/bindings`
  exited 0 after staging regenerated bindings. The generator's native trailing whitespace
  in `ApiStatus.ts` is retained rather than hand-editing a generated file; source/docs whitespace
  checks excluding generated bindings pass.
- **Acceptance still open:** separate-machine HTTP, native rendered notice/keyboard states,
  real-model Ask disconnect and live inference socket inspection. Automated sockets/fixtures
  are not substituted for these checks. No app installation, private-history report, firewall
  mutation, Telegram send, merge, tag or updater publication was performed.
- **Remaining plan:** C–E requires the specifically unresolved report policies in the canonical
  `docs/memory-api-direction-plan.md` / `docs/memory-report-contract-proposal.md`. Product-wide
  validation replaces the original personal usefulness threshold. This is not full-plan completion.

## 2026-10-15 — Memory/API Gate A

- Maintainer authorized execution of the desktop direction plan and clarified that this is
  a public product, not a personal integration. API contract reviewed independently by a
  second agent before implementation. Existing checkout: `ab6c702`, clean on main.
- API-only amendment preserves MCP/inference loopback, default-off, authentication and
  token lifecycle; defines the enabled-install notice and all-interface HTTP threat model.
- C–E product semantics remain in `docs/memory-report-contract-proposal.md` for resolution.
  No private history, vault or Telegram destination has been accessed.
- Contract verification: `git diff --check`, exit 0, raw stdout/stderr empty.
- Environment: system Node 26 differs from CI; official Node 22.23.3 portable archive
  downloaded to temporary build tooling and SHA-256 checked against Node's published manifest.
  Rustup's inventory command auto-installed missing pinned Rust 1.95.0.

> **Populated during the build**, after each meaningful pass (`04 §7`). Record what actually
> happened — honestly. Empty until P0 begins.

For each build pass, append an entry:

## Pass <n> — <date> — <phase, e.g. P0 Scaffold>
- **Implemented:** what now works (with the verbatim verification output that proves it).
- **Skipped / deferred:** what was intentionally not done, and why.
- **Hallucinated / corrected:** anything the agent assumed that turned out wrong.
- **Broke / regressed:** what stopped working, and the fix.
- **Still risky:** areas that compile/pass but warrant scrutiny.

---

> Pre-0.2.x (v0.1.0) history → `specs/archive/05_BUILD_REVIEW.v0.1.0.md`.
> Shipped 0.2.x history (0.2.0–0.2.2) → `specs/archive/05_BUILD_REVIEW.v0.2.x.md`.
> Shipped 0.3.0 history (the whole arc: PR1–PR9 + post-0.2.2 bridge fixes) →
> `specs/archive/05_BUILD_REVIEW.v0.3.0.md`.
> Shipped 0.3.1 history (the P7.1 triage patch: post-0.3.0 bridge fixes PR #79/#80 + PR1–PR4) →
> `specs/archive/05_BUILD_REVIEW.v0.3.1.md`.
> Shipped 0.3.2 history (the P7.2 product-shell mini-arc: PR1–PR6) →
> `specs/archive/05_BUILD_REVIEW.v0.3.2.md`.
> Shipped 0.3.3 history (the browser-freeze hotfix: UIA skips Chromium/Electron windows) →
> `specs/archive/05_BUILD_REVIEW.v0.3.3.md`.
> Shipped 0.4.0 history (the P8 sessions arc: PR1–PR7, Passes 1–17) →
> `specs/archive/05_BUILD_REVIEW.v0.4.0.md`.
> Live file holds only the current arc.

---

## Pass 1 — 2026-09-09 — Approved deletion-only maintenance

- **Scope:** maintainer approved six deletions on `chore/ai-slop-cleanup`, based on
  `f58c42f7ac8eec6e3220ba86eca17f2c67bfb949`. Removed `ScreenScaffold`, `IconSearch`,
  `startOfLocalDay`, `useSetMarkNote`, the unused workspace `thiserror` declaration,
  and OCR's unused direct `tracing` dependency. Cargo regenerated only OCR's
  `tracing` edge in `Cargo.lock`; no versions changed.
- **Before edits:** TypeScript language-service references and source searches found
  no callers of the four UI symbols. Manifest/source searches found no member using
  the workspace `thiserror` declaration and no OCR source using `tracing`.
- **Preserved:** active `setMarkNote` command/overlay caller, date-range helpers,
  generated IPC bindings, existing tests, OCR implementation, schema and module
  boundaries. No new production code, dependency, abstraction, or test stub.
- **Environment correction:** inherited `NODE_ENV=production` made the first
  `npm ci` omit ESLint/TypeScript. `npm ci --include=dev` installed the locked dev
  toolchain without changing npm configuration or manifests. Windows 11, Rust
  stable MSVC 1.98.1, Node 26.8.1; CI's Node 22 was not exercised locally.
- **Observed verification:** UI `npm run lint && npm test && npm run build`; staged
  MCP sidecar; `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  -- -D warnings`; `cargo build --workspace`; `cargo test --workspace`; binding
  freshness and `git diff --check` all passed. OCR's normal and explicitly ignored
  WinRT test also passed after removing its dependency.

Programmatic comparison of the before/after workspace logs (verbatim output;
compares test identities and outcomes, not just aggregate counts):

```text
Before: {"passed": 759, "failed": 0, "ignored": 15, "measured": 0, "filtered_out": 0}
After:  {"passed": 759, "failed": 0, "ignored": 15, "measured": 0, "filtered_out": 0}
PASS: identical Rust test identities and outcomes before/after; 774 named test executions
```

Post-edit build completion output, in MCP/clippy/build/test order:

```text
    Finished `release` profile [optimized] target(s) in 1.78s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.48s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 35.90s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 34.81s
```

`cargo test -p ocr -- --ignored` (verbatim test-result excerpt):

```text
running 1 test
test tests::winrt_ocr_recognizes_blank_image ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s
```

**UI parity — not byte-identical builds.** The initial strict comparison failed:
Tailwind stopped emitting `.max-w-5xl{max-width:64rem}`, used only by the deleted
scaffold. CSS fingerprint changes propagated through JS/HTML asset references.
A bijective comparison of actual build filenames, normalizing only those asset
references, proved the sole content change is that unused CSS rule. The comparator
also rejected deliberate additional JS/CSS differences (in-memory negative controls;
neither build was modified). Verbatim output:

```text
PASS: 32 baseline files and 32 rebuilt files map one-to-one
PASS: 31 files identical after asset-filename reference normalization
PASS: only CSS rule removed: .max-w-5xl{max-width:64rem}
PASS: negative control rejects additional .js content changes
PASS: negative control rejects additional .css content changes
```

**Evidence:** full local logs in `%LOCALAPPDATA%\Temp\screensearch-cleanup-` with
suffixes `ui-baseline.log`, `rust-baseline.log`, `ocr-live-baseline.log`,
`ui-after.log`, `rust-after.log`, `ocr-after.log`, and `ui-parity.log`.
The `ui-after.log` includes the initial strict-parity assertion failure; it is not
hidden by the later explained comparison. The saved baseline build and comparator
live in `%LOCALAPPDATA%\Temp\screensearch-cleanup-baseline-f58c42f\`.
A final full rerun also exited 0; complete output is in
`%LOCALAPPDATA%\Temp\screensearch-cleanup-final-verification.log`. Its workspace
test identities and outcomes still match the unchanged baseline (comparison output
in `screensearch-cleanup-final-comparison.log` in the same directory).

**Still risky / deferred:** only two UI tests exist; this is not a full interactive
UI, GPU/model, capture, or installer audit. The workspace still ignores 15 tests;
OCR's one ignored test was additionally exercised explicitly. Existing npm audit
findings are tracked in `07`; no dependency upgrade or broader refactor was attempted.
**Independent static review:** Hermes reviewer `deleg_0aedecd1` (`gpt-6-astra`)
returned a complete JSON verdict with `passed: true`, no security concerns and no
logic errors. The parent parsed the durable verdict and verified that the current
seven-file production/dependency diff still exactly matches the reviewed snapshot
(zero additions, 53 deletions). The verdict is stored as `review-verdict.json` in
the evidence directory above. This static review did not run or stand in for the
parent's separate build/test checks or interactive Windows smoke testing. At the
end of this verification pass, no PR, remote CI run, release or merge was performed.

## Pass 2 — 2026-09-09 — Local release installer

- **Requested:** build the reviewed cleanup for hands-on testing, without installing,
  launching, publishing or changing application behavior.
- **Build:** installed the root's locked Tauri CLI with `npm ci --include=dev`.
  The existing root build script staged the real MCP sidecar, rebuilt/typechecked
  the UI, compiled the optimized Windows application, and produced the NSIS bundle.
- **Local-only signing override:** a temporary file outside the repository,
  `%LOCALAPPDATA%\Temp\screensearch-local-build.json`, contained:

```json
{"bundle":{"createUpdaterArtifacts":false}}
```

The command below ran from the repository root in Git Bash (exit 0):

```bash
npm run build -- --ci --no-sign --config "$LOCALAPPDATA/Temp/screensearch-local-build.json"
```

The override disables updater-artifact generation for this invocation only;
`--no-sign` explicitly skips signing. The checked-in Tauri configuration, updater
public key/endpoint, app identity and version remain unchanged. This is a local
unsigned installer, not a signed auto-update release or an isolated test profile.

Verbatim build-output excerpts:

```text
    Finished `release` profile [optimized] target(s) in 4m 16s
       Built application at: C:\Users\nicol\screensearch-v2c\target\release\screensearch.exe
```

```text
    Finished 1 bundle at:
        C:\Users\nicol\screensearch-v2c\target\release\bundle\nsis\ScreenSearch_0.4.0_x64-setup.exe
```

- **Artifact:** `target/release/bundle/nsis/ScreenSearch_0.4.0_x64-setup.exe`;
  13,695,933 bytes (13.06 MiB).
- **SHA-256:** `cce7bebcb7efcfedde0156dd1728870f5d4fb1cf9a895f64400707526e85b281`.
- **Independent artifact checks:** parsed the installer's PE header and verified
  the application is an x64 PE. Compared the full tracked diff with the pre-package
  snapshot; packaging did not mutate tracked files. Verbatim output:

```text
PASS: installer PE and x64 application artifact verified
PASS: tracked repository diff unchanged by packaging
```

- **Evidence:** `%LOCALAPPDATA%\Temp\screensearch-cleanup-release-build.log` retains
  complete stdout/stderr. Installer and build outputs remain git-ignored; no binary
  or signing material is committed.
- **Limits:** installer execution and interactive application QA have not been
  performed. Windows may show a SmartScreen warning. Existing application identity
  means this installer is not a side-by-side sandbox. Build success is not a claim
  of installation, runtime smoke-test, remote CI, release or merge success.
- **Delivery authorization:** after receiving the installer, the maintainer
  requested documentation, commit, branch push and an open PR. This authorizes
  that delivery only; no merge or release is authorized.



## 2026-10-14 — PR #112: Quinn security update and review remediation

- Retained `quinn-proto` 0.11.16; GHSA-4w2j-m93h-cj5j affects versions
  below 0.11.15. GitHub Dependabot alert #4 was open before this change.
- Read both Codex's inline review and Claude's inline/conversation reviews.
  Confirmed from crates.io metadata that 0.11.15 and 0.11.16 require Rust 1.85.
  Main already locks image 0.25.10, time 0.3.49, serde_with 3.21.0 and darling
  0.23.0 requiring Rust 1.88, so the old 1.82 declaration was already inaccurate.
- Initially reconciled Cargo, README, AGENTS, CLAUDE and the production spec at 1.88;
  pinned local rustup, CI and release jobs to 1.88.0 so CI verifies that minimum.
  The prior failed Windows run used moving stable 1.99.0 and hit 57 Clippy
  double-must-use errors in async-trait-generated code.
- Added an advisory-specific lockfile gate to CI and release, including optional
  dependency entries; retained every existing verification step. Cargo CI and
  MCP staging now use --locked. Synced the PR with reviewed main via a merge.
- Local verification uses Linux; full Windows verification remains the CI gate.
  Verbatim initial verification:

```text
rustc 1.88.0 (6b00bc388 2025-06-23)
PASS: GHSA-4w2j-m93h-cj5j — quinn-proto 0.11.16
PASS: security guard rejects the actual vulnerable main lockfile (quinn-proto 0.11.14)
```

`cargo metadata --locked --no-deps`, `cargo fmt --all -- --check` and
`git diff --check` exited 0 with no diagnostics. Windows CI, updated reviews,
merge and GitHub's alert rescan must be observed before claiming completion.

Local UI install/lint/build exited 0 (Node 24.19.0 rather than CI's Node 22).
Verbatim production-build excerpt:

```text
vite v6.4.3 building for production...
✓ 446 modules transformed.
✓ built in 3.07s
```

`cargo clippy --locked -p traits --all-targets -- -D warnings` passed on the
pinned minimum, confirming that the generated-code failure is absent on 1.88.0:

```text
    Checking traits v0.4.0 (/workspace/screensearch-v2c/crates/traits)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 35.03s
```

### Full Windows verification exposed an undeclared upstream MSRV

The pinned-1.88 Windows run failed in libsqlite3-sys 0.38.1's build script,
which omits rust-version but uses the std cfg_select! macro stabilized in 1.95.
Crate metadata alone did not establish the full workspace minimum. Verbatim CI:

```text
2026-10-14T21:21:01.6806612Z error: cannot find macro `cfg_select` in this scope
```

Corrected the final Cargo/local/CI/release/documentation contract to Rust 1.95.0.
The Quinn patch and all verification/security gates are retained. The earlier
1.88 trait and MCP checks remain accurate partial results, not full-workspace
verification. Source: https://doc.rust-lang.org/std/macro.cfg_select.html.

The generated-trait Clippy check also passes on the corrected minimum:

```text
    Checking traits v0.4.0 (/workspace/screensearch-v2c/crates/traits)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.73s
```

### Rust 1.95 Windows lint remediation

The full Windows check passed SQLite compilation and then flagged two UIA warning
counters with the newly active manual_is_multiple_of lint. Applied Clippy's
exact equivalent u64 is_multiple_of(64) expressions, preserving fetch_add order,
short-circuiting, and warning frequency. No lint suppression or behavior change.

```text
2026-10-14T21:30:55.6090700Z error: manual implementation of `.is_multiple_of()`
2026-10-14T21:30:55.6449686Z error: manual implementation of `.is_multiple_of()`
```

Applied the same equivalent predicate to the store perf fixture's unsigned
enumeration index so the all-targets Clippy gate cannot hit the same lint next.
The signed-integer remainder expressions elsewhere remain valid and unchanged.

## 2026-10-15 — C–E implementation verification

Three agents implemented/reviewed persistence and provenance, runtime/service integration, and
UI/file/Telegram adapters. Parent integrated the pure calendar helper, documentation and full
verification. The scope is configurable product behavior for supported Windows installations;
there are no personal paths, recipient defaults or usage scoring. Contracts preceded code in
4e0df3a and 217f9ef. Schema changes are one forward-only 11 → 12 migration.

Initial `cargo check --locked -p screensearch --all-targets` passed with one unused-frontmatter
parser warning; index recovery integration will consume that parser before the warnings-as-errors
gate. UI install/lint/build/tests passed on Node 22.23.3. npm install reports 10 development-tree
advisories (3 moderate, 7 high); production-only audit output is `found 0 vulnerabilities`.
No dependency remediation outside the approved feature scope was applied.

Reviews corrected swallowed evidence-read errors, incomplete delivery-intent recovery,
write-only credential handling, reparse-point recovery reads, immutable saved provenance,
retry versus regenerate behavior and uncertain delivery suppression. Native filesystem tests
use actual temporary files; transport response fixtures do not certify live Telegram delivery.

Final full-suite and package output is recorded below. Raw local logs use
`target/verification/memory-*.log`; no private history or credentials are written to them.

### Native writer finding from the full suite

The first C–E full suite failed a real Windows parent-directory rename check. The initial
FILE_READ_ATTRIBUTES-only handle did not prevent `std::fs::rename` despite omitting share-delete.
The invariant was retained; the native desired-access mask is being corrected and re-tested.
Initial raw output is retained in `target/verification/memory-tests-initial.log`:

```text
---- memory_files::tests::protected_parent_cannot_be_swapped_during_write stdout ----
assertion failed: fs::rename(&parent, dir.path().join("displaced")).is_err()
test result: FAILED. 42 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
```

The native writer correction uses FILE_LIST_DIRECTORY together with FILE_READ_ATTRIBUTES so
Windows enforces sharing denial on the held directory handles. The original rename assertion
is unchanged. Scoped verification:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 28.12s
running 6 tests
test memory_files::tests::protected_parent_cannot_be_swapped_during_write ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 0.01s
```

Blocking recovery/index reads also run outside the async worker; a stalled OS file call cannot
prevent abort/join of that worker. This preserves the documented distinction between bounded
async shutdown and non-cancellable operating-system file I/O.

### Final automated verification

Windows; Rust 1.95.0; official portable Node 22.23.3. All commands exited 0:
UI `npm ci`, `npm run lint`, `npm run build`, `npm test`; MCP staging; `cargo fmt --all -- --check`;
`cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo build --locked --workspace`;
`cargo test --locked --workspace`; generated binding diff guard; Quinn security guard.
Full Rust result totals: 817 passed, zero failed, 15 existing hardware/manual tests ignored.
The UI suite contains two existing tests; it is not native MemoryPanel interaction acceptance.

Raw output excerpts (kernel, application, store and traits respectively):

```text
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.21s
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
test result: ok. 93 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
```

Clippy and workspace build final lines:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.78s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.26s
PASS: generated bindings unchanged after cargo test
PASS: GHSA-4w2j-m93h-cj5j — quinn-proto 0.11.16
```

Full raw output is retained in `target/verification/memory-tests.log`, `memory-clippy.log`,
`memory-build.log`, `memory-ui-lint.log`, `memory-ui-build.log`, `memory-ui-test.log`,
`memory-stage.log` and `memory-package.log`. The initial failing native-writer run remains
`memory-tests-initial.log`; the invariant was corrected rather than removed.

### Local unsigned installer

`npm run build -- --ci --no-sign --config <temporary-config>` exited 0. The temporary config
only disables updater-artifact creation; checked-in signing/updater configuration and version
are unchanged. Raw final output:

```text
    Finished `release` profile [optimized] target(s) in 3m 10s
       Built application at: C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\screensearch.exe
        Info Patching C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\screensearch.exe with bundle type information: nsis
        Info Target: x64
     Running makensis to produce C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\bundle\nsis\ScreenSearch_0.4.0_x64-setup.exe
    Finished 1 bundle at:
        C:\Users\nicol\Documents\GitHub\screensearch-v2c\target\release\bundle\nsis\ScreenSearch_0.4.0_x64-setup.exe
```

Preserved copy: `target/verification/artifacts/ScreenSearch_daily-memory_0.4.0_x64-setup.exe`.
Size: 14,257,182 bytes. SHA256:
`8C632174FC3D25A6C43E0D53F91E738833FF22F5E25EEEEACA29CF29A60E536A`.
The older API-only installer remains separately preserved as
`target/verification/artifacts/ScreenSearch_network-api_0.4.0_x64-setup.exe`.
The packaged application version probe exits before Tauri/profile initialization:

```text
ScreenSearch 0.4.0
Version probe exit: 0
```

This proves a local installer build and executable version response, not interactive GUI,
model-quality, Obsidian or Telegram acceptance. The ordinary installation was not replaced;
no real Telegram send, merge, release tag or updater publication was performed.


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


### Current correction verification (Rust 1.95.0 / Node 22.23.3)

Commands observed exit 0: UI `npm ci`, `npm run lint`, `npm run build`, `npm test`;
MCP staging; `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`;
`cargo build --locked --workspace`; `cargo test --locked --workspace`; generated binding guard.
The final workspace log totals 823 passed, zero failed, 15 existing hardware/manual ignored.
The UI's two existing tests passed; they are not interactive Settings tests.
Production-only npm audit reports zero vulnerabilities; install still reports ten development-tree
advisories (three moderate, seven high), with no dependency version changes in this correction.

Raw observed output excerpts:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.03s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.69s
test explicit_ipv4_and_ipv6_listeners_require_authentication ... ok
test result: ok. 26 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.26s
test memory_files::tests::displayed_verbatim_paths_preserve_native_atomic_operations ... ok
test local_api::tests::invalid_saved_address_can_be_disabled_without_broadening ... ok
test local_api::tests::existing_enabled_install_stays_loopback_and_explicit_address_survives_restart ... ok
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test memory::tests::normalized_paths_preserve_existing_file_ownership_without_duplicate_entries ... ok
test result: ok. 53 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s
found 0 vulnerabilities
```

The original path guards and edited-file preservation tests remain unchanged and passing.
A separate native test also passed all eight file-writer tests, including forward-slash ordinary
and verbatim atomic create/replace/read. Runtime/display paths normalize separators while native
Windows prefix syntax is restored at filesystem boundaries. Legacy ownership aliases are updated
transactionally without losing the prior hash or producing duplicate index entries.

Logs: `target/verification/correction-{ui-install,ui-lint,ui-build,ui-test,stage,fmt,clippy,build,tests,native-path,production-audit}.log`.
Native UI/model quality, Obsidian inspection, separate-machine API, live Telegram send and the
consented multi-configuration pilot remain open. This correction does not certify those gates.


### Corrected unsigned Windows installer

`npm run build -- --ci --no-sign --config <local no-updater-artifacts config>` exited 0.
Raw packaging/version output excerpts:

```text
    Finished `release` profile [optimized] target(s) in 3m 23s
    Finished 1 bundle at:
ScreenSearch 0.4.0
```

Preserved artifact: `target/verification/artifacts/ScreenSearch_configurable-api_memory_0.4.0_x64-setup.exe`.
Size: 14,252,824 bytes. SHA-256:
`4950FEECC39EA80D4A6BB826FB4F480C22AA859A48DED6C48FE0DD922CA152FC`.
Canonical bundle output remains `target/release/bundle/nsis/ScreenSearch_0.4.0_x64-setup.exe`.
Full raw output: `target/verification/correction-package.log`; version stdout/stderr:
`correction-version.log` / `correction-version-error.log` in the same directory.
The hidden executable `--version` command exited 0 before application/profile startup;
this is a CLI smoke check, not interactive GUI acceptance. The unsigned 0.4.0 test installer
supersedes earlier test artifacts for the current behavior; no installed profile was replaced.


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


### Date-range implementation verification

Observed exit 0: Node 22 `npm ci`, UI lint/build/two existing tests; MCP staging; Rust 1.95
fmt, workspace Clippy with `-D warnings`, workspace build/test and generated binding guard.
Full Rust suite: 832 passed, zero failed, 15 existing hardware/manual tests ignored. No dependency
version changes; npm install reports the existing ten development-tree advisories. Automated
UI tests do not establish native interaction or actual Telegram acceptance.

Raw current output excerpts:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.57s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.63s
test memory_schedule::tests::selected_dates_are_inclusive_dst_aware_and_today_is_capped ... ok
test memory_schedule::tests::selected_dates_reject_invalid_future_reversed_and_skipped_days ... ok
test report_service::tests::blank_model_output_is_failure_for_single_pass_and_map_reduce ... ok
test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.21s
test memory::tests::current_day_real_evidence_is_partial_but_empty_is_still_no_data ... ok
test memory_telegram::tests::digest_labels_requested_dates_without_sending ... ok
test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test memory::tests::explicit_ranges_are_independent_and_immutable_with_legacy_compatibility ... ok
test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s
```

Native filesystem tests create real temporary Daily and Reports notes, preserve independent
same-date files, reuse the base UUID filename for regeneration, and keep valid index links after
run retention. They do not touch the reported Obsidian folder. Calendar tests cover inclusive
single/multi-day bounds, spring/autumn DST, skipped civil dates, future dates in the selected
zone and today's saved cutoff. Real-store tests cover legacy JSON missing the optional field,
immutable requested ranges and preserved legacy deduplication.

The empty-generation regression uses a real SQLite evidence store with a deliberately
reasoning/whitespace-only test provider. Single-pass and all-empty map generation fail visibly;
whitespace prompts use coverage rules, while a genuinely unmatched Custom query still returns
honest no-match output. This is fault-injection coverage, not a real-model acceptance claim.

Logs: `target/verification/range-{ui-install,ui-lint,ui-build,ui-test,stage,fmt,clippy,build,tests,empty-generation}.log`.
No live destination send, live run rewrite, ordinary-install replacement or app restart occurred.


### Date-range Windows installer

`npm run build -- --ci --no-sign --config <local no-updater-artifacts config>` exited 0.
The packaged executable's hidden `--version` command exited 0 before Tauri/profile initialization.
Raw output excerpts:

```text
    Finished `release` profile [optimized] target(s) in 3m 09s
    Finished 1 bundle at:
ScreenSearch 0.4.0
```

Preserved unsigned test artifact:
`target/verification/artifacts/ScreenSearch_memory-date-ranges_0.4.0_x64-setup.exe`.
Size: 14,270,046 bytes. SHA-256:
`253E143F4C58A3913DCA38A63E094C235720886C42CA50F9D82C73A30306F98E`.
Full packaging/version output: `target/verification/range-package.log`, `range-version.log`
and `range-version-error.log`. The canonical NSIS output remains
`target/release/bundle/nsis/ScreenSearch_0.4.0_x64-setup.exe`.
This proves the current installer build and CLI version response, not native UI/model quality
or live Telegram acceptance. The reported running app was not restarted or replaced.
