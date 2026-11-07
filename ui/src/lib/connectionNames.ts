// Naming the evidence behind a connection (`03 §7h.4`).
//
// A connection is supported by an entity two threads share, and the sharing is computed
// across *every* member session of the thread (`crates/store/src/graph.rs`: the `shared`
// subquery joins `thread_sessions` on both sides). So the entity backing a connection
// frequently belongs to a sibling session and is absent from the opened session's own
// entity list — which is the only list a Session or Moment holds. The panel's contract
// falls back to `Entity <id>` for anything it cannot name, so callers resolve these ids
// with `get_entities` and merge the names in; without that, the rows whose evidence
// matters most are exactly the rows that lose their words.
import type { ConnectedThread } from "../bindings/ConnectedThread";

/** The shared-entity ids a caller cannot name from a session-scoped entity read. */
export function unresolvedSharedEntityIds(
  connections: ConnectedThread[],
  entityNames: Record<number, string>,
): number[] {
  const ids = new Set<number>();
  for (const connection of connections) {
    for (const id of connection.shared_entity_ids) {
      if (!(id in entityNames)) ids.add(id);
    }
  }
  return [...ids].sort((a, b) => a - b);
}
