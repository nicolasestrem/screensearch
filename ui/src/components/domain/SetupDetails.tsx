// SetupDetails (2026-10-20, `03 §7g`; UI_REFERENCE §4 'Setup indicator' row,
// §5) — the Settings · Models setup-operations list: one row per operation in
// the snapshot (step / stage / bytes), a determinate or indeterminate bar while
// work runs, done copy when an operation completes, and for a failure a plain
// cause line, the per-operation next step, and an expandable diagnostic block.
//
// States (§4): loading = skeleton line (chip shows nothing, zero rail presence);
// empty (nothing active or failed / no snapshot) = render nothing; error =
// inline explain + retry (refetch); populated = one row per operation.
// Tokens only, one scroll context (this list wraps, it never scrolls).
import { Skeleton } from "../primitives";
import { useSetupStatus } from "../../lib/ipc/queries";
import {
  SETUP_OPERATION_LABELS,
  setupDetailsSteps,
  setupFailureCause,
  setupBytesText,
  setupDoneText,
  setupDownloadPercent,
  setupFailureNextStep,
  setupFormatBytes,
  setupStageText,
} from "../../lib/setup";
import type { SetupStep } from "../../bindings/SetupStep";

function SetupRow({ step }: { step: SetupStep }) {
  const label = SETUP_OPERATION_LABELS[step.operation];
  const pct = setupDownloadPercent(step);
  const bytes = setupBytesText(step, setupFormatBytes);

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <span className="text-body text-ink font-body">{label}</span>
        <span className="font-mono text-data text-ink-muted whitespace-nowrap">
          {bytes}
        </span>
      </div>
      <span className="text-caption text-ink-muted font-body">
        {setupStageText(step)}
      </span>

      {pct !== null ? (
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
          aria-label={`${label} setup progress`}
          className="h-1.5 w-full overflow-hidden rounded-chip bg-overlay"
        >
          <div
            className="h-full rounded-chip bg-accent transition-[width] duration-fast ease-ui"
            style={{ width: `${pct}%` }}
          />
        </div>
      ) : step.phase === "downloading" || step.phase === "preparing" ? (
        // Indeterminate — no percentage is invented (fastembed exposes no byte
        // counter; the length may be unknown). The pulse is reduced-motion-safe.
        <div
          role="progressbar"
          aria-busy="true"
          aria-label={`${label} setup progress (indeterminate)`}
          className="h-1.5 w-full overflow-hidden rounded-chip bg-overlay"
        >
          <div className="h-full w-1/3 animate-pulse rounded-chip bg-accent" />
        </div>
      ) : null}

      {step.phase === "done" && (
        <span className="text-caption text-ok font-body">
          {setupDoneText(step.operation)}
        </span>
      )}

      {step.phase === "failed" && (
        <div
          role="alert"
          className="flex flex-col gap-1 rounded-panel border border-danger bg-overlay px-3 py-2"
        >
          <span className="text-caption text-danger font-body">
            {label} didn’t finish.
          </span>
          {step.error && (
            <span className="text-caption text-ink font-body">
              {setupFailureCause(step.error)}
            </span>
          )}
          <span className="text-caption text-ink-muted font-body">
            {setupFailureNextStep(step.operation)}
          </span>
          {step.error && (
            <details className="text-caption text-ink-faint font-body">
              <summary className="cursor-pointer select-none hover:text-ink-muted">
                Technical details
              </summary>
              <pre className="mt-1 overflow-x-auto whitespace-pre font-mono text-data text-ink-faint">
                {step.error}
              </pre>
            </details>
          )}
        </div>
      )}
    </div>
  );
}

export function SetupDetails() {
  // Hooks first (Rules of Hooks); every branch below is a JSX conditional.
  const setup = useSetupStatus();

  if (setup.isPending) {
    // Skeleton matching the final layout (§8: skeletons reserve space).
    return (
      <div className="flex flex-col gap-4">
        <Skeleton className="h-6 w-40" />
        <Skeleton className="h-1.5 w-full" />
      </div>
    );
  }

  if (setup.isError) {
    return (
      <div
        role="alert"
        className="flex flex-col gap-1 rounded-panel border border-danger bg-overlay px-3 py-2"
      >
        <span className="text-caption text-danger font-body">
          Couldn’t read the setup status. Nothing was lost — closing and reopening
          Settings retries.
        </span>
        <span className="text-caption text-ink-muted font-body">
          {String(setup.error)}
        </span>
        <button
          type="button"
          onClick={() => setup.refetch()}
          className="self-start text-caption font-body text-ink underline underline-offset-2 hover:text-ink-muted"
        >
          Try again
        </button>
      </div>
    );
  }

  // Nothing active or failed → details list nothing (§4: zero presence when
  // idle, even after every operation has completed).
  const steps = setupDetailsSteps(setup.data);
  if (steps.length === 0) return null;

  return (
    <div className="flex flex-col gap-4" aria-label="Setup operations">
      {steps.map((step) => (
        <SetupRow key={step.operation} step={step} />
      ))}
    </div>
  );
}
