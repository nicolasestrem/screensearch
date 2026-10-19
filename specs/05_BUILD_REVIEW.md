# 05 — Build Review

## 2026-10-19 — llama.cpp bootstrap recovery (branch `fix/llama-bootstrap-recovery`)

- **Trigger (field evidence):** installed 0.4.2 on tbx logged
  `ERROR screensearch_lib: llama-server unavailable error=llama.cpp release query returned an
  error status` at every start (19:43:37Z and 19:55:24Z), with `sidecar/` empty, no
  `llama-server*` anywhere and zero `.gguf` files. The app's exact anonymous request
  (`GET https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10`, UA
  `screensearch-v2c`) returned `HTTP 403` with `x-ratelimit-remaining: 0` / `used: 60`; the
  same host's authenticated probe still resolves assets, so naming/matcher/repo move were
  ruled out. `init_inference` returns on that error and is spawned once, so the session stays
  without inference.
- **Change (`crates/inference/src/download.rs`, `src-tauri/src/lib.rs`):** resolve through the
  API as before, and on any lookup failure fall back to a pinned Windows Vulkan asset
  (`b11739`, digest `2a26916f…aba249`, verified before install); bounded three-attempt retry
  for the binary zip on `401`/`403`/`408`/`429`/`5xx`/transport errors; `init_inference` logs
  and reports the full `{e:#}` chain. `SSV2C_LLAMA_RELEASE_URL` and an existing install still
  short-circuit before any network call.
- **RED (native tbX, Rust 1.95.0, before the fix):**

  ```text
  thread 'download::tests::rate_limited_release_lookup_uses_direct_fallback' panicked at
  crates\inference\src\download.rs:1802:34:
  a rate-limited API must not prevent first-run bootstrap: llama.cpp release query returned an
  error status
  Caused by: HTTP status client error (403 Forbidden) for url (http://127.0.0.1:54571/releases)
  test result: FAILED. 0 passed; 1 failed
  ```

- **GREEN (same host, after the fix):**

  ```text
  test result: ok. 110 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
  TEST_EXIT=0
  CLIPPY_EXIT=0            # cargo clippy -p inference -p screensearch --all-targets -D warnings
  BINARY OK: C:/Users/nicol/Documents/GitHub/ssv2c-bin-smoke\api-path\llama\llama-server.exe
             (91841309 bytes total install)
  BINARY OK: C:/Users/nicol/Documents/GitHub/ssv2c-bin-smoke\pinned-fallback\llama\llama-server.exe
             (91841309 bytes total install)
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 110 filtered out   # --ignored
  CARGO_EXIT=0
  ```

  The two `#[ignore]`d real-network checks download the actual archive: one through the live
  API, one through a deliberately failing API URL (nonexistent repo) that exercises the pin,
  digest check and extraction; both then run `llama-server --help` so the Windows loader and
  the DLL set are proven, not assumed. Note: since the b10xxx builds `llama-server.exe` is a
  ~9 KB launcher stub with the code in `llama-server-impl.dll`, so the install directory — not
  the `.exe` — is what the check measures.
