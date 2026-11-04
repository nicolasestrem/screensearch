// Button - the one action primitive (UI_REFERENCE §2/§5/§7).
//
// Variants read only semantic tokens: primary = --action-primary-*, secondary =
// raised surface + control border, ghost = transparent with a hover wash, danger
// = --action-danger-*. Sizes set the visual height from --control-h / -sm and
// widen the hit area to --hit-min with a transparent pseudo-element, so a 28px
// button still answers a 32px target. No hex, no px font, no magic spacing.
//
// `preview` renders data-preview="hover" | "active" | "focus" and mirrors every
// interactive style with a data-[preview=...] twin, so /kitchen can show states
// statically; it changes no behaviour.
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconSpinner } from "./icons";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

const FOCUS =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "data-[preview=focus]:outline data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-[var(--focus-ring)]";

const VARIANTS: Record<ButtonVariant, string> = {
  primary: cn(
    "border-transparent bg-action-primary text-action-primary-fg",
    "hover:bg-action-primary-hover active:bg-action-primary-hover",
    "data-[preview=hover]:bg-action-primary-hover",
    "data-[preview=active]:bg-action-primary-hover",
  ),
  secondary: cn(
    "border-line-control bg-surface-raised text-fg-primary",
    "hover:border-line-strong hover:bg-surface-hover",
    "active:border-line-strong active:bg-surface-selected",
    "data-[preview=hover]:border-line-strong data-[preview=hover]:bg-surface-hover",
    "data-[preview=active]:border-line-strong data-[preview=active]:bg-surface-selected",
  ),
  ghost: cn(
    "border-transparent bg-transparent text-fg-secondary",
    "hover:bg-surface-hover hover:text-fg-primary",
    "active:bg-surface-selected active:text-fg-primary",
    "data-[preview=hover]:bg-surface-hover data-[preview=hover]:text-fg-primary",
    "data-[preview=active]:bg-surface-selected data-[preview=active]:text-fg-primary",
  ),
  danger: cn(
    "border-transparent bg-action-danger text-action-danger-fg",
    "hover:bg-action-danger-hover active:bg-action-danger-hover",
    "data-[preview=hover]:bg-action-danger-hover",
    "data-[preview=active]:bg-action-danger-hover",
  ),
};

const SIZES: Record<ButtonSize, string> = {
  sm: "h-control-sm px-3 text-t-sm",
  md: "h-control px-4 text-t-ui",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Decorative glyph rendered before the label. */
  leadingIcon?: ReactNode;
  /** Busy state: sets aria-busy + aria-disabled and swaps the leading glyph for a spinner. */
  loading?: boolean;
  /** Static state preview for the kitchen; renders data-preview. */
  preview?: "hover" | "active" | "focus";
}

export function Button({
  variant = "secondary",
  size = "md",
  leadingIcon,
  loading = false,
  preview,
  className,
  type = "button",
  onClick,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      {...rest}
      type={type}
      aria-busy={loading ? true : rest["aria-busy"]}
      aria-disabled={loading ? true : rest["aria-disabled"]}
      data-preview={preview}
      // While loading the button must not act, and that includes a native form
      // submit: `aria-disabled` is announced but does not stop activation, so
      // dropping `onClick` alone would leave `type="submit"` free to submit the
      // form again while the previous request is still pending.
      onClick={(event) => {
        if (loading) {
          event.preventDefault();
          return;
        }
        onClick?.(event);
      }}
      className={cn(
        "relative inline-flex shrink-0 select-none items-center justify-center gap-2",
        "whitespace-nowrap rounded-md border font-text font-medium",
        "transition-colors duration-fast ease-standard",
        "motion-reduce:transition-none",
        // Transparent hit area: visual height may be shorter than --hit-min.
        "before:absolute before:inset-x-0 before:top-1/2 before:h-hit-min before:-translate-y-1/2 before:content-['']",
        "disabled:cursor-not-allowed disabled:border-line-subtle disabled:bg-surface-inset disabled:text-fg-disabled",
        FOCUS,
        FOCUS_PREVIEW,
        VARIANTS[variant],
        SIZES[size],
        className,
      )}
    >
      {loading ? (
        <span className="inline-flex shrink-0 items-center" aria-hidden="true">
          <IconSpinner size={16} className="motion-safe:animate-spin" />
        </span>
      ) : (
        leadingIcon && (
          <span className="inline-flex shrink-0 items-center" aria-hidden="true">
            {leadingIcon}
          </span>
        )
      )}
      {children}
    </button>
  );
}
