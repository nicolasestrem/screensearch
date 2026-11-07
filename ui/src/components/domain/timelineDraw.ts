// Shared canvas draw for the density timeline and its thin minimap (UI_REFERENCE
// §1/§5 — "Reused as a thin minimap strip"). Keeping the density ribbon in one draw
// function guarantees the full timeline and the minimap read identically. The colors
// are read from the live v2 semantic custom properties since a <canvas> can't use
// Tailwind classes.
import type { TimelineBucket } from "../../bindings/TimelineBucket";
import type { TimeRange } from "../../bindings/TimeRange";

export interface RibbonColors {
  /** Bar fill (accent-wash). */
  bar: string;
  /** Bar cap line (accent). */
  barTop: string;
  /** Baseline / track (border-subtle). */
  track: string;
}

/** Reads the v2 timeline color tokens off an element's computed style. */
export function readRibbonColors(el: HTMLElement): RibbonColors {
  const cs = getComputedStyle(el);
  const root = getComputedStyle(document.documentElement);
  const v = (name: string) =>
    cs.getPropertyValue(name).trim() ||
    root.getPropertyValue(name).trim() ||
    "transparent";
  return {
    bar: v("--accent-wash"),
    barTop: v("--accent"),
    track: v("--border-subtle"),
  };
}

export interface DrawOptions {
  /** Logical (CSS) pixel size. */
  width: number;
  height: number;
  /** Device pixel ratio — the backing store is sized to width*dpr × height*dpr. */
  dpr: number;
  buckets: TimelineBucket[];
  range: TimeRange;
  colors: RibbonColors;
}

/**
 * Draws the density ribbon onto a 2D context whose canvas backing store is already
 * `width*dpr × height*dpr`. Bars are positioned by **time** (the sparse buckets map
 * onto the window), with heights normalized to the busiest bucket — so the ribbon
 * encodes real capture density, never decorative ticks. Leaves the canvas blank when
 * the window is degenerate or empty (the screen renders an explicit empty state).
 */
export function drawDensityRibbon(
  ctx: CanvasRenderingContext2D,
  o: DrawOptions,
): void {
  const { width, height, dpr, buckets, range, colors } = o;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  // Baseline track.
  ctx.fillStyle = colors.track;
  ctx.fillRect(0, height - 1, width, 1);

  const span = range.end - range.start;
  if (span <= 0 || buckets.length === 0) return;
  const maxCount = buckets.reduce((m, b) => Math.max(m, b.count), 0);
  if (maxCount <= 0) return;

  const timeToX = (t: number) => ((t - range.start) / span) * width;
  const minBarH = 2;

  for (const b of buckets) {
    const x0 = timeToX(b.start);
    const x1 = timeToX(b.end);
    const w = Math.max(1, x1 - x0 - 1); // 1px gap between adjacent bars
    const h = Math.max(minBarH, (b.count / maxCount) * (height - 2));
    const y = height - h;
    ctx.fillStyle = colors.bar;
    ctx.fillRect(x0, y, w, h);
    ctx.fillStyle = colors.barTop;
    ctx.fillRect(x0, y, w, 1); // accent cap
  }
}
