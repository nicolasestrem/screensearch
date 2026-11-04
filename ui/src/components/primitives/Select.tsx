// Select: a native <select> on the shared inset control surface. Native keeps
// OS keyboard behaviour and screen-reader support for free; the visual shell is
// ours, so the OS arrow is hidden and a token-stroked IconChevronDown is drawn
// inside a relative wrapper instead.
import { useId, type ReactNode, type SelectHTMLAttributes } from "react";
import { cn } from "../../lib/cn";
import { IconChevronDown } from "./icons";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

const SIZES = {
  sm: "min-h-control-sm pl-2 text-t-sm",
  md: "min-h-control pl-3 text-t-body",
};

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends Omit<
  SelectHTMLAttributes<HTMLSelectElement>,
  "id" | "size"
> {
  label: string;
  options: SelectOption[];
  hint?: ReactNode;
  /** Error message; sets aria-invalid and the danger border. */
  error?: string | null;
  /** Control height from the density scale. Default `md` (32px). */
  size?: "sm" | "md";
  /** Optional explicit id; generated when omitted. */
  id?: string;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
}

export function Select({
  label,
  options,
  hint,
  error,
  size = "md",
  id: idProp,
  preview,
  className,
  required = false,
  disabled = false,
  multiple = false,
  ...rest
}: SelectProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const invalid = Boolean(error);
  const describedBy =
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") ||
    undefined;

  return (
    <div className="flex flex-col gap-2">
      <label
        htmlFor={id}
        className={cn(
          "text-t-ui font-medium",
          disabled ? "text-fg-disabled" : "text-fg-primary",
        )}
      >
        {label}
        {required && (
          <span aria-hidden="true" className="text-status-danger-fg">
            {" *"}
          </span>
        )}
      </label>
      <div className="relative">
        <select
          {...rest}
          id={id}
          required={required}
          disabled={disabled}
          multiple={multiple}
          aria-invalid={invalid || undefined}
          aria-describedby={describedBy}
          data-preview={preview}
          className={cn(
            "w-full rounded-md border bg-surface-inset text-fg-primary font-text",
            "caret-accent",
            "transition-colors duration-fast ease-standard motion-reduce:transition-none",
            FOCUS,
            "disabled:cursor-not-allowed disabled:border-line-subtle disabled:bg-surface-inset disabled:text-fg-disabled",
            invalid
              ? "border-status-danger-border"
              : "border-line-control hover:border-line-strong data-[preview=hover]:border-line-strong active:border-line-strong data-[preview=active]:border-line-strong",
            multiple ? "py-1" : "appearance-none pr-8",
            SIZES[size],
            className,
          )}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        {!multiple && (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-fg-tertiary"
          >
            <IconChevronDown />
          </span>
        )}
      </div>
      {hint && (
        <span
          id={hintId}
          className={cn(
            "text-t-xs",
            disabled ? "text-fg-disabled" : "text-fg-tertiary",
          )}
        >
          {hint}
        </span>
      )}
      {error && (
        <span id={errorId} role="alert" className="text-t-xs text-status-danger-fg">
          {error}
        </span>
      )}
    </div>
  );
}
