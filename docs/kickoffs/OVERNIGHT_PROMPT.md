# Overnight run instructions: ScreenSearch 0.4.5 wave A

You are an autonomous coding agent working in the ScreenSearch repository.
You run unattended overnight. Nobody will answer questions until morning.
This file is your task list. Re-read it whenever you lose track of what you were doing.

---

## 0. Absolute rules

Violating any of these means stop and write what happened to `docs/kickoffs/RUN_STATE.md`.

1. **Never push. Never merge. Never open a pull request. Never commit to `main`.**
   Commit locally, on the feature branches named below, and nothing else.
2. **Never touch** any `.db` / `.sqlite` file, any model file (`.gguf`, `.onnx`), any `.env`,
   any token or secret. Never run the app against real user data.
3. **Stop at ambiguity.** If `specs/` or `docs/0.4.5.md` is silent or self-contradictory on a
   *product* decision, do NOT invent an answer. Append the question to `specs/07_KNOWN_GAPS.md`
   (if silent) or `specs/06_PATCH_PLAN.md` (if contradictory), then skip that item and continue
   with the next item that does not depend on it.
4. **Verbatim verification.** Any claim that something builds or passes must be backed by the raw
   command output, pasted into `specs/05_BUILD_REVIEW.md`. Never paraphrase output. Never write
   "tests pass" without the output.
5. **No stubs, no placeholders, no hardcoded expected values** to make something look finished.
6. **Never use the em dash character** in anything you write. Use commas, colons, semicolons,
   parentheses or separate sentences.
7. This is **Windows / PowerShell**. `&&` is not a valid separator. Use `;` or run commands one
   at a time. Node 22, Rust 1.95.0 (pinned in `rust-toolchain.toml`).

---

## 1. Read before doing anything

In this order:

1. `CLAUDE.md`
2. `specs/01_PROJECT_CONTEXT.md` (in full, it is short)
3. `specs/04_CLAUDE_CODE_BUILD_PROMPT.md`, sections 1, 3, 5, 7
4. `docs/0.4.5.md` (in full, this is the arc roadmap the tasks below come from)

For `specs/02_STRATEGIC_PLAN.md`, `specs/03_MASTER_PRODUCTION_SPEC.md` and
`specs/UI_REFERENCE.md`: list the headings first (search for lines starting with `#`) and read
only the sections a task names, plus anything those sections cross-reference.
**Never read `03` end to end**, it is about 2000 lines.

---

## 2. Progress file (do this before task 1, and update it after every task)

Create `docs/kickoffs/RUN_STATE.md` and keep it current. After finishing or abandoning each
task, rewrite it so it always reflects reality:

```
# Run state
Started: <timestamp>
Last updated: <timestamp>
Branch now: <branch>

| Task | Status | Branch | Last commit | Notes |
|---|---|---|---|---|
| 1 P1 contract | not started / in progress / done / blocked | | | |
| 2 P1 self-test | | | | |
| 3 P2 directions | | | | |
| 4 kickoff files | | | | |
| 5 P3 schema | | | | |
| 6 P4 eval | | | | |
```

If you lose context, read this file plus this one, and resume at the first task that is not done.

---

## 3. Tasks, in this exact order

### Task 1: P1 specs contract

Branch: `feat/0.4.5-p1-specs-contract`, created from `main`.

Do everything listed under "P1: Specs contract" in `docs/0.4.5.md`. In particular:

- Normalize decisions **D1 to D18** from `docs/0.4.5.md` section 6 into the specs, as contract
  language, not as a copy of the roadmap.
- `specs/03_MASTER_PRODUCTION_SPEC.md` section 4: write the **complete baseline DDL v14**,
  including the new tables named in the roadmap (`entities`, `entity_aliases`,
  `session_entities`, `threads`, `thread_sessions`, `thread_overrides`, `session_vectors`,
  `insights`, `insight_evidence`, `ask_threads`, `ask_turns`).
- `specs/03` section 8: write the **settings v2 key table**. To build it, read the real current
  key set from `crates/traits` and `ui/src/bindings/Settings.ts`, and give every existing key a
  disposition: `UI`, `file-only` or `deleted`, with its default. Target: at most 25 keys in the
  UI outside Advanced.
- Add new sections `03 §7h` (work graph), `§7i` (Ask v2), `§7j` (insights), per the roadmap.
- `specs/UI_REFERENCE.md`: leave sections 1 and 2 as clearly marked placeholders that say the
  design direction is pending the maintainer's pick. Do not invent a palette or a type stack.
- Update `specs/01` (single user), `specs/02` (new section 5k), `specs/04` (0.4.5 pointer,
  wave model, source-of-truth row), `CLAUDE.md` and `AGENTS.md` (baseline schema rule for this
  arc, current state), `specs/07_KNOWN_GAPS.md`, `CHANGELOG.md`, `specs/08_CHANGELOG_AI.md`.

No code changes in this task. Commit: `specs: 0.4.5 P1 contract`.

### Task 2: P1 self-test (do not skip this)

Still on the P1 branch.

Act as a fresh agent that has never seen `docs/0.4.5.md`. Using **only** `specs/` and `CLAUDE.md`,
write down every question you would have to ask before you could implement P3, then P4, then P5.
Save the list as `docs/kickoffs/P1-selftest.md`.

If the list is not empty: fix the specs so each question is answered, or, if a question is a real
product decision only the maintainer can make, log it in `specs/07_KNOWN_GAPS.md`. Commit, then
run the self-test again. Stop after at most 3 rounds and record the final list, even if it still
has entries.

### Task 3: P2 design directions (highest priority after the contract)

Branch: `feat/0.4.5-p2-design-directions`, created from the P1 branch.

