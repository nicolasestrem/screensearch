// DensityTimeline — the v2 timeline instrument (UI_REFERENCE §1/§4/§7). A canvas
// density ribbon (drawn by the shared drawDensityRibbon) whose bar heights are the
// real capture count of each bucket, plus a hover preview and a hairline selection
// marker. The v1 signature skin is gone — no sweeping head, no texture overlay, no
// glow, no ambient drift (§1: no glow, motion is state-only); what remains is the
// measured ribbon and the affordances that act on it.
//
// It is a real slider: role="slider" with valuemin/max/now/text, full keyboard
// scrub (Arrows step, Shift = coarse, Home/End jump, Enter opens), and pointer
// drag. `aria-valuetext` carries the count of the bucket under the head, so
// scrubbing reads "14:32 — 7 captures in 14:30–14:35" rather than a bare clock,
// and a described-by summary names the window total and the busiest bucket: every
// bar is reachable by name without mounting one node per bucket.
//
// devicePixelRatio-crisp. The session-band layer renders in normal flow beneath
// the ribbon, so it can never overlap the bars it annotates.
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";

import type { TimelineBucket } from "../../bindings/TimelineBucket";
import type { TimeRange } from "../../bindings/TimeRange";
import type { FrameMeta } from "../../bindings/FrameMeta";
import { FrameImage } from "./FrameImage";
import { drawDensityRibbon, readRibbonColors } from "./timelineDraw";
import { absoluteDate, absoluteTime, clockTime } from "../../lib/time";

export interface DensityTimelineProps {
  buckets: TimelineBucket[];
  range: TimeRange;
  /** The focused moment (unix ms); the selection marker sits here. Controlled. */
  position: number;
  /** User moved the head (drag / click / keyboard step). */
  onScrub: (position: number) => void;
  /** User committed (Enter / double-click) — open the moment under the head. */
  onOpen: (position: number) => void;
  /** Frames for the hover thumbnail preview (nearest to the cursor). */
  thumbnails?: FrameMeta[];
  /** Ribbon height in CSS px; the session-band layer flows beneath it. */
  height?: number;
  /** Additive session-band layer. Kept outside the slider node so its native
   *  buttons do not become interactive descendants of an ARIA slider. */
  sessionLayer?: ReactNode;
}

const clamp = (v: number, lo: number, hi: number) =>
  Math.min(Math.max(v, lo), hi);

/** Half of the hover card's width (`w-28` = 112 px), so the preview stays inside the ribbon.
 *  At 112 px wide the thumbnail is 63 px tall and the clock line 19 px, so the card is 90 px
 *  inside a 96 px ribbon: it fits with 2 px to spare, and a wider card would be clipped. */
const HOVER_CARD_HALF = 56;

const countLabel = (count: number) =>
  `${count} ${count === 1 ? "capture" : "captures"}`;

function nearestThumbnailByTime(
  thumbnails: FrameMeta[],
  time: number,
): FrameMeta | null {
  if (thumbnails.length === 0) return null;

  let lo = 0;
  let hi = thumbnails.length;
  while (lo < hi) {
    const mid = lo + Math.floor((hi - lo) / 2);
    if (thumbnails[mid].captured_at < time) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }

  if (lo < thumbnails.length && thumbnails[lo].captured_at === time) {
    return thumbnails[lo];
  }
  if (lo === 0) return thumbnails[0];
  if (lo === thumbnails.length) return thumbnails[thumbnails.length - 1];

  const before = thumbnails[lo - 1];
  const after = thumbnails[lo];
  return time - before.captured_at <= after.captured_at - time ? before : after;
}

