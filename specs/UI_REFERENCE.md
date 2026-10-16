# UI Reference (companion spec)

> **Scope:** the frontend contract — identity, design tokens, screen inventory, per-screen state
> matrix, components, typed-data rules, accessibility, performance, and voice. Same authority model
> as `03` but narrowed to the UI. The build (P5) is held to this file. When it is silent or
> contradictory, **stop and ask** (`04 §5`).

---

## 1. Aesthetic direction (the point of view)

**Thesis — a forensic console for your own screen-time.** This is not a productivity dashboard;
it's an instrument that *reads back a recording* of what you saw. The product literally captures
screens, so the interface is built from that material: a dark telemetry console where **time is a
physical filmstrip** you scrub, and a single signal-orange scan-head marks "now / here." Everything
is quiet and disciplined except that one living element.

This honors the pinned **Command Deck** identity (warm-graphite + a single signal-orange accent,
Windows-native fonts, dark-only, WCAG-AA) and gives it a reason to exist beyond "dark + accent."

**The one aesthetic risk (justified):** treat the timeline as a real instrument — a continuous
**Scanline Timeline** with a sweeping scan-head and a faint scanline texture on surfaces. The
scanline is the subject's *native material* (screen capture / CRT / monitoring), not decoration. We
spend all boldness here; every other surface stays calm.

### Palette (warm graphite + one bold accent)
| Token | Hex | Use |
|---|---|---|
| `--bg-base` | `#15120D` | app background (warm near-black, not pure black) |
| `--bg-surface` | `#1E1A13` | panels, cards |
| `--bg-overlay` | `#262017` | popovers, command palette, modals |
| `--line` | `#332B20` | hairline dividers, borders |
| `--ink` | `#ECE4D5` | primary text (warm off-white) |
| `--ink-muted` | `#9A8F7A` | secondary text, labels |
| `--ink-faint` | `#6B6253` | tertiary, disabled |
| `--accent` | `#FF6A1A` | **the only bold color** — scan-head, active state, primary action |
| `--accent-wash` | `rgba(255,106,26,0.14)` | scanline glow, selection, active row |

**Functional-only** (never brand decoration, used solely in status contexts, kept desaturated):
`--danger #C5524B` · `--warn #D9912F` · `--ok #7FA88B`. If a screen reaches for a fourth hue, stop.

### Typography (Windows-native — a constraint *and* a personality)
No web-font downloads (privacy/offline/portable). The native stack is the character:
- `--font-display`: **Bahnschrift** (condensed industrial / DIN-like) → labels, eyebrows,
  section heads, set in **uppercase with tracking** for an instrument-readout feel.
- `--font-body`: **Segoe UI** → prose, answers, descriptions.
- `--font-mono`: **Consolas** → all data: timestamps, counts, durations, model/job state.

**Type scale:** display 28/600 · title 20/600 · subtitle 16/600 · body 14/400 · caption 12/500 ·
data 13 mono. Long-form RAG answers use `@tailwindcss/typography` (body face).

### Form
- **Radius:** 4px panels, 2px data chips, **0 on the timeline ribbon** (it's an instrument, not a
  card). Not zero everywhere — that's the broadsheet default.
- **Elevation:** surface color steps + hairline, minimal shadow (dark UI).
- **Motion:** 120–200ms ease for UI; the scan-head moves **linearly**; an ambient scanline drifts
  *very* slowly. All ambient motion disabled under `prefers-reduced-motion`.

### Signature: the **Scanline Timeline**
A horizontal time-ribbon: frames appear as density ticks (busier time = denser), thumbnails on
hover, a sweeping **signal-orange scan-head** at the focused moment, faint scanline texture. It
encodes real information (when things happened, how much) — never decorative tick-marks. Reused as
a thin "minimap" strip at the top of every recall view.

## 2. Design tokens (single source)
Tokens live in `ui/src/styles/tokens.css` as CSS custom properties; Tailwind theme maps to them.
**No component hardcodes a hex, px font, or magic spacing** — everything references a token.
Spacing scale: 4 · 8 · 12 · 16 · 24 · 32 · 48. Z-layers: base / rail / overlay / toast.

