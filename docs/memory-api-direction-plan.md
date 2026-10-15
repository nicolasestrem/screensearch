# ScreenSearch: useful memory, scheduled reports, and a network-accessible API

Status (2026-10-15): **Execution authorized. Gate A API contract reviewed and committed;
increment B implementation verification recorded separately. C–E report policies approved by
the maintainer ("Sure GO"); local schedule/durable contract in `03 §8c`, Telegram permission
frozen separately in `03 §8d` before code. No blanket plan approval is outstanding. Native
acceptance remains open.**

This is the canonical repository execution plan, adapted from the original desktop planning
artifact; that external file is preserved unchanged. The product serves approximately 15,000
people, as clarified by the maintainer. Pilot and release validation must cover supported user
environments rather than use one person's workflow as the acceptance standard.

## 1. Goal and scope

Make ScreenSearch useful without requiring the user to open its search UI: turn captured work into source-backed memory in Obsidian and useful scheduled reports delivered through Telegram. Keep search as the evidence-retrieval foundation.

Change the application HTTP API listener from `127.0.0.1` to **`0.0.0.0`**. This means all IPv4 interfaces, not just a private-network interface. Clients use the computer's actual address, never `0.0.0.0` as a destination. Ideally make the API available to other devices on the same network, but do not expose it to the public Internet. Keep the existing loopback-only MCP destination validation and default URL; remote MCP is a separate candidate increment.

Execute the authorized increments with their separate contract, implementation and evidence gates. Do not mark an increment accepted until its verification output has been inspected and native acceptance observed. Firewall changes, private-history generation, an everyday installation replacement and Telegram test sends remain separately scoped actions; do not infer a destination or permission from implementation authorization.

## 2. Evidence and corrections

**Original planning evidence:** inspected baseline `3bf9e9f`, initially clean on `main`,
with planning branch `docs/memory-api-direction-plan`. The table below records that original
inspection; line numbers and statements about the old network boundary are historical evidence,
not current promises.

**Current execution references:** source baseline `ab6c702`; reviewed API contract amendment
`4fd037d`; B implementation on `feat/network-api`, with verification ongoing. The current specs
and build-loop logs own current behavior and evidence. This document does not assert that B or
native acceptance has passed.

| Observed foundation | Evidence |
|---|---|
| Windows-native app is intentional; cross-platform abstractions are prohibited | `AGENTS.md:84–89`; `specs/01_PROJECT_CONTEXT.md:29–35,82–85` |
| API has authenticated search, frames, sessions, marks, Ask and export, but no reports route | `crates/api/src/lib.rs:204–226`; `docs/API.md` |
| Loopback binding is an explicit contract, not a missing configuration field | `specs/03_MASTER_PRODUCTION_SPEC.md:612–621,933–936`; `crates/api/src/lib.rs` (`BIND_IP`, `ApiServer::start`) |
| MCP actively rejects non-loopback URLs | `crates/mcp/src/config.rs:94–106,119–140` |
| Report engine already generates daily/weekly/custom Markdown with citations and coverage metadata | `crates/kernel/src/reports.rs:109–139`; `crates/traits/src/domain.rs:526–587` |
| Tauri command supplies settings, progress, cancellation, and an empty-history fast path | `src-tauri/src/lib.rs:697–785` |
| Reports with evidence currently require a ready answer model | `src-tauri/src/lib.rs:748–751` |
| Export is content-text JSON; saved/scheduled reports are deferred | `docs/API.md:202–229`; `specs/03_MASTER_PRODUCTION_SPEC.md:970–974` |

Corrections to the earlier recommendation:
- The assistant's runtime does not establish the application's supported operating systems. Do not make product decisions from that assumption.
- ScreenSearch's prose is LLM-generated. Its orchestration and receipts can be deterministic; report accuracy and superiority to Screenpipe have not been benchmarked.
- A Screenpipe database cannot simply be passed to ScreenSearch's report engine: the engine consumes ScreenSearch traits/schema. No compatible adapter was demonstrated.
- Screenpipe documents Obsidian connections as well as API/MCP and task-based approaches. Lack of an official Obsidian plugin does not mean lack of native integration.
- The previous two-to-three-day estimate had no validated implementation basis and is withdrawn.

