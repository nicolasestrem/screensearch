import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { URL } from "node:url";

import { packFixedSessionBands } from "../src/components/domain/sessionBandLayout.ts";

test("five simultaneous sessions use four fixed rows and aggregate the fifth", () => {
  const sessions = Array.from({ length: 5 }, (_, index) => ({
    id: index + 1,
    start: 10,
    end: 20,
  }));

  const packed = packFixedSessionBands(
    sessions,
    0,
    100,
    { width: 320, hitTarget: 32, gap: 4 },
    (session) => session.start,
    (session) => session.end,
  );

  assert.equal(packed.rowCount, 4);
  assert.equal(packed.bands.length, 4);
  assert.deepEqual(
    packed.bands.map(({ item, lane }) => [item.id, lane]),
    [
      [1, 0],
      [2, 1],
      [3, 2],
      [4, 3],
    ],
  );
  assert.equal(packed.overflowCount, 1);
});

test("Timeline keeps fixed session lanes in every content state", () => {
  const source = readFileSync(
    new URL("../src/routes/Timeline.tsx", import.meta.url),
    "utf8",
  );

  assert.equal(
    source.match(/\{sessionLayer\(true\)\}/g)?.length ?? 0,
    1,
    "the loading skeleton must reserve the fixed session lanes",
  );
  assert.equal(
    source.match(/^\s*\{sessionLayer\(\)\}\s*$/gm)?.length ?? 0,
    2,
    "the top-level error and empty branches must each reserve the fixed session lanes",
  );
  assert.match(
    source,
    /sessionLayer=\{sessionLayer\(/,
    "the populated branch must keep the bands inside DensityTimeline",
  );
  assert.doesNotMatch(
    source,
    /\{sessionLayer\(timeline\.isLoading\)\}/,
    "the session layer must not move below the populated scanline",
  );
});
