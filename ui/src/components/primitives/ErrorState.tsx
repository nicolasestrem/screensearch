// ErrorState: explains what happened and how to fix it, never apologises and
// never goes vague (UI_REFERENCE §4/§9). `role="alert"` announces it, the icon is
// always present and always danger-toned (a status is never colour-alone), the
// message slot names the remedy, and the retry action is the one way forward.
//
// The retry button is deliberately local markup rather than the Button
// primitive: primitives in this folder do not import across slices. The visual
// contract (secondary weight, 32px hit, focus ring) is identical, and `action`
// lets a caller pass the shared Button once the barrel composition lands.
import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconAlert } from "./icons";

export type ErrorStatePreview = "hover" | "active" | "focus";

export interface ErrorStateProps {
  /** Overrides the default danger icon. */
  icon?: ReactNode;
  /** What failed, plainly (e.g. "Couldn't reach the kernel"). */
  title: string;
  /** The cause and the fix, in one or two sentences. */
  message?: ReactNode;
  /** Retry handler; renders a "Try again" button when present. */
  onRetry?: () => void;
  retryLabel?: string;
  /** Full replacement for the built-in retry button (e.g. a shared Button). */
  action?: ReactNode;
  /** `center` on a page, `start` inside a card or the context rail. */
  align?: "center" | "start";
  /** Static state preview for the design-system kitchen. */
  preview?: ErrorStatePreview;
  className?: string;
}

export function ErrorState({
  icon,
  title,
  message,
  onRetry,
  retryLabel = "Try again",
  action,
  align = "center",
  preview,
  className,
}: ErrorStateProps) {
  return (
    <div
      role="alert"
      data-state="error"
      className={cn(
        "flex flex-col gap-3 px-pane py-8",
        align === "start" ? "items-start text-start" : "items-center text-center",
        className,
      )}
    >
      <div className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className="inline-flex shrink-0 items-center justify-center text-status-danger-fg"
        >
          {icon ?? <IconAlert size={18} />}
        </span>
        <h2 className="text-t-h3 font-medium leading-tight text-fg-primary">
          {title}
        </h2>
      </div>
      {message && (
        <p className="max-w-prose break-words text-t-ui text-fg-secondary">
          {message}
        </p>
      )}
      {action ??
        (onRetry ? (
          <button
            type="button"
            onClick={onRetry}
            data-preview={preview}
            className={cn(
              "mt-1 inline-flex min-h-hit-min items-center justify-center gap-2 rounded-sm",
              "border border-line-control bg-surface-raised px-4 text-t-ui font-medium text-fg-primary",
              "motion-safe:transition-colors duration-fast ease-standard",
              "hover:bg-surface-hover active:bg-surface-inset",
              "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
              "data-[preview=hover]:bg-surface-hover data-[preview=active]:bg-surface-inset",
              "data-[preview=focus]:outline-2 data-[preview=focus]:outline-offset-2 data-[preview=focus]:outline-focus",
            )}
          >
            {retryLabel}
          </button>
        ) : null)}
    </div>
  );
}
