// Insight card (`03 §7j`) - one typed statement, rendered from its payload.
//
// The payload is the truth and the model is never it: every figure below comes
// from a typed field, so the card reads the same whether the row was generated
// heuristically or phrased by a model. Phrasing, when a caller has some, is an
// additive paragraph in the secondary type colour - it can never replace, reorder
// or reword a fact row. `stale` rows keep their facts and say so in the header;
// nothing about a stale row is styled as current.
//
// `0.4.5 D13`: no badge, no score, no grade. `similarity` is a payload figure and
// renders as a plain number beside the neighbour it belongs to, never as a rank.
import type { Insight } from "../../bindings/Insight";
import type { InsightKind } from "../../bindings/InsightKind";
import type { ReactNode } from "react";
import { Chip, Panel } from "../primitives";
import { absoluteTime, relativeTime } from "../../lib/time";
import { EvidenceLink } from "./evidenceLink";
import { formatCount, formatDuration, formatGap } from "./format";

/** The plain-language name of each kind. The tokens stay internal. */
const KIND_LABEL: Record<InsightKind, string> = {
  thread_activity: "Thread activity",
  open_loop: "Open loop",
  recurrence: "Seen again",
  semantic_neighbour: "Similar sessions",
  resumption: "Picked up again",
};

interface Fact {
  label: string;
  value: ReactNode;
}

/** A moment: relative in place, absolute (and machine-readable) on hover. */
function Time({ ms }: { ms: number }) {
  return (
    <time dateTime={new Date(ms).toISOString()} title={absoluteTime(ms)}>
      {relativeTime(ms)}
    </time>
  );
}

