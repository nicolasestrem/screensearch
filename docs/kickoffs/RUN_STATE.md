# Run state

Started: 2026-11-03 21:20
Last updated: 2026-11-04 16:25
Branch now: feat/0.4.5-p3-baseline-schema

| Task | Status | Branch | Last commit | Notes |
|---|---|---|---|---|
| 1 P1 contract | done (merged) | feat/0.4.5-p1-specs-contract | e27f42c (squash of 3d15756) | Specs-only. The maintainer merged it to main as e27f42c (#150); P2's design system merged as 7e73e30 (#151) and the records as 3976551 (#152). The overnight run itself pushed nothing. |
| 2 P1 self-test | done | feat/0.4.5-p1-specs-contract | 83e281a | `P1-selftest.md`. Round 1 fixed the merge-override wording in `03 §7h.5`; two phase-level choices logged; round 2 clean. |
| 3 P2 directions | done | feat/0.4.5-p2-design-directions | 71030f6 | `P2-directions/` A/B/C plus the later D/D2 review set. The maintainer picked the Logbook direction and merged a separate P2 design-system branch as #151. |
| 4 kickoff files | done | feat/0.4.5-p1-specs-contract | dad1eb2 | `P2.md` to `P10.md`. |
| 5 P3 schema | done (code + synced + audited), maintainer run open | feat/0.4.5-p3-baseline-schema | c039d5e (+ this records commit) | 2026-11-04: rebased cleanly onto `origin/main` 3976551; the P2 pick landed as `ui.appearance`, the 26th UI key (`31f7c66`); baseline DDL parity test added (`47b1f1b`); rebuild fixture seeds the new key (`1407e5b`); the dev-gallery stale retired-key specimen removed with a one-time grant (`bbd8976`); the Maintainer run rewritten as a nine-step procedure and the contract text corrected (`c039d5e`). Acceptance a to f met with raw output in `05`; full suite 1195 passed, 0 failed. Real-DB rebuild, per-table counts, 20-frame Moment spot check, v14 boot and the Settings > Integrations in-app check remain the maintainer's. |
| 6 P4 eval | done (code), baseline run open | feat/0.4.5-p4-auto-eval | c437297 | Unchanged today. Because P3 was rebased, P4 still carries the old P3 hashes and needs a rebase onto the new P3 tip before its turn. The Settings fix was not moved. |
| 7 P3 sync + audit (2026-11-04) | done | feat/0.4.5-p3-baseline-schema | (this records commit) | `P3-READY.md`, the `05` entry and this file. The P2 section 8 hunk was escalated and answered "land it in P3"; no push, PR or merge. |

Rules in force: no push, no PR, no merge, local commits only. Nothing touched a real database, a
model file, a token or a secret. No em dashes in new text.

Coordination note (overnight 2026-11-03): the tree is one working copy, so git was locked to the
parent after two lanes created branches on their own. Lane commits were staged by path, in this
order: P3 (336efc6), then P4 (rebased onto each new P3 tip; now c437297), then the records on the P1
branch.

Today (2026-11-04): the P3 branch was synced onto main and four read-only audit workstreams ran as
teammates on the same working tree (schema, rebuild, settings, maintainer-run docs). The Lead
committed by path, one fix per commit, in this order: `31f7c66`, `47b1f1b`, `1407e5b`, `bbd8976`,
`c039d5e`, then this records commit. Verification is pasted in `specs/05_BUILD_REVIEW.md` under
"2026-11-04: 0.4.5 P3 baseline schema + settings v2".
