# 08 — AI Changelog

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