The maintainer must be able to pick a design direction first thing in the morning. He cannot run
a build or a dev server to do it. So produce **static, self-contained HTML files**, not an app
route. Playwright is not installed; do not install it, and do not take screenshots.

Create `docs/kickoffs/P2-directions/` containing:

- `direction-a.html`, `direction-b.html`, `direction-c.html`
- `compare.html`

Each direction file must:

- be **one file**, with its CSS inline, no build step, no external network request, no CDN link,
  no web font download. System fonts only at this stage (the real P2 picks bundled fonts later).
- define its own colours, type scale, spacing and radii as CSS custom properties at the top of
  the file, in one clearly commented block, so the maintainer can see the whole system at a glance.
- render the **same five realistic screens**, stacked vertically, with plausible ScreenSearch
  content (not "Lorem ipsum"):
  1. a Settings page with a category sidebar and about 8 setting rows, including a toggle, a
     select, a text field, a slider and a destructive action
  2. a search result list with about 6 results, each with a timestamp, an app name, a snippet
     with a highlighted match and a thumbnail placeholder
  3. a session card with a title, time range, duration, the tools or apps involved and a summary
  4. an Ask answer with streamed-looking prose and inline citations that look clickable
  5. a form state gallery: every control in default, hover, focus, disabled, error and loading
- include a dark and a light variant if that direction has both, switchable with a visible button
  at the top of the file using a `data-theme` attribute.
- state its name and a three-line description of its point of view at the very top of the page.
- be genuinely **different** from the other two in structure and feel, not the same layout with
  three palettes. One of the three may evolve the existing warm graphite "Command Deck" identity
  described in the current `specs/UI_REFERENCE.md` section 1; the other two must not.
- meet WCAG AA contrast for text against its own backgrounds.

`compare.html` must be one self-contained file that shows the three directions side by side in
iframes, at a width where each is still readable, with each direction's name, description, and its
font and colour choices listed underneath, plus a line saying how to view one full screen.

Then **stop P2**. Do not write any token file, Tailwind config, primitive component or app route.
Those belong to the real P2 after the maintainer picks.

Commit: `docs: 0.4.5 P2 design directions for review`.

### Task 4: kickoff files for every phase

Branch: back on `feat/0.4.5-p1-specs-contract`.

For each phase P2 through P10, write `docs/kickoffs/P<n>.md`. Each file must stand alone as the
only instruction a weaker model will ever receive for that phase, and must contain:

- the branch name
- the exact file paths that phase owns (copy them from `docs/0.4.5.md` section 4)
- the exact spec sections to read, with their section numbers
- the acceptance criteria, copied verbatim from the roadmap
- the verification commands to run, in order (see "Verification" below)
- explicit stop conditions: what must make that phase halt and ask
- for P3 and P4, a "Maintainer run" section listing the commands the maintainer runs himself
  against his real database, which an agent must never run

Commit: `docs: 0.4.5 phase kickoff files`.

### Task 5: P3 baseline schema and settings v2

Branch: `feat/0.4.5-p3-baseline-schema`, created from the P1 branch.
Implement `docs/kickoffs/P3.md`.

The rebuild tool must be tested only against a **synthetic v13 fixture database that you create
inside a test**. Never against a real database, and never against any file outside the repository.
Write the real-database procedure into the "Maintainer run" section of `docs/kickoffs/P3.md`
instead of performing it.

Run the full verification below. Paste the output into `specs/05_BUILD_REVIEW.md`.

### Task 6: P4 automatic evaluation harness

Branch: `feat/0.4.5-p4-auto-eval`, created from the P1 branch.
Implement `docs/kickoffs/P4.md`, against a fixture database only.

**No hand-written question set. No manual labelling of anything.** This is decision D17: the
harness generates its own question and answer pairs from facts already present in the database.
If you find yourself about to ask the maintainer to write questions or label sessions, you have
misread the task.

Run the full verification below. Paste the output into `specs/05_BUILD_REVIEW.md`.

---

## 4. Verification (for any task that changes code)

Run these in order, from the repository root, one at a time. UI first: the Rust build embeds
`ui/dist`, so cargo fails if the UI has not been built.

```
cd ui
npm ci
npm run lint
npm run build
cd ..
node scripts/stage-mcp.mjs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
git diff --exit-code -- ui/src/bindings
```

If `cargo test` regenerates the TypeScript bindings, commit the regenerated files.
Paste the raw output of each command into `specs/05_BUILD_REVIEW.md` under the task's heading.

---

## 5. If something fails

Try to fix it. If you cannot fix it after a reasonable number of attempts:

1. Paste the raw failing output into `specs/05_BUILD_REVIEW.md`.
2. Commit whatever part of the work is correct and self-consistent.
3. Update `docs/kickoffs/RUN_STATE.md` with status `blocked` and one sentence on why.
4. Move on to the next task that does not depend on the failed one.

Never delete or rewrite someone else's work to make your own task pass. Never weaken a test,
a lint rule or an acceptance criterion to get a green result.

---

## 6. Last thing you do: the morning report

Write `docs/kickoffs/MORNING.md`, one page maximum, on the P1 branch:

- One line per task: done, partial or blocked, with its branch and last commit hash.
- The three design directions: name and one line each, and the exact command to open
  `compare.html` (`start docs\kickoffs\P2-directions\compare.html` on Windows).
- The final question list from the P1 self-test.
- Every question you logged in `specs/07_KNOWN_GAPS.md` or `specs/06_PATCH_PLAN.md` tonight.
- The maintainer actions needed at Gate A, in order, with exact commands.
- Anything you deliberately did not do, and why.

Then stop. Do not push.
