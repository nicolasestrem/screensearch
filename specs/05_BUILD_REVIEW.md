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
  validation. Installed app/history untouched. Native baseline at `cd9f5fe` completed;
  parent read back actual return codes from both phase logs and independently checked
  the native worktree remained clean:

  ```text
  RC_UI_NPM_CI=0 elapsed_s=4.3
  RC_UI_NPM_LINT=0 elapsed_s=2.5
  RC_UI_NPM_BUILD=0 elapsed_s=5.3
  RC_STAGE_MCP=0 elapsed_s=32.9
  RC_FMT=0 elapsed_s=0.9
  RC_CLIPPY=0 elapsed_s=66.5
  RC_BUILD=0 elapsed_s=83.7
  RC_TEST=0 elapsed_s=100.7
  RC_BINDING_GUARD=0
  ```

  This verifies the unchanged base, not the concurrently developed P1 implementation.
- **Not accepted:** new API behavior, native UI, cross-machine traversal, macOS
  archive execution, allocator behavior, consumer resume and paging measurements
  still require implementation and real evidence. No merge/release/publication.

## 2026-10-16 — P1 implementation in progress (not acceptance)

- Store/traits contracts now exist. Allocator/setup/readiness/reserved-setting guards
  and real SQLite regression tests are implemented; native allocator review/tests
  are running on an immutable component snapshot, not the changing parent tree.
  Frame/coverage SQL is a separate disjoint module under implementation.
- Kernel process-local observations are wired to actual controller/worker events.
  Its isolated pure-std buffer check returned:

  ```text
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```

  Native controller tests and independent review are in progress. Unobserved WGC-
  internal readback errors stay unknown; no frame-gap-derived intervals were added.
- API explicit permissions and independent live read-token authentication are
  implemented. The worker observed clippy exit 0; its API test run exited 101
  solely on the retained IPv6 listener test because that Linux host has no `::1`
  address. The failure is not patched out or called a native pass. Native API
  verification and new frame/coverage endpoints remain open.
- Reference consumer local SQLite/UTC/resume/conflict checks were independently
  rerun by the parent:

  ```text
  Ran 20 tests in 2.788s

  OK (skipped=4)
  CONSUMER_LOCAL_TEST_EXIT=0
  ```

  Four skipped tests require the real API+Store fixture runner; no successful
  production API consumption is claimed. Token UI/composition and the isolated
  MCP transport lane are still under implementation. Full P1 acceptance remains open.

## 2026-10-16 — P1 real API consumer gate exercised

The fixture runner composes the actual API router, production SQLite Store and kernel
observations over nonprivate seeded history, with capture/scheduling off and answer
provider unavailable. It exercises more than 1000 frames, ties/out-of-order timestamps,
mid-window resume, full/textless/empty text, late enrichment, timestamp identity conflict,
and backup rewind/catch-up reconciliation. This is not a canned HTTP response server.

The parent independently reran the full consumer suite with
`SCREENSEARCH_CONSUMER_FIXTURE_CMD` pointing to the compiled
`memory_consumer_fixture` example and `PYTHONDONTWRITEBYTECODE=1`:

```text
Ran 20 tests in 50.708s

OK
PARENT_REAL_API_CONSUMER_EXIT=0
```

All four real-API cases executed; no skips in this run. The earlier local-only skipped
run above remains historical. Timing is under concurrent development load, not a release
latency budget. Native whole-workspace/app/UI and cross-machine MCP acceptance remain
separate gates; P1 is not declared complete by this fixture pass.

### P1 scoped API follow-up (Linux, not native app acceptance)

Parent-executed checks on the current HTTP foundation:

```text
cargo test --locked -p api --test http_api history_reads
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out
P1_HTTP_HISTORY_EXIT=0

cargo test --locked -p api --lib
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
P1_API_LIB_EXIT=0
```

The eight HTTP cases exercise the real Store-backed frame/coverage routes, more than
1000-frame chronology and bounded membership, literal filters/metadata/text, deletion
progress, cursor/history failures, explicit GET/HEAD authorization, truthful empty coverage
and safe errors. Library cases cover cursor bounds/codec/query rejection, future-route
default-deny, capture permission separation and retained connection/stream limits.

These are scoped package checks, not a full Linux workspace build, Windows/IPv6 or UI
acceptance. Independent consumer audit additionally found mid-traversal rewind and
single-history-change recovery bugs not covered by the earlier 20 passing consumer tests;
their fixes and real-API regressions remain required before the P1 gate closes.

### P1 native integration and consumer recovery follow-up

