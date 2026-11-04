// Expander - a Panel-shaped disclosure for a collapsed-by-default group of
// settings (UI_REFERENCE §2 semantic tokens, §5 one job per component, §7 AA).
// The whole header row is the disclosure button (`aria-expanded` +
// `aria-controls`); the body is a labelled `role="region"` so the group keeps the
// form-group semantics its caller gives the surrounding panels. The intro
// sentence stays visible while collapsed - it is what tells a user whether the
// group is worth opening. Open state belongs to the caller (per-session via the
// ui store; no persistence machinery here).
//
// The body is always in the DOM so its markup can be asserted while collapsed;
// `hidden` keeps it out of the accessibility tree and out of the layout. The
// height/opacity transition runs 140ms on the shared ease curve and is fully
// disabled under prefers-reduced-motion (motion-reduce:transition-none), which
// is why the grid row, not a measured pixel height, does the animating.
import { useId, type ReactNode } from "react";

import { IconChevronRight } from "./icons";
import { cn } from "../../lib/cn";

/** Static state for the /kitchen sheet; mirrors the live interaction states. */
export type ExpanderPreview = "hover" | "active" | "focus";

/** One focus ring for every interactive element in the primitives. */
const FOCUS_RING =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

export interface ExpanderProps {
  /** Title of the group (sentence case, e.g. "Capture tuning"). */
  title: string;
  /** One plain-language sentence on what the group is for (UI_REFERENCE §9). */
  intro?: string;
  /** Whether the body is shown. Controlled by the caller. */
  open: boolean;
  onToggle: () => void;
  /** Renders `data-preview` so states can be inspected statically. */
  preview?: ExpanderPreview;
  className?: string;
  children: ReactNode;
}

export function Expander({
  title,
  intro,
  open,
  onToggle,
  preview,
  className,
  children,
}: ExpanderProps) {
  const id = useId();
  const titleId = `${id}-title`;
  const bodyId = `${id}-body`;
  return (
    <section
      className={cn(
        // A surface, not a card: the disclosure body may hold a field or a switch,
        // which must be the only bordered element (no card inside a card).
        "group bg-surface-raised",
        className,
      )}
      data-preview={preview}
      data-state={open ? "open" : "closed"}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={onToggle}
        className={cn(
          "flex w-full items-center justify-between gap-3 px-4 py-3 text-left",
          "min-h-row rounded-lg text-t-ui text-fg-primary",
          "transition-colors duration-fast ease-standard",
          "hover:bg-surface-hover group-data-[preview=hover]:bg-surface-hover",
          "active:bg-surface-selected group-data-[preview=active]:bg-surface-selected",
          "focus-visible:bg-surface-hover group-data-[preview=focus]:bg-surface-hover",
          FOCUS_RING,
          open && "rounded-b-none",
        )}
      >
        <span className="flex min-w-0 flex-col gap-0.5">
          <span id={titleId} className="font-medium">
            {title}
          </span>
          {intro && (
            <span className="text-t-sm font-normal text-fg-secondary">
              {intro}
            </span>
          )}
        </span>
        <IconChevronRight
          size={16}
          className={cn(
            "shrink-0 text-fg-tertiary transition-transform duration-base ease-standard motion-reduce:transition-none",
            open && "rotate-90",
          )}
        />
      </button>
      <div className={cn("grid", open ? "grid-rows-[1fr]" : "grid-rows-[0fr]")}>
        <div className="min-h-0 overflow-hidden">
          <div
            id={bodyId}
            role="region"
            aria-labelledby={titleId}
            hidden={!open}
            className={cn(
              "border-t border-line-subtle p-4 rounded-b-lg text-t-body text-fg-secondary",
              "transition-opacity duration-base ease-standard motion-reduce:transition-none",
              open ? "opacity-100" : "opacity-0",
            )}
          >
            {children}
          </div>
        </div>
      </div>
    </section>
  );
}
