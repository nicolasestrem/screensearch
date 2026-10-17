#!/usr/bin/env node
// Verifies a packaged `screensearch-mcp` archive on the native OS (agent-memory P2.4):
//
//   1. sha256-checks the archive against its `.sha256` file,
//   2. unpacks the archive and locates the packaged binary inside it,
//   3. executes the packaged binary's `--version`,
//   4. drives stdio `initialize` + `tools/list` as an MCP client,
//   5. performs an authenticated HTTP read (`list_marks`) through the packaged binary
//      against the real `ApiServer` + `SqliteStore` fixture
//      (`cargo build --locked -p mcp --example api_fixture`) over a disposable SQLite
//      file — real product code end to end, no synthetic HTTP responses,
//   6. relaunches the packaged binary with `--read-only` and checks its `tools/list` is the
//      read surface only (every read tool present, no mutator and no Ask tool).
//
// Usage:
//   node scripts/verify-mcp-package.mjs --archive <path> --checksums <path> \
//        --fixture-bin <path> [--expect-version <x.y.z>] [--keep]
//
// Exit code 0 only when every check passes. The OS/architecture, every command run and
// its raw output are printed; nothing is piped away.
//
// Log privacy: the fixture bearer token is a live credential and must never reach stdout.
// The fixture `ready` line is asserted to carry no token, and the verifier's only stdout
// writer (`log`) refuses any line containing the token — a log-safeness regression driven by
// the actual generated token, so a future edit that dumps a raw object fails the gate. The
// token is random per run; set SCREENSEARCH_VERIFY_FIXTURE_TOKEN to inject a known canary
// (used by the local regression to assert the captured output is token-free).
//
// Archiver selection: on Windows the CI shell is GitBash, whose PATH shadows the OS
// libarchive `bsdtar` with MSYS/GNU `tar` (which cannot read zip and misparses drive
// paths), so `archiver()` resolves `%SystemRoot%\System32\tar.exe` by absolute path. A
// `.zip` that is not a real ZIP (the mislabeled-tar failure mode) is rejected before unpack.

import { execFileSync, spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import readline from "node:readline";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/// Tools that must be present in the normal-mode `tools/list` (subset check only — the tool
/// surface grows, so an exact count is deliberately never asserted). This is the current
/// wrapper surface, including chronological and saved-report reads.
const REQUIRED_TOOLS = [
  "search_screen_history",
  "ask_screen_history",
  "get_moment",
  "list_frames",
  "get_capture_coverage",
  "list_reports",
  "get_report",
  "where_was_i",
  "list_marks",
  "add_mark",
  "list_sessions",
  "get_session",
  "ask_session",
];

/// Tools that must be present in the `--read-only` `tools/list` (subset check). The read
/// surface is the classified read allowlist only.
const REQUIRED_READ_TOOLS = [
  "search_screen_history",
  "get_moment",
  "list_frames",
  "get_capture_coverage",
  "list_reports",
  "get_report",
  "where_was_i",
  "list_marks",
  "list_sessions",
  "get_session",
];

/// Mutators / Ask tools the read-only listing must never expose.
const FORBIDDEN_READ_ONLY_TOOLS = ["add_mark", "ask_screen_history", "ask_session"];

const FIXTURE_READY_TIMEOUT_MS = 60_000;
const MCP_RESPONSE_TIMEOUT_MS = 30_000;
const PROCESS_EXIT_TIMEOUT_MS = 30_000;

const tag = "[verify-mcp-package]";

function fail(message) {
  throw new Error(message);
}

/// The fixture bearer token is a live credential, so `log` is the only stdout writer and it
/// refuses any line containing it. If a future edit dumps a raw object (or a fixture line)
/// that carries the token, the run fails here instead of leaking it into CI logs. The token is
/// installed into `logGuardToken` once generated in `main` — this is the log-safeness
/// regression, driven by the actual generated token.
let logGuardToken = null;
function log(...args) {
  const text = args
    .map((value) => (typeof value === "string" ? value : JSON.stringify(value)))
    .join(" ");
  if (logGuardToken && text.includes(logGuardToken)) {
    fail("refusing to log a line containing the fixture token (credential leak)");
  }
  process.stdout.write(`${text}\n`);
}

function parseArgs(argv) {
  const args = { archive: undefined, checksums: undefined, fixtureBin: undefined, expectVersion: undefined, keep: false };
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
    if (key === "--archive") args.archive = value();
    else if (key === "--checksums") args.checksums = value();
    else if (key === "--fixture-bin") args.fixtureBin = value();
    else if (key === "--expect-version") args.expectVersion = value();
    else if (key === "--keep") args.keep = true;
    else throw new Error(`unknown argument: ${arg} (see the header for usage)`);
  }
  for (const [key, flag] of [["archive", "--archive"], ["checksums", "--checksums"], ["fixtureBin", "--fixture-bin"]]) {
    if (!args[key]) throw new Error(`${flag} is required`);
  }
  return args;
}

