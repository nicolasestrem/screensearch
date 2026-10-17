# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Changed

- Note frontmatter parsing now accepts only schema `1` or `2`; any other value is rejected as
  invalid provenance metadata (`FILE_CONFLICT`), where previously an unknown schema number was
  accepted. Affects hand-edited notes and notes written by a future build (`03 §8e`).

### Added

- **F3 G1 (internal):** report runs carry a `kind` (`daily` | `weekly_synthesis`) persisted by
  the forward-only schema 12 → 13 migration and exposed on `GET /v1/reports`; ISO-week window
  helpers and note frontmatter schema v2 for the coming weekly synthesis. No user-visible
  behavior yet (`03 §8e`).

### Changed
- **CI cost:** Claude PR review is no longer automatic. `claude-code-review.yml` runs only on a
  `/claude-review` PR comment (owner/member/collaborator) or a manual dispatch and uses Sonnet
  (`claude-sonnet-5-5`). `claude.yml` (`@claude` mentions) is unchanged.

### Security
- `claude-code-review.yml` no longer checks out PR head code in its secret-bearing comment/dispatch
  job (CodeQL `actions/untrusted-checkout` + `-toctou`, high, introduced by #134); it checks out
  the default branch and reads the PR diff via the GitHub API.

### Documentation
- **Agent token cost:** `CLAUDE.md` and `specs/04` §1 switch from "read 01–04 + UI_REFERENCE end
  to end every session" (~60k tokens) to targeted section reads; the CLAUDE.md release-history
  paragraph is condensed to a pointer (history stays in `CHANGELOG-ARCHIVE.md` /
  `specs/archive/`). CLAUDE.md documents the on-demand review triggers.
- **F3 deeper-memory contract (GO):** Nico's decisions for the deeper-memory slice are final and
  the plan (`docs/deeper-memory-f3-plan.md`) is approved (F1-pilot contingency waived) — weekly
  index-linked synthesis plus user-confirmed promotion into one accumulating project note. Kind
  token `weekly_synthesis`; persisted `kind` column (migration 12 → 13) exposed on
  `GET /v1/reports`; 4-complete-ISO-week catch-up cap; fixed weekly headings; one project note;
  weekly Telegram digest under `03 §8d` (increment G6). Normative text `02 §5i` + `03 §8e`;
  decisions ledger `specs/07_KNOWN_GAPS.md`.

## Older versions

Releases 0.4.1 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
