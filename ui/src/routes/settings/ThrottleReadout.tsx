// Live enrichment-throttle readout (`UI_REFERENCE §4` Settings · Performance
// throttle, `03 §5`/`§8`): the current CPU/GPU pressure, the throttle level
// (Normal / High / Sustained) and a truthful "GPU not monitored" line when Windows
// exposes no GPU counters. `useThrottleStatus` reads `get_throttle_status` and is
// kept live by the `throttle_changed` event, so this needs no timer of its own.
//
// All four view states are handled here, inside the throttle row: loading reserves
// the readout's height, an unavailable probe says so and offers Retry, the off case
// collapses to one line, and the populated case is the measured readout.
//
// The off line follows the SAVED setting, not the probe: `get_throttle_status` is
// only re-read on mount and on `throttle_changed`, and a settings save does not
// invalidate it (`lib/ipc` is not this phase's file), so trusting the snapshot for
// "on/off" would contradict the switch the user just flipped. The measured numbers
// still come from the probe; when the probe has not caught up yet the row says so
// instead of printing a stale "off".
import { Button, Skeleton } from "../../components/primitives";
import { useThrottleStatus } from "../../lib/ipc/queries";

/** `ThrottleStatus.level`: 0 Normal · 1 High · 2 Sustained. */
const LEVEL_LABEL = ["Normal", "High", "Sustained"] as const;

export function ThrottleReadout({ enabled }: { enabled: boolean }) {
  const status = useThrottleStatus();

  if (!enabled) {
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
  const level = LEVEL_LABEL[data.level] ?? LEVEL_LABEL[0];

  return (
    <div className="flex min-w-0 flex-col gap-1">
      <p className="min-w-0 font-mono text-t-data tabular-nums text-fg-primary">
        CPU {cpu} · GPU {gpu} · Level: {level}
      </p>
      {data.gpu_monitored ? null : (
        <p className="min-w-0 text-t-xs text-fg-tertiary">
          This machine exposes no GPU performance counters, so throttling reads CPU
          pressure only.
        </p>
      )}
      {data.level > 0 ? (
        <p className="min-w-0 text-t-xs text-fg-tertiary">
          Throttling is active: vision tagging and image indexing are paused
          {data.level >= 2 ? " and text-embedding concurrency is reduced" : ""}.
          Capture, OCR and storage keep running.
        </p>
      ) : null}
    </div>
  );
}
