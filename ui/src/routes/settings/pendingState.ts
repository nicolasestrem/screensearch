// Row commit ownership (P7 review, PR #161 — P2 "preserve newer pending edits when an
// older save settles").
//
// A row can have several commits in flight: the write queue serialises them, but an
// earlier save still settles first. If that earlier save unconditionally clears the
// row's pending value, the row flashes the older persisted value and drops the newer,
// still-pending edit — and a user acting during that flash composes their next edit from
// the wrong list, so the newer value is never written.
//
// So every commit carries the number it was issued with, and only the newest commit may
// change the row's pending/error state. Older settlements are silent: their write still
// happened, it just no longer owns what the row displays.

export interface RowCommitState<V> {
  /** The newest commit number issued for this row. */
  latest: number;
  /** The value the row displays while a commit is in flight, or null. */
  pending: V | null;
  /** The last value whose save failed, with the reason, or null. */
  failure: { value: V; message: string } | null;
}

export function initialRowState<V>(): RowCommitState<V> {
  return { latest: 0, pending: null, failure: null };
}

/** A commit was issued: it becomes the newest owner and clears any stale failure. */
export function begun<V>(value: V, issued: number): RowCommitState<V> {
  return { latest: issued, pending: value, failure: null };
}

/** A commit succeeded; only the newest commit clears the pending value. */
export function settled<V>(
  state: RowCommitState<V>,
  issued: number,
): RowCommitState<V> {
  if (issued !== state.latest) return state;
  return { ...state, pending: null };
}

/** A commit failed; only the newest commit may report it. */
export function failed<V>(
  state: RowCommitState<V>,
  issued: number,
  value: V,
  message: string,
): RowCommitState<V> {
  if (issued !== state.latest) return state;
  return { latest: state.latest, pending: null, failure: { value, message } };
}

/**
 * Retire a draft only when the completed save is the one that owns it: a success that
 * arrives after the user started typing the next value must not erase that value.
 */
export function clearedIfOwned<T extends { issued: number }>(
  entry: T | null,
  issued: number,
): T | null {
  return entry !== null && entry.issued === issued ? null : entry;
}
