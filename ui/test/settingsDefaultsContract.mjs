// Settings-defaults contract helper (TODO-6, 0.4.6 stabilisation run).
//
// Deliberately not a `*.test.mjs`: the payload is the real serialized
// `Settings::default()` produced by the Rust contract test
// (`src-tauri/tests/settings_defaults_contract.rs`) and piped in on stdin. This file
// therefore owns no value of its own: it imports the real UI registry and asserts the
// Rust-owned defaults for every mapped field, so the UI's fallback table can never
// silently drift from `Default for Settings`.
//
// Run by the Rust test only; the UI suite's `test/*.test.mjs` glob never picks it up
// because it would block waiting for stdin.
import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import console from "node:console";
import { register } from "node:module";
import process from "node:process";

register("./ui-loader.mjs", import.meta.url);

const { SETTING_DEFS, SETTINGS_DEFAULTS } = await import(
  "../src/routes/settings/registry.ts"
);

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const raw = Buffer.concat(chunks).toString("utf8").trim();
assert.notEqual(
  raw,
  "",
  "expected the Rust get_settings_defaults payload on stdin; the Rust contract test pipes it",
);

let payload;
try {
  payload = JSON.parse(raw);
} catch (error) {
  assert.fail(`the Rust defaults payload is not JSON: ${error.message}\n${raw}`);
}
assert.ok(
  payload !== null && typeof payload === "object" && !Array.isArray(payload),
  "the Rust defaults payload must be one JSON object",
);

const fieldDefs = SETTING_DEFS.filter((def) => def.field !== null);
const credentialDefs = SETTING_DEFS.filter((def) => def.field === null);

assert.equal(
  SETTING_DEFS.length,
  26,
  "the registry must keep the closed 26-key set",
);
assert.equal(
  credentialDefs.length,
  1,
  "api.token must be the only non-value action",
);
assert.equal(
  credentialDefs[0].key,
  "api.token",
  "the sole field:null action is the credential row",
);
assert.ok(
  Object.keys(payload).every((key) => !/token/i.test(key)),
  "api.token has no Settings value; no credential may appear in the defaults payload",
);
assert.equal(
  fieldDefs.length,
  25,
  "the 26 UI keys map 25 Settings fields plus the credential action",
);

// The fallback table is the in-flight value source, so it too must cover exactly the
// mapped fields: no stale entry and no missing field.
assert.deepEqual(
  Object.keys(SETTINGS_DEFAULTS).sort(),
  fieldDefs.map((def) => def.field).sort(),
  "SETTINGS_DEFAULTS must cover exactly the registry's mapped fields",
);

// Exact coverage both ways: the Rust payload has no field the registry cannot reset,
// and every registry field has a Rust default.
assert.deepEqual(
  Object.keys(payload).sort(),
  fieldDefs.map((def) => def.field).sort(),
  "the Rust Settings fields and the registry's mapped fields must be the same set",
);

for (const def of fieldDefs) {
  const field = def.field;
  const where = `${def.key} (${field})`;
  assert.ok(
    Object.prototype.hasOwnProperty.call(payload, field),
    `${where} is missing from the Rust defaults payload`,
  );
  const actual = payload[field];
  const expected = SETTINGS_DEFAULTS[field];
  assert.equal(
    typeof actual,
    typeof expected,
    `${where} default type differs: Rust ${typeof actual}, registry ${typeof expected}`,
  );
  assert.equal(
    Array.isArray(actual),
    Array.isArray(expected),
    `${where} default shape differs: Rust ${Array.isArray(actual) ? "array" : "scalar"}, registry ${Array.isArray(expected) ? "array" : "scalar"}`,
  );
  assert.deepEqual(
    actual,
    expected,
    `${where} default differs: Rust ${JSON.stringify(actual)}, registry ${JSON.stringify(expected)}`,
  );
}

console.log(
  `settings defaults contract: ${fieldDefs.length} fields match, api.token is the sole field:null action`,
);
