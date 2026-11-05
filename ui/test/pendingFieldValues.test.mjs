// The pending value beside the write queue must survive a category unmount, and only the
// commit that set it may retire it (P7 review, PR #161 — rows 80 and 78).
import assert from "node:assert/strict";
import test from "node:test";

import {
  clearPendingFailure,
  clearPendingField,
  nextPendingToken,
  pendingFailure,
  pendingFieldValue,
  setPendingFailure,
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

test("a remount never re-uses a live commit's token", () => {
  // A remount used to restart its counter at 1, so its first edit took the same token
  // as the previous mount's in-flight save and that save could retire the newer value.
  const first = nextPendingToken();
  const second = nextPendingToken();
  assert.notEqual(first, second);
  setPendingField(FIELD, ["A", "B"], second);
  clearPendingField(FIELD, first);
  assert.deepEqual(pendingFieldValue(FIELD), ["A", "B"]);
  clearPendingField(FIELD, second);
});

test("a failure that arrives after unmount is still there with its value", () => {
  const token = nextPendingToken();
  setPendingField(FIELD, ["A"], token);
  // The write fails after the category unmounted: the pending value is retired, but the
  // failure (with what to retry) must survive so returning shows the error.
  clearPendingField(FIELD, token);
  setPendingFailure(FIELD, ["A"], "Settings could not be saved", token);
  assert.deepEqual(pendingFailure(FIELD), {
    value: ["A"],
    message: "Settings could not be saved",
    issued: token,
  });
  clearPendingFailure(FIELD, token);
  assert.equal(pendingFailure(FIELD), undefined);
});

test("a newer edit clears an older failure, and an older save cannot clear a newer one", () => {
  const first = nextPendingToken();
  const second = nextPendingToken();
  setPendingFailure(FIELD, ["A"], "refused", first);
  // The user edits again: the recorded failure is superseded.
  clearPendingFailure(FIELD, second);
  assert.notEqual(pendingFailure(FIELD), undefined, "a non-owning commit clears nothing");
  clearPendingFailure(FIELD, first);
  assert.equal(pendingFailure(FIELD), undefined);
});
