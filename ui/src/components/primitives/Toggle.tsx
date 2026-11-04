// Toggle: an on/off switch (`role="switch"` + `aria-checked`). The native
// <button> gives Space/Enter activation and focus for free; the visual track is
// 18px tall (12px knob, 2px inset, 1px border) inside a 32px hit box, so the
// control stays easy to hit without a chunky track.
import { useId, type ReactNode } from "react";
import { cn } from "../../lib/cn";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

export interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  hint?: ReactNode;
  disabled?: boolean;
  /** Optional explicit id; generated when omitted. */
  id?: string;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
}

export function Toggle({
  label,
  checked,
  onChange,
  hint,
  disabled = false,
  id: idProp,
  preview,
}: ToggleProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const labelId = `${id}-label`;
  const hintId = `${id}-hint`;

  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex flex-col gap-1">
        <label
          id={labelId}
          htmlFor={id}
          className={cn(
            "text-t-ui font-medium",
            disabled ? "text-fg-disabled" : "text-fg-primary",
          )}
        >
          {label}
        </label>
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
      </div>
      <button
        type="button"
        id={id}
        role="switch"
        aria-checked={checked}
        aria-labelledby={labelId}
        aria-describedby={hint ? hintId : undefined}
        disabled={disabled}
        data-preview={preview}
        onClick={() => onChange(!checked)}
        className={cn(
          "group relative inline-flex min-h-hit-min shrink-0 items-center justify-center rounded-pill px-1",
          "transition-colors duration-fast ease-standard motion-reduce:transition-none",
          "disabled:cursor-not-allowed",
          FOCUS,
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            "flex w-8 items-center rounded-pill border p-0.5",
            "transition-colors duration-fast ease-standard motion-reduce:transition-none",
            disabled
              ? "border-line-subtle bg-surface-inset"
              : checked
                ? "border-accent bg-accent group-hover:bg-accent-strong group-active:bg-accent-strong group-data-[preview=hover]:bg-accent-strong group-data-[preview=active]:bg-accent-strong"
                : "border-line-control bg-surface-inset group-hover:border-line-strong group-active:border-line-strong group-data-[preview=hover]:border-line-strong group-data-[preview=active]:border-line-strong",
          )}
        >
          <span
            className={cn(
              "size-3 rounded-pill",
              "transition-transform duration-fast ease-standard motion-reduce:transition-none",
              disabled
                ? "bg-fg-disabled"
                : checked
                  ? "bg-fg-on-accent"
                  : "bg-fg-secondary",
              checked ? "translate-x-3.5" : "translate-x-0",
            )}
          />
        </span>
      </button>
    </div>
  );
}
