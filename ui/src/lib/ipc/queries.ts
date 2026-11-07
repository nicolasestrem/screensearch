// Read-side hooks. TanStack Query owns all server-state — one place for cache,
// loading, error, and refetch (UI_REFERENCE §6; no bespoke useEffect fetches).
//
// Each read hook's result is passed through `useMaybeOverride` — the single seam
// for the DEV-ONLY route-state override (KNOWN_GAPS #43). It is a no-op in
// production (const-folds away) and whenever no `?__devState=…` is present, so the
// live empty/partial/populated outcomes are untouched; only forced loading/error
// states flow through. Mutations and the Ask/Report state machines are NOT wrapped.
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import type { UseQueryResult } from "@tanstack/react-query";

import * as cmd from "./commands";
import { queryKeys } from "./queryKeys";
import { useMaybeOverride } from "../dev/useDevStateOverride";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TimeRange } from "../../bindings/TimeRange";
import type { SidecarStatus } from "../../bindings/SidecarStatus";
import type { ModelDownloadStatus } from "../../bindings/ModelDownloadStatus";
import type { ThrottleStatus } from "../../bindings/ThrottleStatus";
import type { HotkeyStatus } from "../../bindings/HotkeyStatus";
import type { Mark } from "../../bindings/Mark";
import type { UiResumeContext } from "../../bindings/UiResumeContext";
import type { ApiStatus } from "../../bindings/ApiStatus";
import type { UpdateStatus } from "../../bindings/UpdateStatus";
import type { SessionQuery } from "../../bindings/SessionQuery";
import type { SessionGraph } from "../../bindings/SessionGraph";
import type { SetupStatus } from "../../bindings/SetupStatus";
import type { ThreadListQuery } from "../../bindings/ThreadListQuery";
import type { EntityListQuery } from "../../bindings/EntityListQuery";
import type { InsightListQuery } from "../../bindings/InsightListQuery";
import type { AskThreadSummary } from "../../bindings/AskThreadSummary";
import type { AskTurn } from "../../bindings/AskTurn";
import { newerSetupStatus } from "../setup";

/** How many Ask conversations the list reads. Conversations are sparse rows like threads, so
 *  one page covers a long history; the command clamps to `1..=1000` regardless. */
const ASK_THREAD_LIST_LIMIT = 100;

/** Subsystem readiness; kept live by `readiness_changed` (see useLiveEvents). */
export function useReadiness() {
  const q = useQuery({
    queryKey: queryKeys.readiness,
    queryFn: cmd.getReadiness,
  });
  return useMaybeOverride(q, queryKeys.readiness);
}

/** Job-queue counts; kept live by `job_progress`. */
export function useJobStats() {
  const q = useQuery({
    queryKey: queryKeys.jobStats,
    queryFn: cmd.getJobStats,
  });
  return useMaybeOverride(q, queryKeys.jobStats);
}

/** Storage footprint; refreshed by capture/retention events. */
export function useStorageStats() {
  const q = useQuery({
    queryKey: queryKeys.storageStats,
    queryFn: cmd.getStorageStats,
  });
  return useMaybeOverride(q, queryKeys.storageStats);
}

/** Connected monitors for Settings. */
export function useMonitors() {
  const q = useQuery({
    queryKey: queryKeys.monitors,
    queryFn: cmd.getMonitors,
  });
  return useMaybeOverride(q, queryKeys.monitors);
}

/** llama.cpp device ids for advanced sidecar selection. */
export function useSidecarDevices(enabled = true) {
  const q = useQuery({
    queryKey: queryKeys.sidecarDevices,
    queryFn: cmd.listSidecarDevices,
    enabled,
    retry: 0,
  });
  return useMaybeOverride(q, queryKeys.sidecarDevices);
}

/**
 * Latest sidecar lifecycle status. There is no fetch command for this — it is an
 * event-sourced cache entry populated by `sidecar_status` (the queryFn just
 * preserves whatever the event last wrote, so a refetch never clobbers it).
 */
export function useSidecarStatus() {
  const qc = useQueryClient();
  const q = useQuery<SidecarStatus | null>({
    queryKey: queryKeys.sidecarStatus,
    queryFn: () =>
      qc.getQueryData<SidecarStatus | null>(queryKeys.sidecarStatus) ?? null,
    staleTime: Infinity,
  });
  return useMaybeOverride(q, queryKeys.sidecarStatus);
}

