// Pending field values shared across mounts (P7 review, PR #161 — P2 "preserve queued
// field edits across category navigation").
//
// `SettingsLayout` unmounts the selected category, so component-local pending state is
// lost when a save is still waiting in the write queue — the user returns to a row that
// shows the older cached value, edits it, and that second edit (composed from the stale
// display) silently removes what the queued save is about to store.
//
// So the pending value lives beside the queue instead of inside a component: the row
// writes it when the commit is issued and the *owning* commit retires it on settlement.
// Retirement is tokenised for the same reason the row state is (row 78): an older save
// settles while a newer edit is queued, and an unconditional clear would erase that
// newer value — leaving the stale display the whole mechanism exists to prevent.

interface PendingEntry {
  value: unknown;
  /** The commit number that put this value here. */
  issued: number;
}

interface FailureEntry {
  /** The value whose save failed, so Retry submits what the user meant. */
  value: unknown;
  message: string;
  issued: number;
}

const entries = new Map<string, PendingEntry>();
// A failure belongs here for the same reason a pending value does: it can arrive after
// the category unmounted, and losing it shows the old saved value with no error and no
// Retry — the submitted edit disappears without explanation.
const failures = new Map<string, FailureEntry>();
const listeners = new Set<() => void>();

// Tokens come from one process-wide counter, not a per-mount one: a remount would
// otherwise start again at 1 and its first edit could re-use the token of a save the
// previous mount still has in flight, letting that older save retire the newer value.
let lastToken = 0;

/** Allocate the ownership token for one commit. Never repeats within a session. */
export function nextPendingToken(): number {
  lastToken += 1;
  return lastToken;
}

function notify(): void {
  for (const listener of listeners) listener();
}

/** Show `value` for this field, owned by commit `issued`. */
export function setPendingField(field: string, value: unknown, issued: number): void {
  entries.set(field, { value, issued });
  notify();
}

/** Retire the pending value, but only if `issued` is still the commit that set it. */
export function clearPendingField(field: string, issued: number): void {
  const entry = entries.get(field);
  if (entry === undefined || entry.issued !== issued) return;
  entries.delete(field);
  notify();
}

/** Record a failed save, owned by commit `issued`. */
export function setPendingFailure(
  field: string,
  value: unknown,
  message: string,
  issued: number,
): void {
  failures.set(field, { value, message, issued });
  notify();
}

/** Retire a recorded failure, but only if `issued` is still the commit that set it. */
export function clearPendingFailure(field: string, issued: number): void {
  const entry = failures.get(field);
  if (entry === undefined || entry.issued !== issued) return;
  failures.delete(field);
  notify();
}

/** The last failure for a field, or `undefined` when the last save succeeded. */
export function pendingFailure(field: string): FailureEntry | undefined {
  return failures.get(field);
}

/** The pending value for a field, or `undefined` when nothing is queued. */
export function pendingFieldValue(field: string): unknown {
  return entries.get(field)?.value;
}

export function subscribePendingField(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
