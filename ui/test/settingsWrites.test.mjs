// Settings writes are serialized (P7 review finding, PR #161 — P1 "serialize
// autosaves before building whole-settings snapshots").
//
// `set_settings` carries the WHOLE struct and the mutation is optimistic, so with two
// writes in flight the earlier one's refetch replaces the later one's optimistic value
// and the next commit built from the cache writes that reverted value back: a silently
// lost update. The queue is the mechanism that makes that impossible; this test fails
// if the queue ever lets two jobs overlap.
import assert from "node:assert/strict";
import test from "node:test";

import { setTimeout as sleep } from "node:timers/promises";

import { createSettingsWriteQueue } from "../src/routes/settings/settingsWrites.ts";

const tick = (ms = 5) => sleep(ms);

test("two enqueued writes never overlap", async () => {
  const queue = createSettingsWriteQueue();
  const events = [];
  let inFlight = 0;
  let maxInFlight = 0;

  const job = (name, delay) => () => {
    inFlight += 1;
    maxInFlight = Math.max(maxInFlight, inFlight);
    events.push(`${name}:start`);
    return tick(delay).then(() => {
      events.push(`${name}:end`);
      inFlight -= 1;
    });
  };

  await Promise.all([queue.enqueue(job("A", 20)), queue.enqueue(job("B", 1))]);

  assert.equal(maxInFlight, 1, `writes overlapped: ${events.join(" ")}`);
  // B is shorter than A, so overlap would show up as B finishing first.
  assert.deepEqual(events, ["A:start", "A:end", "B:start", "B:end"]);
});

test("a rejected write does not wedge the queue", async () => {
  const queue = createSettingsWriteQueue();
  const ran = [];

  const failing = queue.enqueue(async () => {
    ran.push("first");
    throw new Error("backend refused");
  });
  await assert.rejects(failing, /backend refused/);

  await queue.enqueue(async () => {
    ran.push("second");
  });
  assert.deepEqual(ran, ["first", "second"]);
});

test("each waiting write sees the result of the one before it", async () => {
  const queue = createSettingsWriteQueue();
  // Stands in for the settings cache: a write is built from it, then persisted into it.
  const cache = { value: "initial" };
  const written = [];

  const write = (next) =>
    queue.enqueue(async () => {
      const base = cache.value; // the snapshot the payload is built from
      await tick(next === "B" ? 1 : 15);
      cache.value = next;
      written.push(`${base}->${next}`);
    });

  await Promise.all([write("A"), write("B")]);
  assert.deepEqual(written, ["initial->A", "A->B"]);
});
