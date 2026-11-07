// TimelineMinimap — the thin, read-only reuse of the density ribbon (UI_REFERENCE
// §1/§5: "Reused as a thin minimap strip"). Shares drawDensityRibbon with the full
// DensityTimeline, so density reads identically. Click (or Enter) seeks: it reports
// the time under the cursor so the host can jump into the full timeline there; a
// keyboard activation has no cursor, so it seeks to the most recent edge of the
// window, which is where the full timeline opens by default.
import { useEffect, useRef, useState } from "react";

import type { TimelineBucket } from "../../bindings/TimelineBucket";
import type { TimeRange } from "../../bindings/TimeRange";
import { absoluteDate } from "../../lib/time";
import { cn } from "../../lib/cn";
import { drawDensityRibbon, readRibbonColors } from "./timelineDraw";

export interface TimelineMinimapProps {
  buckets: TimelineBucket[];
  range: TimeRange;
  /** Click / Enter reports the time under the cursor (keyboard: the window end). */
  onSeek?: (time: number) => void;
  height?: number;
}

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

export function TimelineMinimap({
  buckets,
  range,
  onSeek,
  height = 28,
}: TimelineMinimapProps) {
  const wrapRef = useRef<HTMLButtonElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const span = Math.max(0, range.end - range.start);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    drawDensityRibbon(ctx, {
      width,
      height,
      dpr,
      buckets,
      range,
      colors: readRibbonColors(wrap),
    });
  }, [width, height, buckets, range]);

  const seekFromX = (clientX: number) => {
    const el = wrapRef.current;
    if (!el || span === 0) return onSeek?.(range.end - 1);
    const rect = el.getBoundingClientRect();
    const frac = Math.min(Math.max((clientX - rect.left) / rect.width, 0), 1);
    onSeek?.(Math.round(range.start + frac * span));
  };

  const windowLabel = `${absoluteDate(range.start)} to ${absoluteDate(range.end - 1)}`;
  // The strip is a bar ribbon: name it with the window it covers and the total it
  // encodes, so the accessible name is not a bare "open".
  const total = buckets.reduce((sum, b) => sum + b.count, 0);
  const densityLabel =
    total === 0
      ? `no captures, ${windowLabel}`
      : `${total} capture${total === 1 ? "" : "s"}, ${windowLabel}`;

  return (
    <button
      ref={wrapRef}
      type="button"
      aria-label={`Capture density: ${densityLabel} — open the timeline`}
      onClick={(e) => {
        // Keyboard activation reports no pointer position (`detail` 0), so it
        // seeks to the recent edge instead of the oldest instant in the window.
        if (e.detail === 0) {
          onSeek?.(range.end - 1);
          return;
        }
        seekFromX(e.clientX);
      }}
      className={cn(
        "relative block w-full overflow-hidden rounded-none bg-surface-app",
        FOCUS_RING,
      )}
      style={{ height }}
    >
      <canvas ref={canvasRef} className="block h-full w-full" />
    </button>
  );
}
