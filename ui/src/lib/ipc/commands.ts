// Typed command wrappers — the only place the UI calls `invoke`. Every argument
// and return type comes from the ts-rs–generated `bindings/*` (UI_REFERENCE §6:
// the UI never hand-writes an API type). Argument records use camelCase keys;
// Tauri maps them onto the snake_case Rust parameters (`{ frameId }` → `frame_id`).
import { invoke } from "@tauri-apps/api/core";

import type { Readiness } from "../../bindings/Readiness";
import type { JobStats } from "../../bindings/JobStats";
import type { UiFrameDetail } from "../../bindings/UiFrameDetail";
import type { FrameMeta } from "../../bindings/FrameMeta";
import type { TextSpan } from "../../bindings/TextSpan";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { SearchHit } from "../../bindings/SearchHit";
import type { CaptureControl } from "../../bindings/CaptureControl";
import type { VisionTarget } from "../../bindings/VisionTarget";
import type { AskRequest } from "../../bindings/AskRequest";
import type { ReportRequest } from "../../bindings/ReportRequest";
import type { ReportResponse } from "../../bindings/ReportResponse";
import type { SetModelTier } from "../../bindings/SetModelTier";
import type { ModelLane } from "../../bindings/ModelLane";
import type { TimeRange } from "../../bindings/TimeRange";
import type { TimelineBucket } from "../../bindings/TimelineBucket";
import type { InsightsSummary } from "../../bindings/InsightsSummary";
import type { Settings } from "../../bindings/Settings";
import type { StorageStats } from "../../bindings/StorageStats";
import type { MonitorInfo } from "../../bindings/MonitorInfo";
import type { AppSuppression } from "../../bindings/AppSuppression";
import type { ThrottleStatus } from "../../bindings/ThrottleStatus";
import type { HotkeyStatus } from "../../bindings/HotkeyStatus";
import type { Mark } from "../../bindings/Mark";
import type { UiResumeContext } from "../../bindings/UiResumeContext";
import type { ApiStatus } from "../../bindings/ApiStatus";
import type { ExportRequest } from "../../bindings/ExportRequest";
import type { ExportResult } from "../../bindings/ExportResult";
import type { UpdateStatus } from "../../bindings/UpdateStatus";
import type { Session } from "../../bindings/Session";
import type { SessionDetail } from "../../bindings/SessionDetail";
import type { SessionQuery } from "../../bindings/SessionQuery";
import type { SessionRecapRequest } from "../../bindings/SessionRecapRequest";

/** Liveness probe for the IPC bridge. */
export const ping = (): Promise<string> => invoke<string>("ping");

/** Current subsystem readiness (`03 §7`). */
export const getReadiness = (): Promise<Readiness> =>
  invoke<Readiness>("get_readiness");

/** Aggregate job-queue counts (`03 §7`). */
export const getJobStats = (): Promise<JobStats> =>
  invoke<JobStats>("get_job_stats");

/** Storage footprint for the StatusRail. */
export const getStorageStats = (): Promise<StorageStats> =>
  invoke<StorageStats>("get_storage_stats");

/** Connected monitor metadata for Settings. */
export const getMonitors = (): Promise<MonitorInfo[]> =>
  invoke<MonitorInfo[]>("get_monitors");

/** Device ids reported by llama.cpp `--list-devices`. */
export const listSidecarDevices = (): Promise<string[]> =>
  invoke<string[]>("list_sidecar_devices");

/** Full per-frame detail; `null` if the id is unknown. */
export const getFrame = (frameId: number): Promise<UiFrameDetail | null> =>
  invoke<UiFrameDetail | null>("get_frame", { frameId });

/** Sessions overlapping the requested range, with optional kind/tool filters. */
export const listSessions = (query: SessionQuery): Promise<Session[]> =>
  invoke<Session[]>("list_sessions", { query });

/** A bounded session drill-in. `includeSummary` lazily generates and caches the
 *  title/summary on the in-app path; the base read remains inference-free. */
export const getSession = (
  sessionId: number,
  includeSummary: boolean,
): Promise<SessionDetail | null> =>
  invoke<SessionDetail | null>("get_session", { sessionId, includeSummary });

/** A frame's recognized text spans (normalized geometry + role), reading order. Backs
 *  the Moment text+layout reconstruction shown when a screenshot has been degraded. */
export const getFrameSpans = (frameId: number): Promise<TextSpan[]> =>
  invoke<TextSpan[]>("get_frame_spans", { frameId });

/** Hybrid search over OCR text + vectors, fused via RRF (`03 §7`). */
export const search = (query: SearchQuery): Promise<SearchHit[]> =>
  invoke<SearchHit[]>("search", { query });

/** Start/stop the always-on capture loop. */
export const captureControl = (control: CaptureControl): Promise<void> =>
  invoke<void>("capture_control", { control });

