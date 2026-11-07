// FrameReconstruction — a text + layout reconstruction of a captured frame, drawn from
// its stored OCR/UIA spans (normalized [0,1] geometry + classified role). This is the
// durable "proof" shown in the Moment view once a frame's screenshot has been retention-
// degraded (the pixels are gone, but each word's position and line height are enough to
// redraw what was on screen). Positions/sizes come from span data; color and font are
// tokens only. Container-query height units (`cqh`) scale each word to its real line
// height so the layout reads at any container size.
import type { TextSpan } from "../../bindings/TextSpan";
import { cn } from "../../lib/cn";

export interface FrameReconstructionProps {
  spans: TextSpan[];
  /** Native frame dimensions — set the canvas aspect ratio. */
  width: number;
  height: number;
  className?: string;
}

/** Role → emphasis: content reads at full strength, chrome/system/background recede. */
const ROLE_TONE: Record<TextSpan["role"], string> = {
  content: "text-fg-primary",
  chrome: "text-fg-tertiary",
  system: "text-fg-tertiary",
  background: "text-fg-tertiary",
  unknown: "text-fg-secondary",
};

export function FrameReconstruction({
  spans,
  width,
  height,
  className,
}: FrameReconstructionProps) {
  const aspect = width > 0 && height > 0 ? width / height : 16 / 9;

  return (
    <div
      className={cn(
        "relative w-full overflow-hidden rounded-md border border-line-subtle bg-surface-pane",
        className,
      )}
      style={{ aspectRatio: String(aspect), containerType: "size" }}
      role="img"
      aria-label="Text reconstruction of the captured screen"
    >
      {spans.length === 0 ? (
        <div className="absolute inset-0 flex items-center justify-center p-4 text-center">
          <span className="text-t-body text-fg-tertiary">
            No recognized text to reconstruct for this frame.
          </span>
        </div>
      ) : (
        spans.map((s, i) => {
          // Fit each word inside its STORED bbox so the reconstruction reflects the saved
          // layout, not an overlapping one: line height sets the size, but a long word is
          // also capped to its box width (≈0.6em per monospace char) so it can't paint over
          // the spans to its right. `cqh`/`cqw` = % of the canvas height/width.
          const chars = Math.max(s.text.length, 1);
          const byHeight = `${Math.max(s.h * 90, 0.5)}cqh`;
          const fontSize =
            s.w > 0
              ? `min(${byHeight}, ${(s.w * 100) / (0.6 * chars)}cqw)`
              : byHeight;
          return (
            <span
              key={i}
              className={cn(
                "absolute overflow-hidden whitespace-nowrap font-mono leading-none",
                ROLE_TONE[s.role],
              )}
              // Geometry is data, not design tokens: position + box come from the span's
              // normalized bbox. `maxWidth` is clamped ≥ 0 — OCR noise can yield x or w > 1,
              // and a negative max-width is invalid CSS (silently ignored).
              style={{
                left: `${s.x * 100}%`,
                top: `${s.y * 100}%`,
                maxWidth: `${Math.max(s.w * 100, 0)}%`,
                fontSize,
              }}
            >
              {s.text}
            </span>
          );
        })
      )}
    </div>
  );
}
