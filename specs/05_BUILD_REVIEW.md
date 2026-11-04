# 05 — Build Review

## 2026-11-04: 0.4.5 P3 baseline schema + settings v2, synced to main, audited and re-verified

- **Scope and method.** The overnight P3 branch (`feat/0.4.5-p3-baseline-schema`, tip `48a9705`,
  base `dad1eb2`) was synced onto `origin/main`, audited against its acceptance criteria by four
  independent read-only reviewers, and re-verified end to end. Five commits were added today:
  `31f7c66` (ui.appearance), `47b1f1b` (DDL parity test), `1407e5b` (rebuild fixture),
  `bbd8976` (dev-gallery stale specimen), `c039d5e` (maintainer run + contract text). No real
  database, model file, `.env` or secret was opened; the maintainer run was prepared, never
  performed. No em dash was used in new text.
- **STEP 1, sync.** `git fetch --all --prune`; working tree clean. Tips recorded:
  P1 `3d15756`, P2 directions `71030f6`, P3 `48a9705`, P4 `c437297`, `origin/main` `3976551`.
  `git diff 3d15756 e27f42c` is empty, so the P1 tip tree is exactly main's #150 squash and main
  contains the P1 tip by content even though the squash leaves the P1 commits non-ancestors. P3 was
  rebased with `git rebase --onto origin/main dad1eb2 feat/0.4.5-p3-baseline-schema`: clean, five
  commits replayed, new base `3976551`, new tip `0404dd4` before today's fixes. No conflict touched
  `03` section 4 or section 8. P4 was not moved, and the Settings fix (`ui/src/lib/settingsDraft.ts`)
  stays where last night left it: on P3, rebased as `0404dd4` (original hash `48a9705`); it is also
  present in P4's ancestry.
- **STEP 2, what the P2 pick means for P3.** The literal command
  `git diff 3d15756 feat/0.4.5-p2-design-directions -- specs/` changes no section 8 settings key: it
  flips only section 4's rebuild-crate sentence and the build-loop records. The P2 pick that the
  maintainer merged to main (`7e73e30`) does change section 8, so it was escalated before the
  rebase. Exact hunk:
  ```diff
  -**Settings v2 (0.4.5 D6, D9).** The Settings UI is a **closed set of 25 keys**
  +**Settings v2 (0.4.5 D6, D9).** The Settings UI is a **closed set of 26 keys**
   D6). The count rose from 25 to 26 at the 0.4.5 P2 pick (2026-11-04), which added `ui.appearance` so
   the four themes are user-selectable instead of one hardcoded theme.
  ...
  +| `ui.appearance` | `pine-light` | UI | General | Enum `pine-light` / `pine-dark` / `pine-system` / `ink-light` / `ink-dark` / `ink-system`; ... P3 adds it to the settings v2 registry and baseline (0.4.5 P2 pick, 2026-11-04; `UI_REFERENCE §1`/`§2`). |
  ```
  Maintainer answer: land it in P3. Landed in `31f7c66`: `UiAppearance` enum with the six kebab-case
  wire tokens, `Settings.ui_appearance` with `#[serde(default)]` for pre-v2 config snapshots,
  default `PineLight`, kernel load/save of the `ui.appearance` key, rebuild policy count 26 plus
  carry assertions, and the regenerated bindings. The shell theme applier and the Settings select
  stay P7's, as the section 8 row states.

