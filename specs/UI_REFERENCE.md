# UI Reference (companion spec)

> **Scope:** the frontend contract — identity, design tokens, screen inventory, per-screen state
> matrix, components, typed-data rules, accessibility, performance, and voice. Same authority model
> as `03` but narrowed to the UI. The build (P5) is held to this file. When it is silent or
> contradictory, **stop and ask** (`04 §5`).

---

## 1. Aesthetic direction (the point of view)

**Logbook** - recorded 2026-11-04 as the 0.4.5 P2 direction pick (D4/D5). Logbook keeps direction C "Signal Console" structure exactly and replaces its skin: the density, the logbook-first chronology, a context rail on every screen and tabular figures for measured values all survive, while C's cyan/violet pair, glow, scanlines and sub-12px tracked micro-labels do not. The superseded v1 identity (warm graphite + signal-orange, Windows-native fonts, Scanline Timeline, dark-only) is preserved verbatim at `specs/archive/UI_REFERENCE_v1_identity.md` and is **not** normative for 0.4.5 onward.

**Visual evidence:** the two rendered skins live on branch `feat/0.4.5-p2-design-directions` as `docs/kickoffs/P2-directions/direction-d.html` (ink) and `direction-d2.html` (pine), with `compare.html` showing C, D and D2 side by side and `PICK.md` recording the pick and its `impeccable detect` counts. Those files are the rendered comparison, not the token contract: each is a self-contained HTML mockup with its own inline `<style>` and short local names, while the app's names are the ones in `§2`.

**Skin rules (binding).** No cyan, no violet, no neon, no glow, no scanlines. **Mono is for measured values only** - timestamps, counts, durations, sizes, percentages, frame numbers - never for labels, headings, buttons or setting names. **One restrained accent per theme.** Red and amber appear only as states, never as decoration and never as a second accent. **Nothing renders below 12px.** **No tracked uppercase labels:** sentence case throughout, with weight and tone carrying hierarchy. Density comes from layout and rhythm, not from shrinking type.

**Colour roles.** The accent is action, selection, focus and the matched term. Relationships (entity chips, thread membership, session links) are neutral surfaces, neutral borders and a small link icon, never a second hue. The match highlight is accent wash plus an accent underline - there is no yellow. Red and amber are states. **Pine success:** in pine, success is its own token family and visibly distinct from the pine accent - a neutral status treatment (primary-text foreground on a raised-surface background with a strong border) that always carries a check/state icon, so "Capturing", "Saved" and "completed" never read as the accent. In ink, success stays green. Status components therefore always render their icon, in every theme (`§7`). The state glyphs are fixed shapes: a check (or check-in-circle where the badge is self-contained) for ok, a triangle for warn, a circled exclamation for danger, an info glyph for neutral advisories, and a plain arc for loading, which animates only under `motion-safe`. Danger and warn are deliberately distinct shapes and are never swapped.

**Four themes, two accents.** Themes differ only in the semantic token layer, switched by `data-theme` on the root element; structure, type and components are shared. Two accents ship, each in light and dark, all four user-selectable: **pine** (default) - near-neutral graphite and mist neutrals on hue 160, with a pine-green accent on hue 165 - and **ink** - cool slate and paper neutrals on hue 252, with an ink-blue accent on hue 258. The values are exactly `pine-light` (the default, and the `:root` theme), `pine-dark`, `ink-light` and `ink-dark`.

The choice is the setting **`ui.appearance`** (`03 §8`): enum `pine-light` (default) | `pine-dark` | `pine-system` | `ink-light` | `ink-dark` | `ink-system`, rendered in Settings > General. The `*-system` values follow the OS light/dark preference within the chosen accent. P3 registers the key in the settings v2 registry; P7 renders the control, resolves `*-system` through `prefers-color-scheme` and applies `data-theme` on the root. Until P7 lands the app ships `pine-light` by default.

Fonts and icons are **bundled local files** (0.4.5 D15): OFL-licensed variable fonts shipped with the app, nothing fetched at runtime - **Source Sans 3** (variable, weights 200-900) is the one text family, carrying headings, labels, body and interface text, and **Cascadia Mono** (variable, 200-700) is the one mono family, used for measured values only. Display treatment comes from weight and size; there is no third face. Icons are **one local SVG set** under `ui/src/components/primitives/icons/` at a single stroke width - `--stroke-icon`, 1.5, on a 24 viewBox with round caps and joins, drawing `currentColor` (`docs/0.4.5.md` P2 item 6 replaces the hand-maintained `ui/src/components/icons.tsx`). No web-font CDN and no icon service.

