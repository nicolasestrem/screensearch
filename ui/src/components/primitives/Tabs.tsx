// Tabs: a tab list with roving tabindex and manual activation: arrows and
// Home/End move focus, Enter or Space (or a click) activates, and only the
// active panel is shown. The selected tab carries an accent underline; a
// disabled tab keeps its place but reads as disabled and cannot be focused.
import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "../../lib/cn";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

export interface TabItem {
  /** Stable identity of the tab; doubles as the DOM id suffix. */
  id: string;
  label: ReactNode;
  content?: ReactNode;
  disabled?: boolean;
}

export interface TabsProps {
  items: TabItem[];
  /** Controlled selected tab id. Omit to let the tabs own the selection. */
  value?: string;
  defaultValue?: string;
  onValueChange?: (id: string) => void;
  /** Accessible name of the tab list. */
  label: string;
  className?: string;
  preview?: "hover" | "active" | "focus";
}

export function Tabs({
  items,
  value,
  defaultValue,
  onValueChange,
  label,
  className,
  preview,
}: TabsProps) {
  const baseId = useId();
  const enabled = items.filter((item) => !item.disabled);
  const fallbackId = enabled[0]?.id ?? items[0]?.id ?? "";
  const [uncontrolled, setUncontrolled] = useState(defaultValue ?? fallbackId);
  const [focused, setFocused] = useState<string | null>(null);
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());

  const selectedId = value ?? uncontrolled;
  const focusFallback = enabled.some((item) => item.id === selectedId)
    ? selectedId
    : enabled[0]?.id ?? "";
  const focusedId =
    focused && enabled.some((item) => item.id === focused) ? focused : focusFallback;

  const slug = (id: string) => id.replace(/[^A-Za-z0-9_-]/g, "-");
  const tabDomId = (item: TabItem) => `${baseId}-tab-${slug(item.id)}`;
  const panelDomId = (item: TabItem) => `${baseId}-panel-${slug(item.id)}`;

  const select = (id: string) => {
    if (value === undefined) setUncontrolled(id);
    onValueChange?.(id);
  };

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, item: TabItem) => {
    if (
      event.key !== "ArrowRight" &&
      event.key !== "ArrowLeft" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    if (enabled.length === 0) return;
    event.preventDefault();

    const index = enabled.findIndex((candidate) => candidate.id === item.id);
    let nextIndex = index < 0 ? 0 : index;
    if (event.key === "ArrowRight") nextIndex = (nextIndex + 1) % enabled.length;
    else if (event.key === "ArrowLeft") nextIndex = (nextIndex - 1 + enabled.length) % enabled.length;
    else if (event.key === "Home") nextIndex = 0;
    else nextIndex = enabled.length - 1;

    const next = enabled[nextIndex];
    setFocused(next.id);
    tabRefs.current.get(next.id)?.focus();
  };

  return (
    <div className={cn("group flex min-w-0 flex-col gap-3", className)} data-preview={preview}>
      <div
        role="tablist"
        aria-label={label}
        aria-orientation="horizontal"
        className="flex items-center gap-1 border-b border-line-subtle pb-2"
      >
        {items.map((item) => {
          const selected = item.id === selectedId;
          const focusable = item.id === focusedId && !item.disabled;
          return (
            <button
              key={item.id}
              ref={(node) => {
                if (node) tabRefs.current.set(item.id, node);
                else tabRefs.current.delete(item.id);
              }}
              id={tabDomId(item)}
              type="button"
              role="tab"
              aria-selected={selected}
              aria-controls={panelDomId(item)}
              tabIndex={focusable ? 0 : -1}
              disabled={item.disabled}
              onClick={() => {
                setFocused(item.id);
                select(item.id);
              }}
              onKeyDown={(event) => moveFocus(event, item)}
              className={cn(
                "-mb-2 inline-flex min-h-hit-min items-center gap-2 border-b-2 px-3",
                "text-t-ui font-medium font-text",
                "transition-colors duration-fast ease-standard",
                "hover:border-line-strong hover:text-fg-primary",
                "group-data-[preview=hover]:border-line-strong group-data-[preview=hover]:text-fg-primary",
                selected ? "border-accent text-fg-primary" : "border-transparent text-fg-secondary",
                item.disabled
                  ? "text-fg-disabled hover:border-transparent hover:text-fg-disabled"
                  : null,
                FOCUS_RING,
                FOCUS_PREVIEW,
              )}
            >
              {item.label}
            </button>
          );
        })}
      </div>
      {items
        .filter((item) => item.content !== undefined)
        .map((item) => (
          <div
            key={item.id}
            id={panelDomId(item)}
            role="tabpanel"
            aria-labelledby={tabDomId(item)}
            tabIndex={item.id === selectedId ? 0 : -1}
            hidden={item.id !== selectedId}
            className={cn(
              "min-w-0 rounded-md text-t-ui font-text text-fg-secondary",
              FOCUS_RING,
            )}
          >
            {item.content}
          </div>
        ))}
    </div>
  );
}
