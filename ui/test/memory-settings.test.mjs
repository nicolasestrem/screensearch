// Saving memory settings can move the output folder, which changes the project
// note the promotion lane targets. `ProjectNoteStatus` is keyed by a constant, so
// a mounted reader only refreshes when the save explicitly invalidates it. These
// tests drive the real QueryClient/QueryObserver/MutationObserver pairing (no DOM
// harness) and prove the refetch happens before the save reports success.
import assert from "node:assert/strict";
import { register } from "node:module";
import { setTimeout as delay } from "node:timers/promises";
import test from "node:test";
import { MutationObserver, QueryClient, QueryObserver } from "@tanstack/react-query";

register("./ui-loader.mjs", import.meta.url);
const { memorySettingsMutationOptions } = await import("../src/lib/ipc/memory.ts");
const { promotionKeys } = await import("../src/lib/ipc/promotion.ts");

const OLD_NOTE = { path: "C:/Notes/ScreenSearch/Projects/project-alpha.md", exists: true, count: 1 };
const NEW_NOTE = { path: "D:/Notes/ScreenSearch/Projects/project-beta.md", exists: true, count: 1 };
const SETTINGS = {
  enabled: false,
  local_time: "08:00",
  timezone: "Europe/Paris",
  output_folder: "C:/Notes",
  telegram_enabled: false,
  telegram_chat_id: null,
};
const status = (settings) => ({
  settings,
  source_id: "source-1",
  latest_run: null,
  missed_windows: [],
  worker_running: false,
  worker_error: null,
  telegram_credential_available: false,
});

async function waitFor(predicate, label) {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (predicate()) return;
    await delay(5);
  }
  assert.fail(`timed out waiting for ${label}`);
}

function deferred() {
  let resolve;
  const promise = new Promise((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

test("saving settings refetches the mounted note status and stays pending until it lands", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  let fetches = 0;
  let next = null;
  const note = new QueryObserver(client, {
    queryKey: promotionKeys.noteStatus,
    queryFn: async () => {
      fetches += 1;
      return next ? next.promise : OLD_NOTE;
    },
  });
  const unsubscribe = note.subscribe(() => {});
  await waitFor(() => note.getCurrentResult().isSuccess, "the first note status");
  assert.equal(note.getCurrentResult().data, OLD_NOTE);

  // The folder moves; hold the refetch open so the save's pending state is visible.
  next = deferred();
  let settled = false;
  const save = new MutationObserver(
    client,
    memorySettingsMutationOptions(client, async (settings) => {
      assert.equal(settings.output_folder, "D:/Notes");
      return status(settings);
    }),
  );
  const saving = save.mutate({ ...SETTINGS, output_folder: "D:/Notes" }).then(() => {
    settled = true;
  });

  await waitFor(() => fetches === 2, "the note-status refetch");
  // The reader still holds the old folder's note until the refetch resolves, and
  // the mutation is not allowed to report success in the meantime.
  assert.equal(note.getCurrentResult().data, OLD_NOTE);
  assert.equal(note.getCurrentResult().isFetching, true);
  assert.equal(save.getCurrentResult().isPending, true);
  assert.equal(settled, false);

  next.resolve(NEW_NOTE);
  await saving;
  assert.equal(note.getCurrentResult().data, NEW_NOTE);
  assert.equal(note.getCurrentResult().isSuccess, true);
  unsubscribe();
  client.clear();
});

test("saving settings clears a previous note-status error without a manual retry", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  let fetches = 0;
  const note = new QueryObserver(client, {
    queryKey: promotionKeys.noteStatus,
    queryFn: async () => {
      fetches += 1;
      // First run: no output folder, so the status read fails. Saving one must
      // refetch and replace the error, exactly like the settings section.
      if (fetches === 1) throw new Error("Select an output folder first.");
      return NEW_NOTE;
    },
  });
  const unsubscribe = note.subscribe(() => {});
  await waitFor(() => note.getCurrentResult().isError, "the first note-status error");
  assert.equal(note.getCurrentResult().error.message, "Select an output folder first.");

  const save = new MutationObserver(
    client,
    memorySettingsMutationOptions(client, async (settings) => status(settings)),
  );
  await save.mutate({ ...SETTINGS, output_folder: "D:/Notes" });

  assert.equal(fetches, 2);
  assert.equal(note.getCurrentResult().isSuccess, true);
  assert.equal(note.getCurrentResult().error, null);
  assert.equal(note.getCurrentResult().data, NEW_NOTE);
  unsubscribe();
  client.clear();
});
