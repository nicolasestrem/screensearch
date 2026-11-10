// A settings save changes more than the `settings` query: `set_settings` hot-applies
// the typed keys (hotkey re-registration, local-API apply, throttle governor
// start/stop), so each surface reading one of those needs its own invalidation.
// The defect this guards is a *missing key*: `throttleStatus` stayed at its pre-save
// state until the next governor tick, which is why the throttle readout used to take
// its on/off line from a local prop instead of the shared cache (TODO-4).
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);

const { queryKeys } = await import("../src/lib/ipc/queryKeys.ts");
const { invalidateAfterSettingsSave } = await import(
  "../src/lib/ipc/mutations.ts"
);

/** Drive the same loop `useSetSettings.onSettled` runs, recording every key. */
function invalidatedKeys() {
  const keys = [];
  invalidateAfterSettingsSave((queryKey) => {
    keys.push(queryKey);
  });
  return keys;
}

test("a settings save invalidates the throttle status the readout reads", () => {
  const keys = invalidatedKeys();
  const families = keys.map((key) => key[0]);
  assert.ok(
    families.includes(queryKeys.throttleStatus[0]),
    `settings save invalidates ${families.join(", ")}; throttleStatus is missing`,
  );
});

test("the save invalidation covers every settings-derived family and nothing else", () => {
  const keys = invalidatedKeys().sort((a, b) =>
    String(a[0]).localeCompare(String(b[0])),
  );
  // `settings` is the write itself; `hotkeyStatus`, `apiStatus` and `throttleStatus`
  // are the payloads `set_settings` hot-applies. Hardware reads (monitors), event-only
  // caches (sidecar/model/download/setup/update) and the read-only advanced projection
  // are not settings-derived and stay out of the list.
  assert.deepEqual(keys, [
    queryKeys.apiStatus,
    queryKeys.hotkeyStatus,
    queryKeys.settings,
    queryKeys.throttleStatus,
  ]);
});
