// Segmented: a radiogroup drawn as one pill-shaped group on the inset
// surface. The selected option lifts onto the raised surface with primary
// text; arrows move the selection like any radio group, and a disabled option
// is really disabled (no focus, no click).
import { useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "../../lib/cn";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

export interface SegmentedOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
}

export interface SegmentedProps {
  options: SegmentedOption[];
  /** Controlled selection. Omit to let the group own it. */
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  /** Accessible name of the group. */
  label: string;
  className?: string;
  preview?: "hover" | "active" | "focus";
}

export function Segmented({
  options,
  value,
  defaultValue,
  onChange,
  label,
  className,
  preview,
}: SegmentedProps) {
  const enabled = options.filter((option) => !option.disabled);
  const fallback = enabled[0]?.value ?? options[0]?.value ?? "";
  const [uncontrolled, setUncontrolled] = useState(defaultValue ?? fallback);
  const optionRefs = useRef(new Map<string, HTMLButtonElement>());

  const selected = value ?? uncontrolled;
  const checkedValue = enabled.find((option) => option.value === selected)?.value;
  const focusValue = checkedValue ?? enabled[0]?.value;

  const choose = (next: string) => {
    if (value === undefined) setUncontrolled(next);
    onChange?.(next);
  };

  const moveSelection = (event: KeyboardEvent<HTMLButtonElement>, current: string) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowDown" &&
        event.key !== "ArrowLeft" && event.key !== "ArrowUp") {
      return;
    }
    if (enabled.length === 0) return;
    event.preventDefault();

    const index = enabled.findIndex((option) => option.value === current);
    const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
    const next =
      enabled[
        index < 0
          ? 0
          : (index + (back ? -1 : 1) + enabled.length) % enabled.length
      ];
    choose(next.value);
    optionRefs.current.get(next.value)?.focus();
  };

  return (
    <div
      role="radiogroup"
      aria-label={label}
      data-preview={preview}
      className={cn(
        "group inline-flex items-center gap-1 rounded-pill bg-surface-inset p-2",
        className,
      )}
    >
      {options.map((option) => {
        const checked = option.value === selected;
        const focusable = !option.disabled && option.value === focusValue;
        return (
          <button
            key={option.value}
            ref={(node) => {
              if (node) optionRefs.current.set(option.value, node);
              else optionRefs.current.delete(option.value);
            }}
            type="button"
            role="radio"
            aria-checked={checked}
            disabled={option.disabled}
            tabIndex={focusable ? 0 : -1}
            onClick={() => choose(option.value)}
            onKeyDown={(event) => moveSelection(event, option.value)}
            className={cn(
              "inline-flex min-h-hit-min items-center justify-center rounded-pill px-3",
              "text-t-sm font-medium font-text transition-colors duration-fast ease-standard",
              checked
                ? "bg-surface-raised text-fg-primary shadow-raised"
                : "text-fg-secondary",
              option.disabled
                ? "text-fg-disabled"
                : checked
                  ? null
                  : cn(
                      "hover:bg-surface-hover hover:text-fg-primary",
                      "group-data-[preview=hover]:bg-surface-hover group-data-[preview=hover]:text-fg-primary",
                    ),
              FOCUS_RING,
              FOCUS_PREVIEW,
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
