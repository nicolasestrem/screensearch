// ReportBuilder — picks a report range and triggers generation (UI_REFERENCE §4/§5,
// `docs/0.2.0.md` PR6). Daily / Weekly / Custom; Custom adds a date range and an
// optional steering prompt (which drives semantic retrieval). The concrete LOCAL
// `[start, end)` is computed here in the browser for every kind, so the backend never
// does timezone math. Tokens only.
import { useState, type FormEvent } from "react";

import { Button } from "../primitives";
import { cn } from "../../lib/cn";
import type { ReportKind } from "../../bindings/ReportKind";
import type { ReportRequest } from "../../bindings/ReportRequest";
import type { TimeRange } from "../../bindings/TimeRange";

/**
 * Local midnight (00:00 in the user's timezone) `offsetDays` from the given Date's
 * calendar day. Built from calendar components so it lands on the true local midnight
 * even across DST transitions — a local day is not always 86,400,000 ms (23h or 25h on
 * the switch days), so adding a fixed `DAY_MS` would over/undershoot the next midnight.
 */
function localMidnight(d: Date, offsetDays = 0): number {
  return new Date(
    d.getFullYear(),
    d.getMonth(),
    d.getDate() + offsetDays,
  ).getTime();
}

/**
 * Local midnight `offsetDays` from a `<input type="date">` value ("YYYY-MM-DD"), or null
 * if unparseable. DST-safe (see [`localMidnight`]); the JS Date constructor normalizes
 * month/day overflow, so `offsetDays = 1` correctly rolls to the next calendar day.
 */
function localDateMidnight(value: string, offsetDays = 0): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return null;
  return new Date(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]) + offsetDays,
  ).getTime();
}

/** Today's date as a `YYYY-MM-DD` string in local time (for the date inputs). */
function todayInput(): string {
  const d = new Date();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
}

export interface ReportBuilderProps {
  /** Generate a report for the resolved range (request id is added by the caller). */
  onGenerate: (request: Omit<ReportRequest, "request_id">) => void;
  /** Cancel the in-flight report. */
  onCancel: () => void;
  /** A report is currently generating. */
  busy: boolean;
}

const KINDS: { value: ReportKind; label: string }[] = [
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "custom", label: "Custom" },
];

export function ReportBuilder({
  onGenerate,
  onCancel,
  busy,
}: ReportBuilderProps) {
  const [kind, setKind] = useState<ReportKind>("daily");
  const [from, setFrom] = useState(todayInput());
  const [to, setTo] = useState(todayInput());
  const [prompt, setPrompt] = useState("");
  const [rangeError, setRangeError] = useState<string | null>(null);

  // Resolve the concrete local [start, end) for the selected kind. All bounds are built
  // from local calendar days (DST-safe), never fixed 86.4M-ms spans. Daily = today;
  // Weekly = the trailing 7 local days (incl. today); Custom = [from 00:00, to+1 00:00).
  function resolveRange(): TimeRange | null {
    const now = new Date();
    if (kind === "daily") {
      return { start: localMidnight(now), end: localMidnight(now, 1) }; // [today, tomorrow)
    }
    if (kind === "weekly") {
      // [today-6 00:00, tomorrow 00:00) spans 7 local calendar days including today.
      return { start: localMidnight(now, -6), end: localMidnight(now, 1) };
    }
    const start = localDateMidnight(from);
    const end = localDateMidnight(to, 1); // day after `to` at local 00:00 → inclusive of `to`
    if (start === null || end === null) return null;
    if (end <= start) return null;
    return { start, end };
  }

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    const time_range = resolveRange();
    if (!time_range) {
      setRangeError(
        "Pick a valid date range (the end date can't be before the start).",
      );
      return;
    }
    setRangeError(null);
    const trimmed = prompt.trim();
    onGenerate({
      kind,
      time_range,
      // The optional prompt steers only Custom reports (semantic retrieval).
      prompt: kind === "custom" && trimmed.length > 0 ? trimmed : null,
    });
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="flex gap-1" role="tablist" aria-label="Report range">
        {KINDS.map((k) => (
          <button
            key={k.value}
            type="button"
            role="tab"
            aria-selected={kind === k.value}
            onClick={() => setKind(k.value)}
            className={cn(
              "inline-flex items-center rounded-sm px-3 min-h-hit-min text-t-xs font-semibold",
              "transition-colors duration-fast ease-standard",
              kind === k.value
                ? "bg-accent-wash text-accent"
                : "text-fg-secondary hover:text-fg-primary hover:bg-surface-raised",
            )}
          >
            {k.label}
          </button>
        ))}
      </div>

      {kind === "custom" && (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-end gap-3">
            <label className="flex flex-col gap-1 text-t-xs text-fg-secondary">
              From
              <input
                type="date"
                value={from}
                max={to}
                onChange={(e) => setFrom(e.currentTarget.value)}
                className="rounded-sm border border-line-control bg-surface-app px-3 min-h-hit-min text-t-body text-fg-primary focus:border-accent"
              />
            </label>
            <label className="flex flex-col gap-1 text-t-xs text-fg-secondary">
              To
              <input
                type="date"
                value={to}
                min={from}
                max={todayInput()}
                onChange={(e) => setTo(e.currentTarget.value)}
                className="rounded-sm border border-line-control bg-surface-app px-3 min-h-hit-min text-t-body text-fg-primary focus:border-accent"
              />
            </label>
          </div>
          <label className="flex flex-col gap-1 text-t-xs text-fg-secondary">
            Focus (optional)
            <input
              type="text"
              value={prompt}
              onChange={(e) => setPrompt(e.currentTarget.value)}
              placeholder="e.g. coding work, or a project name — steers what's summarized"
              className="rounded-sm border border-line-control bg-surface-app px-3 min-h-hit-min text-t-body text-fg-primary placeholder:text-fg-tertiary focus:border-accent"
            />
          </label>
        </div>
      )}

      {rangeError && (
        <span className="text-t-xs text-status-danger-fg">{rangeError}</span>
      )}

      <div className="flex items-center gap-2">
        <Button type="submit" variant="primary" disabled={busy}>
          {busy ? "Generating…" : "Generate"}
        </Button>
        {busy && (
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        )}
      </div>
    </form>
  );
}