**Motion is state-only.** A transition may show that something changed - hover, focus, open, save, a row arriving - and nothing animates for atmosphere: no ambient drift, no entrance choreography, no sweep. Durations come from one scale (fast 90 ms for control feedback, base 140 ms, slow 220 ms) on a single ease curve, so a visible state transition lands in the 150-250 ms class, and `prefers-reduced-motion` disables all of them (`§7`). The binding restatement carries over: the signature surface (the timeline, `§3`) encodes real capture density rather than decoration.

## 2. Design tokens (single source)
Three layers in `ui/src/styles/tokens.css`: **primitive** (raw scales: colour ramps, type scale, space scale, radii, shadows, durations) → **semantic** (`--surface-raised`, `--text-secondary`, `--border-subtle`, `--action-primary-bg`, `--focus-ring`…) → **component** (only where a component needs its own knob). **The primitive palette values are P2's to write** in that file; the scales and the semantic names below are the contract those primitives implement, so a token P2 adds either maps onto one of them or is a new decision recorded here.

**Type scale (px).** `12` xs · `13` sm, also `data` at 13 in mono · `14` body · `14` ui · `15` prose · `17` h3 · `20` h2 · `26` h1 · `30` display. **12 is the floor**; `13` mono (`data`) is for measured values only, and `ui` labels setting names, control labels and nav. Weights are **400 / 500 / 600** (normal, medium, semi), and display treatment comes from weight and size, never a third family (`§1`).

**Space scale.** 4-based with named steps (2 · 4 · 6 · 8 · 10 · 12 · 16 · 20 · 24 · 32 · 40 · 48 · 64 px); no component invents a spacing value between steps. **Radii** are `3` · `4` · `6` · `pill` - small by rule, `6` is the ceiling. **Density token:** `comfortable` (default) or `compact`, driving row height (36 / 32 px), control height (32 / 28 px) and grid gap; a screen never hardcodes a row height (`§8`).

**Semantic names** (the token layer a component may reference):
- Surfaces: `--surface-app`, `--surface-chrome`, `--surface-pane`, `--surface-raised`, `--surface-inset`, `--surface-hover`, `--surface-selected`.
- Borders: `--border-subtle`, `--border-strong`, `--border-control`, `--border-focus`.
- Text: `--text-primary`, `--text-secondary`, `--text-tertiary`.
- Accent and action: `--accent`, `--accent-strong`, `--accent-wash`, `--accent-line`, `--on-accent`, `--action-primary-bg`, `--action-primary-fg`.
- Focus: `--focus-ring`.
- States: `--status-ok-fg` / `-bg` / `-border`, and the same three-part family for `warn` and `danger`; the success family is the neutral treatment in `§1` under pine.
- Match highlight: `--mark-bg`, `--mark-fg` - accent wash and accent-inked text; no yellow.

**Themes** are semantic-layer swaps on `[data-theme]`, whose values are exactly `pine-light` (the `:root` default), `pine-dark`, `ink-light` and `ink-dark` (`§1`); the setting that selects them is `ui.appearance` (`03 §8`), whose `*-system` values resolve against `prefers-color-scheme` in the applier rather than adding a fifth `data-theme`. Every semantic token is declared in all four themes, and a theme value references a primitive only.

**Z-layers** stay `base` / `rail` / `overlay` / `toast`. **Durations** are `fast` 90 ms, `base` 140 ms, `slow` 220 ms with one ease curve (`§1`); `prefers-reduced-motion` disables all three (`§7`). **Icon stroke** is `--stroke-icon` = 1.5: one value for every glyph, so a thicker icon is a new decision recorded here, not a local override.

Components may reference **semantic and component tokens only**, never a primitive directly. **No component hardcodes a hex, px font, or magic spacing**: everything references a token, and a later theme is another entry in that layer, not a component change.

Tailwind's theme maps onto the same custom properties, so a Tailwind utility and a hand-written `var(--…)` resolve to one value. A raw hex, a px font size or a non-token spacing value in a primitive is a lint failure (P2 acceptance).