- **Independent review (read-only subagent on `b845f7a`):** no P0/P1. Three P2s, all handled:
  the retry prose said "retried three times" where the code makes three *attempts* (fixed in
  `CHANGELOG`, `ARCHITECTURE`, `08`); the comment claiming the API has no digest was replaced
  with the accurate statement (GitHub does publish one; this resolver does not parse it yet —
  now gap `07` #96); and the `SSV2C_LLAMA_RELEASE_URL` path now also verifies the digest when
  the override URL *is* the pin. One clippy regression from that last edit (the now-unused
  non-verifying wrapper) was removed before the re-run. Re-verified on tbx at the final tree:
  `cargo test -p inference --lib` → `110 passed; 0 failed; 2 ignored` (TEST_EXIT=0) and
  `cargo clippy -p inference -p screensearch --all-targets -- -D warnings` → `Finished`
  (CLIPPY_EXIT=0).
- **Not run / open:** the live installed app was never restarted and its data untouched (the
  single-instance plugin means an isolated first run would require stopping it); no full
  `cargo test --workspace` / UI chain was run on this branch, and no merge/release was made.

## 2026-10-19 — v0.4.2 published and field-tested

- **Release:** tag `v0.4.2` (annotated, on squash commit `611a4b4`, PR #142); release
  <https://github.com/nicolasestrem/screensearch/releases/tag/v0.4.2>, non-draft,
  non-prerelease, Latest, published 2026-10-19T12:22:34Z. Tag run 39249860802 passed every step
  including "Verify version consistency" and "Publish GitHub Release"; the earlier dispatch dry
  run 39249417516 on `611a4b4` also passed (tree identical to the audited `7d5133d`).
- **Assets (observed):**

  ```text
  latest.json 773
  ScreenSearch_0.4.2_x64-setup.exe 14608705
  ScreenSearch_0.4.2_x64-setup.exe.sig 424
  ```

- **Updater endpoint (observed):** `latest.json` reports `version` 0.4.2, installer URL
  `.../screensearch-v2c/releases/download/v0.4.2/ScreenSearch_0.4.2_x64-setup.exe`, signature key
  ID `27E1C773C0BDF81E` (matches the shipped pubkey). The old-name endpoint resolves:

  ```text
  HTTP/2 301 -> .../nicolasestrem/screensearch/releases/latest/download/latest.json
  HTTP/2 302 -> .../releases/download/v0.4.2/latest.json
  HTTP/2 302 -> release-assets.githubusercontent.com/...
  HTTP/2 200
  endpoint version: 0.4.2
  ```

- **Field test (maintainer-reported, 2026-10-19):** Nico: "Auto update works. Tested everything
  green and field tested 0 bugs." Recorded as the maintainer's statement; this agent did not
  observe the run and holds no per-item output, screenshot or Telegram send log from it. It is
  taken as the F3 native interactive acceptance (`04 §1c`) and the auto-update check. The
  entry above it is superseded on "no tag claimed": the tag and release now exist.

## 2026-10-19 — v0.4.2 release-prep archive sweep (branch `release/v0.4.2`)

- **Moved, verbatim:** every 0.4.2-arc entry recorded since the v0.4.1 sweep (security alert
  remediation after PR #124, #125; external-agent memory API P0–P3 and review follow-ups,
  #126–#130; F3 G1–G6 and the PR #139/#140/#141 review rounds) →
  `specs/archive/05_BUILD_REVIEW.v0.4.2.md`,
  which is the complete pre-sweep live file plus a one-line header. Same for `06`/`07`/`08`
  (`specs/archive/*.v0.4.2.md`); the `CHANGELOG.md` `[Unreleased]` block became `[0.4.2]` in
  `CHANGELOG-ARCHIVE.md`. Release prep only: no tag or release is claimed by this entry.
- **Verification (docs only):** each archive minus its two header lines is byte-identical to
  the pre-sweep live file:

  ```text
  05_BUILD_REVIEW archive identical
  06_PATCH_PLAN archive identical
  07_KNOWN_GAPS archive identical
  08_CHANGELOG_AI archive identical
  ```

- **Version bump `98b1466`** (`Cargo.toml`/`Cargo.lock`, `tauri.conf.json`, root and UI
  `package.json` + lockfiles → 0.4.2). Native `04 §1c` chain on tbx (Windows, detached at the
  pushed branch SHA, Rust 1.95.0, Node 22):

  ```text
  HEAD=98b146619c0ae496a2e0c9bba61e6805d1f98870
  rust=rustc 1.95.0 (59807616e 2026-04-14) node=v22.23.3
  EXIT npm ci = 0
  EXIT npm run lint = 0
  EXIT npm run build = 0
  EXIT npm test = 0
  EXIT stage-mcp = 0
  EXIT cargo fmt = 0
  EXIT cargo clippy = 0
  EXIT cargo build = 0
  EXIT cargo test = 0
  EXIT bindings diff = 0
  CHAIN_EXIT=0
  ```

  `cargo test` totals: 62 suites, 1142 passed, 0 failed, 15 ignored. Later commits on the
  branch are docs/specs only. Updater compatibility audited read-only: `plugins.updater`
  pubkey, endpoint and `installMode` are byte-identical to v0.3.2–v0.4.1; the shipped
  `screensearch-v2c` endpoint 301-redirects to `nicolasestrem/screensearch` and resolves to
  the current `latest.json`; `scripts/make-latest-json.mjs` strips the tag `v`; nothing but
  `release.yml` triggers on `v*` tags; the 12 → 13 migration tests pass
  (`cargo test -p store --lib migration_`: 13 passed).
- **Codex P2 on PR #142 (`PRRT_kwDOTAuCtM6t5thb`):** the archived 05 entry "Known docs drift
  left for a follow-up" (`docs/f3-g5-g6-verification.md`: stale candidate, UI count 43) was not
  carried forward. Fixed in the PR instead of carrying it: the doc's revision pointer now names
  `specs/archive/05_BUILD_REVIEW.v0.4.2.md` and a dated correction records the committed UI
  counts, observed on Linux with `node --experimental-strip-types --test test/*.test.mjs`
  (Node v26.8.1) in a temporary detached worktree at `957b234` and in `ui/` at the PR tree:

  ```text
  957b234: ℹ tests 50 / ℹ pass 50 / ℹ fail 0
  a634e8c: ℹ tests 55 / ℹ pass 55 / ℹ fail 0
  ```

  The tbx chain at `98b1466` reported the same `# tests 55` / `# pass 55` / `# fail 0`.
- **Carried forward, still PENDING (not archived away):** F3 native Windows interactive
  acceptance (`04 §1c`) has not been run — no `npm run dev` isolated-profile session, no real
  weekly run over a real week of saved dailies, no Obsidian inspection of the weekly note and
  index links, no promotion in the running app, and no running-app screenshot of the weekly
  Settings hint. The G6 weekly Telegram test send needs an explicitly approved recipient.
  External-agent memory API: native Settings interaction (read-token controls) stayed blocked
  in SSH Session 0, and separate-machine MCP → Windows-app traversal is unproven (default
  inbound block; no firewall workaround applied). Open decisions and readings: `07`.

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
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141) →
> `specs/archive/05_BUILD_REVIEW.v0.4.2.md`.
> Live file holds only the current arc.

---
