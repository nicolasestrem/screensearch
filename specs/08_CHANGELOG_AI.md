# 08 — AI Changelog

## 2026-10-19 — llama.cpp bootstrap recovery

- **Change:** `download.rs` now falls back to a pinned, SHA-256-verified Windows Vulkan asset
  when the anonymous GitHub releases lookup fails, gives the binary zip up to three attempts on
  transient statuses/transport errors, and exposes `ensure_binary_from_url_verified`
  plus two `#[ignore]`d real-network checks; `lib.rs` reports the full anyhow chain for an
  acquisition failure. Docs: `ARCHITECTURE §7.3`, `CHANGELOG [Unreleased]`, this record.
- **Why:** field evidence showed an installed 0.4.2 permanently without inference because the
  per-IP anonymous API quota was exhausted (403, remaining 0), and initialization runs once
  per session. The API-independent pin replaces only the resolution step; a working API still
  selects the newest release and no token is involved.
- **Verification:** native tbX Rust 1.95.0 — RED observed before the fix (403 panic), then
  `cargo test -p inference --lib` 110 passed / 2 ignored, clippy `-D warnings` clean for
  `inference` + `screensearch`, and both gated real-network checks downloaded the archive and
  ran `llama-server --help` (verbatim output in `05`).
- **Not done:** no merge, no release, no change to the installed app or its data.

## 2026-10-19 — Record v0.4.2 publication and field test

- **Change:** `05` gained the publication evidence (tag run, assets, updater endpoint
  redirect chain); `07` and `06` moved F3 native acceptance from PENDING to accepted on the
  maintainer's report.
- **Why:** the sweep entries said "no tag claimed" and left F3 acceptance pending; the release is
  now published and Nico reported a green field test. Recorded as reported, not as agent-observed.
- **Verification:** docs only; values copied from the observed release run output (`05`).

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
