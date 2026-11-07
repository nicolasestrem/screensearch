#!/usr/bin/env node
// Generates the Tauri updater manifest (`latest.json`) from a signed `tauri build` output
// (0.3.2 PR2, #69; `03 §11b`). The updater plugin fetches this file from GitHub Releases,
// verifies the minisign signature it carries against the pubkey baked into tauri.conf.json,
// then downloads + verifies the installer at `url`. Used by both the release workflow
// (.github/workflows/release.yml) and the documented local-release fallback + the PR2 E2E
// runbook (docs/TESTING.md) — one tested code path.
//
// Usage:
//   node scripts/make-latest-json.mjs --tag vX.Y.Z [--bundle-dir DIR] [--url-base URL] [--out FILE] [--notes URL] [--expected-version X.Y.Z]
//
// Defaults: --bundle-dir target/release/bundle/nsis
//           --url-base   https://github.com/nicolasestrem/screensearch/releases/download/<tag>/
//           --out        <bundle-dir>/latest.json
//           --notes      https://github.com/nicolasestrem/screensearch/releases/tag/<tag>
//
// --expected-version is a TEST-ONLY override (the PR2 E2E runbook, docs/TESTING.md): the E2E
// builds installers with a `tauri build --config` overlay that stamps a version WITHOUT editing
// the committed tauri.conf.json, so the tag would otherwise mismatch the still-committed value.
// Passing the overlay's version here checks the tag against the version actually shipped in that
// build. The release workflow never passes it, so the strict conf-drift guard stands for releases.
//
// Hard-fails (never emits a manifest) when:
//   • --tag is missing or does not match the shipped version — `--expected-version` when given,
//     else `v<version>` from tauri.conf.json (catches the four-file hand-synced version drifting
//     from the release tag),
//   • the bundle dir does not contain exactly one `*-setup.exe`,
//   • the matching `<setup>.sig` is absent (i.e. the build was not signed — no
//     TAURI_SIGNING_PRIVATE_KEY), so an unsigned build can never yield a manifest.

import {
  existsSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, "..");
const REPO = "nicolasestrem/screensearch";

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next === undefined || next.startsWith("--")) {
        throw new Error(`missing value for --${key}`);
      }
      args[key] = next;
      i += 1;
    }
  }
  return args;
}

function fail(message) {
  console.error(`[make-latest-json] ERROR: ${message}`);
  process.exit(1);
}

function main() {
  const args = parseArgs(process.argv.slice(2));

  const tag = args.tag;
  if (!tag) {
    fail("--tag vX.Y.Z is required");
  }
  const tagVersion = tag.startsWith("v") ? tag.slice(1) : tag;

  // Confirm the tag matches the version this build actually shipped. For a release that is the
  // committed tauri.conf.json version (the four-file hand-synced value); the E2E runbook, whose
  // overlay stamps a version without touching the committed conf, passes it via --expected-version.
  let shippedVersion;
  if (args["expected-version"] !== undefined) {
    shippedVersion = args["expected-version"];
    console.log(
      `[make-latest-json] using --expected-version ${shippedVersion} (test override; skipping the committed-conf check)`,
    );
  } else {
    const conf = JSON.parse(
      readFileSync(join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
    );
    shippedVersion = conf.version;
  }
  if (shippedVersion !== tagVersion) {
    fail(
      `tag ${tag} (version ${tagVersion}) != shipped version ${shippedVersion}. ` +
        "Bump every version file before tagging (tauri.conf.json, root Cargo.toml, root + ui package.json), " +
        "or pass --expected-version <overlay version> for the E2E overlay flow.",
    );
  }

  const bundleDir =
    args["bundle-dir"] ?? join(repoRoot, "target", "release", "bundle", "nsis");
  if (!existsSync(bundleDir)) {
    fail(`bundle dir not found: ${bundleDir} (did \`tauri build\` run?)`);
  }

  // Exactly one NSIS installer, or we cannot know which to publish.
  const setups = readdirSync(bundleDir).filter((f) =>
    f.toLowerCase().endsWith("-setup.exe"),
  );
  if (setups.length === 0) {
    fail(`no *-setup.exe found in ${bundleDir}`);
  }
  if (setups.length > 1) {
    fail(`expected exactly one *-setup.exe in ${bundleDir}, found: ${setups.join(", ")}`);
  }
  const setup = setups[0];

  // The updater signature Tauri emits beside the installer when createUpdaterArtifacts is on.
  // Its absence means the build was NOT signed — refuse to emit a manifest for it.
  const sigPath = join(bundleDir, `${setup}.sig`);
  if (!existsSync(sigPath)) {
    fail(
      `no signature at ${sigPath} — the build was not signed. ` +
        "Set TAURI_SIGNING_PRIVATE_KEY (+ _PASSWORD) and rebuild with createUpdaterArtifacts enabled.",
    );
  }
  const signature = readFileSync(sigPath, "utf8").trim();
  if (!signature) {
    fail(`signature file is empty: ${sigPath}`);
  }

  const urlBase =
    args["url-base"] ??
    `https://github.com/${REPO}/releases/download/${tag}/`;
  const url = `${urlBase}${encodeURIComponent(setup)}`;
  const notes =
    args.notes ?? `https://github.com/${REPO}/releases/tag/${tag}`;

  // pub_date: RFC 3339. Passed in via --pub-date for a reproducible/CI-stamped value, else now.
  const pubDate = args["pub-date"] ?? new Date().toISOString();

  const manifest = {
    version: tagVersion,
    notes,
    pub_date: pubDate,
    platforms: {
      "windows-x86_64": { signature, url },
    },
  };

  const out = args.out ?? join(bundleDir, "latest.json");
  writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`[make-latest-json] wrote ${out}`);
  console.log(`  version:   ${tagVersion}`);
  console.log(`  installer: ${setup}`);
  console.log(`  url:       ${url}`);
}

main();
