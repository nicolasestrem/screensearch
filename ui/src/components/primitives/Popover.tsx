// Popover: a button-triggered panel. The trigger owns aria-expanded and
// aria-controls, the panel is a named dialog surface, and Escape, an outside
// press or a focus move away all close it. Closing with Escape (or the trigger)
// puts focus back where it came from; a pointer press elsewhere does not steal
// focus back from what the user just touched.
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { cn } from "../../lib/cn";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

export type PopoverSide = "top" | "bottom" | "left" | "right";
export type PopoverAlign = "start" | "center" | "end";

const PLACEMENT: Record<PopoverSide, Record<PopoverAlign, string>> = {
  bottom: {
    start: "top-full left-0 mt-2",
    center: "top-full left-1/2 mt-2 -translate-x-1/2",
    end: "top-full right-0 mt-2",
  },
  top: {
    start: "bottom-full left-0 mb-2",
    center: "bottom-full left-1/2 mb-2 -translate-x-1/2",
    end: "bottom-full right-0 mb-2",
  },
  right: {
    start: "left-full top-0 ml-2",
    center: "left-full top-1/2 ml-2 -translate-y-1/2",
    end: "left-full bottom-0 ml-2",
  },
  left: {
    start: "right-full top-0 mr-2",
    center: "right-full top-1/2 mr-2 -translate-y-1/2",
    end: "right-full bottom-0 mr-2",
  },
};

/** Wiring the trigger button must carry, whether it is the default one or one
 *  the caller renders. */
export interface PopoverTriggerProps {
  ref: (node: HTMLButtonElement | null) => void;
  onClick: () => void;
  "aria-expanded": boolean;
  "aria-controls": string;
  "aria-haspopup": "dialog";
}

export interface PopoverProps {
  /** Trigger content, or a render function when the caller supplies its own
   *  button (pass the properties through unchanged). */
  trigger: ReactNode | ((props: PopoverTriggerProps) => ReactNode);
  /** Accessible name of the panel. Required: a dialog surface must be named. */
  label: string;
  children: ReactNode;
  /** Controlled open state. Omit to let the popover own it. */
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  side?: PopoverSide;
  align?: PopoverAlign;
  className?: string;
  preview?: "hover" | "active" | "focus";
}

export function Popover({
  trigger,
  label,
  children,
  open,
  defaultOpen = false,
  onOpenChange,
  side = "bottom",
  align = "start",
  className,
  preview,
}: PopoverProps) {
  const panelId = useId();
  const rootRef = useRef<HTMLSpanElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const [uncontrolled, setUncontrolled] = useState(defaultOpen);
  const controlled = open !== undefined;
  const expanded = controlled ? open : uncontrolled;

  // Keep the callback in a ref so an inline onOpenChange cannot re-run the
  // open effect (which focuses the panel) on every parent render.
  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;

  const setExpanded = useCallback(
    (next: boolean) => {
      if (!controlled) setUncontrolled(next);
      onOpenChangeRef.current?.(next);
    },
    [controlled],
  );

  useEffect(() => {
    if (!expanded) return;
    panelRef.current?.focus();

    const onPointerDown = (event: PointerEvent) => {
      const root = rootRef.current;
      if (root && !root.contains(event.target as Node)) setExpanded(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setExpanded(false);
      triggerRef.current?.focus();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [expanded, setExpanded]);

  const triggerProps: PopoverTriggerProps = {
    ref: (node) => {
      triggerRef.current = node;
    },
    onClick: () => setExpanded(!expanded),
    "aria-expanded": expanded,
    "aria-controls": panelId,
    "aria-haspopup": "dialog",
  };

  return (
    <span
      ref={rootRef}
      className="group relative inline-flex"
      data-preview={preview}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setExpanded(false);
      }}
    >
      {typeof trigger === "function" ? (
        trigger(triggerProps)
      ) : (
        <button
          type="button"
          {...triggerProps}
          className={cn(
            "inline-flex min-h-hit-min items-center gap-2 rounded-md border border-line-control bg-surface-raised px-3",
            "text-t-ui font-medium font-text text-fg-primary",
            "transition-colors duration-fast ease-standard",
            "hover:bg-surface-hover active:bg-surface-inset",
            "group-data-[preview=hover]:bg-surface-hover group-data-[preview=active]:bg-surface-inset",
            FOCUS_RING,
            FOCUS_PREVIEW,
          )}
        >
          {trigger}
        </button>
      )}
      {expanded ? (
        <div
          ref={panelRef}
          id={panelId}
          role="dialog"
          aria-label={label}
          tabIndex={-1}
          data-popover-panel="true"
          className={cn(
            "absolute z-overlay min-w-56 max-w-80 rounded-lg border border-line-subtle bg-surface-raised p-3",
            "text-t-ui font-text text-fg-primary shadow-panel",
            "outline-none motion-safe:transition-opacity motion-safe:duration-fast motion-safe:ease-standard motion-reduce:transition-none",
            PLACEMENT[side][align],
            className,
          )}
        >
          {children}
        </div>
      ) : null}
    </span>
  );
}