Research basis, collected previously from public sources:
- https://docs.screenpipe.com/second-brain — durable project/person/workflow memory through agents.
- https://docs.screenpipe.com/scheduled-tasks — scheduled, prompt-driven workflows.
- https://docs.screenpipe.com/reliable-reports — explicitly warns that suggested receipt statuses do not guarantee agent behavior and missed runs should not be assumed to backfill.
- https://docs.screenpipe.com/connections and https://screenpipe.com/changelog — Obsidian and messaging integration surfaces.
- https://screensearch.app/second-brain — current positioning emphasizes searchable context alongside notes.

These support a **product hypothesis**, not proof of a competitive advantage: reliable, user-owned memory and delivery may make screen history more useful across the product’s user base. Validate that hypothesis with consenting users.

## 3. Recommended direction

Keep the Windows capture/search application and reuse its report engine. Build one reliable loop:

`captured work → evidence-backed report → Obsidian Markdown → optional Telegram digest`

The existing HTTP API remains available to scripts and agents. A later report endpoint will reuse the same report service, not become a second implementation. All inference remains local. Schedules, file writes, delivery receipts, and retry decisions are application logic, not instructions left to a model.

First useful release: **scheduled daily memory in an Obsidian-compatible folder, with an optional Telegram digest and an authenticated network API**. Land the API change first as a separately verifiable increment. Follow with weekly synthesis and curated project memory only after daily output proves useful.

### Non-goals

- Cross-platform capture, extracting a new core library, or depending on Screenpipe.
- A generic pipes marketplace, arbitrary-shell agent runner, or new chat app.
- Audio capture, enterprise workflow mining, productivity scoring, or nudges.
- Cloud inference, account infrastructure, public hosting, or automatic router port forwarding.
- Exposing the llama.cpp listener; it stays on `127.0.0.1`.
- Two-way Telegram commands or remote capture control in the first release.
- Replacing Obsidian, rewriting existing notes, or treating generated summaries as established facts.

## 4. Constitution gate — resolve before code

Authoritative documents: `AGENTS.md`, `specs/01_PROJECT_CONTEXT.md`, `specs/02_STRATEGIC_PLAN.md`, `specs/03_MASTER_PRODUCTION_SPEC.md`, `specs/UI_REFERENCE.md`, and `specs/04_CLAUDE_CODE_BUILD_PROMPT.md`.

At the original planning baseline, the requested network listener directly conflicted with `03 §7c`: “a 0.0.0.0 bind must be impossible by construction.” The API contradiction has since been resolved by reviewed contract commit `4fd037d`. Telegram delivery still requires a narrow amendment to the no-cloud-upload/network rules; scheduled delivery requires an explicit exception to pull-only behavior. This plan does not silently reinterpret those rules.

**Gate A:** separate reviewed contract-amendment commits must precede their respective implementation increments; then re-check against the amended contract. Split API binding, scheduled delivery and Telegram outbound permission so each can be reviewed and reverted independently. These are major boundary changes, not wording fixes. Remote MCP permission is a separate deferred proposal.

| Existing principle | Proposed resolution / conformance |
|---|---|
| API loopback-only; MCP loopback-only | Amend for all-IPv4 API serving; preserve API default-off and bearer auth. Keep MCP loopback-only in the first increment; remote MCP needs a separate later amendment. Existing code conformed to the old rules. |
| All data local; no cloud uploads; network exceptions only | Amend narrowly for user-enabled API access and opt-in Telegram digest delivery. Raw frames are not automatically uploaded. Inference remains local. Existing code conformed to the old rule. |
| Pull-only/non-shaming | Allow explicitly configured report schedules and selected delivery; retain no scoring, nags, or unsolicited automation. |
| Windows-native; Rust-only runtime | Satisfied: no capture port, extraction, or Python runtime. |
| No real-time vision; no orphaned sidecar | Satisfied: reuse current enrichment and Job-Object lifecycle. |
| Forward-only migrations | Satisfied: any persisted report-run schema gets the next schema version after inventory, never a guessed version or downgrade. |
| Trait module boundaries | Satisfied: orchestration stays in kernel; shared contracts in traits; adapters wired by composition root. |
| Branches, reviewed PRs, no secrets/models/DBs committed | Satisfied: isolated feature branches; scan pending/untracked files before commits. No merge or release implied. |
| Typed UI, complete states, tokens, accessibility | Satisfied: generated bindings and existing UI conventions; no hand-edited generated files. |
| Stop on ambiguity; real evidence; no fake success | Satisfied: unresolved prerequisites stay explicit; native execution is a release gate. |

