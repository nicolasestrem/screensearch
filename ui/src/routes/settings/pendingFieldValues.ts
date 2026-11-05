// Pending field values shared across mounts (P7 review, PR #161 — P2 "preserve queued
// field edits across category navigation").
//
// `SettingsLayout` unmounts the selected category, so component-local pending state is
// lost when a save is still waiting in the write queue — the user returns to a row that
// shows the older cached value, edits it, and that second edit (composed from the stale
// display) silently removes what the queued save is about to store.
//
// So the pending value lives beside the queue instead of inside a component: the row
// writes it when the commit is issued and clears it when the commit settles, and any
// mount of that field reads it through `useSyncExternalStore` in `useSettingField`.

const values = new Map<string, unknown>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

/** Show `value` for this field until the commit that set it settles, or clear it. */
export function setPendingField(field: string, value: unknown | null): void {
  if (value === null) {
    if (!values.delete(field)) return;
  } else {
    values.set(field, value);
  }
  notify();
}

/** The pending value for a field, or `undefined` when nothing is queued. */
export function pendingFieldValue(field: string): unknown {
  return values.get(field);
}

export function subscribePendingField(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
