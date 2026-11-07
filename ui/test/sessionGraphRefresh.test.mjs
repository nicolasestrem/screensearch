// A finished graph job must refresh the session graph (`03 §7h`), not only Work and
// the P8 insight rows. The defect this guards is a *missing key*, not a wrong branch:
// the session graph lives under the `sessions` head, so the work/insightList/entities
// prefixes never covered it, a Session or Moment kept its stale entities and its
// "still clustering" reading, and nothing else would have corrected it — the read has
// no polling and `app/providers.tsx` disables refetch-on-window-focus.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";

register("./ui-loader.mjs", import.meta.url);

const { GRAPH_JOB_INVALIDATIONS, queryKeys } = await import(
  "../src/lib/ipc/queryKeys.ts"
);

/** Does any prefix in the list cover `key`? The same test TanStack applies to a partial key. */
function covered(key) {
  return GRAPH_JOB_INVALIDATIONS.some(
    (prefix) =>
      prefix.length <= key.length &&
      prefix.every((part, i) => part === key[i]),
  );
}

test("a completed graph job invalidates the session graph a Session or Moment is showing", () => {
  assert.equal(covered(queryKeys.sessionGraph(7)), true);
  assert.equal(covered(queryKeys.sessionGraph(218)), true);
});

test("the list still covers every family that was already refreshed", () => {
  // Regression guard on the refactor: routing the handler through one list must not
  // drop a family that used to be invalidated by name.
  assert.equal(covered(queryKeys.threadPage(3)), true);
  assert.equal(covered(queryKeys.insightList({ limit: 20 })), true);
  assert.equal(covered(queryKeys.entityList({}), true), true);
});

test("the debounced block invalidates that one list and nothing by hand", async () => {
  // The handler must consume the list, or adding a key here would change nothing.
  const { readFile } = await import("node:fs/promises");
  const source = await readFile(
    "./src/lib/ipc/useLiveEvents.ts",
    "utf8",
  );
  assert.match(source, /for \(const queryKey of GRAPH_JOB_INVALIDATIONS\)/);
  assert.doesNotMatch(source, /queryKey: queryKeys\.entitiesPrefix/);
});
