// Category frame state (P7 review finding, PR #161 — P2 "preserve populated rows when a
// settings refetch fails").
//
// TanStack keeps the last payload when a background refetch fails, and sets BOTH
// `isError` and `isRefetchError`. The resolver must therefore look at the data first:
// a failure on top of data is `partial` (rows stay mounted, drafts survive), and only a
// read that never produced data is a full `error`.
import assert from "node:assert/strict";
import test from "node:test";

import { categoryFrameState, failureMessage } from "../src/routes/settings/frameState.ts";

const SETTINGS = { capture_interval_ms: 3000 };
const read = (over) => ({
  data: undefined,
  error: null,
  isError: false,
  isPending: false,
  isRefetchError: false,
  ...over,
});

test("a failed first read is a full error", () => {
  assert.equal(
    categoryFrameState(read({ isError: true, error: new Error("ipc") })),
    "error",
  );
});

test("a read with no data and no error is loading", () => {
  assert.equal(categoryFrameState(read({ isPending: true })), "loading");
  assert.equal(categoryFrameState(read({})), "loading");
});

test("a failed background refetch keeps the rows: partial, never error", () => {
  const refetchFailed = read({
    data: SETTINGS,
    isError: true,
    isRefetchError: true,
    error: new Error("ipc"),
  });
  assert.equal(categoryFrameState(refetchFailed), "partial");
});

test("a primary read that errors while data is cached is partial", () => {
  assert.equal(categoryFrameState(read({ data: SETTINGS, isError: true })), "partial");
});

test("a failed status probe beside healthy settings is partial", () => {
  assert.equal(categoryFrameState(read({ data: SETTINGS }), true), "partial");
});

test("healthy data is populated", () => {
  assert.equal(categoryFrameState(read({ data: SETTINGS })), "populated");
});

test("failure messages are one sentence, never a stack", () => {
  assert.equal(failureMessage(new Error("  set_settings failed  ")), "set_settings failed");
  assert.equal(failureMessage(new Error("   ")), "the backend did not respond");
  assert.equal(failureMessage("plain"), "plain");
  assert.equal(failureMessage({ code: 7 }), "the backend did not respond");
});
