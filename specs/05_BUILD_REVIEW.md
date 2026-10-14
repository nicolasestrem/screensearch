# 05 — Build Review

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
