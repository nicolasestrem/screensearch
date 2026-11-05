// Serialized settings writes (P7 review finding, PR #161 — "serialize autosaves
// before building whole-settings snapshots").
//
// `set_settings` takes the WHOLE `Settings` struct, and the mutation is optimistic:
// it writes the submitted object into the cache and refetches on settle. Two
// in-flight writes therefore race — the earlier one's refetch lands after the later
// one's optimistic write and replaces it with the pre-edit snapshot, and the *next*
// commit built from that snapshot silently writes the reverted value back (a lost
// update even though both saves succeeded).
//
// Two rules make that impossible:
//   1. one write at a time (`createSettingsWriteQueue`), so there is never a second
//      optimistic value for a refetch to erase;
//   2. every write is built from a snapshot read *after* the previous write settled
//      and refetched (`freshSettingsSnapshot`), so it can only ever contain values
//      the backend has already accepted.
import type { QueryClient } from "@tanstack/react-query";

import type { Settings } from "../../bindings/Settings";

/** Runs jobs one after another; a failing job never breaks the chain. */
export interface SettingsWriteQueue {
  enqueue<T>(job: () => Promise<T>): Promise<T>;
}

export function createSettingsWriteQueue(): SettingsWriteQueue {
  let tail: Promise<unknown> = Promise.resolve();
  return {
    enqueue<T>(job: () => Promise<T>): Promise<T> {
      const run = tail.then(job, job);
      // The chain must survive a rejected job, otherwise one failed save would wedge
      // every later save behind it.
      tail = run.then(
        () => undefined,
        () => undefined,
      );
      return run;
    },
  };
}

/** The process-wide queue: every settings row in the app shares one write lane. */
export const settingsWriteQueue: SettingsWriteQueue = createSettingsWriteQueue();

/**
 * The snapshot a write must be built from: the persisted settings as the backend
 * has them, after any earlier write in the queue settled.
 *
 * `refetchQueries` resolves when the refetch finishes, so this cannot return the
 * optimistic value a previous mutation left in the cache. Falls back to the cache
 * when the refetch fails; the caller reports the failed save rather than guessing.
 */
export async function freshSettingsSnapshot(
  qc: QueryClient,
  key: readonly unknown[],
): Promise<Settings | undefined> {
  try {
    await qc.refetchQueries({ queryKey: key });
  } catch {
    // A failed refetch is not a failed write; the cache copy is still the best
    // available base, and the mutation's own error path reports a real failure.
  }
  return qc.getQueryData<Settings>(key);
}
