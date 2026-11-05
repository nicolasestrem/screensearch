// Numeric entry validation (P7 review findings, PR #161): the UI must accept exactly
// what the kernel stores. `storage.max_width` keeps `0` as the "native width" sentinel
// and clamps every other value up to 320 (`crates/kernel/src/settings.rs`
// `sanitize_settings`), so 1-319 has to be rejected inline instead of reported as saved
// and silently changed afterwards.
import assert from "node:assert/strict";
import test from "node:test";

import { numberEntryError, settingDef } from "../src/routes/settings/registry.ts";

const maxWidth = settingDef("storage.max_width");
const interval = settingDef("capture.interval_ms");
const retention = settingDef("storage.retention_days");

test("stored image width: 0 is the native sentinel", () => {
  assert.equal(numberEntryError(maxWidth, 0), null);
});

test("stored image width: 1-319 is rejected, not silently clamped", () => {
  for (const value of [1, 100, 319]) {
    const message = numberEntryError(maxWidth, value);
    assert.ok(message, `${value} should be rejected`);
    assert.match(message, /must be 0 \(native width\) or at least 320 px/);
  }
});

test("stored image width: 320 and the upper bound are accepted", () => {
  assert.equal(numberEntryError(maxWidth, 320), null);
  assert.equal(numberEntryError(maxWidth, 1920), null);
  assert.equal(numberEntryError(maxWidth, 7_680), null);
  assert.match(numberEntryError(maxWidth, 7_681), /between 0 and 7680 px/);
});

test("a plain range still rejects both ends", () => {
  assert.match(numberEntryError(interval, 249), /between 250 and 3600000 ms/);
  assert.equal(numberEntryError(interval, 250), null);
  assert.equal(numberEntryError(interval, 4_000), null);
  assert.match(numberEntryError(interval, 3_600_001), /between 250 and 3600000 ms/);
});

test("retention accepts 0 (keep forever) and rejects fractions", () => {
  assert.equal(numberEntryError(retention, 0), null);
  assert.equal(numberEntryError(retention, 365), null);
  assert.match(numberEntryError(retention, 1.5), /needs a whole number/);
});

test("non-integers are never accepted, even inside the range", () => {
  assert.match(numberEntryError(maxWidth, 500.5), /needs a whole number/);
  assert.match(numberEntryError(interval, 2_500.25), /needs a whole number/);
});