Record the contradiction in `specs/06_PATCH_PLAN.md` per `AGENTS.md:78`, and approved resolution in the remaining build-loop documents during Gate A. Include the explicit no-cloud-calls rule in `AGENTS.md:89`, not only the API section. Do not amend the principles and implement the conflicting behavior in one commit. Website privacy/copy changes ship with the behavior they describe, not ahead of it.

## 5. Implementation increments and acceptance gates

Test names below originated as **proposals unless expressly marked existing**. Execution results belong in the build-loop logs with raw command output; inclusion here is not evidence that a check ran or passed. Each task has a checkable outcome.

### A — Contract and coverage inventory

- **A1:** inventory active listener, MCP destination validation, status copy, settings, tests and docs. Deliver a file-by-file checklist; distinguish historical release descriptions from active promises. Search for `127.0.0.1`, `localhost`, `loopback`, `BIND_IP`, `SCREENSEARCH_API_URL`, `0.0.0.0`, `nothing leaves`, `never your network`, and `no cloud`; do not globally replace them. Inventory README/website privacy promises alongside source and specs.
- **A2:** prepare and review the separate contract amendment described above. Gate passes only when the revised rules expressly permit each requested behavior.
- **A3:** freeze report-run and delivery contracts before implementation: stable run identity, time-zone rules, status transitions, API errors, retention, and upgrade behavior. Record them in `03`, not only in this plan.

### B — Network-accessible API (first shippable increment)

- **B1:** change only the application API listener constant to IPv4 unspecified (`0.0.0.0`) in `crates/api/src/lib.rs`. `binds_all_ipv4_interfaces` checks the actual socket address; `loopback_client_still_connects` connects to `127.0.0.1` using that socket's port. Replace the existing `binds_loopback_only` assertion in `crates/api/tests/http_api.rs:254–261`. Update both API and MCP integration-test helpers that construct URLs from `server.local_addr()` (`crates/api/tests/http_api.rs:229–235`; `crates/mcp/tests/stdio_mcp.rs:207–213`, plus the session fixture). Use `127.0.0.1` plus the bound port as the destination, never `0.0.0.0`. Update now-false loopback-only source comments without changing the inference bind.
- **B2:** preserve `api.enabled`, port selection, current tokens, rotation, disable/re-enable, and bind-error behavior in `src-tauri/src/local_api.rs`. Add `disabled_api_has_no_listener`; retain and run existing `token_swap_takes_effect_without_restart` (`crates/api/tests/http_api.rs:309`), `fresh_profile_defaults_off` and `bind_failure_keeps_enabled_with_error` (`src-tauri/src/local_api.rs:519,582`) rather than duplicating them. Existing enabled installations will acquire the broader bind after upgrading as requested. Show a one-time visible in-app notice on that upgrade as well as updated Settings and release notes; do not silently change behavior or introduce an unrequested loopback mode. `enabled_upgrade_shows_bind_change_notice_once` covers the notice. The API remains default-off for new profiles.
- **B3 (deferred, not required for B):** remote MCP support is a separate candidate increment, not implied by changing the server bind. Keep existing MCP destination validation and default URL for B; local MCP works against the all-interface listener. If later accepted, specify remote URL validation, compatibility and the existing `.no_proxy()` policy before changing the client.
- **B4:** retain bearer auth on every route, including health and unknown paths. `all_routes_require_bearer` proves 401 for an unauthenticated unknown path versus 404 with a valid token. Do not add wildcard CORS, cookie auth, or token-bearing URLs. In the deferred B3 client increment, explicitly add redirect refusal to `crates/mcp/src/client.rs:85–89` (none currently configured), verified by `redirect_does_not_forward_token`.
- **B5:** update active API/MCP descriptions, `ApiPanel`, `ApiStatus` documentation and generated bindings as needed. `api_status_displays_all_interface_bind` covers the status; copy distinguishes the listen address from a client destination. Local MCP examples remain valid; remote HTTP examples use a configurable host. Document the Windows inbound-allow step and distinguish 200/401 from connection timeout; no automatic firewall changes. State the actual network contract briefly: this listener is plain HTTP and a token holder can read history from any reachable host, rather than only a local process.
- **B6:** run a native Windows build and real cross-machine checks. From a separate machine, valid-token `/v1/health` returns 200 and actual version; missing/wrong token returns 401. Verify local MCP, disable/enable, app restart, and Ask stream disconnect as a regression check. Remote HTTP is mandatory; remote MCP is not in this increment. Confirm the inference sidecar is still loopback-only. Any firewall adjustment is a separately recorded test-machine action, not application feature scope.

