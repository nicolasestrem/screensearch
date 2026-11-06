// Scope chips - exactly what a turn (or the next turn) was applied to, and
// nothing more: one window chip, one chip per entity, one thread chip. `07` C11's
// rule is that a scope is shown as applied, so the chips never summarise or
// approximate a range: a rolling window reads "Last 7 days", anything else reads
// its dated range, and no window at all reads "All history".
//
// Editable scope = every chip carries Chip's remove control plus one "Edit scope"
// action. Each removal goes to the handler that owns that part: the window chip to
// `onEditWindow`, an entity chip to `onEditEntities`, the thread chip to `onClear`
// (there is no partial thread-scope setter).
import type { TimeRange } from "../../bindings/TimeRange";
import { useId } from "react";
import { Button, Chip } from "../primitives";
import { absoluteDate, absoluteTime } from "../../lib/time";
import { cn } from "../../lib/cn";

const DAY = 86_400_000;
const HOUR = 3_600_000;

/**
 * "Last 7 days" for a window that ends now and spans whole days, else the dated
 * range: `Last 7 days` is only ever said about a window that really is seven days
 * ending today.
 */
function windowLabel(range: TimeRange, now: number): string {
  const span = Math.max(0, range.end - range.start);
  const days = Math.round(span / DAY);
  if (days >= 1 && Math.abs(span - days * DAY) < HOUR && range.end >= now - HOUR) {
    return days === 1 ? "Last 24 hours" : `Last ${days} days`;
  }
  return `${absoluteDate(range.start)} – ${absoluteDate(range.end)}`;
}

export interface ScopeEntity {
  id: number;
  canonical_name: string;
}

export interface ScopeChipsProps {
  /** `null` means no window was applied: every session counts. */
  window: TimeRange | null;
  entities: ScopeEntity[];
  threadTitle: string | null;
  /** The scope came from the previous turn rather than this one. */
  inherited?: boolean;
  editable?: boolean;
  onEditWindow?: () => void;
  onEditEntities?: () => void;
  /** Clears the applied scope; wired to the thread chip's remove control. */
  onClear: () => void;
  className?: string;
}

export function ScopeChips({
  window,
  entities,
  threadTitle,
  inherited = false,
  editable = false,
  onEditWindow,
  onEditEntities,
  onClear,
  className,
}: ScopeChipsProps) {
  const headingId = useId();
  const now = Date.now();
  // Entities are the part of a scope a person edits most, so an entity editor
  // wins when the caller supplies both; a window-only caller still gets the entry.
  const editScope = onEditEntities ?? onEditWindow;

  return (
    <div
      role="group"
      aria-labelledby={headingId}
      className={cn("grid min-w-0 gap-2", className)}
    >
      <h3 id={headingId} className="m-0 text-t-xs font-medium text-fg-secondary">
        Scope
      </h3>

      <ul className="m-0 flex list-none flex-wrap items-center gap-2 p-0">
        <li className="min-w-0">
          <Chip
            tone="neutral"
            onRemove={editable && onEditWindow ? () => onEditWindow() : undefined}
            removeLabel="Change the time window"
            title={
              window
                ? `${absoluteTime(window.start)} – ${absoluteTime(window.end)}`
                : "No time window: every session counts"
            }
          >
            {window ? windowLabel(window, now) : "All history"}
          </Chip>
        </li>

        {entities.map((entity) => (
          <li key={entity.id} className="min-w-0 max-w-full">
            <Chip
              tone="neutral"
              onRemove={editable && onEditEntities ? () => onEditEntities() : undefined}
              removeLabel={`Change the entity scope (currently ${entity.canonical_name})`}
            >
              {`Entity: ${entity.canonical_name}`}
            </Chip>
          </li>
        ))}

        {threadTitle !== null && (
          <li className="min-w-0 max-w-full">
            <Chip
              tone="neutral"
              onRemove={editable ? onClear : undefined}
              removeLabel="Clear the scope"
            >
              {`Thread: ${threadTitle}`}
            </Chip>
          </li>
        )}
      </ul>

      {inherited && (
        <p className="m-0 text-t-xs text-fg-secondary">
          Inherited from the previous turn — change or clear it
        </p>
      )}

      {editable && editScope && (
        <div>
          <Button size="sm" variant="secondary" onClick={editScope}>
            Edit scope
          </Button>
        </div>
      )}
    </div>
  );
}
