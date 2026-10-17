#!/usr/bin/env node
// Standalone MCP wrapper packaging (agent-memory P2.4): builds the release
// `screensearch-mcp` binary for the host target, stages a small distribution tree
// (binary + README + LICENSE), writes one archive, and emits a sha256 checksum beside it.
//
// CI artifacts only: this script never creates tags or releases and never uploads
// anywhere. The Windows NSIS/staging path (scripts/stage-mcp.mjs, src-tauri/) is
// completely untouched — this packages only the standalone wrapper.
//
// Usage:
//   node scripts/package-mcp.mjs [--target <triple>] [--out-dir <dir>]
//
// `--target` must equal the host triple (`rustc -vV`): packaging runs natively on the
// target OS runner, never a cross-compile, so the label can't drift from the binary.
// Archive format follows the target OS: `.zip` on Windows, `.tar.gz` elsewhere.
//
// The archiver is chosen per host (`archiver()`): on Windows the CI shell is GitBash,
// whose PATH shadows the OS libarchive `bsdtar` with MSYS/GNU `tar`, so the native
// `%SystemRoot%\System32\tar.exe` is resolved by absolute path. A `.zip` that is not a
// real ZIP (the mislabeled-tar failure mode) is rejected after creation.
//
// Prints (the workflow parses the archive/checksums lines):
//   [package-mcp] archive=<abs path>
//   [package-mcp] checksums=<abs path>

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function parseArgs(argv) {
  const args = { target: undefined, outDir: undefined };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const eq = arg.indexOf("=");
    const key = eq === -1 ? arg : arg.slice(0, eq);
    const inline = eq === -1 ? undefined : arg.slice(eq + 1);
    const value = () => {
      if (inline !== undefined) return inline;
      i += 1;
      if (i >= argv.length) throw new Error(`${key} requires a value`);
      return argv[i];
    };
    if (key === "--target") args.target = value();
    else if (key === "--out-dir") args.outDir = value();
    else throw new Error(`unknown argument: ${arg} (see the header for usage)`);
  }
  return args;
}

function run(command, args, options = {}) {
  console.log(`[package-mcp] $ ${command} ${args.join(" ")}`);
  return execFileSync(command, args, { cwd: repoRoot, stdio: "inherit", ...options });
}

/**
 * The archiver to run for this host.
 *
 * On Windows the CI shell is GitBash, whose PATH puts MSYS/GNU `tar` (1.35) ahead of the
 * OS-provided libarchive `bsdtar`. GNU tar reads a `D:\...` path as a remote `host:path`
 * ("tar: Cannot connect to D: resolve failed") and cannot write zip, so `-a` with a `.zip`
 * name would emit an uncompressed tar mislabeled `.zip`. The canonical native archiver is
 * `%SystemRoot%\System32\tar.exe` (libarchive bsdtar, present on Windows 10 1803+ / Server
 * 2019+); resolve it by absolute path so the GitBash PATH shadow can never select GNU tar.
 *
 * Elsewhere `tar` from PATH is the platform archiver (GNU tar on Linux, bsdtar on macOS).
 */
function archiver() {
  if (process.platform !== "win32") return "tar";
  const root = process.env.SystemRoot || process.env.windir || "C:\\Windows";
  const native = join(root, "System32", "tar.exe");
  if (!existsSync(native)) {
    throw new Error(
      `no native Windows archiver at ${native} (Windows 10 1803+/Server 2019+ ship ` +
        "libarchive bsdtar); refusing to fall back to MSYS/GNU tar, which cannot write " +
        "zip and misparses drive paths",
    );
  }
  return native;
}

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = out.split(/\r?\n/).find((l) => l.startsWith("host:"));
  if (!line) throw new Error("could not determine the host target triple from `rustc -vV`");
  return line.slice("host:".length).trim();
}

function workspaceVersion() {
  const toml = readFileSync(join(repoRoot, "Cargo.toml"), "utf8");
  const block = toml.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/);
  const version = block && block[1].match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!version) throw new Error("could not read [workspace.package] version from Cargo.toml");
  return version[1];
}

/** Builds the release wrapper and returns the artifact path cargo actually produced. */
function buildReleaseBinary() {
  // `json-render-diagnostics` keeps human progress on stderr (inherited, unmasked) while
  // stdout carries the machine-readable artifact messages — so CARGO_TARGET_DIR and any
  // other layout choice are honored without hardcoding target/release paths.
  const stdout = execFileSync(
    "cargo",
    ["build", "--locked", "--release", "-p", "mcp", "--message-format=json-render-diagnostics"],
    {
      cwd: repoRoot,
      stdio: ["inherit", "pipe", "inherit"],
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 256,
    },
  );
  let executable = null;
  for (const line of stdout.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    if (
      message.reason === "compiler-artifact" &&
      message.target &&
      message.target.name === "screensearch-mcp" &&
      message.executable
    ) {
      executable = message.executable;
    }
  }
  if (!executable) {
    throw new Error("cargo did not report a screensearch-mcp executable artifact");
  }
  return executable;
}

