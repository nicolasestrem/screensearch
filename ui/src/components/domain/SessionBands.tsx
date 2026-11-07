// SessionBands — an additive layer over the density ribbon (`UI_REFERENCE §3`/§4).
// The measured pixel geometry (including the tokenized 32px hit target) drives both
// deterministic collision packing and rendering, so CSS cannot create overlaps the
// packer did not see. Rows stay in normal flow and expand the ribbon as needed.
//
// Exactly four lanes exist in every state (`FIXED_SESSION_LANES`): the initial
// skeleton, loading, error, empty and populated all reserve the same four
// token-height rows. Sessions that packing would place in lane 5+ never grow the
// ribbon; they are counted into one neutral, keyboard-operable overflow control
// that focuses the Timeline's existing range presets.
//
// Bands are session links, so they read as relationships: neutral surfaces,
// neutral borders and the kind carried by the label — no second hue (`§1`).
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

import type { Session } from "../../bindings/Session";
import type { SessionKind } from "../../bindings/SessionKind";
import type { TimeRange } from "../../bindings/TimeRange";
import { absoluteTime } from "../../lib/time";
import { cn } from "../../lib/cn";
import { Skeleton } from "../primitives";
import {
  FIXED_SESSION_LANES,
  packFixedSessionBands,
  type SessionBandLayoutMetrics,
} from "./sessionBandLayout";

export interface SessionBandsProps {
  sessions: Session[];
  range: TimeRange;
  loading?: boolean;
  error?: string | null;
  onRetry?: () => void;
  onFocusRangePresets?: () => void;
  onOpen: (sessionId: number) => void;
}

const EMPTY_METRICS: SessionBandLayoutMetrics = {
  width: 0,
  hitTarget: 0,
  gap: 0,
};

const KIND_LABEL: Record<SessionKind, string> = {
  focus: "Focus",
  ai: "AI",
  meeting: "Meeting",
  other: "Other",
};

/** The one band surface: a neutral relationship, never a status hue (`§1`). */
const BAND_STYLE = "border-line-subtle bg-surface-raised text-fg-primary";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

const CONTROL_STYLE = cn(
  "pointer-events-auto min-h-hit-min rounded-sm border border-line-subtle px-3 text-t-xs",
  "motion-safe:transition-colors duration-fast ease-standard",
  FOCUS_RING,
);

function cssTokenPx(style: CSSStyleDeclaration, token: string): number {
  const value = Number.parseFloat(style.getPropertyValue(token));
  return Number.isFinite(value) && value > 0 ? value : 0;
}

function stopSliderPointer(e: ReactPointerEvent<HTMLButtonElement>) {
  e.stopPropagation();
}

export function SessionBands({
  sessions,
  range,
  loading = false,
  error = null,
  onRetry,
  onFocusRangePresets,
  onOpen,
}: SessionBandsProps) {
  const measureRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] =
    useState<SessionBandLayoutMetrics>(EMPTY_METRICS);

  useEffect(() => {
    const element = measureRef.current;
    if (!element) return;
    const measure = () => {
      const style = getComputedStyle(element);
      const next = {
        width: element.clientWidth,
        hitTarget: cssTokenPx(style, "--hit-min"),
        gap: cssTokenPx(style, "--space-1"),
      };
      setMetrics((current) =>
        current.width === next.width &&
        current.hitTarget === next.hitTarget &&
        current.gap === next.gap
          ? current
          : next,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const packed = useMemo(
    () =>
      packFixedSessionBands(
        sessions,
        range.start,
        range.end,
        metrics,
        (session) => session.started_at,
        (session) => session.ended_at ?? range.end,
      ),
    [sessions, range.start, range.end, metrics],
  );

  return (
    <div className="pointer-events-none relative z-rail pb-2 pt-2">
      <div ref={measureRef} className="relative mx-2 flex flex-col gap-1">
        <div className="flex h-hit-min items-center justify-end">
          {loading ? (
            <Skeleton className="h-hit-min w-1/4 border border-line-subtle" />
          ) : error ? (
            <button
              type="button"
              className={cn(
                CONTROL_STYLE,
                "bg-surface-raised text-fg-secondary hover:text-fg-primary",
              )}
              onPointerDown={stopSliderPointer}
              onClick={(e) => {
                e.stopPropagation();
                onRetry?.();
              }}
              title={error}
            >
              Sessions unavailable · Retry
            </button>
          ) : packed.overflowCount > 0 ? (
            <button
              type="button"
              className={cn(
                CONTROL_STYLE,
                "bg-transparent text-fg-secondary hover:bg-surface-hover hover:text-fg-primary",
              )}
              onPointerDown={stopSliderPointer}
              onClick={(e) => {
                e.stopPropagation();
                onFocusRangePresets?.();
              }}
            >
              {packed.overflowCount} more sessions — narrow the range
            </button>
          ) : (
            <span className="text-t-xs text-fg-tertiary">
              {packed.bands.length > 0
                ? "Sessions"
                : "No sessions in this range"}
            </span>
          )}
        </div>
        <div
          className="grid gap-1"
          style={{
            gridTemplateRows: `repeat(${FIXED_SESSION_LANES}, var(--hit-min))`,
          }}
          role="group"
          aria-label="Sessions in this timeline range"
        >
          {loading
            ? Array.from({ length: FIXED_SESSION_LANES }, (_, lane) => (
                <Skeleton
                  key={lane}
                  className="h-hit-min border border-line-subtle"
                  style={{
                    gridRow: lane + 1,
                    width: `${Math.max(20, 35 - lane * 5)}%`,
                  }}
                />
              ))
            : packed.bands.map(({ item: session, lane, leftPx, widthPx }) => {
              const kind = KIND_LABEL[session.kind];
              const endLabel = session.ended_at
                ? absoluteTime(session.ended_at)
                : "still running";
              const title = session.title?.trim() || "Untitled session";
              const context = [session.tool, session.host]
                .filter((value) => value != null)
                .join(", ");
              const label = `${kind} session: ${title}. ${absoluteTime(session.started_at)} to ${endLabel}${context ? `. ${context}` : ""}`;
              return (
                <button
                  key={session.id}
                  type="button"
                  aria-label={label}
                  title={label}
                  onPointerDown={stopSliderPointer}
                  onDoubleClick={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation();
                    onOpen(session.id);
                  }}
                  className={cn(
                    "pointer-events-auto h-hit-min truncate rounded-sm border px-2 text-left text-t-xs font-medium",
                    "motion-safe:transition-colors duration-fast ease-standard hover:bg-surface-hover",
                    FOCUS_RING,
                    BAND_STYLE,
                  )}
                  style={{
                    gridRow: lane + 1,
                    gridColumn: 1,
                    marginLeft: `${leftPx}px`,
                    width: `${widthPx}px`,
                  }}
                >
                  {kind} · {absoluteTime(session.started_at)}–{endLabel} · {title}
                </button>
              );
            })}
        </div>
      </div>
    </div>
  );
}
