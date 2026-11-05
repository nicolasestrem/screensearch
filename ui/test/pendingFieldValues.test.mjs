// The pending value beside the write queue must survive a category unmount, or a queued
// save is invisible when the user comes back and the next edit is composed from the
// stale list (P7 review, PR #161).
import assert from "node:assert/strict";
import test from "node:test";

import {
  pendingFieldValue,
  setPendingField,
  subscribePendingField,
} from "../src/routes/settings/pendingFieldValues.ts";

test("a queued value is visible to a fresh mount of the same field", () => {
  setPendingField("privacy.excluded_apps", ["A"]);
  assert.deepEqual(pendingFieldValue("privacy.excluded_apps"), ["A"]);
  setPendingField("privacy.excluded_apps", null);
});

test("settling clears the shared value so later mounts read the cache", () => {
  setPendingField("privacy.excluded_apps", ["A"]);
  setPendingField("privacy.excluded_apps", null);
  assert.equal(pendingFieldValue("privacy.excluded_apps"), undefined);
});

test("fields do not see each other's pending values", () => {
  setPendingField("api.port", 9000);
  assert.equal(pendingFieldValue("api.enabled"), undefined);
  setPendingField("api.port", null);
});

test("a change notifies subscribers once, and clearing an absent value does not", () => {
  let calls = 0;
  const stop = subscribePendingField(() => {
    calls += 1;
  });
  setPendingField("api.port", 9000);
  assert.equal(calls, 1);
  setPendingField("api.port", null);
  assert.equal(calls, 2);
  setPendingField("api.port", null);
  assert.equal(calls, 2, "no value to clear, nothing to re-render");
  stop();
  setPendingField("api.port", 1);
  assert.equal(calls, 2, "unsubscribed listeners are not called");
  setPendingField("api.port", null);
});
