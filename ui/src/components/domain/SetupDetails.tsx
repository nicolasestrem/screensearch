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
        <span className="text-t-body text-fg-primary">{label}</span>
        <span className="font-mono text-t-data text-fg-secondary whitespace-nowrap">
          {bytes}
        </span>
      </div>
      <span className="text-t-xs text-fg-secondary">
        {setupStageText(step)}
      </span>

      {pct !== null ? (
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
          aria-label={`${label} setup progress`}
          className="h-1.5 w-full overflow-hidden rounded-sm bg-surface-raised"
        >
          <div
            className="h-full rounded-sm bg-accent transition-[width] duration-fast ease-standard motion-reduce:transition-none"
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
          className="h-1.5 w-full overflow-hidden rounded-sm bg-surface-raised"
        >
          <div className="h-full w-1/3 animate-pulse rounded-sm bg-accent motion-reduce:animate-none" />
        </div>
      ) : null}

      {step.phase === "done" && (
        <span className="text-t-xs text-status-ok-fg">
          {setupDoneText(step.operation)}
        </span>
      )}

      {step.phase === "failed" && (
        <div
          role="alert"
          className="flex flex-col gap-1 rounded-md border border-status-danger-border bg-surface-raised px-3 py-2"
        >
          <span className="text-t-xs text-status-danger-fg">
            {label} didn’t finish.
          </span>
          {step.error && (
            <span className="text-t-xs text-fg-primary">
              {setupFailureCause(step.error)}
            </span>
          )}
          <span className="text-t-xs text-fg-secondary">
            {setupFailureNextStep(step.operation)}
          </span>
          {step.error && (
            <details className="text-t-xs text-fg-tertiary">
              <summary className="cursor-pointer select-none hover:text-fg-secondary">
                Technical details
              </summary>
              <pre className="mt-1 overflow-x-auto whitespace-pre font-mono text-t-data text-fg-tertiary">
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
        className="flex flex-col gap-1 rounded-md border border-status-danger-border bg-surface-raised px-3 py-2"
      >
        <span className="text-t-xs text-status-danger-fg">
          Couldn’t read the setup status. Nothing was lost — closing and reopening
          Settings retries.
        </span>
        <span className="text-t-xs text-fg-secondary">
          {String(setup.error)}
        </span>
        <button
          type="button"
          onClick={() => setup.refetch()}
          className="self-start text-t-xs text-fg-primary underline underline-offset-2 hover:text-fg-secondary"
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
