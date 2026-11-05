// The pending value beside the write queue must survive a category unmount, and only the
// commit that set it may retire it (P7 review, PR #161 — rows 80 and 78).
import assert from "node:assert/strict";
import test from "node:test";

import {
  clearPendingField,
  pendingFieldValue,
  setPendingField,
  subscribePendingField,
} from "../src/routes/settings/pendingFieldValues.ts";

const FIELD = "privacy.excluded_apps";

test("a queued value is visible to a fresh mount of the same field", () => {
  setPendingField(FIELD, ["A"], 1);
  assert.deepEqual(pendingFieldValue(FIELD), ["A"]);
  clearPendingField(FIELD, 1);
});

test("settling clears the shared value so later mounts read the cache", () => {
  setPendingField(FIELD, ["A"], 1);
  clearPendingField(FIELD, 1);
  assert.equal(pendingFieldValue(FIELD), undefined);
});

test("an older save settling does not retire a newer queued value", () => {
  setPendingField(FIELD, ["A"], 1);
  setPendingField(FIELD, ["A", "B"], 2);
  // Save 1 settles while save 2 is still queued: its value must stay visible, or the
  // row shows the stale list the next edit is composed from.
  clearPendingField(FIELD, 1);
  assert.deepEqual(pendingFieldValue(FIELD), ["A", "B"]);
  clearPendingField(FIELD, 2);
  assert.equal(pendingFieldValue(FIELD), undefined);
});

test("fields do not see each other's pending values", () => {
  setPendingField("api.port", 9000, 1);
  assert.equal(pendingFieldValue("api.enabled"), undefined);
  clearPendingField("api.port", 1);
});

test("a change notifies subscribers once, and clearing what it does not own does not", () => {
  let calls = 0;
  const stop = subscribePendingField(() => {
    calls += 1;
  });
  setPendingField("api.port", 9000, 1);
  assert.equal(calls, 1);
  clearPendingField("api.port", 2);
  assert.equal(calls, 1, "not the owning commit: nothing changes, nothing re-renders");
  clearPendingField("api.port", 1);
  assert.equal(calls, 2);
  stop();
  setPendingField("api.port", 1, 1);
  assert.equal(calls, 2, "unsubscribed listeners are not called");
  clearPendingField("api.port", 1);
});