## 3. Information architecture (every screen → a real job)
```
AppShell
 ├─ StatusRail (top): capture state · DB size · queue depth · sidecar/model · readiness · throttle   [telemetry]
 ├─ NavRail (left): Deck · Ask · Work · Recall · Insights · Settings (0.4.5 D7) · footer: version → repo link (0.3.1) · update indicator (0.3.2, presence-only) + a quiet "Check for updates" control (0.3.2 PR2, the quick-menu manual check — gap #99)
 ├─ CommandPalette (⌘K): jump-to + actions (search, ask, tag, settings)
 └─ Routes:
     /            Deck       — at-a-glance: capture status, today's activity, jump back in (0.3.0: where-was-i "Jump back" card + Intentions strip)
     /ask         Ask        — Ask v2 conversation (0.4.5 P9): persisted threads, session + frame citations, editable scope chips
     /work        Work       — thread list (0.4.5 P9): sortable by last touched / time spent
     /work/:id    Thread     — one thread (0.4.5 P9): its sessions as a timeline, entities, open loops, connected threads, Ask scoped to the thread, rename / merge / split
     /recall      Recall     — Search · Reports (0.2.x); content text default, opt-in raw/chrome
     /timeline    Recall     — the timeline browser (the Scanline Timeline or its v2 successor, per the `§1` pick); same NavRail entry as `/recall`
     /timeline/:id Moment    — DETAIL route, never a rail item: one frame — image, OCR text, vision tags, context, actions
     /timeline/session/:id  Session — DETAIL route, never a rail item (0.4.0 PR5): title, span, tool/host, frames strip, exchanges, Recap
     /insights    Insights   — cited insight cards grouped by type + window picker (0.4.5 P9 rewrite)
     /settings    Settings   — category sidebar over `routes/settings/`: General · Capture · Privacy · Intelligence · Models · Data · Integrations · Advanced (0.4.5 D7/D9)
     *            NotFound

 ┄ FlowOverlay (0.3.0) — a SEPARATE always-on-top hotkey window, NOT a NavRail route: Ctrl+Alt+Z
                         summons instant search-as-you-type / Ask over content text; Esc dismisses,
                         Enter jumps to the Moment; an empty query shows the where-was-i strip.
 ┄ Tray (0.3.2)       — an OS surface, not a window/route: passive capture-state icon + menu (03 §7d)
```

**IA change (0.4.5 D7).** The NavRail carries six entries — **Deck · Ask · Work · Recall (search + timeline) · Insights · Settings** — and **Session and Moment stay detail routes**: reached from a search result, the timeline, a thread or the Deck's Jump-back card, never as rail items. The 0.3.2-era "no new route" fence (D13-of-0.4.0, which kept sessions inside existing routes) is **lifted** for this arc: `/ask`, `/work`, `/work/:id` and the rewritten `/insights` are new routes, and `routes/settings/` replaces the single Settings view. Recall keeps its shipped surfaces — Search and Reports — with the timeline browser as its second surface under the same rail entry (`/timeline`); whether those paths merge into one is P7's call, and the existing deep links (`/timeline/:id`, `/timeline/session/:id`) keep working either way.

Rules: one primary action per screen; every route is reachable from NavRail or a link; no orphan
screens; deep-linkable (real routes, `/timeline/:id` shareable within the app).

### Screen → job (v2)
One job per screen, one primary action. A screen whose job cannot be stated in one line is two screens. `§4` owns each screen's loading / empty / error / partial / populated states.

| Route / surface | Screen | The job (one line) | Primary action |
|---|---|---|---|
| `/` | Deck | Answer "what was I doing, and what now?" from the last hours | Jump back to that moment |
| `/ask` | Ask v2 conversation | Answer a question across threads, sessions and frames, with citations | Ask (submit a turn) |
| `/work` | Work list | Show what is in flight, most recently touched first | Open a thread |
| `/work/:id` | Thread page | Explain one thread — what it is, what is unfinished, what it connects to | Ask scoped to the thread |
| `/recall` | Recall · Search | Find one specific thing you saw | Open the moment |
| `/recall` | Recall · Reports | Turn a span into a written recap | Generate |
| `/timeline` | Recall · Timeline | Show *when* things happened at a glance | Scrub / select a band |
| `/timeline/:id` | Moment (detail) | Read one captured moment in full | Open the session it belongs to |
| `/timeline/session/:id` | Session (detail) | Reconstruct one work session | Recap |
| `/insights` | Insights v2 | Surface a pattern worth knowing, each with its evidence | Open an insight's evidence |
| `/settings/*` | Settings (8 categories) | Change one behaviour and see its effect | Edit one row (it autosaves) |
| Flow overlay | FlowOverlay | Recall without leaving the current app | Jump to the Moment |
| Tray | Tray | Control capture and models from the OS shell | Pause / Resume |

