# F3 G5/G6 — implementation and verification

## Candidate

Branch: `feat/f3-g5-g6-weekly-scheduler-digest`.
Verified code candidate: `7bdef552c5916f6ffcdd4d5f0fb14f264724fcab`.
G3/G4 baseline: PR #139 through `176b567f5af8b7ba4036f26c740c015843adb8f2`.
Eight bounded agents handled contract audit, calendar scheduling, digest rendering,
traits/IPC, UI, Windows preparation, runtime persistence, and independent review.
The coordinator integrated and fixed the findings; earlier subagent summaries are
not evidence for the final candidate.

## Delivered behavior

- Default-off weekly schedule beside daily settings; weekly time and IANA zone,
  shared output folder, separate latest-run status and visible missed spans.
- Run most recent complete ISO week works while weekly scheduling is disabled.
  A weekly run snapshots its own zone without changing the daily configuration.
- Independent `memory.weekly.state` schedule UUID, activation and cursor. Calendar
  lookback is bounded to six complete weeks; enqueue the four most recent due
  eligible weeks oldest-first. Enqueue commits before cursor advancement; replay
  reuses the persisted `(schedule_id, UTC window)` identity.
- Missed spans are durable, sorted and disjoint in `memory.weekly.gaps`. Catch-up
  records a gap before skipping it. Timing changes preserve prior gaps and record
  previously eligible closed weeks that the new activation would otherwise hide;
  an open/future activation week is not labelled already missed.
- A future weekly cursor raises a visible scheduling error instead of silently
  suppressing reports. Weekly scheduling errors do not bypass daily pending work;
  daily work is processed before weekly catch-up, with oldest-first weekly order.
- Weekly Telegram delivery has its own default-off consent, sharing only the saved
  credential and selected destination. Both frozen enqueue consent and current
  consent must permit the run kind; live consent is rechecked before every send.
- Weekly digest reads saved output only: `YYYY-Www`, persisted status, saved-daily
  coverage, missing-day disclosure, bounded/deduplicated source IDs and masked,
  bounded synthesis highlights. Existing UTF-16 chunking and receipt machinery are
  retained; no second inference and no raw OCR/screenshots.
- Render failure creates a terminal non-network receipt. Explicit Retry settles
  the same reopened receipt rather than leaving a permanently pending loop.
- Daily digest output is frozen byte-for-byte by a regression test. Weekly settings
  and status fields deserialize old payloads with weekly functionality off.

## Observed checks

On the actual Windows application crate at the code candidate, without transport
or credential stubs:

```text
test result: ok. 146 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s
EXIT cargo clippy = 0
EXIT cargo build = 0
EXIT cargo test = 0
EXIT bindings diff = 0
CHAIN_EXIT=0
```

This includes the real SQLite catch-up/replay test (daily disabled), time-change
missed-span persistence, consent separation, digest formatting/masking/chunking,
producer-banner integration, render-failure retry settlement and daily controls.
The native RED digest checkpoint previously produced `9 passed; 4 failed`; the four
review regressions then passed in the candidate's full app suite.

Coordinator Linux controls:

```text
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
UI: tests 43; pass 43; fail 0
```

Kernel clippy with `-D warnings`, UI lint, typecheck and production build exited 0.
Full Windows `04 §1c` chain passed on the verified candidate: **1,118 Rust tests
passed, 0 failed, 15 existing environment-gated ignores; 45 UI tests passed, 0
failed**. Totals were parsed from all 62 Rust suite receipts in the existing log,
not inferred from test names or a second run. All five spec-named G5/G6 tests are
present as `... ok`, along with the added store/replay and consent regressions.
The script pins the existing checkout's target cache, selects Node 22 per-process
and checks branch, commit and clean source before and after every command. SHA256
of all 11 changed source/binding/test files matched the frozen committed bytes
before and after the chain. No second Windows checkout was used.

Raw log, named-gate receipts, source manifest and teardown receipt are retained in
`/home/hermes/work/f3-g5-g6-evidence/`. Built debug application SHA256:
`f1b6596c8244863488f0c29fcad9a5259521be1ec5c60974a3960f9e6130d15b`.
This is a build identity, not proof of a GUI run or an installed release.

Native teardown restored the owner's `feat/f3-g3-g4-writer-promotion` branch at
`176b567` with a clean checkout and verified all 19 shipped temporary files absent.
The installed app stayed running as PID `21660`; its executable/profile was not
replaced. Concurrent G3/G4 working files on the Linux coordinator were preserved
untouched; the committed tree and clean Windows candidate, not those unrelated
local diffs, are the verified artifact. PR #139 was still open at final inspection;
the branch contains its inspected head rather than assuming a merge.

## Review dispositions and implementation readings

- Fixed: settings save/consent persistence; weekly-only scheduling; distinct latest
  status; all live-send gates; per-lane/per-run error containment; daily-first
  fairness; gaps across both separate outages and timing changes; terminal digest
  failures; bounded unique source IDs; truthful PARTIAL causes and OK wording;
  model-echoed date markers; UI preview/save coupling; future-cursor visibility.
- Missing-date extraction is limited to the saved producer's leading partial banner,
  never arbitrary later model prose. No new saved-report/schema field was invented.
- Different daily/weekly zone strings may leave saved daily coverage missing, as the
  G2 input contract uses exact zone-string equality. The UI now warns explicitly.
- Manual runs remain explicit independent runs (`schedule_id=None`) and do not move
  the scheduler cursor. A manual run does not suppress a later scheduled run.
- All opted-in queued catch-up weeks may deliver their own digest; there is no
  undocumented newest-only suppression. Existing retry bounds and unknown-delivery
  handling apply unchanged. Enabling consent never sends a test message.
- Source IDs shown in the digest are capped at 12, with an exact unique omitted
  count; unusually long IDs are visibly abbreviated. Full provenance remains in
  the saved report. This does not rewrite persisted IDs or saved markdown.

## Remaining acceptance and authorization boundaries

The following have **not** been performed: real-week synthesis over private saved
history in the running installed app, Obsidian inspection with working links,
interactive promotion in that app, or a live weekly Telegram send. They remain the
separate `03 §8e` product sign-off; SQLite/SSR/native test output is not substituted
for them. No installed profile, credentials, live notes or running app were changed.
No push, PR creation/update or merge was performed.

Rollback: turn off the weekly schedule and weekly Telegram opt-in independently;
keep saved reports, notes, receipts and gap history. No schema rollback or deletion
is needed.
