# UI Reference — v1 identity (superseded 2026-11-03 by the 0.4.5 arc)

Superseded because the 0.4.5 arc replaces the UI identity with a v2 design system chosen by the maintainer (`03 §1`; `docs/0.4.5.md` §1 and P2). The v1 bodies below are preserved verbatim; nothing here is normative for 0.4.5 onward.

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
Domain (2026-10-20, `03 §7g`): `SetupIndicator` (StatusRail chip: zero presence when idle, one
summarized active/failed state, links to Settings) and `SetupDetails` (Settings · Models: one row
per setup operation with stage, determinate-or-indeterminate progress and plain failure copy).
Domain (0.4.0, names proposal-level — PR5 owns): `SessionBands` (the kind-coded band layer over
`ScanlineTimeline`, tokens only, one-accent — data via `list_sessions`; exactly four reserved lanes
in every state, lane-5+ overflow aggregated into the neutral range-preset-focus control), `SessionDrillIn` (the
`/timeline/session/:id` view: title/span/tool-host chips + frames strip + exchanges + **Recap** via
the report engine; one scroll context, no nested scrollbars — `§4`/`§8`).
Each component owns one job; a label labels, an example demonstrates — nothing does double duty.