- **STEP 3, acceptance audit.** Verdicts with evidence; the maintainer-only rows stay OPEN.

  | Item | Verdict | Evidence |
  |---|---|---|
  | a. `crates/store/src/schema.rs` holds the v14 DDL from `03` section 4, proven by a comparison test | MET | New `crates/store/tests/p3_schema_parity.rs` (commit `47b1f1b`, hardened in `3d9d89a`). Parses section 4 from `include_str!` of the spec (bounded by the next `##` heading, with sentinel objects), token-normalizes after stripping `--` comments, and asserts every section 4 table column/constraint, `CREATE VIRTUAL TABLE`, index and ALTER-added column exists in `BASELINE_SQL` with the identical definition; the three documented table-definition extras (`frames.image_purged`, `text_spans.line_index`, `embeddings` UNIQUE) are required present, not merely allowed; the seven carried FTS/vec triggers are existence-checked by name because section 4 declares no `CREATE TRIGGER` (their bodies are covered by the functional FTS/vec tests); the two declarations v3 dropped (`ocr_text`, `ocr_text_fts`) are pinned absent; a fresh store materialises exactly the baseline. Mutation-tested five ways: missing `idx_entities_kind` fails with "03 section 4 declares index `idx_entities_kind` but the v14 baseline does not create it"; narrowed `entities.canonical_name` fails with the definition diff; `session_vectors` cosine -> l2 fails with "virtual table `session_vectors` differs"; missing carried `embeddings_ad` fails with "documented carried trigger ... is missing"; removing the `embeddings` UNIQUE fails with "the documented carried extra ... is missing" (both tests red, then restored and green). Raw run on the final tree: `test result: ok. 2 passed; 0 failed`. |
  | b. A DB not at v14 refuses to open, naming the rebuild tool | MET | `crates/store/src/lib.rs:157-207` validates before any pragma and refuses with "database schema_version is {found}, but this build requires 14; rebuild the database with the dev-only `ssv2c-rebuild` tool". `crates/store/tests/baseline.rs:174` builds synthetic v13 and v15 files, asserts both messages contain `ssv2c-rebuild`, the found version and 14, and asserts bytes and mtime are unchanged after the refusal. Raw: `test result: ok. 8 passed; 0 failed` for `cargo test -p store --test baseline`. |
  | c. Rebuild tool: read-only source, hash/mtime unchanged, carry set, derived drops, no rename | MET | `crates/rebuild/src/lib.rs:633-636` opens the source with `SQLITE_OPEN_READ_ONLY`; the only mutating PRAGMA is `foreign_keys=OFF` on the destination (no VACUUM, no checkpoint, no write to the source); `crates/rebuild/tests/rebuild.rs:127-143` asserts source bytes and mtime unchanged; destination must not exist (`lib.rs:170-176`, test `rebuild_refuses_an_existing_destination_and_leaves_it_alone`); carry set `lib.rs:59-73` covers the P3 list plus the documented wider set (`text_spans`, `vision_analysis`, `tags`, `frame_tags`, `report_deliveries`, `06` #32), and the hardened copy loop (`b53db47`) now bails with the table name if a carry table is missing from either side instead of silently skipping it; `jobs` and `chrome_text_catalog` dropped and FTS5/vec0 re-derived from copied rows (`lib.rs:384-437`); the settings policy is embedded from `03` section 8 at build time and parsed at runtime (`crates/rebuild/src/settings.rs:29,132-174`), reserved `api.history_id`/allocator high-water copied verbatim (test high-water 42 vs source max frame id 7), nine deleted keys dropped; `integrity_check`/`foreign_key_check`/schema 14 enforced and tested, and a violation now reports the exact rows (`table`, `rowid`, `parent`, `fkid`) instead of a count; `--counts` and `--sample-frames N` implemented and the 20-id sample is exercised by a dedicated 23-frame fixture test (`b53db47`); no rename flag exists at all, so the rename stays the maintainer's manual step. `cargo test -p rebuild` on the final tree: `test result: ok. 6 passed` (lib) and `ok. 8 passed` (integration). |
  | d. Settings v2: every section 8 row per its disposition; retired keys gone; file-only from `advanced.toml`; rewrite once; round-trip; 26 UI keys | MET (26, not the kickoff's pre-pick 25) | Spec table on main: 26 `UI` (25 typed fields plus `api.token`; `api.read_token` shares that row; `app.tray_toast_done` is a KV marker), 44 `file-only`, 9 `deleted` (`03` section 8 lines 1697-1777). `load_settings`/`save_settings` use the same 25 typed key strings, including `ui.appearance` (`crates/kernel/src/settings.rs:30-77`, `:95-180`); no typed field is also a file field; eight `advanced.toml` sections plus the `app.tray_toast_done` KV marker (`03` section 8 gives it no file home, `07` P1 entry (e)). Rewrite-once and unknown-section/field tests pass (`crates/kernel/tests/settings.rs:1039`, `:459`, `:512`); the full-set round-trip sets all 25 typed fields including `ui_appearance: UiAppearance::InkDark` (`:38-76`) and asserts struct equality, 33/33 pass on the final tree; exactly 26 UI / 44 file-only / 9 deleted is asserted from the parsed table (`crates/rebuild/src/settings.rs:185-190`, test `settings_policy_matches_the_spec_table`); reserved history/allocator keys survive every settings path (store `history_allocator` 10/10, kernel `:534`); `get_advanced_config` is read-only with no setter. |
  | e. Eleven new tables created empty, nothing writes them outside tests | MET | `crates/store/tests/baseline.rs:14-26` lists the eleven tables and `:138-148` asserts each is declared and `COUNT(*) = 0` with "must be created empty: no writer exists in P3". Write grep outside `tests/` returns only the four test-only CHECK fixtures pasted below; read grep for the eleven names returns no SQL access; `JobKind` has only `EmbedText` and `VisionTag`; no command, job or route names them. |
  | f. Bindings regenerated and committed | MET | `cargo test -p traits` regenerated them (124 export tests), then `git diff --exit-code -- ui/src/bindings` printed nothing and exited 0; `31f7c66` commits `Settings.ts` and the new `UiAppearance.ts`. |
  | Maintainer-only: rebuild run against a copy of the real DB, per-table counts before/after, 20-frame Moment spot check, app boots on v14 | OPEN (maintainer) | Prepared as a nine-step procedure in `docs/kickoffs/P3.md` (`c039d5e`); never run by an agent. Raw output to paste back is specified in step 6. |
  | Maintainer-only: in-app Settings > Integrations check from the 2026-11-04 QA pass (turn the HTTP API on, change and save another setting, confirm the API is still listening) | OPEN (maintainer) | The fix is on this branch: `ui/src/lib/settingsDraft.ts` plus `ui/test/settingsDraft.test.mjs`, committed as `0404dd4` (original hash `48a9705`). The regression test covers the exact revert scenario; only the native run is missing. |

- **STEP 3 evidence: greps (raw).**
  ```
  $ git grep -n 'MIGRATIONS' -- crates
  (no output)

  $ git grep -n 'ssv2c-rebuild' -- crates/store/src
  crates/store/src/lib.rs:144:                 missing; rebuild the database with the dev-only `ssv2c-rebuild` tool \
  crates/store/src/lib.rs:176:         rebuild the database with the dev-only `ssv2c-rebuild` tool (`03 §4`, 0.4.5 D2)",
  crates/store/src/lib.rs:196:    /// found and the `ssv2c-rebuild` tool.
  crates/store/src/lib.rs:206:        //   refuse that last case forever (`ssv2c-rebuild` cannot repair it either), and
  crates/store/src/schema.rs:8://! rebuilt with the dev-only `ssv2c-rebuild` tool (`03 §4`, 0.4.5 D2/D3).

  $ git grep -n 'advanced.toml' -- crates
  lines=29

  $ moved typed fields still in crates/traits/src/ipc.rs
  capture_diff_threshold: 0
  sidecar_device: 0
  enrich_embed_text: 0
  overlay_max_results: 0
  uia_latency_budget_ms: 0

  $ git grep -nE 'jpeg_quality|event_on_click|event_on_clipboard|event_on_scroll_stop|event_on_typing_pause|event_typing_pause_ms|uia_run_on_interactive|image_embeddings|network_notice_state' -- ui/src
  (no output)

  $ git grep -nE '\b(INTO|UPDATE|DELETE[[:space:]]+FROM)[[:space:]]+(entities|...|ask_turns)\b' -- crates/ src-tauri/src ui/src
  crates/store/tests/baseline.rs:374:            "INSERT INTO entities (kind, canonical_name, normalized_key, source, confidence,
  crates/store/tests/baseline.rs:386:            "INSERT INTO insights (kind, window_start, window_end, payload, generator)
  crates/store/tests/baseline.rs:396:        "INSERT INTO threads (id, title, cluster_key, first_seen_at, last_seen_at)
  crates/store/tests/baseline.rs:403:            "INSERT INTO thread_overrides (kind, thread_id, target_thread_id, payload)
  ```
  The retired-key grep across `crates` and `ui/src` returns 42 hits, all in the allowed places: the
  single retired list and its doc comments (`crates/kernel/src/settings.rs:256-269`), the retirement
  tests and the pre-v2 snapshot fixture (`crates/kernel/tests/settings.rs:164,478`), the rebuild
  tool's own deleted list and fixtures (`crates/rebuild/src/settings.rs:207-215`,
  `crates/rebuild/tests/rebuild.rs:28-36,397-411`), and two `crates/uia` doc comments explaining the
  retirement. Before `bbd8976` the dev-only P2 kitchen gallery was the one non-allowed hit
  (`ui/src/routes/Kitchen.tsx:322`); it was removed with the maintainer's one-time grant and the
  `ui/src` grep is now empty.

- **STEP 4, maintainer run.** Rewritten as a numbered, copy-pasteable PowerShell procedure in
  `docs/kickoffs/P3.md` (`c039d5e`), nine steps: quit and prove no process remains; the live path as
  the code resolves it (Tauri `app_data_dir()` joined with `screensearch.db`, `src-tauri/src/lib.rs:1439,1444`,
  `%APPDATA%\app.screensearchv2c.desktop\screensearch.db`); copy the db plus `-wal`/`-shm` and print
  sizes; build the tool; rebuild the copy into a new v14 file with per-table counts; what to paste
  back; the swap (no data-dir or db-path override exists, cited) with the `*.v13.bak` rename only
  after the maintainer confirms the new file opens; the Moment spot check via one read-only
  `sqlite3` query and the `/timeline/<id>` route; and a one-command rollback. An adversarial audit
  then hardened it: the swap is guarded and error-stop, the log confirmation is separate from and
  gates the `*.v13.bak` rename, the sample is bounded to the 30-day range the Timeline can reach,
  the Moment expectations accept a localized minute match and a retention-purged reconstruction,
  and the rollback is idempotent. Nothing that existed before the run is deleted (only the rebuild
  tool removes the destination it created on failure). Never run by an agent.

- **STEP 5, verification, raw output.** Run in this order from the repository root on the code/docs
  tip `c039d5e`, one command per shell, Windows PowerShell. (The audit hardening below landed after
  this run; the whole list was run again on the final tree, see "Re-verification" below.)
  ```
  === npm ci ===
  added 347 packages, and audited 348 packages in 5s
  152 packages are looking for funding
  13 vulnerabilities (12 moderate, 1 high)
  npm warn install-scripts 1 package had install scripts blocked because they are not covered by allowScripts:
  npm warn install-scripts   esbuild@0.25.12 (postinstall: node install.js)
  === npm ci exit=0 ===
  === npm run lint ===
  npm notice run screensearch-ui@0.4.2 lint
  npm notice run eslint .
  === npm run lint exit=0 ===
  === npm run build ===
  ✓ built in 1.90s
  === npm run build exit=0 ===
  === node scripts/stage-mcp.mjs ===
  [stage-mcp] building screensearch-mcp (release)...
      Finished `release` profile [optimized] target(s) in 0.23s
  [stage-mcp] up to date: C:\Users\nicol\Documents\GitHub\screensearch\src-tauri\binaries\screensearch-mcp-x86_64-pc-windows-msvc.exe
  === node scripts/stage-mcp.mjs exit=0 ===
  === cargo fmt --all -- --check ===
  (no output)
  === cargo fmt exit=0 ===
  === cargo clippy --workspace --all-targets -- -D warnings ===
      Checking rebuild v0.4.2 (C:\Users\nicol\Documents\GitHub\screensearch\crates\rebuild)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.50s
  === cargo clippy exit=0 ===
  === cargo build --workspace ===
      Compiling screensearch v0.4.2 (C:\Users\nicol\Documents\GitHub\screensearch\src-tauri)
      Finished `dev` profile [unoptimized + debuginfo] target(s) in 36.98s
  === cargo build exit=0 ===
  === cargo test --workspace ===
  ```
  The command exited 0. Raw `test result:` lines for the P3-owned and settings-relevant binaries,
  copied verbatim from the run (in command order):
  ```
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
  test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
  test result: ok. 124 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 80 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
  test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
  test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
  ```
  Across every binary in the workspace the run printed only `test result: ok` lines; their sum is
  1195 passed, 0 failed. The full raw log of the command is at
  `%TEMP%\p3-cargo-test-2026-11-04.log` on the machine that ran it.
  ```
  $ git diff --exit-code -- ui/src/bindings
  (no output)
  === git diff bindings exit=0 ===
  ```
- **Open items handed to the maintainer (not P3 code gaps).** `06` #30 (`03` section 4 does not
  describe ten as-built details; the parity test pins the documented deltas and the doc pass is
  still open), `06` #32 (the wider rebuild carry set needs maintainer confirmation), `06` #33 (three
  section 8 defaults disagree with the as-built code: `storage.max_width`, `storage.retention_days`,
  `text.chrome_suppress_min_seen`), and the `07` question about an existing profile's demoted
  `file-only` values not being imported into `advanced.toml`. None of them changes tonight's code,
  the synthetic rebuild evidence or the verification above.

