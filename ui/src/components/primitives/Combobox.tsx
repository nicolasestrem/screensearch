// Combobox: a filterable single-select built on the APG listbox pattern:
// a text input with role="combobox", a popup listbox, and options. Arrow keys
// move the active option, Home/End jump to the ends, Enter selects, Escape
// closes. The input keeps DOM focus throughout; the active option is announced
// with aria-activedescendant, never by moving focus into the list, and the
// popup scrolls to keep that option visible when the list overflows.
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconChevronDown } from "./icons";

const FOCUS =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

const SIZES = {
  sm: "min-h-control-sm pl-2 pr-8 text-t-sm",
  md: "min-h-control pl-3 pr-8 text-t-body",
};

const POPOVER =
  "absolute left-0 right-0 top-full z-overlay mt-1 max-h-64 overflow-y-auto rounded-md border border-line-subtle bg-surface-raised py-1 shadow-panel";

export interface ComboboxOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface ComboboxProps {
  label: string;
  options: ComboboxOption[];
  value: string;
  onChange: (value: string) => void;
  hint?: ReactNode;
  /** Error message; sets aria-invalid and the danger border. */
  error?: string | null;
  placeholder?: string;
  disabled?: boolean;
  required?: boolean;
  /** Control height from the density scale. Default `md` (32px). */
  size?: "sm" | "md";
  /** Copy shown when the filter matches nothing. */
  emptyMessage?: string;
  /** Data state of the option set. */
  state?: "loading" | "empty" | "error" | "partial" | "populated";
  /** Optional explicit id; generated when omitted. */
  id?: string;
  /** Static interaction state for the kitchen gallery. */
  preview?: "hover" | "active" | "focus";
  className?: string;
}