No report scheduler, vault writer, schema migration, remote-MCP validator change or Telegram code belongs in increment B. The loopback-only MCP guard remains intentional until B3 is separately accepted.

### C — One shared report execution path

- **C1:** factor report execution behind a shared application service used by Tauri and scheduling, retaining the existing kernel report engine and allowing a future HTTP adapter. `report_entry_points_preserve_window_and_coverage` checks matching input windows, evidence scope and metadata, not byte-identical LLM prose.
- **C2:** add durable report-run records via a forward-only migration in `crates/store` with contracts in `crates/traits`. For scheduled runs, uniqueness is schedule ID plus explicit UTC window; retries reuse the run. Configuration/model version and source identity are provenance metadata, not additional scheduler deduplication dimensions. Manual reruns explicitly choose regenerate versus retry. Retain local run records for 90 days without deleting exported notes; never prune nonterminal runs or active delivery receipts. `run_retention_preserves_pending_delivery` verifies pruning. `duplicate_report_request_reuses_run` and `interrupted_run_is_recoverable` verify persistence and recovery.
- **C3 (deferred until after F1):** report HTTP/MCP endpoints are not needed to prove the daily-note/digest loop. If agent-driven report generation is later useful, separately plan authenticated enqueue/status endpoints over the shared service, including idempotency, read-only GET semantics and a report-job write-scope amendment. Do not hold D/E behind this API expansion.
- **C4:** implement truthful results: `NO_DATA` for successfully inspected empty evidence, `PARTIAL` for truncated/incomplete coverage, `BLOCKED` for missing prerequisites, `FAILED` for execution errors, `OK` only for a completed report with its measured coverage. `empty_window_needs_no_model`, `store_error_is_not_no_data`, `truncated_report_is_partial`, and `missing_report_source_is_explicit` prove distinctions. Sampled coverage never claims exhaustive capture.
- **C5:** serialize automatic report inference through the existing model lifecycle. Model unavailable means deferred/blocked, not fabricated output; opt-in schedules may request the existing answer-model loader but never implicit model downloads. `report_model_load_failure_is_visible` verifies readiness failure. Add `shutdown_cancels_active_report_and_joins_worker` to prove the shutdown bound and worker cleanup; distinguish report cancellation (currently cooperative between passes) from Ask stream cancellation. B6 supplies real Ask-disconnect evidence; this plan does not claim existing reports cancel an in-flight model pass instantly.

### D — Scheduled daily memory in Obsidian

- **D1:** implement a default-off daily schedule, user-selected local time and time zone, with half-open previous-calendar-day windows. Persist scheduling state; on restart process missed days oldest-first, capped at seven days, and expose older gaps rather than silently claiming them covered. Tests `daily_window_handles_dst`, `restart_backfills_once`, and `backfill_cap_reports_gap` cover boundaries.
- **D2:** write generated Markdown to a user-selected vault/folder under `ScreenSearch/Daily/YYYY-MM-DD.md`. This is an ordinary folder integration, not a required Obsidian plugin. Include source identity, window, status, model, coverage and source frame/session IDs. `daily_note_contains_provenance` verifies rendered metadata against the report result. Freeze frontmatter schema v1 in A3: `schema`, `run_id`, `source_id`, `window_start`, `window_end`, `timezone`, `status`, `model`, `frames_sampled`, `frames_summarized`, `periods_total`, `periods_covered`, `truncated`, `frame_ids`, `session_ids`. `daily_note_frontmatter_roundtrips` checks typed parse/serialization. Use headings for activity, decisions explicitly evidenced, open questions and next steps; do not turn tentative screen text into commitments.
- **D3:** make writes atomic and idempotent. Store the generated-file hash; replace only unchanged generated files. If the user edited a file or an unrelated file already exists, preserve it and expose a conflict instead of overwriting. `rerun_does_not_duplicate_note`, `edited_note_is_preserved`, and `interrupted_write_preserves_previous_file` verify behavior. Never place API tokens in notes or links.
- **D3a:** maintain `ScreenSearch/Index.md` linking generated daily notes with date, status and brief topic labels. `memory_index_links_existing_notes` verifies links and no duplicates; apply the same atomic-write, ownership and edit-conflict rules as D3. This provides minimal navigable memory rather than a pile of disconnected reports.
- **D4:** add Settings controls for schedule, time zone, output folder, Run now and last-run status, using generated IPC. Verify loading/off/error/partial/success states and keyboard navigation in the running Windows app. A failed vault write must not erase a successfully generated report or advance its delivery state.

