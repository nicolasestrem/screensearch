# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Added
- README "Inspirations & prior art" section crediting screenpipe, Rewind.ai, Rem, and OpenRecall.
- Cleanup regression evidence and a reproducible local unsigned Windows installer
  build record in `specs/05_BUILD_REVIEW.md`.

### Removed
- Unused UI scaffold, search-icon alias, local-day helper, and mark-note mutation hook;
  active UI and mark-note commands are unchanged.
- Unused `thiserror` workspace declaration and the OCR crate's unused direct `tracing`
  dependency; no dependency versions changed.

## Older versions

Releases 0.4.0 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
