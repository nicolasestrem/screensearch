# 07 - Known Gaps

The 0.4.5 arc shipped: P1 through P10, landed as PRs #150-#166. Its full record is archived at
`specs/archive/07_KNOWN_GAPS.v0.4.5.md`, including every phase decision and grant; that archive is
frozen.

On 2026-11-09 every carried-forward item was re-checked against the v0.4.5 tree (`9c45ae8`, #172) and the
git history. Resolved and superseded items are recorded, one line of evidence each, in the archive's
`## 2026-11-09: Carried-forward items closed in the post-0.4.5 specs cleanup` section; the text of
each item at release time stays in the archive. What follows is that carried-forward backlog, still
open, grouped once. Row numbers (`#123`) are kept so other specs can cite them.

Resolved rows from earlier arcs are in `specs/archive/07_KNOWN_GAPS.v0.1.0.md`, `.v0.3.0.md`,
`.v0.3.1.md`, `.v0.3.2.md`, `.v0.4.1.md`, `.v0.4.2.md` and `.v0.4.5.md` (ids preserved). Where another
spec cites a row that is not below, read it there.

The deferrals the 0.4.5 phases recorded against themselves (P2-P10 and the release QA, in the archive's
phase sections) are a separate ledger and are not repeated here. Re-checked on 2026-11-09, 57 of them
were still open, each with an owner in the archive; the ones still worth acting on include the P10
screenshot set held out of the PR because some cells carry live desktop text, the retained `Expander`
primitive and v1 icon set, the still-registered `get_insights` aggregate, and the promotion surface
that only Settings reaches. Promote any of them here when it enters an approved arc.

## Current arc

Nothing yet: the 0.4.5 arc shipped and is archived.

## Open, carried forward

### Maintainer decisions

- **#76 Custom GGUF "bring-your-own-model" path.** Owner: maintainer. Status: deferred product item;
  no implementation exists (no custom-model-path surface in `crates/inference` or the Settings UI).
  Decide whether the tinkerer escape hatch is built.
- **#77 Proactive nudges / notifications.** Owner: maintainer. Status: reaffirmed non-goal in
  `02 §7` (pull-based only); a push mechanism would be a separate opt-in design conversation.
- **#79 API write scopes beyond marks.** Owner: maintainer. Status: deferred — the API stays
  read-only except `POST /v1/marks` and marks resolve (`crates/api/src/routes.rs:169-171,226`).
- **Weekly synthesis citing a revision rather than the base run** (archive line 1271). Owner:
  maintainer. Status: undecided since G3/G5; the engine still consumes base + revisions
  (`weekly_consumed_with_dates`, `crates/kernel/src/reports.rs:997-1000`), so one day can be narrated
  twice.
- **Weeks that closed during a disabled interval.** Owner: maintainer. Status: current behaviour —
  a re-enable neither backfills nor lists them as missed (`src-tauri/src/memory_weekly.rs:191-194`);
  keep that, or record them as missed at disable time.
- **Failed-week edge cases (a)–(d).** Owner: maintainer. Status: implemented as readings and pinned by
  tests (`src-tauri/src/memory_weekly.rs:307-313,737-781`) but never ratified: manual runs are not
  schedule coverage; a failed regeneration leaves the earlier result standing; BLOCKED is not
  recorded as missed; a missed record stays as history after a later success.
- **Existing profile loses customised file-only values.** Owner: maintainer. Status: still undecided.
  `advanced.toml` is the only source for the 44 file-only keys, and the loader reads the file only
  (`crates/kernel/src/settings.rs:364-491`): either seed the file from the settings KV at first boot,
  or accept the reset and say so in the release notes. Was due before Gate A, which never closed.
- **`06` #32 rebuild carry set and #33 stale `03 §8` defaults.** Owner: maintainer. Status: both still
  open in `06_PATCH_PLAN.md`; the carry set ships in `crates/rebuild/src/lib.rs`, and the `03 §8` table
  still reads `storage.max_width` 1280 / `retention_days` "0 = keep" / `chrome_suppress_min_seen` 12
  against code defaults 0 / 30 / 4 (`03 §8` table).
- **Density: no settings key, and `UI_REFERENCE §8` still calls it a Settings choice.** Owner:
  maintainer. Status: unchanged at `UI_REFERENCE.md:396`; the P7 ruling is comfortable-only and no
  density key exists (`ui/src/app/theme.ts:6-8`); `06` #29 tracks the key decision.
- **#92 UIA circuit-breaker thresholds are named constants, not settings.** Owner: maintainer.
  Status: standing user-decided exception (`crates/uia/src/breaker.rs:22-25`); promote to settings only
  if real use shows the 3-bad-walks / 30-minute back-off is wrong.
- **#107 User-editable taxonomy override file.** Owner: maintainer. Status: deferred by 0.4.0 D6; the
  taxonomy is still the bundled `crates/sessions/taxonomy.toml` (v3).
- **#108 Dedicated Sessions NavRail route.** Owner: maintainer. Status: standing not-planned
  deferral (D13, `UI_REFERENCE.md:178-180`); the rail is Deck · Ask · Work · Recall · Insights ·
  Settings (`ui/src/app/nav.ts:26-28`).
- **Usage-signals reading procedure, no script.** Owner: maintainer. Status: the procedure stands as
  written (release-asset counts, traffic clones/views against a personal baseline); the optional
  `scripts/` helper was never built.

### Acceptance gates not yet passed

- **Code-signing certificate.** Owner: maintainer. Status: the NSIS installer stays unsigned;
  `03 §11b` is the authority and the vendor path (SignPath / Azure Trusted Signing / Certum) is still
  undecided. Only the updater `.sig` is signed today (`.github/workflows/release.yml`).
- **Gate A was never closed as a tag.** Owner: maintainer. Status: `v0.4.3`/`v0.4.4` do not exist, so
  two items that waited on Gate A are unresolved: the P6 optional per-stage candidate cap
  (archive line 1110 — counts are recorded per turn on the terminal `done` event, no numeric gate
  stated) and the P4 eval metric conventions (`crates/harness/src/eval/report.rs:344-346` — printed in
  the report, not ratified). Confirm, or state that neither is wanted.
- **G6 live Telegram test send.** Owner: maintainer. Status: needs an explicitly approved recipient;
  no live send has been made or authorized.
- **Separate-machine HTTP authentication and interactive Settings acceptance (configurable bind).**
  Owner: maintainer. Status: only same-host runs exist; the local credential was seeded rather than
  created through the UI, and no cross-machine authentication has completed
  (`specs/archive/05_BUILD_REVIEW.v0.4.2.md:848-855`).
- **Before-network-release evidence set.** Owner: maintainer. Status: still unrecorded from a
  cross-machine run — real separate-machine HTTP authentication, local MCP, restart, disable/enable,
  Ask cancellation and inference bind evidence (archive line 1297).
- **Before-automation-acceptance set.** Owner: maintainer. Status: still needs a user-selected test
  folder, an installed model and an explicitly approved Telegram destination; no fabricated receipts
  or private-history defaults.
- **Product acceptance on diverse setups, not one personal score.** Owner: maintainer. Status:
  standing criterion; no diverse-setups acceptance is recorded, and implementation verification stays
  in `05`.
- **Real installed-model report quality.** Owner: maintainer. Status: never accepted on a real
  installed model with real history; engineering tests are not product acceptance.
- **Slow / unavailable-folder native acceptance.** Owner: maintainer. Status: durable intents are
  implemented, but no native run has checked the behaviour (OS file operations can finish after async
  cancellation).
- **#58 Live multi-DPI / secondary-monitor `target_rect` acceptance.** Owner: maintainer. Status:
  status unverified: the repo holds no record that the manual multi-monitor, mixed-DPI run was made
  before background/system suppression is relied on.
- **External-agent memory API gates.** Owner: maintainer. Status: native Settings interaction
  (read-token controls) was blocked in SSH Session 0 (no WebView2 renderer) and is not passed;
  separate-machine MCP → Windows app traversal timed out under Windows' default inbound block with no
  firewall/port-proxy workaround. Remote report generation/retry/delivery stay excluded by scope.
- **Website source absent; external privacy-copy review.** Owner: maintainer. Status: the website is
  not in this repository, so the review remains open.
- **Sparse-content sampling caveat.** Owner: maintainer. Status: standing — sparse content can need a
  larger frame budget and must never be read as proof that a window had no data.

### Engineering hardening and known defects

- **#91 Monitor hot-unplug mid-session stalls capture silently.** Owner: agent. Status: still open and
  out of scope for 0.4.5 by maintainer decision; no display-topology handling exists (no
  `WM_DISPLAYCHANGE` handling in `crates/capture`); sessions make the stall visible but silent data
  loss remains possible (`UI_REFERENCE.md:234`).
- **e2e capture test fails on `main`.** Owner: agent. Status: `src-tauri/tests/e2e_capture.rs` is
  `#[ignore]`d, and on a real desktop it fails at `assert_eq!(frame.raw_text, frame.content_text)`
  (`:108-113`); it fails identically on unmodified `main`. Decide whether the chrome filter makes the
  two texts differ by design, then correct the assertion or the code. Not to be deleted before that is
  known.
- **Review nits never changed.** Owner: agent (next owner of each file). Status: three kernel settings
  tests are vacuous (`crates/kernel/tests/settings.rs:24,523,1120`); `ssv2c-rebuild` resolves image
  paths against the copied DB's folder (`crates/rebuild/src/lib.rs:546-556`); the harness ignores an
  empty terminal `citations` event (`crates/harness/src/eval/client.rs:113-120`) and cannot match
  non-ASCII reference text, because SQLite's `lower()` is ASCII-only
  (`crates/harness/src/eval/facts.rs:298-300`).
- **`advanced.toml` provenance is honest only until the first rewrite.** Owner: agent. Status: the
  one-shot rewrite still writes every effective value back (`crates/kernel/src/settings.rs:510-517`),
  so from the next start every key reads "from the file"; keep Settings · Advanced read-only and do not
  promise per-key override flags until the rewrite keeps only user-set keys.
- **#124 A lane's setup failure is cleared by lane, not by model.** Owner: agent. Status: valid narrow
  race needing overlapping tier switches on one lane (`crates/kernel/src/lib.rs:1585`); fix is to
  remember the failed model id per lane and clear only on a matching `SidecarStatus.model`, plus a
  late-`Ready` kernel test.
- **#96 The API-selected sidecar asset is downloaded unverified.** Owner: agent. Status: `GithubAsset`
  parses only `name` and `browser_download_url` (`crates/inference/src/download.rs:181-185`), so only the
  pinned fallback carries a digest; defence-in-depth hardening for the next inference change.
- **#95 UIA spawn vs concurrent settings-save residual race.** Owner: agent. Status: accepted,
  not human-reachable two-save micro-race; self-heals on the next save (`src-tauri/src/lib.rs:2755-2756,
  2807-2817`). Revisit only if it proves reachable.
- **#123 Setup chip truncates its stage text at rail width.** Owner: agent. Status: unchanged
  (`ui/src/components/domain/SetupIndicator.tsx:44-46,56`); the width cap is now a deliberate
  stable-width rule per `UI_REFERENCE §8`, so a fix must work inside that contract.
- **#94 No in-app retry after a failed first bootstrap.** Owner: maintainer. Status: engine and
  embeddings failure copy still says restart (`ui/src/lib/setup.ts:100-101`), and "Try again" only
  refetches the snapshot; re-running a failed engine step without restart is unbuilt.
- **TelegramSnapshot has two parse contracts.** Owner: agent. Status: the dispatch loop still
  deserializes per receipt and hard-errors an unparseable payload (`src-tauri/src/memory.rs:2295-2301`)
  while `redirect_stale_telegram_deliveries` guards the same parse with `continue`
  (`src-tauri/src/memory.rs:708-712`); share one contract.
- **Agent readings the maintainer may override.** Owner: agent. Status: all still accurate in code —
  a missing note has no canonical identity, so case-only spellings get different fallback keys
  (`src-tauri/src/promotion.rs:152-163`); a legacy spelling-keyed ownership row becomes unreachable once
  the output folder is respelled; a week with raw evidence but no saved dailies is refused with
  `GENERATION_FAILED`; weekly citation syntax is unspecified, so a bare whole number counts as a
  citation (`crates/kernel/src/reports.rs:1063-1069`); a consumed day rewritten by a linked revision
  renders as a plain-text date in `Index.md`; daily and weekly zones are independent but the weekly
  engine matches saved dailies by exact zone string, so different selections can leave days uncovered
  (the UI warns).
- **#82 `jobs.kind` has no `CHECK` constraint.** Owner: agent. Status: optional hardening, never built;
  adding it is its own baseline edit + `schema_version` bump (`03 §4` DDL comment,
  `crates/store/src/schema.rs:223-226`).
- **#89 v1-API scope residuals.** Owner: agent. Status: all three still deferred — export is JSON-only
  (`crates/api/src/export.rs:177-195`), there is no per-request rate limiting (only the 64-connection cap
  and 30 s header deadline), and the export cursor still scans by id from 0.
- **#109 `browser_url` is dormant in production capture.** Owner: agent. Status: still
  `browser_url: None` (`crates/kernel/src/capture_loop.rs:159-160`); the browser-domain refinements and
  the browser-ai domain match stay dormant until a UIA address-bar read lands, which also gates the
  sharpening half of #116(b).
- **#106 NavRail ghost/duplication (WebView2 compositor).** Owner: agent. Status: upstream artifact;
  the CSS mitigation is shipped and must be kept (`ui/src/components/shell/NavRail.tsx:106-113`,
  `ui/src/components/shell/AppShell.tsx:33-36`). Flip only on a runtime fix or a durable field
  confirmation.
- **#112 / #116 / #117 Accepted session-model limitations.** Owner: agent. Status: (a) same-tool
  instances still fold into one track; (b) `browser-ai` is a single taxonomy id, gated on #109; (c)
  merged-handoff interior splits, spinner ambiguity and thin-overlap evidence are unchanged;
  foreground-only diff-gated capture cannot represent concurrent AI agents. Re-verify on every newly
  labelled day.
- **#113 Anchorless-focus density gate.** Owner: agent. Status: live at 30 fph
  (`crates/traits/src/sessions.rs:19`, `crates/sessions/src/segmenter.rs:440-448`); the owed
  sparse-legitimate false-suppression check on a fresh labelled day is still outstanding.
- **#115 Ground truth from the capture, not recall; non-work fullscreen is uncapturable.** Owner:
  agent. Status: digest-first labelling is standard practice; a non-work category still needs a
  taxonomy entry plus fullscreen-surviving capture.
- **Development dependency advisories.** Owner: maintainer. Status: no recorded re-audit since
  2026-09-09 and the lockfile still pins the four named packages (`ui/package-lock.json`); one re-audit
  under a separate approval is the decisive check (js-yaml 4.3.2 may already be the fixed version).
- **`braces` 3.0.3 build-tool advisory.** Owner: maintainer. Status: upstream has no patched release
  (GHSA-vfj7-8cjw-p6xm, via `chokidar` → `micromatch` → `fast-glob` → `tailwindcss`,
  `ui/package-lock.json:2218`); keep globs fixed in `ui/tailwind.config.js`, reassess on an upstream
  patch or before accepting untrusted build patterns.
- **Regression suite is not in CI.** Owner: maintainer. Status: `ui/test/` holds 26 Node tests but CI
  runs UI lint and build only, so the suite is unit-level protection that no pipeline enforces.

- **#46 residual orphaned-writer risk on the hf-hub single-stream fallback.** Owner: agent. Status:
  mostly resolved — the chunked path awaits its chunks inside the watchdog task and is abort-safe
  (`crates/inference/src/download.rs:1391-1414`); the range-less-server fallback still uses hf-hub
  with bounded mitigations. A cancellable owned chunker for that path remains a larger deferred change.
- **#72 60-frame host-RAM retest on a current bundled build** (`06` #15). Owner: agent. Status: the
  pinned fallback is `llama-b11739` (`crates/inference/src/download.rs:121-125`), newer than the
  leaking build 9842, but no retest is recorded; the normal path selects the newest release asset, so
  the retest must name the build it validated. Local mitigation (sidecar recycle valve) stays active.

### Standalone deliverable (Your-Universe, not product code)

`Your-Universe/` is a separate Three.js viewer built against the same export shape; it touches no
crate, schema, IPC or UI. Its gate failures were documented in `05` and `08` as recorded here, and the
measurements behind them are in `Your-Universe/ITERATIONS.md`.

- **fps bench gate.** Owner: Your-Universe owner. Status: the gate did not pass in cycle 21 — a
  recurring 1.0-1.1 s tail frame, and `p1LowFps` regressed from 169.5 to 41.7 with the crowd and asset
  pass, against a floor of 55 (desktop). Not waived; the deliverable ships with the regression recorded.
- **Draw-call line.** Owner: Your-Universe owner. Status: the header still says under 80 calls, while
  the committed tree re-measured over HTTP at 1600x900, hour 17, awake reports 101 with the asset fleet,
  95 with `?post=0` and 55 with `?noassets=1`. No cycle restated the call budget, so the header and the
  measurement disagree until one of them is corrected.
- **Triangle peak.** Owner: Your-Universe owner. Status: bench-time peaks of 393k-398k triangles were
  measured against the 390k line (the committed snapshot `4c7fbea`, with the modelled fleet on,
  re-measures 388,884 of 400,000, so the shipped assembly is under the 400k cap while the recorded
  peaks were not).
- **Violet dream watch item.** Owner: Your-Universe owner. Status: the dream night sky sits at 5.5-5.6
  percent of frame against the watch target; recorded, not tuned.
- **No human visual pass.** Owner: maintainer. Status: the build has only ever been judged by pixel
  measurement plus written verdicts; nobody has looked at the final frames on a real display.
- **Green wave stays on after its session ends** (PR #173 review). Owner: Your-Universe owner.
  Status: valid, fix not yet shipped. `agents.mjs` gates the wave on `longestSessionBefore(t)`, which
  returns the longest session that *started* by `t` rather than the one open at `t`; over the shipped
  sample that leaves the wave on and the 1.28 avenue multiplier engaged for 472 of 892 sampled minutes
  (52.9 percent) with no session open, against a comment that says "still open at tData". The fix is
  one swap: `activeSessionAt(t)` is already on the model, so the gate becomes `!!active && active.q >= 0`
  and `safeLongest` loses its only caller (the `safeLongest(tData)` in the green-wave gate, line 4969 at
  the merge commit), leaving it dead and due for deletion in the same
  change. Expiry: this lands with the owner loop's snapshot commit of its uncommitted `src/` work;
  **drop this row when that commit carries it**. It cannot be committed on its own because `index.html`
  is generated by `tools/assemble.mjs` from the working-tree `src/`, so a fix committed from a clean
  tree would be reverted by the loop's next commit of files that never contained it.
- **Dream cards survive an export reload** (PR #173 review). Owner: Your-Universe owner. Status: valid,
  fix not yet shipped. `dream.mjs` owns the `.dream-card` nodes and their click handlers in an internal
  `cards` array, and `hideCards()` is not part of the returned API, so `install()` in `main.mjs` can
  only dispose the Three.js group: loading another export while hypotheses are showing leaves the old
  cards in `#cards`, bound to the discarded dream, and the next waking exceeds the two-card limit. The
  fix is two lines there or three here: add `clearCards: hideCards` to `dream.mjs`'s return object, then
  call `dream.clearCards()` in `install()` next to `disposeGroup(dream ? dream.group : null)` and before
  `dream = null`. Expiry: same trigger and same reason as the row above.
- **The startup sample replaces an export imported during the asset wait** (PR #173 review, round
  three). Owner: Your-Universe owner. Status: valid, proved from the committed call order, fix not
  shipped. `main.mjs` starts with `wireInput(); boot();`, so a drop is handled while `boot()`'s own
  `Promise.all([fetchSample(), loadAssets()])` is still waiting, up to the 8000 ms pack deadline.
  `loadExport()` installs the user's district, and the callback then calls `install(next)` with the
  sample regardless. Because `install()` begins by disposing the world, agent and dream groups and
  nulling them, that second call does not overwrite a field: it throws away the district the user just
  imported, and their history is gone without a message. The fix is a module-level
  `let userModel = null`, set to `next` in `loadExport()` after `usable()` passes, then one guard in
  the `boot()` callback around **both** the sample `install(next)` and the portrait-phone
  `rig.setTier('block', false)`, which was chosen for the sample's framing and must not be applied over
  the user's own district. The block after the guard (the declutter interval, the animation loop, the
  cache registration and the bench timeout) must still run, so the guard wraps those two statements
  rather than returning early. Expiry: same trigger as the two rows above, the loop's snapshot commit.
- **The replaced world's shadow render target is never disposed** (PR #173 review, round four). Owner:
  Your-Universe owner. Status: valid, proved from ownership, fix not shipped. `world.mjs` adds the sun
  to the group `install()` disposes (`group.add(sun)`), and `disposeGroup` traverses that group, but it
  returns for any object with no material, and a `DirectionalLight` has none: `sun.shadow.map` survives
  the only thing that referenced it, and removing the light from the scene does not release the GPU
  allocation. Importing another export therefore costs one shadow target on desktop with shadows on,
  accumulating per import until the page unloads. Two shapes, both already available: a light branch in
  `disposeGroup` (`if (object.isLight && object.shadow && object.shadow.map) { object.shadow.map.dispose();
  object.shadow.map = null; }`) placed before the material return, which also covers any other light
  that ever carries a map, or one call in `install()` before `disposeGroup(world ? world.group : null)`
  through `world.lights.setShadowMapSize(0)`, which already disposes and nulls the map
  (line 10353 at the merge commit). Prefer the first: it keeps the rule that a disposed group releases everything it
  owns, instead of requiring each caller to remember the lights. Expiry: same trigger as the three rows
  above, the loop's snapshot commit.


