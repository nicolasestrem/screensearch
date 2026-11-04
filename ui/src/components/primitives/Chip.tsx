// Chip - a small label or status pill (UI_REFERENCE §2/§5).
//
// Tone mapping from the v1 functional palette onto the v2 semantic families:
//   neutral -> surface-inset / text-secondary / border-subtle  (a plain label)
//   accent  -> accent-wash  / accent         / accent-line     (selection, not status)
//   ok      -> status-ok-*      (neutral "saved/complete" treatment in pine,
//                                green in ink; one token family, no theme branch)
//   warn    -> status-warn-*
//   danger  -> status-danger-*
// Old tones keep their names, so `ChipTone` consumers and lib/status.ts are
// unchanged. `dot` keeps a leading status dot; `onRemove` adds an IconClose
// button (>= 24px hit target) with an accessible name.
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconClose } from "./icons";

export type ChipTone = "neutral" | "accent" | "ok" | "warn" | "danger";

const TONES: Record<ChipTone, string> = {
  neutral: "border-line-subtle bg-surface-inset text-fg-secondary",
  accent: "border-accent-line bg-accent-wash text-accent",
  ok: "border-status-ok-border bg-status-ok-bg text-status-ok-fg",
  warn: "border-status-warn-border bg-status-warn-bg text-status-warn-fg",
  danger: "border-status-danger-border bg-status-danger-bg text-status-danger-fg",
};

const REMOVE_FOCUS =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const REMOVE_FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

export interface ChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: ChipTone;
  /** Render a leading status dot in the current tone colour. */
  dot?: boolean;
  /** Render a remove affordance. */
  onRemove?: () => void;
  /** Accessible name for the remove button. Default "Remove". */
  removeLabel?: string;
  /** Static state preview for the kitchen; drives the remove button's states. */
  preview?: "hover" | "active" | "focus";
  children: ReactNode;
}

export function Chip({
  tone = "neutral",
  dot = false,
  onRemove,
  removeLabel = "Remove",
  preview,
  className,
  children,
  ...rest
}: ChipProps) {
  return (
    <span
      {...rest}
      data-preview={preview}
      className={cn(
        "group inline-flex max-w-full items-center gap-1.5 whitespace-nowrap",
        "rounded-pill border px-2 py-1.5 font-text text-t-xs",
        "transition-colors duration-fast ease-standard motion-reduce:transition-none",
        TONES[tone],
        className,
      )}
    >
      {dot && (
        <span
          aria-hidden="true"
          className="h-1.5 w-1.5 shrink-0 rounded-pill bg-current"
        />
      )}
      {typeof children === "string" || typeof children === "number" ? (
        <span className="truncate">{children}</span>
      ) : (
        children
      )}
      {onRemove && (
        <button
          type="button"
          aria-label={removeLabel}
          title={removeLabel}
          onClick={onRemove}
          className={cn(
            "relative -mr-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-pill",
            "text-fg-tertiary transition-colors duration-fast ease-standard motion-reduce:transition-none",
            "hover:bg-surface-hover hover:text-fg-primary",
            "active:bg-surface-selected active:text-fg-primary",
            "group-data-[preview=hover]:bg-surface-hover group-data-[preview=hover]:text-fg-primary",
            "group-data-[preview=active]:bg-surface-selected group-data-[preview=active]:text-fg-primary",
            REMOVE_FOCUS,
            REMOVE_FOCUS_PREVIEW,
          )}
        >
          <IconClose size={14} />
        </button>
      )}
    </span>
  );
}