/**
 * Latest model-download progress (event-sourced, like {@link useSidecarStatus}). Populated
 * by the `model_download` event; `null` until a download starts. The queryFn preserves
 * whatever the event last wrote so a refetch never clobbers it.
 */
export function useModelDownload() {
  const qc = useQueryClient();
  const q = useQuery<ModelDownloadStatus | null>({
    queryKey: queryKeys.modelDownload,
    queryFn: () =>
      qc.getQueryData<ModelDownloadStatus | null>(queryKeys.modelDownload) ??
      null,
    staleTime: Infinity,
  });
  return useMaybeOverride(q, queryKeys.modelDownload);
}

/** Persisted settings. */
export function useSettings() {
  const q = useQuery({
    queryKey: queryKeys.settings,
    queryFn: cmd.getSettings,
  });
  return useMaybeOverride(q, queryKeys.settings);
}

/**
 * Read-only `file-only` settings projection (`03 §8`, 0.4.5 D6): the effective
 * `AdvancedConfig`, the `advanced.toml` path, and which keys came from the file.
 * Read once per app run - the file is loaded at startup - so it never refetches.
 */
export function useAdvancedConfig() {
  const q = useQuery({
    queryKey: queryKeys.advancedConfig,
    queryFn: cmd.getAdvancedConfig,
    staleTime: Infinity,
  });
  return useMaybeOverride(q, queryKeys.advancedConfig);
}

/**
 * Enrichment-throttle status: live CPU/GPU pressure, level, and whether the GPU is
 * monitored. Fetched once, then kept live by the `throttle_changed` event (see
 * useLiveEvents) which the governor emits each sample tick while enabled.
 */
export function useThrottleStatus() {
  const q = useQuery<ThrottleStatus>({
    queryKey: queryKeys.throttleStatus,
    queryFn: cmd.getThrottleStatus,
  });
  return useMaybeOverride(q, queryKeys.throttleStatus);
}

/** Shell hotkey registration status. Registration failures are surfaced in Settings. */
export function useHotkeyStatus() {
  const q = useQuery<HotkeyStatus[]>({
    queryKey: queryKeys.hotkeyStatus,
    queryFn: cmd.getHotkeyStatus,
  });
  return useMaybeOverride(q, queryKeys.hotkeyStatus);
}

/** Per-app text-filter suppression rates (PR3 guardrail). */
export function useTextFilterStats(enabled = true) {
  const q = useQuery({
    queryKey: queryKeys.textFilterStats,
    queryFn: cmd.getTextFilterStats,
    enabled,
  });
  return useMaybeOverride(q, queryKeys.textFilterStats);
}

/** Hybrid search; idle until there is a non-empty query (no empty-string calls). */
export function useSearch(
  query: SearchQuery,
  enabled = true,
  keepPrevious = false,
) {
  const queryKey = queryKeys.search(query);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.search(query),
    enabled: enabled && query.text.trim().length > 0,
    placeholderData: keepPrevious ? keepPreviousData : undefined,
  });
  return useMaybeOverride(q, queryKey);
}

/** Timeline density buckets; invalidated (debounced) by `capture_tick`. */
export function useTimeline(
  range: TimeRange,
  bucketCount: number,
  enabled = true,
) {
  const queryKey = queryKeys.timeline(range, bucketCount);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getTimeline(range, bucketCount),
    enabled: enabled && bucketCount > 0 && range.end > range.start,
  });
  return useMaybeOverride(q, queryKey);
}

/**
 * Lightweight frame list over a window, newest-first (timeline thumbnails, deck
 * recents). Invalidated (debounced) by `capture_tick` as new frames land. Idle for
 * an empty/invalid window so we never ask for the whole table.
 */
export function useFrames(range: TimeRange, limit: number, enabled = true) {
  const queryKey = queryKeys.frames(range, limit);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getFrames(range, limit),
    enabled: enabled && limit > 0 && range.end > range.start,
  });
  return useMaybeOverride(q, queryKey);
}

/**
 * The captures bracketing a frame (closest on each side), for a Moment's prev/next
 * + context strip. Unlike `useFrames`, the result is anchored to `at` rather than the
 * window's newest frames, so the immediate neighbours are always present. Idle until
 * the anchor time is known (the owning `useFrame` has resolved).
 */
