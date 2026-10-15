# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Added
- Manual memory report date/range selection, including today's in-progress evidence, with
  inclusive dates in the selected timezone and separate owned report files/index links.
- Payload-free memory run and delivery diagnostics record status/coverage without report text,
  credentials, recipients or output paths.
- Durable daily memory reports with exact evidence provenance, separate file/delivery receipts,
  retry versus regenerate actions, and a forward-only schema 11 → 12 migration.
- Default-off schedules with an explicit IANA timezone, DST handling and bounded restart catch-up;
  owned Markdown notes and an index preserve user edits and report file conflicts.
- Default-off Telegram text digests with Windows Credential Manager storage, bounded rejection
  retries, per-chunk receipts and explicit review of uncertain delivery.
- Approved, separately reviewed opt-in Telegram digest contract: Windows credential storage,
  saved-report rendering, durable per-chunk receipts and explicit ambiguous-delivery handling.
- Approved contract for durable daily memory reports and user-selected local Markdown output;
  implementation and native acceptance are tracked separately from the network API build.
- README "Inspirations & prior art" section crediting screenpipe, Rewind.ai, Rem, and OpenRecall.
- Cleanup regression evidence and a reproducible local unsigned Windows installer
  build record in `specs/05_BUILD_REVIEW.md`.

### Changed
- README showcases the unreleased memory reports, date/range selection, Obsidian notes,
  optional Telegram digests and configurable API binding, with setup links and release status.
- Coverage reports with captured evidence reject blank model output as generation failure,
  preserving genuine empty-window NO_DATA behavior and prompt-based no-match results.
- Memory status explains why NO_DATA reports do not send Telegram digests.
- Restored the original privacy statement in `specs/01_PROJECT_CONTEXT.md` at the
  maintainer's request; API and delivery details remain in their feature documentation.
- The opt-in HTTP API accepts a configurable IPv4/IPv6 literal bind address. Missing addresses
  default to `127.0.0.1`, including existing enabled installations, so upgrades do not broaden
  access automatically. Address/port edits apply immediately; bearer authentication, plain HTTP,
  Windows exclusive binding and MCP/inference loopback restrictions remain.
- Daily schedule input uses hour/minute precision (`HH:mm`), with no seconds field.
- New memory settings and delivery paths use forward slashes; legacy file-ownership paths
  remain recognized. Windows-only file operations and the database schema are unchanged.
- The memory/reporting execution plan now targets configurable product behavior and validation
  across supported user environments. Native acceptance remains separate from build verification.

### Removed
- Unused UI scaffold, search-icon alias, local-day helper, and mark-note mutation hook;
  active UI and mark-note commands are unchanged.
- Unused `thiserror` workspace declaration and the OCR crate's unused direct `tracing`
  dependency; no dependency versions changed.

## Older versions

Releases 0.4.0 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
