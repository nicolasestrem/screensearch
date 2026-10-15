# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Added
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
- Reviewed contract for an opt-in network HTTP API, including an upgrade notice for
  existing enabled installations. Implementation and native acceptance are tracked separately.
- Persistent upgrade notice for existing enabled API profiles, with explicit acknowledgement
  in the visible main window and guidance for disabling access.
- README "Inspirations & prior art" section crediting screenpipe, Rewind.ai, Rem, and OpenRecall.
- Cleanup regression evidence and a reproducible local unsigned Windows installer
  build record in `specs/05_BUILD_REVIEW.md`.

### Changed
- Restored the original privacy statement in `specs/01_PROJECT_CONTEXT.md` at the
  maintainer's request; API and delivery details remain in their feature documentation.
- The opt-in HTTP API now listens on all IPv4 interfaces (`0.0.0.0`). It remains off by
  default, requires bearer authentication, and uses plain HTTP. Existing enabled installations
  retain their enabled state and token; reachable token holders can read history. MCP and
  inference still use loopback. No firewall or router settings are changed automatically.
- The memory/reporting execution plan now targets configurable product behavior and validation
  across supported user environments. Native acceptance remains separate from build verification.

### Removed
- Unused UI scaffold, search-icon alias, local-day helper, and mark-note mutation hook;
  active UI and mark-note commands are unchanged.
- Unused `thiserror` workspace declaration and the OCR crate's unused direct `tracing`
  dependency; no dependency versions changed.

## Older versions

Releases 0.4.0 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