## 3. Information architecture (every screen → a real job)
```
AppShell
 ├─ StatusRail (top): capture state · DB size · queue depth · sidecar/model · readiness · throttle   [telemetry]
 ├─ NavRail (left): Deck · Recall · Timeline · Insights · Settings · footer: version → repo link (0.3.1) · update indicator (0.3.2, presence-only) + a quiet "Check for updates" control (0.3.2 PR2, the quick-menu manual check — gap #99)
 ├─ CommandPalette (⌘K): jump-to + actions (search, ask, tag, settings)
 └─ Routes:
     /            Deck      — at-a-glance: capture status, today's activity, jump back in (0.3.0: where-was-i "Jump back" card + Intentions strip)
     /recall      Recall    — Search · Ask · Reports (0.2.x); content text default, opt-in raw/chrome
     /timeline    Timeline  — the Scanline Timeline browser
     /timeline/:id Moment   — one frame: image, OCR text, vision tags, context, actions
     /timeline/session/:id  Session drill-in (0.4.0 PR5) — one session: title, span, tool/host, frames strip, exchanges, Recap
     /insights    Insights  — activity analytics (nice-to-have; ships as real or honest-empty)
     /settings    Settings  — two tiers (0.3.2, D6): Essentials (Capture · Hotkeys · Privacy · Models · Storage · App · Data) + Advanced (collapsed per-group expanders)
     *            NotFound

 ┄ FlowOverlay (0.3.0) — a SEPARATE always-on-top hotkey window, NOT a NavRail route: Ctrl+Alt+Z
                         summons instant search-as-you-type / Ask over content text; Esc dismisses,
                         Enter jumps to the Moment; an empty query shows the where-was-i strip.
 ┄ Tray (0.3.2)       — an OS surface, not a window/route: passive capture-state icon + menu (03 §7d)
```
Rules: one primary action per screen; every route is reachable from NavRail or a link; no orphan
screens; deep-linkable (real routes, `/timeline/:id` shareable within the app).

**StatusRail "Throttling" chip (0.2.1).** When the smart enrichment throttle is active the rail
shows a subtle, desaturated **"Throttling"** chip (functional `--warn`, not the bold accent) — shown
**only at level ≥ 1**, hidden at level 0 / when `throttle.enabled` is off. It reflects the live
`throttle_changed` event (`03 §7`) and reads back the current level (High / Sustained); a tooltip
surfaces the live CPU/GPU pressure (or "GPU not monitored" when PDH counters are absent). It is a
status indicator, not a control — the toggle and thresholds live in Settings.

**Flow overlay (0.3.0).** A second always-on-top window summoned by a global hotkey
(`overlay.hotkey`, default `Ctrl+Alt+Z`; `03 §8`) — recall without switching context. It must
**read as ScreenSearch**, not a generic launcher: the Scanline-Timeline signature (a thin scan-head
strip), one-accent discipline (signal-orange only), Windows-native fonts, **tokens only** (`§1`/`§2`;
the `--bg-overlay` token + `overlay` z-layer already exist). Frameless, transparent, centered
upper-third, **hidden-not-destroyed** (show/hide, so summon latency is window-show latency, not a
webview boot). **Keyboard:** input focused on show; `↑/↓` navigate results; `Enter` opens the Moment
in the main window; `Esc` (and blur) dismiss; `Tab` or a `?` prefix switches to **Ask** (streams a
grounded, cited answer via the existing pipeline). An **empty query** shows the **where-was-i strip**
(`03 §7b`, which anchors on the last **non-ScreenSearch** foreground context — the ScreenSearch window
or its overlay never counts as "current") instead of results. **Perf:** visible **< 150 ms** from hotkey (warm); first results within
the existing **< 200 ms** search budget. **Reduced-motion:** ambient scan disabled under
`prefers-reduced-motion` (`§7`). **Privacy:** the overlay is the app's own window and is covered by
the self-exclude capture gate — it must never appear in its own capture history (`03 §7b`, D7). A
failed hotkey registration is a **visible Settings warning + toast**, never silent (D6).

