// StatusRail (top) — live telemetry: capture · DB · queue · sidecar/model, plus an
// aggregate readiness chip (UI_REFERENCE §3). All values come from TanStack Query
// caches that the live event stream keeps fresh (useLiveEvents). Outside the Tauri
// shell the readiness query errors and the rail shows an honest "Kernel offline".
import { Chip, Skeleton, Tooltip } from "../primitives";
import { SetupIndicator } from "../domain";
import {
  IconCapture,
  IconCpu,
  IconDatabase,
  IconQueue,
} from "../icons";
import {
  useJobStats,
  useReadiness,
  useSidecarStatus,
  useStorageStats,
  useThrottleStatus,
} from "../../lib/ipc/queries";
import {
  sidecarStateLabel,
  sidecarStateTone,
  statusLabel,
  statusTone,
  worstStatus,
} from "../../lib/status";

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

export function StatusRail() {
  const readiness = useReadiness();
  const jobStats = useJobStats();
  const storage = useStorageStats();
  const sidecar = useSidecarStatus();
  const throttle = useThrottleStatus();

  // Only present under real pressure (level >= 1) — genuinely subtle, hidden when idle or
  // when the throttle is off. Tells the user why enrichment slowed without hunting Settings.
  const throttling =
    throttle.data?.enabled && throttle.data.level >= 1 ? throttle.data : null;

  return (
    <header className="flex items-center justify-between gap-4 h-12 px-4 bg-surface border-b border-line overflow-x-clip">
      <div className="flex items-baseline gap-3">
        <span className="font-display uppercase tracking-eyebrow text-subtitle text-ink">
          ScreenSearch
        </span>
        {/* Sub-brand eyebrow is the first thing to yield at narrow widths so the live
            chips never overflow the fixed-height rail (D9). */}
        <span className="hidden lg:inline eyebrow text-ink-faint">Command Deck</span>
      </div>

      <div className="flex items-center gap-2 min-w-0">
        {/* Setup chip (03 §7g): the one rail indicator for every setup operation —
            engine, search model and the vision/answer model downloads (which the
            kernel mirrors from `model_download`), so no second download chip.
            Present only while something is active/failed; zero presence otherwise. */}
        <SetupIndicator />
        {throttling && (
          <Tooltip
            label={`Throttling enrichment (${throttling.level >= 2 ? "Sustained" : "High"})${
              throttling.sample
                ? ` · CPU ${Math.round(throttling.sample.cpu_pct)}% · ${
                    throttling.gpu_monitored &&
                    throttling.sample.gpu_pct != null
                      ? `GPU ${Math.round(throttling.sample.gpu_pct)}%`
                      : "GPU not monitored"
                  }`
                : ""
            }`}
            side="bottom"
          >
            <Chip tone="warn" dot>
              <IconCpu size={14} />
              Throttling
            </Chip>
          </Tooltip>
        )}

        {readiness.isLoading && (
          // Five skeletons at the same floor widths as the five populated chips below,
          // so loading -> populated swaps in place with zero layout shift (D9).
          <>
            <Skeleton className="w-24 h-6" />
            <Skeleton className="w-20 h-6" />
            <Skeleton className="w-16 h-6" />
            <Skeleton className="w-28 h-6" />
            <Skeleton className="w-20 h-6" />
          </>
        )}

        {readiness.isError && (
          <Tooltip label={String(readiness.error)} side="bottom">
            <Chip tone="danger" dot>
              Kernel offline
            </Chip>
          </Tooltip>
        )}

        {readiness.data && (
          <>
            <Tooltip
              label={readiness.data.capture.detail ?? "Capture loop"}
              side="bottom"
            >
              <Chip
                tone={statusTone(readiness.data.capture.status)}
                className="min-w-24 justify-center"
              >
                <IconCapture size={14} />
                {statusLabel(readiness.data.capture.status)}
              </Chip>
            </Tooltip>

            <Tooltip
              label={
                storage.data
                  ? `${readiness.data.db.detail ?? "Data store"} · DB ${formatBytes(storage.data.db_bytes)} · frames ${formatBytes(storage.data.frame_bytes)}`
                  : (readiness.data.db.detail ?? "Data store")
              }
              side="bottom"
            >
              <Chip
                tone={statusTone(readiness.data.db.status)}
                className="min-w-20 justify-center"
              >
                <IconDatabase size={14} />
                {storage.data ? formatBytes(storage.data.total_bytes) : "DB"}
              </Chip>
            </Tooltip>

            <Tooltip
              label={
                jobStats.data
                  ? `pending ${jobStats.data.pending} · running ${jobStats.data.running} · done ${jobStats.data.done} · failed ${jobStats.data.failed}`
                  : "Enrichment queue"
              }
              side="bottom"
            >
              <Chip
                tone={
                  jobStats.data && jobStats.data.failed > 0 ? "warn" : "neutral"
                }
                className="min-w-16 justify-center"
              >
                <IconQueue size={14} />
                {jobStats.data
                  ? jobStats.data.pending + jobStats.data.running
                  : "—"}
              </Chip>
            </Tooltip>

            <Tooltip
              label={
                sidecar.data?.model
                  ? `${readiness.data.sidecar.detail ?? "Inference sidecar"} · ${sidecar.data.model}`
                  : (readiness.data.sidecar.detail ?? "Inference sidecar")
              }
              side="bottom"
            >
              {/* Label from the raw lifecycle state when known (truthful: an idle-evicted
                  model reads "Idle — unloaded", never "Ready"); fall back to the aggregate
                  ComponentStatus until the first sidecar_status event arrives. */}
              <Chip
                tone={
                  sidecar.data
                    ? sidecarStateTone(sidecar.data.state)
                    : statusTone(readiness.data.sidecar.status)
                }
                className="min-w-28 justify-center"
              >
                <IconCpu size={14} />
                {sidecar.data
                  ? sidecarStateLabel(sidecar.data.state)
                  : statusLabel(readiness.data.sidecar.status)}
              </Chip>
            </Tooltip>

            <Chip
              tone={statusTone(worstStatus(readiness.data))}
              dot
              className="min-w-20 justify-center"
            >
              {statusLabel(worstStatus(readiness.data))}
            </Chip>
          </>
        )}
      </div>
    </header>
  );
}
