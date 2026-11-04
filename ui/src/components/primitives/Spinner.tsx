// Spinner - the "work in flight" mark for controls and inline rows.
// A slow arc, deliberately slower than the state transitions so a waiting
// control reads as busy rather than jittery, and stopped entirely under
// prefers-reduced-motion (motion-reduce:animate-none) where the label alone
// still announces the state.
//
// `role="status"` makes the state audible on its own; the visible text is
// optional, because most spinners sit beside a control that already says what is
// happening, so the accessible name lives in the visually hidden span.
import type { HTMLAttributes } from "react";

import { IconSpinner } from "./icons";
import { cn } from "../../lib/cn";

/** One revolution; slower than the 90-220ms state transitions by design. */
const SPIN_DURATION = "1.4s";

export interface SpinnerProps extends HTMLAttributes<HTMLSpanElement> {
  /** Glyph size: `sm` for inline text and buttons, `md` for rows and panels. */
  size?: "sm" | "md";
  /** Visually hidden status text. Default "Loading". */
  label?: string;
}

export function Spinner({
  size = "md",
  label = "Loading",
  className,
  ...rest
}: SpinnerProps) {
  return (
    <span
      role="status"
      className={cn("inline-flex items-center gap-2 text-fg-tertiary", className)}
      {...rest}
    >
      <IconSpinner
        size={size === "sm" ? 14 : 18}
        className="shrink-0 animate-spin motion-reduce:animate-none"
        style={{ animationDuration: SPIN_DURATION }}
      />
      <span className="sr-only">{label}</span>
    </span>
  );
}
