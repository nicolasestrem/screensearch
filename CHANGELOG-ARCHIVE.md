# Changelog — archived releases

> Released history split out of `CHANGELOG.md`. The `[0.1.0]` section moved 2026-06-27; the
> `[0.2.0]`–`[0.2.2]` sections moved on the post-v0.2.2 archival sweep (2026-06-30); the
> `[0.3.0]` section moved on the v0.3.0 release sweep (2026-07-04, PR9); the `[0.3.1]`
> section moved on the v0.3.1 release sweep (2026-07-05, 0.3.1 PR4); the `[0.3.2]` section
> moved on the v0.3.2 release sweep (2026-07-06, 0.3.2 PR6); the `[0.3.3]` section moved on
> the v0.3.3 release sweep (2026-07-07, folded into 0.4.0 PR1); the `[0.4.0]` section moved on
> the v0.4.0 release sweep (2026-07-11, 0.4.0 PR7); the `[0.4.1]` section moved on the v0.4.1
> release sweep (2026-10-15). Only
> `[Unreleased]` remains in `CHANGELOG.md`.


## [0.4.1] - 2026-10-15

Highlights: PR [#122](https://github.com/nicolasestrem/screensearch-v2c/pull/122) (opt-in network API,
configurable bind address) and PR [#123](https://github.com/nicolasestrem/screensearch-v2c/pull/123)
(durable memory reports, date/range selection, Telegram digests; schema 11 → 12). The release
workflow now publishes a full release automatically.

### Added
- Manual memory report date/range selection, including today's in-progress evidence, with
  inclusive dates in the selected timezone and separate owned report files/index links.
- Payload-free memory run and delivery diagnostics record status/coverage without report text,
  credentials, recipients or output paths.
- Durable daily memory reports with exact evidence provenance, separate file/delivery receipts,
  retry versus regenerate actions, and a forward-only schema 11 → 12 migration.
- Default-off schedules with an explicit IANA timezone, DST handling and bounded restart catch-up;
  owned Markdown notes and an index preserve user edits and report file conflicts.
- Default-off Telegram text digests with Windows Credential Manager storage, bounded rejection
  retries, per-chunk receipts and explicit review of uncertain delivery.
- Approved, separately reviewed opt-in Telegram digest contract: Windows credential storage,
  saved-report rendering, durable per-chunk receipts and explicit ambiguous-delivery handling.
- Approved contract for durable daily memory reports and user-selected local Markdown output;
  implementation and native acceptance are tracked separately from the network API build.
- README "Inspirations & prior art" section crediting screenpipe, Rewind.ai, Rem, and OpenRecall.
- Cleanup regression evidence and a reproducible local unsigned Windows installer
  build record in `specs/05_BUILD_REVIEW.md`.

### Changed
- Repository agent guidance now has one source of truth in `CLAUDE.md`; `AGENTS.md`
  directs agents to read it, removing duplicated instructions without Windows symlinks.
- README showcases the unreleased memory reports, date/range selection, Obsidian notes,
  optional Telegram digests and configurable API binding, with setup links and release status.
- Coverage reports with captured evidence reject blank model output as generation failure,
  preserving genuine empty-window NO_DATA behavior and prompt-based no-match results.
- Memory status explains why NO_DATA reports do not send Telegram digests.
- Restored the original privacy statement in `specs/01_PROJECT_CONTEXT.md` at the
  maintainer's request; API and delivery details remain in their feature documentation.
- The opt-in HTTP API accepts a configurable IPv4/IPv6 literal bind address. Missing addresses
  default to `127.0.0.1`, including existing enabled installations, so upgrades do not broaden
  access automatically. Address/port edits apply immediately; bearer authentication, plain HTTP,
  Windows exclusive binding and MCP/inference loopback restrictions remain.
- Daily schedule input uses hour/minute precision (`HH:mm`), with no seconds field.
- New memory settings and delivery paths use forward slashes; legacy file-ownership paths
  remain recognized. Windows-only file operations and the database schema are unchanged.
- The memory/reporting execution plan now targets configurable product behavior and validation
  across supported user environments. Native acceptance remains separate from build verification.

### Changed (release pipeline)
- `release.yml` publishes the GitHub Release itself (full release, `generate_release_notes`, body from
  `docs/release-notes/<tag>.md`) instead of leaving a draft, so the signed `latest.json` is
  immediately visible to the updater.

### Removed
- Unused UI scaffold, search-icon alias, local-day helper, and mark-note mutation hook;
  active UI and mark-note commands are unchanged.
- Unused `thiserror` workspace declaration and the OCR crate's unused direct `tracing`
  dependency; no dependency versions changed.

## [0.4.0] - 2026-07-11

**The sessions arc (P8), and the first release existing installs receive automatically.**
ScreenSearch now groups your captured frames into sessions: a meeting, a Claude Code run, a
stretch of focused work. Sessions are additive and pull-based, so nothing about capture, search,
Ask, reports, marks, the overlay, or where-was-i changes, and there are no notifications, nudges,
or scores. Recognition ships from a real-data-tuned taxonomy (Claude Code, Codex, Claude desktop,
browser AI, and five meeting apps), and sessions are reachable in the Timeline and through the
read-only local API and MCP, which closes the AI-tool recall request (#88). This is the first
release delivered automatically to 0.3.2-and-later installs: the update path (check, background
download, user-initiated restart, and a one-time schema upgrade that adds sessions over your
existing history) was verified end to end before release. The installer itself is not yet
Authenticode-signed, so Windows SmartScreen may warn on a manual download.

### Added
- Sessions are now reachable from the local API and its MCP wrapper, read-only. Scripts and AI
  clients can list the sessions overlapping a time range (filtered by kind or tool), open one session
  to see its representative exchanges and its cached summary if one exists, and ask a grounded
  question scoped to a single session so the answer only cites that session's own frames. Three
  matching MCP tools (list sessions, get session, ask session) join the existing set. The API never
  generates a summary on read and never writes, and questions without a session behave exactly as
  before.
- Sessions are now visible throughout the existing recall flow: Timeline shows accessible,
  lane-packed session bands in exactly four stable rows; sessions needing lane five or later are
  summarized by a neutral keyboard control that focuses the existing range presets. A code-split
  drill-in shows lazy summaries, representative frames, exchanges, and cited Recaps; Moment and Deck
  link the same context back to its session; and Advanced Settings exposes the two existing
  session-pass thresholds. No new navigation item or frame-level behavior was added.
- The in-app sessions surface now has typed core commands to list overlapping sessions, load a
  bounded drill-in with representative frames and extracted exchanges, connect Moment and
  where-was-i frames back to their session, lazily cache session title/summary intelligence, and
  generate a cancellable Recap through the existing coverage-first report engine. Session Recaps
  are scoped by exact frame ownership, so overlapping tracks cannot leak citations.
- The sessions engine now groups captured frames in the background into stable focus, meeting, and
  recognized AI-tool sessions. It supports overlapping tools while keeping each frame owned by one
  session, incrementally updates the recent tail, and resumably backfills existing history without
  blocking capture. Recognition ships from the real-data-tuned v3 taxonomy (Claude Code, Codex,
  Claude desktop, browser AI, and five meeting identities); explicit chat exchanges are extracted
  best-effort from filtered content text, and session title/summary generation is lazy and cached for
  the forthcoming in-app surface. No new UI/API commands land in this stage.
- Groundwork for sessions (no visible changes yet). The database gains the tables that will let
  ScreenSearch group your frames into sessions: a meeting, a Claude Code run, a stretch of focused
  work. The upgrade runs on first launch, adds structure only, and creates no sessions on its own;
  the actual grouping arrives in a later stage. It changes nothing about today's features. Search,
  Ask, reports, marks, the overlay, and where-was-i are proven byte-for-byte identical before and
  after the upgrade on a real, populated database, and the upgrade completed in about 150 ms over a
  3,000-frame database in testing.

### Changed
- PR #104 review follow-up resolves all four unresolved inline findings (two were the same store
  issue): session-content preflight now stops inside SQLite with an indexed `EXISTS` query while
  preserving Rust `str::trim`'s exact Unicode-whitespace semantics; Timeline error and empty states
  reserve the same fixed four session lanes as loading and populated states; and the session
  scheduler now emits quiet `sessions_changed` invalidation only for semantic changes, using delta
  ownership/artifact writes so an unchanged pass is also a physical no-op.
- PR5 review hardening makes Timeline session-band packing use measured token-sized hit targets,
  holds loading/error/empty/populated at exactly four lanes to prevent layout shift, and summarizes
  dense overflow without nested/horizontal scrolling. Mounted session queries now refetch after a
  successful scheduler pass through a quiet typed event. Recap cancellation stops backend work on
  cancel, route change, or unmount. Native WebView2 acceptance passed against the live schema-11
  database; the dataset had no open session, so that visual variant remains transparently unobserved
  rather than claimed.
- Review hardening now includes the final frame when sampling session intelligence, parses taxonomy
  match strings once instead of allocating per frame, treats frozen boundary timestamps as
  inclusive, refuses to match merely touching mutable sessions, and resumes a crash-interrupted
  historical row without creating a duplicate session.
- Session backfill now safely resumes when its original historical cutoff lands inside a continuous
  activity track, reconciles delayed work before immutable frozen tails, and preserves exact frame
  ownership across absorbed short excursions and frozen boundaries.
- Specs only (no app changes): the 0.4.0 "sessions" arc contract is now written into the project
  specs, so the next stages of work can be built straight from them. Sessions group your captured
  frames into meaningful stretches (a meeting, a Claude Code run, an afternoon in one repo), and
  are additive: nothing about today's capture, search, Ask, reports, marks, or the overlay changes.
- Dev-only work (no app changes): the sessions design now handles **running several AI tools at
  once** (for example Claude Code and Codex working in parallel) as **overlapping** sessions
  rather than collapsing them into one. This was validated against real captured days in the
  dev-only harness; the app's data model is unaffected. A known limit is recorded honestly: the
  screen capture only sees the foreground window, so a tool running in the background is under-
  recorded, which caps how completely concurrent work can be reconstructed.

### Documentation
- Documentation refresh and repository-exposure scrub. Demo screenshots that had been captured from
  a live install (and therefore showed real personal screen content) were removed pending re-shoot
  against synthetic data. Absolute user paths and personal-activity notes were replaced with
  placeholders in the build-loop logs, test fixtures were changed to synthetic user/host values, and
  the crate author email now uses a noreply address. `docs/ARCHITECTURE.md`, `README.md`,
  `CLAUDE.md`, `AGENTS.md`, and `docs/API.md` were updated for v0.3.3 and the 0.4.0 sessions arc (15
  crates, schema v11). The pre-0.4.0 manual acceptance checklists moved to
  `specs/archive/TESTING.pre-0.4.0.md`, and three obsolete design notes under `docs/superpowers/`
  were removed.
- Restored the README screenshots, now rendered against **synthetic seed data** (invented frames,
  sessions, and text — no personal content): Deck, Timeline (with session bands), Recall, Insights, and
  Moment. A dev-only, CI-ignored seeder (`crates/store/tests/seed_demo.rs`) populates an isolated
  schema-11 database through the public store API, and `docs/SCREENSHOTS.md` documents the process.

## [0.3.3] — 2026-07-07

### Fixed
- Browsers no longer freeze during capture. The text reader (UI Automation) could hang Chromium
  and Electron windows (Chrome, Edge, Slack, Discord, VS Code, Claude Desktop, and similar) by
  walking their accessibility tree, occasionally leaving a window "Not Responding". These windows
  are now read via OCR instead, which never freezes them; their on-screen content stays fully
  captured and searchable. Native apps are unaffected and keep the higher-fidelity reader.

## [0.3.2] — 2026-07-06

**The product-shell mini-arc (P7.2) — and the auto-updater's genesis release.** From this
release on, ScreenSearch updates itself: it checks a signed manifest on GitHub Releases at
launch, downloads in the background, and installs only when you choose to restart — no modal,
no nag, never an auto-restart. v0.3.1-and-earlier users download this installer manually **one
final time**; every release after it arrives automatically. The same arc gives the app a
product shell: a system tray with a passive capture-state icon and quick actions
(close-to-tray on by default, run-at-startup off by default), a hardened one-scroll-context
layout, and a two-tier Settings page (Essentials always visible, Advanced collapsed into seven
groups). Zero database changes — a v0.3.1 database opens as-is.

### Fixed - 0.3.2 PR6: daily/weekly report filenames say which they are (#89)
A downloaded daily or weekly report is now named `screensearch-report-daily-YYYY-MM-DD-HHmm.md` /
`screensearch-report-weekly-...` instead of the undifferentiated date-only stem, so a Downloads
folder full of reports stays tellable-apart. Custom-range reports keep the existing
`screensearch-report-YYYY-MM-DD-HHmm.md` name; the local-time stamp and the same-minute `-2`/`-3`
collision rule are unchanged.

### Changed - 0.3.2 PR5: Settings two-tier IA
The Settings wall (15 flat sections, ~60 fields) is now two tiers. **Essentials** stay always
visible: Capture (interval, monitors, event-driven master toggle), Hotkeys, Privacy, Models (tier
pickers, thinking, and the engine's load/unload controls, now in one place), Storage, App, and Data
(local API + export). **Advanced** collapses into seven expander groups (Capture tuning, Text
source / UIA, Enrichment & scheduling, Performance throttle, Text filtering, Reports & retrieval,
Inference engine) that are closed by default and remember their open state for the session. Every
section now opens with one plain-language sentence saying what it is for. The Hotkeys section gains
a live **conflict warning**: setting the Flow overlay and Mark-this-moment shortcuts to the same
combination shows an inline warning the moment the chords match and clears it when they differ.
Two dead settings are gone: **JPEG quality** (inert since captures moved to lossless WebP;
its own hint said "has no effect today") and **Run UIA on click and scroll** (the click/scroll
triggers it opted into were removed in 0.3.0; for the rare install that had switched it on, it also
used to skip the short "don't read the screen mid-typing" pause on timer captures - that side effect
retires with it, and setting **Suppress during input** to 0 under Advanced restores read-during-input) -
an existing config that still contains either key loads
cleanly; the stale entries are dropped once with a log line. No setting was renamed and none
changed meaning; no database change. Keyboard and screen-reader support: each Advanced group is a
real disclosure button (Enter/Space toggles, state announced), and the page keeps the one-scroll-
context shell contract.

### Fixed - 0.3.2 PR4: shell layout hardening
The app shell now holds one scroll context per route. Recall's search results, the Ask answer's
"Frames checked" strip, the report source frames, and a moment's "Around this moment" filmstrip no
longer have their own inner scrollbars: each grows or wraps inline and the page scrolls as a single
column, with the scrollbar at the content-pane edge. No screen shows a horizontal scrollbar at any
supported size (1280x720 through ultrawide, 100 to 150 percent scaling): reasoning and answer text
wraps, and a moment with very wide recognized text (a captured table or terminal) no longer pushes the
context panel off-screen. Wide recognized text keeps its columns and scrolls sideways within its own
block (like a code block) instead of wrapping into an unreadable stack. Loading no longer jumps the
layout: the status-rail chips have stable widths, panel headers reserve their height, and the Deck,
Insights, and Settings loading skeletons match the shape of the real content (Insights even shows its
live header while data loads). The intermittent left-rail "ghost" glitch (nav items faintly
duplicated at the wrong height) is a known display-driver compositing quirk in the WebView2 runtime,
not an app bug; the rail is now painted on its own isolated layer to avoid it, and the issue is
tracked for a runtime fix. Structural only, no visual redesign; no settings or database changes.

### Added — 0.3.2 PR3: system tray + quick actions (#56/#57)
ScreenSearch now lives in the system tray. A tray icon shows live capture state at a glance
(capturing / paused / capture error, as a colored status dot + tooltip) — the passive "app running
reminder" of #56, with **no notifications, nudges, or counting badges** of any kind. Its menu is
Open ScreenSearch · Pause/Resume capture · Load/Unload answer model · Start/Stop vision tagging ·
Check for updates · Quit, each acting through the same commands the app already uses. **Closing the
main window now keeps ScreenSearch running in the tray** (capture continues); a one-time,
non-shaming toast explains this the first time you reopen the window, and a new **Settings → App**
toggle turns it off (window close then quits cleanly). A second **Run at startup** toggle (default
off) registers launch-at-login. The same **Load/Unload answer model** and **Start/Stop vision
tagging** quick actions also join the left-rail quick menu and the command palette, completing #57 —
"Start vision tagging" tags the untagged backlog, "Stop" cancels the pending vision jobs (a running
one finishes). Left-click on the tray icon, a second launch, or the tray's **Open** all restore a
tray-hidden window (the existing single-instance behavior). Quit from the tray shuts down cleanly
via the existing Job-Object lifecycle (`03 §7d`). No DB schema change. The tray reuses the sister
app's product-shell *patterns* (atomic pause toggle with rollback, register-before-persist for
startup, teardown-safe state access) — patterns, not code; its pause/resume notifications are
deliberately excluded (D4).

### Fixed — 0.3.2 PR3 review follow-up
Four correctness fixes from the PR #92 automated review: (1) the command palette now offers only
the contextually valid half of each lifecycle toggle (never "Load answer model" while it is already
loaded, nor "Stop vision tagging" with nothing running), mirroring the left-rail quick menu;
(2) the **Settings → App** toggles (Run at startup, Keep running in the tray) no longer silently
revert when you later Save an unrelated Settings field — their live values are mirrored into the
form draft; (3) the tray now seeds its **Start/Stop vision tagging** label from the durable job
queue at startup, so a restart with a leftover backlog opens on "Stop" (and can cancel it) instead
of "Start"; (4) if persisting settings fails after the launch-at-login registration was changed,
that OS registration is now rolled back, so run-at-startup can never drift from the stored setting.

### Added — 0.3.2 PR2: auto-update (#69)
ScreenSearch can now update itself. It checks for a new version on launch (and on demand from a
quiet **Check for updates** control in the left rail and the **Settings → App** section), downloads
a signed update in the background, and installs it **only when you choose to restart** — no modal,
no nag, no auto-restart. When an update is waiting, a quiet dot appears in the left rail; when there
is none, there is nothing to see. Built on `tauri-plugin-updater` with a **minisign-signed**
`latest.json` manifest published on GitHub Releases: a tampered, unsigned, or wrong-key update is
rejected before it can install (`03 §11b`). A new tag-triggered release workflow
(`.github/workflows/release.yml`) builds, signs, and drafts each release with the installer, its
signature, and the manifest; `scripts/make-latest-json.mjs` generates the manifest and refuses to
emit one for an unsigned build. **This release itself is still a manual download** — auto-update
delivers every release *after* it (0.4.0 will be the first to arrive automatically). Note: the
minisign updater signature is **not** Windows code signing (Authenticode) — that remains a separate
open item, so SmartScreen still warns on the manual installer.

### Docs — 0.3.2 PR1: specs contract (specs-only; no code / schema / UI)
The 0.3.2 roadmap (`docs/0.3.2.md` — "P7.2: product shell mini-arc", lifecycle + interface) is
normalized into the specs so PR2 through PR5 are implementable from the specs alone. The contract
locks in: the PR order (PR1 specs, then Rust lane PR2 auto-update #69 and PR3 systray #56 + quick
actions #57, parallel to UI lane PR4 shell-layout hardening (reproduce-first), then PR5 Settings
two-tier IA, then PR6 audit + tag `v0.3.2`); auto-update mechanics (`tauri-plugin-updater`,
minisign-signed GitHub-Releases manifest, passive pull-based UX, `03 §11b`); tray + close-to-tray +
single-instance lifecycle (`03 §7d`); two new settings keys (`app.close_to_tray` on,
`app.run_at_startup` off) and two dead-setting retirements (JPEG quality, `uia_run_on_interactive`)
with config-load tolerance (`03 §8`, D8); the shell layout contract (one scroll context per route,
no nested or horizontal scrollbars, no layout shift on load, `UI_REFERENCE §8`, D9); and the
Settings, App, tray, and updater surfaces (`UI_REFERENCE §3`/`§4`). Hard mini-arc constraints
recorded: zero DB schema migrations (D10), presentation-first restructure (D7), no visual redesign
(D12). Deferrals recorded in known-gaps: #88 folded into the 0.4.0 sessions arc (#102), settings
search (#103), visual-refresh possibility (#104), plus the updater key-custody manual step (release
blocker). This change touches only specs and docs.

## [0.3.1] — 2026-07-05

**The post-0.3.0 triage patch.** One regression fix is the reason this release exists: vision
tagging had slowed to roughly a third of its pre-0.3.0 rate after the switch to native-resolution
WebP storage (#64). Release-build profiling traced ~97 % of the slowdown to the *larger image
handed to the vision model* — not the WebP encode, which actually got cheaper — and capping the
model request back at 1280 px restored throughput to 102 % of the pre-WebP baseline. The rest is
small recall polish (#59, #65, the version-link half of #57) and two post-0.3.0 defect fixes that
ride along (the UIA client-lifecycle hang fix and the Flow overlay's new `Ctrl+Alt+Z` default).
**Auto-update (#69) is not in this release** — it is scheduled for 0.3.2 — so this is one more
manual download. Zero schema changes: a v0.3.0 database opens as-is (`schema_version` 10), and
the patch adds no new settings surface.

### Added
- **Recall reports now save with a dated filename and carry a footer (#65).** Downloading a
  report writes `screensearch-report-YYYY-MM-DD-HHmm.md` to your Downloads folder (local time;
  a second save in the same minute becomes `…-2.md`, `…-3.md`, never overwriting the first),
  and a toast shows the saved path. Every report — on screen and in the saved/copied file —
  now ends with a footer line stating the app version, the model used, the dates covered, the
  filters (report kind and any Custom focus prompt), and the coverage counts, so an exported
  report is self-describing.
- **The left nav rail shows the app version, linking to the project on GitHub (#57).** A quiet
  `v<version>` line at the bottom of the rail opens
  https://github.com/nicolasestrem/screensearch-v2c in your default browser.

### Changed
- **A Moment's recognized text no longer has its own scrollbar (#59).** Long OCR/UIA text (a
  full terminal, say) now grows with the page instead of being trapped in a small scroll box —
  one scroll for the whole Moment.
- **Flow overlay default hotkey is now `Ctrl+Alt+Z`** (was `Ctrl+Alt+Space`, which collided
  with Claude Desktop's global quick-entry shortcut). Existing installs still on the old
  default are migrated once on load; a chord you deliberately chose is left untouched. The
  migration is a genuine one-shot (latched by a stored marker), so if you *want*
  `Ctrl+Alt+Space` you can set it back in Settings and it now sticks across restarts instead of
  being re-migrated. A hotkey that fails to register (e.g. another app already owns it) is
  surfaced in Settings, not swallowed.

### Fixed
- **Quitting the app no longer keeps capturing while it shuts down (#84).** The exit handler now
  stops the capture loop *first of all*, before the local API finishes draining any open request
  and before it winds down the throttle, vision scheduler, and workers, so no new screenshots
  are captured or persisted once you close the app, even with a vision tag or model download in
  flight.
- **Links inside report and answer text now open in your browser.** External links cited by a
  report or a grounded answer previously did nothing when clicked (a Tauri limitation with
  plain new-tab links); they now open in your default browser, like the version link.
- **Vision tagging is fast again — throughput restored to the pre-WebP baseline (#64).**
  The 0.3.0 switch to native-resolution WebP storage silently grew the image sent to the
  vision model from 1280×536 (the old stored size) to a 1568 px cap, and release-build
  profiling showed the model doing ~2.4× the work per frame accounted for ~97 % of the
  slowdown — the WebP encode/decode itself measured in the tens of milliseconds (the encode
  is actually *cheaper* than the old resize-to-1280+JPEG). The tag request is now capped at
  1280 px again (matching the pre-WebP request exactly), restoring ~90 tags/min on the
  reference setup (from ~33/min). Stored captures are untouched — still native-resolution
  lossless WebP; only the transient image handed to the vision model is smaller. Profiling
  record: `specs/05_BUILD_REVIEW.md` Pass 3, `specs/06_PATCH_PLAN.md` #22.
- **UI Automation no longer leaves Chromium/Electron apps hung.** The UIA text source keeps
  an accessibility client connected, which flips apps like Chrome, Edge, Codex, and Claude
  Desktop into accessibility mode; previously that client was never released, so those apps
  could stay slow or unresponsive **even after you disabled capture**, and no restart of
  ScreenSearch's capture cleared it. Now:
  - Disabling **Use UI Automation text**, changing any UIA setting, or **stopping capture**
    actually disconnects the client, so the affected apps leave accessibility mode.
  - A **per-app circuit breaker** backs UIA off to OCR for 30 minutes after an app's tree
    walk repeatedly runs over budget or times out, so a heavy app isn't re-walked every frame.
  - A walk that blows its hard timeout is now **cancelled** instead of running to completion
    against the struggling app.
  - **Every UIA setting now takes effect immediately** on save (budget, node caps, control
    view, input-suppression) — no app restart, matching the "Applies now" hints. (A settings
    save that raced the very first client spawn could previously bake the pre-save budget into
    the new client; the client now reads the live config at spawn time, closing that window.)
  - **Recovery for an already-hung app:** disable UI Automation text (or stop capture) — which
    now truly disconnects — then restart the affected browser/Electron app to clear its sticky
    accessibility mode.

### Audited (0.3.1 PR4)
Every decision D1–D9 of `docs/0.3.1.md` was verified landed against the shipped tree — specs
contract and code, evidence-first, each check independently re-verified by an adversarial second
pass (none refuted). `specs/07_KNOWN_GAPS.md` carries every §2 deferral (#96–#99: auto-update →
0.3.2 hard-sequenced before 0.4.0; systray + quick actions → 0.3.2 under the pull-based /
non-shaming principle; #54 folded into the 0.4.0 sessions arc). Zero schema changes since v0.3.0
(`crates/store` untouched; `schema_version` still 10). The opener-plugin capability flagged for
this audit was reviewed and accepted (`http(s)://*` `open-url` only — no path/shell scope;
user-initiated clicks land in the OS browser, never the WebView). Full verification suite green
on the release tree (fmt · clippy `-D warnings` · build · test workspace · UI lint+build ·
bindings guard) and the NSIS installer builds at `0.3.1`. The audit's completeness pass also
surfaced **issue #84** ("Bug when quiting the app" — quit could keep persisting screenshots
behind an in-flight vision call/download), filed after the 0.3.1 disposition table froze, and a
PR4-review Codex P1 pinned the exact cause: the app-exit handler never called `stop_capture()`.
**Fixed in PR4:** the exit handler now stops capture first of all, before the local-API graceful
shutdown and the throttle/vision/worker drain, so no new frames land during quit; the full suite
was re-run green with the fix in.
Recorded and archived as `07` #101. Audit record: `specs/05_BUILD_REVIEW.md` Pass 5 (archived to
`specs/archive/05_BUILD_REVIEW.v0.3.1.md`).

### Docs — 0.3.1 patch specs contract (PR1, specs-only; no code / schema / UI)
The 0.3.1 roadmap (`docs/0.3.1.md` — "P7.1: post-0.3.0 triage", a regression-fix + polish patch)
is normalized into the specs so the later PRs are implementable from the specs alone. **This
change touches only specs and docs.** The contract locks in: the PR order (PR1 specs → PR2 the
#64 vision-throughput regression, profile-first with a stop condition and a fixed fix-preference
order → PR3 polish: #59 Moment text grows inline with no nested scrollbar, #65 dated report
filenames (`screensearch-report-YYYY-MM-DD-HHmm.md`, local time) + a report footer stating app
version/model/time span/filters, and the #57-partial version link — which lands in the **NavRail
footer** and opens the GitHub repo → PR4 audit + tag `v0.3.1`), and the hard patch constraint (no
new subsystems, no schema migrations, no new settings surface). Deferrals are recorded in
known-gaps: **#69 auto-update → 0.3.2, hard-sequenced before 0.4.0 ships**; #56 systray + the
#57 quick actions → the 0.3.2 lifecycle mini-arc, bound by the pull-based / non-shaming reminders
principle; #54 closed and folded into the 0.4.0 sessions arc.

## [0.3.0] — 2026-07-04

**The surface-reduction + flow-recall + local-API release.** The removals lead, deliberately —
each deletes a decision you no longer have to make: the **click / scroll-stop / clipboard /
typing-pause capture triggers** are gone (the first two were the only users of a system-wide
low-level mouse hook; the clipboard listener was a privacy-optics liability; typing-pause was
redundant with idle), the **Beta model tier** is gone (the one non-Apache license and unproven
architecture in the registry), and the flag-off **image-embedding lane** is gone (a second vector
table and model download that nothing exercised). What's added reuses what already exists: the
**Flow overlay** (`Ctrl+Alt+Space`), **where-was-i + mark-this-moment**, and an opt-in
**localhost HTTP API** with JSON export and a bundled **MCP server**. Every acceptance line was
verified end-to-end on a real Windows desktop in the PR9 integration audit (`specs/05`).

### Removed — Event-capture triggers trimmed to foreground + idle (0.3.0 PR2)
Opt-in event-driven capture now offers **two** triggers instead of six: **foreground/app-switch**
and **idle**. Four triggers are **gone**:
- **Capture on click** and **Capture when scrolling stops** — these were the only users of a
  *system-wide low-level mouse hook* (`WH_MOUSE_LL`), the one input-latency/invasiveness risk the
  0.2.0 design had deliberately avoided; removing them deletes that hook and a whole `unsafe` code
  path.
- **Capture on clipboard change** — a clipboard listener is a privacy-optics liability in an app
  built for the privacy-conscious, and app-switch already fires at nearly the same moments (people
  switch windows around copy/paste). Contents were never read; now nothing listens at all.
- **Capture on typing pause** — redundant with **idle** (both derive from the same idle-time poll,
  differing only in threshold).

Everything that carried the value stays: foreground, idle, the fallback interval, the debounce, and
the min-interval rate ceiling. Event mode remains **opt-in, default off**; timer capture is
unchanged. **No database change** — screenshots you captured before this update that were tagged
Click / Scroll stop / Clipboard change / Typing pause **still show that label** in the Moment view;
new captures simply never use those tags again. If your settings still held the removed toggles, they
are **dropped cleanly on the next launch** (logged once, no error). Live-verified: the retired keys
drop on load (one log line, none on relaunch); the app boots clean; the foreground hook thread
starts/stops cleanly 50× on a real desktop; the v8 schema still accepts a legacy `click` frame.

### Removed — Beta model tier retired; Default / Quality only (0.3.0 PR3)
Each model lane (Vision and Answer) now offers **two** tiers instead of three: **Default** and
**Quality**. The **Beta** tier is **gone** on both lanes, which removes the two models behind it —
the vision `Qwen3.5-9B-VLM` and the answer `NVIDIA-Nemotron-3-Nano-4B`. Nemotron was the only
non-Apache-licensed model (NVIDIA OML) and the only unproven hybrid Mamba-Transformer architecture
in the set, so the remaining line-up is **uniformly Apache-2.0 and vanilla-arch** — a smaller,
better-tested matrix and a cleaner licensing story.

If you had **Beta** selected for a lane, it now loads as **Quality** automatically — mapped once on
the next launch, logged, and saved (no error, no reset to Default). Any Beta model files already
downloaded are **left on disk**; you can remove them from the existing model-management surface if
you want the space back. Live-verified on a real desktop: a profile persisted with `beta` for both
lanes boots straight to Quality (one log line per lane, none on relaunch), the change is persisted
to the database, and the app runs on the remapped tiers with inference attached.

### Removed — Image-embedding lane (0.3.0 PR4)
ScreenSearch no longer has an optional **image-embedding** lane. It was dark-launched and off by
default (the `enrich.image_embeddings` "Embed images" toggle), so almost nobody turned it on — and a
flag-off feature is pure carrying cost: a second on-disk vector table, a second model download
(nomic-embed-vision-v1.5), and code that quietly rots because nothing exercises it. Text embeddings
plus vision tags already cover semantic reach, so the lane is **gone**. (Git remembers it if it is
ever wanted back.)

Your data is safe. On first launch after this update the database migrates itself forward (schema
v8 → v9) and **drops only the derived image vectors** — your screenshots, recognised text, and text
embeddings are never touched, and the image vectors were re-derivable from stored frames in any case.
Any queued image-embedding jobs are cleared. If your settings still held the **Embed images** toggle,
it is dropped cleanly on the next launch (logged once, no error), and the Settings → Enrichment panel
now shows just **Embed OCR text**. Search behaviour is unchanged — the image lane was never fused into
hybrid search, verified by identical result rankings across a 10,000-frame fixture before and after.

### Added — Flow overlay (0.3.0 PR5)
ScreenSearch now has a global-hotkey **Flow overlay** for quick recall without leaving the app you
are working in. Press `Ctrl+Alt+Space` to open a second, pre-created Tauri window over the foreground
monitor, type to search, press `Tab` or prefix the query with `?` to switch into Ask, and press
`Enter` to open the selected Moment in the main Command Deck. `Esc` hides the overlay.

The overlay is treated as a privacy boundary: it is hidden by default, capture-protected at the
window level, skipped from the taskbar, and covered by the same own-process capture gate used for the
main window so it does not appear in ScreenSearch's own history. Settings exposes the summon hotkey
and top-N result count (`overlay.max_results`, clamped to `1..=50`). A hotkey registration conflict
is loud: the app emits a warning toast and Settings shows the failed chord instead of silently doing
nothing. Exclusive-fullscreen applications may still suppress global overlays; that limitation is
documented in the manual acceptance checklist. *PR9 audit measurements (warm, debug build): overlay
visible 6–9 ms from the hotkey, input ready 17–23 ms — against a 150 ms budget; and the
capture-protection is strong enough that the overlay is invisible even to external screen-capture
tools, not just to ScreenSearch's own history.*

### Added — "Where was I?" + Mark this moment (0.3.0 PR6)
Two pull-based recall features for picking work back up. **Where was I?** answers "what was I doing
before this detour?" — open the Flow overlay with an empty query (or look at the Deck) and, when it can,
it offers the last context you stayed in for a while before your current detour: the app, the window,
and when you were last there, one click from reopening that Moment. It never nags — if there's nothing
worth resuming, it says so honestly.

**Mark this moment** captures the current screen the instant you press `Ctrl+Alt+M` — even on a static
screen that hasn't changed — and flags it as an intention to come back to. A quiet toast confirms
("Marked ✓") **without stealing focus**, so you keep typing; click it to add an optional one-line note.
Your open intentions live in a new **Intentions** strip on the Deck (newest first, with a thumbnail or
text reconstruction and its age) where you can open, resolve, or dismiss them. There are no badge
counts anywhere — nothing to shame you into acting.

Both hotkeys are configurable in Settings alongside the overlay hotkey, with the same loud
registration-conflict warning. The where-was-i sensitivity ("how long a context must persist to count")
is a setting too. If capture is off, the mark hotkey tells you plainly instead of silently doing
nothing, and a mark taken in an excluded app is refused with a reason. Existing databases migrate
forward (schema v9 → v10) by adding a `marks` table; nothing else is touched, and a marked frame keeps
its text reconstruction reachable even after its screenshot expires under retention. *PR9 audit: a
mark on a screen that had been static for 30+ seconds landed its frame 135 ms after the keypress.*

### Added — Local HTTP API + export (0.3.0 PR7)
ScreenSearch can now serve your screen history to local scripts and agents through an opt-in **local
HTTP API** — the open-source ask since Rewind shut down. It is **off by default**. When you enable it
in Settings it binds `127.0.0.1` only (never your network — that is hard-coded, not a setting) and
mints a **bearer token**: every request must carry it, and anything without it — or with the wrong
one — gets a `401`. The token is shown in Settings to reveal, copy, and regenerate (regenerating takes
effect immediately, no restart). The threat model is stated plainly next to the switch: any local
process holding the token can read your entire screen history, so enabling it is an explicit trust
decision. If the port (default `43210`) is already in use, enabling fails **loudly** — a warning, and
an inline "pick another port" retry — never a silent no-op.

The v1 API mirrors the app: `GET /v1/health`, `GET /v1/search`, `POST /v1/ask` (a streamed, grounded
answer over Server-Sent Events — and disconnecting the client now actually stops the model generating,
freeing the GPU), `GET /v1/frames/{id}` (with `?image=1` for the screenshot),
`GET /v1/context/where-was-i`, and marks (`GET`/`POST` and resolve — the only write surface). Full
docs, with copy-paste examples, are in `docs/API.md`.

A new **Export…** button in Settings writes your frames, content text, and marks to a JSON file in your
Downloads folder (screenshots are not included). It streams to disk, so exporting months of history
stays memory-flat, and it shares the API's export code path — so it works even with the API disabled.

Review hardening (PR #76): malformed requests (a bad query string, body, or path segment) — and now
unknown endpoint paths and an inverted `from > to` window — return the same `{ "error", "message" }`
error as every other response instead of a framework plaintext body; the port can be changed while the
API is running via a "Restart on {port}" button (previously the edited port was inert); the port field
clamps to `1024–65535`; a failed export never leaves a `.partial` file behind (fixed for Windows, where
the open file handle blocked cleanup); a screenshot missing on disk returns a clean `404` rather than a
`500`; overlapping enable/disable calls are serialized so a race can't leave the API running against a
disabled intent; and a client that disconnects mid-answer now stops the stream immediately instead of
draining the model's backlog first. A further round: regenerating the token can no longer be clobbered
by a concurrent config change; a failed settings write is now surfaced as an error rather than silently
leaving the API enabled on disk after a disable; and export filenames carry a random suffix so two
exports in the same second can't collide.

### Added — MCP server (0.3.0 PR8)
ScreenSearch now ships **`screensearch-mcp.exe`**, a small [Model Context Protocol](https://modelcontextprotocol.io)
server that lets Claude Desktop, Claude Code, or any MCP client search, ask about, and mark your screen
history — locally. It is a thin **stdio** wrapper over the local API from PR7: it holds no data of its
own, links no app code, and talks only to `127.0.0.1:<port>` with your bearer token (taken from
`SCREENSEARCH_API_TOKEN` / `SCREENSEARCH_API_URL` or `--token`/`--url`). It exposes six tools —
`search_screen_history`, `ask_screen_history`, `get_moment` (optionally returning the screenshot),
`where_was_i`, `list_marks`, and `add_mark` — mapping onto the same endpoints the app uses. If the API
is off, every tool returns a clear *"enable the API in ScreenSearch Settings"* message instead of failing
silently. The same trust posture as the API applies: any local process holding the token can read your
whole history, so enabling it is a deliberate choice; the copy-paste client config and threat model are
in `docs/MCP.md`.

The binary is packaged **inside the NSIS installer**, next to `ScreenSearch.exe` (a new
`bundle.externalBin` sidecar). There is no schema change.

Review-round hardening: `add_mark` now rejects a non-integer `frame_id` (e.g. a stringified id) as a
tool-input error instead of silently marking the current screen; the server refuses a non-loopback API
URL up front so a typo'd config can't ship your token to a remote host; and `get_moment`'s SSE line
parsing was streamlined. All confined to `screensearch-mcp` — no behaviour change for existing tools.

### Audited — PR9 integration audit + release
Every 0.3.0 acceptance line (`specs/03 §13b`) was verified end-to-end on a real Windows desktop:
grep gates for the removed subsystems, event-mode live-fire (app-switch, idle, timer fallback),
all four model tiers downloaded/loaded/used, overlay latency (6–9 ms vs the 150 ms bar) and
capture-self-exclusion, where-was-i + marks flows (including honest refusals with capture off, in
excluded apps, and while ScreenSearch itself is focused), the full API curl matrix (401s, SSE ask +
disconnect-cancel, loud port conflict with guided retry, live port change, token regeneration,
export with the API off, exit frees the port), and the MCP server against both scripted stdio and a
real Claude Code client. Warn-once semantics of the settings migrations were re-proven on seeded
boots. One environmental finding was recorded as a known gap: hot-unplugging a monitor mid-session
can stall capture until it is stopped and restarted (`specs/07` #91). Tracked evidence lives in
`specs/05_BUILD_REVIEW.md`; the raw transcript is a local-only audit artifact.

### Docs — 0.3.0 arc specs contract (PR1, specs-only; no code / schema / UI)
The 0.3.0 roadmap (`docs/0.3.0.md`) is normalized into the specs so every later PR (PR2–PR9) is
implementable from the specs alone, without reopening the roadmap. **This change touches only specs
and docs.** The contract now locks in the arc's **subtractions** — the six event-capture triggers
trim to foreground + idle (retiring the global `WH_MOUSE_LL` mouse hook, the clipboard listener, and
typing-pause), the model **Beta** tier is retired (Default/Quality only, uniformly Apache — Nemotron
and Qwen3.5-9B-VLM leave the registry), and the flag-off image-embedding lane is removed (with a
forward-only migration that drops only derived vectors) — and its **additions**: a global-hotkey
**Flow overlay** for instant recall, a **where-was-i + mark-this-moment** workflow, and an **opt-in
localhost HTTP API + export** with a thin stdio **MCP** wrapper. Deferrals (audio capture, custom
GGUF, proactive nudges, marks-in-reports, wider API write scopes) are recorded in known-gaps.
Automated PR review then tightened the contract (still specs-only): the "where was I" resume picks up
the app you were actually working in — not the overlay you just opened or a two-second detour;
"mark this moment" is pinned to the screen you're looking at on a multi-monitor setup; the local API
stops generating an answer the instant a caller disconnects and streams exports so a months-long
history can't exhaust memory; and the marks list has one defined order (open items first, newest
first).

### Changed — Faster UIA text capture on heavy windows (gap #71)
Reading a foreground window's accessibility text now fetches each element's properties in a **single
batched call** instead of ~5 separate cross-process calls, cutting the per-walk overhead ~2.5× on the
heavy Chromium/Electron windows the capture targets — while keeping the same small, interruptible steps
that bound the walk to its latency budget (two bulkier single-fetch designs were tried and dropped for
overrunning the budget on very large windows). Editable-field **values** are deliberately kept off the
batch and read only after the password/offscreen privacy guard passes, so a masked or hidden field's
text is never fetched. Live-verified capturing real Chrome pages. The earlier 0.2.1 hang mitigation is
unchanged.

### Changed — Expired screenshots now shrink the database too, not just disk (gap #73a)
When a screenshot expires under retention, the app keeps a text + layout reconstruction of it. That
reconstruction is drawn from per-word text positions, which are the biggest remaining database cost
for old frames. Rather than delete them (which would blank out the reconstruction), the app now
**merges each expired frame's words into per-line entries** — reclaiming roughly 80% of those rows
while the reconstruction still reads correctly at the line level. Merging happens as frames expire,
and a one-time pass on first launch cleans up frames that expired before this shipped. Search is
unaffected. (Full-text and semantic search read separate stored text/vectors, not these positions.)
Following PR review, an expired frame is now retired in a single atomic step, so a momentary
database hiccup can never leave a frame half-degraded (marked expired but still carrying its
per-word rows); if it fails, the frame is simply retried on the next sweep.

### Fixed — Semantic search could miss in-range results on tight time windows (gap #8)
When a search combined a query with a narrow time filter, the vector (meaning-based) arm fetched a
fixed pool of nearest matches and *then* dropped those outside the window — so an in-window match that
happened to rank just past the pool was silently missed. The vector arm now **widens its search
adaptively** when a time range is set: it re-runs with a larger nearest-neighbour count until the pool
fills with in-window matches or the index is exhausted. Unfiltered searches are unchanged. (sqlite-vec
can't filter inside its nearest-neighbour query, so widening the pool is the fix.) Search latency stays
well within budget (10k-frame fixture p95 ~66 ms, bar is 200 ms).
Following review (PR #66, P2): the widening now stops as soon as it has gathered every embedded frame
the window actually **holds**, bounded by a cheap, index-served count of in-window embedded frames.
Without that cap, a *sparse* window — one with fewer captures than the pool — on a database larger than
the widening ceiling would have run the maximum 20 000-neighbour pass on every query even after already
finding all its matches. An empty window now skips the vector query entirely.
Following the follow-up review (PR #66, P2): that count itself is now hard-bounded. A `LIMIT` on
*matches* can't stop early when a window has many captured frames but few embedded ones (an embedding
backlog, or a wide multi-day range), so the count would have walked the whole frame range — O(window),
not O(pool). The count now examines at most a fixed budget of frames; a window too large to prove
sparse within that budget falls back to assuming it is dense (widen up to the pool), which can only
*raise* the target and never drops an in-window match. The count is now O(pool) even on a wide,
sparsely-embedded window.

### Changed — Packaging spec re-scoped to NSIS (Inno / portable ZIP / MSI dropped)
ScreenSearch ships an unsigned **NSIS** installer (Tauri 2 native, since v0.1.0). The specs had still
called for an "Inno Setup installer + portable ZIP"; all nine live references (project intake,
context, plan, master-spec DoD §13.9, architecture doc, CI note, README) are rewritten to NSIS, and
DoD §13.9 is re-scoped to "NSIS installer builds successfully" and met. **Code-signing** is now the
lone remaining packaging follow-up (known-gap #26 closed).

### Fixed — Second launch restores a hidden window
The single-instance handler now shows the existing window before unminimizing and focusing it, so a
hidden / tray-minimized window is properly restored (not just unminimized) on a second launch.

### Accessibility — Keyboard & focus pass (gap #42)
- **Navigation rail** is a single Tab stop with a roving tabindex: Arrow Up/Down (wrapping) and
  Home/End move focus between the five destinations, and the active route carries `aria-current="page"`.
- **Command palette** returns focus to the control that opened it when it closes.
- **Recall → Ask** moves focus to the answer once streaming finishes, so keyboard and screen-reader
  users land on the result.
- **Settings** exposes each section's controls as a labelled group for assistive technology.

### Fixed — Model-downloader resume hardening (two corruption/waste edge cases)
Two narrow but real durability gaps in the parallel chunked model downloader
(`crates/inference/src/download.rs`), neither previously test-covered:
- **A truncated `.part` no longer publishes zeros (gap #69).** The resume bitmap was only discarded
  when the downloader *created* a brand-new `.part`. If an external cleanup tool truncated an
  existing `.part` without deleting it, `set_len` re-grew the file with zeros while a header-matching
  bitmap still marked those (now-zero) chunks "done" — so the zero-filled ranges were published (the
  length check passes; sha256 is skipped when the CDN advertises no `X-Linked-ETag`).
  `open_preallocated` now reports a part as untrustworthy whenever its on-disk length is **not
  exactly** `total` (brand-new, truncated, *or* corruption-grown larger), forcing a full refetch.
  Safe for normal resumes: a legitimate interrupted `.part` is always preallocated to exactly
  `total`, so it is never falsely discarded.
- **A locked download re-checks the cache before re-downloading (PR #27 Codex-P2).** When another
  live downloader holds hf-hub's per-blob advisory lock, the single-stream fallback backs off and
  retries; it now re-checks the clean-layout and HF-cache fast paths **after each backoff**, so if
  the holder finishes during the sleep the loser copies the finished blob into place instead of
  re-downloading it or colliding on publish. The bounded backoff was extended (per-attempt cap + more
  attempts → ~5 min total) so a real multi-GB download by the holder is outlasted rather than
  abandoned after ~20 s, while the cache re-check exits the instant the holder is done.

### Fixed — Full-resolution frames overflowed the vision context (tagging failed + slow)
Native full-resolution captures (e.g. a 3440×1440 ultra-wide frame → ~4148 vision tokens) exceeded
the vision lane's pinned **4096**-token context, so `llama-server` rejected every tag with HTTP 400
`exceed_context_size_error`; jobs retried 3× and dead-lettered (DB showed `vision_tag` 72 dead / 0
done). The deceptively low RAM/VRAM that looked like a "memory miracle" was the model **rejecting
requests in ~0.1 s without running inference** — the GPU sat near-idle while the weights stayed
resident (~6.4 GB VRAM). Fix is two-part, plus the diagnosability gaps that hid it:
- **Downscale the VLM request image** to a 1568 px longest edge before JPEG-encoding
  (`crates/inference/src/vision.rs`). Captures/timeline keep full resolution — only the tag request
  shrinks — so it fixes the overflow *and* cuts prefill time (addresses the slowdown). The 1568 px
  cap bounds even a worst-case square frame to ~2.5 K prompt tokens, comfortably under the
  spec-contracted **4096**-token vision default — so the auto-context is left unchanged (per PR #61
  review: bumping it would have contradicted `03 §8` "not bumped by default" and raised KV-cache
  VRAM on weak GPUs for headroom the downscale makes unnecessary).
- **Record the real cause**: `vision_tag` failures now log the full anyhow chain (`{e:#}`) into
  `jobs.last_error` (`crates/kernel/src/worker_pool.rs`) instead of the collapsed `"vision
  completion"`, and the sidecar's stdout/stderr are captured to `<sidecar dir>/llama-server.log`
  (`crates/inference/src/process.rs`) — previously the sidecar's own error was discarded.

### Docs — Restore the archive-on-release convention across the build-loop logs
Brought the lean-logs convention current. v0.1.0 history was archived on 2026-06-27, but the shipped
**0.2.0, 0.2.1, and 0.2.2** history had piled back up in the live logs. Moved it out **verbatim**
(byte-identical to `git HEAD`; original `#N` ids preserved so cross-references stay valid):
- `specs/07_KNOWN_GAPS.md` — 18 resolved 0.2.x rows + 5 accepted-as-is rows + the ~85-line
  resolved-engineering-decisions list → `specs/archive/07_KNOWN_GAPS.v0.2.x.md`. The live table now
  holds only still-open + current-arc rows (35 → 12).
- `specs/05_BUILD_REVIEW.md`, `specs/08_CHANGELOG_AI.md`, `specs/06_PATCH_PLAN.md` — shipped 0.2.x
  entries → matching `*.v0.2.x.md` archives (06 keeps its one open upstream-leak row #15).
- This file — the 0.2.0/0.2.1/0.2.2 sections (which had accumulated under `[Unreleased]`) →
  `CHANGELOG-ARCHIVE.md` as proper versioned sections.

### Added — Deterministic dev-only route-state triggers (closes `07` #43)
A dev-only `?__devState=loading|error` URL param forces any P5 route into its loading or error state
for audit verification, applied centrally at the TanStack Query seam and **stripped from production
builds** (`import.meta.env.DEV`). Empty/partial/populated stay driven by real data — no mocks in the
production path. Documented in `docs/DEV_STATE_OVERRIDE.md`. Per PR #60 review, the helper reads
`window.location.search` directly instead of `useSearchParams()`, so it calls no React hook: the
production path truly folds to `return result` (no router-history subscription on the 17 query
consumers, no `<Router>`-context coupling) rather than merely stripping the `__devState` string.

### Added — Privacy-safe VLM request logging (closes `07` #44)
The kernel now logs `frame_id` + the relative capture path at `info` immediately before each vision
request, so the VLM image input is visible in `screensearch.log` (previously a log scan found nothing).
No screen content, OCR text, or image bytes are logged.

## [0.2.2] — 2026-06-30

### Storage — readable native-WebP captures + degrade-to-text retention
Two coupled changes fix unreadable ultra-wide captures and unbounded storage growth, while keeping
the on-screen *text* as durable, searchable proof forever.
- **Readable captures.** `storage.max_width` now defaults to `0` = **native resolution** (no
  downscale), and captures are stored as **lossless WebP** instead of downscaled JPEG. On an
  ultra-wide this keeps text crisp where the old 1280 px JPEG crushed it 3–4×. Verified live: a real
  capture stored a 3440×1440 native WebP. (`storage.jpeg_quality` is now inert — the lossless encoder
  ignores it; the setting is kept for forward-compat. No new dependency: the WebP encoder ships in the
  already-present `image-webp`.)
- **Degrade-to-text retention.** `storage.retention_days` now defaults to **30** and its meaning
  changed: past the window the sweep deletes only the **screenshot file** and keeps the frame's row +
  raw/content text + spans + embeddings. Old captures stay fully searchable; the Moment view renders a
  crisp **text + layout reconstruction** from the stored OCR/UIA spans (a new `get_frame_spans`
  command + `FrameReconstruction` component) in place of the image. Previously retention hard-deleted
  the whole capture. Schema v7 adds `frames.image_purged` and v8 adds a partial index so the hourly
  sweep stays fast as purged history accumulates; degraded frames show a "Text kept" state in
  timeline/search/citation tiles. **Heads-up:** the new default auto-expires screenshots older than 30
  days (text is always kept) — set "Keep screenshots (days)" to `0` to keep images forever.

### Ask — OCR hydration failures
Ask now fails clearly before starting an answer stream if the bulk `ocr_texts` context hydration query fails, rather than silently falling back every retrieved frame to search snippets. Individual frames with no stored text still use their snippet after a successful hydration query; review follow-up keeps the regression test focused without a full `Store` fake, and hydration now moves each text out of the bulk-read map instead of cloning it per retrieved chunk.

### UI — Scanline timeline thumbnail lookup
Optimized the Scanline Timeline hover-preview lookup by sorting thumbnail metadata once per
`thumbnails` change and using binary search to find the nearest frame. Empty thumbnail lists,
single-frame lists, exact timestamp matches, and edge-of-range cursor positions preserve the
previous preview behavior.

### 0.2.1 — Sidecar recycle valve
Sidecar recycle valve: recycle llama-server at a committed-RAM ceiling to bound the upstream
multimodal memory leak (~149 MB committed host RAM per vision inference, VRAM flat, confirmed
2026-06-29 on build 9842). `sidecar.recycle_enabled` (default true) / `sidecar.recycle_rss_mb`
(default 0=auto; an explicit ceiling is clamped to 8192–131072 MiB — the 8 GiB floor clears the
vision model's ~6.8 GB warmup baseline so it can't recycle on every reload). Both keys apply on
app restart, like `sidecar.idle_ttl_secs`. Real fix is upstream / a newer bundled llama.cpp build
(`07` #72).

### Docs — README refresh to 0.2.1
Rewrote the top of `README.md` to match the live app (docs-only; no runtime or code change).
- Replaced the stale, run-on 0.2.0-era status blockquote with a short prose status line for 0.2.1.
- Refreshed **Build progress** and **Working today** to reflect shipped 0.2.x work, and added a
  compact **0.2.x arc** table (attention-first text, Recall reports, event-driven capture, UIA text
  source, smart enrichment throttle).
- Added the three missing crates to the repository-layout map (`textfilter`, `uia`, `sysmon`) so it
  lists all 11 workspace crates, and noted the UIA text source + pressure throttle in the
  architecture summary.

## [0.2.1] — 2026-06-29

### 0.2.1 — Fix: UI Automation freezes Chromium/Electron apps (mitigation)
Fixes a bug where enabling **UI Automation text** (`capture.uia_text_enabled`, default on) made
Chromium-based apps — **Chrome, Edge, and Claude Desktop (Electron)** — go "Not responding" once a
page exceeded ~1.5 pages, with scrolling triggering it immediately (e.g. a large data grid like the
qBittorrent web UI). Root cause: every capture trigger ran a live, uncached, *raw-view* walk of the
foreground window's **entire** accessibility subtree, issuing thousands of synchronous cross-process
calls that the target app's UI thread had to service — so the *browser* froze (no CPU spike; threads
parked on COM marshalling). Two amplifiers: the latency budget couldn't interrupt an in-flight call,
and an unbounded request queue let every scroll/click pile another full walk onto the worker so the
target never recovered. This is the low-risk mitigation; a cache-request rewrite follows.

- **Don't run UIA on scroll/click.** High-frequency interactive triggers (scroll-stop, click) now
  fall back to OCR (the captured bitmap, which never touches the target app); idle / timer /
  typing-pause / foreground / clipboard / manual frames still use UIA. Scrolling — the exact
  reproducer — no longer fires a tree walk.
- **No more walk backlog.** At most one UIA walk runs at a time (a worker-owned in-flight guard) and
  the worker's request channel is bounded, so a slow walk can no longer queue further walks against
  the target app. Busy frames skip straight to OCR.
- **Lighter walk.** The walk now uses the UIA **control view** instead of the raw view, collapsing a
  Chromium page's per-text-run node explosion to the elements that actually carry text — far fewer
  cross-process calls — while preserving the extracted text.
- **Costliest call gated.** The live `TextPattern` text read (the one UIA call that can't be
  batched) now runs only on document/editable-text elements and is capped per frame, so a text-heavy
  page can't reopen the call storm.
- **Observability.** Per-walk node count + elapsed are logged at debug, and a rate-limited warning
  fires when a walk hits its latency budget or frames are skipped under load, so the condition is
  visible in the log (previously the UIA path was silent at `info`).
- **Tunable.** New Settings (Text source panel): run UIA on click/scroll (default off), use
  control-view only (default on), max nodes per read, and max text reads per read — all clamped.

A deeper optimization (replacing the per-element reads with a single cached UI Automation
round-trip) is planned as a follow-up and pending live-desktop verification.

_Review follow-up (PR #48):_ closed the one residual freeze path the scroll/click gate left — with
event-driven capture **off** (the default), every frame is a timer frame, so a timer tick could
still walk the tree mid-scroll. A new setting **"Pause UIA after input"**
(`capture.uia_suppress_during_input_ms`, default 500 ms) now routes timer frames captured shortly
after you type / click / scroll / move the mouse to OCR instead, so an active scroll never triggers
a UI Automation walk; `0` disables it, and it's bypassed when "Run UIA on click and scroll" is on.
Also removed a redundant lock around the UIA worker sender, and documented the full 0.2.1 capture
key set (event-driven + UIA text-source, including the hang-fix knobs) in the master spec `§8`.
Project version bumped to **0.2.1** across the workspace, Tauri config, and npm manifests (the
release tag follows separately).

### 0.2.1 — Event-driven capture review follow-up
Tightens the event-driven capture paper trail after review of the implemented PR4/PR5 roadmap items.

- **Docs now match the shipped trigger set.** `docs/ARCHITECTURE.md` and `docs/TESTING.md` now describe
  click and scroll-stop as landed, default-off event triggers, correct the schema reference to v6, and
  include live manual acceptance steps for the mouse-hook triggers.
- **Trigger token coverage hardened.** The `CaptureTrigger` round-trip test now covers `click` and
  `scroll_stop` in addition to the earlier timer / foreground / clipboard / idle / typing-pause /
  manual tokens, and the parser has an exhaustive guard so future enum additions force the
  `from_db_str` path to be revisited.

### 0.2.1 — Smart enrichment throttle (former PR5)
Adds an optional, off-by-default **performance throttle** that keeps the app responsive under load:
when the machine is under sustained CPU/GPU pressure, ScreenSearch eases off the heavy background
work (vision tagging, image embeds) and, at the deepest level, reduces text-embedding concurrency —
while **capture, OCR, and storage never pause**, so nothing is lost. This lands on the 0.2.1 line
(`docs/0.2.0.md` deferred work; `07` #49).

- **Pressure-aware backpressure.** A lightweight monitor samples whole-machine CPU (via
  `GetSystemTimes`) and GPU utilization (via Windows performance counters — works for **any** GPU:
  NVIDIA, AMD, or Intel integrated). Under sustained pressure the enrichment queue pauses
  `vision_tag` + `embed_image`; under deeper/longer pressure it also floors `embed_text` concurrency
  to a configurable minimum so search indexing never fully stalls. It engages and releases with
  hysteresis (separate enter/exit thresholds + dwell times) so it never flaps.
- **Off by default, opt-in.** Master switch `throttle.enabled` (default off → behavior is byte-identical
  to before). When on, nine clamped knobs tune it (CPU/GPU enter+exit %, enter/exit dwell ms, sample
  interval, and the text-embed floor) in a new **Performance throttle** panel in Settings.
- **Truthful status.** The Settings panel shows live CPU/GPU pressure and the current throttle level,
  and a subtle **Throttling** chip appears in the status rail only while it's actually engaged — its
  tooltip surfaces the live CPU/GPU reading. If the GPU exposes no Windows performance counters (some
  VMs / older drivers), both the panel and the chip tooltip say **"GPU not monitored"** plainly and the
  throttle runs on CPU pressure alone — never a misleading 0%.
- **Privacy & safety.** Reads only aggregate system performance counters — no per-app or per-process
  inspection. The throttle only ever narrows what the background enrichment workers claim; it cannot
  touch capture, OCR, storage, or the sidecar lifecycle.

### 0.2.1 — UIA target-window text, with OCR fallback (PR4 part 2)
Adds a higher-fidelity text source: when capturing, the app now reads the **foreground window's text
via Windows UI Automation (UIA)** — more structured than full-screen OCR — and falls back to OCR
whenever UIA yields nothing useful. This lands on the 0.2.1 line (`docs/0.2.0.md` deferred work; `07`
#48).

- **UIA text with automatic OCR fallback.** A new `uia` crate reads the foreground window through UI
  Automation and feeds the *same* text pipeline OCR already uses (per-word spans, normalized
  geometry), so search/Ask/embeddings consume it unchanged. Per frame: try UIA; on any failure,
  latency-budget overrun, or a too-short ("thin") result, fall back to OCR. **OCR remains the
  guaranteed floor** — capture still only refuses to start when OCR itself is unavailable.
- **Default ON.** UIA is enabled by default (`capture.uia_text_enabled`, hot-applied per frame, no
  capture restart). Two clamped knobs tune it: `capture.uia_latency_budget_ms` (150, 20–2000) bounds
  the per-frame tree walk, and `capture.uia_min_text_chars` (16, 0–10000) is the thin-yield floor
  below which the frame falls back to OCR (so GPU/canvas/custom-drawn windows, where OCR is strictly
  better, still get OCR). A new **Text source** panel in Settings exposes all three; the Moment view
  shows a **UIA**/**OCR** chip for how each frame's text was read.
- **Privacy.** UIA can surface text OCR can't, so two hard rules apply: **password fields are never
  read**, and **offscreen/occluded elements are skipped** (preserving OCR's "only what was visible"
  parity); spans outside the foreground window's rect are dropped. As with OCR, every existing capture
  gate (excluded apps, self-window, lock, diff) runs *before* any text is read.
- **Provenance is now truthful.** Each frame records whether its text came from `uia` or `ocr`
  (previously hardcoded to `ocr`), surfaced in the Moment view.
- **Roadmap deviation (authorized).** The roadmap framed UIA as opt-in; shipping it **default ON** was
  the user's explicit choice, which raises the bar on the capability probe, the per-frame latency
  budget, and the OCR fallback (all load-bearing on every frame). Recorded in `07` #48.

### 0.2.1 — Event-driven capture: click + scroll-stop triggers (PR4 remainder)
Completes the event-driven trigger set begun in the entry below with the two remaining triggers
(`07` #47).

- **Two new triggers, default off.** **Click** (a mouse button press) and **scroll-stop** (a scroll
  burst collapsed to a single capture at its trailing edge) join the existing event triggers, behind
  `capture.event_on_click` and `capture.event_on_scroll_stop` (both default off, shown in the
  event-driven Settings panel). They reuse the existing debounce + rate-ceiling machinery, so a scroll
  burst becomes one "scroll-stop" capture and clicks can't storm.
- **Low-level mouse hook — roadmap deviation (authorized).** These require a global low-level mouse
  hook (`WH_MOUSE_LL`), which the roadmap **deliberately steers away from** (a slow hook callback adds
  system-wide input latency). Enabled at the user's explicit request; the hook is installed **only**
  when click or scroll-stop is on, the callback does nothing but a non-blocking signal, and it
  **never reads cursor coordinates** — only *which* mouse message fired. Recorded in `06` and `07` #47.
- **"Captured via" extended.** Frames captured by these triggers record `click` / `scroll stop`,
  shown in the Moment view. Stored via a forward-only migration (`schema_version` 5→6) that widens the
  `frames.capture_trigger` constraint; the parent-table rebuild is FK-safe and proven by a populated-DB
  migration test (children survive, cascade intact, new tokens accepted, bogus tokens rejected).

### 0.2.1 — Event-driven capture (opt-in)
Adds an **opt-in, default-off** event-driven capture mode so capture can fire on real user activity
instead of only a fixed timer. 0.2.0's timer/idle capture is unchanged and remains the default; this
lands on the 0.2.1 line (`docs/0.2.0.md` deferred work; `07` #47).

- **Capture-mode switch.** A new master setting **Event-driven capture** (`capture.event_driven_enabled`,
  default off) selects Timer vs Event-driven. In event mode, capture fires on the enabled triggers
  plus a long **fallback interval** (so a static screen is still sampled), a **debounce** (collapses a
  burst of triggers into one capture), and a **min-interval rate ceiling** (prevents capture storms).
- **Four triggers.** **Foreground / app-switch** (`SetWinEventHook` `EVENT_SYSTEM_FOREGROUND`),
  **clipboard change** (`AddClipboardFormatListener`, change event only), **idle**, and
  **typing-pause** (both derived from `GetLastInputInfo` timing). Foreground + clipboard are on by
  default when event mode is enabled; idle + typing-pause (the noisier triggers) are off by default.
- **Privacy-safe by design.** **No keystrokes and no clipboard contents are ever read or stored** —
  only change/idle-timing signals. The mode is opt-in and off by default, and every existing privacy
  gate still applies (self-exclude own window, excluded apps, pause-on-lock, the diff gate). The
  feature is implemented over a pure, unit-tested trigger state machine plus a single dedicated
  message-pump thread (message-only window + foreground hook + clipboard listener) that tears down
  cleanly; a failed hook install is non-fatal and falls back to the fallback timer + idle polling.
- **"Captured via" provenance.** Each frame now records *why* it was captured (timer, idle,
  foreground change, clipboard change, typing pause, or manual), shown in the Moment view. Stored via
  a forward-only migration (`schema_version` 4→5, a new nullable `frames.capture_trigger` column);
  frames captured before this read as "unknown".
- **All thresholds are settings, never hardcoded.** Debounce, min-interval, typing-pause, idle, and
  fallback durations are clamped settings, alongside per-trigger on/off toggles.
- **Click and scroll-stop** were initially deferred (they need a low-level mouse hook, `WH_MOUSE_LL`,
  the roadmap deliberately avoids) — they have since **landed** at the user's explicit request; see the
  *click + scroll-stop triggers* entry above.

## [0.2.0] — 2026-06-27

### Docs — Token-bloat optimization (archive v0.1.0 history, de-duplicate, fix drift)
Reorganized the documentation so an LLM reads only the current 0.2.x arc instead of wading through
shipped v0.1.0 history. No content was lost — pre-0.2.x entries were moved **verbatim** (byte-identical
to `git HEAD`) into sibling archives:
- Build-loop logs split to `specs/archive/`: `05_BUILD_REVIEW` (167 KB → 5 KB), `08_CHANGELOG_AI`
  (146 KB → 36 KB), `07_KNOWN_GAPS` (38 of 69 resolved v0.1.0 rows archived, ids preserved),
  `06_PATCH_PLAN` (4 of 11 rows archived).
- Released history split out of this file into `CHANGELOG-ARCHIVE.md` (the `[0.1.0]` section).
- De-duplicated `docs/ARCHITECTURE.md` (crate map / schema now point to `03 §2`/`§4`) and
  `docs/0.2.0.md` (rationale → `02 §5b`; audit summaries → `docs/audits/`).
- Relocated point-in-time `AUDIT_0.2.0_PR*.md` evidence into `docs/audits/` (kept git-ignored).
- Fixed README's broken phantom `AUDIT_V2C_2026-06-23.md` link (×3) and refreshed the stale
  "current state" lines in `CLAUDE.md`/`AGENTS.md`.
- Added an **archive-on-release** convention to `CLAUDE.md`/`AGENTS.md`/`04` so logs stay lean.

Total: ~355 KB trimmed from the live docs. Verified: every archive split diffs empty against `git HEAD`;
row counts reconcile (06: 7+4=11, 07: 31+38=69); `cargo check --workspace` green.

Review follow-ups (PR #43): since `docs/audits/` is git-ignored, the README now references those
audits as plain-text local-only artifacts instead of active links (which would 404 on GitHub). The
`docs/ARCHITECTURE.md` schema pointer now names `crates/store/src/schema.rs` (`LATEST_SCHEMA_VERSION = 4`)
as the authoritative as-built DDL — `03 §4` is the design contract and has drifted behind the 0.2.x
text-signal migrations (v3 drops `ocr_text`, v4 adds `text_spans.line_index`).

### Fixed — PR7 Ask source-frame labels
Followed up on the PR7 no-evidence Ask audit finding: source-frame tiles that represent retrieved
context are now labeled **Frames checked** instead of **Cited frames**. This keeps the existing typed
IPC stream and local-model prompt unchanged while avoiding the misleading implication that unrelated
context frames support an honest no-evidence refusal.
Verified in the dev executable launched by `npm run tauri dev`: a no-evidence token refusal rendered
retrieved tiles under **Frames checked** and did not render the old **Cited frames** label.

### Docs — PR7 audit status reconciliation
Reconciled tracked docs after the later PR3 audit fix: the old PR7 static-chrome finding is now
recorded as resolved by the self-exclude/backfill work, with the real residual kept in the
multi-monitor / rect-None gap. Also updated the Architecture search-limit wording to the current
`1..=2,000` backend cap and clarified that detailed `docs/AUDIT*.md` / `.playwright-mcp/` artifacts
are local-only ignored evidence.
The same dev-exe pass also spot-checked the already-fixed range-neutral report progress copy and the
default Chrome search path.

### Docs — 0.2.0 PR8 audit checkpoint
Recorded the PR8 parallel model download audit on `codex/0.2.0-pr8-audit`. The audit used the real
dev executable launched by `npm run tauri dev` from a reset app-data state, downloaded the default
answer model, exercised the Vision Quality 8B GGUF + mmproj path through Moment -> Tag with vision,
and verified an interrupted Vision Beta download resumed from its `.parts` bitmap instead of
restarting from zero. Static review found no PR8 schema/IPC/binding drift beyond the documented
`sha2` dependency and downloader env overrides. The existing PR3 static-chrome release blocker
remains separate; one narrow accepted PR8 follow-up around a pre-existing truncated `.part` plus
stale bitmap is tracked in `specs/07_KNOWN_GAPS.md`.

### Fixed — 0.2.0 PR3 attention-filter release blocker + Privacy/Excluded-Apps hot-apply
Addressed the 2026-06-26 PR3 audit (`docs/AUDIT_0.2.0_PR3_2026-06-26.md`): default content search
no longer ranks frames because of static/app chrome.

- **Self-exclude the app's own window.** The capture privacy gate now skips frames whose foreground
  window belongs to ScreenSearch's own process (PID-based, so it never mismatches a third-party
  window merely titled "screensearch"). Indexing our own UI only stored sidebar nav / command
  palette / a results pane echoing other captures — the dominant `Deck`/`Recall` self-capture leak.
  A gated, one-time startup purge removes any pre-existing own-window captures (a clean install finds
  none and no-ops).
- **Attention-filter backfill.** `FILTER_VERSION` 1→2: on startup the store re-cleans every frame
  below the active version against the now-warm chrome catalog (`textfilter::reconcile`, pure +
  monotonic — it only ever suppresses more, never resurrects content, and needs no `target_rect`).
  Changed frames re-enqueue an `embed_text` job so the vector arm re-embeds from clean text. This
  retroactively fixes the classifier's cold-start window (chrome kept until its signature crossed the
  threshold) — previously frozen with no backfill.
- **Faster cold-start.** `text.chrome_suppress_min_seen` default 12→4, so a repeated short edge label
  is caught within a few captures; long lines, interior body, and rect-less frames stay protected,
  and `include_chrome` + raw text recover any false positive.
- **Privacy / Excluded Apps now apply immediately.** `set_settings` compares the derived
  `CaptureConfig` and reloads a running capture loop on change, so adding an excluded app (or
  changing monitors / interval / pause-on-lock) takes effect at once instead of silently waiting for
  the next manual capture start (a user-reported bug).

Evidence (shipped backfill + purge replayed over the real 313-frame audit corpus copy, default
content FTS hits before→after): `Deck` 68→26, `Recall` 42→15, `Firefox` 24→15, `Steam` 24→15,
`GPU Memory` 19→15 — the remainder is legitimate content (frames literally discussing those terms;
`GPU Memory` in Task Manager). `include_chrome`/raw still recovers everything and real work terms are
intact (`cargo test`=41, `embeddings`=13). Honest residual: rect-None / multi-monitor secondary
captures of *other* apps' desktop chrome remain (gap #58 — needs `target_rect` for those frames).

Review hardening (PR #41, Codex + Gemini):
- **Stale embeddings invalidated during backfill (Codex P1).** When the backfill rewrites a frame's
  `content_text`, it now deletes that frame's stale text embedding (`source = 'ocr'`) in the same
  transaction (the `embeddings_ad` trigger cascades to the vec0 shadow). Otherwise the dropped chrome
  terms could keep surfacing the frame through `hybrid_search`'s vector arm — which fuses even with
  `include_chrome=false` — until the async re-embed ran, or *indefinitely* if text embedding is off.
- **Backfill no longer re-queries the catalog per frame (Gemini).** The read-only chrome catalog is
  cached by `app_hint` across the whole backfill instead of one `chrome_text_catalog` query per frame.
- **Self-capture purge no longer orphans files (Gemini).** A transient JPEG-delete failure now skips
  the row delete (matching the retention sweeper) so the file isn't orphaned; the no-progress guard
  still stops the loop if a batch makes no headway.
- **Backfill releases the store connection between batches (Codex P2, 2nd pass).** The whole backfill
  no longer runs under a single `with_conn`, which would hold the store's one DB connection for the
  entire pass and block search / settings / capture inserts on a large upgraded DB. It now snapshots
  the catalog once up front and processes each batch in its own short-lived `with_conn`, so other DB
  work interleaves during the background startup backfill.
- **Self-purge watermark only on full drain (Codex P2, 2nd pass).** The one-time purge records its
  `maintenance.self_capture_purged` watermark only after every own-window frame is actually gone — so
  a transient failure that leaves frames behind retries on the next launch instead of permanently
  skipping the purge (this matters more now that the orphan fix above can leave a locked frame behind).
- **Accepted, documented as-is:** the lower `chrome_suppress_min_seen` default (12→4) reaches new
  installs only — existing users keep their persisted value (no settings migration, and the
  backfill-clamp alternative was also declined, by choice); and the `reload_capture` stop→start window
  is left non-atomic — its doc comment was corrected (Tauri 2 async commands are *not* serialized, so
  the race is real but accepted: sub-millisecond window, no UI affordance to fire both paths at once).

### Docs — 0.2.0 PR6 audit checkpoint
Recorded a scoped PR6 audit checkpoint for Recall reports and Ask shortcuts. The audit created a
local-only ignored artifact at `docs/AUDIT_0.2.0_PR6_2026-06-26.md` plus evidence under
`.playwright-mcp/pr6-2026-06-26/`. Static wiring and targeted regression tests found no PR6
implementation blocker: reports and Ask shortcuts are wired over `frame_text.content_text`, Ask
defaults to `include_chrome=false`, and the report hardening tests for coverage splitting,
empty/no-sidecar reports, settings clamps, cancellation, and report summary parsing pass. A second
Computer Use run against the real dev executable completed the live UI audit: Search/Ask/Reports
rendered, all five Ask cards were visible, Day Recap submitted with cited frames, Daily/Weekly/
prompted-Custom/no-evidence-Custom reports generated, Settings showed `8/40/200/20`, and a
controlled Windows Notepad capture stored the PR6 probe token in `content_text`. The upstream PR3
static-chrome release blocker remains separate. Full verification passed after clearing a stale
repo-local Vite/esbuild process left over from the dev run.

### Docs — 0.2.0 PR3 audit
Added `docs/AUDIT_0.2.0_PR3_2026-06-26.md` and release-tracking notes for the PR3
attention-first filtering audit. The audit used the real dev app and existing app DB, confirmed the
storage/retrieval plumbing is mostly wired correctly, and records a **release blocker**: default
content search still surfaces static/app chrome for `Firefox`, `Steam`, `Deck`, `Recall`, and
`GPU Memory`, including a fresh Notepad capture that also indexed desktop/app navigation terms in
`content_text`. No code fixes were made in this audit pass.

### Docs — 0.2.0 PR1/PR2 audit follow-ups
Recorded a short `docs/0.2.0.md` handoff list for a later LLM pass before tagging 0.2.0. The note
captures the open PR7 static-chrome retrieval acceptance issue, roadmap/status drift around PR7 and
PR8, missing PR8 sequencing in `02`/`04`, `03` schema/type drift versus schema v4
`text_spans.line_index`, stale mandatory-reading docs, and the PR2 audit verdict. Follow-up review
clarified that the PR7 static-chrome failure remains a release blocker unless the acceptance
contract changes.

### 0.2.0 PR8 — Parallel model download
Replaces the single-stream first-run model fetch with a **multi-connection chunked downloader** so
the GGUF/mmproj download saturates the user's bandwidth instead of being capped at hf-hub's one
HuggingFace-CDN connection (~20 MB/s). Confined to `crates/inference/src/download.rs` (+ its tests),
independent of the retrieval chain (`docs/0.2.0.md` PR8).

- **Chunked, resumable, parallel.** A `Range` probe learns the total size, range support, and the
  LFS sha256 (when advertised); if the server honours ranges, the file is fetched with N parallel
  `Range` requests (`DOWNLOAD_CONNECTIONS`, default 8 — `SSV2C_DOWNLOAD_CONNECTIONS` to override,
  clamped 1–16) writing into one pre-allocated `.part` via positioned writes. A per-chunk completion
  **bitmap** (`<base>.parts`) is fsync'd *after* its chunk's data, so an interrupted download resumes
  exactly where it stopped with no torn-write window.
- **Per-request resolve (HF Xet bridge).** HuggingFace's signed CDN URLs are cryptographically bound
  to the exact `Range` they were minted for (the CloudFront policy pins
  `ByteRange.ExpectedHeader`), so a single resolved URL **cannot** be reused across chunks — reuse
  returns `403`. Each chunk therefore re-requests the stable `…/resolve/main/…` URL with its own
  `Range` and follows the redirect (reqwest preserves `Range` across the hop), minting a fresh
  range-matched signed URL per chunk — the same approach `hf_transfer` uses. A transient
  `403`/`401`/`429`/`5xx` is retried with backoff.
- **Correct integrity target (HF Xet bridge).** A file's true content sha256 is exposed **only** in
  the `X-Linked-ETag` header on the `resolve` *redirect* (alongside `X-Linked-Size` and
  `Accept-Ranges`). The CDN blob *behind* the redirect carries a bare `ETag` that is the **Xet
  content hash** (a different 64-hex digest of the same bytes, also surfaced as `X-Xet-Hash`), not
  the file sha256 — and on classic LFS it is an S3 multipart validator (`md5-partcount`). The probe
  therefore reads the redirect's **own** headers *without following it*, and verifies only against
  `X-Linked-ETag`; the bare `ETag` is never trusted. Previously the probe followed the redirect and
  read the CDN `ETag`, so a correctly-downloaded Xet model (e.g. Qwen3-VL-4B `Q4_K_M`) failed sha256
  every time and re-downloaded in a loop — visible as a stall at ~75% (the GGUF is 75% of the
  GGUF+mmproj total, so it finished, failed verification, and restarted before the mmproj began).
- **Reuses the existing machinery, doesn't bypass it.** All chunks feed the same downloaded-byte
  counter, so the UI percentage and the aggregate-progress stall watchdog stay truthful with
  concurrent writers ("no aggregate progress across all chunks" is the stall condition, unchanged).
  The finished blob still lands in the clean layout via an atomic rename, and `fetch_one`'s
  already-in-layout / already-a-cache-blob fast paths are still checked first.
- **Range-less fallback + integrity.** A server that doesn't advertise ranges (no `Accept-Ranges` /
  no length) falls back to the single-stream hf-hub path, so a non-resumable mirror still downloads.
  The assembled file is verified against the LFS sha256 when present (else byte length); a mismatch
  discards the partial for a clean retry. A per-chunk `206` assertion refuses a server that silently
  ignores `Range` rather than corrupting the file; an expired CDN signed URL (`401`/`403`) triggers
  one re-resolve.
- **Cancellable (gap #46 for this path).** The chunk futures are awaited inside the fetch task with
  `buffer_unordered`, so the stall watchdog's `task.abort()` drops every in-flight chunk — no orphaned
  writers, unlike hf-hub's internally-detached chunk tasks on the fallback path.
- **Tests (this PR owns them):** parallel byte-identity, resume-skips-completed-chunks,
  fail-fast-on-stuck-chunk, range-ignored rejection, sha256 accept/reject, `X-Linked-ETag`-only
  integrity (rejects the bare CDN/Xet `ETag`), and the `range_plan` decision — all fully mocked
  (wiremock + a local hang-after-206 server), no network.
- **PR8 review hardening (PR #35 bot review).** Five robustness fixes in `download.rs`, all with
  tests: (1) a brand-new (zero-filled) `.part` now **discards a stale all-done `.parts` manifest**
  rather than trusting it — `open_preallocated` reports whether it created the file (atomic via
  `create_new`), and an all-complete bitmap over a fresh part is re-initialised, closing a path that
  could publish a zero-filled model when no `X-Linked-ETag` was advertised; (2) a transient
  **network error** on a chunk request (dropped connection / timeout / DNS blip) is now retried with
  backoff instead of failing the whole download on the first hiccup; (3) a **present-but-unreadable**
  `.parts` manifest (a Windows sharing violation from AV / the indexer) now surfaces an error so the
  job-queue retries, instead of silently truncating the bitmap and discarding real progress; (4)
  streamed body frames are **coalesced to ~256 KiB** before each positioned write, cutting
  `spawn_blocking` churn; (5) the terminal chunk error now distinguishes "server ignored Range"
  (a `200`) from "failed after N retries" (an exhausted `403`/`429`), so logs don't send an operator
  chasing a non-existent range-support bug. Two reviewer suggestions to make `download.rs` compile on
  macOS/Linux (`#[cfg(unix)]` `write_at`) were intentionally **declined** — the project is
  Windows-only by design (CLAUDE.md hard rule; CI is `windows-latest`), matching the
  Windows-native convention already in `flags.rs`/`lib.rs`.
- **PR8 follow-up (PR #35 bot review).** Generalised the fresh-part stale-manifest guard: a
  brand-new (zero-filled) `.part` now discards a header-matching `.parts` bitmap that marks **any**
  chunk complete, not only an all-complete one. The earlier guard caught the all-done case (failed
  post-publish cleanup) but missed a **partly-done** bitmap surviving over a fresh part — e.g. an
  interrupted download whose multi-GB `.part` a user/cleanup tool later reclaims. Trusting it would
  skip the done-marked ranges, leaving zeros in the assembled GGUF (length passes; sha256 is skipped
  with no advertised `X-Linked-ETag`). New `Manifest::any_complete()` drives the check; covered by
  `fresh_part_discards_stale_partial_manifest`, which fails on the prior condition.

### 0.2.0 PR7 — Integration audit
Ran the PR7 end-to-end audit against the existing populated app-data DB using `npm run tauri dev`
and the real debug executable (`target/debug/screensearch.exe`). Evidence screenshots were captured
locally under ignored `.playwright-mcp/pr7-2026-06-25/` storage and are not tracked.

- **Search audit:** content searches for real work terms (`Calendar-Grid`, `cargo test`, etc.) worked
  and the `include app chrome + raw text` toggle recovered raw/static labels. Default content search
  still surfaced static/app-chrome-heavy results for terms like `Firefox`, `Steam`, and especially
  `Deck` in the populated corpus; that finding was later addressed by the PR3 self-capture/backfill
  fix, with residual rect-None / secondary-monitor chrome tracked separately.
- **Ask audit:** a Calendar-Grid Coverage Map-Reduce question grounded on content text and showed
  source frames. A unique-token no-evidence question refused honestly, but still displayed retrieved
  context frames under `CITED FRAMES`; the PR7 follow-up relabels those tiles as `Frames checked`.
- **Reports audit:** Daily and Weekly reports generated through the UI, cited source frames, and
  reported honest pass/frame footer metadata. Fixed one small UI copy bug where the progress helper
  said "Weekly reports..." even while generating Daily; it now uses range-neutral bounded-pass copy.
- **Capture audit:** started capture briefly, observed a fresh `20:26` frame/tick and queue advance,
  then stopped capture. No destructive DB operation was performed.

### 0.2.0 PR6 — Recall reports + Ask shortcuts
Adds a third Recall capability on top of the attention-first `content_text`: **Reports** —
on-device Daily / Weekly / Custom summaries that **cite the frames they used** — plus per-request Ask
depth and premade Ask prompt cards (`03 §7`/`§8`/`§8b`, `docs/0.2.0.md` PR6). The defining
requirement (user directive): **report context scales with the time range and guarantees temporal
coverage, not just relevance** — a weekly report genuinely covers the whole week (every active day),
never a flat window biased to the most-recent or most-relevant frames.

- **Coverage-first report engine (Calendar-Grid Coverage Map-Reduce).** Instead of growing the model
  window for longer ranges (which scales KV-cache VRAM and forces a sidecar relaunch), the model
  context stays **pinned flat at 8192** and the *number* of bounded passes grows with the range. A
  range is split into a per-calendar-day grid; a single density probe finds the active days; each
  active day gets its own frame budget (floored so a quiet Saturday is never starved by a busy
  Monday, capped so a dense day can't dominate) sampled **evenly across the day** in chronological
  order; one summary pass runs per active day, then a bounded, time-ordered hierarchical reduce folds
  them into the final report. This makes per-day coverage a structural guarantee at the same VRAM as
  a single Ask. A **Custom report with a prompt** instead drives semantic retrieval
  (`hybrid_search`) for relevance.
- **Honest by construction.** An empty range returns a no-evidence report with **zero** model calls.
  Every report footer shows what actually happened — model, passes, covered/total active periods,
  summarized/sampled frame counts, an estimated token figure, and a truncation notice if a
  pathological range hit the safety ceiling — and the body **cites the exact source frames** (chips
  that open the Moment).
- **Reports UI mode.** Recall gains a third mode beside Search and Ask: pick Daily / Weekly / Custom
  (Custom = date range + optional prompt), Generate with live progress ("summarizing day k of N…")
  and Cancel, then read the rendered markdown with clickable source-frame chips, **Copy**, and **.md
  download**. All view states (empty / generating / error / populated / honest no-evidence) are
  defined.
- **Premade Ask cards.** The Ask idle state now offers five one-tap prompts — Day Recap, Standup
  Update, Time Breakdown, Top of Mind, AI Habits — that fill and submit a grounded, cited answer.
- **Ask depth is now a setting, not a constant.** Removed the hardcoded `ASK_TOP_K`; Ask reads
  `retrieval.default_top_k` (with an optional per-request override). A new **"Reports & retrieval"**
  Settings panel exposes the four tuning keys (`retrieval.default_top_k` 8, `reports.daily_top_k` 40 =
  per-active-day budget, `reports.weekly_top_k` 200 = global frame cap, `reports.map_reduce_min_frames`
  20 = single-pass threshold), all clamped.
- **No schema change.** The temporal sampler is a query; the report types are additive IPC
  (`ts-rs` bindings regenerated). Ask output is unchanged (the shared context-packer is byte-identical
  to before). Deviations from the literal spec and three accepted limitations (DST grid skew,
  structural bounds as constants, estimated-token footer) are recorded in `specs/06`/`07`.
- **Review fixes (PR #33).** (1) Removed an N+1 over the report's day grid — the coverage path now
  bulk-fetches every active day's text in one query (Gemini). (2) A trailing single summary in the
  hierarchical reduce passes through instead of spending a model call to "combine" one node (Gemini).
  (3) Prompted (Custom-with-prompt) reports no longer cap relevance retrieval at the search-UI's 100
  frames — the backend ceiling was raised so `reports.weekly_top_k` (up to 2000) is honored (Codex).
  (4) The report planner budgets passes against the provider's **actual** answer-lane context window
  (via the new `AnswerProvider::answer_context_budget`) so a user-lowered `sidecar.ctx_size` can't make
  it pack summaries the sidecar then truncates (Codex). (5) Doc-comment clarified that the default
  `summarize` does not forward the system prompt (Claude).
- **Review fixes (PR #33, round 2).** (1) The temporal sampler returned ~half the requested frames
  whenever a period had just over the limit (a `ceil(total/limit)` stride doubling at `total > limit`,
  e.g. 41 frames → 21); replaced with even-rank bucketing that returns the full `min(total, limit)`
  quota (Codex). (2) Prompted reports no longer show the "range trimmed to fit" notice spuriously —
  the relevance path now flags `truncated` only when the search cap is actually hit, not when
  empty-text hits are filtered (Claude). (3) Empty/no-evidence reports no longer require the answer
  sidecar, so they work on first launch while the model is still downloading (Codex). (4) Daily/Weekly/
  Custom ranges are computed from local calendar days instead of fixed 86.4M-ms spans, so DST
  transition days no longer include/drop an hour (Codex). (5) The `frames_summarized` doc-comment no
  longer claims equality with `cited_frame_ids.len()` (it can exceed it when the citation cap fires)
  (Claude).
- **Coverage fix — dense single periods no longer truncate (PR #33).** A single calendar period
  (always the case for a Daily report, and for short Custom ranges) was one map group, and map-reduce
  only fanned out *across* groups — so a dense day's frames were crammed into one 8192-token pass and
  silently trimmed ("more was captured than summarized"). Each over-large period is now split into
  pass-sized sub-batches before the map step, so a dense day fans out into several passes and the
  reduce folds them back: within-period coverage is complete, not just per-period. The collapse-to-one-
  pass fast path now fires only when the whole report genuinely fits one window. New kernel regression
  test.

### 0.2.0 PR3 — Attention-first text filtering
Replaces PR2's `content_text` passthrough with a real span-aware filter so search, Ask, and
embeddings stop ranking on chrome (toolbars, taskbar, desktop icons, background windows). Searching
"Firefox" / "Steam" no longer surfaces frames purely because those static labels were on screen
(`03 §3b/§4/§8`, `docs/0.2.0.md` PR3). Top risk by design is **false suppression (silent data
loss)**, so the filter is conservative and fully recoverable.

- **New `textfilter` crate — a pure, deterministic classifier.** Groups OCR words by `line_index`
  and assigns each line one of five roles (`system` / `background` / `chrome` / `content` /
  `unknown`) by **geometry before repetition**: a line is `background`/`system` only with a
  confident foreground-window rect; the window title is treated as metadata (never repeated body
  text); long, information-rich lines are **never** catalogued or suppressed for merely repeating.
  Default `content_text` = `content` + `unknown` inside the target window, built from classified
  spans (not string-subtraction). No I/O, no Windows APIs — unit-tested with an anonymized synthetic
  OCR fixture.
- **Static-chrome suppression with a guardrail.** A line's signature
  (`app_hint ⏐ normalized_text ⏐ region_bucket`) is marked chrome only after it has been seen
  `text.chrome_suppress_min_seen` times **and** is shorter than `text.chrome_protect_min_chars`
  **and** the foreground-window rect is known (an unknown rect can't tell a short toolbar label
  from short body text, so it never catalogs or suppresses — keeping the invariant that a missing/
  wrong rect can only *under*-suppress, never silently lose content). All thresholds are
  **settings, never hardcoded** (`chrome_suppress_min_seen` 12, `chrome_protect_min_chars` 48,
  `chrome_region_buckets` 8).
- **Filtered write in one transaction.** New `insert_ocr_filtered` classifies, writes the filtered
  `content_text` directly (so the content FTS index is written **once** — no transient unfiltered
  window a concurrent search could match), stores the classified spans, and bumps the chrome
  catalog. Embeddings run over the filtered text because the embed job is enqueued only after this
  commits. `raw_text` is always preserved; `include_chrome` recovers suppressed terms.
- **Capture learns the foreground window.** Each frame carries a normalized `target_rect` mapped
  from the foreground window's visual bounds (`DwmGetWindowAttribute` extended-frame bounds) against
  the captured monitor's `rcMonitor`; `None` (other monitor / minimized / unresolved) is the safe
  fallback that disables positional suppression.
- **Per-app suppression-rate readout.** New `get_text_filter_stats` command + a read-only per-app
  list in Settings (all view states) surface the silent-data-loss alarm; the Recall search gains an
  `include_chrome` toggle.
- **Schema `schema_version` 3 → 4 (forward-only).** Adds `text_spans.line_index` so the classifier
  groups lines exactly (robust on multi-column layouts). `filter_version` is now `1`; bumping it
  wipes the chrome catalog so suppression re-learns. No backfill of old frames (clean-DB
  assumption). ts-rs bindings regenerated.
- **Review fixes (PR #32).** (1) Removed the N+1 catalog lookup on the OCR hot path — `seen_count`
  was one `SELECT` per candidate line; `insert_ocr_filtered` now pre-fetches the foreground app's
  catalog rows in a single bulk query into a `HashMap` (Claude/Gemini). (2) An unknown `target_rect`
  no longer lets repetition suppress short body text — static-chrome cataloguing/suppression now
  requires a known rect, with a regression test asserting a catalog-saturated signature survives a
  rect-less frame (Codex P2).

### 0.2.0 PR2 — Text-signal data model + OCR spans
The schema, types, and OCR geometry the attention-first retrieval pipeline needs (`03 §3/§3b/§4`,
`docs/0.2.0.md` PR2). No behaviour change for users yet — `content_text` is a passthrough copy of
`raw_text` until PR3's classifier lands (clean-DB assumption, `07` #51).

- **OCR now emits per-word spans.** `crates/ocr` walks WinRT `Lines().Words()` and emits one
  `TextSpan` per word with its `BoundingRect` normalized to `[0,1]` (origin top-left). WinRT exposes
  no per-word confidence, so the `CONFIDENCE_UNKNOWN` sentinel stays; PR2 spans are role `unknown`
  (PR3 classifies). Verified live on real WinRT OCR (gated test asserts every bbox is in `[0,1]`).
- **Schema `schema_version` 2 → 3 (forward-only).** New `frame_text` (preserved `raw_text` +
  filtered `content_text` + `primary_source`/`filter_version`/`suppressed_count` + foreground
  metadata), `frame_text_fts` (content-text FTS) and `frame_text_raw_fts` (raw FTS), per-word
  `text_spans` (normalized geometry + role, CHECK-constrained), and `chrome_text_catalog` (PR3's
  suppression counter). Every per-frame table cascades on frame delete. On a clean DB the legacy
  `ocr_text` table is dropped — `frame_text` is the single text store (`03 §4`).
- **New types.** `TextSource` / `TextRole` / `SuppressReason` enums + `TextSpan`; `OcrResult` gains
  `spans`; `FrameDetail` replaces `text` with `raw_text` + `content_text` + `text_source` +
  `suppressed_text_count`; `SearchQuery` gains `include_chrome` (default `false`). ts-rs bindings
  regenerated.
- **Default retrieval is now content text.** Hybrid search's FTS arm runs over `content_text`;
  `include_chrome=true` adds a raw-text FTS arm (chosen over a role-filtered spans FTS — roles
  aren't populated until PR3). Embeddings and Ask grounding read `content_text`. The FTS fallback is
  never removed.
- **Moment view** surfaces `content_text` with `raw_text` always viewable via a disclosure — raw is
  preserved and viewable even though search defaults to content.

### Docs — Planned 0.2.0 roadmap: attention-first text retrieval + recall reports
- **Added `docs/0.2.0.md` (now tracked).** Plans the 0.2.0 line. Core fix: today the app indexes
  raw full-screen OCR with no filtering, so search / Ask / embeddings get dominated by static
  chrome (toolbars, taskbar, desktop icons, the app's own sidebar labels). 0.2.0 derives a filtered
  **content-text** layer used by default; raw full-screen text is preserved and stays searchable
  opt-in via `include_chrome`.
- **Scope:** PR1 specs → PR2 data model + OCR bounding-box spans → PR3 attention-first filtering
  (clean DB, no backfill) → PR6 Recall reports + premade prompt cards → PR7 audit. Event-driven
  capture, UIA text, and smart enrichment throttling are deferred to **0.2.1**. Each PR recycles
  `specs/04_CLAUDE_CODE_BUILD_PROMPT.md` as its operating prompt.
- **Planning doc only — no runtime or code changes.** Also un-ignored `docs/0.2.0.md` in
  `.gitignore` so the roadmap is version-controlled.
- **Added PR8 (parallel model download) to the 0.2.0 line.** Promotes the previously-deferred
  single-stream download-speed limitation to a scheduled, individually-gated PR: a multi-connection
  chunked downloader (N parallel HTTP `Range` requests → one pre-allocated file) that reuses the
  existing progress / resume / stall / clean-layout machinery in `crates/inference/src/download.rs`.
  Independent of the retrieval chain, sequenced last. Planning only — no runtime or code changes
  (`docs/0.2.0.md` PR8; the `specs/07_KNOWN_GAPS.md` item is relabeled from a deferred 0.1.1 TODO to
  scheduled 0.2.0 PR8).

### Docs — 0.2.0 PR1: specs contract for attention-first content text + Recall reports
- **Wrote the 0.2.0 contract into `/specs/` (specs-only; no runtime or code changes).** The roadmap
  in `docs/0.2.0.md` is now the authoritative spec contract that PR2–PR7 implement:
  - `specs/02 §5b` — the **0.2.x arc** (P6: attention-first text signal + recall workflows), framed
    as a post-1.0 arc, **not** retrofitted into the P0–P5 v1.0 framing.
  - `specs/03` — raw vs **content text** (filtered OCR/UIA, *not* vision descriptions; the default
    retrieval input), active/target-window semantics, text spans + roles, static chrome suppression;
    the `frame_text` / `text_spans` / `chrome_text_catalog` schema (`schema_version` 2→3);
    `TextSource` / `TextRole` / `SuppressReason` plus `OcrResult.spans`,
    `FrameDetail`, and `SearchQuery.include_chrome`; and `generate_report` reports (`§8b`). Default
    search stays **hybrid (FTS + vector) over content text**; raw text is preserved but opt-in via
    `include_chrome`.
  - `specs/UI_REFERENCE.md` — Recall = **Search / Ask / Reports**, content-text default + raw/chrome
    toggle, premade Ask cards, all five view states for the new/changed screens.
  - `specs/07` — deferrals (event-driven capture, UIA text, smart enrichment throttle → 0.2.1;
    scheduled reports) and the **PR2→PR3 interim**: `content_text` is a passthrough copy of
    `raw_text` until PR3's filter lands, with no backfill (clean-DB assumption).
  - `specs/04` — `docs/0.2.0.md` added to the reading order; the PR1→PR2→PR3→PR6→PR7 build order
    appended alongside P0–P5.
  - Review-round hardening (PR #30): `suppress_reason` is now `Option<SuppressReason>` (no redundant
    in-enum `None`, mapping the nullable column); `frame_text` / `text_spans` DDL gained `CHECK`
    constraints on the enum columns; `reports.map_reduce_min_frames` default lowered 40→20 (worst-case
    single-pass fit, so frames batch rather than drop before the 8192 answer context overflows); and
    `docs/0.2.0.md`'s status now records PR1 as complete (PR2 next), resolving the roadmap/contract
    contradiction now that `04` makes the roadmap mandatory reading.

## [0.1.0] — 2026-06-24

### Packaging — First public release: standalone unsigned Windows installer
- **First tagged release (`v0.1.0`).** Ships a standalone **NSIS installer** (`.exe`) produced by
  `tauri build`. `bundle.targets` is pinned to `["nsis"]` (was `"all"`) so the build emits a single
  installer and does not require the MSI/WiX toolchain.
- **Version bumped `0.0.0` → `0.1.0`** across the workspace `Cargo.toml`, `src-tauri/tauri.conf.json`,
  and both `package.json` files (root + `ui/`).
- **The installer carries only the ~11 MB app binary.** The llama.cpp sidecar and all GGUF / embedding
  models download into the per-user app-data folder (`%APPDATA%\app.screensearchv2c.desktop\`) on first
  run — nothing heavy is bundled. ONNX
  Runtime is **statically linked** into the binary via `ort`'s `download-binaries` (not `load-dynamic`),
  so no separate `onnxruntime.dll` is shipped.
- **Unsigned by design.** Windows SmartScreen warns on first launch (*More info → Run anyway*); a
  code-signing certificate remains a follow-up (see `specs/07_KNOWN_GAPS.md` #26). First run requires
  internet and downloads ~8–10 GB of models; a Vulkan-capable GPU is recommended (CPU fallback exists).
- Everything below (P0–P5: capture, data spine, OCR/embeddings, vision + sidecar inference, the
  Command-Deck UI) is part of this first release.

### Fixed — Quality (8B) vision download stuck in a retry storm (2026-06-24)
- **Selecting the Quality vision tier could leave the model perpetually "downloading" / failing.**
  Root cause (reproduced deterministically): nothing prevented a *second* app instance from running
  against the **same** app-data dir — and users relaunch/spam when nothing seems to be happening — so
  two processes raced hf-hub's per-model cache lock. The loser failed with `LockAcquisition` after
  ~5 s and the vision scheduler retried instantly (a download "storm"); the largest model (8B) was the
  most exposed because it holds the lock longest. Two complementary fixes:
  - **Single-instance enforcement** (`tauri-plugin-single-instance`, registered first): a second
    launch focuses the running window and exits, so only one ScreenSearch ever touches the SQLite DB
    and the model cache (also prevents DB corruption / double-capture). (`src-tauri/src/lib.rs`.)
  - **Resilient downloader**: a `LockAcquisition` during any residual contention now **backs off and
    retries** (linear, bounded) instead of hard-failing into a storm. A lock left by a force-killed
    process is released by the OS, so this only ever waits on a genuinely live holder.
    (`crates/inference/src/download.rs`.)

### Fixed — Settings opened a flashing console window (2026-06-24)
- **Opening Settings briefly flashed a terminal window.** Enumerating GPU devices for the inference
  panel ran `llama-server --list-devices` as a console process without Windows' `CREATE_NO_WINDOW`
  flag, so a console flashed each time Settings opened (alarming, looks malware-ish). Now suppressed,
  matching the sidecar/`probe_caps` spawns. (`src-tauri/src/lib.rs::list_devices_from_binary`.)

### Known limitation — first-run model download speed (RESOLVED in 0.2.0 PR8)
- Models used to download **single-stream** via hf-hub (~20 MB/s; HuggingFace's CDN throttles single
  connections), so the ~5 GB Quality vision model took a few minutes. **Resolved by the 0.2.0 PR8
  multi-connection chunked downloader above** (single-stream remains the fallback for a range-less
  server).

### Added — Model-tier tooltips (2026-06-24)
- **The Default / Quality / Beta tier buttons now show which model they map to** on hover and
  keyboard focus (e.g. Answer → Quality → "Qwen3-4B-Thinking"), so the tier names aren't opaque.
  The names mirror `crates/inference/src/models.rs::repo_for`. (`ui/src/components/domain/ModelTierPicker.tsx`.)

### Fixed — Review hardening (PR #26): size-probe timeout, broadcast lag, orphan pidfile (2026-06-24)
- **The HF tree-API size probe can no longer hang the download.** `total_download_bytes`
  built a `reqwest::Client` with no timeout, and it runs *before* the stall watchdog is
  spawned — so a hung API would block the download from ever starting. It now uses a 15 s
  timeout (`HTTP_API_TIMEOUT`); the size is best-effort anyway and the download proceeds
  regardless. (`crates/inference/src/download.rs`.)
- **The status/download event bridges survive a lagging receiver.** Both
  `while let Ok(_) = rx.recv().await` broadcast loops terminated on *any* error, including
  `Lagged` — a burst of events would silently kill the bridge and freeze the StatusRail /
  download chip for the rest of the session. They now skip on `Lagged` and stop only on
  `Closed`. (`src-tauri/src/lib.rs`.)
- **A surviving sidecar is no longer stranded as an orphan.** `kill_and_confirm` removed the
  pidfile unconditionally; if the process outlived the kill, the next launch's
  `reap_stray_any` then had no record to clean it up. The pidfile is now kept when the
  process is still alive (and only removed once it has exited). (`crates/inference/src/supervisor.rs`.)
- **The Ask context budget no longer undercounts non-Latin text.** The token estimate used a
  `chars/3` ratio, which *under*-counts dense scripts (a CJK character is ~3 bytes yet ≈1–1.5
  tokens), so CJK OCR snippets could admit several× too much context and re-trigger the very
  `exceed_context_size_error` the budgeting prevents. It now estimates from UTF-8 *bytes* at a
  conservative **2 bytes/token** upper bound (`BYTES_PER_TOKEN`) — safe for both Latin and the
  worst-case Mistral-family CJK tokenizer of the default answer model — and truncates on char
  boundaries. (`crates/inference/src/answer.rs`.)
- **Idle keep-warm can no longer lose a race with the evictor.** `maybe_evict` sampled the
  `backfill_active` / `pinned` keep-warm flags *before* taking the state lock but only re-checked
  `in_flight` *after* — so a keep-warm set in that window still evicted the model, reintroducing
  cold-start churn mid-drain. All three keep-warm signals are now re-checked under the lock.
  (`crates/inference/src/supervisor.rs`.)
- **The binary-download network calls can't hang either.** `resolve_binary_url` (GitHub releases
  JSON) gained the same 15 s request timeout, and `http_get_bytes` (the multi-MB llama.cpp zip)
  gained a 15 s **connect** timeout **plus a 30 s per-read-chunk `read_timeout`** — so a CDN that
  accepts the socket then goes silent mid-transfer fails fast (the binary path has no separate
  stall watchdog) without aborting a slow-but-progressing download. (`crates/inference/src/download.rs`.)
- **A small `ctx_size` no longer breaks grounded Ask.** With a low `sidecar.ctx_size` (Settings
  allows down to 512) the fixed 2048-token reply budget could reserve the *entire* window, so every
  Ask dropped all retrieved snippets and answered "(no relevant snippets found)". The reply budget
  is now capped to half the context window, leaving room for grounding; normal 4K/8K windows are
  unaffected. (`crates/inference/src/answer.rs`.)
- **A slow disk no longer makes a download falsely "stall".** Publishing a cached or
  just-downloaded multi-GB blob into the clean layout is local disk I/O, but it ran under the 180 s
  *network* stall watchdog — on a slow disk the copy could exceed it and report a spurious failure.
  The watchdog now pauses its stall counter during the clean-layout copy phase. (`crates/inference/src/download.rs`.)
- **Documented (not yet fixed):** a stall-abort can't cancel hf-hub 0.4.3's *detached* per-chunk
  tasks; the blast radius is bounded (committed-counter resume + per-lane serialization) and a real
  fix needs a cancellable downloader we own — tracked as known gap #46 (`specs/07_KNOWN_GAPS.md`).

### Fixed — "Ask" answer truncated to nothing; download now visible from anywhere (2026-06-24)
- **The Ask answer is no longer cut off / "always thinking."** The default answer models are
  *reasoning* models that stream a `<think>…</think>` trace before the answer; the reply budget was
  `ASK_MAX_TOKENS = 512`, which the reasoning alone exhausted — so generation was truncated
  mid-thought and the answer area stayed empty (only the Thinking box rendered). Raised to **2048**,
  enough to finish reasoning *and* produce the answer while still leaving most of the 8K context
  window for retrieved snippets. (`ui/src/routes/Recall.tsx`.)
- **Model downloads now show in the top status rail**, not just on the Settings page — a "↓ 42%"
  chip (with a model/size tooltip) appears whenever a fetch is in progress, so the multi-GB download
  is visible from any screen. (`ui/src/components/shell/StatusRail.tsx`, new `IconDownload`.)

### Fixed — Model download could hang on a resumed/incomplete fetch; progress bar was untruthful (2026-06-24)
- **The Inference-engine download no longer hangs on an interrupted prior attempt, and the
  progress bar now shows *real* progress.** Root cause (proven against the live partial 8B fetch):
  the previous bar polled the on-disk file size, but hf-hub `set_len`s the `.sync.part` to the file's
  full length *up front* — so the bar jumped to ~81% (≈5 GB allocated of 6.2 GB) while only **33%**
  had actually downloaded, then froze. The fetch itself stalled because hf-hub 0.4.3 builds a
  `reqwest` client with **no socket timeout** and we ran it with no retries, so a dead CDN connection
  blocks forever. Fixes:
  - Progress is now driven by hf-hub's `Progress` callbacks (true streamed bytes, **resume-aware** —
    a restart picks up where it left off), not the pre-allocated file size.
  - A **stall watchdog** aborts a download that receives no new bytes for 180 s and surfaces a clear,
    retryable error (`"download stalled — no data received; click Load to resume"`) instead of hanging
    indefinitely; the partial is kept on disk so the next Load/tag **resumes** from there.
  - A finalized-but-not-copied cached blob is now reused instead of re-downloaded (cache pre-check).
  - The **Inference engine** panel shows a failed-download banner with the reason and resume hint.
  (`crates/inference/src/download.rs`, `ui/src/components/domain/ModelPanel.tsx`.)

### Added/Fixed — Model-download progress, download reliability, load-pinning, thinking auto-scroll (2026-06-24)
- **Model downloads now show progress and surface errors.** A new coordinated
  `ModelDownloader` broadcasts `model_download` events (`downloaded_bytes` / `total_bytes` from the
  HF tree API); the **Settings → Inference engine** panel renders a live progress bar (e.g.
  "Downloading vision model… 42% · 2.6 / 6.2 GB") and toasts on completion/failure. Previously a
  multi-GB fetch was opaque network activity with no feedback. (`crates/inference/src/download.rs`,
  `ui/src/components/domain/ModelPanel.tsx`.)
- **Quality vision tagging no longer dead-letters from concurrent download races.** Downloads are
  now serialized per lane, so multiple enrichment workers can't fetch the same multi-GB model at
  once (the race that left `vision_tag` jobs `dead` with "download vision model"). The model
  downloads once (with progress); jobs then tag normally.
- **A manually loaded model stays loaded.** "Load model" now *pins* the model so the idle-TTL won't
  evict it (the "evicted right after it downloaded" surprise); "Unload" (or switching models) clears
  the pin. (`crates/inference/src/supervisor.rs`.)
- **The answer's "Thinking" trace auto-scrolls** and is height-bounded, so a long reasoning stream
  follows the latest line instead of overflowing the view (it stays put if you scroll up to re-read).
  (`ui/src/components/domain/AnswerStream.tsx`.)

### Fixed — Idle backfill, sidecar reload/status, and the "Ask" context-overflow failure (2026-06-24)
- **Idle vision tagging now drains the whole backlog while the PC is idle, instead of one batch
  then dormant.** The idle scheduler used to enqueue a single batch *only* on the transition into
  idle, so a long idle period processed one batch and stopped. It now keeps topping the queue up to
  a batch whenever in-flight vision work falls below a low watermark, and tells the sidecar to stay
  loaded (keep-warm) for the duration of the drain. When the backlog is empty or the user returns,
  keep-warm is released so the normal idle-TTL eviction frees the VRAM. The timer trigger is
  unchanged. (`crates/kernel/src/vision_scheduler.rs`, new `Store::pending_vision_job_count`.)
- **The sidecar idle-TTL no longer fights idle backfill.** A new keep-warm flag
  (`BackfillControl`, implemented by the supervisor) suppresses idle eviction *only* while the idle
  backfill is actively draining, so the model isn't evicted between batches. Extracted a pure
  `should_evict` predicate (unit-tested). (`crates/inference/src/supervisor.rs`, `crates/traits`.)
- **Model status no longer lies about "Ready".** An idle-evicted sidecar used to map to
  `ComponentStatus::Ready`; it now maps to a neutral `Disabled`, and the UI labels the engine from
  the *raw* `SidecarState` — "Loaded" / "Loading…" / "Idle — unloaded" / "Error" — in both the
  StatusRail chip and a new **Settings → Inference engine** panel. `SidecarStatus` gained a `lane`
  field so the panel shows vision vs. answer truthfully. (`crates/kernel/src/lib.rs`,
  `crates/traits/src/ipc.rs`, `ui/src/components/domain/ModelPanel.tsx`, `ui/src/lib/status.ts`.)
- **Manual Load / Unload controls.** The Inference engine panel can pre-load the answer model (so
  the next Ask is instant) or unload the resident model now to free VRAM, via new `load_model` /
  `unload_model` commands backed by `ModelSupervisor::{preload, unload}`.
- **"Ask" no longer fails with the opaque "sidecar returned an error status."** Root cause (verified
  by reproducing it directly against `llama-server`): the grounded prompt concatenated *all*
  retrieved chunks with no token budget, so a large context exceeded the model's window and
  llama-server returned `HTTP 400 exceed_context_size_error` — which the client swallowed. Fixes:
  (1) the client now surfaces the real HTTP status + response body; (2) `build_messages` budgets the
  included context to the model's `ctx_size` (reserving the reply + prompt overhead), dropping or
  truncating chunks that don't fit and citing only the frames actually included.
  (`crates/inference/src/client.rs`, `crates/inference/src/answer.rs`.)
- **Eviction/unload now verify the kill.** Every sidecar teardown (idle eviction, manual unload,
  model switch, shutdown) routes through a shared `kill_and_confirm` that waits for the process to
  exit and logs `killed` / `still_alive`, so a kill that fails to free VRAM is visible rather than
  silent. (`crates/inference/src/supervisor.rs`.)

### Added — Sidecar memory tuning: pinned context + KV quantization + flash attention (2026-06-24)
- **The llama.cpp sidecar now pins a small, workload-appropriate context window and quantizes its
  KV cache**, instead of inheriting the model's full trained context. `build_args` previously passed
  no `--ctx-size`, so llama.cpp defaulted to `0` ("loaded from model") — for the default models that
  is a **262 144-token** trained context, and the server auto-sized a ~118 k-token KV cache that
  alone consumed most of a 16 GB GPU. New balanced defaults: context **auto** (vision 4096 / answer
  8192), KV cache **q8_0**, flash attention **auto**.
  - **Measured (RTX 5060 Ti, bundled Vulkan `llama-server`, default answer model):** peak VRAM for
    the answer model dropped from **14082 MiB** (untuned, `n_ctx` 118272) to **4253 MiB** (tuned,
    `n_ctx` 8192) — a **~9.8 GB / ~70%** reduction, with no expected quality loss for this workload.
- **Three new user-adjustable settings** (Settings → Sidecar (advanced)), all applied on the next
  sidecar launch via the existing relaunch-on-next-request path:
  - `sidecar.ctx_size` (`Settings.sidecar_ctx_size`, default **0 = auto**, else clamped
    **512–32768**) — `0` substitutes a per-lane default (vision 4096 / answer 8192); any other value
    overrides both lanes.
  - `sidecar.kv_cache_type` (`Settings.sidecar_kv_cache_type`, new `KvCacheType` enum: `f16` /
    `q8_0` / `q4_0`, default **q8_0**) — quantized KV is emitted only when flash attention is active.
  - `sidecar.flash_attn` (`Settings.sidecar_flash_attn`, new `FlashAttnSetting` enum: `auto` / `on` /
    `off`, default **auto**).
- **Version-safe flag emission.** The bundled `llama-server` is the *latest* llama.cpp Vulkan release
  fetched at runtime, so its flag spelling varies (`--flash-attn` bare boolean vs. `on|off|auto`;
  quantized `--cache-type-v` needs flash attention). A new `inference::flags::probe_caps` reads the
  binary's `--help` once at init; `build_args` emits only verified flags and silently degrades (e.g.
  drops KV quantization when flash attention is unavailable) rather than producing an arg list the
  binary would reject.
- **Review-round hardening (PR #25):**
  - `FlashAttnSetting::Auto` now emits `--flash-attn auto` (defer to llama.cpp) on value-taking
    binaries instead of forcing `on`, so `Auto` and `On` are observably distinct; `On` still forces
    `on`. Both keep flash active for the purpose of unlocking quantized KV.
  - The `probe_caps` `--help` call (a blocking syscall) now runs on `spawn_blocking` so it never
    parks the async executor, falling back to the conservative flag set if the task fails.
  - `push_kv_cache` warns when quantization is configured but the binary advertises neither
    `--cache-type-k` nor `--cache-type-v` (previously a silent f16 fallback).
  - The `--flash-attn` capability probe also recognizes a parenthesised value set (`(on/off/auto)`).
  - **Round 2 (diagnostics + robustness):** `push_flash_attn` now returns a `FlashState`
    (`Active` / `BinaryUnsupported` / `UserDisabled`) so the quantized-KV downgrade warning names the
    real cause — a binary limitation vs. the user turning flash attention off — instead of always
    blaming the build. An explicit `flash_attn = on` on a binary that lacks the flag now logs a warn
    rather than being dropped silently. And `set_settings` sanitizes the incoming `Settings` *before*
    building `SidecarParams`, so a direct (non-UI) IPC call with an out-of-range `sidecar_ctx_size`
    can no longer run the raw value in the next sidecar spawn while the DB stores the clamped one.

### Fixed — Vision scheduler: configurable batch size + pending-job dedup (2026-06-24)
- **The timer/idle vision batch size is now a setting.** It was a hardcoded `const BATCH = 20`, so
  the scheduler logs showed a fixed `count=20` every tick with no way to drain a backlog faster.
  Added `enrich.vision_batch_size` (`Settings.enrich_vision_batch_size`, default **20**, clamped
  **1–500**) wired through `load`/`save`/`sanitize`, surfaced as a "Frames per run" field in the
  Settings → Enrichment schedule control, and read fresh each run so it applies on the next
  scheduled tick.
- **The scheduler no longer re-enqueues frames that already have an in-flight `vision_tag` job.**
  `untagged_frame_ids` only filtered on "no `vision_analysis` row", so every tick re-queued the same
  oldest-untagged frames while their jobs were still `pending`/`running`, and the timer + idle lanes
  double-queued the same frames when they fired together. Added a `NOT EXISTS` guard on
  `jobs(kind='vision_tag', state IN ('pending','running'))` so a trigger enqueues only genuinely
  eligible frames; a finished (`done`) job or a job of another kind still leaves an untagged frame
  eligible. Resolves the "batch size N" and "no pending-job dedup" threads of gap #19.
- **Review hardening (PR #23):**
  - **No infinite retry of poisoned frames.** The dedup now also excludes frames whose `vision_tag`
    job `dead`-lettered (exhausted retries without writing a `vision_analysis` row); previously such a
    frame was re-enqueued every tick forever. On-demand single-frame re-tagging still bypasses the
    query, so a dead frame can be force-retagged.
  - **Indexed the dedup lookup.** Forward migration `schema_version` → **2** adds
    `idx_jobs_frame_kind_state` on `jobs(frame_id, kind, state)` so the correlated `NOT EXISTS` doesn't
    scan the whole `jobs` table per candidate frame.
  - **Atomic scheduled enqueue.** The timer and idle producers now share an async mutex across their
    read-then-enqueue, so a simultaneous wake can't both read the same frames before either inserts —
    closing the residual timer/idle double-queue race.

### Docs — README refresh + visual showcase (2026-06-24)
- **Added a Screenshots section** to `README.md` showing five live Command-Deck screens (Deck,
  Recall/Ask, Insights, Moment, Timeline). Renamed the committed `screenshots/screensearch_*.png`
  captures to descriptive names (`deck.png`, `recall-ask.png`, `insights.png`, `moment.png`,
  `timeline.png`).
- **Fixed the Build & run order.** The Rust build was listed before the UI build, but
  `src-tauri`'s `generate_context!` embeds the git-ignored `ui/dist`, so cargo fails unless the UI
  is built first. Reordered to UI → Rust, switched `cargo build` to `cargo build --workspace`, added
  the `npm run lint` Rules-of-Hooks gate and the `git diff --exit-code -- ui/src/bindings` binding
  guard — matching CI.
- **Linked the live-verification audit** (`docs/AUDIT_V2C_2026-06-23.md`) from the status blockquote
  and added `AGENTS.md`, `screenshots/`, and the audit doc to the repository-layout map.
- **Aligned the status with the audit.** Reworded "P0–P5 complete" to "P0–P4 complete and verified;
  P5 UI feature-complete with keyboard/state/a11y verification in progress," marked the P5 row
  `🚧 Feature-complete; UI verification in progress`, and named the open UI gaps (the keyboard/state/
  a11y matrix and a no-evidence answer still rendering cited-frame tiles).

### Docs — Added V2c audit artifact (2026-06-23)
- Added `docs/AUDIT_V2C_2026-06-23.md`, recording the non-packaging audit pass, including CI
  gates, hardware/model smokes, GPU runtime evidence, and the later live GUI follow-up for
  synthetic capture/search/Ask, no-evidence refusal, persisted Moment VLM analysis, and P5
  route/state/a11y observations.
- PR #21 review follow-up mirrors the audit follow-up into `specs/05_BUILD_REVIEW.md`,
  `specs/06_PATCH_PLAN.md`, and `specs/07_KNOWN_GAPS.md`, and clarifies the local Node 26
  verification caveat against CI's Node 22 runtime.

### Fixed — P5 comprehensive review hardening (2026-06-23)
- **Date ranges now use local calendar midnights.** Deck, Timeline, Insights, and Recall no longer
  derive "today" / trailing windows from fixed 24-hour offsets, avoiding DST drift.
- **Navigation and IPC are bounded.** Timeline nearest-frame opens are constrained to the selected
  range, while frame-list, frame-context, Timeline, and Insights command sizes are clamped for direct
  IPC callers.
- **Operational state is explicit.** Deck panels now show retryable error states, the Command
  Palette exposes combobox/listbox ARIA semantics, Timeline canvas colors come from CSS tokens, and
  backend `toast` / richer `job_completed` events drive more precise UI refreshes.
- **Storage and retention are real.** `storage.retention_days` is enforced at startup and hourly in
  safe bounded batches, and StatusRail displays DB, frame, and total storage size via
  `get_storage_stats`.
- **Streaming answers are request-scoped and cancellable.** `answer_delta` carries a request id,
  superseded asks are cancelled, and stale deltas are ignored by the UI.
- **Settings are more live and discoverable.** Embedding workers reconfigure when enrichment lanes
  change, monitor selection uses real `get_monitors` data with a manual fallback, Timeline/Insights
  bucket counts adapt to chart width, and advanced llama.cpp device selection uses
  `--list-devices` / `--device` through the optional `sidecar.device` setting.
- **PR #19 review follow-up:** fixed ask-task cleanup race, made retention continue past a
  per-frame DB delete failure, corrected monitor toggling from the default "all monitors" state,
  refreshed sidecar devices when readiness becomes ready, removed redundant device controls, and
  clarified that capture enqueue settings apply on the next capture start while worker claiming
  updates after save.
- **PR #19 Codex follow-up:** enabling image embeddings after startup now reloads the FastEmbed
  provider with the image lane instead of letting `embed_image` jobs fail until restart, and
  retention removes frame files before DB rows so transient Windows file locks do not create
  permanent orphan JPEGs.
- Packaging remains deferred as the separate DoD §13.9 follow-up.

### Docs — Refreshed root `CLAUDE.md` to current state (2026-06-23)
- **Corrected the stale "current state" headline.** It claimed *"specification complete, no
  application code yet — the build starts at P0"*; it now reflects reality: **P0–P5 complete and
  merged to `main`, in post-merge hardening**, with a 9-crate Rust workspace + React/TS UI.
- **Fixed the app run/build command.** Replaced `cargo tauri dev` / `cargo tauri build` (no
  `cargo-tauri` installed) with `npm run tauri dev` / `npm run build`.
- **Made Build/verify match CI.** Documented the UI-before-cargo order (`generate_context!` embeds
  the git-ignored `ui/dist`), the `npm run lint` gate, `--all-targets` on clippy, `--workspace` on
  build/test, the `git diff --exit-code -- ui/src/bindings` binding guard, and the Rust 1.82 / Node
  22 toolchain.
- **Added a compact "Where the code lives" crate map** so an agent can orient without reading the
  full spec. No code or behavior changes.

### Fixed — P4 sidecar hardening (2026-06-23)
- **Model switches no longer cut off active sidecar requests.** `ModelSupervisor` now serializes
  sidecar leases, so a lane/tier switch waits for the current answer stream or vision tag request to
  finish before killing and replacing the running `llama-server`.
- **Same-model reuse validates the running sidecar.** Before handing out an existing process, the
  supervisor checks both OS liveness and bounded `/health`; a dead or unhealthy sidecar is marked
  crashed, stopped, and respawned for the current request.
- **Sidecar HTTP calls now have deadlines.** Health checks, non-streaming vision completions, and
  streaming answer waits are bounded so a hung localhost sidecar returns an error instead of pinning
  a worker or answer stream forever. Vision jobs retry through the durable queue; answers surface a
  terminal error delta.
- **`SSV2C_LLAMA_RELEASE_URL` really overrides existing installs.** The override now resolves before
  normal install reuse and extracts into a URL-specific override directory, preserving the app-managed
  Vulkan release.
- **PR #18 review follow-up:** sidecar leases now use shared/exclusive gate semantics, so same-model
  requests can run concurrently while model switches and crash recovery still drain all request slots
  before stopping `llama-server`. Stream connection and SSE-idle deadlines are split, override zip
  extraction is staged atomically in a `.partial` directory on a blocking thread, and startup reap now
  recognizes exact binaries from both normal and URL-specific override installs. A second review
  follow-up emits the observed crashed model in a crash-recovery/model-switch race and runs installed
  binary reaping before override download, so a bad override URL cannot strand an old sidecar.

### Fixed — P3 deferred enrichment hardening (2026-06-23)
- **Vision-only enrichment no longer waits on embeddings.** The worker pool now starts from either
  provider slot: embedding jobs are claimed only when an embedder is attached and enabled, while
  `vision_tag` jobs drain as soon as inference attaches. This fixes stalled vision backlogs when text
  and image embeddings are disabled or unavailable.
- **Backend search limits are clamped.** `SearchQuery.limit` was initially normalized to `1..=100`
  with a 500-row candidate pool, matching the Recall UI and protecting direct IPC callers from
  oversized hydration work. PR6 report retrieval later widened the backend cap to `1..=2,000` with a
  2,000-row candidate pool so larger bounded reports can collect enough evidence.
- Added regression coverage for vision jobs draining without an embedder, excessive search limits,
  and zero-limit normalization. Updated P3 architecture/spec notes and the live-event comment to
  match the current worker/event behavior. No schema, IPC, `ts-rs`, or trait signature changes.
- **PR #17 review follow-up:** the worker loop now builds claim kinds in a fixed stack array instead
  of allocating a `Vec` on every idle poll, the `job_progress` comment now says it fires after each
  job attempt completes, and the startup-scoped embedding-lane flags are tracked as a live
  reconfiguration follow-up.

### Fixed — P2 capture hardening (2026-06-23)
- **Capture readiness now clears after unexpected source shutdown.** If the WGC capture source exits
  without the user pressing Stop, the kernel removes the live capture handle and reports
  `capture = Error` with a detail instead of leaving the UI stuck on `Ready`.
- **OCR-unavailable machines no longer create empty OCR rows.** If WinRT OCR cannot be created (for
  example, no recognizer language is installed), the app still boots but capture start fails with
  `capture = Unavailable`; the defensive fallback provider now errors instead of silently returning
  empty text.
- **Backend settings are sanitized on load and save.** Direct IPC or hand-edited DB values are clamped
  to the same numeric bounds as the Settings UI, including a finite `[0,1]` capture diff threshold.
  Added regression tests for source shutdown, OCR-unavailable start refusal, and malformed persisted
  settings. No schema, IPC, or `ts-rs` binding changes.
- **PR #16 review follow-up:** the unexpected-source-shutdown supervisor now keeps the capture mutex
  held while publishing `capture = Error`, closing a restart race where a new capture session could
  report `Ready` and then be overwritten by the old loop's shutdown error.

### Fixed — P0/P1 store hardening (2026-06-23)
- **Job finalization now requires a claimed running job.** `complete_job` and `fail_job` no longer
  rewrite pending, done, or dead jobs by id alone. This protects the durable queue state machine from
  stale-worker finalization after a retry, dead-letter, or stale-running recovery.
- **Older builds reject newer database schemas.** Opening a SQLite store with a `schema_version`
  greater than the compiled migration set now fails clearly instead of reporting the DB as ready.
- **PR #15 review follow-up:** the future-schema guard now derives the supported version from the
  compiled `MIGRATIONS` set and debug-asserts that `LATEST_SCHEMA_VERSION` stays in sync. The
  future-schema regression test now uses `tempfile::tempdir`, with `tempfile` centralized in workspace
  dependency versions for Rust tests.
- **PR #15 Codex review follow-up:** the periodic enrichment stale-job sweep now skips while the
  current worker pool has in-flight jobs. A long but live provider call stays `running`, so its later
  retry/dead-letter failure is accounted by `fail_job` instead of being requeued out from under the
  worker. Startup recovery still requeues leftover `running` jobs before workers start.
- Added regression tests for pending/done/dead job finalization and future-schema rejection. No IPC,
  `ts-rs`, schema, or trait signatures changed.

### Fixed — Vision-tagging quality (2026-06-22)
- **Vision tags no longer record a fabricated confidence.** The vision prompt previously showed a
  literal `"confidence": 0.0`, which the model echoed — so every tag was stored with a `0.0`
  certainty that *looked* real. The prompt now describes the confidence field instead of demonstrating
  a value, and parsing treats `0.0` (and any out-of-range value) as the same `-1.0` "unknown" sentinel
  the OCR path uses — we never invent a score.
- **`activity_type` is now a closed set, not free-form.** Vision tagging is constrained with a
  `response_format` JSON schema (the sidecar turns it into a sampling grammar), and any label outside
  the known set — coding, browsing, email, reading, chat, terminal, design, video — including the
  model's own "unknown", is dropped to none rather than stored. Keeps the Insights activity breakdown
  meaningful. Resolves `07_KNOWN_GAPS.md` #19/#20.
- **Low-signal frames stay untagged instead of being mislabelled (PR #14 review).** The activity
  grammar now also permits `null`, and the field is no longer required — a blank desktop, a lock
  screen, or the synthetic two-tone smoke image no longer forces the model to pick one of the eight
  labels just to satisfy the schema (which had been skewing the Insights breakdown). The prompt tells
  the model to answer `null` rather than guess. Verified on a real GPU: the gated smoke's two-tone
  test frame now comes back `activity=none, confidence=unknown` — the model honestly declines —
  whereas the forced-enum schema had it confidently (and wrongly) report `browsing` at `0.95`.
- **`app_hint` "null" is dropped case-insensitively (PR #14 review).** A model that emits the text
  `"null"`/`"NULL"`/`"Null"` (instead of a JSON null) no longer leaks it as an application name; the
  surviving hint is also trimmed.
- **Unknown confidence renders as `n/a`, not `-100%` (PR #14 review).** The Moment detail view now
  shows a neutral `n/a` chip when the stored confidence is the `-1.0` sentinel, instead of multiplying
  it into a misleading `-100%`.

### Added — P5 (M5) Settings & Insights (2026-06-22)
- **Settings**: a single editable form over every persisted setting — capture (interval, change
  threshold, monitors), storage (JPEG quality, max width) and retention, model tiers, the answer
  "thinking" toggle, enrichment (text/image embeddings, worker concurrency) and the deferred-vision
  schedule, privacy (excluded apps, pause-on-lock), and advanced sidecar knobs. Saves are optimistic
  and reconcile against the backend; model tiers additionally **hot-apply the instant you pick them**
  (the running provider switches without waiting for Save). Every field states *when* it takes effect
  — tiers now, the thinking flag on your next question, capture/storage/privacy on the next capture
  start, enrichment/sidecar on restart — and retention is labelled honestly as recorded-but-not-yet-
  enforced. A failed save keeps your edits and explains.
- **Insights**: truthful activity analytics computed entirely from your own history — total and
  vision-tagged capture counts, a captures-over-time density chart, your top foreground apps, and the
  vision activity-type breakdown. No fabricated numbers: when there's nothing yet it says "not enough
  history yet", and while vision tagging is still catching up the activity breakdown is labelled
  "tagged only" with the count it's based on. Lightweight inline charts (no chart library) keep the
  bundle small — Settings and Insights are each their own lazily-loaded route chunk.
- New reusable controls: `ModelTierPicker`, `ScheduleControl`, `RetentionControl`, and the Insights
  `CapturesTrend` / `InsightsBars` charts.

### Added — P5 (M3+M4) Recall, Deck, Timeline & Moment (2026-06-22)
- **Deck (home)**: capture status with one-click start/stop, today's activity (capture count,
  density minimap, top apps), the enrichment-queue meter, and a "jump back in" grid of your most
  recent captures. Onboards from empty ("start capture"), and flags when captures are recorded but
  not yet enriched.
- **Recall (search + ask)**: hybrid search over your screen text/meaning, shown as a virtualized
  result list that links straight to the frame; and a grounded **Ask** that streams an answer with a
  collapsible "thinking" trace and clickable citation thumbnails to the source moments. Tells you
  honestly when it's "searching text only" (embeddings still indexing) or the answer model isn't
  loaded — never a dead end.
- **Timeline (the Scanline)**: the signature instrument — a canvas density ribbon over a chosen
  window (Today / 7 days / 30 days) with a sweeping signal-orange scan-head, hover thumbnails, and a
  faint scanline texture. Fully keyboard-operable (arrows scrub, Shift for big steps, Home/End jump,
  Enter opens the moment under the head); the open lands on the real nearest frame. Ambient motion
  honors "reduce motion".
- **Moment (one frame)**: the capture image, its recognized text, the vision description/tags and
  context, prev/next and a strip of nearby frames, plus an on-demand **"Tag with vision"** action
  when a frame hasn't been analyzed yet. Deep-linkable and resilient to a missing/deleted frame.
- **Frame browsing data** (`get_frames`, `get_nearest_frame`): the backend can now list frames in a
  window (newest-first) and resolve a point in time to the nearest captured frame — what the Timeline
  thumbnails, the Deck recents, and "Enter opens the moment" need. New typed `FrameMeta`.
- Every screen renders all of loading / empty / error / partial / populated — no mock data, no dead
  ends. Initial JS stays ~87 KB gzipped (the markdown renderer ships only with Recall).

### Fixed — P5 (M3+M4) review follow-ups (2026-06-22)
- **Moment Prev/Next & "around this moment" now work in real sessions.** They were sourced from a
  newest-first window that, in a busy capture session, returned only frames from the far edge of the
  window — so the buttons silently went dead and the strip showed unrelated frames. The backend now
  serves the captures immediately *bracketing* a moment (`get_frame_context`), so navigation always
  lands on the true neighbours.
- **Answer links open safely.** Links inside a streamed answer now open in your browser instead of
  navigating away inside the app window.
- **The answer's "Thinking" trace no longer snaps shut** the instant the answer finishes — it stays
  as you left it so you can read it.

### Added — P5 (M1+M2) UI foundation, shell & primitives (2026-06-22)
- **The "Command Deck" shell**: the app now opens to a real frame — a top StatusRail (live
  capture / data store / enrichment queue / inference status), a left NavRail (Deck · Recall ·
  Timeline · Insights · Settings), a ⌘K command palette (jump-to + start/stop capture), and a
  readiness banner that appears only while something is starting or needs attention.
- **Design system**: warm-graphite dark theme with a single signal-orange accent and Windows-native
  fonts, defined once as design tokens — every color, font, radius, and spacing in the UI references
  a token (no hardcoded styling). Ambient motion (the scanline texture) honors "reduce motion".
- **Typed data layer end-to-end**: every screen reads the backend through generated types only —
  typed command wrappers, a live-event subscription that keeps the status readouts fresh, and a
  streaming reducer for grounded answers. No hand-written API types.
- **Reusable building blocks**: Panel, Button, IconButton, Field, Select, Toggle, Chip, Toast,
  EmptyState, ErrorState, Skeleton, and Tooltip — accessible (visible focus, ≥32px hit targets,
  AA contrast) and built once for the screens that follow.
- **Routing & resilience**: real, deep-linkable routes for every screen (including a per-frame
  `/timeline/:id` and a friendly not-found), each wrapped in its own error boundary so a broken
  view never blanks the whole app. Routes are code-split to keep startup small (~86 KB gzipped).
- The data-bearing screen bodies (search, the Scanline Timeline, settings, insights) build on this
  foundation in the next milestones.

### Fixed — P5 (M1+M2) PR #11 review (2026-06-22)
- **Command palette recovers from a no-match search**: after typing a query that matches nothing
  and pressing the down arrow, the highlight no longer gets stuck — clearing back to a matching
  query re-highlights a command and Enter runs it again.
- **Correct keyboard hint**: the palette shortcut now reads **Ctrl+K** (this is a Windows-only app),
  not the Mac ⌘K.
- **Clearer error screens**: a route error now shows the actual status/detail of a failed navigation
  (not just a generic message), and the command-palette input no longer triggers browser
  autocomplete/spellcheck overlays.
- **Safer answers (forward-looking)**: starting a new question while one is still streaming is now
  blocked, so a previous answer's late text can't bleed into the next one.

### Added — P5 (M0) backend completion (2026-06-22)
- **Timeline data** (`get_timeline`): frame-density buckets over a time range — the data behind
  the Scanline Timeline. Sparse and half-open, with a presentation-driven bucket count.
- **Insights data** (`get_insights`): real activity aggregates — total and vision-tagged frame
  counts, capture density over time, top apps, and an activity-type breakdown. Honest-empty when
  there isn't enough history; never fabricated.
- **Settings read/write** (`get_settings` / `set_settings`): the full settings object round-trips
  through the key/value store. Model-tier changes apply live; other settings persist and take
  effect on the next capture start or app restart (the Settings screen will label which is which).
- **Frame images in the UI**: enabled Tauri's asset protocol (scoped to the capture-frames folder)
  so the interface can show stored screenshots, and replaced the permissive dev CSP with a tight,
  local-only content-security policy.
- New typed IPC: `InsightsSummary` / `AppCount` / `ActivityCount`.
- Packaging (installer + portable ZIP) is intentionally deferred to a later pass.

### Fixed — P5 (M0) PR #10 review (2026-06-22)
- **Timeline math is overflow-safe**: extreme or malformed time ranges from the UI can no longer
  panic the timeline query — an unrepresentable range simply returns no buckets.
- **Settings save is now atomic**: writing settings commits all keys in a single transaction, so a
  crash or error part-way through can never leave a half-updated settings store.
- **Insights skips needless work** on an empty or invalid time range, returning the honest-empty
  summary immediately.

### Added — P4 Inference sidecar (2026-06-21)
- **No-orphan guarantee, proven first** (`02 §2`, `03 §6`, DoD #7): the `llama-server`
  sidecar is bound to a Windows **Job Object** with `KILL_ON_JOB_CLOSE`, and the child
  is assigned to it **before** its main thread resumes — so if the app dies for any
  reason, the OS takes the sidecar down with it. A cross-process test (kill the parent,
  assert the child dies) is the gate; it passes.
- **Model supervisor** (`03 §6`): lazy spawn on first use, `/health`-gated startup,
  idle eviction after `sidecar.idle_ttl_secs`, crash restart, a startup **reap** of a
  stray sidecar from a prior run (pidfile + image-path sentinel — never an unrelated
  process), and model switching (stop + restart with a new GGUF; vision adds `--mmproj`).
- **Vision tagging** (`03 §3/§5`): on-demand (the `enqueue_vision` command), plus
  opt-in **timer** and **idle** producers (`GetLastInputInfo`) that batch still-untagged
  frames — never real-time. The worker runs the frame's JPEG through the sidecar and
  stores the analysis.
- **Grounded, streaming answers** (`03 §13.5`): the `ask` command retrieves top-K
  hybrid hits, grounds the answer in their OCR text, and streams typed `answer_delta`
  events — `thinking` trace, answer tokens, and one citation per source frame. Reasoning
  is handled both as a `reasoning_content` field and as inline `<think>` tags.
- **Tiered models, runtime-downloaded, Rust-only** (`MODEL_REGISTRY`, `01 §5`): vision +
  answer each offer Default/Quality/Beta via `set_model_tier`. The `llama-server` binary
  (prebuilt Vulkan release) and the GGUF weights (+ same-repo mmproj) download on first
  use — the binary from GitHub, the models via `hf-hub` (no Python in the runtime).
- New IPC: `ask`, `enqueue_vision`, `set_model_tier` commands; `answer_delta` +
  `sidecar_status` events; `sidecar` readiness now tracks the supervisor's lifecycle.
- Real end-to-end inference (download + GPU) is covered by `#[ignore]`d smoke tests
  (`cargo test -p inference --test smoke -- --ignored`); the always-green suite proves
  the lifecycle, the HTTP client (against a mock), and the providers deterministically.

### Fixed — P4 sidecar binary resolution (2026-06-22)
- **Resilient llama.cpp release selection** — `ensure_binary` previously read GitHub's
  single `/releases/latest`, which fails with `no win-vulkan-x64 asset in the latest
  llama.cpp release` whenever that release ships an **incomplete asset set** (llama.cpp's
  CI occasionally publishes a build with only a partial set of platform zips — observed:
  `b9753` had 1 asset, no Vulkan zip). Resolution now scans the recent-releases list and
  takes the newest release that actually carries the `*-win-vulkan-x64.zip` asset. Pure
  selector `pick_vulkan_from_releases` is unit-tested (incomplete-newest-then-complete,
  newest-wins, none-found). Vulkan stays the shipped lane — it is vendor-neutral and runs
  on Blackwell GPUs (RTX 50-series) where the prebuilt `cuda-12.4` asset would not.

### Fixed — P3 review (PR #7, 2026-06-21)
- **Stale-job recovery** never misses a job now: the startup sweep requeues every
  `running` job unconditionally, and `claim` + the periodic sweep share one DB clock
  (no Rust-millisecond vs SQLite-second mismatch).
- **Image embedding** retries on a transient file lock (AV / indexer / backup) and
  only dead-letters a genuinely missing JPEG — robuster on Windows.
- The worker pool now stops its background tasks even if its handle is dropped
  without a graceful shutdown (`Drop` signals stop), so it can't keep draining the
  queue after an unexpected teardown.

### Added — P3 Deferred enrichment (2026-06-21)
- **Embedding worker, end-to-end** (`02 §5`, `03 §5/§13.2`): a bounded worker pool
  drains the `embed_text` jobs capture enqueues into vectors via **fastembed**
  (in-process ONNX, no Python) — `EmbeddingGemma-300M` text embeddings (768-dim),
  with optional `nomic-embed-vision-v1.5` image embeddings behind
  `enrich.image_embeddings`. Workers run in the background, draining the backlog
  independent of capture; concurrency is `enrich.worker_concurrency`.
- **Hybrid search is live** (`03 §13.4`): once the model loads, the vector arm of
  `hybrid_search` (FTS5 + sqlite-vec KNN → RRF) lights up. Measured **p95 ≈ 33 ms
  over 10 000 frames** — well under the ~200 ms bar. A typed `search` command
  (`SearchQuery → SearchHit[]`) makes it reachable (UI lands in P5).
- **Durable, self-healing queue**: jobs retry with exponential backoff and
  dead-letter at `max_attempts`; a job a crashed worker left `running` is requeued
  by a startup + periodic sweep (`03 §6`). A `job_progress` event surfaces queue
  depth; `embed_model` readiness reports loading → ready/unavailable.
- The embedding model loads off the launch thread, so app start never blocks on the
  first-run model download.
- Vision tagging stays fully deferred to P4 (no `vision_tag` is ever auto-enqueued).

### Added — P2 Capture happy path (2026-06-21)
- Always-on **capture → OCR → store** pipeline (`02 §5`, `03 §3/§5`): screen capture
  via Windows.Graphics.Capture with a diff gate that stores only changed frames,
  text recognition via WinRT `Media.Ocr` on a COM STA worker, JPEG storage, and a
  row written to `frames` + `ocr_text` — each stored frame enqueueing an
  `embed_text` job (consumed in P3). Vision is never auto-run (`13.3`).
- `kernel` orchestrator: the capture loop, a typed event bus (`capture_tick` /
  `readiness_changed`), a typed-`Settings` loader, and idempotent
  `start_capture`/`stop_capture`. Capture is **off until you start it**
  (privacy-first); `privacy.excluded_apps` and `privacy.pause_on_lock` are honored.
- App: `capture_control` (start/stop) and `get_frame` commands, live `db` +
  `capture` readiness, and a **minimal live timeline** UI (Start/Stop, readiness
  strip, a row per captured frame). The full Command-Deck UI lands in P5.
- Captured frames carry the foreground app/window as `app_hint` / `window_title`.
- OCR `mean_confidence` is recorded as `-1.0` ("unknown") — WinRT OCR exposes no
  confidence; no value is fabricated (see `specs/06_PATCH_PLAN.md` #2).

### Documentation (P0/P1 review pass)
- Reviewed merged P0 + P1 against the spec — both complete and compliant, no
  correctness bugs (record in `specs/05_BUILD_REVIEW.md` Pass 3). Doc/clarity
  touch-ups only: `TimeRange` now states its half-open `[start, end)` semantics
  explicitly; the concurrent-claim test documents what it actually proves; and the
  hybrid-search latency (`03 §13`) and vector-arm time-range approximations are now
  tracked gaps for P3 (`specs/07_KNOWN_GAPS.md` #7/#8). No behavior change.

### Fixed (post-P1 review, PR #4)
- `open_state` reports `db = Error` (not `Ready`) if the post-open schema-version
  probe fails — no "ready but unqueryable" store reaches the UI.
- `complete_job` / `fail_job` now error on an unknown job id instead of silently
  doing nothing (consistent with the queue's no-silent-loss contract).
- `insert_vision` also fills `frames.activity_type` (`03 §4`: "filled by vision")
  so the timeline can filter by activity without a join.
- Hybrid-search hydration uses two bulk `IN` queries instead of per-hit queries
  (removes an N+1 pattern); shared `f32_blob` / `dedup_keep_order` helpers.

### Added — P1 Data Spine (2026-06-21)
- `store` crate — the durable data spine on SQLite (WAL) + sqlite-vec (`vec0`) + FTS5
  (`03 §4/§5`): forward-only schema migrations tracked in `schema_version`, and the
  full `Store` contract — frame / OCR / vision inserts, key-value settings, text &
  image embedding upserts with synchronized vector shadows, and `get_frame` /
  `delete_frame`.
- Durable **job queue** — atomic claim (`UPDATE … RETURNING`), priority + scheduled
  (`not_before`) dispatch, retry-with-backoff, and dead-lettering at `max_attempts`
  (no silent loss).
- **Hybrid search** — FTS5 (BM25) fused with sqlite-vec cosine-KNN via Reciprocal
  Rank Fusion, with time-range filtering and highlighted snippets. The vector arm is
  driven by an injected embedding provider (FTS-only until P3 wires fastembed).
- App wiring — the desktop shell opens `screensearch.db` at the app-data dir on
  launch, reports real `db` readiness, exposes a `get_job_stats` command, and writes
  a daily-rotating file log under `<app-data>/logs/` (`03 §9`).

### Added — P0 Scaffold (2026-06-21)
- Cargo workspace with a modular kernel layout (`03 §2`): `traits` (contracts +
  domain/jobs/IPC types) and skeleton crates `kernel`, `store`, `capture`, `ocr`,
  `embeddings`, `inference`.
- `src-tauri` — Tauri 2 desktop shell (composition root) with typed `ping` and
  `get_readiness` commands.
- React 18 + TypeScript + Vite UI skeleton (`ui/`) with a P0 typed-IPC smoke screen
  and an ESLint flat config enforcing the Rules-of-Hooks gate at error level.
- Typed UI↔core contract generated with `ts-rs` into `ui/src/bindings/`, with a
  regression guard keeping 64-bit ids as TS `number` (Tauri JSON wire).
- `doctor` environment smoke-check (`cargo run -p doctor`) for WebView2 / Vulkan /
  llama-server.
- GitHub Actions CI (`.github/workflows/ci.yml`, windows-latest): UI lint/build,
  `cargo fmt`/`clippy -D warnings`/`build`/`test`, and a ts-rs binding-drift guard.
- Generated application icons from `assets/icon-source.png`.

### Fixed (post-P0 review)
- `doctor`: load `vulkan-1.dll` with `LOAD_LIBRARY_SEARCH_SYSTEM32` so the Windows
  loader resolves it only from System32 — prevents DLL search-order hijacking
  without trusting the (manipulable) `SystemRoot` env var or a hardcoded path.
- UI: issue the independent `ping` / `get_readiness` IPC calls in parallel.
- CI: Claude review now actually posts — added the `--comment` flag (the skill
  produces a review but only posts with it) and granted the workflows write
  permissions (posts were silently denied before); `concurrency` added; actions
  bumped to Node-24 majors.

_Data spine landed in P1; capture, embeddings, and inference still to come (P2–P4)._
