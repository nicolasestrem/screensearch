# Good morning: 0.4.6 stabilisation run

Unattended run, 2026-11-10, branch `fix/0.4.6-stabilisation` from `origin/main` at `93a7219`.
Local commits only: nothing was pushed, merged, tagged, released, or opened as a pull request, and
`main` is untouched. Nine queue items are complete or recorded; two of them are blocked on a product
decision and say so.

## Queue, one line each

| # | Item | Status | Commit |
|---|---|---|---|
| run | Run state opened | done | `c62ddad` |
| 1 | `06` #150 launch panic (`UpdaterState` ordering) | **fixed and verified** | `ca2d7ed` |
| 2 | `06` #116 Ask turn whose provider never answers | **blocked** on a spec decision, no code | `9afb7b2` |
| 3 | `TODO-4` `throttleStatus` invalidation | **fixed and verified** | `d193624` |
| 4 | `TODO-6` settings defaults over IPC | **implemented and verified** | `57bbdae` |
| 5 | `06` #143 `get_insights` removal | done, **no code change** (importer exists) | `a9ba52d` |
| 6 | `06` #137 / #138 `Expander` and v1 icons | done, **no code change** (importers exist) | `070a07d` |
| 7 | `06` #42 stale `jobs.kind` comment | done, comment only | `06e0f3f` |
| 8 | `07` #91 monitor hot-unplug | **blocked** on a spec decision, no code | `2a00469` |
| 9 | v0.4.6 version bump and release notes | done, gate green at 0.4.6 | `3eb680c` |
| 10 | This file | done | the commit that adds it |

## What actually changed in the product

1. **The launch crash.** `UpdaterState` is registered on the Tauri builder through the single seam
   `manage_pre_window_state`, before any window or webview exists. The setup-closure registration is
   gone. `src-tauri/tests/launch_order.rs` reproduces the production panic on the old ordering and
   passes on the new one.
2. **Settings > Advanced.** A settings save now invalidates `throttleStatus` through a shared
   `SETTINGS_SAVE_INVALIDATIONS` list, and `ThrottleReadout` no longer takes a local `enabled` prop.
3. **Settings reset markers.** `get_settings_defaults` serves `Settings::default()` over IPC; the
   reset marker and its "changed" flag read it, with the old `SETTINGS_DEFAULTS` table demoted to an
   in-flight fallback. A contract test compares the real Rust payload against every registry row.
4. **Documentation.** The `jobs.kind` comment lists all eight job kinds in both copies.

## Proof, briefly

- Item 1: the regression fails with `state() called before manage() for
  screensearch_lib::update::UpdaterState` at `tauri-2.11.3/src/lib.rs:734` before the fix and passes
  after. Twenty consecutive cold launches of an isolated dev profile came up with zero panics and
  zero early exits, each observed ten seconds past readiness. The pre-fix dev control also passed
  8/8, so the live series does not discriminate on its own; the failing-first regression does. No
  release-binary launch test and no updater download/install is claimed.
- Item 3: the new UI test fails on the old three-key set with `throttleStatus is missing`; the lead
  reproduced that independently by deleting the key.
- Item 4: the contract test failed first on the missing command, and a deliberate
  `storage_max_width = 1920` failed it with `storage.max_width ... Rust 1920, registry 0`.
- Every item that changed code ran the full ordered chain (`ui_ci`, `ui_lint`, `ui_build`,
  `ui_test`, `stage_mcp`, `cargo_fmt`, `cargo_clippy`, `cargo_build`, `cargo_test`) under Node
  v22.23.4 with every step `EXIT=0`; item 1's final run reported 87 suites / 1653 passed and items 4
  and 9 reported 88 suites / 1654 passed, with 174 UI node tests.
- Records: `specs/05_BUILD_REVIEW.md` has one section per item with the raw blocks, `specs/06` row
  statuses are current, `specs/07` carries both blocked questions, `specs/08` and `CHANGELOG.md` are
  updated, and `specs/TODO.md` marks TODO-6 closed.

