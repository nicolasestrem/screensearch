// Pure setup-snapshot logic (03 §7g; UI_REFERENCE §4 "Setup indicator" row). No
// React, no IPC — everything here is unit-testable with the node test runner.
//
// The kernel owns the authoritative SetupStatus and broadcasts the whole snapshot
// on every change; `revision` increases by one per change. A late mount, a webview
// reload, or a lagged event must reconcile to the higher revision — an older
// snapshot NEVER overwrites a newer one. `newerSetupStatus` is the single guard
// used by both write paths: the `setup_changed` event handler and the
// `get_setup_status` queryFn (which merges its fetch with the current cache).
import type { SetupOperation } from "../bindings/SetupOperation";
import type { SetupPhase } from "../bindings/SetupPhase";
import type { SetupStep } from "../bindings/SetupStep";
import type { SetupStatus } from "../bindings/SetupStatus";

/** Byte formatter — one copy for the setup surfaces (same figures as the rail). */
export function setupFormatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/**
 * Should `candidate` replace `current`? True when `current` is empty (first
 * arrival) or `candidate.revision` is strictly higher. Equal revisions never
 * replace (idempotent), lower revisions are discarded.
 */
export function newerSetupStatus(
  candidate: SetupStatus | null | undefined,
  current: SetupStatus | null | undefined,
): boolean {
  return (
    candidate != null &&
    (current == null || candidate.revision > current.revision)
  );
}

/** Phases that mean real work is happening right now. */
const ACTIVE_PHASES: SetupPhase[] = [
  "checking",
  "downloading",
  "verifying",
  "installing",
  "preparing",
];

/** Human names for the four operations (Labels, not lane jargon). */
export const SETUP_OPERATION_LABELS: Record<SetupOperation, string> = {
  engine: "Engine",
  embeddings: "Search model",
  vision_model: "Vision model",
  answer_model: "Answer model",
};

/** Stage copy for a details row (the per-operation stage line, §4). */
export function setupStageText(step: SetupStep): string {
  switch (step.phase) {
    case "checking":
      return "Checking";
    case "downloading":
      return "Downloading";
    case "verifying":
      return "Verifying";
    case "installing":
      return "Installing";
    case "preparing":
      return "Preparing";
    case "done":
      return "Done";
    case "failed":
      return "Failed";
  }
}

/** Done copy per operation — what a finished stage means for the user. */
export function setupDoneText(operation: SetupOperation): string {
  switch (operation) {
    case "engine":
      return "Engine available";
    case "embeddings":
      return "Search model ready";
    case "vision_model":
    case "answer_model":
      return "Downloaded";
  }
}

/** Next-step copy under a failed stage. Engine/embeddings need an app restart;
 *  model lanes keep the existing Load action (`ModelPanel`). Never "offline". */
export function setupFailureNextStep(operation: SetupOperation): string {
  switch (operation) {
    case "engine":
    case "embeddings":
      return "Resolve the problem, then restart ScreenSearch.";
    case "vision_model":
    case "answer_model":
      return "Use Load to try again — the download resumes where it stopped.";
  }
}

/** The chip's active step line. Percent shown ONLY while downloading with both
 *  byte counts known (floor, clamped 0..100) — never for embeddings (preparing). */
export function setupChipStepText(step: SetupStep): string {
  const label = SETUP_OPERATION_LABELS[step.operation];
  const pct = setupDownloadPercent(step);
  switch (step.operation) {
    case "engine":
      switch (step.phase) {
        case "checking":
          return "Checking engine";
        case "downloading":
          return pct !== null ? `Downloading engine ${pct}%` : "Downloading engine";
        case "verifying":
          return "Verifying engine";
        case "installing":
          return "Installing engine";
        default:
          return `${setupStageText(step)} engine`;
      }
    case "embeddings":
      return "Preparing search model";
    case "vision_model":
    case "answer_model":
      return pct !== null
        ? `Downloading ${label.toLowerCase()} ${pct}%`
        : `Downloading ${label.toLowerCase()}`;
  }
}

/**
 * Download percentage for a step, or `null` when indeterminate. Only
 * `downloading` with known received AND total bytes (> 0) yields a number;
 * anything else (embeddings preparing, unknown length, non-download phases) is
 * indeterminate per spec — never an invented percentage.
 */
export function setupDownloadPercent(step: SetupStep): number | null {
  if (step.phase !== "downloading") return null;
  const { downloaded_bytes: got, total_bytes: total } = step;
  if (got == null || total == null || total <= 0) return null;
  return Math.max(0, Math.min(100, Math.floor((got / total) * 100)));
}