- **Independent adversarial validation (2026-11-04, 16 subagents in two waves).** Sixteen fresh
  read-only reviewers attacked the branch from separate angles: schema parity test, rebuild tool,
  settings v2, maintainer-run procedure, P3-READY and 05 record truthfulness, patch ownership and
  hygiene, topology and publication state, an isolated clean-worktree build, plus three
  cross-checkers. Wave 1 found six should-fix defects plus record nits; wave 2 independently
  confirmed the still-open ones, rejected the claims already fixed in-tree, and surfaced two more
  (`ctx_size` sentinel and overlay-hotkey coverage, P3-authored em dashes). The clean-worktree run
  (`git worktree add --detach` at the then-tip, then `cargo fmt --all -- --check`,
  `cargo clippy -p store -p rebuild --all-targets -- -D warnings`, `cargo test -p store -p rebuild`)
  exited 0/0/0, and the worktree plus its temp target were removed. No reviewer edited a tracked
  file; no real database, model, `.env` or secret was touched.
- **Hardening and record corrections landed from the audit (one fix per commit).**
  - `3d9d89a`: the parity test now *requires* the documented as-built extras instead of merely
    allowing them, anchors the section 4 extraction at the next heading with sentinel objects, and
    corrects the trigger claim (section 4 declares none; the carried triggers are name-checked and
    their bodies are covered by the functional FTS/vec tests). Mutation check: removing the
    `embeddings` UNIQUE made both tests fail with "the documented carried extra ... is missing",
    then the tree was restored, forced to rebuild and re-verified green.
  - `b53db47`: the rebuild now refuses a source or destination missing a carry table instead of
    silently skipping it, reports the exact `pragma_foreign_key_check` rows (`table`, `rowid`,
    `parent`, `fkid`) instead of only a count, and three new tests cover a dropped carry table, an
    orphan artifact row and a 20-id sample from a fixture extended to 23 frames.
  - `8aca7a5`: the `UiAppearance` doc comment no longer claims six `data-theme` ids (four are;
    `*-system` is a stored intent), and the `Settings`/`AdvancedConfig` headers no longer claim
    exact `03 §8` default parity while `06` #33 is open.
  - `244638a`: all 83 em-dash characters the overnight commits introduced in P3-authored lines
    (comments, four CLI help strings, generated bindings) were replaced with ASCII hyphens; Rust
    sources were fixed and `ui/src/bindings` regenerated, never hand-edited. Pre-existing text
    outside the P3 additions keeps its characters.
  - `9d3a9ee`: restored the `sidecar.ctx_size = 0` auto-sentinel test and the raw-DB assertion in
    the custom overlay-hotkey test, both of which the settings refactor had dropped.
  - Record corrections in this entry: the trigger-parity, typed-key-count, rebuild-parse and
    "only PRAGMA" wordings above; test-function names replace line citations where line drift is
    possible; and the maintainer-run description now matches the hardened procedure.
- **Re-verification on the hardened tree (raw).** The full list was run again from the repository
  root on the final tree (`9d3a9ee` with the em-dash and coverage commits), one command per shell.
  Every command exited 0:
  ```
  === npm ci exit=0 ===
  === npm run lint exit=0 ===
  === npm run build exit=0 ===        (✓ built in 2.03s)
  === node scripts/stage-mcp.mjs exit=0 ===
  === cargo fmt --all -- --check exit=0 ===
  === cargo clippy --workspace --all-targets -- -D warnings exit=0 ===
  === cargo build --workspace exit=0 ===
  === cargo test --workspace exit=0 ===  (TOTAL passed=1199 failed=0)
  === git diff --exit-code -- ui/src/bindings exit=0 ===
  ```
  The P3-owned binaries in that run: rebuild lib 6, rebuild integration 8 (was 5), kernel settings
  33 (was 32), traits 124 + sessions_contract 4, store parity 2, store baseline 8,
  history_allocator 10, store integration 65. Raw tails included
  `test result: ok. 8 passed; 0 failed` for `tests/rebuild.rs` and
  `test result: ok. 33 passed; 0 failed` for `kernel --test settings`. Full log kept at
  `%TEMP%\p3-cargo-test3-2026-11-04.log`. The clean-worktree build above independently re-ran
  `cargo fmt`, `cargo clippy -p store -p rebuild` and `cargo test -p store -p rebuild` from a detached
  checkout with its own target directory, so the branch is not relying on the shared build cache.
