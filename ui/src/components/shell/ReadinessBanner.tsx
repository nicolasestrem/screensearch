// ReadinessBanner — a thin, truthful strip shown only while a subsystem is starting,
// unavailable or errored (UI_REFERENCE §3/§4). It mirrors live readiness; when
// everything resolves it disappears (no dismiss — it is state, not a notice), and
// `disabled` (intentionally off) and `ready` never raise it. The banner is the one
// surface allowed to reserve space (§8); it does so only while a concern exists, so a
// healthy app pays zero height for it.
import { Chip } from "../primitives";
import { useReadiness } from "../../lib/ipc/queries";
import { statusLabel, statusTone } from "../../lib/status";
import { cn } from "../../lib/cn";
import type { ComponentStatus } from "../../bindings/ComponentStatus";

const SUBSYSTEMS = [
  { key: "capture", label: "Capture" },
  { key: "db", label: "Data store" },
  { key: "embed_model", label: "Embeddings" },
  { key: "sidecar", label: "Inference" },
] as const;

/** Statuses worth surfacing in the banner. */
function isConcerning(status: ComponentStatus): boolean {
  return (
    status === "initializing" || status === "unavailable" || status === "error"
  );
}

export function ReadinessBanner() {
  const { data } = useReadiness();
  if (!data) return null;

  const issues = SUBSYSTEMS.map((s) => ({ ...s, cr: data[s.key] })).filter(
    (s) => isConcerning(s.cr.status),
  );
  if (issues.length === 0) return null;

  const anyError = issues.some(
    (s) => s.cr.status === "error" || s.cr.status === "unavailable",
  );

  return (
    <div
      role="status"
      className={cn(
        "flex flex-wrap items-center gap-3",
        "border-b border-line-subtle bg-surface-pane px-pane py-2",
      )}
    >
      <span
        className={cn(
          "text-t-xs font-text font-medium",
          anyError ? "text-status-danger-fg" : "text-status-warn-fg",
        )}
      >
        {anyError ? "Attention" : "Starting up"}
      </span>
      {issues.map((s) => (
        <Chip key={s.key} tone={statusTone(s.cr.status)} dot>
          {s.label}: {statusLabel(s.cr.status)}
          {s.cr.detail ? ` — ${s.cr.detail}` : ""}
        </Chip>
      ))}
    </div>
  );
}
