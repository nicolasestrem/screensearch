# P1 self-test: fresh-agent question list

Method: act as an agent that has never read `docs/0.4.5.md`. Read `CLAUDE.md`, `specs/01` in full,
the headings of `02` / `03` / `UI_REFERENCE`, then only the sections a phase needs. Write down every
question that must be answered before writing code for P3, then P4, then P5. Answer each from the
specs. Any question the specs cannot answer is a defect: fix the specs, or log a real product
question in `specs/07_KNOWN_GAPS.md`.

## Round 1 (2026-11-03): 4 questions

| # | Phase | Question | Resolution |
|---|---|---|---|
| 1 | P3 | Does `api.read_token` count against the 25-key UI ceiling? | Answered by the maintainer at P1: it shares the `api.token` row (`03 §8`). No action. |
| 2 | P5 | For a `merge` override, is the fold driven by thread ids or by `payload.session_ids`, and which thread survives? | Defect: `§7h.5` and the `thread_overrides` DDL comment could be read two ways. Fixed in `03 §7h.5`: `target_thread_id` is the survivor, the fold follows `thread_sessions`, `payload.session_ids` is the recorded member set. |
| 3 | P6 | Are the per-stage candidate budgets in `§7i.2` numeric? | Logged in `07` (2026-11-03, P1 self-test residue): order is normative, no numeric ceiling; measurement is P6's, a gate cap is the maintainer's call at Gate A. Does not block starting P6. |
| 4 | P8 | Is the JSON shape of `insights.payload` fixed per kind? | Logged in `07` (same bullet): five kinds and the render-without-model rule are contract, the per-kind shape is P8's additive decision with a fixture test per kind. Does not block starting P8. |

## Round 2 (after the fixes): 0 new unanswered questions

Re-ran the same pass over P3, P4, P5 with the round-1 fixes applied:

- **P3 answerable from `03 §4` + `§8` alone.** Every table has typed columns, PKs, FKs, CHECKs and
  indexes; `schema_version = 14`; the refusal message names `ssv2c-rebuild`; the `*.v13.bak` rename
  happens only after the maintainer confirms; the settings table gives every key a disposition and a
  default; `advanced.toml` location, shape and strictness are stated.
- **P4 answerable from `§13d` + the roadmap acceptance line.** The harness is dev-only and read-only,
  runs against a fixture in this repository, and generates its own pairs from facts in the DB
  (0.4.5 D17). No question set, no labelling, no golden file.
- **P5 answerable from `§7h` + `§4` alone.** Six entity kinds, extraction sources and confidence,
  the linking rule, the clustering threshold and decay, `cluster_key` determinism, override
  precedence, backfill resumability, and the four job kinds are all stated.

## Conclusion

P1 is implementable from `specs/` alone for the wave-A phases. The two residual items are phase-level
design choices with an owner and a rule, not gaps a fresh agent would have to guess.