/** The typed rows: a label/value pair per field the payload actually carries. */
function Facts({ rows }: { rows: Fact[] }) {
  return (
    <dl className="m-0 grid grid-cols-[auto_1fr] items-baseline gap-x-3 gap-y-1 text-t-ui">
      {rows.map((row) => (
        <div key={row.label} className="contents">
          <dt className="text-fg-secondary">{row.label}</dt>
          <dd className="m-0 min-w-0 break-words text-fg-primary">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function PayloadBody({
  insight,
  entityNames,
}: {
  insight: Insight;
  entityNames: Record<number, string>;
}) {
  const payload = insight.payload;

  switch (payload.kind) {
    case "thread_activity":
      return (
        <>
          <p className="m-0 text-t-ui font-medium text-fg-primary">{payload.title}</p>
          <Facts
            rows={[
              { label: "Sessions", value: formatCount(payload.session_count, "session") },
              { label: "Time spent", value: formatDuration(payload.active_ms) },
              { label: "First touched", value: <Time ms={payload.first_touched_at} /> },
              { label: "Last touched", value: <Time ms={payload.last_touched_at} /> },
            ]}
          />
        </>
      );

    case "open_loop": {
      const thread =
        payload.thread_id === null ? null : (payload.title ?? `Thread ${payload.thread_id}`);
      // The note is what the person wrote, so it leads when it exists; the thread
      // title does the same job for an abandoned thread.
      const subject =
        payload.note ??
        thread ??
        (payload.source === "unresolved_mark" ? "A mark you left open" : "A thread you left open");
      const rows: Fact[] = [
        {
          label: "Source",
          value: payload.source === "unresolved_mark" ? "Unresolved mark" : "Thread left open",
        },
      ];
      if (payload.mark_id !== null) rows.push({ label: "Mark", value: `Mark ${payload.mark_id}` });
      // The thread title is already the subject when there is no note.
      if (payload.note !== null && thread !== null) rows.push({ label: "Thread", value: thread });
      rows.push({ label: "Opened", value: <Time ms={payload.opened_at} /> });
      rows.push({ label: "Open for", value: formatDuration(payload.idle_ms) });
      return (
        <>
          <p className="m-0 text-t-ui font-medium text-fg-primary">{subject}</p>
          <Facts rows={rows} />
        </>
      );
    }

    case "recurrence":
      return (
        <>
          <p className="m-0 text-t-ui font-medium text-fg-primary">
            {entityNames[payload.entity_id] ?? payload.entity_name}
          </p>
          <Facts
            rows={[
              { label: "Sessions", value: formatCount(payload.occurrence_count, "session") },
              { label: "Seen again after", value: formatGap(payload.gap_ms) },
              { label: "First seen", value: <Time ms={payload.first_seen_at} /> },
              { label: "Last seen", value: <Time ms={payload.second_seen_at} /> },
            ]}
          />
        </>
      );

    case "semantic_neighbour":
      return (
        <>
          <p className="m-0 text-t-ui font-medium text-fg-primary">
            {payload.session_title ?? `Session ${payload.session_id}`}
          </p>
          <Facts rows={[{ label: "Started", value: <Time ms={payload.session_started_at} /> }]} />
          <p className="m-0 text-t-xs text-fg-secondary">Other sessions like this one</p>
          <ul className="m-0 grid list-none gap-1 p-0">
            {payload.neighbours.map((neighbour) => (
              <li
                key={neighbour.session_id}
                className="flex min-w-0 items-baseline justify-between gap-3"
              >
                <span className="min-w-0 truncate text-t-ui text-fg-primary">
                  {neighbour.title ?? `Session ${neighbour.session_id}`}
                </span>
                <span className="shrink-0 text-t-xs text-fg-secondary">
                  {relativeTime(neighbour.started_at)} · similarity{" "}
                  {neighbour.similarity.toFixed(2)}
                </span>
              </li>
            ))}
          </ul>
        </>
      );

    case "resumption": {
      const rows: Fact[] = [
        { label: "Picked up after", value: formatGap(payload.gap_ms) },
        { label: "Previous session ended", value: <Time ms={payload.previous_ended_at} /> },
        { label: "Resumed", value: <Time ms={payload.resumed_at} /> },
      ];
      if (payload.last_doing !== null) {
        rows.push({ label: "Last doing", value: payload.last_doing });
      }
      return (
        <>
          <p className="m-0 text-t-ui font-medium text-fg-primary">{payload.title}</p>
          <Facts rows={rows} />
        </>
      );
    }
  }
}

export interface InsightCardProps {
  insight: Insight;
  /** Entity id -> name, so a recurrence names the entity instead of echoing the payload. */
  entityNames?: Record<number, string>;
  /**
   * Restated wording for an `llm_phrased` insight. Additive only: it renders under
   * the typed rows, never instead of them. The wire type carries no phrasing field
   * yet (`Insight` has `generator` alone), so a caller that receives one passes it
   * here; `heuristic` rows never show it.
   */
  phrasing?: string | null;
  className?: string;
}

export function InsightCard({ insight, entityNames = {}, phrasing, className }: InsightCardProps) {
  // `evidence` is stored ordered by ordinal; sort anyway so a caller that merged
  // two lists still renders the authored order.
  const evidence = [...insight.evidence].sort((a, b) => a.ordinal - b.ordinal);
  // Only a mark can need its frame to resolve; every other kind uses its own ref.
  const markFrameId = insight.payload.kind === "open_loop" ? insight.payload.frame_id : null;

  return (
    <Panel
      title={KIND_LABEL[insight.kind]}
      action={insight.stale ? <Chip tone="warn">May be out of date</Chip> : undefined}
      className={className}
    >
      <div className="grid gap-3">
        <div className="grid gap-2">
          <PayloadBody insight={insight} entityNames={entityNames} />
        </div>

        {insight.generator === "llm_phrased" && phrasing ? (
          <p className="m-0 border-s-2 border-line-subtle ps-3 text-t-ui text-fg-secondary">
            {phrasing}
          </p>
        ) : null}

        {evidence.length > 0 && (
          <div className="grid gap-1">
            <h3 className="m-0 text-t-xs text-fg-secondary">Evidence</h3>
            <ul className="m-0 flex list-none flex-wrap items-center gap-x-3 gap-y-1 p-0">
              {evidence.map((item) => (
                <li key={`${item.kind}-${item.ref_id}`} className="min-w-0">
                  <EvidenceLink kind={item.kind} refId={item.ref_id} frameId={markFrameId} />
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Panel>
  );
}