export function DensityTimeline({
  buckets,
  range,
  position,
  onScrub,
  onOpen,
  thumbnails = [],
  height = 96,
  sessionLayer,
}: DensityTimelineProps) {
  const sliderRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragging = useRef(false);
  const summaryId = useId();
  const [size, setSize] = useState({ w: 0, h: height });
  const [hover, setHover] = useState<{ x: number; time: number } | null>(null);

  const span = Math.max(0, range.end - range.start);
  const sortedThumbnails = useMemo(
    () => [...thumbnails].sort((a, b) => a.captured_at - b.captured_at),
    [thumbnails],
  );

  // Track the ribbon's pixel box so the canvas can be redrawn crisply on resize.
  useEffect(() => {
    const el = sliderRef.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Draw the density ribbon whenever the data or the box changes.
  useEffect(() => {
    const canvas = canvasRef.current;
    const slider = sliderRef.current;
    if (!canvas || !slider || size.w === 0 || size.h === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.w * dpr);
    canvas.height = Math.round(size.h * dpr);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    drawDensityRibbon(ctx, {
      width: size.w,
      height: size.h,
      dpr,
      buckets,
      range,
      colors: readRibbonColors(slider),
    });
  }, [size, buckets, range]);

  const xToTime = (clientX: number): number => {
    const el = sliderRef.current;
    if (!el || span === 0) return range.start;
    const rect = el.getBoundingClientRect();
    const frac = clamp((clientX - rect.left) / rect.width, 0, 1);
    return Math.round(range.start + frac * span);
  };

  const positionPct =
    span === 0
      ? 0
      : (clamp(position, range.start, range.end) - range.start) / span;

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    sliderRef.current?.focus();
    onScrub(xToTime(e.clientX));
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const el = sliderRef.current;
    if (el) {
      const rect = el.getBoundingClientRect();
      setHover({ x: e.clientX - rect.left, time: xToTime(e.clientX) });
    }
    if (dragging.current) onScrub(xToTime(e.clientX));
  };
  const endDrag = (e: ReactPointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (span === 0) return;
    const step = Math.max(1, Math.round(span / 240));
    const bigStep = Math.max(step, Math.round(span / 24));
    let next: number | null = null;
    switch (e.key) {
      case "ArrowRight":
        next = clamp(
          position + (e.shiftKey ? bigStep : step),
          range.start,
          range.end,
        );
        break;
      case "ArrowLeft":
        next = clamp(
          position - (e.shiftKey ? bigStep : step),
          range.start,
          range.end,
        );
        break;
      case "Home":
        next = range.start;
        break;
      case "End":
        next = range.end - 1;
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        onOpen(position);
        return;
      default:
        return;
    }
    e.preventDefault();
    if (next !== null) onScrub(next);
  };

  const hoverFrame = hover
    ? nearestThumbnailByTime(sortedThumbnails, hover.time)
    : null;

  // The bucket under the head: what `aria-valuetext` names, so each bar is
  // announced with its own span and count while scrubbing.
  const headBucket =
    buckets.find((b) => position >= b.start && position < b.end) ?? null;
  const valueText = headBucket
    ? `${absoluteTime(position)} — ${countLabel(headBucket.count)} in ${clockTime(headBucket.start)}–${clockTime(headBucket.end)}`
    : `${absoluteTime(position)} — no captures`;

  const total = buckets.reduce((sum, b) => sum + b.count, 0);
  const busiest = buckets.reduce<TimelineBucket | null>(
    (max, b) => (max === null || b.count > max.count ? b : max),
    null,
  );
  const summary =
    total === 0 || busiest === null
      ? `Capture density: no captures between ${absoluteDate(range.start)} and ${absoluteDate(range.end - 1)}.`
      : `Capture density: ${countLabel(total)} between ${absoluteDate(range.start)} and ${absoluteDate(range.end - 1)}; busiest ${clockTime(busiest.start)}–${clockTime(busiest.end)} with ${countLabel(busiest.count)}.`;

  return (
    <div className="relative w-full select-none overflow-hidden rounded-none bg-surface-app">
      <p id={summaryId} className="sr-only">
        {summary}
      </p>
      <div
        ref={sliderRef}
        role="slider"
        tabIndex={0}
        aria-label="Capture timeline — arrow keys scrub, Enter opens the moment"
        aria-valuemin={range.start}
        aria-valuemax={range.end}
        aria-valuenow={clamp(position, range.start, range.end)}
        aria-valuetext={valueText}
        aria-describedby={summaryId}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onPointerLeave={() => {
          setHover(null);
        }}
        onDoubleClick={(e) => onOpen(xToTime(e.clientX))}
        onKeyDown={onKeyDown}
        className="relative w-full cursor-crosshair outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-focus"
        style={{ height }}
      >
        <canvas
          ref={canvasRef}
          className="absolute inset-0 block h-full w-full"
        />

        {/* Selection marker — one accent hairline at the focused moment (§1: the
            accent is selection) with its clock read-out. No glow, no sweep. */}
        <div
          className="pointer-events-none absolute bottom-0 top-0 w-px bg-accent"
          style={{ left: `${positionPct * 100}%` }}
          aria-hidden="true"
        >
          <span className="absolute left-1 top-1 whitespace-nowrap rounded-sm bg-surface-raised px-1 font-mono text-t-data text-accent">
            {clockTime(position)}
          </span>
        </div>

        {/* Hover thumbnail preview (nearest frame to the cursor). */}
        {hover && hoverFrame ? (
          <div
            className="pointer-events-none absolute top-1 z-rail w-28 -translate-x-1/2"
            style={{
              left: clamp(hover.x, HOVER_CARD_HALF, Math.max(HOVER_CARD_HALF, size.w - HOVER_CARD_HALF)),
            }}
            aria-hidden="true"
          >
            <FrameImage
              imagePath={hoverFrame.image_path}
              alt=""
              className="aspect-video w-full rounded-sm border border-line-subtle bg-surface-raised object-cover"
            />
            <span className="mt-1 block text-center font-mono text-t-data text-fg-secondary">
              {clockTime(hoverFrame.captured_at)}
            </span>
          </div>
        ) : null}
      </div>
      {sessionLayer}
    </div>
  );
}