export function useFrameContext(
  at: number,
  halfWindowMs: number,
  limitEach: number,
  enabled = true,
) {
  const queryKey = queryKeys.frameContext(at, halfWindowMs, limitEach);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getFrameContext(at, halfWindowMs, limitEach),
    enabled: enabled && halfWindowMs > 0 && limitEach > 0,
  });
  return useMaybeOverride(q, queryKey);
}

/** One frame's full detail; idle until a frame id is selected. */
export function useFrame(frameId: number | null) {
  const queryKey = queryKeys.frame(frameId ?? -1);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getFrame(frameId as number),
    enabled: frameId != null,
  });
  return useMaybeOverride(q, queryKey);
}

/** Sessions overlapping a Timeline range. TanStack Query deduplicates identical
 *  consumers and keeps the frame ribbon independent from this additive layer. */
export function useSessions(query: SessionQuery, enabled = true) {
  const queryKey = queryKeys.sessionList(query);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.listSessions(query),
    enabled:
      enabled &&
      (query.time_range == null ||
        query.time_range.end > query.time_range.start),
  });
  return useMaybeOverride(q, queryKey);
}

/** One session detail. Base and summary reads use distinct keys so a drill-in can
 *  start both together: metadata paints immediately while lazy intelligence runs. */
export function useSession(
  sessionId: number | null,
  includeSummary: boolean,
  enabled = true,
) {
  const queryKey = queryKeys.sessionDetail(sessionId ?? -1, includeSummary);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getSession(sessionId as number, includeSummary),
    enabled: enabled && sessionId != null,
  });
  return useMaybeOverride(q, queryKey);
}

/**
 * One frame's text spans for the text+layout reconstruction. Idle until a frame id is
 * selected and `enabled` (the Moment route turns it on only for retention-degraded
 * frames, so the spans aren't fetched for the common image case).
 */
export function useFrameSpans(frameId: number | null, enabled = true) {
  const queryKey = queryKeys.frameSpans(frameId ?? -1);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getFrameSpans(frameId as number),
    enabled: enabled && frameId != null,
  });
  return useMaybeOverride(q, queryKey);
}

/** Activity aggregates for the Insights screen. */
export function useInsights(
  range: TimeRange,
  bucketCount: number,
  enabled = true,
) {
  const queryKey = queryKeys.insights(range, bucketCount);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getInsights(range, bucketCount),
    enabled: enabled && bucketCount > 0 && range.end > range.start,
  });
  return useMaybeOverride(q, queryKey);
}

/** The Work screen's thread page (`03 §7h.4`): threads overlapping the window — plus, when
 *  `entity_id` is set, only the threads that entity was seen in — and the backfill progress that
 *  frames the list ("N of M sessions clustered"). */
export function useThreadList(query: ThreadListQuery) {
  const queryKey = queryKeys.threadList(query);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.listThreads(query),
  });
  return useMaybeOverride(q, queryKey);
}

/** One thread's full page (`03 §7h.4`/`§7h.5`/`§7j`): member sessions and entities, the merge
 *  survivor a folded thread points at, connected threads, open loops, backfill progress. */
export function useThreadPage(threadId: number) {
  const queryKey = queryKeys.threadPage(threadId);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getThreadPage(threadId),
  });
  return useMaybeOverride(q, queryKey);
}

/** One session's work-graph context (`03 §7h`): the entities it touched, the thread it was
 *  clustered into, that thread's connections and the backfill progress that decides "partial" —
 *  the same figures the Thread page reports. Idle until a session id is known, so a Moment with
 *  no owning session asks for nothing. */
export function useSessionGraph(
  sessionId: number | null,
): UseQueryResult<SessionGraph> {
  const queryKey = queryKeys.sessionGraph(sessionId ?? -1);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getSessionGraph(sessionId as number),
    enabled: sessionId !== null,
  });
  return useMaybeOverride(q, queryKey);
}

/** Entities filtered by kind and/or name substring (`03 §7c` P8 block). `enabled` lets a picker
 *  hold the read idle until it opens. */
export function useEntityList(query: EntityListQuery, enabled = true) {
  const queryKey = queryKeys.entityList(query);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.listEntities(query),
    enabled,
  });
  return useMaybeOverride(q, queryKey);
}

/** The entities named by `ids`, for scope chips and evidence labels. A deleted entity is simply
 *  absent from the result, so a shorter list is expected; an empty id list is not fetched. */
export function useEntities(ids: number[]) {
  const queryKey = queryKeys.entitiesByIds(ids);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.getEntities(ids),
    enabled: ids.length > 0,
  });
  return useMaybeOverride(q, queryKey);
}