**Settings v2 (0.4.5 D7/D9; P7 builds it).** Settings is one shell with a **category sidebar**: **General · Capture · Privacy · Intelligence · Models · Data · Integrations · Advanced**, implemented as `routes/settings/`, one file per category (none over ~300 lines) and a search across all settings that finds every key by label and by key name (gap `07` #103). Every category uses **one row pattern**: label, one-line plain-language effect, control, "changed" marker with reset-to-default. **Autosave per field with an undo toast, and there is no global Save button** (0.4.5 D9); a failed save keeps the typed value in place, explains, and retries. **Destructive actions** (delete history, revoke a credential, remove an output folder) sit behind a confirm dialog that names exactly what is destroyed (0.4.5 D9). Live status shows inline where a setting has a runtime effect: hotkey registered, API listening on, model loaded. `api.*` and the Telegram/Obsidian credentials live under **Integrations**; `file-only` keys appear read-only under **Advanced** with the `advanced.toml` path. Membership is normative in `03 §8`: only the 26 UI keys are controls, `ui.appearance` among them; everything else is file-only or gone, with no unknown-key tolerance (0.4.5 D6).

**Identity and navigation notes below are historical (0.4.5 D4/D5/D7).** The rest of this section records what shipped in 0.2.x–0.4.2. Its **behavioural** rules still bind — states, keyboard paths, privacy, no badge counts, one scroll context, honest empty — but every **identity or navigation** reference in it (signal-orange accent, Windows-native fonts, the Scanline signature, the 0.3.2 two-tier Settings, the 0.4.0 "no new route" fence) is superseded by `§1`/`§2` and by the v2 IA above.

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
| Setup indicator + Settings · Models setup details (2026-10-20, `03 §7g`) | nothing active or failed → **zero presence** (no chip; details list nothing) | an operation `failed` → danger-toned chip "Setup failed" until that operation retries; details show a plain cause line + the next step (engine: "resolve the problem, then restart ScreenSearch"; answer model: the existing "Load answer model" action; vision model: the existing "Start vision tagging" action) + an expandable diagnostic detail | several operations at once → one chip summarizing the most urgent (failed > active); details list each operation independently | chip copy "Preparing local AI" + the active step ("Downloading engine 42%" only when bytes and total are both known, otherwise indeterminate); details = per-operation stage (Checking → Downloading → Verifying → Installing → Done; embeddings "Preparing search model", never a percentage); loading = the snapshot query is pending → no chip, skeleton line in details; chip is a link/button to Settings; progress is `role="progressbar"` with values only when determinate; one polite live region announces phase changes, never byte ticks; reduced-motion disables the indeterminate animation |
| Ask v2 conversation (0.4.5, `03 §7i`) | no conversation yet → prompt invites a question (or a premade card); a saved thread with no turns shows the same invitation | sidecar unavailable → "answer model not loaded; load it?"; a failed turn keeps the question in the thread with a retry, never a lost turn | streaming (tokens arriving) → citations land as they resolve; scope chips show the window/thread/entity actually applied | persisted `ask_threads`/`ask_turns`; every turn cites **session ids and frame ids**; a turn with no citation is a defect; scope chips (time window · thread · entity) are editable and a follow-up reuses the previous window/entities unless the question overrides them; entity scope degrades to plain text when the graph is empty |
| Work list (0.4.5, `03 §7h`) | "No threads yet — they appear once enough sessions share a subject" (also the honest state when clustering has not run) | load failed → retry, keep sort | backfill/clustering running → rows appear as they land, with a counts line ("N of M sessions clustered") | sortable by last touched / time spent; low-confidence threads say so; rename / merge / split are written to `thread_overrides` and always beat the clusterer |
| Thread page (0.4.5, `03 §7h`/`§7j`) | thread has no sessions in this window → widen the range | thread missing or merged away → explain the merge + link to the surviving thread | some sessions still clustering, or the scoped Ask has not assembled context yet → labeled | session timeline + entities + open loops + connected threads + Ask scoped to the thread; every rendered claim links to its evidence; rename/merge/split confirm (destructive of the current grouping) |
| Insights v2 (0.4.5, `03 §7j`) | "Not enough history yet" (honest) or "no insights for this window" | generation failed → retry, keep the picked window | window partially computed → labeled as partial | cited cards grouped by the five types (thread activity · open loops · recurrence · semantic neighbours · resumption); each card opens its evidence; never a fabricated chart, never a notification, streak, score or grade (0.4.5 D13) |
| Settings · General (0.4.5) | — (every control holds a value) | save failed → value stays in place, inline explain + retry (there is no global Save) | a setting applies asynchronously → row reads "applying" until the runtime confirms | run at startup · close to tray · version + update status (the 0.3.2 App content folds in here); autosave per field + undo toast (0.4.5 D9) |
| Settings · Capture (0.4.5) | capture paused → rows stay editable, status line says "Paused" | save failed (interval, monitors, hotkey) → keep the form, retry; a hotkey that will not register is a loud warning + toast, never silent | monitor list refreshing after a hot-plug → the prior list stays usable | interval · monitors · event-driven master toggle · hotkeys with the live cross-chord conflict warning (`07` #100); an unplugged monitor degrades honestly (`07` #91 stays out of scope) |
| Settings · Privacy (0.4.5) | no excluded apps → "Nothing excluded yet" + add affordance | app picker failed → keep the list, retry | an app just added but not yet confirmed by the capture gate → labeled pending | excluded apps · pause on lock; ScreenSearch always excludes its own window and that is not a setting (`03 §7b`) |
| Settings · Intelligence (0.4.5) | nothing scheduled → "enrichment runs on demand" | job status unavailable → retry, controls stay live | a job running → progress + cancel, per operation | vision timer + interval, schedule (on-demand / timer / idle), and the graph/insight compute readout: normalization, session embeddings and insights run on demand or on idle under the existing scheduler and pressure throttle, never on the capture hot path (0.4.5 D11) |
| Settings · Models (0.4.5) | nothing active or failed → **zero presence** (no chip, details list nothing) | download/verify failed → cause line + the next step, retry | several operations → one chip for the most urgent (failed > active), details list each independently | vision tier · answer tier · show thinking · per-model load/unload; the 2026-10-20 `SetupIndicator`/`SetupDetails` row above still binds verbatim |
| Settings · Data (0.4.5) | no saved reports → "Nothing saved yet" (the section still explains setup) | export/backup failed → keep the panel, explain, retry | retention change applying → "applying" until the sweep confirms | retention days · max image width · Export (`export_data`; works with the API off) · Memory reports (`§11`); deleting history is destructive → confirm naming the span and row counts (0.4.5 D9) |
| Settings · Integrations (0.4.5) | API off → "API disabled"; no read token → Create action; both independent | port in use → loud warning + toast + inline "pick another port"; credential action failed → no secret echo | enabled but bind error → show both facts, never claim connectivity | `api.enabled` · `api.port` · `api.token` · `api.bind_address` · the read credential (`§12`'s table binds verbatim) · Telegram and Obsidian credentials (write-only, never prefilled, `§11`) |
| Settings · Advanced (0.4.5) | no tuning keys set → "everything is at its default" | `advanced.toml` unreadable → explain, show the path, state that defaults still apply | the file holds an unknown/retired key → rewritten once with it removed, no error, no migration (0.4.5 D6) | read-only display of the `file-only` keys with the `advanced.toml` path; the file is the control surface and there is no Save |

**Settings v2 relocates, it does not renegotiate (0.4.5 D7/D9).** The rows above for Settings, Settings · Performance throttle, Settings · HTTP API, Settings · App and Settings · Models setup details remain in force: the eight categories are a new grouping and a new row pattern (autosave per field + undo toast, no global Save, destructive confirm), not new contracts for those controls.

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
**Placeholder — P2 writes the inventory here as the primitives land, after the `§1` direction pick (0.4.5 D4).** Two inputs are authoritative now and do not wait for the pick:

- **The primitive list to build** is P2's item 5 in `docs/0.4.5.md` (`:150-155`): Button (primary / secondary / ghost / danger × sm / md, icon and loading states), IconButton, Field, TextInput, Textarea, Select, Combobox, Toggle, Checkbox, Radio group, Segmented control, Slider with numeric input, Chip/Tag, Badge, Tooltip, Popover, Dialog, Sheet, Toast, Tabs, Panel/Card, List row, Table, Skeleton, EmptyState, ErrorState, Kbd, Divider. Every primitive ships all interaction states, keyboard behaviour, a focus ring, reduced-motion handling and WCAG-AA contrast in every theme.
- **The shipped v1 inventory** — shell, primitives, domain components, and the arc-by-arc notes explaining why each exists — is preserved verbatim at `specs/archive/UI_REFERENCE_v1_identity.md`. It is provenance, not a list to keep: P10 deletes dead components left behind by the redesign and removes `tokens.compat.css` (P2 item 7 / P10 in `docs/0.4.5.md`), and P2's `tokens.compat.css` aliases keep unconverted screens rendering in the meantime.

Still binding without a pick: **each component owns one job** — a label labels, an example demonstrates, nothing does double duty; components reference semantic and component tokens only (`§2`); and the *behavioural* contracts these components carried do **not** lapse with their v1 visuals — full state coverage per `§4`, one scroll context and no nested scrollbars per `§8`, honest empty states, no badge counts. The v2 inventory is written here by P2 as primitives land, extended by P9 (`ConnectionsPanel`, `EntityChips` — built there, placed in Session and Moment by P10) and by P10's refonted domain components.

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
disables scan/ambient (incl. session-band motion); respects the OS colour scheme for every theme the
v2 pick ships (v1 was dark-only by design; superseded by 0.4.5 D5);
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

### Shell layout contract — v2 restatement (0.4.5; binds P2 and everything after it)
The 0.3.2 contract above stays in force **verbatim**; v2 restates the shell it is verified against, so P2's layout tokens and P7's shell are built to one sentence rather than to the roadmap.

- **Fixed shell, one scrolling pane:** `StatusRail` across the top (telemetry only, one row, chip widths stable) and `NavRail` down the left (six entries per `§3` — Deck · Ask · Work · Recall · Insights · Settings — plus the version footer and the update presence). Both rails are fixed; only the content pane scrolls, and a route never introduces a second vertical scroller.
- **12-column content grid:** wide screens lay the content pane out on a **12-column grid** with container widths taken from `§2`'s scales; below the wide breakpoint the grid collapses to one column instead of scrolling sideways. Every grid item carries `min-w-0` — the pre-block exemption above depends on it, and so does the no-horizontal-scrollbar rule.
- **One density token:** row height, control height and grid gap come from a single **density token** (`comfortable` / `compact`, `§2`); no screen hardcodes a row height. `comfortable` is the default; `compact` is a choice in Settings, not a per-screen tweak.
- **Same verification, wider matrix:** the screenshot matrix keeps its size/DPI set and extends to every v2 route and state — Deck, Ask v2 conversation, Work list, Thread page, Recall (search + reports) and the timeline, Moment, Session, Insights v2, and each of the eight Settings categories — in every theme the `§1` pick ships. The one-scroll, no-horizontal-scrollbar and no-CLS rules are unchanged, including the timeline layer's four reserved band lanes (or the equivalent fixed reservation of its v2 successor, `docs/0.4.5.md` P10).

**The performance budgets above are unchanged by v2** — initial JS ≤ 250 KB gzip, route-split per page, virtualized frame grids and timeline, lazy `FrameImage`, results rendered < 100 ms after data, interaction latency < 100 ms, no layout shift on data arrival. A v2 direction that cannot meet them is a regression against this section, not a renegotiation of it.

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
7. The timeline scrubs smoothly and reflects real capture density — the Scanline Timeline or its v2
   successor, whichever the `§1` pick names (0.4.5 D4).
8. Verified by **running the app and capturing screenshots** of each screen in each state — not by
   "it compiles" (`04 §6`).
9. The `§8` shell layout contract holds: one scroll context per route, fixed rails, no nested or
   horizontal scrollbars at supported sizes/DPI, no layout shift on load (0.3.2, D9).

---

*Companion to `00`–`04`. Aesthetic direction is intentional and subject-grounded; if a future change
makes the UI read as a templated dark dashboard, that's a regression against §1.*

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

## 12. External-agent read credential (approved contract; shipped in v0.4.2, native UI verification pending)

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