function workspaceVersion() {
  const toml = readFileSync(join(repoRoot, "Cargo.toml"), "utf8");
  const block = toml.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/);
  const version = block && block[1].match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!version) fail("could not read [workspace.package] version from Cargo.toml");
  return version[1];
}

function run(command, args, options = {}) {
  log(`${tag} $ ${command} ${args.join(" ")}`);
  return execFileSync(command, args, { stdio: "inherit", ...options });
}

/**
 * The archiver to run for this host.
 *
 * On Windows the CI shell is GitBash, whose PATH puts MSYS/GNU `tar` ahead of the
 * OS-provided libarchive `bsdtar`. GNU tar cannot read zip at all and reads a `D:\...`
 * path as a remote `host:path` ("tar: Cannot connect to D: resolve failed"), so unpacking
 * a `.zip` there fails. The canonical native archiver is `%SystemRoot%\System32\tar.exe`
 * (libarchive bsdtar); resolve it by absolute path so the GitBash PATH shadow cannot pick
 * GNU tar. Elsewhere `tar` from PATH is the platform archiver (GNU tar on Linux, bsdtar on
 * macOS).
 */
function archiver() {
  if (process.platform !== "win32") return "tar";
  const root = process.env.SystemRoot || process.env.windir || "C:\\Windows";
  const native = join(root, "System32", "tar.exe");
  if (!existsSync(native)) {
    fail(
      `no native Windows archiver at ${native} (Windows 10 1803+/Server 2019+ ship ` +
        "libarchive bsdtar); refusing to fall back to MSYS/GNU tar, which cannot read zip " +
        "and misparses drive paths",
    );
  }
  return native;
}

/** A promise-based line reader for a child's stdout. */
function createLineReader(stream) {
  const rl = readline.createInterface({ input: stream, crlfDelay: Infinity });
  const lines = [];
  const waiters = [];
  let closed = false;

  rl.on("line", (line) => {
    const waiter = waiters.shift();
    if (waiter) waiter.settle(null, line);
    else lines.push(line);
  });
  rl.on("close", () => {
    closed = true;
    for (const waiter of waiters.splice(0)) {
      waiter.settle(new Error("stream closed before the expected line arrived"));
    }
  });

  return {
    next(timeoutMs, what) {
      if (lines.length > 0) return Promise.resolve(lines.shift());
      if (closed) return Promise.reject(new Error(`stream already closed while waiting for ${what}`));
      return new Promise((resolvePromise, rejectPromise) => {
        const waiter = {
          settle: (error, line) => {
            clearTimeout(timer);
            if (error) rejectPromise(error);
            else resolvePromise(line);
          },
        };
        const timer = setTimeout(() => {
          const index = waiters.indexOf(waiter);
          if (index >= 0) waiters.splice(index, 1);
          rejectPromise(new Error(`timed out after ${timeoutMs}ms waiting for ${what}`));
        }, timeoutMs);
        waiters.push(waiter);
      });
    },
  };
}