### E — Optional Telegram digest

- **E1:** add a default-off Telegram destination with token stored via Windows Credential Manager, outside general settings responses, prompts, logs and exports. This deliberately differs from the existing API-token KV storage: only the new outbound-service credential uses the Windows store; migrating existing API credentials is not in scope. `telegram_token_never_enters_report_context` and credential-unavailable tests verify the boundary. Opting into Telegram explicitly permits the selected digest to leave the device.
- **E2:** deliver a concise text-only digest from the saved report, not a second independently generated account of the day. Bound length before splitting; omit screenshots/raw OCR; mask supported credential patterns and state that this is not comprehensive PII detection. `digest_chunks_fit_telegram_limit` and `digest_uses_saved_report` cover rendering. Existing capture exclusions remain important input controls.
- **E3:** persist delivery separately from generation and vault-writing. Store Telegram's returned message ID, retry explicit transient failures with backoff, and mark a timeout after sending as `UNKNOWN_DELIVERY` rather than claiming exactly-once success or blindly duplicating. `confirmed_send_is_not_repeated`, `ambiguous_send_is_not_auto_retried`, and `vault_failure_does_not_regenerate_report` cover behavior.
- **E4:** exercise the real Bot API with a user-approved test destination and inspect the delivered message in Telegram. An API acknowledgement proves acceptance, not that the user read the message. This remains future verification requiring an explicitly approved test destination and send; implementation authorization alone is not authorization to message a recipient.

### F — Product validation, then deeper memory

**Proposed pilot and release validation; acceptance criteria remain to be reviewed.**

### F1 — Opt-in pilot across supported environments

Run the completed daily loop with consenting participants representing the supported Windows
configurations and relevant usage variations: installed/unavailable models, different timezone
and DST behavior, intermittent app uptime, different capture density, existing/edited vault
files, and optional Telegram use. Define recruitment, observation duration and release
acceptance criteria with the maintainer before starting. The pilot must not implicitly enable
schedules or outbound delivery for existing installations.

Collect generation and destination receipts, participant judgments of usefulness/noise,
corrections, concrete later recall examples and manual interventions. Keep raw screen history
and generated private content local unless a participant separately consents to sharing a
bounded example. This proposal does not add telemetry or default uploads.

Release review must examine observed unsupported claims, omitted context, provenance quality,
missed/partial/failed-run visibility, content-preservation conflicts, duplicate deliveries,
model readiness failures and recovery after restart. A visible environment failure remains a
failure, not a successful report. Unit tests and service acknowledgements do not replace native
app, file and delivery evidence. Report results by tested configuration and disclose coverage
gaps; one successful personal setup does not establish reliability for the wider user base.

### F2 — Consented workflow comparison and release decision

Compare bounded, consented examples with participants' existing recall or note workflows.
Assess source traceability, unsupported claims, useful recall and required corrections without
assuming an architectural advantage. Distinguish personal preferences from defects and from
generalizable usability findings.

Review the evidence and remaining limitations with the maintainer before a release decision.
If usefulness or reliability is inadequate, resolve it or reconsider maintaining only the
API/export path before adding deeper memory. F3's weekly synthesis and project-note work remain
separate planning gates after pilot review; this document does not authorize them.

