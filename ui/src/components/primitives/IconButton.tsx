// IconButton - an icon-only square action (UI_REFERENCE §2/§7).
//
// The accessible name is mandatory: `label` becomes both aria-label and the
// native tooltip. The visual is one control square (--control-h) which is also
// the minimum hit target. `active` maps to aria-pressed plus the selected
// surface, so a toggle never needs a second primitive.
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

const FOCUS =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "data-[preview=focus]:outline data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-[var(--focus-ring)]";

export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label"> {
  /** Accessible name and tooltip text. Required. */
  label: string;
  /** Toggle state: true/false renders aria-pressed; omit for a plain action. */
  active?: boolean;
  /** Static state preview for the kitchen; renders data-preview. */
  preview?: "hover" | "active" | "focus";
  children: ReactNode;
}

export function IconButton({
  label,
  active,
  preview,
  className,
  type = "button",
  children,
  ...rest
}: IconButtonProps) {
  return (
    <button
      {...rest}
      type={type}
      aria-label={label}
      aria-pressed={active}
      title={label}
      data-preview={preview}
      className={cn(
        "relative inline-flex h-control w-control shrink-0 select-none items-center justify-center",
        "rounded-md border border-transparent font-text",
        "transition-colors duration-fast ease-standard",
        "motion-reduce:transition-none",
        active
          ? cn(
              "bg-accent-wash text-accent",
              "hover:text-accent-strong active:text-accent-strong",
              "data-[preview=hover]:text-accent-strong",
              "data-[preview=active]:text-accent-strong",
            )
          : cn(
              "bg-transparent text-fg-secondary",
              "hover:bg-surface-hover hover:text-fg-primary",
              "active:bg-surface-selected active:text-fg-primary",
              "data-[preview=hover]:bg-surface-hover data-[preview=hover]:text-fg-primary",
              "data-[preview=active]:bg-surface-selected data-[preview=active]:text-fg-primary",
            ),
        "disabled:cursor-not-allowed disabled:bg-transparent disabled:text-fg-disabled",
        FOCUS,
        FOCUS_PREVIEW,
        className,
      )}
    >
      {children}
    </button>
  );
}
