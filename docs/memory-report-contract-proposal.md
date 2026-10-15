# Memory reports: proposed execution and delivery contract

Status: **Proposal for review, not an approved implementation contract.**

This document resolves the remaining A3 design questions in
[memory-api-direction-plan.md](memory-api-direction-plan.md). It is a review artifact for increments
C–E, not permission to implement them. Approved decisions must be recorded in
`specs/03_MASTER_PRODUCTION_SPEC.md` before implementation, with the separate scheduling and
Telegram boundary amendments required by Gate A. The application serves approximately 15,000
people, as clarified by the maintainer; an individual setup is one validation environment, not
the product’s audience or acceptance standard.

## Existing plan decisions

The execution plan already specifies default-off daily schedules and Telegram delivery;
user-selected time and timezone; half-open previous-calendar-day windows; durable runs;
scheduled-run uniqueness by schedule ID and explicit UTC window; oldest-first restart backfill
capped at seven days with older gaps exposed; local inference without automatic model downloads;
90-day run retention that preserves nonterminal runs and active delivery receipts; atomic,
ownership-checked Markdown writes; and separate generation, vault and Telegram outcomes.

Those decisions remain the baseline. Report HTTP/MCP enqueue and status endpoints are deferred
under C3; this proposal does not add them or define a premature HTTP error contract.

## Proposed policies requiring review

### First enable and resume

Enabling a schedule records an activation instant. It does not automatically generate reports
for days before activation. The first eligible daily window is the first full calendar day in
the selected timezone whose start is at or after activation; its report becomes due at the
configured time on the following day. This intentionally excludes a partial activation day.

Restart recovery applies only to eligible windows missed while the schedule was enabled. It
uses the plan's seven-day cap and exposes older gaps. Explicitly disabling a schedule ends its
automatic coverage period; enabling it again starts a new activation period, without silently
backfilling the disabled interval. Existing runs and receipts remain available subject to
retention. These activation periods must not erase the stable identity of previously created
runs.

Settings must state the first scheduled report date before enablement is saved. Existing
installations upgrade with scheduling and Telegram disabled; upgrading does not select a
timezone, destination folder or Telegram recipient on a user's behalf.

### Generation status and delivery eligibility

Generation outcome and destination outcome are separate fields. A saved report remains saved
when a destination fails; delivery retry must not regenerate it.

| Generation outcome | Proposed vault behavior | Proposed Telegram behavior |
|---|---|---|
| `OK` | Write the saved report with measured coverage and provenance. | Send a digest derived from that saved report when enabled. |
| `PARTIAL` | Write the usable saved report with a prominent partial-coverage explanation. | Send its digest with the same prominent limitation; never imply a complete day. |
| `NO_DATA` | Write a factual no-data note with the inspected window and coverage receipt; no model call. | Do not send a digest. Record a visible skipped/no-data destination outcome. |
| `BLOCKED` | Preserve the run and prerequisite explanation in app status; create no daily note. | Do not send; record a visible skipped/blocked destination outcome. |
| `FAILED` | Preserve the error receipt in app status; create no daily note. | Do not send; record a visible skipped/failed destination outcome. |

`NO_DATA` requires a successful evidence inspection. Store errors are failures, not empty
history. Missing prerequisites are blocked, not successful reports. `PARTIAL` requires usable
saved output plus an explicit description of incomplete coverage. These statuses never imply
exhaustive capture merely because every sampled frame was summarized.

Skipped delivery is not delivery success. It is an explicit terminal destination outcome with
a reason, so users can distinguish intentional suppression from pending or failed work. The
index links only notes that were actually written; a failed write must not produce a broken
index entry or a successful vault receipt.

### Ambiguous Telegram delivery

A timeout after sending, or recovery after an attempt whose acceptance cannot be established,
becomes `UNKNOWN_DELIVERY`. It must not be automatically resent, including after restart,
destination edits or another scheduler pass. App status asks the user to inspect the selected
Telegram conversation and offers explicit resolution:

- **Mark reviewed without resend:** retain the uncertainty and the user's resolution; do not
  fabricate a Telegram message ID or confirmed delivery receipt.
- **Resend intentionally:** create a linked delivery attempt after clearly explaining that the
  earlier message may already exist. Preserve the earlier unknown attempt in history.

Telegram's returned message ID proves service acceptance, not that a person read the message.
Confirmed attempts must never be repeated automatically. Explicit transient failures may use
bounded backoff; exact bounds and error classification must be frozen in the implementation
contract before E3, with no retries of ambiguous attempts.

### Source identity

Create a random, durable profile identifier once and persist it alongside report state. It is
the frontmatter `source_id`; it must not contain a username, hostname, vault path or API token.
Ordinary profile moves and backup restoration preserve it, keeping existing notes and run
provenance attributable to the same source.

A byte-for-byte clone also copies that identity. It is therefore not proof that two active
installations are the same running source. Document this caveat: independently active clones
must be explicitly assigned a new profile identity before scheduled export/delivery is enabled
on the clone. Changing identity affects future runs only; previously saved provenance is not
rewritten. The product must expose a deliberate clone/reset identity action rather than infer
identity from machine names. Two profiles targeting the same daily-note path still follow the
ownership/conflict rules; identity collisions do not authorize overwriting.

### Evidence IDs and frontmatter

Freeze schema-v1 frontmatter keys from D2 exactly: `schema`, `run_id`, `source_id`,
`window_start`, `window_end`, `timezone`, `status`, `model`, `frames_sampled`,
`frames_summarized`, `periods_total`, `periods_covered`, `truncated`, `frame_ids`, and
`session_ids`.

Proposed meaning: `frame_ids` contains the full distinct set of frames actually summarized,
not merely the capped citation list used by the existing UI. `session_ids` contains the
distinct known session IDs associated with those frames at generation time; no session is
invented for an unassigned frame. Persist these IDs with the saved result so later retention
or session recomputation cannot silently alter original provenance.

The existing engine caps displayed/returned citations at 200
(`crates/kernel/src/reports.rs`, `MAX_REPORT_CITATIONS`), while its summarized count reflects
the full union. Implementation therefore needs a separate uncapped provenance collection;
renaming the current capped list is insufficient. Verify that the distinct `frame_ids` count
matches `frames_summarized`. A no-data result has empty evidence ID lists and no model.

IDs identify historical evidence; they do not guarantee that frames or images remain available.
Expose later missing evidence honestly. Do not invent desktop deep links or imply images were
uploaded to Telegram.

## Pending maintainer questions

The following questions have already been raised with the maintainer and remain unanswered in
this proposal. None of the alternatives below is approved.

1. **DST and timezone/time changes:** how should nonexistent or repeated scheduled local times
   fire, and should changing the configured timezone or local time apply only to future windows
   while preserving previously finalized UTC windows? The contract must specify the effective
   boundary, preserve existing run identities, and prevent automatic duplicate finalized days.
2. **Retry versus regenerate:** should Retry reuse an existing run and saved output/destination
   receipts, while Regenerate creates a new linked result? For regeneration, explicitly decide
   whether to replace an unchanged owned daily note and whether another Telegram digest is sent.
   Edited or unrelated notes remain protected under every answer.

These decisions affect persistence and visible behavior and must be resolved before C–E
implementation. Choosing a UUID library, SQL indexes or an atomic-write primitive does not
itself require a separate product decision.

## Proposed replacement for F1/F2 validation

This section records the proposed product-wide validation mirrored in the canonical
[memory-api-direction-plan.md](memory-api-direction-plan.md). It does not claim pilot results
or invent a population-wide quality score.

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

## Evidence and approval boundary

This is a documentation proposal. No C–E behavior, migrations, native runtime checks, private
history analysis, file integration, or Telegram delivery has been performed by drafting it.
The user-selected vault and approved Telegram test destination remain setup inputs for later
acceptance testing, not defaults to infer. Implementation and validation status must remain
explicit until each gate has actual evidence.