/** Enqueue deferred vision tagging; returns the number of jobs enqueued. */
export const enqueueVision = (target: VisionTarget): Promise<number> =>
  invoke<number>("enqueue_vision", { target });

/** Cancel all pending `vision_tag` jobs (the "Stop vision tagging" quick action, 0.3.2
 *  PR3); returns how many were cancelled. A running job finishes. */
export const cancelVision = (): Promise<number> =>
  invoke<number>("cancel_vision");

/** Ask a grounded question; the answer streams back via `answer_delta` events. */
export const ask = (request: AskRequest): Promise<void> =>
  invoke<void>("ask", { request });

/** Cancel a streaming grounded answer by request id. */
export const cancelAsk = (requestId: string): Promise<void> =>
  invoke<void>("cancel_ask", { requestId });

/** Generate a recall report over a time range (`03 §8b`); progress streams via
 *  `report_progress` events, the report returns when complete. */
export const generateReport = (
  request: ReportRequest,
): Promise<ReportResponse> =>
  invoke<ReportResponse>("generate_report", { request });

/** Cancel an in-flight report by request id (stops at the next pass boundary). */
export const cancelReport = (requestId: string): Promise<void> =>
  invoke<void>("cancel_report", { requestId });

/** Generate the existing coverage-first report over one stable session id. */
export const sessionRecap = (
  request: SessionRecapRequest,
): Promise<ReportResponse> =>
  invoke<ReportResponse>("session_recap", { request });

/** Save a report's markdown to a date-stamped `.md` file in Downloads and return the
 *  written path (0.3.1 D2, #65). `stem` is the local-time filename stem the UI builds;
 *  same-minute collisions get `-2`/`-3`. */
export const saveReportMarkdown = (
  stem: string,
  markdown: string,
): Promise<string> =>
  invoke<string>("save_report_markdown", { stem, markdown });

/** Change the active model tier for a lane (hot-applies on the next request). */
export const setModelTier = (request: SetModelTier): Promise<void> =>
  invoke<void>("set_model_tier", { request });

/** Eagerly load a lane's model into the sidecar so the next request is instant. */
export const loadModel = (lane: ModelLane): Promise<void> =>
  invoke<void>("load_model", { lane });

/** Unload the resident sidecar model now, freeing VRAM/RAM. */
export const unloadModel = (): Promise<void> => invoke<void>("unload_model");

/** Frame-count density buckets over `[start, end)` for the Scanline Timeline. */
export const getTimeline = (
  range: TimeRange,
  bucketCount: number,
): Promise<TimelineBucket[]> =>
  invoke<TimelineBucket[]>("get_timeline", { range, bucketCount });

/** Lightweight frame list over `[start, end)`, newest-first, capped at `limit`
 *  (timeline thumbnails, deck recents, moment neighbours). */
export const getFrames = (
  range: TimeRange,
  limit: number,
): Promise<FrameMeta[]> => invoke<FrameMeta[]>("get_frames", { range, limit });

/** The frame whose capture time is nearest `at` (unix ms), or `null` if the DB has
 *  no frames — resolves a timeline scan-head position to a concrete frame. */
export const getNearestFrame = (
  at: number,
  range?: TimeRange,
): Promise<FrameMeta | null> =>
  invoke<FrameMeta | null>("get_nearest_frame", { at, range: range ?? null });

/** The captures bracketing `at` (unix ms): up to `limitEach` closest frames on each
 *  side within `±halfWindowMs`, ascending by time, excluding the anchor. Backs a
 *  Moment's prev/next + context strip (the adjacent frames, not the window's newest). */
export const getFrameContext = (
  at: number,
  halfWindowMs: number,
  limitEach: number,
): Promise<FrameMeta[]> =>
  invoke<FrameMeta[]>("get_frame_context", { at, halfWindowMs, limitEach });

/** Truthful activity aggregates over `[start, end)` for the Insights screen. */
export const getInsights = (
  range: TimeRange,
  bucketCount: number,
): Promise<InsightsSummary> =>
  invoke<InsightsSummary>("get_insights", { range, bucketCount });

/** Read the persisted user settings (missing keys fall back to defaults). */
export const getSettings = (): Promise<Settings> =>
  invoke<Settings>("get_settings");

/** Persist user settings; tiers hot-apply, the rest on restart / next capture. */
export const setSettings = (settings: Settings): Promise<void> =>
  invoke<void>("set_settings", { settings });

/** Per-app text-filter suppression metric — the guardrail against silent
 *  over-suppression (PR3, `03 §3b`). */
export const getTextFilterStats = (): Promise<AppSuppression[]> =>
  invoke<AppSuppression[]>("get_text_filter_stats");

/** Current enrichment-throttle status: live CPU/GPU pressure, level, and whether the
 *  GPU is monitored (`03 §7`, former PR5). Live updates arrive via `throttle_changed`. */
