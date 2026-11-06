// Connected threads (`03 §7h.4`, grant 3): the threads that share at least one
// entity with the thread being read. Fetches nothing - the Thread page owns the
// read - and never asserts a relationship the rows cannot prove: the shared
// entity names ARE the evidence, so they are always visible in the row.
//
// Five states, and the loading one reserves the final geometry (`--row-h`), so a
// populated panel does not jump when the rows arrive.
import type { ConnectedThread } from "../../bindings/ConnectedThread";
import type { ReactNode } from "react";
import { EmptyState, ErrorState, ListRow, Panel } from "../primitives";
import { relativeTime } from "../../lib/time";
import { evidenceHref } from "./evidenceLink";

export type ConnectionsState = "loading" | "empty" | "error" | "partial" | "populated";

/**
 * What each state says, in one table so a state can never exist without copy.
 * `populated`'s title is the panel heading every state carries; the others are
 * the state's own line (announced, or shown in place of the rows).
 */
// The table is this component's own data, not a sibling module's: Fast Refresh
// reloads component-only files, and the panel holds no state to lose.
// eslint-disable-next-line react-refresh/only-export-components
export const CONNECTION_STATE_COPY: Record<
  ConnectionsState,
  { title: string; detail: string | null }
> = {
  loading: { title: "Connecting threads", detail: null },
  empty: {
    title: "No connected threads yet",
    detail: "They appear when threads share an entity.",
  },
  error: {
    title: "Couldn't load connected threads",
    detail: "Retry to check for threads that share an entity.",
  },
  partial: { title: "Some connections still clustering", detail: null },
  populated: { title: "Connected threads", detail: null },
};

export interface ConnectionsPanelProps {
  connections: ConnectedThread[];
  /** Entity id -> name, from the caller's entity read; unknown ids fall back to `Entity <id>`. */
  entityNames: Record<number, string>;
  state: ConnectionsState;
  onRetry?: () => void;
  className?: string;
}

/** The shared entities as one line: that line is the proof of the connection. */
function sharedEntityNames(
  connection: ConnectedThread,
  entityNames: Record<number, string>,
): string {
  const names = connection.shared_entity_ids.map((id) => entityNames[id] ?? `Entity ${id}`);
  return names.length === 0 ? "No shared entity recorded" : names.join(" · ");
}

export function ConnectionsPanel({
  connections,
  entityNames,
  state,
  onRetry,
  className,
}: ConnectionsPanelProps) {
  const copy = CONNECTION_STATE_COPY[state];
  const rows = connections.map((connection) => (
    <li key={connection.thread.id}>
      <ListRow
        as="a"
        href={evidenceHref("thread", connection.thread.id)}
        title={connection.thread.title}
        meta={sharedEntityNames(connection, entityNames)}
        trailing={relativeTime(connection.thread.last_seen_at)}
      />
    </li>
  ));

  let body: ReactNode;
  if (state === "loading") {
    body = (
      <>
        <p role="status" className="sr-only">
          {copy.title}
        </p>
        <ul className="m-0 list-none p-0">
          {[0, 1, 2].map((row) => (
            <li key={row}>
              <ListRow state="loading" />
            </li>
          ))}
        </ul>
      </>
    );
  } else if (state === "error") {
    body = (
      <ErrorState
        align="start"
        title={copy.title}
        message={copy.detail}
        onRetry={onRetry}
      />
    );
  } else {
    body = (
      <>
        {state === "partial" && (
          // Labelled, not folded into the rows: the rows that arrived are real,
          // the clustering pass simply has not finished — and that is also why
          // an empty list can still be worth waiting on.
          <p className="m-0 border-b border-line-subtle px-pane py-2 text-t-xs text-fg-secondary">
            {copy.title}
          </p>
        )}
        {connections.length === 0 ? (
          <EmptyState
            align="start"
            title={CONNECTION_STATE_COPY.empty.title}
            description={CONNECTION_STATE_COPY.empty.detail}
          />
        ) : (
          <ul className="m-0 list-none p-0">{rows}</ul>
        )}
      </>
    );
  }

  return (
    <Panel title={CONNECTION_STATE_COPY.populated.title} flush className={className}>
      {body}
    </Panel>
  );
}
