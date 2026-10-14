// Write-side hooks. Each wraps a command in a TanStack mutation and reconciles
// the affected query cache on success. Callers fire client-side toasts from
// onSuccess/onError (UI_REFERENCE §6); the backend additionally emits `toast` events
// for hotkey/local-API bind warnings, forwarded by useLiveEvents.
import { useMutation, useQueryClient } from "@tanstack/react-query";

import * as cmd from "./commands";
import { queryKeys } from "./queryKeys";
import type { CaptureControl } from "../../bindings/CaptureControl";
import type { VisionTarget } from "../../bindings/VisionTarget";
import type { Settings } from "../../bindings/Settings";
import type { SetModelTier } from "../../bindings/SetModelTier";
import type { ModelLane } from "../../bindings/ModelLane";
import type { ApiStatus } from "../../bindings/ApiStatus";
import type { ExportRequest } from "../../bindings/ExportRequest";
import type { ExportResult } from "../../bindings/ExportResult";
import type { UpdateStatus } from "../../bindings/UpdateStatus";

/** Start/stop capture; readiness refetches so the StatusRail flips immediately. */
export function useCaptureControl() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (control: CaptureControl) => cmd.captureControl(control),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.readiness });
    },
  });
}

/** Enqueue on-demand vision tagging for a frame or range. */
export function useEnqueueVision() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (target: VisionTarget) => cmd.enqueueVision(target),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.jobStats });
    },
  });
}

/** Cancel pending vision tagging — the "Stop vision tagging" quick action (0.3.2 PR3).
 *  The backend emits `job_progress`, but invalidate so the label flips promptly too. */
export function useCancelVision() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => cmd.cancelVision(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.jobStats });
    },
  });
}

/**
 * Persist settings. Optimistic: the cache is set to the submitted value up front
 * and reconciled with a refetch afterwards (UI_REFERENCE §4 Settings row).
 */
export function useSetSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: Settings) => cmd.setSettings(settings),
    onMutate: async (settings: Settings) => {
      await qc.cancelQueries({ queryKey: queryKeys.settings });
      const previous = qc.getQueryData<Settings>(queryKeys.settings);
      qc.setQueryData(queryKeys.settings, settings);
      return { previous };
    },
    onError: (_err, _settings, ctx) => {
      if (ctx?.previous) qc.setQueryData(queryKeys.settings, ctx.previous);
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.settings });
      qc.invalidateQueries({ queryKey: queryKeys.hotkeyStatus });
    },
  });
}

/** Change a single lane's model tier (used by the tier picker for immediacy). */
export function useSetModelTier() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: SetModelTier) => cmd.setModelTier(request),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.settings });
    },
  });
}

/** Eagerly load a lane's model. Status events drive the panel, but invalidate readiness
 *  + sidecar status so the label flips promptly even if an event is missed. */
export function useLoadModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (lane: ModelLane) => cmd.loadModel(lane),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.readiness });
      qc.invalidateQueries({ queryKey: queryKeys.sidecarStatus });
    },
  });
}

/** Unload the resident model now (frees VRAM). Same cache reconciliation as load. */
export function useUnloadModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => cmd.unloadModel(),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.readiness });
      qc.invalidateQueries({ queryKey: queryKeys.sidecarStatus });
    },
  });
}

/** Resolve a mark — done or dismiss, the same operation (`03 §7b`). Refreshes the
 *  Intentions strip. The backend also broadcasts `marks_changed`, but invalidating here
 *  keeps the strip snappy even before the event round-trips. */
export function useResolveMark() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (markId: number) => cmd.resolveMark(markId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.marks });
    },
  });
}

/**
 * Enable/disable the local HTTP API (and set the port). Returns the new `ApiStatus`;
 * the panel reads `enabled && !running && last_error` to show the loud port-in-use
 * state (`03 §7c`). Invalidates `apiStatus` so the panel reflects the new state.
 */
export function useSetApiConfig() {
  const qc = useQueryClient();
  return useMutation<ApiStatus, unknown, { enabled: boolean; port?: number | null }>(
    {
      mutationFn: (args) => cmd.setApiConfig(args),
      onSettled: () => {
        qc.invalidateQueries({ queryKey: queryKeys.apiStatus });
      },
    },
  );
}

/** Regenerate the bearer token (no server restart). Reconciles the panel via the
 *  returned status + an invalidation. */
export function useRegenerateApiToken() {
  const qc = useQueryClient();
  return useMutation<ApiStatus, unknown, void>({
    mutationFn: () => cmd.regenerateApiToken(),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.apiStatus });
    },
  });
}

/** Export screen history to a JSON file in Downloads (`03 §7c`, D12). Works with the
 *  API disabled; the caller toasts the returned path. No cache to reconcile. */
export function useExportData() {
  return useMutation<ExportResult, unknown, ExportRequest>({
    mutationFn: (request) => cmd.exportData(request),
  });
}

/**
 * Manual "Check for updates" (0.3.2 PR2, #69; `03 §11b`). Writes the returned snapshot
 * into the `updateStatus` cache so the App panel + NavRail indicator update immediately;
 * a background download that follows finishes via `update_status_changed`. No toast — the
 * updater is quiet (D1); errors render inline in the App panel.
 */
export function useCheckForUpdates() {
  const qc = useQueryClient();
  return useMutation<UpdateStatus, unknown, void>({
    mutationFn: () => cmd.checkForUpdates(),
    onSuccess: (status) => {
      qc.setQueryData(queryKeys.updateStatus, status);
    },
  });
}

/** Install the downloaded update and restart — the only install trigger (D1). On success
 *  the process hands off to the passive installer, so there is no cache to reconcile. */
export function useRestartToApplyUpdate() {
  return useMutation<void, unknown, void>({
    mutationFn: () => cmd.restartToApplyUpdate(),
  });
}
