// SetupIndicator (2026-10-20, `03 §7g`; UI_REFERENCE §4 'Setup indicator' row,
// §5) — a quiet StatusRail chip, present ONLY while an operation is active or
// failed. Zero presence otherwise: the element does not exist, it is not "shown
// as zero" (same discipline as the updater indicator). It summarizes the most
// urgent state (failed > active) and links to Settings, where SetupDetails
// lists each operation. No modal, no toast, no badge count.
//
// A11y: determinate progress is a `role="progressbar"` with values only when
// bytes and total are both known; indeterminate carries `aria-busy` and no
// valuenow, its pulse disabled under prefers-reduced-motion (globals.css). One
// polite live region announces phase changes only — the announcement string is
// derived from operation+phase, never from byte ticks.
import { NavLink } from "react-router-dom";

import { cn } from "../../lib/cn";
import { useSetupStatus } from "../../lib/ipc/queries";
import { setupAnnouncement, summarizeSetup } from "../../lib/setup";

export function SetupIndicator() {
  // Hooks first (Rules of Hooks); zero-presence is a JSX conditional, not an
  // early return before hooks.
  const setup = useSetupStatus();
  const summary = summarizeSetup(setup.data);
  const announcement = setupAnnouncement(setup.data ?? null);

  const failed = summary?.tone === "failed";

  return (
    <>
      {summary != null && !setup.isPending && (
        <NavLink
          to="/settings"
          aria-label={
            failed
              ? "Setup failed — open Settings · Models"
              : `Preparing local AI — open Settings · Models: ${summary.stepText ?? ""}`
          }
          className={cn(
            "flex items-center gap-2 px-3 rounded-chip border",
            "min-h-hit-min whitespace-nowrap font-mono text-data leading-none",
            // Stable width per §8 — summarize text changes inside a reserved
            // minimum, the rail never reflows around it.
            "min-w-40 max-w-80 justify-center",
            "transition-colors duration-fast ease-ui",
            failed
              ? "text-danger border-danger hover:text-ink"
              : "text-accent border-accent hover:border-ink-faint",
          )}
        >
          <span
            className="w-2 h-2 rounded-full bg-current shrink-0"
            aria-hidden="true"
          />
          {failed ? (
            <span>{summary.label}</span>
          ) : (
            <>
              <span>{summary.label}</span>
              <span className="text-ink-muted min-w-0 truncate">{summary.stepText}</span>
            </>
          )}
          {!failed &&
            (summary.progress?.kind === "determinate" ? (
              <span
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={summary.progress.value}
                aria-label="Setup progress"
                className="w-10 shrink-0"
              >
                <span className="block h-0.5 w-full overflow-hidden rounded-chip bg-overlay">
                  <span
                    className="block h-full rounded-chip bg-accent transition-[width] duration-fast ease-ui"
                    style={{ width: `${summary.progress.value}%` }}
                  />
                </span>
              </span>
            ) : (
              <span
                role="progressbar"
                aria-busy="true"
                aria-label="Setup progress (indeterminate)"
                className="w-10 shrink-0"
              >
                <span className="block h-0.5 w-full overflow-hidden rounded-chip bg-overlay">
                  <span className="block h-full w-1/3 animate-pulse rounded-chip bg-accent" />
                </span>
              </span>
            ))}
        </NavLink>
      )}

      {/* The polite live region rides outside the chip so phase changes are
          announced even while the chip holds steady; byte ticks cannot change
          the string, so they never re-announce. */}
      {setup.data != null && (
        <div role="status" aria-live="polite" className="sr-only">
          {announcement}
        </div>
      )}
    </>
  );
}
