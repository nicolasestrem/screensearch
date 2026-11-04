# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Added — specs

- **0.4.5 arc (P9) contract: work graph, Ask v2, insights, baseline schema v14, settings v2, v2
  design-system placeholders.** Specs-only; zero code. Roadmap: `docs/0.4.5.md`; contracts:
  `specs/03_MASTER_PRODUCTION_SPEC.md` §4, §7h–§7j, §8, §13d and `specs/UI_REFERENCE.md`.

### Added

- **Setup progress and errors are visible in the app.** While ScreenSearch prepares local AI —
  downloading and installing the llama.cpp engine, preparing the search model, or downloading a
  vision/answer model — a quiet status-bar chip reads "Preparing local AI" with the current
  stage, and Settings · Models lists each step: Checking → Downloading → Verifying → Installing →
  Done. Real progress (bytes and a percentage) appears only when it can be measured; the search
  model shows honest indeterminate progress. A failed step stays visible as "Setup failed" until
  that step runs again, with a plain cause, the next step and the technical details one click away (signed
  URL parameters are stripped). Nothing blocks capture or search, and no new download, setting
  or network destination is added. Spec: `03 §7g`.

- **A dev-only evaluation harness for search and answers.** It writes its own questions from facts
  already in your history (when you last opened a file, which sessions mentioned a repo, what you
  were doing before a session, where a line appeared again), then scores retrieval, citation
  validity, latency and entity recovery. No hand-written question set to maintain and nothing to
  label, so the next improvements are measured rather than guessed.

### Changed

- **One clean database schema, no migration chain.** The database is now created from a single
  baseline at schema version 14, and any other version refuses to open with a message that names
  the `ssv2c-rebuild` tool. That dev-only tool carries an existing history into the new file
  read-only: frames, images, text, embeddings, sessions, marks, reports and the API identity are
  copied, derived indexes are rebuilt, the old file is never modified, and renaming it to
  `*.v13.bak` stays your step. Spec: `03 §4`.
- **Settings are cut down to what a person can actually decide.** The Settings screen edits 25 keys
  instead of 78. Engine tuning moves to a documented `advanced.toml` file beside the database, nine
  retired keys are gone, and an unknown key is rewritten away once instead of being silently
  tolerated. Spec: `03 §8`.

### Fixed

- **Local inference could be permanently unavailable on a rate-limited network.** The app
  resolves its llama.cpp sidecar through GitHub's *anonymous* releases API, which is capped at
  60 requests/hour per IP. On a shared address the quota can be exhausted, and a single `403`
  left the app with no `llama-server` for the whole session — search still worked, but vision
  tagging and Ask never did. Resolution now falls back to a pinned, SHA-256-verified Windows
  Vulkan release asset (downloaded from GitHub's release-asset host, which is not subject to the
  API quota); a working API still picks the newest build, and `SSV2C_LLAMA_RELEASE_URL` still
  overrides both. The binary download itself gets up to three attempts on transient
  `401`/`403`/`408`/`429`/`5xx` and transport errors, and the failure message now carries the
  full error chain instead of only the outer context.

## Older versions

Releases 0.4.2 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