/** Insight rows overlapping a window, grouped by kind in the view (`03 §7j`); the page's status
 *  carries `pending_jobs`, so a still-filling window renders as partial rather than complete. */
export function useInsightList(query: InsightListQuery) {
  const queryKey = queryKeys.insightList(query);
  const q = useQuery({
    queryKey,
    queryFn: () => cmd.listInsights(query),
  });
  return useMaybeOverride(q, queryKey);
}

/** The Ask conversation list, newest first (`03 §7i.4`). Kept in step by `useAskConversation`'s
 *  terminal invalidation and by the title/delete mutations. */
export function useAskThreads() {
  const q = useQuery<AskThreadSummary[]>({
    queryKey: queryKeys.askThreads,
    queryFn: () => cmd.listAskThreads(ASK_THREAD_LIST_LIMIT),
  });
  return useMaybeOverride(q, queryKeys.askThreads);
}

/** One conversation's persisted turns in `ordinal` order (`03 §7i.4`) — the record a reloaded
 *  thread renders from, streaming turn or not. Idle until a thread is selected. */
export function useAskTurns(threadId: number | null) {
  const queryKey = queryKeys.askTurns(threadId ?? -1);
  const q = useQuery<AskTurn[]>({
    queryKey,
    queryFn: () => cmd.listAskTurns(threadId as number),
    enabled: threadId != null,
  });
  return useMaybeOverride(q, queryKey);
}

/**
 * The last sustained context before the current detour (`where_was_i`, `03 §7b`, D9);
 * `null` = "nothing to resume yet". Depends on the live foreground context, so it is not
 * cached long — the overlay invalidates it on each summon and the Deck card refetches on
 * mount. `enabled` lets the overlay hold it idle until an empty-query strip is shown.
 */
export function useWhereWasI(enabled = true) {
  const q = useQuery<UiResumeContext | null>({
    queryKey: queryKeys.whereWasI,
    queryFn: cmd.whereWasI,
    enabled,
    staleTime: 0,
    gcTime: 0,
  });
  return useMaybeOverride(q, queryKeys.whereWasI);
}

/** All marks (unresolved first, newest-first within each group). Kept live by the
 *  `marks_changed` event (see useLiveEvents); the Intentions strip renders the
 *  unresolved head. */
export function useMarks(enabled = true) {
  const q = useQuery<Mark[]>({
    queryKey: queryKeys.marks,
    queryFn: cmd.listMarks,
    enabled,
  });
  return useMaybeOverride(q, queryKeys.marks);
}

/** API status for Settings (`03 §7c`). Mutations and the
 *  startup `api_status_changed` event refresh this shared cache. */
export function useApiStatus() {
  const q = useQuery<ApiStatus>({
    queryKey: queryKeys.apiStatus,
    queryFn: cmd.getApiStatus,
  });
  return useMaybeOverride(q, queryKeys.apiStatus);
}

/** Auto-update snapshot for the NavRail indicator + the Settings · App section
 *  (0.3.2 PR2, #69; `03 §11b`). Fetched once, then kept live by the
 *  `update_status_changed` event (see useLiveEvents); the manual-check mutation also
 *  writes the cache directly. `idle` renders zero UI presence. */
export function useUpdateStatus() {
  const q = useQuery<UpdateStatus>({
    queryKey: queryKeys.updateStatus,
    queryFn: cmd.getUpdateStatus,
  });
  return useMaybeOverride(q, queryKeys.updateStatus);
}

/**
 * The setup/acquisition snapshot (2026-10-20, `03 §7g`). Fetched on mount — but
 * the subscription lives in useLiveEvents, which registers `setup_changed` while
 * this query runs, so subscribe-before-fetch ordering holds at the app level.
 * The queryFn merges its fetch against the current cache through
 * `newerSetupStatus`, so a fetch racing a newer live event can never downgrade
 * the cache: the result already inside the cache (revised by the event) wins.
 */
export function useSetupStatus() {
  const qc = useQueryClient();
  const q = useQuery<SetupStatus>({
    queryKey: queryKeys.setupStatus,
    queryFn: async () => {
      const fetched = await cmd.getSetupStatus();
      const cached = qc.getQueryData<SetupStatus | null>(queryKeys.setupStatus);
      return newerSetupStatus(fetched, cached) ? fetched : (cached as SetupStatus);
    },
  });
  return useMaybeOverride(q, queryKeys.setupStatus);
}
