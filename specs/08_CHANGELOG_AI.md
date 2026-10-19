# 08 — AI Changelog

## 2026-10-19 — v0.4.2 release-prep archive sweep (branch `release/v0.4.2`)

- **Change:** moved the 0.4.2-arc entries of `05`/`06`/`07`/`08` verbatim into
  `specs/archive/<NN>_<NAME>.v0.4.2.md` (complete pre-sweep live files plus a one-line header)
  and the `CHANGELOG.md` `[Unreleased]` block into `CHANGELOG-ARCHIVE.md` as `[0.4.2]`, adding
  the missing external-agent memory API (#126–#130) and dependency-security (#125) bullets and
  scoping the all-NO_DATA review-fix bullet to fully covered weeks (#141, squashed in `3038a60`).
  Still-open items stay live: F3 native Windows interactive acceptance (pending),
  open weekly decisions and readings (`07`), pending `06` items.
- **Why:** CLAUDE.md "Archive on release"; modelled on the v0.4.1 sweep (`6587e84`). Release
  prep only; no tag or release is claimed here.
- **Verification:** archive bodies diffed byte-identical to the pre-sweep live files (`05`).
- **Review fix (Codex `PRRT_kwDOTAuCtM6t5thb`):** the archived docs-drift follow-up for
  `docs/f3-g5-g6-verification.md` was fixed rather than dropped: its revision pointer names the
  0.4.2 archive and a dated correction records the committed UI test counts (50 at `957b234`,
  55 at v0.4.2; observed output in `05`).

> Append-only record of what the agent changed during the build, **with reasons**. One entry per
> meaningful change set. Empty until P0 begins. (This tracks build work; the design-phase history
> lives in git.)

## <date> — <short title>
- **Change:** what was added/modified.
- **Why:** the reason, tied to a spec section.
- **Verification:** the command run + verbatim result.

---

> Pre-0.2.x (v0.1.0) history → `specs/archive/08_CHANGELOG_AI.v0.1.0.md`.
> Shipped 0.2.x history (0.2.0–0.2.2) → `specs/archive/08_CHANGELOG_AI.v0.2.x.md`.
> Shipped 0.3.0 history (PR1–PR9 + bridge fixes) → `specs/archive/08_CHANGELOG_AI.v0.3.0.md`.
> Shipped 0.3.1 history (post-0.3.0 bridge fixes + PR1–PR4) →
> `specs/archive/08_CHANGELOG_AI.v0.3.1.md`.
> Shipped 0.3.2 history (PR1–PR6) → `specs/archive/08_CHANGELOG_AI.v0.3.2.md`.
> Shipped 0.3.3 history (the browser-freeze hotfix) → `specs/archive/08_CHANGELOG_AI.v0.3.3.md`.
> Shipped 0.4.0 history (the P8 sessions arc: PR1–PR7) →
> `specs/archive/08_CHANGELOG_AI.v0.4.0.md`.
> Shipped 0.4.1 history (opt-in network API #122, durable memory reports #123, maintenance) →
> `specs/archive/08_CHANGELOG_AI.v0.4.1.md`.
> Shipped 0.4.2 history (dependency/security fixes #120/#121/#125, external-agent memory API
> #126–#130, Claude workflows #133–#135, F3 deeper memory #131/#136–#141) →
> `specs/archive/08_CHANGELOG_AI.v0.4.2.md`.
> Live file holds only the current arc.

---
