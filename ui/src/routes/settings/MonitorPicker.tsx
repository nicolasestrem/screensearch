// Monitor multi-select for `capture.monitors` (`03 §8`: empty selection means
// every monitor). A chip per connected display, pressed while it is captured,
// plus an honest chip for a saved display that is no longer connected so the
// user can drop it (gap `07` #91 — hot-unplug handling — stays out of scope, but
// an unplugged display never silently disappears from the selection).
import type { MonitorInfo } from "../../bindings/MonitorInfo";
import { Skeleton } from "../../components/primitives";
import { cn } from "../../lib/cn";
import { canToggleOff, removeMonitor, toggleMonitor } from "./monitorSelection";

export interface MonitorPickerProps {
  /** The connected displays, or undefined while they are still being read. */
  monitors: readonly MonitorInfo[] | undefined;
  /** Saved indices; empty means "every monitor". */
  selected: readonly number[];
  onChange(next: number[]): void;
  /** Accessible name for the option group (the row prints the visible label). */
  label: string;
  /** The display list is still being read. */
  loading?: boolean;
  /** Plain-language reason the display list could not be read. */
  error?: string | null;
}

const OPTION = cn(
  "inline-flex min-h-hit-min min-w-0 max-w-full items-center gap-1.5 rounded-pill border px-2 py-1.5",
  "text-t-xs transition-colors duration-fast ease-standard motion-reduce:transition-none",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
);

const PRESSED = "border-accent-line bg-accent-wash text-accent hover:border-accent-strong";
const RELEASED =
  "border-line-subtle bg-surface-inset text-fg-secondary hover:border-line-strong hover:text-fg-primary";
const MISSING = "border-status-warn-border bg-status-warn-bg text-status-warn-fg";

export function MonitorPicker({
  monitors,
  selected,
  onChange,
  label,
  loading = false,
  error,
}: MonitorPickerProps) {
  const available = monitors ?? [];
  const availableIndices = available.map((monitor) => monitor.index);
  const listed = monitors !== undefined && !loading;
  const missing = listed
    ? selected.filter((index) => !availableIndices.includes(index))
    : [];
  const summary =
    monitors === undefined
      ? "Reading connected displays…"
      : selected.length === 0
        ? "All monitors — every connected display is captured."
        : availableIndices.length === 0
          ? "None of the saved displays are connected right now."
          : `${selected.length} of ${availableIndices.length} connected displays captured.`;

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div
        role="group"
        aria-label={label}
        className="flex min-w-0 flex-wrap items-center gap-2"
      >
        {monitors === undefined ? (
          <>
            <Skeleton w="24" h="controlSm" />
            <Skeleton w="16" h="controlSm" />
          </>
        ) : null}
        {available.map((monitor) => {
          const pressed = selected.length === 0 || selected.includes(monitor.index);
          // The last captured display cannot be turned off: `[]` means "every display",
          // so that click would start capturing what the user just excluded.
          const canRemove = canToggleOff(selected, monitor.index, availableIndices);
          return (
            <button
              key={monitor.index}
              type="button"
              aria-pressed={pressed}
              disabled={!canRemove}
              aria-disabled={!canRemove}
              title={
                canRemove
                  ? undefined
                  : "The last captured display stays selected — an empty selection means every display."
              }
              onClick={() =>
                onChange(toggleMonitor(selected, monitor.index, availableIndices))
              }
              className={cn(OPTION, pressed ? PRESSED : RELEASED)}
            >
              <span className="min-w-0">
                {monitor.name.trim().length > 0
                  ? monitor.name
                  : `Display ${monitor.index + 1}`}
              </span>
              <span className="font-mono text-t-data text-fg-tertiary">
                {monitor.width}×{monitor.height}
              </span>
              {monitor.is_primary ? (
                <span className="text-fg-tertiary">primary</span>
              ) : null}
            </button>
          );
        })}
        {missing.map((index) => {
          // The same refusal as a connected row: removing the last selection would mean
          // "every display", so it is disabled rather than silently enabling capture.
          const canRemove = canToggleOff(selected, index, availableIndices);
          return (
          <button
            key={index}
            type="button"
            aria-label={`Display ${index + 1} is not connected — remove it from the selection`}
            disabled={!canRemove}
            aria-disabled={!canRemove}
            title={
              canRemove
                ? undefined
                : "The last captured display stays selected — an empty selection means every display."
            }
            onClick={() =>
              onChange(removeMonitor(selected, index, availableIndices))
            }
            className={cn(OPTION, MISSING)}
          >
            Display {index + 1} (not connected)
          </button>
          );
        })}
      </div>
      <p className="min-w-0 text-t-xs text-fg-tertiary">{summary}</p>
      {error ? (
        <p role="status" className="min-w-0 text-t-xs text-status-warn-fg">
          {error}
        </p>
      ) : null}
    </div>
  );
}
