// Live enrichment-throttle readout (`UI_REFERENCE §4` Settings · Performance
// throttle, `03 §5`/`§8`): CPU/GPU and host-memory pressure, the throttle level,
// and truthful unavailable readings. `useThrottleStatus` reads `get_throttle_status`
// and stays live through `throttle_changed`, so this needs no timer of its own.
//
// All four view states are handled here, inside the throttle row: loading reserves
// the readout's height, an unavailable probe says so and offers Retry, the off case
// collapses to one line, and the populated case is the measured readout.
//
// The on/off line follows the SETTING and the measured line follows the probe, both
// read here: `useSettings` owns the master switch (optimistic, refetched after a write)
// and `useThrottleStatus` owns the pressure snapshot. A settings save invalidates the
// throttle status too (`SETTINGS_SAVE_INVALIDATIONS`), so the probe no longer keeps its
// pre-save value; the setting is still what tells an off throttle apart from one whose
// governor has not produced its first sample yet. Both report `enabled: false`.
import { Button, Skeleton } from "../../components/primitives";
import { useSettings, useThrottleStatus } from "../../lib/ipc/queries";

/** `ThrottleStatus.level`: 0 Normal · 1 High · 2 Sustained. */
const LEVEL_LABEL = ["Normal", "High", "Sustained"] as const;

export function ThrottleReadout() {
  const settings = useSettings();
  const status = useThrottleStatus();

  // The Advanced row already gates its own loading/error panel on this query; keep a
  // readout-sized placeholder if this component is ever mounted ahead of it.
  if (settings.data === undefined) return <Skeleton w="half" h="control" />;

  if (!settings.data.throttle_enabled) {
    return (
      <p className="min-w-0 text-t-sm text-fg-secondary">
        Throttle is off — enrichment always runs at full speed.
      </p>
    );
  }

  if (status.isPending) return <Skeleton w="half" h="control" />;

  if (status.isError || !status.data) {
    return (
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="text-t-sm text-status-warn-fg">
          Throttle is on, but pressure can&rsquo;t be read yet — nothing is being
          throttled until it can.
        </span>
        <Button size="sm" variant="ghost" onClick={() => status.refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  const data = status.data;
  if (!data.enabled) {
    return (
      <p className="min-w-0 text-t-sm text-fg-secondary">
        Turning on — waiting for the first pressure sample.
      </p>
    );
  }

  const cpu = data.sample ? `${Math.round(data.sample.cpu_pct)}%` : "—";
  const gpu = !data.gpu_monitored
    ? "not monitored"
    : data.sample?.gpu_pct != null
      ? `${Math.round(data.sample.gpu_pct)}%`
      : "—";
  // The available-memory count and usage percentages come from the same sample.
  const mem = !data.mem_monitored
    ? "not monitored"
    : data.sample
      ? `${Math.round(data.sample.mem_used_pct)}% (${formatGiB(data.sample.avail_phys_mb)} available)`
      : "—";
  const commit =
    data.mem_monitored && data.sample
      ? `${Math.round(data.sample.commit_pct)}%`
      : "—";
  const level = LEVEL_LABEL[data.level] ?? LEVEL_LABEL[0];

  return (
    <div className="flex min-w-0 flex-col gap-1">
      <p className="min-w-0 font-mono text-t-data tabular-nums text-fg-primary">
        CPU {cpu} · GPU {gpu} · RAM {mem} · Commit {commit} · Level: {level}
      </p>
      {data.gpu_monitored ? null : (
        <p className="min-w-0 text-t-xs text-fg-tertiary">
          GPU counters are unavailable. The remaining monitored resources still
          control throttling.
        </p>
      )}
      {data.mem_monitored ? null : (
        <p className="min-w-0 text-t-xs text-fg-tertiary">
          Host memory is unavailable. The memory thresholds are inactive.
        </p>
      )}
      {data.level > 0 ? (
        <p className="min-w-0 text-t-xs text-fg-tertiary">
          Vision tagging is paused.
          {data.level >= 2
            ? " Text-embedding concurrency is reduced. The app tries to free the AI model's memory, unless you explicitly kept it loaded."
            : ""}
          {" "}Capture, OCR and storage keep running.
        </p>
      ) : null}
    </div>
  );
}

/** Converts available MiB to GiB without implying the pages are unused. */
function formatGiB(mib: number): string {
  return `${(mib / 1024).toFixed(1)} GiB`;
}
