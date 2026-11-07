import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { URL } from "node:url";

import {
  SETTING_CONTROL_SLOT_CLASS,
  settingRowLayout,
} from "../src/routes/settings/settingRowLayout.ts";

// Width utilities that let the box shrink to its content. Any of them would put
// the column back on the row's copy, which is the 0.4.5 General defect.
const CONTENT_SIZED = [
  /^(?:[a-z-]+:)?w-auto$/,
  /^(?:[a-z-]+:)?w-fit$/,
  /^(?:[a-z-]+:)?w-max$/,
  /^(?:[a-z-]+:)?basis-auto$/,
  /^(?:[a-z-]+:)?basis-full$/,
  /^(?:[a-z-]+:)?flex-1$/,
  /^(?:[a-z-]+:)?flex-auto$/,
  /^(?:[a-z-]+:)?grow(?:-0)?$/,
  /^(?:[a-z-]+:)?shrink(?:-0)?$/,
];

const classes = (value) => value.split(/\s+/).filter(Boolean);

test("both row variants place every control in the same fixed column", () => {
  const stacked = settingRowLayout({ labelInControl: true });
  const split = settingRowLayout({ labelInControl: false });

  assert.equal(stacked.variant, "stacked");
  assert.equal(split.variant, "split");
  assert.equal(
    stacked.controlSlot,
    split.controlSlot,
    "a control must sit in one column whatever variant the row uses",
  );
  assert.equal(stacked.controlSlot, SETTING_CONTROL_SLOT_CLASS);
});

test("the control column has one token width and no content-sized utility", () => {
  const slot = classes(settingRowLayout({ labelInControl: true }).controlSlot);

  const widths = slot.filter((name) => /^(?:[a-z-]+:)?w-/.test(name));
  assert.deepEqual(
    widths.sort(),
    ["md:w-96", "w-full"],
    "one density-scale width plus the narrow-layout full width, nothing else",
  );

  for (const name of slot) {
    for (const pattern of CONTENT_SIZED) {
      assert.doesNotMatch(
        name,
        pattern,
        `${name} would size the column to its content or to its siblings`,
      );
    }
  }

  assert.ok(slot.includes("max-w-full"), "a narrow grid cell must clamp the column");
  assert.ok(slot.includes("min-w-0"), "the column must be allowed to shrink in its cell");
});

test("the column width comes from the documented spacing scale", () => {
  const slot = classes(SETTING_CONTROL_SLOT_CLASS);
  const tokenScaleWidth = slot.find((name) => /^(?:[a-z-]+:)?w-\d+$/.test(name));

  assert.equal(tokenScaleWidth, "md:w-96", "no literal or arbitrary width may set the column");

  const key = tokenScaleWidth.slice(tokenScaleWidth.indexOf("w-") + 2);
  const tokens = readFileSync(
    new URL("../src/styles/tokens.css", import.meta.url),
    "utf8",
  );
  assert.match(
    tokens,
    new RegExp(`--space-${key}:`),
    `the ${tokenScaleWidth} utility must resolve through --space-${key}`,
  );
});

test("the marker beside the control cannot squeeze the column", () => {
  const stacked = settingRowLayout({ labelInControl: true });
  const split = settingRowLayout({ labelInControl: false });

  for (const layout of [stacked, split]) {
    assert.ok(
      classes(layout.controlCell).includes("flex-wrap"),
      "the marker and reset must wrap below the column, never shrink it",
    );
    assert.equal(layout.controlSlot, SETTING_CONTROL_SLOT_CLASS);
  }

  assert.ok(classes(stacked.controlCell).includes("col-span-12"));
  assert.ok(classes(stacked.controlCell).includes("order-1"));
  assert.ok(classes(split.controlCell).includes("md:col-span-5"));
  assert.ok(classes(split.controlCell).includes("md:justify-end"));
});
