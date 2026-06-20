# Frontend — Command Deck UI

**Version:** 3.0 (greenfield rebuild, v0.5.0)
**Identity:** "Command Deck" — an instrument / mission-control surface for your own
on-device screen memory.

The web UI was rebuilt from scratch in v0.5.0. This document covers both the
**design system** (palette, type, components) and the **frontend architecture**
(stack, routing, data layer, pages). For where a feature lives by file, see
[Code Navigation](CODE_NAVIGATION.md#frontend-command-deck-ui).

---

## 1. Design language

A deliberately non-generic, telemetry-styled dark UI. The personality comes from
mono-dominant data readouts and a single warm accent, not from glow or gradients.

### Palette (warm graphite + signal orange)

Defined once as CSS custom properties in `src/index.css` and mirrored in
`tailwind.config.js` as Tailwind colors. Dark-only for this release.

| Token | Hex | Use |
|---|---|---|
| `void` | `#17171B` | app background (warm near-black, not blue-black) |
| `panel` | `#1F1F25` | panels / cards |
| `panel2` | `#1A1A1F` | rails, elevated surfaces |
| `rule` / `rule2` | `#32323C` / `#3E3E48` | borders, gridlines |
| `ink` / `ink2` | `#ECE7DF` / `#C9C3B8` | primary / secondary text |
| `muted` / `faint` | `#A39E94` / `#928C81` | labels, captions (both ≥ 4.5:1) |
| `accent` / `accent2` | `#FF6A3D` / `#C24E2A` | the single signal colour (playhead, focus, active, links) |
| `ok` / `warn` / `alert` | `#7FB87A` / `#E0A33E` / `#E5564C` | real system states only |
| `act.code/research/comms/reading/media/idle` | see config | vision activity-type categories |

Status colours (`ok`/`warn`/`alert`) are reserved for genuine system state, never
decoration. Accent is used sparingly.

### Typography

Windows-native faces — **no web fonts are downloaded at runtime** (the app is
local-only). Three roles:

- **Display** — `Bahnschrift` (a DIN-style technical grotesque): headings, big
  metric numbers, nav, section labels.
- **Data** — `Consolas` / `Cascadia Mono`: timecodes (`HH:MM:SS`), telemetry rows,
  metrics, status chips. Mono is the dominant voice.
- **Prose** — `Segoe UI`: body copy and answer/report text.

The scale spans ~10.5px labels to 40px metric numbers. All text meets WCAG AA
contrast on its background (verified: small labels 4.9–5.2:1, headers ≥ 9:1).

### Layout & motion

- **0px border radius** on panels (instrument feel); `rounded-full` only for status
  dots.
- Panels are framed with thin `rule` borders and a small-caps mono section header.
- Motion is restrained and respects `prefers-reduced-motion`: the `REC` pulse, a
  blinking cursor, the timeline playhead, and a subtle row-in.

### Signature element — the Scanline Timeline

`src/components/ScanlineTimeline.tsx`. A 24-hour track with:

- a **frame-density ribbon** (per-15-min-bucket counts);
- **activity-type colour bands** derived from the vision `activity_type`;
- a live **"now"** line and a draggable **playhead** (scrub → filter).

It is the through-line of the app: compact on the Deck, full-width and interactive
on the Timeline.

---

## 2. Frontend architecture

### Stack

| Concern | Choice |
|---|---|
| Build / framework | Vite 8 + React 18 + TypeScript |
| Routing | `react-router-dom` (deep-linkable routes) |
| Server state | TanStack Query (polling for live status) |
| HTTP | a typed `fetch` client (`src/lib/api.ts`) — no axios |
| UI state | Zustand (`src/lib/store.ts`) — palette, view mode, AI provider config |
| Markdown | `react-markdown` (answers / reports) |
| Icons | `lucide-react` |

The production bundle is embedded in the Rust binary via `rust-embed`; see the
[Developer Guide](developer-guide.md#embedded-frontend-build).

### Routes / pages

| Route | Page | Purpose |
|---|---|---|
| `/` | Deck | Mission-control overview: status rail, Ask box, index/vision coverage, scanline timeline, activity feed, apps & sites. |
| `/recall` | Recall | *Ask your screen* (RAG via `POST /api/generate`) with cited frame chips; plus daily/weekly/custom **Report** mode (`POST /api/ai/generate`). |
| `/timeline` | Timeline | Interactive scanline + searchable, filterable contact sheet (date, app, monitor, activity, fts/semantic/hybrid). |
| `/timeline/:id` | Moment | Per-frame detail: screenshot, vision panel, OCR, metadata, tags, on-demand analyze. |
| `/insights` | Insights | Real analytics: activity mix, apps & sites (incl. per-site via `browser_url`), hourly rhythm. |
| `/settings` | Settings | Capture, monitors, excluded apps, retention; semantic-search, vision model picker, AI provider + test, local answer-engine model/server controls. |

A global **⌘K command palette** (`src/app/shell/CommandPalette.tsx`) offers
search + ask + navigation. A **readiness banner**
(`src/app/shell/ReadinessBanner.tsx`) surfaces startup/download progress from
`GET /api/system/readiness`.

### Data layer

- `src/lib/api.ts` — one typed client per endpoint. **Collection routes have no
  trailing slash** (`/api/frames`, `/api/tags`, `/api/settings`, `/api/search`);
  sub-resources use the path form (`/api/frames/:id`). Frame images are fetched as
  object URLs and cached.
- `src/lib/hooks.ts` — TanStack Query wrappers (status endpoints poll on an
  interval; data endpoints are staleness-bounded).
- `src/lib/types.ts` — TypeScript mirrors of the Rust serde structs.
- `src/lib/display.ts` — adapters from `Frame` / `SearchResult` to a common
  display shape used by the contact sheet and feed.
- `src/lib/activity.ts` — maps free-form vision `activity_type` strings to a small
  set of canonical, coloured buckets.

### Honest states

There is **no mock data**. Every panel renders real API data or an explicit
loading / empty / error state (e.g. "Not enough captured yet", "Could not load
settings", "Enable semantic search"). When a list is capped, the UI says so.

---

## 3. Conventions

- Reuse the primitives in `src/components/Panel.tsx` and `src/components/ui.tsx`
  (`Panel`, `PanelHeader`, `StatusDot`, `CoverageBar`, `ActivityBadge`, `Button`,
  `Metric`, `Empty`, `Spinner`, `ErrorNote`) rather than re-styling.
- Reference colours via Tailwind tokens (`text-ink`, `border-rule`, `bg-accent`);
  use the CSS variables only for inline/SVG styles that Tailwind can't express.
- Keep the accent rare. If a new element needs colour, ask whether a status colour
  or a neutral is more honest first.
