// Central registry of TanStack Query keys. Prefix arrays are stable so live
// events can invalidate a whole family (e.g. `["timeline"]` matches every
// bucket-count/range variant) — see useLiveEvents.
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TimeRange } from "../../bindings/TimeRange";
import type { SessionQuery } from "../../bindings/SessionQuery";
import type { ThreadListQuery } from "../../bindings/ThreadListQuery";
import type { EntityListQuery } from "../../bindings/EntityListQuery";
import type { InsightListQuery } from "../../bindings/InsightListQuery";

export const queryKeys = {
  readiness: ["readiness"] as const,
  jobStats: ["jobStats"] as const,
  storageStats: ["storageStats"] as const,
  monitors: ["monitors"] as const,
  sidecarDevices: ["sidecarDevices"] as const,
  sidecarStatus: ["sidecarStatus"] as const,
  modelDownload: ["modelDownload"] as const,
  throttleStatus: ["throttleStatus"] as const,
  hotkeyStatus: ["hotkeyStatus"] as const,
  settings: ["settings"] as const,
  // `get_settings_defaults`: the as-built `Settings::default()`. Never changes at
  // runtime, so it is read once and stays out of every invalidation set.
  settingsDefaults: ["settingsDefaults"] as const,
  advancedConfig: ["advancedConfig"] as const,
  textFilterStats: ["textFilterStats"] as const,
  // `*Prefix` keys match every variant of a family for bulk invalidation (e.g.
  // a `capture_tick` invalidates all timeline ranges/bucket-counts at once; a
  // completed enrichment job invalidates all frame/search/insights variants).
  searchPrefix: ["search"] as const,
  search: (query: SearchQuery) => ["search", query] as const,
  timelinePrefix: ["timeline"] as const,
  timeline: (range: TimeRange, bucketCount: number) =>
    ["timeline", range, bucketCount] as const,
  framePrefix: ["frame"] as const,
  frame: (frameId: number) => ["frame", frameId] as const,
  // Session list + base/summary detail variants. Capture/retention invalidates the
  // whole family because open rows accrete and representative frame images can expire.
  sessionsPrefix: ["sessions"] as const,
  sessionListsPrefix: ["sessions", "list"] as const,
  sessionList: (query: SessionQuery) => ["sessions", "list", query] as const,
  sessionDetailsPrefix: ["sessions", "detail"] as const,
  sessionDetail: (sessionId: number, includeSummary: boolean) =>
    ["sessions", "detail", sessionId, includeSummary] as const,
  sessionRecap: (sessionId: number) =>
    ["sessions", "recap", sessionId] as const,
  // The session's work-graph context (`03 §7h`), under the same `sessions` head so a
  // `sessionsPrefix` invalidation (capture/retention) refreshes it with its siblings.
  sessionGraphPrefix: ["sessions", "graph"] as const,
  sessionGraph: (sessionId: number) =>
    ["sessions", "graph", sessionId] as const,
  // A frame's text spans (for the text+layout reconstruction of a degraded capture).
  frameSpansPrefix: ["frameSpans"] as const,
  frameSpans: (frameId: number) => ["frameSpans", frameId] as const,
  // Frame *lists* (FrameMeta) — distinct from the singular `frame` detail above.
  // A new capture (`capture_tick`) invalidates every range/limit variant at once.
  framesPrefix: ["frames"] as const,
  frames: (range: TimeRange, limit: number) =>
    ["frames", range, limit] as const,
  // A frame's neighbour context (closest captures on each side). A new capture
  // (`capture_tick`) can add a neighbour, so this family is invalidated alongside
  // the frame lists above.
  frameContextPrefix: ["frameContext"] as const,
  frameContext: (at: number, halfWindowMs: number, limitEach: number) =>
    ["frameContext", at, halfWindowMs, limitEach] as const,
  insightsPrefix: ["insights"] as const,
  insights: (range: TimeRange, bucketCount: number) =>
    ["insights", range, bucketCount] as const,
  // P9 intelligence (`03 §7h`, §7i.4, §7j). `workPrefix` covers the whole work graph —
  // the Work list and every thread page — so a completed `graph_cluster` job refreshes
  // both with one invalidation. `insightListPrefix` is the P8 insight rows (distinct from
  // the legacy `insights` counts above, which a later slice removes with its route).
  workPrefix: ["work"] as const,
  threadList: (query: ThreadListQuery) => ["work", "list", query] as const,
  threadPage: (threadId: number) => ["work", "thread", threadId] as const,
  entitiesPrefix: ["entities"] as const,
  entityList: (query: EntityListQuery) => ["entities", "list", query] as const,
  entitiesByIds: (ids: number[]) => ["entities", "byIds", ids] as const,
  insightListPrefix: ["insightList"] as const,
  insightList: (query: InsightListQuery) => ["insightList", query] as const,
  // Ask conversations: `askPrefix` invalidates the list and every turn family at once
  // (a sent turn changes both, and a deleted thread removes its turns).
  askPrefix: ["ask"] as const,
  askThreads: ["ask", "threads"] as const,
  /** Every conversation's turn list — an `askTurns(id)` key starts with this prefix. */
  askTurnsPrefix: ["ask", "turns"] as const,
  askTurns: (threadId: number) => ["ask", "turns", threadId] as const,
  // 0.3.0 flow recall (PR6): the where-was-i card/strip and the Intentions strip. A
  // `marks_changed` event invalidates `marks`; the overlay re-fetches `whereWasI` on
  // each summon (it depends on the live foreground context, not a data-change event).
  whereWasI: ["whereWasI"] as const,
  marks: ["marks"] as const,
  // API panel status; mutations and api_status_changed
  // refresh it, including completion of asynchronous startup.
  apiStatus: ["apiStatus"] as const,
  // Availability only. Read-token secrets must never enter a query cache.
  apiReadTokenStatus: ["apiReadTokenStatus"] as const,
  // 0.3.2 auto-update (PR2, #69): the updater snapshot behind the NavRail presence
  // indicator + the Settings · App section. Kept live by the `update_status_changed`
  // event; the manual check mutation also writes it (`03 §11b`).
  updateStatus: ["updateStatus"] as const,
  // Setup visibility (2026-10-20, `03 §7g`): the setup snapshot behind the
  // StatusRail chip + the Settings · Models setup details. Kept live by the
  // whole-snapshot `setup_changed` event; revision-guarded both write paths.
  setupStatus: ["setupStatus"] as const,
};

