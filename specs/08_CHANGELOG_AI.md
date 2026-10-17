# 08 — AI Changelog

## 2026-10-17 — Fix untrusted PR checkout in on-demand Claude review (CI security)

- **Change:** `claude-code-review.yml` checks out the default branch instead of
  `refs/pull/<n>/head`; the review reads the diff via the GitHub API.
- **Why:** #134 made the review comment/dispatch-triggered, which runs with secrets
  (`CLAUDE_CODE_OAUTH_TOKEN`, `id-token: write`); checking out fork PR code there let a PR plant
  `.claude/settings.json` hooks or CLAUDE.md instructions that Claude Code would load. CodeQL flagged
  `actions/untrusted-checkout/high` + `actions/untrusted-checkout-toctou/high` (alerts #3, #4).
  #134 was merged before its checks finished — process miss; wait for checks before merging.
- **Verification:** `actionlint 1.7.7` exit 0; CodeQL re-scan on the fix PR.

## 2026-10-17 — F3 deeper-memory plan corrected after review (docs only)

- **Change:** corrected `docs/deeper-memory-f3-plan.md` (PR #131): renamed the proposed kind
  away from the existing `ReportKind::Weekly`; stated that `kind` is new persisted/wire surface,
  not an existing field; scoped the slice to one project note; described the index/path/provenance
  writer work; fixed heading names, the symlink-rule citation and the retention wording. Logged the
  open decisions in `07`.
- **Why:** two valid review threads (r4303787850, r4303788563) and a citation check against code
  and specs at `e55f2cc`. `02`/`03` untouched; no schema or code change.
- **Verification:** docs only; each cited `path:line` was re-read at the PR head. No build run.

## 2026-10-17 — Claude cost reduction: on-demand PR review + targeted spec reads (CI/docs)

- **Change:** `claude-code-review.yml` no longer runs on `pull_request`; it runs only on a
  `/claude-review` PR comment from an owner/member/collaborator or a manual `workflow_dispatch`
  (PR number input), checks out the PR head, and pins `--model claude-sonnet-5-5`. `CLAUDE.md` and
  `04 §1` replace the "read 01–04 + UI_REFERENCE end to end every session" rule with targeted
  section reads; the CLAUDE.md release-history paragraph is condensed to a pointer to
  `CHANGELOG-ARCHIVE.md` / `specs/archive/` (no history lost).
- **Why:** Nico approved (A–D) after cost review: ~50 automatic multi-agent reviews in 3 days
  (every push, Dependabot, docs-only PRs) and ~60k tokens of mandatory spec reading per session.
- **Verification:** `actionlint 1.7.7` on both Claude workflows exit 0. The comment trigger only
  takes effect once the workflow is on `main` (`issue_comment` uses the default-branch file).

## 2026-10-17 — P3 saved-report read contract and visibility resolution (docs/policy ledger)

- **Change:** resolved the stale "awaiting Nico / P3 only" saved-output visibility hold across the
  live ledgers as **approved/superseded** (historical records preserved): saved report output is
  exposed by persisted presence, including while a run is queued or retrying for delivery; absent
  output is explicit `null`; stored `NO_DATA` stays factual; a malformed saved payload is a read
  error (`03 §7f.10`, Nico GO). Documented the exact P3 report read contract — `GET /v1/reports`
  and `GET /v1/reports/{run_id}` inputs, wire DTOs (`ReportSummary`/`ReportPage`/`ReportDetail`/
  `SavedReport`), provenance, nullability, privacy exclusions, error mapping, half-open window
  overlap and the descending `(created_at, run_id)` tuple cursor with live-status semantics — in
  `docs/API.md`, and recorded the `list_reports`/`get_report` tools and stacked-branch status in
  `docs/MCP.md`. Recorded the P3 verification layers (genuine `ApiServer`+`SqliteStore` fixture vs
  raw-HTTP protocol fixtures, the independent expected manifest/driver, the model-unloaded /
  schedules-off no-mutation gate) and the pending GUI/native and cross-machine gates in
  `docs/TESTING.md`; updated `README.md` and `docs/memory-api-direction-plan.md`.
- **Why:** the earlier P3 visibility hold was answered by Nico (GO) and the contract is frozen in
  `03 §7f`; the docs must state the approved contract without claiming it is implemented. The
  runtime is being implemented on `feat/agent-memory-saved-reports`.
- **Verification:** documentation-only edit (no runtime, schema, index, build or Git-index change).
  Markdown link/fragment validation and a targeted diff check were run; **P3 runtime verification
  remains pending** and is not claimed here. No P2 old-head CI run is reused as P3 evidence.

## 2026-10-16 — External-agent memory API contract and inventories

- **Change:** explicit narrow portable/remote stdio-client boundary plus `03 §7f`
  contracts for chronological evidence, stable allocator metadata, truthful bounded
  coverage, independent read credential, safe saved-report reads and acceptance.
  Companion scope/operating/UI documents updated; schema remains 12. No runtime
  feature implementation in P0 and no listener or inference-default changes.
- **Why:** execute Nico's attached plan in independently checkable increments,
  retaining ordinary local workflows. An unfinished-run/saved-output contradiction
  is explicitly held for Nico rather than guessed; P1/P2 remain independent.
- **Verification:** independent inventories and boundary/full-contract review;
  residual OS prohibition and implicit HEAD classification corrected, verified by
  parent. Separate boundary commit `9f912e2`. Unchanged Linux MCP baseline:

  ```text
  BUILD EXIT CODE: 0
  TEST EXIT CODE: 0
  CLIPPY EXIT CODE: 0
  ```

  Parent binary/version/stdio verification and uncompleted native/remote gates in `05`.

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
> Live file holds only the current arc.

---

## 2026-10-15 — Six security alerts

- **Change:** reapplied the five dependency fixes onto v0.4.1 main: rustls
  0.23.45/webpki 0.103.15, brace-expansion 1.1.21/5.0.12, source-map-js 1.2.2,
  and selector-parser 7.1.6 via override. Dismissed CodeQL alert 2 as a false
  positive after verifying CredReadW allocation, null check, copy and free order.
- **Why:** remediate the six alerts at the maintainer's request while retaining
  all 0.4.1 changes and the Quinn fix.
- **Verification:** clean UI install/lint/build/tests and byte-identical output;
  production npm audits clean. Alert mapping and raw check excerpts in `05`.
