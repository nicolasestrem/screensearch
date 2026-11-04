// TextInput: one line of text on an inset surface. The visual shell for every
// single-line control: inset background, control border, themed caret and
// placeholder, an explicit focus ring and a danger border when `invalid`.
//
// Field owns the label/hint/error wiring; TextInput only paints the control, so
// it can also sit inside composite widgets (Combobox reuses the same shell).
import type { InputHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

/** Shared focus ring: 2px token outline with a 2px offset, preview-mirrored. */
const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

const SIZES = {
  sm: "min-h-control-sm px-2 text-t-sm",
  md: "min-h-control px-3 text-t-body",
};

export interface TextInputProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "size"
> {
  /** Control height from the density scale. Default `md` (32px). */
  size?: "sm" | "md";
  /** Paints the danger border and sets `aria-invalid`. */
  invalid?: boolean;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
}

export function TextInput({
  size = "md",
  invalid = false,
  preview,
  className,
  type = "text",
  ...rest
}: TextInputProps) {
  return (
    <input
      type={type}
      aria-invalid={invalid || undefined}
      data-preview={preview}
      className={cn(
        "w-full rounded-md border bg-surface-inset text-fg-primary font-text",
        "placeholder:text-fg-tertiary caret-accent",
        "transition-colors duration-fast ease-standard motion-reduce:transition-none",
        FOCUS,
        "disabled:cursor-not-allowed disabled:border-line-subtle disabled:bg-surface-inset disabled:text-fg-disabled disabled:placeholder:text-fg-disabled",
        invalid
          ? "border-status-danger-border"
          : "border-line-control hover:border-line-strong data-[preview=hover]:border-line-strong active:border-line-strong data-[preview=active]:border-line-strong",
        SIZES[size],
        className,
      )}
      {...rest}
    />
  );
}