async function readJsonLine(reader, what) {
  const line = await reader.next(MCP_RESPONSE_TIMEOUT_MS, what);
  try {
    return JSON.parse(line);
  } catch {
    fail(`non-JSON line while waiting for ${what}: ${line}`);
  }
}

function waitExit(proc, timeoutMs) {
  if (proc.exitCode !== null) return Promise.resolve(proc.exitCode);
  return new Promise((resolvePromise, rejectPromise) => {
    const timer = setTimeout(
      () => rejectPromise(new Error(`process did not exit within ${timeoutMs}ms`)),
      timeoutMs,
    );
    proc.once("exit", (code) => {
      clearTimeout(timer);
      resolvePromise(code);
    });
  });
}

function findPackagedBinary(dir) {
  const wanted = process.platform === "win32" ? "screensearch-mcp.exe" : "screensearch-mcp";
  const hits = [];
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name === wanted) hits.push(path);
    }
  };
  walk(dir);
  return hits;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const archive = resolve(args.archive);
  const checksums = resolve(args.checksums);
  const fixtureBin = resolve(args.fixtureBin);
  const expectVersion = args.expectVersion || workspaceVersion();

  log(`${tag} os=${process.platform} arch=${process.arch} node=${process.version}`);
  log(`${tag} archive=${archive}`);
  log(`${tag} checksums=${checksums}`);
  log(`${tag} fixture=${fixtureBin}`);
  log(`${tag} expect-version=${expectVersion}`);

  if (!existsSync(archive)) fail(`archive not found: ${archive}`);
  if (!existsSync(checksums)) fail(`checksums file not found: ${checksums}`);
  if (!existsSync(fixtureBin)) fail(`fixture binary not found: ${fixtureBin}`);

  // --- 1. checksum ---------------------------------------------------------
  const checksumParts = readFileSync(checksums, "utf8").trim().split(/\s+/);
  const expectedSha = checksumParts[0];
  if (!/^[0-9a-f]{64}$/.test(expectedSha)) {
    fail(`checksums file does not start with a sha256 hex digest: ${checksums}`);
  }
  const actualSha = createHash("sha256").update(readFileSync(archive)).digest("hex");
  if (expectedSha !== actualSha) {
    fail(`sha256 mismatch: checksums says ${expectedSha}, archive hashes to ${actualSha}`);
  }
  const checksumName = checksumParts[1];
  if (checksumName && checksumName !== basename(archive)) {
    log(`${tag} note: checksums file names "${checksumName}" but archive is "${basename(archive)}"`);
  }
  log(`${tag} PASS checksum: sha256=${actualSha}`);

  // --- 2. unpack -----------------------------------------------------------
  const extractDir = mkdtempSync(join(tmpdir(), "mcp-verify-extract-"));
  const workDir = mkdtempSync(join(tmpdir(), "mcp-verify-fixture-"));
  const tar = archiver();
  log(`${tag} archiver=${tar}`);
  if (archive.endsWith(".zip")) {
    // Assert the container is a real ZIP before unpacking: a tar mislabeled `.zip` (what
    // MSYS/GNU tar `-a` produces) must fail here rather than silently unpack as a tar.
    const magic = readFileSync(archive).subarray(0, 4);
    const hex = [...magic].map((b) => b.toString(16).padStart(2, "0")).join(" ");
    if (!(magic[0] === 0x50 && magic[1] === 0x4b)) {
      fail(`archive is not a real ZIP (magic ${hex}); refusing to unpack a tar mislabeled .zip`);
    }
    run(tar, ["-xf", archive, "-C", extractDir]);
  } else if (archive.endsWith(".tar.gz") || archive.endsWith(".tgz")) {
    run(tar, ["-xzf", archive, "-C", extractDir]);
  } else {
    fail(`unsupported archive extension (expected .zip/.tar.gz/.tgz): ${archive}`);
  }
  const binaries = findPackagedBinary(extractDir);
  if (binaries.length !== 1) {
    fail(`expected exactly one screensearch-mcp binary in the archive, found ${binaries.length}`);
  }
  const bin = binaries[0];
  log(`${tag} PASS unpack: ${bin}`);

  // --- 3. version ----------------------------------------------------------
  const versionOut = execFileSync(bin, ["--version"], { encoding: "utf8" }).trim();
  if (versionOut !== `screensearch-mcp ${expectVersion}`) {
    fail(`--version printed "${versionOut}", expected "screensearch-mcp ${expectVersion}"`);
  }
  log(`${tag} PASS version: ${versionOut}`);

  // --- 4/5. real server fixture + packaged binary --------------------------
  let fixture = null;
  let child = null;
  let readOnlyChild = null;
  let fixtureSpawnError = null;
  try {
    // A fresh random token per run; SCREENSEARCH_VERIFY_FIXTURE_TOKEN injects a known canary
    // so the log-privacy regression can assert the captured output carries no token.
    const token =
      process.env.SCREENSEARCH_VERIFY_FIXTURE_TOKEN || randomBytes(32).toString("hex");
    // Arm the log guard now that the credential exists: no stdout line may carry it.
    logGuardToken = token;
    const dbPath = join(workDir, "fixture.db");
    log(`${tag} $ ${fixtureBin}  (SCREENSEARCH_FIXTURE_DB=${dbPath}, token=generated)`);
    fixture = spawn(fixtureBin, [], {
      env: { ...process.env, SCREENSEARCH_FIXTURE_DB: dbPath, SCREENSEARCH_FIXTURE_TOKEN: token },
      stdio: ["pipe", "pipe", "inherit"],
    });
    fixture.on("error", (error) => {
      fixtureSpawnError = error;
    });

    const fixtureReader = createLineReader(fixture.stdout);
    let ready = null;
    try {
      while (!ready) {
        const line = await fixtureReader.next(FIXTURE_READY_TIMEOUT_MS, "fixture ready line");
        let message;
        try {
          message = JSON.parse(line);
        } catch {
          log(`${tag} fixture (non-JSON stdout): ${line}`);
          continue;
        }
        if (message.event === "ready") ready = message;
        else log(`${tag} fixture: ${line}`);
      }
    } catch (error) {
      if (fixtureSpawnError) fail(`fixture failed to start: ${fixtureSpawnError.message}`);
      fail(`fixture did not report ready: ${error.message}`);
    }
    // The `ready` line is a status line only: it must never carry the token. The verifier's
    // HTTP sanity check below (401 anonymous, 200 authenticated) is what proves the token works.
    if (ready.token !== undefined) {
      fail("fixture ready line must not carry the token (credential leak)");
    }
    if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(ready.url)) fail(`fixture url is not loopback: ${ready.url}`);
    // Log only the explicit, non-credential ready fields — never the raw object.
    log(
      `${tag} PASS fixture ready: url=${ready.url} db=${ready.db} ` +
        `frame_id=${ready.frame_id} mark_id=${ready.mark_id} note=${JSON.stringify(ready.note)}`,
    );

    // Fixture sanity: the server really requires the bearer token (401 anonymous, 200
    // authenticated) before we ask the packaged binary to read through it.
    const anonymous = await fetch(`${ready.url}/v1/marks`, { signal: AbortSignal.timeout(10_000) });
    if (anonymous.status !== 401) fail(`fixture should reject an anonymous read with 401, got ${anonymous.status}`);
    const authenticated = await fetch(`${ready.url}/v1/marks`, {
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(10_000),
    });
    if (authenticated.status !== 200) fail(`fixture should accept the bearer token with 200, got ${authenticated.status}`);
    const authenticatedBody = await authenticated.json();
    if (!Array.isArray(authenticatedBody) || authenticatedBody.length !== 1) {
      fail(`fixture should hold exactly 1 seeded mark, got ${JSON.stringify(authenticatedBody)}`);
    }
    log(`${tag} PASS fixture auth sanity: anonymous=401 authenticated=200 marks=1`);

    // The packaged binary, pointed at the fixture over HTTP.
    log(`${tag} $ ${bin}  (SCREENSEARCH_API_URL=${ready.url}, token=fixture token)`);
    child = spawn(bin, [], {
      env: { ...process.env, SCREENSEARCH_API_URL: ready.url, SCREENSEARCH_API_TOKEN: token },
      stdio: ["pipe", "pipe", "inherit"],
    });
    const reader = createLineReader(child.stdout);
    let nextId = 1;
    const request = async (method, params) => {
      const id = nextId++;
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      for (;;) {
        const message = await readJsonLine(reader, `${method} response`);
        if (message.id === id) {
          if (message.error) fail(`${method} returned a JSON-RPC error: ${JSON.stringify(message.error)}`);
          return message.result;
        }
        log(`${tag} (ignoring message while waiting for ${method}): ${JSON.stringify(message)}`);
      }
    };

    const init = await request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "verify-mcp-package", version: "1.0.0" },
    });
    if (init.protocolVersion !== "2025-06-18") fail(`initialize negotiated ${init.protocolVersion}`);
    if (!init.serverInfo || init.serverInfo.name !== "screensearch-mcp") {
      fail(`initialize serverInfo.name is ${init.serverInfo && init.serverInfo.name}`);
    }
    if (typeof init.capabilities?.tools !== "object" || init.capabilities.tools === null) {
      fail("initialize did not advertise the tools capability");
    }
    log(`${tag} PASS initialize: ${JSON.stringify(init)}`);
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);

    const listed = await request("tools/list", {});
    const tools = listed && listed.tools;
    if (!Array.isArray(tools) || tools.length === 0) fail("tools/list returned no tools");
    const names = tools.map((tool) => tool.name);
    const missing = REQUIRED_TOOLS.filter((name) => !names.includes(name));
    if (missing.length > 0) {
      fail(`tools/list is missing required tools: ${missing.join(", ")} (got: ${names.join(", ")})`);
    }
    for (const tool of tools) {
      if (!tool.inputSchema || tool.inputSchema.type !== "object") {
        fail(`tool ${tool.name} has no object inputSchema`);
      }
    }
    log(`${tag} PASS tools/list: ${tools.length} tools (${names.join(", ")})`);

    const call = await request("tools/call", { name: "list_marks", arguments: {} });
    if (call.isError === true) fail(`list_marks returned isError: ${JSON.stringify(call)}`);
    const block = call.content && call.content[0];
    if (!block || block.type !== "text" || typeof block.text !== "string") {
      fail(`list_marks content is not a text block: ${JSON.stringify(call)}`);
    }
    log(`${tag} list_marks raw result: ${block.text}`);
    let marks;
    try {
      marks = JSON.parse(block.text);
    } catch {
      fail(`list_marks text is not JSON: ${block.text}`);
    }
    const mine = Array.isArray(marks) ? marks.find((mark) => mark.mark_id === ready.mark_id) : undefined;
    if (!mine) fail(`list_marks did not contain the seeded mark ${ready.mark_id}: ${block.text}`);
    if (mine.frame_id !== ready.frame_id) fail(`seeded mark frame_id ${mine.frame_id} != ${ready.frame_id}`);
    if (mine.note !== ready.note) fail(`seeded mark note ${JSON.stringify(mine.note)} != ${JSON.stringify(ready.note)}`);
    if (mine.resolved_at !== null) fail(`seeded mark should be unresolved, got resolved_at=${mine.resolved_at}`);
    log(`${tag} PASS authenticated read (list_marks): ${JSON.stringify(mine)}`);

    // Closing stdin must end the packaged binary cleanly.
    child.stdin.end();
    const code = await waitExit(child, PROCESS_EXIT_TIMEOUT_MS);
    if (code !== 0) fail(`packaged binary exited ${code} after stdin close, expected 0`);
    log(`${tag} PASS stdin-close exit: code=${code}`);

    // --- 6. read-only listing ----------------------------------------------
    // The same packaged binary, relaunched with `--read-only`: its `tools/list` must carry
    // every classified read tool and no mutator or Ask tool.
    log(`${tag} $ ${bin} --read-only  (tools/list must be the read surface only)`);
    readOnlyChild = spawn(bin, ["--read-only"], {
      env: { ...process.env, SCREENSEARCH_API_URL: ready.url, SCREENSEARCH_API_TOKEN: token },
      stdio: ["pipe", "pipe", "inherit"],
    });
    const roReader = createLineReader(readOnlyChild.stdout);
    let roNextId = 1;
    const roRequest = async (method, params) => {
      const id = roNextId++;
      readOnlyChild.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      for (;;) {
        const message = await readJsonLine(roReader, `${method} response (read-only)`);
        if (message.id === id) {
          if (message.error) {
            fail(`${method} (read-only) returned a JSON-RPC error: ${JSON.stringify(message.error)}`);
          }
          return message.result;
        }
        log(`${tag} (ignoring message while waiting for ${method} (read-only)): ${JSON.stringify(message)}`);
      }
    };

    await roRequest("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "verify-mcp-package", version: "1.0.0" },
    });
    readOnlyChild.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
    const roListed = await roRequest("tools/list", {});
    const roTools = roListed && roListed.tools;
    if (!Array.isArray(roTools) || roTools.length === 0) fail("read-only tools/list returned no tools");
    const roNames = roTools.map((tool) => tool.name);
    const roMissing = REQUIRED_READ_TOOLS.filter((name) => !roNames.includes(name));
    if (roMissing.length > 0) {
      fail(`read-only tools/list is missing required read tools: ${roMissing.join(", ")} (got: ${roNames.join(", ")})`);
    }
    const roForbidden = FORBIDDEN_READ_ONLY_TOOLS.filter((name) => roNames.includes(name));
    if (roForbidden.length > 0) {
      fail(`read-only tools/list must not expose: ${roForbidden.join(", ")} (got: ${roNames.join(", ")})`);
    }
    for (const tool of roTools) {
      if (!tool.inputSchema || tool.inputSchema.type !== "object") {
        fail(`read-only tool ${tool.name} has no object inputSchema`);
      }
    }
    log(`${tag} PASS read-only tools/list: ${roTools.length} tools (${roNames.join(", ")})`);

    readOnlyChild.stdin.end();
    const roCode = await waitExit(readOnlyChild, PROCESS_EXIT_TIMEOUT_MS);
    if (roCode !== 0) fail(`packaged binary (read-only) exited ${roCode} after stdin close, expected 0`);
    log(`${tag} PASS read-only stdin-close exit: code=${roCode}`);
  } finally {
    if (readOnlyChild && readOnlyChild.exitCode === null) {
      try {
        readOnlyChild.stdin.end();
      } catch {
        // stdin may already be closed
      }
      readOnlyChild.kill();
      await waitExit(readOnlyChild, 5_000).catch(() => {});
    }
    if (child && child.exitCode === null) {
      try {
        child.stdin.end();
      } catch {
        // stdin may already be closed
      }
      child.kill();
      await waitExit(child, 5_000).catch(() => {});
    }
    if (fixture && fixture.exitCode === null) {
      try {
        fixture.stdin.end();
      } catch {
        // stdin may already be closed
      }
      fixture.kill();
      await waitExit(fixture, 5_000).catch(() => {});
    }
    if (args.keep) {
      log(`${tag} kept: extract=${extractDir} work=${workDir}`);
    } else {
      rmSync(extractDir, { recursive: true, force: true });
      rmSync(workDir, { recursive: true, force: true });
    }
  }

  log(`${tag} ALL CHECKS PASSED os=${process.platform} arch=${process.arch} archive=${archive}`);
}

main().catch((error) => {
  // Redact rather than throw on the error path, so a failure can never echo the token.
  const message = logGuardToken
    ? String(error.message).split(logGuardToken).join("<redacted>")
    : String(error.message);
  console.error(`${tag} FAILED: ${message}`);
  process.exit(1);
});
