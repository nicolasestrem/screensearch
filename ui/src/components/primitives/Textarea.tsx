// Textarea: multi-line text on the same inset surface as TextInput. Rows and
// every native textarea prop pass through; height stays token-driven so the
// control never invents a pixel value.
import type { TextareaHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

const SIZES = {
  sm: "min-h-control-sm px-2 py-1 text-t-sm",
  md: "min-h-control px-3 py-2 text-t-body",
};

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  /** Control density. Default `md` (32px minimum height, plus `rows`). */
  size?: "sm" | "md";
  /** Paints the danger border and sets `aria-invalid`. */
  invalid?: boolean;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
}

export function Textarea({
  size = "md",
  invalid = false,
  preview,
  className,
  rows = 3,
  ...rest
}: TextareaProps) {
  return (
    <textarea
      rows={rows}
      aria-invalid={invalid || undefined}
      data-preview={preview}
      className={cn(
        "w-full resize-y rounded-md border bg-surface-inset text-fg-primary font-text",
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
