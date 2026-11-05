// The per-app text-filter suppression readout, carried over from the v1 Settings
// screen into Intelligence. It is the guardrail that makes silent over-suppression
// observable (`03 §3b`): the share of recognised spans dropped as chrome, system or
// background text, per foreground app, over frames classified by the live filter
// version. A very high rate can mean over-suppression — the preserved raw text and
// "include app chrome" always recover it, so it is a readout, never a limit.
//
// Empty is honest: before capture has classified any frame there is nothing to
// report, and the readout says so instead of printing 0%. Loading and error are
// distinct states of their own.
import { Skeleton } from "../../components/primitives";
import { useTextFilterStats } from "../../lib/ipc/queries";

/** The eight apps that shed the most text, highest rate first. */
const SHOWN_APPS = 8;

function percent(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

export function SuppressionReadout() {
  const stats = useTextFilterStats();

  if (stats.isPending) return <Skeleton w="half" h="control" />;

  if (stats.isError) {
    return (
      <p role="alert" className="text-t-xs text-status-danger-fg">
        {`Couldn’t read the suppression counts: ${
          stats.error instanceof Error && stats.error.message.trim().length > 0
            ? stats.error.message.trim()
            : "the backend didn’t answer"
        }. Capture keeps running.`}
      </p>
    );
  }

  const rows = stats.data ?? [];
  if (rows.length === 0) {
    return (
      <p className="text-t-xs text-fg-tertiary">
        No filtered captures yet — start capture to see how much chrome each app
        sheds.
      </p>
    );
  }

  const suppressed = rows.reduce((sum, row) => sum + row.suppressed_spans, 0);
  const total = rows.reduce((sum, row) => sum + row.total_spans, 0);
  const ranked = [...rows].sort((a, b) => b.rate - a.rate).slice(0, SHOWN_APPS);

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <p className="min-w-0 text-t-xs text-fg-secondary">
        <span className="font-mono text-t-data tabular-nums">
          {suppressed.toLocaleString()}
        </span>
        {` of `}
        <span className="font-mono text-t-data tabular-nums">
          {total.toLocaleString()}
        </span>
        {` recognised spans dropped as chrome, system or background text across `}
        <span className="font-mono text-t-data tabular-nums">{rows.length}</span>
        {rows.length === 1 ? " app" : " apps"}
        {total > 0 ? ` (${percent(suppressed / total)} overall)` : ""}.
      </p>
      <ul className="flex min-w-0 flex-col gap-1">
        {ranked.map((row) => (
          <li
            key={row.app ?? "(unknown)"}
            className="flex min-w-0 items-baseline justify-between gap-3"
          >
            <span className="min-w-0 truncate text-t-sm text-fg-primary">
              {row.app ?? "(unknown app)"}
            </span>
            <span className="shrink-0 font-mono text-t-data tabular-nums text-fg-secondary">
              {percent(row.rate)} · {row.suppressed_spans}/{row.total_spans}
            </span>
          </li>
        ))}
      </ul>
      <p className="min-w-0 text-t-xs text-fg-tertiary">
        {`Share of recognised text dropped per app, highest first${
          rows.length > SHOWN_APPS ? ` (${SHOWN_APPS} of ${rows.length} shown)` : ""
        }. A very high share can mean over-suppression; the preserved raw text and turning app chrome back on always recover it.`}
      </p>
    </div>
  );
}
