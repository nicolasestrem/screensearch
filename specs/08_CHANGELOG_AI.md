# 08 — AI Changelog

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
