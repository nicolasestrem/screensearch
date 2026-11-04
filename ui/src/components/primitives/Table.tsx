// Table: the one tabular primitive. A semantic <table> owns the structure, so
// no widget role is invented: the sort control is a real <button> inside the
// <th> and `aria-sort` reports the current direction (UI_REFERENCE §7). Numbers
// are the only cells that take the mono face, and only when the column declares
// `numeric` (§1: mono is for measured values only). All five view states live
// here, because no screen ships with only the happy path (§4).
//
// Density comes from the token layer, not from this file: cells take their
// height from `--row-h` (36px comfortable / 32px compact) and their horizontal
// rhythm from `--grid-gap`, so a screen never hardcodes a row height.
import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./ErrorState";
import { IconChevronDown, IconChevronUp, IconSortAsc } from "./icons";

export type TableState = "loading" | "empty" | "error" | "partial" | "populated";
export type TablePreview = "hover" | "active" | "focus";
export type TableSortDirection = "asc" | "desc";

export interface TableSort {
  columnId: string;
  direction: TableSortDirection;
}

export interface TableColumn<Row> {
  /** Stable column identity; also the row field used when `cell` is omitted. */
  id: string;
  header: ReactNode;
  /** Cell renderer. Without it the row's own field named by `id` is rendered. */
  cell?: (row: Row) => ReactNode;
  /** Measured value: mono + tabular figures, right aligned by default. */
  numeric?: boolean;
  /** Overrides the alignment implied by `numeric`. */
  align?: "start" | "end";
  /** Renders the header as a sort button reporting `aria-sort`. */
  sortable?: boolean;
}

export interface TableProps<Row> {
  columns: Array<TableColumn<Row>>;
  rows: Array<Row>;
  /** Stable row identity for keys and selection. Defaults to the row index. */
  getRowId?: (row: Row) => string;
  /** Accessible name for the table, rendered as a visually hidden <caption>. */
  caption?: string;
  state?: TableState;
  /** Controlled sort state; the caller cycles it in `onSortChange`. */
  sort?: TableSort | null;
  onSortChange?: (columnId: string) => void;
  /** Selected row id, filled with --surface-selected. */
  selectedRowId?: string | null;
  /**
   * Mouse convenience for activating a row. Keyboard users need a real link or
   * button inside a cell; this primitive deliberately does not put a button
   * role on a <tr>.
   */
  onRowActivate?: (row: Row) => void;
  /** Alternate row fill. Off by default: the logbook reads as one surface. */
  zebra?: boolean;
  /** Keep the header visible while the route's scroll context moves. */
  stickyHeader?: boolean;
  /** Skeleton row count while loading; matches the final row height. */
  loadingRows?: number;
  /** Rows known to the server, so `partial` can say "Showing N of M". */
  total?: number;
  emptyTitle?: string;
  emptyDescription?: ReactNode;
  emptyIcon?: ReactNode;
  emptyAction?: ReactNode;
  errorTitle?: string;
  errorMessage?: ReactNode;
  onRetry?: () => void;
  retryLabel?: string;
  preview?: TablePreview;
  className?: string;
}

/**
 * Numeric cells are mono and tabular; every other cell keeps the text face.
 * `tabular-nums` is repeated here so the cell stays correct even if it is
 * rendered outside the global `.font-mono` rule.
 */
const NUMERIC_CELL = "font-mono tabular-nums";

function defaultCell<Row>(row: Row, column: TableColumn<Row>): ReactNode {
  const value = (row as unknown as Record<string, ReactNode>)[column.id];
  return value ?? null;
}

