# 05 — Build Review

## 2026-10-16 — External-agent memory API P0 inventory and baseline

- **Contract work:** six delegated inventories/contracts covered production inserts,
  settings/reset boundaries, every HTTP route/MCP tool, saved-report projections,
  wrapper portability and native Windows readiness at `cd9f5fe` (0.4.1, schema 12).
  New normative contract: `03 §7f`. No runtime feature implementation in this pass.
- **Review:** independent boundary review found the leftover bare macOS/Linux
  non-goal in `02 §7`; corrected and parent-verified before runtime implementation.
  Full P1/P2 review found implicit HEAD classification missing; made explicit along
  with persisted-before-live read-token lifecycle. No P1/P2 product contradiction.
  P3 visibility contradiction remains open in `06`/`07`. Separately reviewed boundary
  committed as `9f912e2`; exact parent checks:

  ```text
  boundary_staged_non_goal_carveout=PASS
  boundary_only_staging=PASS
  boundary_secret_scan=PASS
  ```
- **Baseline:** unchanged Linux MCP package, Rust 1.95.0. Actual process output:

  ```text
  BUILD EXIT CODE: 0
  TEST EXIT CODE: 0
  CLIPPY EXIT CODE: 0
  ```

  Commands: `cargo +1.95.0 build --locked -p mcp`, `cargo +1.95.0 test --locked -p mcp`,
  `cargo +1.95.0 clippy --locked -p mcp --all-targets -- -D warnings`.
  Parent independently exercised the compiled binary and stdio protocol:

  ```text
  screensearch-mcp 0.4.1
  parent_verified_stdio_exit=0
  parent_verified_tools=9
  parent_verified_server={"name": "screensearch-mcp", "title": "ScreenSearch screen history", "version": "0.4.1"}
  ```

- **Windows:** `ssh nicol@tbx` available; separate detached worktree and fixture
  tooling created, Rust 1.95.0 verified, official Node 22 extracted with checksum
  validation. Native baseline checks assigned. Installed app/history untouched.
- **Not accepted:** new API behavior, native UI, cross-machine traversal, macOS
  archive execution, allocator behavior, consumer resume and paging measurements
  still require implementation and real evidence. No merge/release/publication.

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
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/05_BUILD_REVIEW.v0.4.1.md`.
> Live file holds only the current arc.

---

## 2026-10-15 — Security alert remediation after PR #124

Based on main `6587e84` (v0.4.1 tag), preserving its version bump, schema 12,
network API, memory reports, Telegram and Quinn security fix. The earlier local
0.4.0 worktree is retained; only the targeted dependency changes were reapplied.

| GitHub alert | Resolution |
|---|---|
| Dependabot [36](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/36) | rustls 0.23.40 → 0.23.45; rustls-webpki 0.103.13 → 0.103.15 |
| Dependabot [39](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/39) | brace-expansion 5.0.8 → 5.0.12 |
| Dependabot [40](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/40) | brace-expansion 1.1.16 → 1.1.21 |
| Dependabot [42](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/42) | all postcss-selector-parser copies → 7.1.6 via npm override ^7.1.6 |
| Dependabot [43](https://github.com/nicolasestrem/screensearch-v2c/security/dependabot/43) | source-map-js 1.2.1 → 1.2.2 |
| CodeQL [2](https://github.com/nicolasestrem/screensearch-v2c/security/code-scanning/2) | dismissed as false positive through GitHub API at 2026-10-15T21:44:44Z |

CodeQL starts from the null pointer initializer but does not model successful
`CredReadW` allocation. `credential_read` dereferences only after success and a
null check, copies the blob before `CredFree`, and never dereferences after free.
The [Microsoft contract](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw#parameters)
guarantees a single allocated credential block and interior pointers, freed via
`CredFree`. This confirms the earlier review in the 0.4.1 archive. No broad
suppression or unsafe-code change is needed. GitHub confirmed `state: dismissed`.

The parser override replaces Tailwind 3, postcss-nested and typography's 6.x
copies without migrating Tailwind. Fresh Node 22 install, lint, TypeScript/Vite
build and existing UI tests passed. All generated assets are byte-identical to a
clean build of main (`diff -qr` exited 0 with no output). Verbatim test summary:

```text
# tests 2
# suites 0
# pass 2
# fail 0
```

Root and UI production-only npm audits returned:

```text
found 0 vulnerabilities
```

The Quinn security guard returned:

```text
PASS: GHSA-4w2j-m93h-cj5j — quinn-proto 0.11.16
```

Cargo formatting and generated-binding diff checks passed. Native Windows
inference, desktop and installer checks cannot run in this Linux environment.
Dependency alerts close after default-branch integration and graph refresh;
local fixes are not claimed to have closed the five Dependabot alerts.

Portable Rust verification used the committed Rust 1.95.0 toolchain.
`cargo test --locked -p api -p mcp` exited 0 (97 passed, one ignored):

```text
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.32s
test result: ok. 29 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 30.20s
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
```

`cargo clippy --locked -p api -p mcp --all-targets -- -D warnings` exited 0:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.86s
```

This supersedes the earlier 0.4.0 worktree result from Rust 1.99; no trait
changes or lint suppressions were needed with the pinned toolchain.
