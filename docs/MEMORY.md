# Memory reports

This workflow creates daily Markdown reports from captured evidence (since v0.4.1), an optional
weekly synthesis over those saved dailies, and user-confirmed entries in a project note (both
since v0.4.2). It is
configured per installation in **Settings → Memory reports**. Scheduling and Telegram are off
by default; no folder, timezone, model download or recipient is selected automatically.

## Configure local reports

Choose an absolute output folder and an IANA timezone such as `Europe/Paris`. For scheduled
reports, also choose a local `HH:mm` time (hours and minutes, with no seconds field) and enable the schedule. The preview shows the first
full calendar day and when its report becomes due. A manual run covers the preceding calendar
day in the selected timezone. Reports use an already installed answer model; install missing
models explicitly in Models and retry the blocked run.

For an immediate manual report, use **Run a report** to select a start date and an optional
end date. Leaving the end date blank selects a single day; otherwise both dates are included.
Dates use the saved timezone. Future dates are rejected. Selecting today stops the report window
at the instant you press Run; evidence-based output is labeled PARTIAL with its as-of time.
The previous-day shortcut remains available and shows the date it will cover. A report with no
captured evidence is NO_DATA and Telegram is skipped, even when its delivery toggle is enabled.

For example, enabling a schedule on 15 October during the day starts its first full reporting
day on 16 October; that report is due on 17 October at the configured time. Running the
previous-day shortcut on 15 October instead covers 14 October. This distinction matters when
only today's capture history exists.

The daily schedule starts with the first complete day after activation. Changing its time or
timezone starts a new activation period; disabled periods are not backfilled. The selected
timezone remains fixed when travelling. At DST transitions a missing local time moves to the
first valid instant, while an overlapping time runs once at the earlier instant. Restart catches
up the seven most recent eligible days, oldest first; an older skipped interval stays visible.
There is one report worker, and it stops during application shutdown.

## Weekly synthesis

The weekly synthesis builds one report over a complete ISO week (Monday 00:00 to the next
Monday 00:00 in the weekly timezone) from that week's **saved daily reports**, never from a fresh
scan of raw captures. It is configured in the **Weekly synthesis** section with its own
default-off schedule, local `HH:mm` time and IANA timezone; the output folder is shared with
daily reports. A scheduled week becomes due at the weekly time on the Monday after it closes.
DST weeks are 167 or 169 hours long, never a fixed 168.

The schedule starts with the first complete ISO week after activation; a partially elapsed
activation week is never scheduled and disabled periods are not backfilled. **Run most recent
complete week** works while the schedule is off. On restart, catch-up looks back at most six
complete weeks and enqueues the four most recent due weeks, oldest first. Older weeks, weeks a
time or timezone change would hide, and scheduled weeks whose synthesis failed are listed under
**Missed weeks**; they are never reported as covered. A later manual run of the same week shows
under the last weekly synthesis.

Statuses stay truthful: `NO_DATA` when the week has no saved dailies with evidence (no model is
loaded), `PARTIAL` when any local date lacks a complete saved daily report (the missing dates are
named), `BLOCKED` when the answer model is missing, `FAILED` on errors, and `OK` only with
complete saved-daily coverage. A day whose saved daily is itself `NO_DATA` counts as measured,
but a week of nothing but `NO_DATA` dailies is `NO_DATA`, not `OK`. Each run records the
`daily_run_ids` it consumed.

Weekly notes are written to `<folder>/ScreenSearch/Weekly/YYYY-Www.md` (for example
`2026-W42.md`) with frontmatter schema 2 (`kind`, `daily_run_ids` and every local date of the
week); daily notes keep schema 1. `Index.md` lists each week with its status and links to the
weekly note and the daily notes it consumed. A consumed day whose note no longer exists is shown
as a plain date, and its run ID stays in the weekly frontmatter. `NO_DATA`, blocked and failed
weeks write no weekly note. The same ownership rules as daily notes apply: an edited weekly
note is preserved and reported as a conflict.

## Project note promotion

A saved daily or weekly report can feed one user-created project note. Create it from
**Settings → Memory reports → Project note** (it is created empty under
`<folder>/ScreenSearch/Projects/`); the app never creates, renames or deletes it on its own.
Exactly one note is supported: a second `.md` file in `Projects/`, including a retained
`.screensearch-recovery-*.md` copy, blocks promotion until you move or remove it. Other hidden
dot-files are ignored.

Open a saved report, tick items under *Explicitly evidenced decisions*, *Open questions*,
*Next steps* or *Activity*, and review the preview. Nothing is written until you press
**Confirm**. One promotion appends a dated `### YYYY-MM-DD` block with the report's provenance
under each touched section (`## Decisions`, `## Open questions`, `## Next steps`, `## Log`) in a
single atomic write; missing sections are added at the end in that order. Existing lines are
never rewritten. The date is the promotion date in the report's timezone.

