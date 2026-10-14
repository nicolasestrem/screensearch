#!/usr/bin/env node
// GHSA-4w2j-m93h-cj5j: quinn-proto <0.11.15 permits remote memory exhaustion.
// Check every lockfile entry, including optional/transitive dependencies that
// GitHub scans even when the application's enabled features don't compile them.
import { readFileSync } from "node:fs";

const lockfile = readFileSync(new URL("../Cargo.lock", import.meta.url), "utf8");
const versions = lockfile.split(/^\[\[package\]\]\s*$/m)
  .filter((entry) => /^name = "quinn-proto"\s*$/m.test(entry))
  .map((entry) => {
    const version = /^version = "([^"]+)"\s*$/m.exec(entry)?.[1];
    if (!version) throw new Error("quinn-proto lockfile entry has no version");
    return version;
  });

for (const version of versions) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) throw new Error(`Unrecognized quinn-proto version: ${version}`);
  const [major, minor, patch] = match.slice(1).map(Number);
  if (major === 0 && (minor < 11 || (minor === 11 && patch < 15))) {
    throw new Error(`quinn-proto ${version} is affected by GHSA-4w2j-m93h-cj5j; use >=0.11.15`);
  }
}

console.log(`PASS: GHSA-4w2j-m93h-cj5j — ${versions.length ? `quinn-proto ${versions.join(", ")}` : "quinn-proto absent"}`);
