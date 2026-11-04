# Run state

Started: 2026-11-03 21:20
Last updated: 2026-11-04 00:05
Branch now: feat/0.4.5-p1-specs-contract

| Task | Status | Branch | Last commit | Notes |
|---|---|---|---|---|
| 1 P1 contract | done | feat/0.4.5-p1-specs-contract | 3446d9b | Specs-only. PR #150 was opened before the overnight rules took effect and is not merged. Nothing else was pushed tonight. |
| 2 P1 self-test | done | feat/0.4.5-p1-specs-contract | 83e281a | `P1-selftest.md`. Round 1 fixed the merge-override wording in `03 §7h.5`; two phase-level choices logged; round 2 clean. |
| 3 P2 directions | done | feat/0.4.5-p2-design-directions | cdf20ba | `P2-directions/`: A Command Deck refined (3064 lines), B Paper Ledger (3333), C Signal Console (2565), plus `compare.html`. P2 stopped at the direction checkpoint as instructed. |
| 4 kickoff files | done | feat/0.4.5-p1-specs-contract | dad1eb2 | `P2.md` to `P10.md`, nine files with branch, owned paths, spec sections, verbatim acceptance, ordered verification, stop conditions. |
| 5 P3 schema | done (code), maintainer run open | feat/0.4.5-p3-baseline-schema | b431bc8 | Baseline v14, no migration chain, refusal naming `ssv2c-rebuild`, `crates/rebuild` tool, settings v2 (25 UI / 43 advanced / 9 deleted), regenerated bindings. Full suite 1241 passed, 0 failed after the QA pass. The real-database rebuild and the twenty frame spot check are the maintainer's, never run here. |
| 6 P4 eval | done (code), baseline run open | feat/0.4.5-p4-auto-eval | cc1d916 | Rebased onto the QA-fixed P3, so the branch is self-consistent. Eval runner, self-generated pairs, six metrics, determinism and read-only tests, harness tests green including 4 new ground-truth regression tests. The real baseline report needs the maintainer's database and a running app. |
| Morning report | done | feat/0.4.5-p1-specs-contract | (this commit) | `MORNING.md`. |

Rules in force tonight: no push, no PR, no merge, local commits only. Nothing touched a real
database, a model file, a token or a secret. No em dashes in new text.

Coordination note: the tree is one working copy, so git was locked to the parent after two lanes
created branches on their own. Lane commits are staged by path, in this order: P3 (336efc6), then
P4 (cc1d916, based on the P3 commit), then the records on the P1 branch.
