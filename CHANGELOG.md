# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

### Added

- **F3 G1:** report runs carry a `kind` (`daily` | `weekly_synthesis`) persisted by the
  forward-only schema 12 → 13 migration and exposed as a required field on `GET /v1/reports`
  summaries and details; ISO-week window helpers and note frontmatter schema 2 (`03 §8e`).
- **F3 G2:** weekly synthesis runs are generated from that week's saved daily reports through
  the shared report engine, with truthful NO_DATA / PARTIAL / OK / BLOCKED / FAILED statuses
  that name any local date missing complete daily coverage and record the consumed
  `daily_run_ids`.
- **F3 G3:** weekly notes at `ScreenSearch/Weekly/YYYY-Www.md` (frontmatter schema 2) and
  weekly entries in `ScreenSearch/Index.md` linking the week's note and the daily notes it
  consumed; NO_DATA weeks write no note. Daily notes stay schema 1.
- **F3 G4:** user-confirmed promotion of selected report items into one user-created project
  note under `ScreenSearch/Projects/` — a dated, provenance-carrying block per touched section,
  appended in one atomic write; an edit between preview and confirm surfaces as a conflict. No
  HTTP/MCP surface.
- **F3 G5:** default-off weekly schedule (own time, IANA zone and schedule ID), Run most recent
  complete week, restart catch-up of the four most recent due weeks oldest-first within a
  six-week scan, and visible missed weeks (including failed scheduled weeks).
- **F3 G6:** weekly Telegram digest under its own default-off consent: `YYYY-Www` header, saved
  status and coverage, masked and chunked saved synthesis; no second model.

### Changed

- Note frontmatter parsing now accepts only schema `1` or `2`; any other value is rejected as
  invalid provenance metadata (`FILE_CONFLICT`), where previously an unknown schema number was
  accepted. Affects hand-edited notes and notes written by a future build (`03 §8e`).
- **F3 review fixes:** a week whose saved dailies are all NO_DATA is saved NO_DATA instead of
  OK; the saved-report shape on `GET /v1/reports/{run_id}` and MCP `get_report` no longer
  carries the internal `daily_run_ids` field; saved reports downloaded from the Memory panel
  name their kind (`…-weekly-synthesis-…`); a project note starting with a byte-order mark keeps
  its sections; hidden dot-files in `Projects/` no longer count as a second note; a weekly
  digest never counts a named partial day as covered.
- **CI cost:** Claude PR review is no longer automatic. `claude-code-review.yml` runs only on a
  `/claude-review` PR comment (owner/member/collaborator) or a manual dispatch and uses Sonnet
  (`claude-sonnet-5-5`). `claude.yml` (`@claude` mentions) is unchanged.

### Security
- `claude-code-review.yml` no longer checks out PR head code in its secret-bearing comment/dispatch
  job (CodeQL `actions/untrusted-checkout` + `-toctou`, high, introduced by #134); it checks out
  the default branch and reads the PR diff via the GitHub API.

### Documentation
- **Current state moved out of `CLAUDE.md`:** release, DB schema and merged-increment status now
  live in `specs/01_PROJECT_CONTEXT.md §8` (schema 13, F3 G1–G6 merged); `CLAUDE.md` keeps
  agent instructions and a pointer. `docs/ARCHITECTURE.md` names the v1→v13 migration chain.
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
