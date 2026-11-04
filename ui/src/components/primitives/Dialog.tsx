// Dialog: modal surface per the APG dialog pattern: a named `role="dialog"`
// with aria-modal, a scrim on --surface-scrim, a Tab cycle that stays inside
// the panel, Escape to close, and focus returned to whatever opened it. The
// panel fades and lifts in over the base duration; reduced motion gets the end
// state with no animation at all.
import { useEffect, useId, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";
import { IconClose } from "./icons";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const SIZE: Record<NonNullable<DialogProps["size"]>, string> = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
};

/**
 * Trap focus in `panel` while `open`, close on Escape, lock the page scroll and
 * restore the previously focused element on teardown.
 */
function useModalFocus(
  open: boolean,
  panelRef: RefObject<HTMLDivElement | null>,
  onClose: () => void,
) {
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const panel = panelRef.current;
    if (!panel) return;

    const previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusable = () =>
      Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (node) => node.tabIndex !== -1 && !node.hasAttribute("hidden"),
      );

    // Focus the surface itself: the dialog is announced with its name and
    // description, and Tab decides where inside it to start.
    panel.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab") return;

      const nodes = focusable();
      if (nodes.length === 0) {
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = nodes[0];
      const last = nodes[nodes.length - 1];
      const active = document.activeElement;
      const atEdge = active === panel || !panel.contains(active);

      if (event.shiftKey && (active === first || atEdge)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || atEdge)) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousOverflow;
      previouslyFocused?.focus();
    };
  }, [open, panelRef]);
}

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  /** Footer slot, usually buttons: the decision the dialog exists for. */
  actions?: ReactNode;
  size?: "sm" | "md" | "lg";
  /** Accessible name of the close control. */
  closeLabel?: string;
  className?: string;
  preview?: "hover" | "active" | "focus";
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  actions,
  size = "md",
  closeLabel = "Close",
  className,
  preview,
}: DialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);

  useModalFocus(open, panelRef, onClose);

  useEffect(() => {
    if (!open) {
      setShown(false);
      return;
    }
    const frame = requestAnimationFrame(() => setShown(true));
    return () => cancelAnimationFrame(frame);
  }, [open]);

  if (!open) return null;

  const content = (
    <div
      className="group fixed inset-0 z-overlay flex items-end justify-center p-4 sm:items-center"
      data-preview={preview}
    >
      <div
        aria-hidden="true"
        onClick={onClose}
        className={cn(
          "absolute inset-0 bg-surface-scrim",
          "motion-safe:transition-opacity motion-safe:duration-base motion-safe:ease-standard motion-reduce:transition-none",
          shown ? "opacity-100" : "opacity-0",
        )}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
        tabIndex={-1}
        data-focus-trap="true"
        className={cn(
          "relative flex max-h-[85vh] w-full flex-col rounded-lg border border-line-subtle bg-surface-raised",
          "text-t-ui font-text text-fg-primary shadow-panel",
          "motion-safe:transition-[opacity,transform] motion-safe:duration-base motion-safe:ease-standard motion-reduce:transition-none",
          shown ? "scale-100 opacity-100" : "scale-95 opacity-0",
          SIZE[size],
          className,
        )}
      >
        <div className="flex items-start gap-3 px-4 pt-4">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="text-t-h3 font-semibold font-text text-fg-primary">
              {title}
            </h2>
            {description ? (
              <p id={descriptionId} className="mt-1 text-t-sm font-text text-fg-secondary">
                {description}
              </p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={closeLabel}
            className={cn(
              "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-fg-secondary",
              "transition-colors duration-fast ease-standard hover:bg-surface-hover hover:text-fg-primary",
              "active:bg-surface-inset group-data-[preview=hover]:bg-surface-hover group-data-[preview=hover]:text-fg-primary group-data-[preview=active]:bg-surface-inset",
              FOCUS_RING,
              FOCUS_PREVIEW,
            )}
          >
            <IconClose size={16} />
          </button>
        </div>
        {children ? (
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3 text-t-ui font-text text-fg-secondary">
            {children}
          </div>
        ) : null}
        {actions ? (
          <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line-subtle px-4 py-3">
            {actions}
          </div>
        ) : null}
      </div>
    </div>
  );

  if (typeof document === "undefined") return content;
  return createPortal(content, document.body);
}
