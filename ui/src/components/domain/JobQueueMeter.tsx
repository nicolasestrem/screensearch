// JobQueueMeter — the deferred-enrichment queue at a glance (UI_REFERENCE §5).
// Shows the live counts (pending · running · done · failed/dead) as labelled data
// chips plus a thin proportion bar. Enrichment is deferred work (`03 §5`); this is
// how the user sees the backlog drain. Counts come from `useJobStats`, kept live by
// the `job_progress` event (useLiveEvents). Honest-empty when nothing is queued.
import type { JobStats } from "../../bindings/JobStats";
import { Chip } from "../primitives";
import { cn } from "../../lib/cn";

export interface JobQueueMeterProps {
  stats: JobStats;
}

export function JobQueueMeter({ stats }: JobQueueMeterProps) {
  const failed = stats.failed + stats.dead;
  const total = stats.pending + stats.running + stats.done + failed;

  if (total === 0) {
    return (
      <p className="text-t-body text-fg-secondary">
        No enrichment queued.
      </p>
    );
  }

  // Proportions of the bar; widths are inline (data-driven percentages aren't a
  // styling token). The track and segment colors are tokens.
  const pct = (n: number) => `${(n / total) * 100}%`;
  const segments: Array<{ key: string; n: number; className: string }> = [
    { key: "done", n: stats.done, className: "bg-status-ok-fg" },
    { key: "running", n: stats.running, className: "bg-accent" },
    { key: "pending", n: stats.pending, className: "bg-fg-tertiary" },
    { key: "failed", n: failed, className: "bg-status-danger-fg" },
  ];

  return (
    <div className="flex flex-col gap-3">
      <div
        className="flex h-2 w-full overflow-hidden rounded-sm bg-surface-inset"
        role="img"
        aria-label={`Queue: ${stats.pending} pending, ${stats.running} running, ${stats.done} done, ${failed} failed`}
      >
        {segments.map((s) =>
          s.n > 0 ? (
            <span
              key={s.key}
              className={cn("h-full", s.className)}
              style={{ width: pct(s.n) }}
            />
          ) : null,
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone={stats.running > 0 ? "accent" : "neutral"}>
          running {stats.running}
        </Chip>
        <Chip tone="neutral">pending {stats.pending}</Chip>
        <Chip tone="ok">done {stats.done}</Chip>
        {failed > 0 && <Chip tone="danger">failed {failed}</Chip>}
      </div>
    </div>
  );
}
