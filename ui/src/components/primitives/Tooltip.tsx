// Tooltip: a short label revealed by intent: 400 ms of hover, or keyboard
// focus. Dismissed on leave, blur or Escape. Placed in pure CSS inside a
// relative wrapper, so it measures nothing, never blocks the pointer and needs
// no portal. The trigger keeps its own focus ring; the bubble adds none.
import {
  cloneElement,
  isValidElement,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { cn } from "../../lib/cn";

/** Hover/focus dwell before the label appears. */
const INTENT_DELAY_MS = 400;

export type TooltipSide = "top" | "bottom" | "left" | "right";

const PLACEMENT: Record<TooltipSide, string> = {
  top: "bottom-full left-1/2 mb-2 -translate-x-1/2",
  bottom: "top-full left-1/2 mt-2 -translate-x-1/2",
  left: "top-1/2 right-full mr-2 -translate-y-1/2",
  right: "top-1/2 left-full ml-2 -translate-y-1/2",
};

export interface TooltipProps {
  /** The label. Text only: a tooltip is a description, not a surface. */
  label: ReactNode;
  /** Which edge of the trigger the label sits on. */
  side?: TooltipSide;
  /** Dwell before showing, in ms. */
  delay?: number;
  /** Suppress the label without unmounting the trigger. */
  disabled?: boolean;
  className?: string;
  /** Static state for the kitchen; hover or focus also show the label. */
  preview?: "hover" | "active" | "focus";
  children: ReactNode;
}

export function Tooltip({
  label,
  side = "top",
  delay = INTENT_DELAY_MS,
  disabled = false,
  className,
  preview,
  children,
}: TooltipProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);

  const stopTimer = () => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  };

  const showSoon = () => {
    if (disabled || !label || open) return;
    stopTimer();
    timer.current = window.setTimeout(() => setOpen(true), delay);
  };

  const hide = () => {
    stopTimer();
    setOpen(false);
  };

  const visible = open || preview !== undefined;

  // Wire aria-describedby straight onto the trigger when it is a single
  // element, merging with any description the consumer already set.
  let trigger = children;
  if (isValidElement(children)) {
    const element = children as ReactElement<Record<string, unknown>>;
    const existing = element.props["aria-describedby"];
    trigger = cloneElement(element, {
      "aria-describedby": visible
        ? existing
          ? `${String(existing)} ${id}`
          : id
        : existing,
    });
  }

  return (
    <span
      className="group relative inline-flex"
      data-preview={preview}
      onMouseEnter={showSoon}
      onMouseLeave={hide}
      onFocus={showSoon}
      onBlur={hide}
    >
      {trigger}
      {label ? (
        <span
          id={id}
          role="tooltip"
          className={cn(
            "pointer-events-none absolute z-overlay max-w-xs rounded-md border border-line-subtle bg-surface-raised",
            "w-max px-2 py-1 text-t-xs font-text text-fg-primary shadow-raised",
            "transition-opacity duration-fast ease-standard motion-reduce:transition-none",
            visible ? "visible opacity-100" : "invisible opacity-0",
            PLACEMENT[side],
            className,
          )}
        >
          {label}
        </span>
      ) : null}
    </span>
  );
}