If the note changes between the preview and Confirm, the promotion is refused as a conflict
and the note is left untouched; prepare it again. A confirmed promotion that was interrupted is
recovered on retry without appending twice. Promotion has no HTTP or MCP surface.

## Saved evidence and notes

The app saves the generated report before attempting delivery. Each run records its UTC window,
timezone, source identity, model, coverage counts and all frames actually summarized, plus the
associated sessions. Display citations can be capped independently of that full provenance.
Sampling is not exhaustive coverage and generated prose can still be wrong; inspect evidence
before treating a suggested decision or next step as fact.

Successful empty evidence inspection produces `NO_DATA` without running a model. Read failures
produce a visible failure, missing models block the run, and incomplete generation produces
`PARTIAL` with its limitations. Deleted source frames do not silently rewrite a saved report.

Notes are written to `<folder>/ScreenSearch/Daily/YYYY-MM-DD.md` (weekly notes to
`ScreenSearch/Weekly/`, see above); the generated navigation file is
`<folder>/ScreenSearch/Index.md`. Explicit selected-date reports use
`<folder>/ScreenSearch/Reports/<start>_to_<end>_<base-run-UUID>.md`. Each independent selection gets
its own file, while Regenerate updates only its unchanged owned base file. These reports appear
in the same index with their date-range labels and never replace scheduled daily notes.
Open that folder in Obsidian or any Markdown editor. Typed daily
frontmatter contains schema version 1, run/source IDs, window and coverage metadata. Since
v0.4.2, frontmatter is accepted only with schema `1` (daily and selected-date notes) or `2`
(weekly notes); any other value, for example from a hand-edited note or a future build, is
rejected as invalid provenance metadata (`FILE_CONFLICT`) instead of being accepted. Generated
files use recorded content hashes: an edited or unrelated file produces a conflict instead of
being overwritten. A detected concurrent replacement preserves a recovery file whose path is
shown in the receipt. Reparse points and symbolic links are rejected by the writer.

**Retry / resume delivery** reuses the saved result when available. **Regenerate** creates a
linked revision and performs new inference; it may replace only an unchanged owned note.
Generation, daily note, index and Telegram each have separate status. A delivery failure does
not require another model run. Local terminal run history is retained for 90 days, while active
work and unresolved delivery uncertainty remain. Exported files and their ownership records
survive history pruning.

## Optional Telegram delivery

Enter a bot token and an explicit chat ID, then enable Telegram and save. Tokens are write-only
and stored in Windows Credential Manager for this profile identity, outside settings, reports,
notes and logs. Telegram receives selected report text over HTTPS; inference stays local. No
screenshots or raw OCR are sent. Supported credential patterns are masked, but this is not
comprehensive secret or personal-data detection. Keep capture exclusions appropriate to your use.

Digests reuse the saved report, label partial coverage, bound length and split Unicode safely.
Each chunk has a persisted intent and a separate accepted message ID. That ID proves service
acceptance, not that anyone read it. Explicit rate-limit rejections retry at most three times;
timeouts, disconnects and incomplete responses are uncertain and never automatically resent.
Inspect the conversation, then mark uncertain attempts reviewed or explicitly confirm a resend
with its duplicate risk. Regeneration does not automatically repeat confirmed ancestor delivery.

Weekly digests need their own **Send weekly Telegram digests** consent, which is off by default;
the daily consent never authorizes weekly sends. They reuse the saved bot credential and chat
ID. A weekly digest is built from the saved synthesis only: a `YYYY-Www` header, the saved
status, saved-daily coverage with any missing dates, the consumed daily run IDs and masked,
bounded highlights, chunked by the same rules. No second model runs.

Disabling Telegram suppresses new dispatch; an accepted message cannot be recalled. Changing
the configured chat does not redirect existing attempts. NO_DATA, BLOCKED and FAILED runs do
not send a digest. An installation cloned from another profile should disable automation and
use the explicit source-identity reset before configuring its own credentials and output folder.

## Verification boundary

The local build and automated checks are recorded in `specs/05_BUILD_REVIEW.md`. Native model
quality, Obsidian inspection, separate-machine network access and an explicitly authorized
Telegram test send are separate acceptance checks in `docs/TESTING.md`. The weekly features
shipped in v0.4.2 on automated tests and the Windows CI/native test chain; their native
interactive acceptance (a real week in the installed app, Obsidian inspection, a promotion, a
live weekly digest) has not yet been performed. Rollout must cover varied supported machines,
history density, timezones, folder permissions and optional Telegram usage.

Paths shown and saved by Memory use forward slashes. Windows extended-path prefixes retain
their identity and are restored to native spelling only during filesystem operations; legacy
file-ownership records remain recognized.
