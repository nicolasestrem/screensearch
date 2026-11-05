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
  supersedePendingFailure,
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

test("a newer edit supersedes an older failure even though its token differs", () => {
  const first = nextPendingToken();
  setPendingField(FIELD, ["A"], first);
  setPendingFailure(FIELD, ["A"], "refused", first);
  // A new edit carries a fresh token, so an ownership check would never clear this.
  const second = nextPendingToken();
  assert.notEqual(first, second);
  clearPendingFailure(FIELD, second);
  assert.notEqual(pendingFailure(FIELD), undefined, "the ownership clear is not the tool");
  supersedePendingFailure(FIELD);
  assert.equal(pendingFailure(FIELD), undefined);
});

test("an older write failing cannot publish over a newer edit", () => {
  const first = nextPendingToken();
  const second = nextPendingToken();
  setPendingField(FIELD, ["A"], first);
  setPendingField(FIELD, ["A", "B"], second);
  // The older write fails while the newer one still owns the field: publishing its
  // value would make the row show [A] and the next edit submit [A, C], dropping B.
  setPendingFailure(FIELD, ["A"], "refused", first);
  assert.equal(pendingFailure(FIELD), undefined);
  assert.deepEqual(pendingFieldValue(FIELD), ["A", "B"]);
  // The owner failing does publish, with the value to retry.
  clearPendingField(FIELD, second);
  setPendingFailure(FIELD, ["A", "B"], "refused", second);
  assert.deepEqual(pendingFailure(FIELD)?.value, ["A", "B"]);
  supersedePendingFailure(FIELD);
});

test("a stale failure is dropped even after the field's entry retired", () => {
  const first = nextPendingToken();
  const second = nextPendingToken();
  setPendingField(FIELD, ["A"], first);
  setPendingField(FIELD, ["A", "B"], second);
  clearPendingField(FIELD, second);   // the newer save settled; no entry remains
  setPendingFailure(FIELD, ["A"], "refused", first);
  assert.equal(pendingFailure(FIELD), undefined, "only the newest token may publish");
  // The newest token still may, which is how a retry gets its value back.
  setPendingFailure(FIELD, ["A", "B"], "refused", second);
  assert.deepEqual(pendingFailure(FIELD)?.value, ["A", "B"]);
  supersedePendingFailure(FIELD);
});
