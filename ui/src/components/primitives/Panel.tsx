// Panel - a raised surface container (card / section). (UI_REFERENCE §2/§5)
//
// One elevation step: --surface-raised, --border-subtle, radius-lg, and padding
// from the density-aware --pane-pad. `title` + `action` build the header row;
// `flush` drops the body padding for edge-to-edge content (ribbons, tables);
// `group` makes the panel an accessible form group labelled by its title.
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

export interface PanelProps extends HTMLAttributes<HTMLElement> {
  /** Header label. Sentence case; the panel does not decorate it. */
  title?: string;
  /** Right-aligned header content (a Button, Chip or status text). */
  action?: ReactNode;
  /** Removes the body padding (e.g. for an edge-to-edge ribbon). */
  flush?: boolean;
  /**
   * Expose the panel as an accessible form group: sets `role="group"` +
   * `aria-labelledby` pointing at the header label, so screen readers announce
   * the title when navigating the controls inside (the ARIA equivalent of
   * fieldset/legend without changing the card layout). Requires a `title`.
   */
  group?: boolean;
  children: ReactNode;
}

export function Panel({
  title,
  action,
  flush = false,
  group = false,
  className,
  children,
  ...rest
}: PanelProps) {
  const titleId =
    group && title
      ? `panel-${title
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "")}`
      : undefined;

  return (
    <section
      // A panel is a SURFACE, not a card: no border, no radius. Anything a panel
      // holds (a field, a chip, a switch) is then the only bordered element, so a
      // panel never nests a card inside a card (UI_REFERENCE §2; craft floor).
      className={cn("min-w-0 bg-surface-raised", className)}
      role={titleId ? "group" : undefined}
      aria-labelledby={titleId}
      {...rest}
    >
      {(title || action) && (
        <header className="flex min-h-12 items-center justify-between gap-3 border-b border-line-subtle px-pane py-3">
          {title ? (
            <h2
              id={titleId}
              className="m-0 font-text text-t-ui font-medium text-fg-primary"
            >
              {title}
            </h2>
          ) : (
            <span />
          )}
          {action}
        </header>
      )}
      <div className={cn(!flush && "p-pane")}>{children}</div>
    </section>
  );
}
