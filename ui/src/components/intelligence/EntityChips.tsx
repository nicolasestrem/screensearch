// Entity chips - one chip per entity a thread or a scope touched (`03 §7h.1`).
// Fetches nothing: the caller owns the rows, the selection and the removal, so
// the same chip row serves a thread's entity list (a link into the filtered work
// list), a filter (a pressed state) and a scope (a removable chip).
//
// A chip hosts at most one interactive label plus Chip's own remove button; the
// label is never a button wrapping a button, and never a link wrapping a button.
import { Link } from "react-router-dom";
import type { ReactNode } from "react";
import { Chip } from "../primitives";
import { cn } from "../../lib/cn";
import { evidenceHref } from "./evidenceLink";

export interface EntityChipItem {
  id: number;
  /** The `entities.kind` token (repo / file / domain / document / ref / app). */
  kind: string;
  canonical_name: string;
  /** Carried by thread and entity reads; never rendered (no score, `0.4.5 D13`). */
  confidence?: number;
}

export interface EntityChipsProps {
  entities: EntityChipItem[];
  /** Ids currently applied as a filter; renders `aria-pressed`. */
  selectedIds?: number[];
  /**
   * Selects instead of navigating. When set the chip label is a `<button>`, so a
   * caller that also wants removal must supply its own control: a chip cannot
   * host two buttons without nesting them.
   */
  onSelect?: (id: number) => void;
  removable?: boolean;
  onRemove?: (id: number) => void;
  /** Shown when `entities` is empty. */
  emptyLabel?: string;
  className?: string;
}

const LABEL_CLASS =
  "inline-flex min-h-hit-min min-w-0 items-center rounded-sm text-start " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:text-fg-primary " +
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

export function EntityChips({
  entities,
  selectedIds,
  onSelect,
  removable = false,
  onRemove,
  emptyLabel,
  className,
}: EntityChipsProps) {
  if (entities.length === 0) {
    return (
      <p className={cn("m-0 text-t-ui text-fg-secondary", className)}>
        {emptyLabel ?? "No entities in this thread"}
      </p>
    );
  }

  const selected = new Set(selectedIds ?? []);

  return (
    <ul className={cn("m-0 flex list-none flex-wrap items-center gap-2 p-0", className)}>
      {entities.map((entity) => {
        const isSelected = selected.has(entity.id);
        const label = <span className="truncate">{entity.canonical_name}</span>;
        const removeLabel = `Remove ${entity.canonical_name}`;

        let control: ReactNode;
        if (onSelect) {
          control = (
            <button
              type="button"
              aria-pressed={isSelected}
              onClick={() => onSelect(entity.id)}
              className={LABEL_CLASS}
            >
              {label}
            </button>
          );
        } else {
          const href = evidenceHref("entity", entity.id);
          control = (
            <Link
              to={href}
              title={`Threads that touched ${entity.canonical_name}`}
              className={LABEL_CLASS}
            >
              {label}
            </Link>
          );
        }

        return (
          <li key={entity.id} className="min-w-0 max-w-full">
            <Chip
              tone="neutral"
              // Selection and removal are two different controls: Chip's remove
              // button cannot live inside the selecting label, so a selectable
              // chip leaves removal to the caller.
              onRemove={!onSelect && removable && onRemove ? () => onRemove(entity.id) : undefined}
              removeLabel={removeLabel}
              className={cn(isSelected && "ring-1 ring-inset ring-accent")}
            >
              {control}
            </Chip>
          </li>
        );
      })}
    </ul>
  );
}
