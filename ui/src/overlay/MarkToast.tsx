// The mark-this-moment confirmation, shown in the overlay window after the mark hotkey
// fires (0.3.0 PR6; `03 §7b`, D1/D8). It appears WITHOUT stealing focus (the shell shows
// the window non-focusable), so the user keeps typing in their app; clicking the note
// field focuses the overlay (focusOverlayForNote) so the note can be typed. Ignoring it
// costs nothing — it auto-dismisses. Tokens only; role="status" for a polite announce.
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";

import { IconMark } from "../components/icons";
import { cn } from "../lib/cn";
import {
  dismissMarkToast,
  focusOverlayForNote,
  setMarkNote,
} from "../lib/ipc/commands";
import type { MarkToast as MarkToastPayload } from "../bindings/MarkToast";

/** Auto-dismiss delay for the mark toast (a UX constant, like SEARCH_DEBOUNCE_MS —
 *  the durations users care about are settings, this is just responsiveness). */
const MARK_TOAST_DISMISS_MS = 6000;

interface MarkToastProps {
  payload: MarkToastPayload;
  onDone: () => void;
}

export function MarkToast({ payload, onDone }: MarkToastProps) {
  const [note, setNote] = useState("");
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const success = payload.level === "success" && payload.mark_id != null;

  const dismiss = useCallback(() => {
    void dismissMarkToast().catch(() => undefined);
    onDone();
  }, [onDone]);

  const saveAndDismiss = () => {
    const trimmed = note.trim();
    if (success && trimmed.length > 0 && payload.mark_id != null) {
      void setMarkNote(payload.mark_id, trimmed).catch(() => undefined);
    }
    dismiss();
  };

  // Auto-dismiss, paused while the user is engaging the note field (focused or typing) so
  // a note in progress is never cut off. Ignoring it lets it fade after the delay.
  const paused = focused || note.length > 0;
  useEffect(() => {
    if (paused) return;
    const t = setTimeout(dismiss, MARK_TOAST_DISMISS_MS);
    return () => clearTimeout(t);
  }, [paused, dismiss]);

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveAndDismiss();
    } else if (e.key === "Escape") {
      e.preventDefault();
      dismiss();
    }
  };

  return (
    <div className="flex h-full items-start justify-center p-3">
      <section
        role="status"
        aria-live="polite"
        className="w-full overflow-hidden rounded-md border border-line-subtle bg-surface-raised shadow-toast"
      >
        <div className="flex items-center gap-3 p-3">
          <span
            className={cn(
              "grid h-9 w-9 shrink-0 place-items-center rounded-sm border",
              success
                ? "border-status-ok-border bg-status-ok-bg text-status-ok-fg"
                : "border-status-warn-border bg-status-warn-bg text-status-warn-fg",
            )}
            aria-hidden="true"
          >
            <IconMark size={18} />
          </span>
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <span className="text-t-body text-fg-primary">
              {success ? "Marked ✓" : payload.message}
            </span>
            {success && (
              <input
                ref={inputRef}
                value={note}
                onChange={(e) => setNote(e.currentTarget.value)}
                onKeyDown={handleKeyDown}
                onPointerDown={() =>
                  void focusOverlayForNote().catch(() => undefined)
                }
                onFocus={() => {
                  setFocused(true);
                  void focusOverlayForNote().catch(() => undefined);
                }}
                onBlur={() => setFocused(false)}
                placeholder="Add a note (optional) — Enter to save"
                aria-label="Mark note"
                className={cn(
                  "min-w-0 rounded-md border border-line-control bg-surface-inset px-3 min-h-hit-min",
                  "text-t-body text-fg-primary placeholder:text-fg-tertiary caret-accent",
                  "transition-colors duration-fast ease-standard motion-reduce:transition-none",
                  "hover:border-line-strong",
                  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
                )}
              />
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
