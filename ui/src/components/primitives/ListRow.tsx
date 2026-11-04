// ListRow: one row in a dense list (Work, Recall, Settings groups). A grid is
// used so the leading slot, the text column and the trailing slot keep one
// alignment whether the row is plain content or a real link/button; keyboard
// and pointer therefore share exactly one surface. Height comes from `--row-h`
// (36px comfortable / 32px compact), which always clears `--hit-min` (32px), so
// the interactive hit area never falls below the accessibility floor (§7).
//
// Selection is not colour-only: the row fills with --surface-selected *and*
// carries a 1px accent inset outline plus a 2px leading accent bar, so the state
// survives a monochrome or low-vision rendering.
import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconAlert, IconInfo } from "./icons";

export type ListRowState =
  | "loading"
  | "empty"
  | "error"
  | "partial"
  | "populated";
export type ListRowPreview = "hover" | "active" | "focus";

export interface ListRowProps {
  /** An <a> when `href` is set, a <button>, or a plain <div>. */
  as?: "a" | "button" | "div";
  href?: string;
  /** Activation handler for the button, or an extra handler for the anchor. */
  onActivate?: () => void;
  /** Leading slot: an icon, or a small avatar. */
  icon?: ReactNode;
  title?: ReactNode;
  /** One line of secondary context. */
  meta?: ReactNode;
  /** Trailing slot: a count, a timestamp, or one quiet control. */
  trailing?: ReactNode;
  selected?: boolean;
  state?: ListRowState;
  preview?: ListRowPreview;
  className?: string;
}

const ROW_BASE =
  "relative grid w-full min-h-row grid-cols-[auto_1fr_auto] items-center gap-3 px-3 text-start";

const ROW_INTERACTIVE =
  "cursor-pointer motion-safe:transition-colors duration-fast ease-standard " +
  "hover:bg-surface-hover active:bg-surface-inset " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus " +
  "data-[preview=hover]:bg-surface-hover data-[preview=active]:bg-surface-inset " +
  "data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus";

const ROW_SELECTED = "bg-surface-selected ring-1 ring-inset ring-accent";


export function ListRow({
  as = "div",
  href,
  onActivate,
  icon,
  title,
  meta,
  trailing,
  selected = false,
  state = "populated",
  preview,
  className,
}: ListRowProps) {
  const skipsInteraction = state === "loading" || state === "empty" || state === "error";
  const tag = as === "a" && href && !skipsInteraction ? "a" : as === "button" && !skipsInteraction ? "button" : "div";
  const interactive = tag !== "div";

  const rowClass = cn(
    ROW_BASE,
    interactive && ROW_INTERACTIVE,
    selected && ROW_SELECTED,
    className,
  );

  const selectionMark = selected ? (
    <>
      <span className="sr-only">Selected</span>
    </>
  ) : null;

  const trail = trailing ? (
    <span className="flex shrink-0 items-center gap-2 text-t-xs text-fg-tertiary">
      {trailing}
    </span>
  ) : null;

  if (state === "loading") {
    return (
      <div className={rowClass} aria-busy="true">
        {icon && (
          <span className="flex items-center justify-center">
            <span className="block h-4 w-4 animate-pulse rounded-sm bg-surface-inset" />
          </span>
        )}
        <span className="grid min-w-0 gap-1">
          <span className="block h-3 w-1/2 animate-pulse rounded-sm bg-surface-inset" />
          <span className="block h-2.5 w-1/3 animate-pulse rounded-sm bg-surface-inset" />
        </span>
        {trailing && (
          <span className="flex shrink-0 items-center">
            <span className="block h-3 w-8 animate-pulse rounded-sm bg-surface-inset" />
          </span>
        )}
      </div>
    );
  }

  const partialMark = state === "partial";

  const text =
    state === "empty" ? (
      <span className="grid min-w-0 gap-0.5">
        <span className="truncate text-t-body font-medium leading-tight text-fg-secondary">
          {title ?? "Nothing here yet"}
        </span>
      </span>
    ) : state === "error" ? (
      <span className="grid min-w-0 gap-0.5">
        <span className="truncate text-t-body font-medium leading-tight text-fg-primary">
          {title ?? "Couldn't load this row"}
        </span>
        {meta && (
          <span className="truncate text-t-xs leading-tight text-fg-secondary">
            {meta}
          </span>
        )}
      </span>
    ) : (
      <span className="grid min-w-0 gap-0.5">
        {title && (
          <span className="truncate text-t-body font-medium leading-tight text-fg-primary">
            {title}
          </span>
        )}
        {(meta || partialMark) && (
          <span className="flex min-w-0 items-center gap-2">
            {meta && (
              <span className="truncate text-t-xs leading-tight text-fg-secondary">
                {meta}
              </span>
            )}
            {partialMark && (
              <span className="flex shrink-0 items-center gap-1 text-t-xs text-fg-tertiary">
                <IconInfo size={12} />
                Partial data
              </span>
            )}
          </span>
        )}
      </span>
    );

  const leading =
    state === "error" ? (
      <span className="flex items-center justify-center text-status-danger-fg">
        <IconAlert size={18} />
      </span>
    ) : (
      icon && (
        <span className="flex items-center justify-center text-fg-tertiary">
          {icon}
        </span>
      )
    );

  const content = (
    <>
      {selectionMark}
      {leading}
      {text}
      {trail}
    </>
  );

  if (tag === "a") {
    return (
      <a
        href={href}
        onClick={onActivate}
        aria-current={selected ? "true" : undefined}
        data-selected={selected || undefined}
        data-preview={preview}
        className={rowClass}
      >
        {content}
      </a>
    );
  }

  if (tag === "button") {
    return (
      <button
        type="button"
        onClick={onActivate}
        aria-current={selected ? "true" : undefined}
        data-selected={selected || undefined}
        data-preview={preview}
        className={rowClass}
      >
        {content}
      </button>
    );
  }

  return (
    <div
      aria-current={selected ? "true" : undefined}
      data-selected={selected || undefined}
      data-preview={preview}
      className={rowClass}
    >
      {content}
    </div>
  );
}