function readme(version, target) {
  return `ScreenSearch MCP server (screensearch-mcp) — standalone stdio wrapper
================================================================

Version: ${version}
Target:  ${target}

This is the standalone MCP wrapper over the ScreenSearch local HTTP API. It holds no
data of its own: it speaks MCP (newline-delimited JSON-RPC 2.0 over stdio) and talks
HTTP to a ScreenSearch instance whose HTTP API is enabled (default
http://127.0.0.1:43210).

Configuration (flag > environment variable > default):
  --api-url <URL>        SCREENSEARCH_API_URL     default http://127.0.0.1:43210
  --token <TOKEN>        SCREENSEARCH_API_TOKEN   required for tool calls
  --read-only            list and accept only read tools (no marks/capture/ask)

Run \`screensearch-mcp --help\` for the full usage. The API is off by default; enable
it in ScreenSearch Settings -> HTTP API and copy the token shown there. Any process
holding the token can read your history — enabling the API is a trust decision.

--read-only is a client-side tool filter only: it narrows the tool list this wrapper
offers. The server independently authorizes every request against the token's own
permissions, so for server-enforced read-only access configure an independently
created read-only API token instead.

Docs: https://github.com/nicolasestrem/screensearch/blob/main/docs/MCP.md
`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const host = hostTriple();
  const target = args.target || host;
  if (target !== host) {
    throw new Error(
      `--target ${target} does not match the host triple ${host}; this script packages ` +
        "natively only (run it on the target OS, never as a cross-build)",
    );
  }

  const version = workspaceVersion();
  const outDir = resolve(args.outDir || join(repoRoot, "dist", "mcp"));
  const exeName = target.includes("windows") ? "screensearch-mcp.exe" : "screensearch-mcp";
  const ext = target.includes("windows") ? "zip" : "tar.gz";
  const base = `screensearch-mcp-${version}-${target}`;

  const executable = buildReleaseBinary();
  const built = execFileSync(executable, ["--version"], { encoding: "utf8" }).trim();
  if (built !== `screensearch-mcp ${version}`) {
    throw new Error(`built binary reports "${built}", expected "screensearch-mcp ${version}"`);
  }
  console.log(`[package-mcp] built binary reports: ${built}`);

  // Stage the distribution tree, then archive the directory itself so unpacking gives a
  // single top-level folder.
  const staging = join(outDir, base);
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });
  copyFileSync(executable, join(staging, exeName));
  copyFileSync(join(repoRoot, "LICENSE"), join(staging, "LICENSE"));
  writeFileSync(join(staging, "README.txt"), readme(version, target));

  const archive = join(outDir, `${base}.${ext}`);
  rmSync(archive, { force: true });
  const tar = archiver();
  if (ext === "zip") {
    // bsdtar writes a real zip when `-a` sees a .zip name; GNU tar would emit an
    // uncompressed tar mislabeled .zip, so `archiver()` never returns MSYS/GNU tar here.
    run(tar, ["-a", "-cf", archive, "-C", outDir, base]);
  } else {
    run(tar, ["-czf", archive, "-C", outDir, base]);
  }
  if (!existsSync(archive)) throw new Error(`archive was not created at ${archive}`);

  // A `.zip` that is not a real ZIP (or a `.tar.gz` that is not gzip) means the wrong
  // archiver ran; fail loudly rather than publish a mislabeled archive.
  const magic = readFileSync(archive).subarray(0, 4);
  const hex = [...magic].map((b) => b.toString(16).padStart(2, "0")).join(" ");
  const okZip = magic[0] === 0x50 && magic[1] === 0x4b; // "PK"
  const okGzip = magic[0] === 0x1f && magic[1] === 0x8b;
  if (ext === "zip" ? !okZip : !okGzip) {
    throw new Error(
      `archive ${archive} is not a real ${ext === "zip" ? "ZIP" : "gzip"} (magic ${hex}); ` +
        "the selected archiver did not honor the container format",
    );
  }

  const sha256 = createHash("sha256").update(readFileSync(archive)).digest("hex");
  const checksums = `${archive}.sha256`;
  writeFileSync(checksums, `${sha256}  ${basename(archive)}\n`);

  // Forward-slash paths so the workflow's bash parsing and GITHUB_OUTPUT stay portable.
  const fwd = (p) => p.replaceAll("\\", "/");
  console.log(`[package-mcp] version=${version}`);
  console.log(`[package-mcp] target=${target} (host ${host})`);
  console.log(`[package-mcp] binary=${fwd(executable)}`);
  console.log(`[package-mcp] archive=${fwd(archive)}`);
  console.log(`[package-mcp] checksums=${fwd(checksums)}`);
  console.log(`[package-mcp] sha256=${sha256}`);
  console.log(`[package-mcp] size=${statSync(archive).size}`);
}

try {
  main();
} catch (error) {
  console.error(`[package-mcp] FAILED: ${error.message}`);
  process.exit(1);
}
