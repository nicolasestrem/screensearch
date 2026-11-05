// Row commit ownership (P7 review, PR #161 — P2 "preserve newer pending edits when an
// older save settles").
//
// With the write queue, a row can still have two commits in flight: the earlier one
// settles first. If it unconditionally cleared the row's pending value, the row would
// flash the older persisted value and drop the newer edit — and the user's next edit
// would be composed from that stale list, silently removing what the newer save had
// stored. Only the newest commit may change what the row displays.
import assert from "node:assert/strict";
import test from "node:test";

import {
  begun,
  clearedIfOwned,
  failed,
  initialRowState,
  settled,
} from "../src/routes/settings/pendingState.ts";

test("the newest commit owns the row's pending value", () => {
  let state = initialRowState();
  state = begun(["A"], 1);
  state = begun(["A", "B"], 2);
  assert.deepEqual(state.pending, ["A", "B"]);
  assert.equal(state.latest, 2);
});

test("an older save settling does not drop a newer pending edit", () => {
  let state = initialRowState();
  state = begun(["A"], 1);
  state = begun(["A", "B"], 2);
  // Save A settles while B is still queued.
  const afterA = settled(state, 1);
  assert.deepEqual(afterA.pending, ["A", "B"], "B must survive A's settlement");
  // The real owner settles later and clears it.
  assert.equal(settled(afterA, 2).pending, null);
});

test("an older save's failure never masks the newer edit", () => {
  let state = initialRowState();
  state = begun(["A"], 1);
  state = begun(["A", "B"], 2);
  const afterA = failed(state, 1, ["A"], "backend refused");
  assert.deepEqual(afterA.pending, ["A", "B"]);
  assert.equal(afterA.failure, null, "the newest commit still owns the row");
});

test("the newest failure is reported with the value to retry", () => {
  let state = initialRowState();
  state = begun(["A"], 1);
  const done = failed(state, 1, ["A"], "settings could not be re-read");
  assert.deepEqual(done.failure, {
    value: ["A"],
    message: "settings could not be re-read",
  });
  assert.equal(done.pending, null);
});

test("beginning a commit clears an earlier failure", () => {
  let state = initialRowState();
  state = failed(begun(["A"], 1), 1, ["A"], "refused");
  assert.notEqual(state.failure, null);
  assert.equal(begun(["B"], 2).failure, null);
});

test("a newer draft survives an earlier save's success", () => {
  // Commit 10 minutes, then type 20 while the write is queued: the 10-minute save's
  // success must not clear the 20-minute draft.
  const typedLater = { value: 20, issued: 2 };
  assert.deepEqual(clearedIfOwned(typedLater, 1), typedLater);
  assert.equal(clearedIfOwned(typedLater, 2), null);
});
