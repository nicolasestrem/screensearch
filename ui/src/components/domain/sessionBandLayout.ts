/**
 * The band reservation (`UI_REFERENCE §3`): exactly four lanes exist in every
 * Timeline state — initial skeleton, loading, error, empty and populated all
 * reserve these same rows, so data arrival never shifts the layout. Items that
 * packing would place in lane 5+ are counted by `overflowCount` instead of
 * growing the layer (no fifth lane, no modal, no new control).
 */
export const FIXED_SESSION_LANES = 4;

export interface SessionBandLayoutMetrics {
  width: number;
  hitTarget: number;
  gap: number;
}

export interface FixedSessionBand<T> {
  item: T;
  lane: number;
  leftPx: number;
  widthPx: number;
}

export interface FixedSessionBandLayout<T> {
  bands: FixedSessionBand<T>[];
  overflowCount: number;
  rowCount: typeof FIXED_SESSION_LANES;
}

export function packFixedSessionBands<T>(
  items: T[],
  rangeStart: number,
  rangeEnd: number,
  metrics: SessionBandLayoutMetrics,
  getStart: (item: T) => number,
  getEnd: (item: T) => number,
): FixedSessionBandLayout<T> {
  const { width, hitTarget, gap } = metrics;
  if (width <= 0 || hitTarget <= 0) {
    return { bands: [], overflowCount: 0, rowCount: FIXED_SESSION_LANES };
  }

  const span = Math.max(1, rangeEnd - rangeStart);
  const minBandWidth = Math.min(hitTarget, width);
  const ordered = items
    .map((item, inputOrder) => {
      const start = Math.max(rangeStart, getStart(item));
      const end = Math.min(rangeEnd, getEnd(item));
      const rawLeft = ((start - rangeStart) / span) * width;
      const rawRight = ((end - rangeStart) / span) * width;
      const renderedWidth = Math.min(
        width,
        Math.max(minBandWidth, rawRight - rawLeft),
      );
      const leftPx = Math.min(
        Math.max(0, rawLeft),
        Math.max(0, width - renderedWidth),
      );
      return {
        item,
        inputOrder,
        start,
        end,
        leftPx,
        widthPx: renderedWidth,
        rightPx: leftPx + renderedWidth,
      };
    })
    .filter((item) => item.end > item.start)
    .sort(
      (a, b) =>
        a.leftPx - b.leftPx ||
        a.rightPx - b.rightPx ||
        a.start - b.start ||
        a.inputOrder - b.inputOrder,
    );
  const laneEnds: number[] = [];
  const bands: FixedSessionBand<T>[] = [];
  let overflowCount = 0;

  for (const item of ordered) {
    let lane = laneEnds.findIndex((endPx) => item.leftPx >= endPx + gap);
    if (lane < 0) lane = laneEnds.length;
    laneEnds[lane] = item.rightPx;
    if (lane >= FIXED_SESSION_LANES) {
      overflowCount += 1;
      continue;
    }
    bands.push({
      item: item.item,
      lane,
      leftPx: item.leftPx,
      widthPx: item.widthPx,
    });
  }

  return { bands, overflowCount, rowCount: FIXED_SESSION_LANES };
}
