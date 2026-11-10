# Dev route-state override

A **DEV-ONLY** mechanism for deterministically forcing the `loading` and `error`
render states of any P5 route, so an auditor can verify every view's state
handling without having to engineer a slow backend or a thrown command. It closes
[`specs/07_KNOWN_GAPS.md` #43](../specs/07_KNOWN_GAPS.md).

> **Production safety.** The entire feature is stripped from production builds.
> Every entry point guards on `import.meta.env.DEV`, which Vite replaces with the
> literal `false` in a production build; the dead branch and the modules it reaches
> (`ui/src/lib/dev/*`, `DevStateBadge`) are then dropped by the tree-shaker. The
> bundle ships **no** override code — the `__devState` string is absent from the
> emitted JS (verified in CI-style build checks; see "Verifying the prod strip").

## What it does — and what it deliberately does NOT do

The override forces only the **query-result flags** at the single `useQuery` seam
(`ui/src/lib/ipc/queries.ts`). It **never fabricates a payload**:

- `loading` → `isPending/isLoading = true`, `isFetching = true`, `data = undefined`,
  `status = "pending"`. The view renders its skeleton.
- `error` → `isError = true`, `error = Error("dev: forced route error")`,
  `data = undefined`, `status = "error"`. The view renders its error state. The
  **real `refetch` is preserved**, so a view's "Retry" button still calls the live
  query (the forced state clears once real data resolves and you remove the param).

Because no data is invented, the other three states — `empty`, `partial`,
`populated` — are **only ever reached through real data**, never through this
override. This honours the gap's "no mocks in production paths" constraint: there
are no fixtures, and even in dev nothing fake is substituted for backend data.

## Usage

Append a query param to any route URL:

| Param | Values | Effect |
| --- | --- | --- |
| `__devState` | `loading` \| `error` | Forces that state on **every** read query in the view. |
| `__devStateKey` | a query-family head (e.g. `timeline`, `frame`, `search`) | Optional. Scopes the override to a single query family, leaving the rest of the view live. |

The family head is the first segment of the TanStack Query key
(`ui/src/lib/ipc/queryKeys.ts`) — e.g. `["timeline", range, n]` → `timeline`,
`["frame", id]` → `frame`. Unrecognised values are ignored (inert no-op).

A small **DEV STATE** badge appears in the bottom-left corner whenever an override
is active, so it is always obvious the view is being forced rather than reflecting
live data.

### Per-route verification table

URLs assume the dev server origin (e.g. `http://localhost:1420`). `loading` /
`error` are forced via the override; `empty` / `partial` / `populated` are reached
with **real data** as noted.

| Route | Path | Force `loading` | Force `error` | `empty` / `partial` / `populated` via REAL data |
| --- | --- | --- | --- | --- |
| **Deck** | `/` | `/?__devState=loading` | `/?__devState=error` | The view gates on `useReadiness`; to force only its top-level skeleton/error use `__devStateKey=readiness`. **empty:** fresh DB (no frames) → onboarding "No captures yet". **partial:** capturing but nothing tagged yet (`total>0, tagged=0`) → "enrichment pending". **populated:** a range with tagged frames → today's aggregates + recents. |
| **Timeline** | `/timeline` | `/timeline?__devState=loading` | `/timeline?__devState=error` | Scope to the ribbon with `__devStateKey=timeline`. **empty:** pick a range with no captures (e.g. `Today` on a fresh DB) → "No captures in this range". **partial:** a populated window while thumbnails (`frames`) are still resolving → "Loading thumbnails…". **populated:** a range with captures. |
| **Moment** | `/timeline/:id` | `/timeline/123?__devState=loading` | `/timeline/123?__devState=error` | Scope the main frame load with `__devStateKey=frame`. **empty:** an unknown id → `getFrame` returns `null` → "Moment not found". **partial:** a retention-degraded frame (`image_purged`) → text+layout reconstruction from `frameSpans` instead of the image; neighbour strip (`frameContext`) filling in. **populated:** a normal frame with image + neighbours. |
| **Insights** | `/insights` | `/insights?__devState=loading` | `/insights?__devState=error` | Scope with `__devStateKey=insights`. **empty:** a range with no captures (`total_frames=0`) → "Not enough history yet". **partial:** a range where `tagged < total` → "tagged only" charts + `% tagged` chip. **populated:** a fully-tagged range → trend + top-apps + activities. |
| **Recall** | `/recall` | `/recall?__devState=loading` | `/recall?__devState=error` | Forces the **search** query; scope with `__devStateKey=search`. Search is idle until you type, so enter a query first, then append the param. **empty:** a query with no hits → "No matches". **populated:** a query that matches frames. For the **Ask** path, the grounded-answer state is driven by `useAsk` (a non-query state machine, intentionally NOT overridable); reach its error/empty states for real by stopping the sidecar (Settings → unload, or kill the process) so `sidecar.status` becomes `unavailable`/`error`. |
| **Settings** | `/settings` | `/settings?__devState=loading` | `/settings?__devState=error` | The form gates on `useSettings`; scope its skeleton/error with `__devStateKey=settings`. Sub-panels load independently — `useMonitors`, `useSidecarDevices`, `useThrottleStatus`, `useTextFilterStats` each have their own family head if you want to force one panel (e.g. `__devStateKey=textFilterStats`). **empty/partial/populated:** real, e.g. an empty `textFilterStats` list vs. a populated one; an idle vs. ready sidecar for the device list. |

> **Note (Deck / Settings).** A bare `?__devState=loading` forces *all* of the
> view's read queries at once, which is the right default for a full-screen
> skeleton/error check. Use `__devStateKey` when you want to exercise one panel's
> state while the rest of the screen stays live.

## How it is wired

- `ui/src/lib/dev/devState.ts` — pure (no React): parses `__devState` /
  `__devStateKey` from a search string and applies the forced flags to a
  `UseQueryResult`. Type-correct against TanStack Query v5's discriminated result
  union (no `any`).
- `ui/src/lib/dev/useDevStateOverride.ts` — `useMaybeOverride(result, queryKey)`.
  Returns early in production via `import.meta.env.DEV` (the first statement), then
  reads `window.location.search` directly — it calls **no** React hooks, so the
  production helper folds to a plain `return result` (no router subscription on the
  17 query consumers) and the read queries never require a `<Router>` context.
- `ui/src/lib/ipc/queries.ts` — every read hook ends with
  `return useMaybeOverride(q, queryKey)`. Mutations (`mutations.ts`) and the
  `useAsk` / `useReport` state machines are **not** wrapped.
- `ui/src/components/shell/DevStateBadge.tsx` — the corner badge, mounted in
  `AppShell` behind `{import.meta.env.DEV && …}`.

## Verifying the prod strip

After a production build (`npm run build`), the override must not appear in the
emitted bundle:

```sh
# From ui/ — expect NO matches (exit code 1 from grep means "not found", which is success).
grep -r "__devState" dist/assets/*.js
```

If that prints any line, the dead-code elimination did not run — check that the
guards are literal `import.meta.env.DEV` checks (not aliased through a variable).

## Isolated startup checks (0.4.6 stabilisation)

The route-state override above does **not** isolate persistent data. A plain `npm run dev`
uses the installed app's profile. For startup checks, create an external Tauri config overlay:

```json
{
  "identifier": "app.screensearchv2c.devlaunch",
  "productName": "ScreenSearch DevLaunch"
}
```

Both fields are required: `identifier` separates Roaming data, local WebView2 data and the
single-instance lock; `productName` separates the Windows autostart Run value. Without the
second field, a fresh profile's startup reconciliation can delete the installed `ScreenSearch`
Run value. Before the first launch, create only the isolated profile's `advanced.toml` with
`[enrich]`, `embed_text = false` and `vision_idle_enabled = false`, so startup does not load models.

```powershell
Remove-Item Env:HF_HOME -ErrorAction SilentlyContinue
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
npm run dev -- --config C:/chosen/devlaunch.overlay.json
```

Never open the installed database to check isolation. Observe the isolated log path and stop
only the process tree the check started, not all processes named `screensearch`.