The frozen 41-file runtime candidate (`content_sha256`
`139392392d9299fcad21c40d7ac87a5b413a2e6b0b6d040a00e92cab7c17facf`)
passed on native Windows with Rust 1.95.0 / Node 22:

```text
P1N_fmt_EXIT=0
P1N_clippy_EXIT=0
P1N_build_EXIT=0
P1N_test_EXIT=0
P1N_RUN_DONE
```

Commands were workspace formatting, locked all-target clippy with warnings denied,
locked workspace build/test and the generated-binding/source hash guards. Unchanged UI
bytes had already passed `npm ci`, lint, build, 14 UI tests and MCP staging. Parent parsed
the raw native results: **928 passed, 0 failed, 15 ignored** across 60 result summaries;
the IPv4/IPv6 HTTP listener and IPv6 composition tests both executed successfully. The
15 existing environment/hardware-gated ignores were retained, not introduced to obtain green.
Parent independently re-read the remote native source files: `41/41 PASS`, matching this
candidate and the local worktree.

Consumer F1/F2/F3 are now fixed: a proven mid-drain allocator decrease reconciles after
the real cursor 400; one 409/history transition consumes one logical bounded restart;
a conflicting detail read is not counted as a successful textless re-read. Three new
real-API regressions failed on the old logic and pass on the fix. The full consumer suite
ran **23 tests, 0 skips**; parent verified the runner/source hashes and independently ran
all seven real-API cases:

```text
Ran 7 tests in 31.761s
OK
PARENT_CONSUMER_RECOVERY_EXIT=0
```

Native build/test acceptance is not native app/UI acceptance. Settings keyboard/clipboard/
error-state inspection and separate-machine MCP remain open. In particular, the desktop
app autostarts capture; there is no production `capture.enabled` setting. Verification
must not assume that an invented seed key disables capture or launch against a live profile.

### Follow-up: real tuple seeks and the native UI launch blocker

Measured continuation work exposed a prefix scan despite correct page membership. The
membership CTE now merges two disjoint indexed branches (equal timestamp plus rowid range,
then greater timestamps), using the existing `idx_frames_captured_at`; no DDL, index,
migration, cursor format or public contract changed. Bundled SQLite measurements cover
distinct, all-equal, grouped and shuffled timestamps. A persistent VM-step regression fails
against the old SQL and passes with the fix, alongside a 2500 equal-timestamp-frame drain.
Parent independently reran the full Store package and eight Store-backed HTTP history cases;
both commands exited 0. Native affected/full checks for these new SQL bytes are separate
from the earlier 928-pass snapshot and must be observed before counting them green.

The actual isolated native app was launched on a service window station in SSH Session 0.
It opened only the verification profile, failed WGC capture as expected and initialized the
backend, but created no WebView2 renderer/CDP endpoint. Native Settings interaction therefore
remains **blocked**, not passed. No interactive-session fallback or capture-disable bypass
was used. No model inference, report generation or delivery was exercised. The isolated
process tree was stopped; nonprivate fixtures remain available for backend-only remote proof.

### Post-seek native and actual-app backend evidence

The new seek bytes were subsequently verified on native Windows, not borrowed from the
earlier 928-pass snapshot. Formatting, locked workspace all-target clippy/build/test,
generated bindings and 41-file source guards all passed. Parent fetched and parsed the raw
native log, independently rechecked all remote source hashes and confirmed fixture cleanup:

```text
POSTSEEK_NATIVE_PASSED=930
POSTSEEK_NATIVE_FAILED=0
POSTSEEK_NATIVE_IGNORED=15
POSTSEEK_REMOTE_SOURCE_HASHES=41/41 PASS
FIXTURE_PORT45999_LISTENERS=0
```

A separately built P2 Windows wrapper then exercised the **actual isolated Windows app**
backend on Windows localhost: 1035 expected/received frames, process restart/resume,
1035 matching details, no hash mismatch, valid read credential 200, invalid credential 401,
and direct existing-frame mutation 403. The local fixture credential was seeded, not created
through the unavailable UI. Session 0 capture failed; no GGUF model, inference, report
generation or delivery was used. Private API settings/credentials were revoked/restored and
the owned app stopped after the test.

The separate-machine connection still timed out under Windows' default inbound block with
no rule for the test port. No firewall, SSH-forward or port-proxy workaround was applied.
**Windows localhost is not the cross-machine gate.** Native Settings UI remains blocked.

Independently, the checksum-verified **CI-packaged Linux client** exercised the real P1
API/SQLite/kernel fixture: 1035 frames in five pages, process restart/resume, matching full
text/nulls and 1035 details, 401/403 authorization, zero OCR/answer-provider calls. This closes
neither the Windows network nor GUI gate; P3 saved-report policy remains separately blocked.

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
