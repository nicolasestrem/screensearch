// ModelPanel (UI_REFERENCE §5) — truthful inference-engine status + manual load/unload.
// The label is driven by the *raw* sidecar lifecycle state (useSidecarStatus), so an
// idle-evicted model reads "Idle — unloaded" instead of the old, misleading "Ready". The
// answer model can be pre-loaded so the next Ask is instant, or unloaded to free VRAM now;
// the vision model loads automatically while the idle backfill drains the tagging backlog.
import { Button, Chip, Tooltip } from "../primitives";
import { IconCpu } from "../icons";
import {
  useModelDownload,
  useReadiness,
  useSidecarStatus,
} from "../../lib/ipc/queries";
import { useLoadModel, useUnloadModel } from "../../lib/ipc/mutations";
import { sidecarStateLabel, sidecarStateTone } from "../../lib/status";
import type { ModelLane } from "../../bindings/ModelLane";
import { toast } from "../../state/toastStore";

const LANE_LABEL: Record<ModelLane, string> = {
  vision: "Vision",
  answer: "Answer",
};

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function ModelPanel() {
  const sidecar = useSidecarStatus();
  const readiness = useReadiness();
  const download = useModelDownload();
  const loadModel = useLoadModel();
  const unloadModel = useUnloadModel();

  const status = sidecar.data;
  const state = status?.state ?? null;
  // Only a process actually resident in VRAM can be unloaded.
  const resident = state === "ready" || state === "starting";
  const busy = loadModel.isPending || unloadModel.isPending;
  const laneLabel = status?.lane ? LANE_LABEL[status.lane] : null;
  const detail = readiness.data?.sidecar.detail ?? "Inference engine";

  const dl = download.data?.phase === "downloading" ? download.data : null;
  const failed = download.data?.phase === "failed" ? download.data : null;
  const dlPct =
    dl && dl.total_bytes && dl.total_bytes > 0
      ? Math.min(100, Math.round((dl.downloaded_bytes / dl.total_bytes) * 100))
      : null;

  const load = () =>
    loadModel.mutate("answer", {
      onSuccess: () => toast.success("Answer model loaded"),
      onError: (e) => toast.error(String(e)),
    });
  const unload = () =>
    unloadModel.mutate(undefined, {
      onSuccess: () => toast.success("Model unloaded"),
      onError: (e) => toast.error(String(e)),
    });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <Chip tone={state ? sidecarStateTone(state) : "neutral"}>
          <IconCpu size={14} />
          {state ? sidecarStateLabel(state) : "Unknown"}
        </Chip>
        {laneLabel && (
          <span className="text-t-xs text-fg-secondary">
            {laneLabel} model
          </span>
        )}
        {status?.model && (
          <Tooltip label={status.model} side="bottom">
            <span className="max-w-xs truncate text-t-xs text-fg-tertiary">
              {status.model}
            </span>
          </Tooltip>
        )}
      </div>
      <span className="text-t-xs text-fg-tertiary">{detail}</span>

      {dl && (
        <div className="flex flex-col gap-1">
          <div className="flex items-center justify-between text-t-xs text-fg-secondary">
            <span>Downloading {dl.model ?? `${dl.lane} model`}…</span>
            <span className="font-mono text-t-data">
              {dlPct !== null
                ? `${dlPct}% · ${formatBytes(dl.downloaded_bytes)} / ${formatBytes(dl.total_bytes ?? 0)}`
                : formatBytes(dl.downloaded_bytes)}
            </span>
          </div>
          <div className="h-1.5 w-full overflow-hidden rounded-sm bg-surface-raised">
            <div
              className={
                dlPct !== null
                  ? "h-full rounded-sm bg-accent transition-[width] duration-fast ease-standard motion-reduce:transition-none"
                  : "h-full w-1/3 animate-pulse rounded-sm bg-accent motion-reduce:animate-none"
              }
              style={dlPct !== null ? { width: `${dlPct}%` } : undefined}
            />
          </div>
        </div>
      )}

      {failed && !dl && (
        <div
          role="alert"
          className="flex flex-col gap-0.5 rounded-md border border-status-danger-border bg-surface-raised px-3 py-2"
        >
          <span className="text-t-xs text-status-danger-fg">
            {failed.model ?? `${failed.lane} model`} download didn’t finish
          </span>
          <span className="text-t-xs text-fg-secondary">
            {failed.error ?? "The download was interrupted."} It resumes from
            where it stopped — load the model or run a tag to retry.
          </span>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="secondary"
          onClick={load}
          disabled={busy || dl !== null}
          leadingIcon={<IconCpu size={14} />}
        >
          {loadModel.isPending || dl ? "Loading…" : "Load answer model"}
        </Button>
        <Button variant="ghost" onClick={unload} disabled={busy || !resident}>
          {unloadModel.isPending ? "Unloading…" : "Unload now"}
        </Button>
      </div>
    </div>
  );
}