export const getThrottleStatus = (): Promise<ThrottleStatus> =>
  invoke<ThrottleStatus>("get_throttle_status");

/** Shell-managed global hotkey registration status for Settings warnings. */
export const getHotkeyStatus = (): Promise<HotkeyStatus[]> =>
  invoke<HotkeyStatus[]>("get_hotkey_status");

/** Hide the Flow overlay without destroying the pre-created webview. */
export const hideOverlay = (): Promise<void> => invoke<void>("hide_overlay");

/** Acknowledge the overlay's first focused paint for hotkey-to-input timing logs. */
export const overlayShownAck = (): Promise<void> =>
  invoke<void>("overlay_shown_ack");

/** Open a captured frame in the main window, then dismiss the overlay. */
export const openMoment = (frameId: number): Promise<void> =>
  invoke<void>("open_moment", { frameId });

/** The last sustained context before the current detour, or `null` for "nothing to
 *  resume yet" (`where_was_i`, `03 §7b`). Drives the overlay empty state + Deck card. */
export const whereWasI = (): Promise<UiResumeContext | null> =>
  invoke<UiResumeContext | null>("where_was_i");

/** Create a mark: exactly one of an existing `frameId`, or `captureNow` to capture the
 *  current screen past the diff gate first (`add_mark`, `03 §7b`, D8). Returns the id. */
export const addMark = (args: {
  frameId?: number | null;
  captureNow?: boolean;
  note?: string | null;
}): Promise<number> =>
  invoke<number>("add_mark", {
    frameId: args.frameId ?? null,
    captureNow: args.captureNow ?? false,
    note: args.note ?? null,
  });

/** All marks, unresolved first then newest-first within each group (`list_marks`). */
export const listMarks = (): Promise<Mark[]> => invoke<Mark[]>("list_marks");

/** Resolve a mark — done or dismiss, the same operation (`resolve_mark`, `03 §7b`). */
export const resolveMark = (markId: number): Promise<void> =>
  invoke<void>("resolve_mark", { markId });

/** Attach the optional one-line note to an existing mark (`set_mark_note`, `03 §7`). */
export const setMarkNote = (markId: number, note: string): Promise<void> =>
  invoke<void>("set_mark_note", { markId, note });

/** Make the overlay focusable + focus it when the user clicks the mark toast's note
 *  field, so keystrokes land in the note (the toast is otherwise non-focusable, D1). */
export const focusOverlayForNote = (): Promise<void> =>
  invoke<void>("focus_overlay_for_note");

/** Dismiss the mark toast, restoring the overlay to focusable for the next summon. */
export const dismissMarkToast = (): Promise<void> =>
  invoke<void>("dismiss_mark_toast");

/** Enable/disable the local HTTP API (and set the port). Returns the new status; a
 *  bind failure is reported as `enabled && !running && last_error` (loud + guided,
 *  `03 §7c`), not an error. */
export const setApiConfig = (args: {
  enabled: boolean;
  port?: number | null;
}): Promise<ApiStatus> =>
  invoke<ApiStatus>("set_api_config", {
    enabled: args.enabled,
    port: args.port ?? null,
  });

/** The current local-API status (enabled, running, port, token, last error). */
export const getApiStatus = (): Promise<ApiStatus> =>
  invoke<ApiStatus>("get_api_status");

/** Explicit visible-main-window acknowledgement; persistence failure preserves pending state. */
export const acknowledgeApiNetworkNotice = (): Promise<ApiStatus> =>
  invoke<ApiStatus>("acknowledge_api_network_notice");

/** Regenerate the bearer token (no server restart — the middleware reads it live). */
export const regenerateApiToken = (): Promise<ApiStatus> =>
  invoke<ApiStatus>("regenerate_api_token");

/** Export frames + content text + marks to a JSON file in Downloads (`03 §7c`, D12).
 *  Works with the API disabled; returns the written path + counts. */
export const exportData = (request: ExportRequest): Promise<ExportResult> =>
  invoke<ExportResult>("export_data", { request });

/** Current auto-update snapshot (0.3.2 PR2, #69; `03 §11b`). `idle` = zero UI presence. */
export const getUpdateStatus = (): Promise<UpdateStatus> =>
  invoke<UpdateStatus>("get_update_status");

/** Manually check for updates; returns the post-check status. A found update downloads
 *  in the background and finishes via the `update_status_changed` event. */
export const checkForUpdates = (): Promise<UpdateStatus> =>
  invoke<UpdateStatus>("check_for_updates");

/** Install the downloaded update and restart — the only install trigger (D1). Errs when
 *  nothing is `ready`. On success the process hands off to the passive installer. */
export const restartToApplyUpdate = (): Promise<void> =>
  invoke<void>("restart_to_apply_update");
