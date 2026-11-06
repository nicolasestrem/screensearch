// Evidence link - the one place a claim is connected to the row that proves it
// (`03 §7j.2`, grant 5). Every insight row ends in a list of these, so the
// destination mapping lives in exactly one function: `evidenceHref`.
//
//   frame   -> /timeline/:id          the moment itself
//   session -> /timeline/session/:id  one session's timeline
//   thread  -> /work/:id              one work-graph thread
//   entity  -> /work?entity=<id>      the thread list filtered by that entity
//   mark    -> /timeline/:frame_id    the moment the mark flags, when known
//   report  -> /settings/data         where a saved report renders inline
//
// A `mark` without a frame has nowhere to go: marks have no page of their own,
// so it renders as a labelled, explicitly disabled span rather than a dead link
// (`07` C10). `report` has no route either - Settings -> Data is the only place a
// saved report is readable today - so it is labelled with its destination.
import type { EvidenceKind } from "../../bindings/EvidenceKind";
import { Link } from "react-router-dom";
import { cn } from "../../lib/cn";

/**
 * Where an evidence row points. The overloads are the honest signature: only a
 * `mark` can lack a destination, so every other kind is a plain `string` and
 * callers never branch on a `null` that cannot happen.
 */
// A pure mapping the tests and sibling components import directly; this module
// holds no state, so Fast Refresh has nothing to preserve here.
/* eslint-disable react-refresh/only-export-components -- pure destination mapping, no render state */
export function evidenceHref(kind: "mark", refId: number, frameId?: number | null): string | null;
export function evidenceHref(
  kind: Exclude<EvidenceKind, "mark">,
  refId: number,
  frameId?: number | null,
): string;
export function evidenceHref(kind: EvidenceKind, refId: number, frameId?: number | null): string | null;
export function evidenceHref(
  kind: EvidenceKind,
  refId: number,
  frameId?: number | null,
): string | null {
  switch (kind) {
    case "frame":
      return `/timeline/${refId}`;
    case "session":
      return `/timeline/session/${refId}`;
    case "thread":
      return `/work/${refId}`;
    case "entity":
      return `/work?entity=${refId}`;
    case "mark":
      return frameId === undefined || frameId === null ? null : `/timeline/${frameId}`;
    case "report":
      return "/settings/data";
  }
}
/* eslint-enable react-refresh/only-export-components */

/**
 * The short label a row shows when the caller has nothing better. A name beats an
 * id, so callers that know one pass `label` ("Entity: Acme / thread list").
 */
function defaultLabel(kind: EvidenceKind, refId: number, frameId?: number | null): string {
  switch (kind) {
    case "frame":
      return `Moment ${refId}`;
    case "session":
      return `Session ${refId}`;
    case "thread":
      return `Thread ${refId}`;
    case "entity":
      return `Entity ${refId} · thread list`;
    case "mark":
      return frameId === undefined || frameId === null ? "Mark (no page yet)" : `Mark ${refId}`;
    case "report":
      return `Report ${refId} · data settings`;
  }
}

const LINK_CLASS =
  "inline-flex min-h-hit-min items-center rounded-sm text-t-ui text-fg-secondary " +
  "underline decoration-line-subtle underline-offset-2 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:text-fg-primary hover:decoration-line-strong " +
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

export interface EvidenceLinkProps {
  kind: EvidenceKind;
  refId: number;
  /**
   * The moment a `mark` flags (`OpenLoopPayload.frame_id`). Ignored by every
   * other kind; without it a mark has no destination.
   */
  frameId?: number | null;
  /** Replaces the derived label, e.g. with the entity's name. */
  label?: string;
  className?: string;
}

export function EvidenceLink({ kind, refId, frameId, label, className }: EvidenceLinkProps) {
  const href = evidenceHref(kind, refId, frameId);
  const text = label ?? defaultLabel(kind, refId, frameId);

  if (href === null) {
    return (
      <span
        aria-disabled="true"
        title="Marks have no page of their own yet — the moment one flags is the way in."
        className={cn(
          "inline-flex min-h-hit-min items-center text-t-ui text-fg-secondary",
          className,
        )}
      >
        {text}
      </span>
    );
  }

  return (
    <Link to={href} className={cn(LINK_CLASS, className)}>
      {text}
    </Link>
  );
}
