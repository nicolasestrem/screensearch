// Typed event subscriptions. The payload map is the authoritative client view of
// what the backend emits (forward_events in src-tauri/src/lib.rs). Note: the
// `job_progress` event carries a bare `JobStats` (the kernel emits the inner value
// of KernelEvent::JobProgress), NOT the `JobProgress` wrapper binding.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CaptureTick } from "../../bindings/CaptureTick";
import type { Readiness } from "../../bindings/Readiness";
import type { JobStats } from "../../bindings/JobStats";
import type { JobCompleted } from "../../bindings/JobCompleted";
import type { SidecarStatus } from "../../bindings/SidecarStatus";
import type { ModelDownloadStatus } from "../../bindings/ModelDownloadStatus";
import type { AnswerEvent } from "../../bindings/AnswerEvent";
import type { ReportProgress } from "../../bindings/ReportProgress";
import type { Toast } from "../../bindings/Toast";
import type { ThrottleStatus } from "../../bindings/ThrottleStatus";
import type { OpenMoment } from "../../bindings/OpenMoment";
import type { MarkToast } from "../../bindings/MarkToast";
import type { UpdateStatus } from "../../bindings/UpdateStatus";

/** Map of backend event name → payload type. */
export interface AppEvents {
  capture_tick: CaptureTick;
  readiness_changed: Readiness;
  job_progress: JobStats;
  job_completed: JobCompleted;
  sidecar_status: SidecarStatus;
  model_download: ModelDownloadStatus;
  answer_delta: AnswerEvent;
  report_progress: ReportProgress;
  toast: Toast;
  throttle_changed: ThrottleStatus;
  overlay_shown: null;
  overlay_hidden: null;
  open_moment: OpenMoment;
  // 0.3.0 flow recall (PR6): the overlay mark-this-moment confirmation toast, and a
  // main-window signal that the marks set changed (fired from the mark hotkey + the
  // marks commands) so the Deck's Intentions strip refreshes across windows.
  mark_toast: MarkToast;
  marks_changed: null;
  // 0.3.2 auto-update (PR2): every updater state transition (checking / available /
  // downloading / ready / error / back to idle). The UI mirrors it into the
  // `updateStatus` query cache — no toast, the updater is quiet (`03 §11b`, D1).
  update_status_changed: UpdateStatus;
  // Pull-based cache invalidation after the independent sessions scheduler commits.
  // Null payload and no toast: mounted views simply refetch their typed queries.
  sessions_changed: null;
  // API startup completed; invalidate status without broadcasting its bearer token.
  api_status_changed: null;
}

/**
 * Subscribe to a backend event with a typed payload. Returns the Tauri
 * `UnlistenFn` (call it to detach). Outside the Tauri shell `listen` rejects;
 * callers should treat a failed subscription as "no live events" (dev mode).
 */
export function listenTo<K extends keyof AppEvents>(
  event: K,
  handler: (payload: AppEvents[K]) => void,
): Promise<UnlistenFn> {
  return listen<AppEvents[K]>(event, (e) => handler(e.payload));
}