- **F3:** only after F1 passes and F2 is reviewed, separately plan a minimum deeper-memory slice: one weekly index-linked synthesis and one user-selected project note accumulating source-backed changes across days. Target content includes source-backed decisions, open questions and next steps linked to daily notes, with user-confirmed promotion into durable project notes. No automatic edits to personal identity or existing knowledge pages. This is the next planning gate, not hidden scope in the daily-report release.

## 6. Verification strategy

Run on a real Windows build environment. Record the installed app version, source commit, toolchain and selected model for each native acceptance run. The original planning phase did not inspect a running installation; current verification must be established by the build-loop evidence, not inferred from that historical statement.

Use the repository's declared order: UI `npm ci`, `npm run lint`, `npm run build`; stage MCP with `node scripts/stage-mcp.mjs`; then `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace`; finally `git diff --exit-code -- ui/src/bindings`. Confirm toolchain requirements against current CI before installing tools.

Native acceptance requires actual socket checks, HTTP/MCP responses, rendered UI states, a real local-model report, a real Markdown file visible in Obsidian, and a real approved Telegram delivery. Unit-test fixtures do not substitute for these proofs. Save verbatim check output without secrets or private screen text.

## 7. Review Focus

Inputs requiring explicit implementation tests (not yet implemented):
1. **Midnight/DST and machine time-zone changes** — no duplicate or skipped finalized windows; owned by D1.
2. **Crash between Telegram acceptance and receipt persistence** — unknown delivery remains visible; never promise exactly-once transport; owned by E3.
3. **Vault edits, permissions loss or replacement during writing** — preserve human content and old valid output; owned by D3/D4.
4. **Remote disconnect, token rotation and app shutdown during inference** — no leaked worker/model work or unauthenticated new request; owned by B2/B6/C5.
5. **Frames deleted or images expired after report generation** — provenance retains IDs but reports unavailable evidence honestly; add `missing_report_source_is_explicit` in C4. Do not invent a working desktop deep-link scheme: none was verified in this audit.

## 8. Rollback and release gates

- Documentation rollback: revert the relevant documentation commit on its feature branch while preserving the execution record. The original desktop planning artifact remains unchanged; do not describe the current implementation work as planning-only.
- API rollback: disable API immediately if needed; revert increment B on its branch and restore the previous build to regain loopback binding. B adds no database migration. Do not reset tokens unless necessary.
- Automation rollback: disable schedules and delivery independently. Preserve generated notes and run history. Remove only run-owned temporary files; never delete or rewrite user notes to roll back.
- Schema rollback: never downgrade a live migrated DB. Snapshot the database before migration; test restoration in a separate location. If an older binary cannot read the newer schema, recovery uses a consistent backup, not a forced schema-version edit.
- Keep implementation increments separately reviewable. No main merge, tag, updater publication or replacement of an everyday installation is implied by execution authorization.

## 9. Decisions and handoff state

**User requirements:** execute the memory/reporting direction using subagents and the repository methodology; serve the application API on `0.0.0.0`; design for the application’s wider user base.

**Authorized direction:** retain the Windows app; native scheduled reports and folder integration; opt-in outbound Telegram; stage B independently before C–E; evaluate durable project memory only after daily use has been validated. This is not waiting for blanket plan approval again.

**Rejected for this iteration:** speculative cross-platform rewrite, Screenpipe schema compatibility by assumption, and building a pipes marketplace.

**Resolved report decisions:** the maintainer approved activation, DST/timezone changes,
Retry/Regenerate, destination statuses, unknown-delivery handling, profile identity and full
provenance policies. `03 §8c` is the local durable-report contract and
[memory-report-contract-proposal.md](memory-report-contract-proposal.md) records the accepted
policy history. Telegram outbound permission is recorded in the separately reviewed `03 §8d` amendment.
Implementation is authorized; actual acceptance evidence remains required.

**Native acceptance and setup:** cross-machine HTTP, local MCP, lifecycle/disconnect and rendered
UI evidence remain acceptance gates for B. Later stages need a selected vault, installed local
model and explicitly approved Telegram test destination/credentials. These are setup inputs,
not reasons to invent defaults or access private history now. Pilot criteria and broader release
validation remain proposed until reviewed.

Continue checkpointed execution under the 2026-10-15 authorization. The contract, API, report service and delivery stages remain separately reviewable; each stage stays open until its own acceptance evidence exists. No merge, release or publication is authorized by this plan.

The app must build at the end.
