// Settings draft reconcile (0.4.5 P3 QA blocker): a bulk Save of the Settings form must never
// write back a stale copy of keys edited outside the form. The bug: enabling the HTTP API in
// ApiPanel, then saving any other setting, wrote the form's stale `api_enabled=false` back and
// stopped the listener. Pure logic, no DOM: imports src/lib/settingsDraft.ts.
import assert from "node:assert/strict";
import test from "node:test";

import {
  EXTERNALLY_EDITED_KEYS,
  mirrorExternallyEditedKeys,
} from "../src/lib/settingsDraft.ts";

// Only the fields the helper reads, plus one form-owned field to prove edits survive.
function settings(overrides = {}) {
  return {
    capture_interval_ms: 2000,
    app_run_at_startup: false,
    app_close_to_tray: true,
    api_enabled: false,
    api_port: 43210,
    api_bind_address: "127.0.0.1",
    ...overrides,
  };
}

test("an API change made in ApiPanel is mirrored into the form draft", () => {
  // The form was seeded while the API was off.
  const draft = settings({ api_enabled: false });
  // ApiPanel then enabled it on another port; the settings refetch returns the new state.
  const persisted = settings({ api_enabled: true, api_port: 50123, api_bind_address: "0.0.0.0" });

  const next = mirrorExternallyEditedKeys(draft, persisted);

  assert.equal(next.api_enabled, true, "a Save must not write the stale disabled value back");
  assert.equal(next.api_port, 50123);
  assert.equal(next.api_bind_address, "0.0.0.0");
});

test("the App toggles are mirrored too", () => {
  const next = mirrorExternallyEditedKeys(
    settings({ app_run_at_startup: false, app_close_to_tray: true }),
    settings({ app_run_at_startup: true, app_close_to_tray: false }),
  );
  assert.equal(next.app_run_at_startup, true);
  assert.equal(next.app_close_to_tray, false);
});

test("in-progress edits to the form's own fields are preserved", () => {
  const draft = settings({ capture_interval_ms: 750, api_enabled: false });
  const persisted = settings({ capture_interval_ms: 2000, api_enabled: true });

  const next = mirrorExternallyEditedKeys(draft, persisted);

  assert.equal(next.capture_interval_ms, 750, "the user's unsaved edit survives the refetch");
  assert.equal(next.api_enabled, true);
});

test("an unchanged draft keeps its reference, so the reconcile effect cannot loop", () => {
  const draft = settings({ capture_interval_ms: 750 });
  // Differs only in a form-owned field: nothing external changed.
  const persisted = settings({ capture_interval_ms: 2000 });
  assert.equal(mirrorExternallyEditedKeys(draft, persisted), draft);
});

test("the draft is never mutated in place", () => {
  const draft = settings({ api_enabled: false });
  const snapshot = { ...draft };
  mirrorExternallyEditedKeys(draft, settings({ api_enabled: true }));
  assert.deepEqual(draft, snapshot);
});

test("the mirrored set is exactly the five externally edited keys", () => {
  assert.deepEqual(
    [...EXTERNALLY_EDITED_KEYS].sort(),
    ["api_bind_address", "api_enabled", "api_port", "app_close_to_tray", "app_run_at_startup"],
  );
});
