// Toast: one transient notification, and the viewport that stacks them. The
// viewport keeps the store contract used by the shell and the overlay window
// (useToastStore.push/dismiss); each item owns its auto-dismiss timer, which a
// caller can override or switch off with `duration={0}`. The live region is
// always mounted so a change is announced rather than appearing with the text.
import { useEffect } from "react";
import { cn } from "../../lib/cn";
import { useToastStore, type ToastItem } from "../../state/toastStore";
import type { ToastLevel } from "../../bindings/ToastLevel";
import {
  IconAlert,
  IconCheckCircle,
  IconClose,
  IconInfo,
  IconSpinner,
  IconUndo,
  IconWarn,
  type IconProps,
} from "./icons";

const FOCUS_RING =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";
const FOCUS_PREVIEW =
  "group-data-[preview=focus]:outline group-data-[preview=focus]:outline-2 group-data-[preview=focus]:outline-offset-2 group-data-[preview=focus]:outline-[var(--focus-ring)]";

/** Default lifetime per level; errors linger longest. */
const DEFAULT_DURATION: Record<ToastLevel, number> = {
  info: 4000,
  success: 4000,
  warning: 6000,
  error: 8000,
};

const LEVEL: Record<ToastLevel, { icon: string; edge: string }> = {
  info: { icon: "text-fg-secondary", edge: "border-l-line-strong" },
  // Pine success is the neutral treatment: primary ink on the raised surface.
  success: { icon: "text-fg-primary", edge: "border-l-status-ok-border" },
  warning: { icon: "text-status-warn-fg", edge: "border-l-status-warn-border" },
  error: { icon: "text-status-danger-fg", edge: "border-l-status-danger-border" },
};

function LevelIcon({ level, ...rest }: { level: ToastLevel } & IconProps) {
  if (level === "success") return <IconCheckCircle {...rest} />;
  if (level === "warning") return <IconWarn {...rest} />;
  if (level === "error") return <IconAlert {...rest} />;
  return <IconInfo {...rest} />;
}

export interface ToastAction {
  label: string;
  onAction: () => void;
}

export interface ToastProps {
  toast: ToastItem;
  onClose: (id: number) => void;
  /** Milliseconds before auto-dismiss; `0` keeps the toast until dismissed. */
  duration?: number;
  /** Trailing action, usually Undo. */
  action?: ToastAction;
  /** Work in progress: shows a spinner and marks the toast aria-busy. */
  loading?: boolean;
  preview?: "hover" | "active" | "focus";
}

export function Toast({
  toast,
  onClose,
  duration,
  action,
  loading = false,
  preview,
}: ToastProps) {
  const { id, level, message } = toast;

  useEffect(() => {
    const lifetime = duration ?? DEFAULT_DURATION[level];
    if (loading || lifetime <= 0) return;
    const timer = window.setTimeout(() => onClose(id), lifetime);
    return () => window.clearTimeout(timer);
  }, [id, level, duration, loading, onClose]);

  return (
    <div
      data-preview={preview}
      aria-busy={loading || undefined}
      className={cn(
        "group pointer-events-auto flex w-full max-w-sm items-start gap-3",
        "rounded-lg border border-line-subtle bg-surface-raised px-3 py-2",
        "text-t-ui font-text text-fg-primary shadow-toast",
        "motion-safe:transition-opacity motion-safe:duration-base motion-safe:ease-standard motion-reduce:transition-none",
        LEVEL[level].edge,
      )}
    >
      <span className={cn("mt-0.5 shrink-0", LEVEL[level].icon)}>
        {loading ? (
          <IconSpinner size={16} className="motion-safe:animate-spin motion-reduce:animate-none" />
        ) : (
          <LevelIcon level={level} size={16} />
        )}
      </span>
      <p className="min-w-0 flex-1 break-words text-t-ui font-text text-fg-primary">{message}</p>
      {action ? (
        <button
          type="button"
          onClick={action.onAction}
          className={cn(
            "inline-flex min-h-hit-min shrink-0 items-center gap-1 rounded-md px-2",
            "text-t-sm font-medium font-text text-accent",
            "transition-colors duration-fast ease-standard hover:bg-surface-hover active:bg-surface-inset",
            "group-data-[preview=hover]:bg-surface-hover group-data-[preview=active]:bg-surface-inset",
            FOCUS_RING,
            FOCUS_PREVIEW,
          )}
        >
          <IconUndo size={14} />
          {action.label}
        </button>
      ) : null}
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => onClose(id)}
        className={cn(
          "inline-flex min-h-hit-min min-w-hit-min shrink-0 items-center justify-center rounded-md text-fg-tertiary",
          "transition-colors duration-fast ease-standard hover:bg-surface-hover hover:text-fg-primary",
          "active:bg-surface-inset group-data-[preview=hover]:bg-surface-hover group-data-[preview=hover]:text-fg-primary group-data-[preview=active]:bg-surface-inset",
          FOCUS_RING,
          FOCUS_PREVIEW,
        )}
      >
        <IconClose size={14} />
      </button>
    </div>
  );
}

export function ToastViewport() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  return (
    <div
      aria-live="polite"
      aria-relevant="additions"
      className="pointer-events-none fixed right-4 bottom-4 z-toast flex max-w-sm flex-col items-end gap-2"
    >
      {toasts.map((item) => (
        <Toast key={item.id} toast={item} onClose={dismiss} />
      ))}
    </div>
  );
}
