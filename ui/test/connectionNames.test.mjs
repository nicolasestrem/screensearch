// Naming the evidence behind a connection (`03 §7h.4`).
//
// The defect: `shared_entity_ids` is computed across *every* member session of a thread
// (`crates/store/src/graph.rs`, the `shared` subquery joins `thread_sessions` on both
// sides), while a Session or Moment only holds the opened session's own entities. A
// connection supported by a sibling session's entity therefore rendered "Entity 123" —
// the panel's documented fallback — on exactly the rows whose evidence matters.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);

const { unresolvedSharedEntityIds } = await import("../src/lib/connectionNames.ts");

/** A `ConnectedThread` as the panel receives it, with only the fields under test. */
function connection(id, sharedEntityIds) {
  return { thread: { id }, shared_entity_ids: sharedEntityIds };
}

test("an entity a sibling session touched is reported as unresolved", () => {
  // Session 5 touched entity 2 only; the connection is supported by 2 and by 3, which
  // belongs to another member session of the same thread.
  const names = { 2: "Sunrise" };
  assert.deepEqual(
    unresolvedSharedEntityIds([connection(9, [2, 3])], names),
    [3],
  );
});

test("nothing is requested when the session already names every shared entity", () => {
  const names = { 2: "Sunrise", 3: "Project Atlas" };
  assert.deepEqual(
    unresolvedSharedEntityIds([connection(9, [2, 3]), connection(11, [3])], names),
    [],
  );
});

test("the ids are unique and ordered, so one read covers every connection", () => {
  const names = { 2: "Sunrise" };
  const asked = unresolvedSharedEntityIds(
    [connection(9, [7, 3]), connection(11, [3, 5]), connection(12, [2])],
    names,
  );
  assert.deepEqual(asked, [3, 5, 7]);
});

test("no connections and no names ask for nothing", () => {
  assert.deepEqual(unresolvedSharedEntityIds([], {}), []);
});
