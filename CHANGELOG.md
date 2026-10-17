# Changelog

All notable changes to ScreenSearch V2c are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Detailed AI build records live in `specs/08_CHANGELOG_AI.md`; this file is the
> human-facing summary.

## [Unreleased]

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

## Older versions

Releases 0.4.1 and earlier are archived in [CHANGELOG-ARCHIVE.md](./CHANGELOG-ARCHIVE.md).
