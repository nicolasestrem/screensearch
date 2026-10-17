// Central registry of TanStack Query keys. Prefix arrays are stable so live
// events can invalidate a whole family (e.g. `["timeline"]` matches every
// bucket-count/range variant) — see useLiveEvents.
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TimeRange } from "../../bindings/TimeRange";
import type { SessionQuery } from "../../bindings/SessionQuery";

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
};
