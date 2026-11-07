// Timeline (/timeline) — the density timeline browser (UI_REFERENCE §1/§3/§4).
// A canvas density ribbon over the selected window — every bar is the real capture
// count of its bucket, never decoration — scrubbed by pointer or keyboard; Enter
// opens the moment nearest the selection marker (resolved server-side to a real
// frame id, so it lands precisely — not on a sampled thumbnail). Range presets scope
// the window (and the Recall search). All five states: loading skeleton, empty ("No
// captures in this range"), error+retry, partial (thumbnails resolving), populated.
// The scrub area never goes blank — empty windows show an invitation.
//
// The session-band layer sits inside the ribbon component and reserves exactly four
// lanes in all five states (`SessionBands`, `FIXED_SESSION_LANES`), so data arrival
// never shifts the page. One scroll context: the shell's <main>; nothing here
// scrolls and no strip overflows horizontally.
import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";

import {
  Button,
  Chip,
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
} from "../components/primitives";
import { DensityTimeline, SessionBands } from "../components/domain";
import * as cmd from "../lib/ipc/commands";
import { useFrames, useSessions, useTimeline } from "../lib/ipc/queries";
import { useUiStore } from "../state/uiStore";
import { toast } from "../state/toastStore";
import { absoluteDate, absoluteTime } from "../lib/time";
import { lastDaysRange } from "../lib/timeRanges";
import { cn } from "../lib/cn";
import { useAdaptiveBucketCount } from "../lib/useAdaptiveBucketCount";

const DEFAULT_BUCKETS = 240;
const THUMB_LIMIT = 300; // hover-preview frames sampled across the window
const PRESETS = [
  { label: "Today", days: 1 },
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
] as const;

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

const clamp = (v: number, lo: number, hi: number) =>
  Math.min(Math.max(v, lo), hi);

export function Component() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const setSelectedRange = useUiStore((s) => s.setSelectedRange);
  const rangePresetRef = useRef<HTMLDivElement>(null);

  const [days, setDays] = useState(1);
  const range = lastDaysRange(days);

  // Selection position (unix ms). Initialised from a `?t=` deep link (e.g. the deck
  // minimap) or defaults to the most recent edge of the window.
  const [position, setPosition] = useState(() => {
    const t = Number(searchParams.get("t"));
    return Number.isFinite(t) && t > 0 ? t : range.end - 1;
  });

  const [timelineMeasureRef, bucketCount] = useAdaptiveBucketCount(
    DEFAULT_BUCKETS,
    120,
    2000,
    4,
  );
  const timeline = useTimeline(range, bucketCount);
  const thumbs = useFrames(range, THUMB_LIMIT);
  const sessions = useSessions({
    time_range: range,
    kind: null,
    tool: null,
    limit: null,
  });

  // Keep the head inside the window when the range changes.
  useEffect(() => {
    setPosition((p) => clamp(p, range.start, range.end - 1));
  }, [range.start, range.end]);

  const pickRange = (d: number) => {
    setDays(d);
    const r = lastDaysRange(d);
    setSelectedRange(r);
    setPosition(r.end - 1);
  };

  const openAt = async (pos: number) => {
    try {
      const frame = await cmd.getNearestFrame(Math.round(pos), range);
      if (frame) navigate(`/timeline/${frame.frame_id}`);
      else toast.info("No capture near that time");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const buckets = timeline.data ?? [];
  const hasData = buckets.length > 0;
  const sessionLayer = (forceLoading = false) => (
    <SessionBands
      sessions={sessions.data ?? []}
      range={range}
      loading={forceLoading || sessions.isLoading}
      error={sessions.isError ? String(sessions.error) : null}
      onRetry={() => sessions.refetch()}
      onFocusRangePresets={() =>
        rangePresetRef.current
          ?.querySelector<HTMLButtonElement>("button")
          ?.focus()
      }
      onOpen={(sessionId) => navigate(`/timeline/session/${sessionId}`)}
    />
  );

  const rangeControl = (
    <div
      ref={rangePresetRef}
      className="flex flex-wrap gap-1"
      role="group"
      aria-label="Time range"
    >
      {PRESETS.map((p) => (
        <button
          key={p.days}
          type="button"
          aria-pressed={days === p.days}
          onClick={() => pickRange(p.days)}
          className={cn(
            "inline-flex min-h-hit-min items-center rounded-sm px-3 text-t-xs font-medium",
            "motion-safe:transition-colors duration-fast ease-standard",
            FOCUS_RING,
            days === p.days
              ? "bg-accent-wash text-accent"
              : "text-fg-secondary hover:bg-surface-hover hover:text-fg-primary",
          )}
        >
          {p.label}
        </button>
      ))}
    </div>
  );

  return (
    <div className="mx-auto flex min-w-0 max-w-6xl flex-col gap-pane px-pane py-pane">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-t-h1 font-semibold text-fg-primary">Timeline</h1>
          <p className="mt-1 text-t-sm text-fg-secondary">
            {absoluteDate(range.start)} — {absoluteDate(range.end - 1)}
          </p>
        </div>
        {rangeControl}
      </header>

      <div ref={timelineMeasureRef} className="min-w-0">
        <Panel
          title="Capture density"
          flush
          action={
            hasData ? (
              <Chip tone="accent">
                <span className="font-mono">{absoluteTime(position)}</span>
              </Chip>
            ) : undefined
          }
        >
          {timeline.isLoading ? (
            <div className="flex flex-col gap-2">
              <div className="relative min-h-24 overflow-hidden rounded-none bg-surface-app">
                <Skeleton className="absolute inset-0 h-full w-full rounded-none" />
              </div>
              {sessionLayer(true)}
              <Skeleton className="h-4 w-2/3" />
            </div>
          ) : timeline.isError ? (
            <div className="flex flex-col gap-2">
              <ErrorState
                title="Couldn't load the timeline"
                message={String(timeline.error)}
                onRetry={() => timeline.refetch()}
              />
              {sessionLayer()}
            </div>
          ) : !hasData ? (
            <div className="flex flex-col gap-2">
              <EmptyState
                title="No captures in this range"
                description="Nothing was recorded in this window. Widen the range, or start capture from the Deck."
                action={
                  <Button variant="secondary" onClick={() => navigate("/")}>
                    Back to Deck
                  </Button>
                }
              />
              {sessionLayer()}
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <DensityTimeline
                buckets={buckets}
                range={range}
                position={position}
                onScrub={setPosition}
                onOpen={openAt}
                thumbnails={thumbs.data ?? []}
                sessionLayer={sessionLayer()}
              />
              <p className="px-1 text-t-xs text-fg-tertiary">
                Drag or use ← → to scrub (Shift for bigger steps, Home/End to
                jump). Enter opens the moment.
                {thumbs.isLoading ? " · Loading thumbnails…" : ""}
                {sessions.isLoading ? " · Loading sessions…" : ""}
                {sessions.isError ? " · Session bands unavailable." : ""}
              </p>
            </div>
          )}
        </Panel>
      </div>
    </div>
  );
}