**NavRail version footer (0.3.1, D4 — the "quick menu" version line of issue #57).** The bottom of
the NavRail carries a small footer line showing the **app version** (e.g. `v0.3.2`, mono data face,
`--ink-faint`); it is a **link** — activating it (click or keyboard) opens
`https://github.com/nicolasestrem/screensearch-v2c` in the **default browser** (external open, not
in-app). Visible on every screen; quiet (no accent color — it is telemetry, not a primary action);
keyboard-focusable with a visible focus ring (`§7`). Surface decided in `07` #99 (user, 2026-07-04).
The rest of #57 — load/unload-model and start/stop-vision quick actions — does **not** land in
0.3.1; it is deferred to the 0.3.2 lifecycle mini-arc (`07` #97).

**Settings two-tier IA (0.3.2, D6/D7).** The flat settings wall becomes two tiers. **Essentials
(always visible):** Capture (interval, monitors, event-driven master toggle) · Hotkeys (overlay
chord, mark chord, overlay results, dwell — plus the inline **cross-chord conflict warning**, gap
`07` #100: fires live when `overlay.hotkey` and `marks.hotkey` are set to the same chord and clears
when they differ) · Privacy (excluded apps, pause on lock) · Models (vision/answer pickers, show
thinking, load/unload) · Storage (retention days, max width) · **App** (below) · Data (HTTP API
toggle/port/token, export). **Advanced (collapsed, one expander per group):** Capture tuning · Text
source / UIA · Enrichment & scheduling · Performance throttle (+ live readout) · Text filtering ·
Reports & retrieval · Inference engine (sidecar). Tier membership is **settled**; within-tier
ordering and visual grouping are the implementer's. Every section opens with **one plain-language
sentence** stating what it is for and when a normal person would touch it (voice per `§9`).
Presentation-first (D7): zero key renames, zero semantic changes; the two dead settings (JPEG
quality, `uia_run_on_interactive`) leave the UI per `03 §8` (D8). Advanced's collapsed state may
persist per-session; no new persistence machinery. A settings search box is deferred (`07` #103).

**Settings · App section (0.3.2, D1/D3).** A new Essentials section — the app's own lifecycle home:
**Run at startup** (`app.run_at_startup`, default off) · **Close to tray** (`app.close_to_tray`,
default on; user-voice label, e.g. "Closing the window keeps ScreenSearch running in the tray") ·
**update status + "Check for updates"** (the status line reads e.g. "v0.3.3 available — restart to
update" only while an update exists; otherwise nothing) · **version + repo link** (the NavRail
footer's information, restated where a user would look for it).

**Tray (0.3.2, D3/D4).** An OS surface, not a route or webview — mechanics in `03 §7d`; this file
owns its UX rules. The icon is **passive telemetry**: glyph/tint tracks capture state (capturing /
paused / error) live. It is the entire "reminder" feature — **no notifications, no nudges, no badge
counts** (the `§4` Deck rule, extended to the OS shell). Menu: Open ScreenSearch · Pause/Resume
capture · Load/Unload answer model · Start/Stop vision tagging · Check for updates · Quit. The
**one-time close-to-tray toast** uses the existing `Toast` primitive + `toast` z-layer, explains that
the app keeps running and where to turn the behavior off, and never repeats — it fires on the **first
time the window is reopened from the tray** (not at hide, when it would never be seen; persisted via
the `app.tray_toast_done` marker, `03 §8`). States per `§4`.

**Updater surface (0.3.2, D1).** Pull-based (`03 §11b`). When — and only when — an update exists: a
**quiet presence indicator** on the NavRail (a dot/glyph, **never a count** — the no-badge-counts
rule holds) plus the App-section status line. Background download; install only on user-initiated
restart; no modal, no nag, no auto-restart. No update → **zero UI presence** (the indicator does not
exist, not "shown as zero"). Quiet styling (functional palette, not the bold accent) — it is
telemetry, not a primary action, same discipline as the version footer.

**Sessions in the UI (0.4.0, D11/D13).** Sessions surface **inside existing routes** — there is **no
new NavRail entry** (D13 is the fence; a dedicated Sessions route is a 0.4.x conversation only after
live use proves the in-route surfaces insufficient, `07` #108). Four touch-points, all reusing shipped
primitives (`03 §7e`):
- **Timeline — session bands.** A kind-coded band layer over the Scanline ribbon (`§1`), **tokens
  only, one-accent discipline** (signal-orange stays the single accent — bands are differentiated by
  the functional palette + labels, not by competing accents). Data comes from `list_sessions`
  (`03 §7`). Selecting a band opens the drill-in. **Geometry is fixed at exactly four lanes in every
  state**: initial route skeleton, loading, error, empty, and populated all reserve the same four
  token-height rows. Sessions whose collision packing would require lane 5+ are not allowed to grow
  the ribbon; they are aggregated into one neutral, keyboard-operable “more sessions — narrow the
  range” control. Activating it focuses the Timeline's **existing range presets** (it does not add a
  fifth lane, modal, new route, or new range control). This is the user-selected no-CLS resolution:
  D9 remains strict and is not relaxed for dense overlap.
- **Session drill-in** (`/timeline/session/:id`, a Timeline sub-route, **not** a NavRail item): title,
  span, tool/host chips, the **frames strip**, **exchanges** when present (best-effort — absent when
  markers didn't match, shown honestly, never invented), and a **Recap** action that runs the existing
  report engine over the session's range/frames (`03 §8b`) — with the same honest footer reports
  already carry. Bound by the `§8` shell-layout contract: **one scroll context, no nested scrollbars**
  (the drill-in must not reintroduce the 0.3.1-#59 nested-scroll problem).
- **Moment — "part of session" line.** A line linking the frame into its drill-in; a frame with no
  session simply omits the line (no placeholder).
- **Deck — where-was-i framing.** The "Jump back" card gains session framing ("Jump back:
  *repo — VS Code*, session 09:14–14:32").

**Tone (D11).** Truthful, on-demand, neutral — **no** durations-as-judgment, **no** streaks, **no**
scores, **no** session notifications. Session `confidence` is surfaced honestly (a low-confidence
session says so); an **open** session reads as still-running with non-final boundaries. Reduced-motion
honored (`§7`).

## 4. Per-screen state matrix (the comprehensiveness guarantee)
**Every view defines all of: `loading` · `empty` · `error` · `partial` · `populated`.** No screen
ships with only the happy path; no mock data; no "Coming Soon."

| Screen | empty | error | partial | notes |
|---|---|---|---|---|
| Deck | "Capture is off / no frames yet — start capture" | readiness probe failed → retry | capturing but no enrichment yet | drives onboarding; **0.3.0:** where-was-i "Jump back" card + Intentions strip (unresolved marks, newest-first; open/resolve/dismiss; **no badge counts**); **0.4.0:** the "Jump back" card gains session framing ("*repo — VS Code*, session 09:14–14:32", `03 §7e`) |
| Recall (search) | "No matches — try different words / widen the range, or include app chrome" | search cmd failed → retry | vectors still indexing → "searching text only for now" banner | content text by default + "include app chrome / raw text" toggle; never a zero-result dead end |
| Recall (ask) | prompt invites a question (or a premade card) | sidecar unavailable → "answer model not loaded; load it?" | streaming (tokens arriving) | cite frames; premade cards prefill + submit |
| Recall (reports) | range picked; prompt invites "Generate" | generation failed → retry, keep range | generating (single-pass / map-reduce in progress) | markdown + clickable source-frame chips + Copy + `.md` download + footer; honest empty on no-evidence ranges. **0.3.1 (D2/D3, #65):** the download is named `screensearch-report-YYYY-MM-DD-HHmm.md` (**local time**; same-minute collisions append `-2`, `-3`, …; extension unchanged); **0.3.2 (#89):** daily/weekly downloads carry the kind in the stem — `screensearch-report-daily-…` / `-weekly-…` — custom keeps the bare shape; the footer is one plain-text line block stating the **app version, model id used for generation, time span covered, and active filters** (plus the existing counts: passes · periods covered · frames summarized) — no new settings toggle |
| Timeline | "No captures in this range" | load failed → retry | thumbnails still resolving | scrub never blank; **0.4.0:** session bands over the scanline (`03 §7e`) reserve **exactly four lanes** in initial skeleton/loading/error/empty/populated; a range with frames but no sessions shows four empty reserved rows, while lane-5+ collisions aggregate into the neutral keyboard range-focus control — no geometry change |
| Moment | — | frame missing/deleted → explain + back | vision not yet tagged → "queue vision for this frame" | on-demand vision entry point; **0.3.1 (D1, #59):** the recognized-text **and** raw-text regions **grow inline** with their content — full height, no internal max-height, **no nested scrollbar**; the page is one scroll context (the outer page scrolls); **0.4.0:** a "part of session" line links into the drill-in (absent, not placeholdered, when the frame has no session) |
| Session drill-in (0.4.0) | session has **no exchanges** captured → honest "no exchanges captured for this session" (never invented) | session missing/deleted → explain + back to Timeline | **open session** → "still running — boundaries not final"; summary generating → skeleton line | title/span/tool-host chips + frames strip + exchanges (when present) + **Recap** (runs the report engine, `03 §8b`); low `confidence` surfaced honestly; **one scroll context**, no nested scrollbars (`§8`); loading = skeleton; populated = the assembled session |
| Insights | "Not enough history yet" (honest) | compute failed → retry | partial windows labeled | no fabricated charts |
| Settings | — | save failed → keep form, explain | model downloading (progress) | optimistic + reconcile |
| Settings · Performance throttle (0.2.1) | toggle OFF → readout collapsed to "Throttle disabled" (empty-off) | status probe failed → "Pressure unavailable", keep toggle + fields | partial: GPU unmonitored → CPU% shown + honest "GPU not monitored" | master toggle + live CPU/GPU + level readout + 8 threshold fields; loading = skeleton readout; populated = live CPU/GPU% + level (Normal/High/Sustained) |
| Flow overlay (0.3.0) | empty query → where-was-i strip ("Jump back: *repo* — VS Code, until 14:32") or honest "Nothing to resume yet" | search/ask cmd failed → inline retry, overlay stays open | results streaming / Ask tokens arriving | frameless always-on-top; loading = skeleton rows; populated = top-N results with thumbnails; `Esc`/blur dismiss; five states like every view |
| Settings · HTTP API (Gate A network contract) | toggle OFF → "API disabled" (empty-off); nothing listens | port in use on enable → **loud warning + toast + inline "pick another port"**; save failed → keep form | enabling → binding | master toggle + IPv4/IPv6 literal address field (default 127.0.0.1) + port field + token reveal/copy/regenerate + threat-model copy; populated = actual listen address/port; changes apply immediately; distinguish wildcard listen addresses from usable client destinations |
| Tray (0.3.2) | — (icon present whenever the app runs) | capture error → error glyph/tint; menu still operates | paused → paused glyph/tint | passive state only — no notifications, no counts (`03 §7d`); menu actions round-trip through existing commands |
| Updater indicator (0.3.2) | no update → **zero presence** (nothing rendered) | check/download failed → quiet App-section line ("couldn't check for updates" + retry), never a modal | downloading → indicator present, App line reflects it | presence, never a count; install only on user-initiated restart (`03 §11b`) |
| Settings · App (0.3.2) | — | startup-registration or update-check failure → inline explain + retry, form kept | update downloading → status line reflects it | run-at-startup + close-to-tray apply optimistic + reconcile, like the rest of Settings |

**API address and threat model (configurable-bind correction).** Default off, with missing
bind address defaulting to `127.0.0.1` even for existing enabled installations. Allow any IPv4/IPv6
literal, including specific interfaces and wildcards; reject invalid input visibly. Address and
port changes persist and apply immediately. Display the actual listener or bind error honestly.
Explain plain HTTP, reachable token holders' full-history access, token regeneration, and
user-managed firewall access. A wildcard does not enforce LAN-only access. IPv6 client URLs use
brackets. Preserve accessible controls, loading/error/off/listening states and token privacy.
The earlier automatic-broadening upgrade notice and acknowledgement flow are removed.

Loading uses skeletons that match final layout (no spinner-only screens). Empty states are
**invitations to act**, not mood.

## 5. Component inventory (built once, reused)
Shell: `AppShell`, `StatusRail`, `NavRail` (0.3.1: footer version link → opens the GitHub repo in
the default browser — D4, `§3`), `CommandPalette`, `ReadinessBanner`; 0.3.2: `UpdateIndicator`
(NavRail footer: the presence indicator + the quiet manual "Check for updates" control —
`§3`/`§4`) + `TrayMenu` (the native Tauri tray — Rust-built, not a
React component; listed here so its `§4` states are owned by this file; mechanics `03 §7d`).
Primitives: `Panel`, `Button`, `IconButton`, `Field`, `Select`, `Toggle`, `Chip`, `Toast`,
`EmptyState`, `ErrorState`, `Skeleton`, `Tooltip`, `Expander` (0.3.2 PR5: a Panel-shaped
disclosure for a collapsed-by-default settings group — header row = the disclosure button
with `aria-expanded`/`aria-controls`, labelled-region body, intro sentence visible while
collapsed; open state is per-session via the ui store, per `§3` D6).
Domain: `ScanlineTimeline`, `FrameTile`, `FrameImage` (lazy), `AnswerStream` (markdown + citations),
`SearchResult`, `MomentDetail` (0.3.1: recognized-text + raw-text regions grow inline — no internal
max-height, no nested scrollbar; one scroll context — D1, `§4`), `JobQueueMeter`, `ModelTierPicker`
(Default/Quality — 0.3.0 retired Beta),
`ScheduleControl` (on-demand/timer/idle), `RetentionControl`.
Domain (0.2.x): `RecallModeTabs` (Search/Ask/Reports), `TextSourceToggle` (content / include-chrome),
`ReportBuilder` (daily/weekly/custom range → Generate), `ReportView` (markdown + clickable
source-frame chips + Copy + `.md` download — 0.3.1 D2: named `screensearch-report-YYYY-MM-DD-HHmm.md`,
local time, `-2`/`-3` on same-minute collision; 0.3.2 #89: daily/weekly stems carry the kind,
`screensearch-report-daily-…` / `-weekly-…`, custom unchanged — + footer, 0.3.1 D3: one plain-text line block with
app version · model id used · time span covered · active filters, plus the existing counts (passes · periods covered · frames summarized);
no new settings toggle), `PromptCardGrid` (premade Ask
cards: Day Recap, Standup Update, Time Breakdown, Top of Mind, AI Habits — click fills + submits).
Domain (0.2.1): `ThrottlePanel` (Settings: master `Toggle` + 8 threshold `Field`s + live readout),
`ThrottleStatus` (live CPU/GPU pressure + level Normal/High/Sustained, honest "GPU not monitored"
when PDH counters are absent — five states per `§4`), `ThrottleChip` (subtle StatusRail indicator,
level ≥ 1 only).
Domain (0.3.0): `FlowOverlay` (the always-on-top hotkey window: search-as-you-type + Ask over content
text, where-was-i empty state, five states per `§4`), `WhereWasICard` (Deck "Jump back" card →
Moment), `IntentionsStrip` (Deck: unresolved marks, newest-first, open/resolve/dismiss, **no badge
counts**), `HotkeyField` (Settings: records a chord for `overlay.hotkey`/`marks.hotkey`, shows a loud
warning on registration conflict), `ApiPanel` (Settings: master `Toggle` + port `Field` + token
reveal/copy/regenerate + threat-model copy + the loud port-in-use "pick another" affordance),
`ExportPanel` (Settings "Data export": an **Export…** `Button` → `export_data`, streaming
frames + content text + marks to a JSON file in the user's Downloads folder, **no images**;
its own panel so it reads as independent of the API — it **works with the API off** — D12).
Domain (0.4.0, names proposal-level — PR5 owns): `SessionBands` (the kind-coded band layer over
`ScanlineTimeline`, tokens only, one-accent — data via `list_sessions`; exactly four reserved lanes
in every state, lane-5+ overflow aggregated into the neutral range-preset-focus control), `SessionDrillIn` (the
`/timeline/session/:id` view: title/span/tool-host chips + frames strip + exchanges + **Recap** via
the report engine; one scroll context, no nested scrollbars — `§4`/`§8`).
Each component owns one job; a label labels, an example demonstrates — nothing does double duty.

## 6. Data & state (reliability by construction)
- **Typed IPC only.** Every command/event payload is a Rust struct exported via **`ts-rs`** and
  imported by the UI. The UI **never** hand-writes an API type. Contract drift is impossible.
- **TanStack Query** owns all server-state (commands): one place for cache/loading/error/refetch.
  No bespoke `useEffect` fetch-and-setState.
- **Zustand** only for ephemeral UI state (palette open, selected range, active Recall mode).
- **Content vs raw text is a typed query param (0.2.x):** the Recall toggle sets
  `SearchQuery.include_chrome` (default `false`); reports call `generate_report`. Both flow through
  TanStack Query — no ad-hoc `useEffect` fetches; premade Ask cards just prefill + submit the
  existing `ask` flow.
- Streaming (`ask`) consumes `answer_delta` events into a reducer; `readiness_changed` /
  `sidecar_status` / `job_progress` / `throttle_changed` (0.2.1) drive the StatusRail live (the
  last feeds the throttle chip + Settings readout via TanStack Query off `get_throttle_status`).
- **Rules of Hooks are inviolable** — all hooks before any early return; JSX conditionals, not
  conditional hooks. (This is a known scar; see `04` guardrails.)
- Error boundaries per route; a thrown render never blanks the whole app.

## 7. Accessibility (WCAG-AA, non-negotiable)
AA contrast for all text/controls (palette chosen for it); visible keyboard focus on every
interactive element; full keyboard nav incl. the timeline (arrow scrub, Enter to open) and, for
0.4.0, the **session bands** (keyboard-operable, Enter opens the drill-in) and the **drill-in** view
(fully keyboard-navigable); ARIA on custom widgets (timeline = slider semantics); `prefers-reduced-motion`
disables scan/ambient (incl. session-band motion); respects OS dark (app is dark-only by design);
hit targets ≥ 32px.

## 8. Performance budgets
Initial JS ≤ 250 KB gzip; route-split per page; virtualized frame grids/timeline (no full-list
DOM); `FrameImage` lazy + decoded async; search results render < 100 ms after data; interaction
latency < 100 ms; no layout shift on data arrival (skeletons reserve space).

### Shell layout contract (0.3.2, D9 — acceptance-grade; binds all future UI work, not just this arc's)
- **One scroll context per route:** NavRail and StatusRail are fixed; only the content pane scrolls.
  **No nested scrollable regions** — the 0.3.1 #59 Moment principle, applied everywhere. The contract's
  unit is the **route**: modal/overlay surfaces are not route scroll contexts and keep their own bounded
  scroll (the command-palette listbox, the toast stack, and the separate Flow-overlay window). Deliberate
  horizontal strips are not exempt — they wrap rather than scroll sideways (Windows draws a permanent
  scrollbar for `overflow-x`), so no route shows a horizontal scrollbar in the matrix. **One scoped
  content exemption:** a **preformatted recognized-text block** (the Moment `content_text`/`raw_text`
  `<pre>`, which faithfully mirrors captured tables / code / terminal output) may scroll **horizontally
  within its own block** (`overflow-x-auto whitespace-pre`) rather than wrap — wrapping a wide table
  destroys the columns that make it readable. It stays a code-block-style local scroller: the vertical
  page scroll is still single, and its grid column carries `min-w-0` so the block never forces the route
  wider than the viewport (grid items default to `min-width:auto` — a wide `<pre>` would otherwise shove
  the context rail off-screen). Wrapped tile/thumbnail strips are **not** this case — they still wrap.
- **No horizontal scrollbar** at any supported size: 1280×720 minimum through ultrawide, at
  100 / 125 / 150 % DPI.
- **No cumulative layout shift on load:** skeletons reserve final dimensions (the budget rule above,
  now acceptance-grade); status chips have stable widths; toasts and banners **overlay** rather than
  push — the readiness banner alone may reserve space. The Timeline's 0.4.0 session layer is a fixed
  application of this rule: initial route skeleton and loading/error/empty/populated states all
  reserve exactly four band lanes; dense overlap is summarized, never allowed to add a fifth row.
- Verified by PR4's screenshot matrix (all six content routes — Deck, Recall, Timeline, Moment
  (`/timeline/:id`), Insights, Settings — × {1280×720, 1920×1080, 3440×1440} × {100 %, 150 %}); any
  later view violating a line above is a regression against this section. Moment is not optional here:
  it is the origin of the one-scroll-context principle (0.3.1 #59).

## 9. Voice & copy (interface speaks plainly, from the user's side)
Name things by what the user controls ("Pause capture", not "halt pipeline"). Actions keep their
name through the flow ("Tag with vision" → toast "Vision tagged"). Errors explain what happened and
how to fix it, in the interface's voice, never apologizing or vague. Sentence case, active voice,
no filler. Timestamps human-relative with absolute on hover.

## 10. Acceptance criteria (definition of done — UI)
1. Every route renders **real API data** or an explicit loading/empty/error/partial state — **zero
   mock data, zero dead ends, zero "Coming Soon."**
2. All payload types are `ts-rs`-generated; no hand-written API types in the UI.
3. `eslint-plugin-react-hooks` passes as an error-level gate; routes have error boundaries.
4. Tokens are the single source — no hardcoded hex/font/spacing in components.
5. AA contrast verified; keyboard-only operation of every screen incl. the timeline; reduced-motion
   honored.
6. Performance budgets (§8) met on a realistic DB.
7. The Scanline Timeline scrubs smoothly and reflects real capture density.
8. Verified by **running the app and capturing screenshots** of each screen in each state — not by
   "it compiles" (`04 §6`).
9. The `§8` shell layout contract holds: one scroll context per route, fixed rails, no nested or
   horizontal scrollbars at supported sizes/DPI, no layout shift on load (0.3.2, D9).

---

*Companion to `00`–`04`. Aesthetic direction is intentional and subject-grounded; if a future change
makes the UI read as a templated dark dashboard, that's a regression against §1.*

## 12. External-agent read credential (approved contract, not implemented)

Settings · HTTP API adds independent read-token controls beside existing legacy-token controls,
using generated Rust IPC types and TanStack Query. No new NavRail route or hand-written payload
types. Existing token access remains; upgrades have no read token. Create/reveal/copy/rotate/
revoke act independently and never enable API or change bind/port. Tokens stay out of general
settings/export/report DTOs, status events and logs. Retain the existing credential storage;
no credential-store migration, account system or scope editor. A dedicated credential action
may reveal/copy the token; passive status reports only availability. Keep successful values
private, hide by default, and never render into errors or event payloads.

Explain: read token can access captured raw/filtered history, stored images and saved reports,
but cannot Ask/generate, mutate marks, capture now, retry reports or deliver content. Plain HTTP
may expose tokens/history to network observers; explicitly configured remote wrapper clients
use the same threat model. Revocation affects subsequent requests, not guaranteed cancellation
of already authorized streams. Creating a credential is not evidence that the API is listening.

| State | Required presentation |
|---|---|
| loading | Geometry-matched skeleton; do not flash a token or false absent state |
| empty | No read token configured; Create action; API-off status remains independent |
| error | Preserve controls/form; explain failed credential action with retry; no secret echo |
| partial | Credential present but API off/bind error; show both facts, do not claim connectivity |
| populated | Hidden token, explicit reveal/copy, independent rotate and revoke; truthful action result |

Use existing tokens/primitives, keyboard focus, accessible labels, error boundary, reduced motion
and one-scroll/no-CLS rules. Verify all states natively, including save failure and independent
revocation. This contract approves UI work; it is not a screenshot or implementation receipt.

## 11. Memory reports (approved 2026-10-15)

Settings adds a Memory reports section using generated `MemorySettings`/`MemoryStatus` and
TanStack Query. Default-off toggle, local time, explicit IANA timezone and selected output folder;
preview first eligible report date before enablement. Keep the previous completed-day shortcut
and add manual date/range controls using generated `MemoryReportRange`. Equal start/end selects
one day; both endpoints are inclusive in the saved timezone. Explain the selected dates before
running and show validation errors without clearing inputs. Today is an in-progress report up
to enqueue time, explicitly partial when evidence exists; future dates are rejected. Selecting
dates does not change the daily schedule or send anything until Run is pressed. History/details
and Telegram label the selected range; display actual saved UTC-window timestamps in its timezone.
Show last-run date/window/status, sampled coverage, saved output and destination outcomes
separately. History exposes Retry (same run/resume delivery) and Regenerate (linked revision;
replace unchanged owned note only; no repeat confirmed Telegram). Errors preserve form/output.

States: loading reserves final geometry; off explains setup; missing model/source/folder is
blocked with actionable explanation; running shows measured progress; partial is explicitly
labeled; NO_DATA is honest empty output; failed write retains generated report; edited-file
conflict preserves user content and exposes recovery path if present. Older missed gaps remain
visible. No success without a persisted receipt. Explicit source-identity reset explains clone
behavior and requires schedules/delivery disabled with no active run. No inferred personal path.
Use existing tokens/primitives, keyboard navigation, route error boundaries and one-scroll/no-CLS
contract. No new NavRail route, productivity notifications or scores. Telegram controls and
outbound permission are specified in their separate amendment (`03 §8d`). Native verification remains required.

**Telegram controls (separate §8d amendment).** Memory reports includes a default-off Telegram
toggle, explicit chat ID, write-only password token field, save/clear credential actions and
availability status. Explain before enablement that saved-report digest content leaves the
device for Telegram/the selected conversation; no screenshots/raw OCR, and masking is not
comprehensive PII detection. Never prefill a saved token. Clear the input after successful save.
Display per-destination/chunk outcomes separately from generation/vault success. Unknown delivery
requires inspect-conversation guidance and explicit Mark reviewed or Resend intentionally; the
latter explains duplicate risk. No automatic resend, fake read receipt or implicit test send.
Disabled, loading, credential unavailable, pending, partial, failed, confirmed and unknown states
use existing tokens/accessible primitives without route layout shift; failures keep the form.
