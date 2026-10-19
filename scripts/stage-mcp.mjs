#!/usr/bin/env node
// Stages the workspace-built screensearch-mcp binary where Tauri's NSIS bundler expects an
// externalBin sidecar: src-tauri/binaries/screensearch-mcp-<host-triple>[.exe] (0.3.0 PR8).
//
// IMPORTANT: tauri-build's build script resolves `bundle.externalBin` on EVERY compile of
// src-tauri — not just at bundle time — and fails the build if the staged file is missing.
// So this MUST run before any cargo step that touches the app crate. It is wired into
// tauri.conf.json's beforeDevCommand / beforeBuildCommand and into CI (see .github/workflows/
// ci.yml). Run it once per fresh clone before a bare `cargo build/clippy/test --workspace`.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, "..");

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = out.split(/\r?\n/).find((l) => l.startsWith("host:"));
  if (!line) {
    throw new Error("could not determine the host target triple from `rustc -vV`");
  }
  return line.slice("host:".length).trim();
}

function main() {
  console.log("[stage-mcp] building screensearch-mcp (release)...");
  execFileSync("cargo", ["build", "--locked", "--release", "-p", "mcp"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  const triple = hostTriple();
  const ext = process.platform === "win32" ? ".exe" : "";
  const src = join(repoRoot, "target", "release", `screensearch-mcp${ext}`);
  if (!existsSync(src)) {
    throw new Error(`built binary not found at ${src}`);
  }

  const destDir = join(repoRoot, "src-tauri", "binaries");
  mkdirSync(destDir, { recursive: true });
  const dest = join(destDir, `screensearch-mcp-${triple}${ext}`);

  // Only rewrite when the bytes differ, to keep mtimes stable and avoid needless rebuilds.
  // Read the destination once, atomically: an existsSync-then-read/write pattern races
  // between the check and the use (CodeQL js/file-system-race). A failed read with
  // ENOENT means the destination is absent; any other error is real and rethrown.
  const srcBytes = readFileSync(src);
  let destBytes = null;
  try {
    destBytes = readFileSync(dest);
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
  if (destBytes !== null && destBytes.equals(srcBytes)) {
    console.log(`[stage-mcp] up to date: ${dest}`);
    return;
  }
  writeFileSync(dest, srcBytes);
  console.log(`[stage-mcp] staged ${dest}`);
}

main();