export function Combobox({
  label,
  options,
  value,
  onChange,
  hint,
  error,
  placeholder,
  disabled = false,
  required = false,
  size = "md",
  emptyMessage = "No matching options",
  state = "populated",
  id: idProp,
  preview,
  className,
}: ComboboxProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const listboxId = `${id}-listbox`;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const invalid = Boolean(error);
  const describedBy =
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") ||
    undefined;

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState<string | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);

  const selected = options.find((option) => option.value === value);
  const text = query ?? selected?.label ?? "";
  const needle = (query ?? "").trim().toLowerCase();
  const filtered =
    query === null || needle.length === 0
      ? options
      : options.filter((option) => option.label.toLowerCase().includes(needle));
  const active =
    filtered.length === 0 || activeIndex < 0
      ? -1
      : Math.min(activeIndex, filtered.length - 1);
  const expanded = open && !disabled;
  const showList = expanded && filtered.length > 0;

  // The popup caps its height and DOM focus never enters it, so the active
  // option is revealed by scrolling the listbox alone: only `scrollTop` moves,
  // never the page. Measured from rects, so the popup's border and `py-1`
  // padding stay visible at either end.
  const listRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    const list = listRef.current;
    if (!showList || active < 0 || !list) return;
    const option = list.children[active] as HTMLElement | undefined;
    if (!option) return;
    const style = getComputedStyle(list);
    const inner = list.getBoundingClientRect().top + list.clientTop;
    const viewTop = inner + (parseFloat(style.paddingTop) || 0);
    const viewBottom =
      inner + list.clientHeight - (parseFloat(style.paddingBottom) || 0);
    const rect = option.getBoundingClientRect();
    if (rect.top < viewTop) list.scrollTop += rect.top - viewTop;
    else if (rect.bottom > viewBottom) list.scrollTop += rect.bottom - viewBottom;
  }, [showList, active, filtered.length]);

  const commit = (option: ComboboxOption) => {
    if (option.disabled) return;
    onChange(option.value);
    setQuery(null);
    setOpen(false);
  };

  // Arrow keys, Home and End land on enabled options only, as a native select
  // does: landing on a disabled row would leave the user on a row that paints no
  // highlight (disabled text over the selected wash is below AA in all four
  // themes) with Enter doing nothing.
  const stepTo = (from: number, direction: 1 | -1): number => {
    const count = filtered.length;
    for (let k = 1; k <= count; k += 1) {
      const index = (((from + direction * k) % count) + count) % count;
      if (!filtered[index]?.disabled) return index;
    }
    return -1;
  };
  const edgeTo = (direction: 1 | -1): number => {
    const start = direction === 1 ? 0 : filtered.length - 1;
    return filtered[start]?.disabled ? stepTo(start, direction) : start;
  };

  const close = () => {
    setOpen(false);
    setQuery(null);
  };

  const openList = () => {
    if (disabled) return;
    setOpen(true);
    // Index into what is actually rendered, so reopening with a filter typed
    // does not point `aria-activedescendant` at a row that is not in the list.
    const selectedAt = filtered.findIndex((option) => option.value === value);
    const start = selectedAt >= 0 ? selectedAt : 0;
    // `stepTo` returns -1 when every option is disabled: no row is active, which
    // is the truth, and Enter then has nothing to commit.
    setActiveIndex(filtered[start]?.disabled ? stepTo(start, 1) : start);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (!expanded) {
          openList();
          return;
        }
        if (filtered.length > 0) {
          const next = stepTo(active, 1);
          if (next >= 0) setActiveIndex(next);
        }
        break;
      case "ArrowUp":
        event.preventDefault();
        if (!expanded) {
          openList();
          return;
        }
        if (filtered.length > 0) {
          const next = stepTo(active, -1);
          if (next >= 0) setActiveIndex(next);
        }
        break;
      case "Home": {
        if (!expanded) return;
        event.preventDefault();
        const first = edgeTo(1);
        if (first >= 0) setActiveIndex(first);
        break;
      }
      case "End": {
        if (!expanded) return;
        event.preventDefault();
        const last = edgeTo(-1);
        if (last >= 0) setActiveIndex(last);
        break;
      }
      case "Enter":
        if (!expanded) return;
        event.preventDefault();
        if (active >= 0 && filtered[active]) commit(filtered[active]);
        break;
      case "Escape":
        if (!expanded) return;
        event.preventDefault();
        close();
        break;
      case "Tab":
        close();
        break;
      default:
        break;
    }
  };

  return (
    <div className={cn("flex flex-col gap-2", className)}>
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
      <div
        className="relative"
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
            close();
          }
        }}
      >
        <input
          id={id}
          type="text"
          role="combobox"
          autoComplete="off"
          spellCheck={false}
          aria-expanded={expanded}
          aria-haspopup="listbox"
          aria-controls={listboxId}
          aria-autocomplete="list"
          aria-activedescendant={
            showList && active >= 0 ? `${listboxId}-option-${active}` : undefined
          }
          aria-invalid={invalid || undefined}
          aria-describedby={describedBy}
          value={text}
          placeholder={placeholder}
          disabled={disabled}
          required={required}
          data-preview={preview}
          onChange={(event) => {
            setQuery(event.currentTarget.value);
            setOpen(true);
            setActiveIndex(0);
          }}
          onClick={openList}
          onKeyDown={onKeyDown}
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
          )}
        />
        <span
          aria-hidden="true"
          className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-fg-tertiary"
        >
          <IconChevronDown />
        </span>
        <ul
          id={listboxId}
          ref={listRef}
          role="listbox"
          aria-label={label}
          hidden={!showList}
          className={POPOVER}
        >
          {showList &&
            filtered.map((option, index) => {
              const optionDisabled = option.disabled === true;
              const isActive = index === active;
              const isSelected = option.value === value;
              return (
                <li
                  key={option.value}
                  id={`${listboxId}-option-${index}`}
                  role="option"
                  aria-selected={isSelected}
                  aria-disabled={optionDisabled || undefined}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => {
                    if (!optionDisabled) setActiveIndex(index);
                  }}
                  onClick={() => commit(option)}
                  className={cn(
                    "flex min-h-hit-min cursor-pointer items-center px-3 text-t-ui",
                    "motion-reduce:transition-none",
                    // Keyboard navigation skips disabled options (as a native
                    // select does), so this branch should be unreachable for
                    // `active`; it stays as the mouse-hover guard.
                    optionDisabled
                      ? "cursor-not-allowed text-fg-disabled"
                      : isActive
                        ? "bg-surface-selected text-fg-primary"
                        : "text-fg-primary",
                  )}
                >
                  <span className="truncate">{option.label}</span>
                </li>
              );
            })}
        </ul>
        {expanded && filtered.length === 0 && (
          <div className={POPOVER}>
            <div role="status" aria-live="polite" className="px-3 py-2">
              {state === "loading" ? (
                <span className="flex flex-col gap-1">
                  <span className="block h-3 w-full rounded-sm bg-surface-hover motion-safe:animate-pulse" />
                  <span className="block h-3 w-2/3 rounded-sm bg-surface-hover motion-safe:animate-pulse" />
                </span>
              ) : state === "error" ? (
                <span className="text-t-xs text-status-danger-fg">
                  Options unavailable
                </span>
              ) : (
                <span className="text-t-xs text-fg-tertiary">{emptyMessage}</span>
              )}
            </div>
          </div>
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
