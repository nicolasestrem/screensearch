// Checkbox: a 14px box with the label to its right, on a 32px row. Custom
// drawn so `aria-checked="mixed"` (indeterminate) is expressible, which a native
// <input type="checkbox"> cannot do through ARIA. Space activates the underlying
// <button>, so keyboard behaviour is native.
import { useId, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconCheck, IconMinus } from "./icons";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

export interface CheckboxProps {
  label: ReactNode;
  checked: boolean;
  /** Renders the mixed state; takes precedence over `checked`. */
  indeterminate?: boolean;
  onChange: (next: boolean) => void;
  hint?: ReactNode;
  disabled?: boolean;
  /** Error message; sets aria-invalid on the checkbox. */
  error?: string | null;
  /** Optional explicit id; generated when omitted. */
  id?: string;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
  className?: string;
}

export function Checkbox({
  label,
  checked,
  indeterminate = false,
  onChange,
  hint,
  disabled = false,
  error,
  id: idProp,
  preview,
  className,
}: CheckboxProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const labelId = `${id}-label`;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const invalid = Boolean(error);
  const describedBy =
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") ||
    undefined;
  const marked = checked || indeterminate;

  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <button
        type="button"
        role="checkbox"
        id={id}
        aria-checked={indeterminate ? "mixed" : checked}
        aria-labelledby={labelId}
        aria-describedby={describedBy}
        aria-invalid={invalid || undefined}
        disabled={disabled}
        data-preview={preview}
        onClick={() => onChange(!checked)}
        className={cn(
          "group flex min-h-hit-min items-center gap-2 rounded-sm text-left",
          "transition-colors duration-fast ease-standard motion-reduce:transition-none",
          "disabled:cursor-not-allowed",
          FOCUS,
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            "flex size-3.5 shrink-0 items-center justify-center rounded-sm border",
            "transition-colors duration-fast ease-standard motion-reduce:transition-none",
            disabled
              ? "border-line-subtle bg-surface-inset text-fg-disabled"
              : invalid
                ? "border-status-danger-border"
                : marked
                  ? "border-accent"
                  : "border-line-control group-hover:border-line-strong group-active:border-line-strong group-data-[preview=hover]:border-line-strong group-data-[preview=active]:border-line-strong",
            disabled
              ? null
              : marked
                ? "bg-accent text-fg-on-accent group-hover:bg-accent-strong group-active:bg-accent-strong group-data-[preview=hover]:bg-accent-strong group-data-[preview=active]:bg-accent-strong"
                : "bg-surface-inset",
          )}
        >
          {indeterminate ? (
            <IconMinus size={12} />
          ) : checked ? (
            <IconCheck size={12} />
          ) : null}
        </span>
        <span
          className={cn(
            "text-t-ui",
            disabled ? "text-fg-disabled" : "text-fg-primary",
          )}
        >
          {label}
        </span>
      </button>
      {hint && (
        <span
          id={hintId}
          className={cn(
            "pl-6 text-t-xs",
            disabled ? "text-fg-disabled" : "text-fg-tertiary",
          )}
        >
          {hint}
        </span>
      )}
      {error && (
        <span
          id={errorId}
          role="alert"
          className="pl-6 text-t-xs text-status-danger-fg"
        >
          {error}
        </span>
      )}
    </div>
  );
}
