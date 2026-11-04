// RadioGroup: a radiogroup with roving tabindex. One radio is in the tab order
// (the checked one, or the first enabled when nothing is checked); arrow keys
// move focus and selection together, Home/End jump to the ends, exactly as the
// APG describes for a radio group.
import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "../../lib/cn";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

export interface RadioOption {
  value: string;
  label: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
}

export interface RadioGroupProps {
  label: string;
  options: RadioOption[];
  value: string;
  onChange: (value: string) => void;
  hint?: ReactNode;
  /** Error message; sets aria-invalid on the group. */
  error?: string | null;
  disabled?: boolean;
  orientation?: "vertical" | "horizontal";
  /** Optional explicit id; generated when omitted. */
  id?: string;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
  className?: string;
}

export function RadioGroup({
  label,
  options,
  value,
  onChange,
  hint,
  error,
  disabled = false,
  orientation = "vertical",
  id: idProp,
  preview,
  className,
}: RadioGroupProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const labelId = `${id}-label`;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const invalid = Boolean(error);
  const describedBy =
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") ||
    undefined;
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  const enabledIndexes: number[] = [];
  options.forEach((option, index) => {
    if (!option.disabled && !disabled) enabledIndexes.push(index);
  });
  const selectedIndex = options.findIndex((option) => option.value === value);
  const tabStop = enabledIndexes.includes(selectedIndex)
    ? selectedIndex
    : enabledIndexes.length > 0
      ? enabledIndexes[0]
      : -1;

  const select = (index: number) => {
    const option = options[index];
    if (!option) return;
    onChange(option.value);
    refs.current[index]?.focus();
  };

  const step = (delta: number) => {
    if (enabledIndexes.length === 0) return;
    const base = enabledIndexes.includes(selectedIndex)
      ? selectedIndex
      : enabledIndexes[0];
    const position = enabledIndexes.indexOf(base);
    const next =
      enabledIndexes[
        (position + delta + enabledIndexes.length) % enabledIndexes.length
      ];
    select(next);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    switch (event.key) {
      case "ArrowDown":
      case "ArrowRight":
        event.preventDefault();
        step(1);
        break;
      case "ArrowUp":
      case "ArrowLeft":
        event.preventDefault();
        step(-1);
        break;
      case "Home":
        event.preventDefault();
        select(enabledIndexes[0] ?? -1);
        break;
      case "End":
        event.preventDefault();
        select(enabledIndexes[enabledIndexes.length - 1] ?? -1);
        break;
      default:
        break;
    }
  };

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <span
        id={labelId}
        className={cn(
          "text-t-ui font-medium",
          disabled ? "text-fg-disabled" : "text-fg-primary",
        )}
      >
        {label}
      </span>
      <div
        role="radiogroup"
        aria-labelledby={labelId}
        aria-describedby={describedBy}
        aria-invalid={invalid || undefined}
        aria-orientation={orientation}
        onKeyDown={onKeyDown}
        className={cn(
          "flex gap-1",
          orientation === "horizontal" ? "flex-row flex-wrap" : "flex-col",
        )}
      >
        {options.map((option, index) => {
          const isSelected = option.value === value;
          const isDisabled = disabled || option.disabled === true;
          const optionHintId = `${id}-option-${index}-hint`;
          return (
            <button
              key={option.value}
              ref={(node) => {
                refs.current[index] = node;
              }}
              type="button"
              role="radio"
              id={`${id}-option-${index}`}
              aria-checked={isSelected}
              aria-describedby={option.hint ? optionHintId : undefined}
              disabled={isDisabled}
              data-preview={preview}
              tabIndex={index === tabStop ? 0 : -1}
              onClick={() => onChange(option.value)}
              className={cn(
                "group flex min-h-hit-min items-center gap-2 rounded-sm px-1 text-left",
                "transition-colors duration-fast ease-standard motion-reduce:transition-none",
                "disabled:cursor-not-allowed",
                FOCUS,
              )}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "flex size-3.5 shrink-0 items-center justify-center rounded-pill border",
                  "transition-colors duration-fast ease-standard motion-reduce:transition-none",
                  isDisabled
                    ? "border-line-subtle bg-surface-inset"
                    : isSelected
                      ? "border-accent bg-surface-inset"
                      : "border-line-control bg-surface-inset group-hover:border-line-strong group-active:border-line-strong group-data-[preview=hover]:border-line-strong group-data-[preview=active]:border-line-strong",
                )}
              >
                {isSelected && (
                  <span
                    className={cn(
                      "size-1.5 rounded-pill",
                      isDisabled ? "bg-fg-disabled" : "bg-accent",
                    )}
                  />
                )}
              </span>
              <span className="flex flex-col gap-0.5">
                <span
                  className={cn(
                    "text-t-ui",
                    isDisabled ? "text-fg-disabled" : "text-fg-primary",
                  )}
                >
                  {option.label}
                </span>
                {option.hint && (
                  <span
                    id={optionHintId}
                    className={cn(
                      "text-t-xs",
                      isDisabled ? "text-fg-disabled" : "text-fg-tertiary",
                    )}
                  >
                    {option.hint}
                  </span>
                )}
              </span>
            </button>
          );
        })}
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