/**
 * Every query family a finished graph or insight job can change (`03 §7h`, `§7j`), in
 * one list so a graph-backed read cannot be added without landing in the invalidation.
 *
 * The live-event handler invalidates exactly this list, debounced, when a
 * `GRAPH_JOB_KINDS` job completes. It lives beside the keys rather than in the handler
 * because a key is what can go missing: the session graph sits under the `sessions`
 * head, so the `work`/`insightList`/`entities` prefixes never covered it and a Session
 * or Moment left open while the graph caught up kept rendering stale entities and
 * "still clustering" — with no polling to save it and refetch-on-window-focus disabled
 * in `app/providers.tsx`.
 */
export const GRAPH_JOB_INVALIDATIONS = [
  queryKeys.workPrefix,
  queryKeys.insightListPrefix,
  queryKeys.entitiesPrefix,
  queryKeys.sessionGraphPrefix,
] as const;

/**
 * Every query family a completed settings save can change, in one list so a
 * settings-derived read cannot be added without landing in the save invalidation.
 *
 * `set_settings` writes the whole `Settings` struct and hot-applies the typed keys
 * (`src-tauri/src/lib.rs`): it re-registers the shell hotkeys, widens or narrows the
 * local API, and starts or stops the throttle governor. Each of those owns a query of
 * its own, and none is covered by the `settings` key. The throttle was the one that
 * went missing: a readout built on `useThrottleStatus()` kept its pre-save state until
 * the next governor tick, which forced a local "use the saved setting" workaround at
 * the call site. A key belongs here when a setting write changes the payload its query
 * reads; families fed by hardware or by an event only stay out.
 */
export const SETTINGS_SAVE_INVALIDATIONS = [
  queryKeys.settings,
  queryKeys.hotkeyStatus,
  // `api_*` are typed settings: a save that changes them starts, stops or rebinds
  // the listener (`set_settings` hot-applies through `apply_api_config`), so the API
  // panel's status must refresh too.
  queryKeys.apiStatus,
  // `throttle.enabled`: the save starts or stops the governor, so the readout's on/off
  // line and level must refetch. Without this key the cached status kept the pre-save
  // value until the next governor tick, which forced a local "use the saved setting"
  // prop at the readout call site.
  queryKeys.throttleStatus,
] as const;