export function Table<Row>({
  columns,
  rows,
  getRowId,
  caption,
  state = "populated",
  sort = null,
  onSortChange,
  selectedRowId = null,
  onRowActivate,
  zebra = false,
  stickyHeader = true,
  loadingRows = 5,
  total,
  emptyTitle = "Nothing here yet",
  emptyDescription,
  emptyIcon,
  emptyAction,
  errorTitle = "Couldn't load this table",
  errorMessage,
  onRetry,
  retryLabel,
  preview,
  className,
}: TableProps<Row>) {
  // Empty and error replace the table body entirely; they keep the same wrapper
  // so a screen's outer layout does not move between states.
  if (state === "empty") {
    return (
      <div className={cn("w-full", className)} data-state="empty">
        <EmptyState
          icon={emptyIcon}
          title={emptyTitle}
          description={emptyDescription}
          action={emptyAction}
        />
      </div>
    );
  }

  if (state === "error") {
    return (
      <div className={cn("w-full", className)} data-state="error">
        <ErrorState
          title={errorTitle}
          message={errorMessage}
          onRetry={onRetry}
          retryLabel={retryLabel}
        />
      </div>
    );
  }

  const cellPad = "h-row px-gap py-1.5 align-middle";
  const headerPad = "h-row p-0 align-middle";
  const rowBorder = "border-b border-line-subtle";

  return (
    <div className={cn("w-full", className)} data-state={state}>
      <table
        className="w-full border-separate border-spacing-0 text-left text-t-ui text-fg-primary"
        aria-busy={state === "loading" || undefined}
      >
        {caption && <caption className="sr-only">{caption}</caption>}
        <thead>
          <tr>
            {columns.map((column) => {
              const sorted = sort?.columnId === column.id;
              const direction = sorted ? sort?.direction : undefined;
              const alignEnd = column.align === "end" || (column.numeric === true && column.align !== "start");
              const ariaSort:
                | "ascending"
                | "descending"
                | "none"
                | undefined = column.sortable
                ? sorted
                  ? direction === "desc"
                    ? "descending"
                    : "ascending"
                  : "none"
                : undefined;
              const label = (
                <>
                  <span className="min-w-0 truncate">{column.header}</span>
                  {column.sortable &&
                    (sorted ? (
                      direction === "desc" ? (
                        <IconChevronDown size={14} className="shrink-0" />
                      ) : (
                        <IconChevronUp size={14} className="shrink-0" />
                      )
                    ) : (
                      <IconSortAsc
                        size={14}
                        className="shrink-0 text-fg-tertiary"
                      />
                    ))}
                </>
              );
              return (
                <th
                  key={column.id}
                  scope="col"
                  aria-sort={ariaSort}
                  className={cn(
                    headerPad,
                    rowBorder,
                    "bg-surface-chrome text-t-xs font-medium text-fg-secondary",
                    stickyHeader && "sticky top-0 z-10",
                  )}
                >
                  {column.sortable ? (
                    <button
                      type="button"
                      onClick={() => onSortChange?.(column.id)}
                      aria-label={
                        typeof column.header === "string"
                          ? `Sort by ${column.header}`
                          : undefined
                      }
                      className={cn(
                        "flex h-full min-h-hit-min w-full items-center gap-1 px-gap text-t-xs font-medium",
                        "rounded-sm text-fg-secondary",
                        "motion-safe:transition-colors duration-fast ease-standard",
                        "hover:text-fg-primary active:text-fg-primary",
                        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
                        "data-[preview=hover]:text-fg-primary data-[preview=active]:text-fg-primary",
                        "data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus",
                        alignEnd ? "justify-end text-end" : "justify-start text-start",
                        sorted && "text-fg-primary",
                      )}
                    >
                      {label}
                    </button>
                  ) : (
                    <div
                      className={cn(
                        "flex h-full items-center px-gap",
                        alignEnd ? "justify-end text-end" : "justify-start text-start",
                      )}
                    >
                      {label}
                    </div>
                  )}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {state === "partial" && (
            <tr>
              <td
                colSpan={columns.length}
                className={cn(
                  cellPad,
                  rowBorder,
                  "bg-surface-inset text-t-xs text-fg-secondary",
                )}
              >
                Showing {rows.length} of {total ?? rows.length}
              </td>
            </tr>
          )}
          {state === "loading"
            ? Array.from({ length: loadingRows }, (_, index) => (
                <tr key={`skeleton-${index}`} aria-hidden="true">
                  {columns.map((column) => (
                    <td key={column.id} className={cn(cellPad, rowBorder)}>
                      <span className="block h-3 w-3/4 animate-pulse rounded-sm bg-surface-inset" />
                    </td>
                  ))}
                </tr>
              ))
            : rows.map((row, index) => {
                const rowId = getRowId ? getRowId(row) : String(index);
                const selected = selectedRowId !== null && selectedRowId === rowId;
                const striped = zebra && !selected && index % 2 === 1;
                return (
                  <tr
                    key={rowId}
                    data-selected={selected || undefined}
                    data-preview={index === 0 ? preview : undefined}
                    onClick={onRowActivate ? () => onRowActivate(row) : undefined}
                    className={cn(
                      "group/row motion-safe:transition-colors duration-fast ease-standard",
                      onRowActivate && "cursor-pointer",
                      "data-[preview=focus]:outline-2 data-[preview=focus]:-outline-offset-2 data-[preview=focus]:outline-focus",
                    )}
                  >
                    {columns.map((column) => {
                      const alignEnd =
                        column.align === "end" ||
                        (column.numeric === true && column.align !== "start");
                      return (
                        <td
                          key={column.id}
                          className={cn(
                            cellPad,
                            rowBorder,
                            "text-t-ui",
                            column.numeric && NUMERIC_CELL,
                            alignEnd ? "text-end" : "text-start",
                            "group-hover/row:bg-surface-hover group-active/row:bg-surface-inset",
                            "group-data-[preview=hover]/row:bg-surface-hover",
                            "group-data-[preview=active]/row:bg-surface-inset",
                            striped && "bg-surface-inset",
                            selected && "bg-surface-selected",
                          )}
                        >
                          {column.cell ? column.cell(row) : defaultCell(row, column)}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
        </tbody>
      </table>
    </div>
  );
}