- **Publication.** Branch `feat/0.4.5-p3-baseline-schema` pushed to `origin` and opened as
  [PR #153](https://github.com/nicolasestrem/screensearch/pull/153) against `main` on the
  maintainer's explicit 2026-11-04 instruction, after the recompute above. **Not merged**; no review
  automation was triggered. The PR body carries the summary, the verification block and the
  maintainer-only items.
- **One ownership note for reviewers.** The 2026-11-04 QA pass's Settings API fix touches
  `ui/src/lib/settingsDraft.ts`, `ui/test/settingsDraft.test.mjs`, `ui/src/lib/ipc/mutations.ts` and
  `ui/src/routes/Settings.tsx`. It is in P3 because P3 introduced the typed `api.*` settings and the
  `set_settings` hot-apply that the stale draft reverted, so it is a settings-consumer regression
  fix, not new behaviour; it was recorded in `05` and the maintainer asked for the branch that
  carries it. The only file outside the stated P3 boundary that P3 changed without that causal
  link is `ui/src/routes/Kitchen.tsx`, removed under the explicit one-time maintainer grant.

## 2026-11-04: 0.4.5 P2 design system v2 (branch `feat/0.4.5-p2-design-system`)

- **Specs first (P2 precondition, 0.4.5 D4).** The pick is recorded before any visual code:
  `UI_REFERENCE` §1 (direction **Logbook**: C's structure kept, two skins as four themes, pine
  default, success rule, Source Sans 3 + Cascadia Mono at a 12px floor, mono only for measured
  values, motion state-only, one local SVG icon set at one 1.5 stroke), §2 (three layers, the
  semantic families, the four `data-theme` values, density, z-layers, durations), §5 (primitive
  inventory), `03 §8` (`ui.appearance`, six-value enum, `pine-light` default; the closed UI set
  rises 25 -> 26), `07` (P3 adds the key to the settings v2 registry; P7 renders it and resolves
  `*-system`), `docs/0.4.5.md` (D6 ceiling 26; P7/P9 acceptance "all four themes").
- **Deviations, stated.** Two, both authorised conversationally (`P2.md`'s "stop and ask" clause)
  rather than pre-approved in writing. (a) *Sequence:* `P2.md` expects the pick merged to `main`
  before this branch exists and says not to commit until the maintainer validates and names the
  repo. The maintainer instructed this run to continue P2 and to commit locally, so the branch was
  cut from `origin/main` (which carries the P1 contract, #150) and the commits stayed local until
  the maintainer authorised the push. (b) *Write boundary:* the kickoff permits only append-only
  entries outside the owned UI paths, but the phase also had to write `specs/01 §8` (Wave A status),
  `specs/03 §8` and `§13d` (the `ui.appearance` row and the acceptance status, both following the
  maintainer's own 26-key ruling), `specs/04 §1` (a pointer the next phase reads), `specs/UI_REFERENCE`
  `§1/§2/§5/§10` (the pick and the two sections whose own text says "P2 writes" them), `docs/0.4.5.md`
  (the roadmap's P2 status), `PRODUCT.md` (written by the maintainer-side `impeccable init`) and
  `docs/screenshots/p2/**` (the phase's visual evidence). Every one is documentation, status or a
  capture: no code left the owned paths plus the three approved exceptions, and the contradiction
  is logged as `06 #34` with what a future kickoff should do instead.
- **Built:** `ui/src/styles/fonts/` (Source Sans 3 upright + italic variable woff2, Cascadia Mono
  variable woff2, SIL OFL 1.1 with licence text beside each file); `tokens.css` (226 distinct custom
  properties over 357 declarations: 114 `--p-*` primitives plus the semantic layer redeclared in four
  theme blocks, the scales and the density block); `tokens.compat.css` (25 aliases, which is what the
  46 v1 names need: 21 of them kept their v2 name); rewritten `globals.css`; `ui/tailwind.config.js` (legacy keys intact, v2 keys
  added, everything var-backed); 30 primitives; `icons/index.tsx` (55 glyphs); `routes/Kitchen.tsx`
  + `KitchenPrimitives.tsx`; `test/contrast.test.mjs` + `test/tokens.test.mjs`; one additive
  dev-only route line.
- **Font axes, measured** (`fontTools`, not asserted):

```
CascadiaMono.woff2           | Cascadia Mono | axes: [('wght', 200.0, 400.0, 700.0)]
SourceSans3-Italic.ttf.woff2 | SourceSans3VF | axes: [('wght', 200.0, 200.0, 900.0)]
SourceSans3-Upright.ttf.woff2| SourceSans3VF | axes: [('wght', 200.0, 200.0, 900.0)]
digit widths: uniform in both faces, so tabular figures need no tnum feature
```

- **Verification, P2.md's order, raw.** `npm ci` first ran while the Vite dev server held
  `node_modules` open and failed on locked files; the server was stopped, `npm ci` then exited 0.

```
=== npm run lint
npm notice run screensearch-ui@0.4.2 lint
npm notice run eslint .
=== npm run test
ℹ tests 90
ℹ pass 90
ℹ fail 0
=== npm run build
dist/assets/react-vendor-LgzazqmD.js                 143.35 kB │ gzip: 45.97 kB
✓ built in 2.29s
=== node scripts/stage-mcp.mjs
[stage-mcp] up to date: ...screensearch-mcp-x86_64-pc-windows-msvc.exe
=== cargo fmt --all -- --check
FMT-OK
=== cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.64s
=== cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 39.22s
=== cargo test --workspace
test result: ok. 12 passed; 0 failed
test result: ok. 113 passed; 0 failed
test result: ok. 4 passed; 0 failed
test result: ok. 26 passed; 0 failed; 4 ignored
```

```
$ grep -rnE "#[0-9a-fA-F]{3,8}\b|rgb\(|hsl\(" ui/src/components/primitives
0
$ grep -rn "font-size: *[0-9]" ui/src/components/primitives
0
$ grep -rniE "url\(http|fonts\.googleapis|fonts\.gstatic|cdn\." ui/src/styles
0
$ grep -rni kitchen ui/dist
0
$ grep -rn "data-theme" ui/src/styles/tokens.css
4
$ git diff --exit-code -- ui/src/bindings
bindings-diff-exit=0
```

- **Enforcement fails when it should, then passes.** `contrast.test.mjs` with pine-light
  `--status-warn-fg` set to `#e8b458` fails with `pine-light: status-warn-fg on status-warn-bg =
  1.53:1`; restored, 5/5 pass and print 124 pairs (31 x 4 themes) with minima 4.90 / 5.39 / 4.88 /
  5.39. `tokens.test.mjs` with `border-[#8ccaaf]` appended to `Button.tsx` fails with
  `Button.tsx:120 [colour-literal] #8ccaaf";`; restored, 3/3 pass, 0 hits over 30 primitives x 7
  rules. Both restorations verified by sha256 against the pre-demo snapshot.
- **Four-variant detect gate** (`vite` dev server, URL scan at 1440x900, pinned 0.1.5):

```
pine-light  1 anti-pattern found.
pine-dark   1 anti-pattern found.
ink-light   1 anti-pattern found.
ink-dark    1 anti-pattern found.
  [layout-transition] transition: width
```

  The single remaining finding is not in P2's code. Tailwind emits `.transition-[width]` because
  three v1 files still use it (`components/domain/ModelPanel.tsx:104`, `SetupDetails.tsx:53`,
  `SetupIndicator.tsx:74`), and `P2.md` forbids touching `components/domain/**`. For P10:
  `grep -rn "transition-\[width\]" ui/src` lists exactly those call sites. Nothing P2 owns has a
  layout transition (`grep -rn "transition-\[width\]|transition-\[height\]|transition-all"` over
  `primitives`, `Kitchen*`, `styles` returns 0). The pass converged 27 -> 9 -> 6 -> 2 -> 1 by real
  fixes: card-in-card specimens un-nested, `Panel` and `Expander` restyled as borderless surfaces
  (a control inside them is then the only bordered element), 8px inner padding on Segmented, Badge,
  Chip, Kbd and table cells, no `transition-[grid-template-rows]`, state icons inlined beside their
  titles, and `/kitchen` moved out of the AppShell so the scan measures v2 and not v1 chrome.
- **AA, all four variants** (`/kitchen?theme=<theme>`, every visible text node against its effective
  background, transitions frozen):

| variant | pairs | min font | under 12px | enabled below AA | disabled below AA | worst |
|---|---|---|---|---|---|---|
| pine-light | 528 | 12px | 0 | 0 | 0 | 4.90:1 |
| pine-dark | 528 | 12px | 0 | 0 | 0 | 5.39:1 |
| ink-light | 528 | 12px | 0 | 0 | 0 | 4.88:1 |
| ink-dark | 528 | 12px | 0 | 0 | 0 | 5.39:1 |

  Re-measured after the PR #151 review round added four gallery specimens (528 pairs, up from 516):
  0 failures, min font 12px, worst unchanged at the ratios above. Re-measured a third time on `main`
  after the Cascadia Mono face was corrected from `font-weight: 400` to the file's real `200 700`
  range, since a weight change could shift rendering: identical at 528 pairs, 0 failures, and the
  same four minima.
  Disabled text clears AA too: each theme gained one ink step (`--p-mist-ink-550`,
  `--p-graphite-ink-450`, `--p-paper-ink-550`, `--p-slate-ink-450`) solved to 4.5:1 on
  `--surface-inset`, so `--text-disabled` does not rely on the SC 1.4.3 exemption and inertness is
  carried by surface, border and absent hover instead of washed-out text.
- **Shim proof.** In a plain browser (no Tauri backend, so each screen shows its real error state):
  `/` "Can't reach the kernel", `/recall` "Search your screen history", `/timeline` "Couldn't load
  the timeline", `/settings` "Couldn't load settings", all on `rgb(238, 241, 239)` with
  `--accent: #23694f`. No unstyled or transparent surface.
- **Review round on the residuals.** Three claims from review were checked against the tree:
  (a) the `nested-cards` findings were cleared structurally by making `Panel` and `Expander`
  borderless surfaces, not by bending the gallery, so the Panel action chips and the Badge
  ("Running", "Saved", "Saved") were **restored** afterwards and the count stayed at 1 per variant:
  the shipped direction keeps those chips, and a gallery that hides them would misreport the design.
  (b) `--text-tertiary` on `--surface-app` measures 5.22 / 7.52 / 5.24 / 7.48 across the four themes,
  so no tertiary pair fails; the audit walks it and the static test covers it. (c) Surface separation
  after the borderless change is real, not invisible: `--surface-raised` renders
  `rgb(255,255,255)` on `rgb(238,241,239)` (pine-light) and `rgb(27,31,29)` on `rgb(12,15,13)`
  (pine-dark), a 14 to 17 channel-point step, with the ink pairing identical. The three
  P10-owned items this leaves behind (legacy `transition-[width]` bars, the `StatusRail` uppercase
  wordmark, and the flatter look of unconverted screens until the refont) are now tracked as gaps
  in `07`, not only described here.
- **PR #151 review round (Codex `6dd36eb`, 1 inline finding, valid; two more of the same class found
  by audit and fixed).** (a) *Reported:* the Combobox popup caps its height and focus stays in the
  input, so `aria-activedescendant` moved past visible options without revealing the active row,
  and Enter could commit an option the user could not see. `Combobox` now holds a ref on the listbox
  and, whenever the active index or the visible set changes, moves only that listbox's `scrollTop` by
  the rect delta measured against the popup's inner padded band (`clientTop` plus computed
  `paddingTop`/`paddingBottom`), so the border and `py-1` padding survive at both ends and page
  scroll never moves. (b) *Found by audit, same finding class:* arrow keys could land on a
  `disabled` option, which paints no highlight at all, leaving a dead Enter. Painting the active row
  instead was measured and **rejected**: `--text-disabled` over `--surface-selected` is 4.44 / 3.39 /
  4.44 / 3.41 across the four themes, below AA, and P2 claims AA even for disabled text. Navigation
  now skips disabled options (`stepTo`/`edgeTo`, the same rule `RadioGroup` and `Segmented` already
  follow and what a native `<select>` does), hover on a disabled row no longer moves the active row,
  and `openList` indexes into the *rendered* set so it cannot point at a row that is not in the list.
  When every option is disabled no row is active at all (the derived index is `-1`), so
  `aria-activedescendant` is omitted rather than pointing at a row that paints nothing.
  (c) *Found by audit:* the Dialog/Sheet focus trap filtered candidates with
  `!node.hasAttribute("hidden")`, which only sees a node's own attribute, so a control inside a
  collapsed `Expander` body or an inactive tab panel was accepted as the first/last candidate and
  `.focus()` on it was a silent no-op, swallowing the Tab wrap. Both traps now require
  `!node.closest("[hidden]") && node.getClientRects().length > 0`.
  Evidence, real browser, in two eras because the skip rule landed between them. Against the reveal
  alone (Excel still reachable, 15 keyboard steps captured as `active label / scrollTop /
  clientHeight / scrollHeight / fully inside the band / window.scrollY`): **0 of 15 hidden with the
  fix and 7 of 15 hidden with the reveal disabled** (Excel, PowerPoint, Outlook, Spotify, Zoom, End
  and the ArrowUp wrap, all at `scrollTop 0`), `scrollY` fixed at `6078` throughout. Re-probed on
  the final code, where arrows skip Excel (13 steps: seven ArrowDown, End, Home, the ArrowUp wrap,
  a hover on the disabled row, plus open): **0 hidden, no active row ever disabled**, same `6078`.
  Opening with the last option selected lands at `scrollTop 138` of a 138 max, the row fully
  visible; ArrowDown from Obsidian goes to PowerPoint and hovering Excel leaves the active row on
  Zoom. Over a new `Dialog focus trap` specimen, Shift+Tab from the panel stops on the visible
  `Advanced capture` toggle with no step inside a `[hidden]` subtree and Escape returns focus to the
  trigger, where the reverted predicate leaves focus stuck **on the panel**. Both control runs
  restored their file byte-identically. Lint clean, `90 / 90` tests, build ok, `kitchen` still 0 hits
  in `dist`.
- **Design gate held at one finding, after one real regression.** The first version of the trap
  specimen carried a ~100 character `note`, and the gallery's note `<p>` had no measure, so the
  detector reported `[line-length]` (2 anti-patterns per variant) against a ~1400px row. The shared
  `KitchenSpecimen` note now takes `max-w-prose` (65ch, a Tailwind preset), which is the detector's
  own recommendation and keeps any note readable: the same 100 character note now renders as two
  lines at 49 characters each, and the gate is back to the single `transition: width` residual.
- **PR #151 second review round (Codex `917059e`, 1 inline finding, valid and latent, fixed).**
  `Button` dropped `onClick` while `loading`, which stops a click handler but not a native form
  submit: with `type="submit"`, `aria-disabled` is announced and ignored, so clicking the spinner or
  activating the focused button with Enter submitted the form again while the first request was
  pending. The handler now stays attached and calls `event.preventDefault()` while loading, which
  covers an ordinary click, Enter/Space on the focused button, and the implicit submission a form
  runs by clicking its default submit button. No callsite hits it today (the three `type="submit"`
  users pass native `disabled`), so the defect was latent in the primitive's contract rather than
  live in a screen. Evidence, throwaway form page at `ui/probe.html` with one loading and one idle
  submit button, deleted afterwards: **0 submits while loading** for click, Enter-on-button and
  implicit submission, `1` submit when idle, and the same three runs against the reverted handler
  submit every time. `Button.tsx` restored byte-identically. Lint clean, `90 / 90` tests, build ok.
- **Commits (local branch `feat/0.4.5-p2-design-system`, cut from `origin/main` at `e27f42c`,
  the P1 contract):** `cf56222` PRODUCT.md, `8136179` specs record the pick, `5c9c679` and `8e1935b`
  icon contract wording, `75c77ba` the implementation, `d113a0f` the review round (truthful gallery,
  `07` residuals), then the documentation commit that also opens the PR, the review rounds and the
  capture refreshes (16 commits). Merged to `main`: yes, `7e73e30` (squash), 2026-11-04.
- **Open.** Populated screenshots need the Tauri app and a seeded database (`docs/SCREENSHOTS.md`),
  which this run did not touch by rule; `/kitchen` in all four themes is the visual record.
  `StatusRail` still renders an uppercase tracked wordmark (v1 shell, P10).

## 2026-11-04: QA pass on P2, P3 and P4 (branches `feat/0.4.5-p3-baseline-schema`, `feat/0.4.5-p4-auto-eval`)

- **Method.** Each branch was verified on its own (not only the combined tree), an independent
  read-only reviewer audited the P3 and P4 diff against the kickoffs and `03 §4`/`§8`, and every
  finding was reproduced in code before it was fixed. Every code fix has a regression test that
  was **mutation-checked**: the old behaviour was put back and the test was confirmed to fail,
  then the fix was restored and confirmed byte-identical.
- **Found by testing the P3 branch alone (merge blocker).** `node scripts/stage-mcp.mjs` failed
  on P3 with `cannot update the lock file ... because --locked was passed`. The P3 commit's
  `Cargo.lock` had been written while P4's harness work shared the tree, so it listed five
  harness dependencies P3 does not use. It passed only on the combined tree, which is why the
  overnight run missed it. Fixed on P3 (`d68a6f5`); P4 re-adds its own edges (`cedfc73`).
- **Blocker (review, verified).** A Settings save could undo an API change made in the HTTP API
  panel and stop the listener: `api.enabled/port/bind_address` became typed settings that
  `set_settings` hot-applies, but the Settings draft never refreshed them and `set_api_config`
  never invalidated the `settings` query, so a stale `api_enabled=false` was written back. Fixed:
  the draft mirrors `api_*` like it already mirrored `app_*`, `set_api_config` invalidates
  `settings`, `set_settings` invalidates `apiStatus`. **Not run live:** the app has no data-dir
  override, so running it would open the real profile, which the run rules forbid. The behavioural
  proof is the maintainer's native check (enable the API in Integrations, change any other
  setting, confirm the listener stays up).
- **Should-fix, all fixed (`b431bc8`, `cd79f40`, `48a9705`, `c437297`):**
  - `ssv2c-rebuild` dropped five live internal markers. Losing `sessions.backfill_done_until`
    re-segments the carried history. Now carried, plus a drift-guard test that scans the shipped
    source for `settings` key constants and fails on any key the rebuild neither carries nor drops
    on purpose. `text.catalog_filter_version` is dropped on purpose: it watermarks
    `chrome_text_catalog`, which is not carried.
  - The allocator test could not tell copy from recompute (fixture high-water 7 = max frame id 7).
    Now 42; a recompute mutant fails with `source Some("42"), destination Some("7")`.
  - Store open decided fresh vs existing by `path.exists()`, so a first run that failed mid-baseline
    left an empty file refused forever, and the check raced a file appearing. It now decides by
    contents: zero schema objects is initialised, anything else takes the strict v14 refusal, a
    non-SQLite file is refused untouched. A simulated crashed first run now recovers to v14.
  - `advanced.toml` rewrite is atomic (temp file then rename).
  - The `#[ignore]`d e2e capture test still wrote `capture.diff_threshold` to the `settings`
    table, which the kernel no longer reads, so it silently ran at the default threshold. It now
    sets the value through `advanced.toml` and asserts it took effect (`cd79f40`). Run for real on
    this desktop: setup and capture work. The test then fails at `raw_text == content_text`, and
    **fails identically on unmodified `main` (`4bd120b`)**, so that failure predates the arc; it is
    recorded in `07`, not silenced.
  - Four P4 metrics did not match their printed definitions: `sequence` ties, `window_recall`
    expecting out-of-window frames, recall@10 never reaching sessions, and the seen-again probe
    varying with `--pairs`. Fixed, report text updated to match.
- **Accepted, not changed:** the reviewer's nits on vacuous older kernel tests, the rebuild's
  image check resolving paths against the copied DB's folder, and harness edge cases (empty
  terminal citations event, non-ASCII needles). Recorded in `07`; none changes a result
  tonight's fixture or the maintainer's run depends on.
- **Blocker regression test (`48a9705`).** The Settings mirror step moved into
  `ui/src/lib/settingsDraft.ts` and is covered by `ui/test/settingsDraft.test.mjs` (6 tests: the
  exact revert scenario, App toggles, preserved form edits, same reference when nothing changed,
  no mutation, the mirrored key set). Removing `api_enabled` from the mirrored set, which is the
  original bug, fails 3 of the 6.
- **Reviewed and kept as is:** `text.catalog_filter_version` is not carried. A missing watermark
  only makes the startup pass re-clean rows whose own `frame_text.filter_version` is below
  current; the rebuild copies that per-row column verbatim, so the pass finds no frames, writes the
  watermark, and changes no text.
- **P2 deliverables.** All four HTML files parse with zero structural errors and no duplicate ids,
  make no network request, and the three directions use no raw colour or px font size outside their
  token blocks. Contrast was **computed from the token values**, not read from the files' own
  comments, at two scopes: every rule that sets both `color` and `background` (worst 5.03:1), and
  every text token against every page-background token within each theme (worst 4.52:1, direction
  A light, faint text on the document background). Both clear WCAG AA 4.5:1 in every theme.
  Inherited combinations (a colour from an ancestor over a descendant's background) were not
  rendered; for those the evidence is each file's own documented audit.
- **Final gate, P4 tip (contains all P3 fixes), raw:**
  ```
  === npm ci                                                             exit=0
  === npm run lint                                                       exit=0
  === npm run build                                                      exit=0
  === npm test               tests 88 / pass 88 / fail 0                 exit=0
  === node scripts/stage-mcp.mjs                                         exit=0
  === cargo fmt --all -- --check                                         exit=0
  === cargo clippy --locked --workspace --all-targets -- -D warnings     exit=0
  === cargo build --locked --workspace                                   exit=0
  === cargo test --locked --workspace                                    exit=0
  === git diff --exit-code -- ui/src/bindings                            exit=0
  passed: 1241  failed: 0
  ```
  Run on the final P4 tip `c437297`, which contains every P3 change. 1241 Rust tests = the
  earlier 1233 plus 8 new regression tests; 88 UI tests = the earlier 82 plus 6 for the blocker.
  The P3 branch alone was also run green.

## 2026-11-03: 0.4.5 P4 automatic evaluation harness (branch `feat/0.4.5-p4-auto-eval`)

- **Scope (roadmap P4, kickoff `docs/kickoffs/P4.md`):** dev-only eval runner in `crates/harness`.
  Five self-generated families (last-seen, involvement, sequence, window recall, seen-again),
  unlabeled entity check, six metrics. **No hand-written question set, no labelling, no golden
  file** (0.4.5 D17). `D17` is enforced by construction: the only inputs are the fixture database
  and a seed.
- **Metrics defined in code and printed in the report:** evidence recall@10 over cited ids (the
  only evidence a client sees, `03 §7i.3`), a separate `/v1/search` retrieval recall@10 diagnostic
  that is never mixed in, citation validity (frame exists and captured in window; session exists
  and overlaps the window, NULL end treated as open), known-fact presence after case and
  whitespace normalization, latency p50/p95 by nearest rank, entity recovery and invention with
  `n/a (no entity surface)` when the graph is empty.
- **Fixture run (not the baseline):** 87 pairs from a 12 session, 36 frame fixture. `pairs.jsonl`,
  `runs.jsonl`, `report.md`, `report.json`. A fixture yields fewer pairs than real history, and the
  report prints a pair-shortfall note rather than inventing pairs.
- **Verification (branch tip, raw):**
  ```
  cargo fmt --all -- --check                                       exit=0
  cargo clippy -p harness --all-targets -- -D warnings              exit=0
  cargo test -p harness   146 passed / 1 / 3 / 6 / 6 / 3 / 3 passed, 0 failed
  ```
  Read-only proof: `PRAGMA query_only=1`, INSERT and CREATE TABLE fail, reads succeed, the file is
  byte identical afterwards, with a writable negative control on the same file. Determinism:
  same database and seed produce identical pairs; a different seed does not; editing a frame
  changes the derived pair.
- **Maintainer run (not performed by an agent):**
  `cargo run -p harness --bin harness -- eval --db <db> --api http://127.0.0.1:43210 --out harness-data/eval-baseline`
- **Not claimed:** the baseline report against real history (maintainer's run), and the P5/P6
  thresholds, which freeze from that report at Gate A.

## 2026-11-03: 0.4.5 P3 baseline schema and settings v2 (branch `feat/0.4.5-p3-baseline-schema`)

- **Scope (roadmap P3, kickoff `docs/kickoffs/P3.md`):** `crates/store` baseline v14 with the
  v1 to v13 chain deleted, the dev-only `crates/rebuild` tool (D3), settings v2 in
  `crates/traits`/`crates/kernel`/`src-tauri` (D6), and the regenerated bindings.
- **Baseline fidelity.** The chain was dumped before deletion: a throwaway test opened an
  in-memory database through the old migrations and wrote `sqlite_master` to `target/v13_dump.sql`,
  then a second throwaway harness rebuilt v13 from that dump and compared it with `create_baseline`
  table by table (`PRAGMA table_info`, `foreign_key_list`, `index_list`, index and trigger SQL).
  Result: `parity OK: 41 v13 objects preserved`, `missing (0)`, `added (19)` = the eleven new tables
  and eight indexes. 31 tables (27 plain plus 4 virtual), 22 indexes, 7 triggers.
- **Open refusal.** A database that is not v14 fails before any pragma runs, so a refused file is
  never header rewritten; the message names the found version and `ssv2c-rebuild`. Tests assert the
  file bytes and mtime are unchanged after a refusal, for a v13 and a v15 fixture.
- **Settings v2 counts (grep, not assertion):** 25 UI rows = 24 typed fields plus `api.token`;
  44 file-only = 43 `AdvancedConfig` knobs plus `app.tray_toast_done`; 9 deleted. Retired-key grep
  hits only the retired list, its doc comments, the retirement test, the rebuild tool's own copy of
  the retired list and fixtures, and two `uia` doc comments that explain the retirement.
- **Deleted-key and moved-key greps (raw):**
  ```
  grep -rn MIGRATIONS crates                                    -> 0 lines
  grep -rn ssv2c-rebuild crates/store/src                       -> 4 lines
  grep -rn advanced.toml crates                                 -> 28 lines
  capture_diff_threshold / sidecar_device / enrich_embed_text /
    overlay_max_results in crates/traits/src/ipc.rs             -> 0 each
  ```
- **Per-crate gates (raw, from the lanes' logs):**
  ```
  cargo fmt --all -- --check                                    exit=0
  cargo clippy -p store --all-targets -- -D warnings             exit=0
  cargo test -p store        171 passed (80 lib + 6 baseline + 65 store + others), 0 failed
  cargo test -p rebuild      4 + 5 passed, 0 failed
  cargo test -p traits       123 + 4 passed, 0 failed
  cargo test -p kernel       126 + 9 + 17 + 3 + 31 + 10 + 2 passed, 0 failed
  cargo test -p harness      146 + 1 + 3 + 6 + 6 + 3 + 3 passed, 0 failed
  ```
- **Full suite on the integrated tree (verbatim, one command per line):**
  ```
  === npm ci                                                   exit=0
  === npm run lint                                             exit=0
  === npm run build                                            exit=0
  === node scripts/stage-mcp.mjs                               exit=0
  === cargo fmt --all -- --check                               exit=0
  === cargo clippy --workspace --all-targets -- -D warnings     exit=0
  === cargo build --workspace                                  exit=0
  === cargo test --workspace                                   exit=0
  ```
  `cargo test --workspace` summary: **1233 passed, 0 failed**; the four `1 ignored` are the
  pre-existing `perf` and `seed_demo` gates. Full log kept at `%TEMP%\p3-full-verify.txt`.
- **UI build was red mid-phase and was repaired.** Removing 44 file-only and 9 deleted keys from
  the typed `Settings` broke `Settings.tsx`, `ScheduleControl.tsx` and `FlowOverlay.tsx` (157
  type errors). Repair: 40 controls removed from `Settings.tsx` (that is the D6 subtraction P7 will
  re-home by category), 3 from `ScheduleControl.tsx`, and the overlay now reads `overlay.max_results`
  from a new read-only `get_advanced_config` projection instead of a removed field. No hardcoded
  fallback. `npm test` 82 passed.
- **Not claimed:** the rebuild run against the maintainer's real database, the per-table counts
  before and after, and the twenty frame spot check. Those are the maintainer run in
  `docs/kickoffs/P3.md`; the agent evidence is the synthetic fixture test only.
- **Gaps recorded:** `06` #30 (the spec's §4 does not describe ten as-built details), #31 (the
  rebuild tool's home reconciled from `crates/harness` to `crates/rebuild`), #32 (carry set wider
  than the kickoff list), #33 (three §8 defaults disagree with the as-built code). `07` carries the
  product questions, including the `advanced.toml` reset for an existing profile.

## 2026-11-03: 0.4.5 P1 specs contract (branch `feat/0.4.5-p1-specs-contract`, specs-only)

- **Scope (roadmap `docs/0.4.5.md` P1):** zero code. D1–D18 normalized into the specs so a fresh
  agent can start any wave-A phase (P2/P3/P4) from `specs/` alone. Landing sites: `01 §2/§6/§7/§8`
  (single user, baseline schema, rebuild tool, local-only, next arc); `02 §5k` (arc), `§5d`/`§7`
  (audio not a priority); `03 §4` (baseline rule at `schema_version = 14`, 11 new tables, migration
  prose kept as history), `§7c`/`§7f.7` (4 read routes, 4 MCP tools), new `§7h`/`§7i`/`§7j` (work
  graph, Ask v2, insights), `§8` (settings v2 table, load rule), `§12` (schema failure mode), new
  `§13d` (gates + per-phase DoD); `04 §1/§2/§3/§4` (0.4.5 pointer, ownership, D2/D18 guardrails);
  `UI_REFERENCE §1/§2/§5` (placeholders filled at P2), `§3` (v2 IA), `§4` (12 state rows), `§7`,
  `§8` (v2 shell contract), `§10.7`; v1 identity archived verbatim to
  `specs/archive/UI_REFERENCE_v1_identity.md`; `07` (0.4.5 section, #75, #82, #104); `CLAUDE.md`
  hard rule; one-line amendments in `docs/ARCHITECTURE.md` and `docs/memory-api-direction-plan.md`.
- **Implemented by four disjoint writer lanes** (`03` core sections, `03 §8`, `UI_REFERENCE`, the
  other specs/docs), integrated by the parent, which also fixed survivors the lanes did not own:
  `03 §12` still stated forward-only migrations as the current rule, `02` closing status still said
  "no next arc", `UI_REFERENCE §7` still said "dark-only by design", `07` #82/#104 pointed forward
  at the superseded rules.
- **`03 §8` conversion:** 78 key rows — 25 `UI` (the closed list, each with a live consumer;
  `api.read_token` shares the `api.token` row), 44 `file-only` (live consumer, engine tuning), 9 `deleted` (hits only in the
  `RETIRED_SETTINGS_KEYS` sweep/tests/doc comments, or none). Two backticked tokens in the old list
  were not keys (`frames.capture_trigger`, `set_api_config.bind_address`) and stay in prose.
- **Gates 1–2 (raw output goes in the PR body):** scope — 13 tracked files modified plus the new
  archive file and `docs/0.4.5.md`, every changed path `*.md`, no `crates/`/`ui/`/`src-tauri/`
  path; UI rows `25`; tolerance grep over `03` empty (`§8b` byte-identical to `main`); `D1`–`D18`
  each cited ≥1 file; `CREATE (VIRTUAL) TABLE` count `29` with each of the 11 new names once; every
  surviving `forward-only` hit is historical or amended; `git diff --check` clean. No build/test
  run — no code changed.
- **Gate 3 cold reads:** P3 from `03 §4` alone — **yes** (typed columns, PKs/FKs/CHECKs/indexes for
  all 11 tables, `schema_version = 14`, refusal naming `ssv2c-rebuild`, no normative migration
  function); `06` #27 resolved by the maintainer. P2 from `UI_REFERENCE §1/§2/§5` alone —
  **yes** (each placeholder names who fills it, when, and what unblocks it; `§3`/`§4`/`§8` already
  authoritative). P5/P8 from `03 §7h` + `§7j` alone — **yes** (six entity kinds, linking rule,
  ≥0.7 confidence with 7 d → 30 d decay, override precedence, five insight types; `TBD` count `0`).
- **Resolved (`06` #27, maintainer 2026-11-03):** `api.read_token` shares the `api.token` UI row;
  `sidecar.device` added as `file-only`. Committed and pushed after the maintainer validated the
  content and named `nicolasestrem/screensearch` (0.4.5 D18).

## 2026-10-20 — Setup visibility (branch `feat/setup-progress-visibility`, `06` #26)

- **Scope (approved contract `02 §5j`, `03 §7g`):** additive `SetupStatus` snapshot +
  `setup_changed` event + `get_setup_status`; engine acquisition reports real per-attempt
  bytes and Checking/Downloading/Verifying/Installing; embeddings report Preparing → Done/
  Failed (indeterminate); model-lane downloads are mirrored from `model_download`;
  StatusRail `SetupIndicator` + Settings · Models `SetupDetails`. No schema, setting, network
  destination or downloader-policy change.
- **Implemented by three disjoint writer lanes** (kernel+Tauri, `download.rs`, UI), integrated
  and verified by the parent. All compilation and tests ran on tbx (Windows, Rust 1.95.0,
  Node 26.12.0 — documented Node is 22); nothing was compiled on the Linux coordinator.
- **Native gate at the pre-review tree** (`44398dd`): `npm ci` 0 · `npm run lint` 0 ·
  `npm run build` 0 · `cargo fmt --all -- --check` 0 · `cargo clippy --workspace
  --all-targets -- -D warnings` 0 · `cargo test --workspace` 0 (inference `117 passed;
  2 ignored`, kernel setup `8 passed`) · `git diff --exit-code -- ui/src/bindings` 0. The first
  UI test run failed (`actual: '12 MB', expected: '12.4 MB'`): the test assumed one decimal,
  the code follows the existing rail/ModelPanel rule (whole numbers from 10 up) — the test was
  corrected, not the code; rerun `tests 80 / pass 80 / fail 0`.
- **RED proofs (mutation on tbx):** making `newerSetupStatus` accept unconditionally failed
  exactly the four ordering tests (`pass 76 / fail 4`); removing the kernel identical-step
  guard failed `setup_identical_re_report_neither_bumps_revision_nor_broadcasts`
  (`assertion left == right failed`, exit 101). Both files restored and verified.
- **Independent review** (read-only): `clean_with_minor_findings`, no P0/P1. Fixed: P2 empty
  snapshot announced "Local AI setup complete" → now silent; P3 Verifying/Installing reset the
  attempt to 1 → carry the delivering attempt; P3 identical re-report bumped the revision → no
  bump, no broadcast (new test); P3 chip had no width cap → `max-w-80` + truncated step text.
- **Native gate at the post-review tree** (`a36504b`): lint 0 · UI tests 0 · build 0 · fmt 0 ·
  workspace clippy 0 · `cargo test -p kernel --test setup` `9 passed` · `cargo test -p
  inference --lib` `117 passed; 2 ignored` · `cargo test -p screensearch` 0 (`162 passed`) ·
  bindings clean.
- **Native interactive run (Nico, tbx, 2026-10-20, `npm run dev` of the `a36504b` tree, sidecar
  folder emptied):** reported "All good!" with four screenshots. Observed in them:
  (1) `SSV2C_LLAMA_RELEASE_URL` pointed at a missing asset → StatusRail chip **"Setup failed"**
  plus the existing readiness banner "Inference: Unavailable — llama-server unavailable:
  download …/b0/nope.zip returned HTTP 404 Not Found"; (2) a real first-run acquisition →
  chip "Preparing local AI · Installing e…" (step text truncated by the new width cap) beside
  the "Embeddings: Starting" / "Inference: Starting — resolving llama-server" banner; (3) the
  console log shows the anonymous API lookup **failing with HTTP 403 rate limit and the pinned
  fallback being used** (the #145 path, live); (4) Settings · Models afterwards: **Engine —
  Done, "Engine available"; Search model — Done, "Search model ready"**, no chip.
  Not captured as screenshots (covered only by Nico's "All good!" report): the failed row in
  Settings details with its next step, the determinate download percentage, and the Ctrl+R
  reload reconciliation. Polish noted: at the rail's width the active chip truncates the stage
  word ("Installing e…"); a shorter active label is a candidate follow-up, not a defect against
  `03 §7g`. Not pushed; no PR; no merge.
- **PR #146 review round 1 (Codex, `e0503e3`, 3 inline P2 threads; all valid, fixed):**
  (1) `PRRT_kwDOTAuCtM6uUbG1` — the snapshot query mounts in the child `StatusRail`, whose fetch
  can resolve before `useLiveEvents`' async `setup_changed` listener exists, so a terminal event
  in that gap was lost → the hook now invalidates the setup query once the listener is live (the
  revision guard makes the refetch safe). Timing inside the WebView; no node-runner test can
  reproduce it — verified by reading, lint/build. (2) `PRRT_kwDOTAuCtM6uUbG7` — Settings kept
  listing all-`done` rows forever, contrary to `UI_REFERENCE §4` ("details list nothing") → new
  pure `setupDetailsSteps` returns nothing once no operation is active or failed; test
  `setupDetailsSteps: zero presence once every operation is done…` proven RED on tbx (guard
  removed → `pass 80 / fail 1`, file restored). This also corrects the native-run observation
  above: the "Engine available / Search model ready" rows after setup were that defect, not the
  intended idle state. (3) `PRRT_kwDOTAuCtM6uUbHC` — terminal engine `Done`/`Failed` reset
  `attempt` to 1 → the last download attempt is shared (`Arc<AtomicU32>`) with the terminal
  steps. Gate on tbx at the fixed tree: lint 0 · UI `tests 81 / pass 81 / fail 0` · build 0 ·
  fmt 0 · workspace clippy `-D warnings` 0 · `cargo test -p screensearch` `162 passed` ·
  kernel setup `9 passed` · bindings clean.
- **PR #146 review round 2 (Codex, `b500176`, auto-triggered on push; 3 inline P2, all valid,
  fixed):** (1) `PRRT_kwDOTAuCtM6uUowo` — a vision/answer download showed the legacy
  `model_download` rail chip *and* the setup chip; `UI_REFERENCE §4` wants one summarized
  indicator → the legacy rail chip is removed (the kernel already mirrors both lanes into the
  setup snapshot with real bytes; ModelPanel keeps its own `model_download` view). (2)
  `PRRT_kwDOTAuCtM6uUowu` — a failed row showed no cause outside the collapsed diagnostic →
  pure `setupFailureCause` renders a visible cause line derived only from the error text (HTTP
  status, digest mismatch, else the innermost chain segment; never an inferred "offline");
  test proven RED on tbx (`pass 81 / fail 1`, file restored). (3) `PRRT_kwDOTAuCtM6uUowy` — a
  lagged kernel broadcast could drop the terminal `SetupChanged` → `forward_events` re-emits
  `kernel.setup_status()` on `RecvError::Lagged` (the UI revision guard keeps it safe). Gate on
  tbx: lint 0 · UI `tests 82 / pass 82 / fail 0` · build 0 · fmt 0 · workspace clippy
  `-D warnings` 0 · `cargo test -p screensearch` `162 passed` · kernel setup `9 passed` ·
  bindings clean. Independent read-only review of the round-2 diff: no defects. Merge
  authorized by Nico for #146 once checks on the pushed head pass.
- **Auto-triggered Codex review on `1291529` (1 inline P2, valid, fixed):**
  `PRRT_kwDOTAuCtM6uU02n` — the vision-model failure copy said "Use Load", but Load only loads
  the answer model, so it pointed at an action that cannot retry the vision download → each
  lane now names its own existing action ("Load answer model" / "Start vision tagging"); `03
  §7g` and the `UI_REFERENCE §4` row corrected to match (no new retry mechanism). Copy-only
  change (UI + specs only — no Rust path changed since `1291529`, whose Windows CI passed). tbx:
  lint 0 · UI `tests 82 / pass 82 / fail 0` · build 0; the old "Use Load" copy fails the updated
  test (`pass 81 / fail 1`, file restored).
- **Auto-triggered Codex review on `fa186f5` (1 inline P2, valid, fixed — final round, Nico:
  "merge after addressing this round"):** `PRRT_kwDOTAuCtM6uVC5e` — after a model download
  failed, switching that lane to a tier already on disk resolves without the downloader, so the
  lane's `failed` step stayed until restart while the model served fine → the kernel clears a
  lane's `failed` setup step when the sidecar reports that lane `Ready` (other lane and non-ready
  transitions untouched). Test `setup_lane_failure_clears_when_that_lane_model_is_ready`, proven
  RED on tbx with the call removed (exit 101, file restored). tbx: UI build 0 · fmt 0 · workspace
  clippy `-D warnings` 0 · `cargo test -p kernel` all green (setup `10 passed`); tbx restored to
  clean `main`.

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
- **Review round 1 (PR #145):** Codex reviewed `fd35571` with **no findings** (0 inline
  threads, 0 review submissions); `@claude review` returned six non-blocking suggestions as a
  PR conversation comment. Dispositions: (1) broad fallback trigger — intentional and now
  covered by `release_lookup_without_a_matching_asset_uses_direct_fallback`; (2) pin staleness
  — already `07` #95; (3) `verify_sha256` wording — reworded; (4) env-var race in the ignored
  test — now takes `ENV_LOCK` like the other env-reading test; (5) 32 MB double copy —
  pre-existing and unchanged (would need a `Bytes`-based signature), left as is; (6) offline
  start — already `07` #94. Only (1), (3) and (4) changed code/docs; no product contract moved.
- **Not run / open:** the live installed app was never restarted and its data untouched (the
  single-instance plugin means an isolated first run would require stopping it); no full
  `cargo test --workspace` / UI chain was run locally on this branch (CI's Windows job passed
  on `fd35571`), and no merge/release was made.

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
