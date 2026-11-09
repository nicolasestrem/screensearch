# Big cleanup of /specs/ (continue the work already started). Headless: no one will answer questions.

Context: v0.4.5 is released and tagged. A first pass already moved the post-release v0.4.5 QA entries out of `specs/05_BUILD_REVIEW.md`, `specs/07_KNOWN_GAPS.md` and `specs/08_CHANGELOG_AI.md` into `specs/archive/*.v0.4.5.md` (uncommitted, see `git diff -- specs`). Keep that work.

Follow the repository's CLAUDE.md for what each spec file is for. Work on the current branch. Do NOT commit, push, merge or open a PR. Do not touch anything outside `specs/` (except reading code to verify claims). Do not touch `Your-Universe/`.

## Goal
Make the live specs lean, current and true for the next arc, without losing history.

## Tasks
1. `07_KNOWN_GAPS.md`: for every remaining item in the carried-forward sections (2026-11-03, 2026-10-19, 2026-10-15, 2026-09-09 maintenance, npm advisory), check the code and git history to decide whether it is resolved, superseded or still open. Move resolved or superseded items to `specs/archive/07_KNOWN_GAPS.v0.4.5.md` with a one-line note of the evidence (commit, PR or file:line). Keep genuinely open items, grouped under one clear "Open, carried forward" section with owner and a one-line status each. If you cannot establish the status with evidence, keep the item open and say so.
2. `06_PATCH_PLAN.md`: same treatment for the "Still open from earlier arcs" table.
3. `TODO.md`: keep deferred items that still apply; mark or archive those already implemented, with evidence.
4. `01_PROJECT_CONTEXT.md` §8 Status: tighten it so it describes the state after v0.4.5 in a short, current form; move long historical narrative of past arcs into `specs/archive/` (new file `01_STATUS_HISTORY.md` if needed) and keep a pointer.
5. `04_CLAUDE_CODE_BUILD_PROMPT.md`: update the §1 current-arc pointer so the next session is not pointed at finished 0.4.5 phases; leave the rules untouched.
6. `03_MASTER_PRODUCTION_SPEC.md`, `02_STRATEGIC_PLAN.md`, `UI_REFERENCE.md`: do NOT rewrite normative content. Only fix clearly stale cross-references, broken section pointers and statements contradicted by shipped code, each listed in your report. No product decisions.
7. Encoding: make sure every touched file is valid UTF-8 and that no mojibake was introduced.
8. Append one entry to `specs/08_CHANGELOG_AI.md` "Current arc" describing this cleanup.

## Report (final message)
A table per file: what moved where, what was kept open and why, every cross-reference fixed, and the `git diff --stat -- specs` output. List anything you were unsure about instead of guessing.