/** The friendly bytes line for a downloading step ('4.8 MB of 32 MB', or a
 *  bare '4.8 MB' when the length is unknown). Empty outside downloading. */
export function setupBytesText(
  step: SetupStep,
  formatBytes: (bytes: number) => string,
): string {
  if (step.phase !== "downloading") return "";
  const { downloaded_bytes: got, total_bytes: total } = step;
  if (got == null) return "";
  return total != null && total > 0
    ? `${formatBytes(got)} of ${formatBytes(total)}`
    : formatBytes(got);
}

/** One are-you-informed summary of the whole snapshot, or `null` for zero
 *  presence (nothing active or failed). Failed outranks active; among active
 *  steps a determinate (byte-counting) one wins so the chip can show progress. */
export type SetupSummary = {
  tone: "active" | "failed";
  /** Chip headline: 'Preparing local AI' / 'Setup failed'. */
  label: string;
  /** The active step line ('Downloading engine 42%'), null when failed. */
  stepText: string | null;
  progress:
    | { kind: "determinate"; value: number }
    | { kind: "indeterminate" }
    | null;
  operation: SetupOperation;
};

export function summarizeSetup(
  status: SetupStatus | null | undefined,
): SetupSummary | null {
  if (status == null) return null;
  const failed = status.steps.filter((s) => s.phase === "failed");
  const active = status.steps.filter((s) => ACTIVE_PHASES.includes(s.phase));

  if (failed.length > 0) {
    // The first failed step in declaration order is the headline; details list
    // every operation independently anyway.
    return {
      tone: "failed",
      label: "Setup failed",
      stepText: null,
      progress: null,
      operation: failed[0].operation,
    };
  }

  if (active.length === 0) return null;

  const determinate = active.find(
    (s) => setupDownloadPercent(s) !== null,
  );
  const step = determinate ?? active[0];
  return {
    tone: "active",
    label: "Preparing local AI",
    stepText: setupChipStepText(step),
    progress:
      setupDownloadPercent(step) !== null
        ? {
            kind: "determinate",
            value: setupDownloadPercent(step) as number,
          }
        : { kind: "indeterminate" },
    operation: step.operation,
  };
}

/**
 * A plain, visible cause line for a failed step (`UI_REFERENCE §4`: cause line +
 * next step + a separately expandable diagnostic). Derived only from what the
 * error actually says — never an inferred "offline": an HTTP status names the
 * server's answer, a digest mismatch says so, otherwise the innermost error in
 * the anyhow chain (`outer: inner: innermost`), capped so it stays one line.
 * Empty when there is no error text.
 */
export function setupFailureCause(error: string | null | undefined): string {
  const text = (error ?? "").trim();
  if (text === "") return "";
  const http = /HTTP (\d{3}(?: [A-Za-z][A-Za-z ]*[A-Za-z])?)/.exec(text);
  if (http) return `The download server answered HTTP ${http[1]}.`;
  if (/sha256 mismatch/i.test(text)) {
    return "The download didn’t match its published checksum, so it wasn’t installed.";
  }
  const segments = text.split(": ").map((s) => s.trim()).filter((s) => s !== "");
  const innermost = segments[segments.length - 1] ?? text;
  const capped = innermost.length > 160 ? `${innermost.slice(0, 157)}…` : innermost;
  return capped.charAt(0).toUpperCase() + capped.slice(1);
}

/**
 * The rows Settings · Models lists (`UI_REFERENCE §4`): every step while any
 * operation is active or failed (completed rows stay for context), and nothing
 * once everything has finished — zero presence when idle.
 */
export function setupDetailsSteps(status: SetupStatus | null | undefined): SetupStep[] {
  if (status == null) return [];
  return summarizeSetup(status) === null ? [] : status.steps;
}

/**
 * The live-region announcement. Deliberately derived ONLY from operation + phase
 * (plus presence), never from byte counts — so a stream of progress ticks that
 * only moves bytes does not re-announce anything (§4: phase changes only).
 */
export function setupAnnouncement(status: SetupStatus | null): string {
  // An empty snapshot means nothing has reported yet (cold start, or no kernel) —
  // that is not "complete", so stay silent.
  if (status == null || status.steps.length === 0) return "";
  const parts = status.steps
    .filter(
      (s) => s.phase === "failed" || ACTIVE_PHASES.includes(s.phase),
    )
    .map((s) => `${SETUP_OPERATION_LABELS[s.operation]} ${setupStageText(s).toLowerCase()}`);
  if (parts.length === 0) return "Local AI setup complete";
  return parts.join("; ");
}
