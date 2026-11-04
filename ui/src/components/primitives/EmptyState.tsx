// EmptyState: an *invitation to act*, never decorative mood (UI_REFERENCE §4).
// The title states the situation plainly, the one hint line says what to do, and
// the optional action is the single next thing. Copy slots are honest: no
// marketing, no fabricated numbers. Padding comes from `--pane-pad` so the state
// fits a dense rail and a full page without a second size variant.
import type { ReactNode } from "react";
import { cn } from "../../lib/cn";

export interface EmptyStateProps {
  /** Inline SVG icon (one local set; no web fonts). */
  icon?: ReactNode;
  title: string;
  /** One line of guidance: what the user can do next. */
  description?: ReactNode;
  /** The single primary action (e.g. a "Start capture" Button). */
  action?: ReactNode;
  /** `center` on a page, `start` inside a card or the context rail. */
  align?: "center" | "start";
  className?: string;
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  align = "center",
  className,
}: EmptyStateProps) {
  return (
    <div
      data-state="empty"
      className={cn(
        "flex flex-col gap-3 px-pane py-8",
        align === "start" ? "items-start text-start" : "items-center text-center",
        className,
      )}
    >
      <div className="flex items-center gap-2">
        {icon && (
          <span aria-hidden="true" className="flex shrink-0 items-center justify-center text-fg-tertiary">
            {icon}
          </span>
        )}
        <h2 className="text-t-h3 font-medium leading-tight text-fg-primary">
          {title}
        </h2>
      </div>
      {description && (
        <p className="max-w-prose text-t-ui text-fg-secondary">{description}</p>
      )}
      {action && <div className="mt-1 flex items-center gap-2">{action}</div>}
    </div>
  );
}