## Questions that need your answer before those two items can move

Both are recorded in `specs/07_KNOWN_GAPS.md`; neither has code behind it.

1. **`06` #116, an Ask turn whose provider never answers.** `03 §7i` defines no turn deadline, `§7c`
   says the 30 s header deadline is not a body deadline, and `§8` forbids hardcoded thresholds plus
   caps the Settings UI at 26 keys. With no answer weights, the turn enters the unbounded first-use
   download before any delta exists. (a) May an Ask start that download, or must it fail before the
   turn until the model is loaded? (b) If a first-delta deadline is required, what value? (c) Does
   the clock start at turn creation (covering a legitimate multi-GB download) or at model readiness?
   And where does the threshold live, given the closed key set?
2. **`07` #91, a monitor unplugged mid-session.** `03` prescribes nothing for a topology change, and
   `UI_REFERENCE.md:234` asks only that an unplugged monitor "degrades honestly". What should happen:
   restart capture, drop the monitor from the session, surface a state, or hold silently? A test seam
   needs that answer before anything can be written.

## Environment facts this run hit

- Gate runs need `CARGO_TARGET_DIR` outside `Documents` on this machine. Test executables built under
  the repository's own `target` cannot create temporary directories (Windows code 5), which fails
  `crates/api` reproducibly; the same tests pass from an external target. Full evidence is in
  `specs/07`. No test or timeout was changed.
- CI pins Node 22 and this host defaults to Node 26, so a checksum-verified portable Node v22.23.4
  lives at `C:/Users/nicol/screensearch-evidence/0.4.6/node22/node-v22.23.4-win-x64/` and every final
  gate ran under it, with a probe asserting both the invoked interpreter and a spawned child report
  `v22.23.4`.

## Your live checks (nothing here was forced by me)

1. **Launch the installed 0.4.6 build about ten times and confirm no panic.** The crash was
   intermittent, so volume matters:

   ```powershell
   $exe = (Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run").ScreenSearch
   1..10 | ForEach-Object {
     Start-Process $exe; Start-Sleep -Seconds 12
     Get-Process screensearch -ErrorAction SilentlyContinue | Stop-Process -Force
   }
   $log = Get-ChildItem "$env:APPDATA\app.screensearchv2c.desktop\logs\screensearch.log.*" |
     Sort-Object LastWriteTime | Select-Object -Last 1
   Select-String -Path $log.FullName -Pattern "state\(\) called before manage", "panicked at"
   ```

   No output from the last line means no panic was logged.
2. **Settings > Advanced: flip the enrichment throttle.** Save, and watch the readout switch to the
   off line at once and return to the measured line (or the honest "waiting for the first pressure
   sample") when you turn it back on.
3. **Settings: change a row, then reset it.** The "changed" marker should appear and clear, and the
   value should return to the default the app reports. This is the first release where the marker
   comes from the app rather than the interface's own copy.
4. **Confirm the version reads 0.4.6** in the app's about/status surface.

## Pushing

Nothing is pushed. To publish the branch and open the pull request:

```powershell
cd C:\Users\nicol\Documents\GitHub\screensearch
git push -u origin fix/0.4.6-stabilisation
gh pr create --base main --head fix/0.4.6-stabilisation --title "0.4.6 stabilisation" --body "Fixes the 0.4.5 launch panic (06 #150) and lands the throttle invalidation (TODO-4), IPC settings defaults (TODO-6) and the jobs.kind doc fix. Two items are blocked on spec decisions and are recorded in specs/07, not implemented: 06 #116 and 07 #91. Local gate: full ordered chain green under Node 22, 88 suites / 1654 tests. No tag or release."
```

Do not merge it until you have read `specs/05` and decided the two questions above. Re-running the
gate yourself:

```powershell
cd C:\Users\nicol\Documents\GitHub\screensearch
powershell -NoProfile -File C:\Users\nicol\screensearch-evidence\0.4.6\gate.ps1 -Log C:\Users\nicol\screensearch-evidence\0.4.6\gate-manual.log
```
