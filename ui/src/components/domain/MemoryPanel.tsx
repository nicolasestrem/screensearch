import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ReportView } from "./ReportView";
import { Button, Chip, Field, Panel, Toggle } from "../primitives";
import type { MemorySettings } from "../../bindings/MemorySettings";
import type { ReportDelivery } from "../../bindings/ReportDelivery";
import type { ReportResponse } from "../../bindings/ReportResponse";
import type { ReportRun } from "../../bindings/ReportRun";
import * as cmd from "../../lib/ipc/commands";
import { savedRunFooterScope } from "../../lib/reportFooter";
import {
  memoryError, useMemoryDeliveries, useMemoryMutation, useMemoryRun,
  useMemoryRuns, useMemorySchedulePreview, useMemoryStatus, useSetMemorySettings,
} from "../../lib/ipc/memory";
import {
  promotionError, useCreateProjectNote, useProjectNoteStatus,
} from "../../lib/ipc/promotion";

const forwardPath = (value: string) => value.replaceAll("\\", "/");
const displaySettings = (settings: MemorySettings): MemorySettings => ({
  ...settings,
  output_folder: settings.output_folder === null ? null : forwardPath(settings.output_folder),
});

function timestamp(value: number, timeZone?: string) {
  return new Date(value).toLocaleString(undefined, {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", timeZone,
  });
}

function reportDates(run: ReportRun) {
  const range = run.requested_range;
  return range ? range.start_date === range.end_date ? range.start_date : `${range.start_date} – ${range.end_date}` : run.local_date;
}

/**
 * SavedReport → the shared `ReportView` response shape. Coverage counts and the body
 * stay exactly as persisted; the run's own window supplies the range label.
 */
function savedReportResponse(run: ReportRun): ReportResponse | null {
  const saved = run.result;
  if (!saved) return null;
  return {
    markdown: saved.markdown,
    cited_frame_ids: saved.cited_frame_ids,
    range_label: reportDates(run),
    periods_total: saved.periods_total,
    periods_covered: saved.periods_covered,
    frames_sampled: saved.frames_sampled,
    frames_summarized: saved.frames_summarized,
    passes: saved.passes,
    truncated: saved.truncated,
    model: saved.model,
  };
}

function todayInZone(timeZone?: string | null) {
  if (!timeZone) return null;
  try {
    const parts = new Intl.DateTimeFormat("en", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(new Date());
    const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((value) => value.type === type)?.value;
    return `${part("year")}-${part("month")}-${part("day")}`;
  } catch {
    return null;
  }
}

function previousDate(today: string | null) {
  if (!today) return null;
  const date = new Date(`${today}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() - 1);
  return date.toISOString().slice(0, 10);
}

function DeliveryReceipt({ receipt }: { receipt: ReportDelivery }) {
  const resolve = useMemoryMutation(cmd.resolveTelegramDelivery);
  const [confirmResend, setConfirmResend] = useState(false);
  return (
    <li className="rounded-panel border border-line p-3 flex flex-col gap-2 text-body">
      <p className="font-semibold">
        {receipt.destination} · {receipt.state.replaceAll("_", " ")}
        {receipt.chunk_index !== null && ` · chunk ${receipt.chunk_index + 1}/${receipt.chunk_count}`}
      </p>
      {receipt.path && <p className="break-all font-mono text-data">{forwardPath(receipt.path)}</p>}
      {receipt.error && <p className="text-warn">{receipt.error.message}</p>}
      {receipt.recovery_path && (
        <p className="break-all">Preserved recovery file: {forwardPath(receipt.recovery_path)}</p>
      )}
      {receipt.message_id !== null && (
        <p className="text-caption text-ink-muted">Telegram accepted message {receipt.message_id}; this is not a read receipt.</p>
      )}
      {receipt.next_attempt_at !== null && (
        <p className="text-caption">Next retry: {timestamp(receipt.next_attempt_at)}</p>
      )}
      {receipt.state === "unknown_delivery" && (
        <div className="flex flex-col gap-2">
          <p>Telegram may already have received this message. Inspect the conversation before choosing an action.</p>
          <div className="flex flex-wrap gap-2">
            <Button disabled={resolve.isPending} onClick={() => resolve.mutate({ attemptId: receipt.id, resolution: "reviewed" })}>
              Mark reviewed without resend
            </Button>
            <Button disabled={resolve.isPending} onClick={() => setConfirmResend(true)}>Resend intentionally…</Button>
          </div>
          {confirmResend && (
            <div role="group" aria-label="Confirm intentional resend" className="border border-warn rounded-panel p-3 flex flex-col gap-2">
              <p>This can create a duplicate message. The earlier uncertain attempt remains in history.</p>
              <div className="flex flex-wrap gap-2">
                <Button disabled={resolve.isPending} onClick={() => resolve.mutate({ attemptId: receipt.id, resolution: "resend" }, { onSuccess: () => setConfirmResend(false) })}>
                  Confirm resend
                </Button>
                <Button disabled={resolve.isPending} onClick={() => setConfirmResend(false)}>Cancel</Button>
              </div>
            </div>
          )}
        </div>
      )}
      {resolve.isError && <p role="alert" className="text-warn">{memoryError(resolve.error)}</p>}
    </li>
  );
}

function RunDetails({ runId }: { runId: string }) {
  const detail = useMemoryRun(runId);
  const deliveries = useMemoryDeliveries(runId);
  const retry = useMemoryMutation(cmd.retryMemoryReport);
  const regenerate = useMemoryMutation(cmd.regenerateMemoryReport);
  const [confirmRegenerate, setConfirmRegenerate] = useState(false);
  const navigate = useNavigate();
  const run = detail.data;
  const savedReport = run ? savedReportResponse(run) : null;
  const active = run?.phase === "queued" || run?.phase === "running";
  return (
    <section aria-label="Selected report" className="flex flex-col gap-3 border border-line rounded-panel p-4">
      {!run && <p>{detail.isError ? memoryError(detail.error) : "Loading report…"}</p>}
      {detail.isError && <Button onClick={() => detail.refetch()}>Reload report</Button>}
      {run && <>
        <h3 className="font-display text-subtitle">{reportDates(run)} · {run.outcome ?? run.phase}</h3>
        <p className="text-caption text-ink-muted">{timestamp(run.window_start, run.timezone)} – {timestamp(run.window_end, run.timezone)} · {run.timezone}</p>
        <p className="text-caption font-mono break-all">Run {run.run_id} · source {run.source_id}</p>
        {run.revision_of && <p className="text-caption break-all">Revision of {run.revision_of}</p>}
        {run.error && <p role="status" className="text-warn">{run.error.message}</p>}
        {active && <p role="status">{run.phase === "queued" ? "Waiting for the report worker." : "Generating from local evidence. Coverage appears when the result is saved."}</p>}
        <div className="flex flex-wrap gap-2">
          <Button disabled={active || retry.isPending || regenerate.isPending} onClick={() => retry.mutate(runId)}>Retry / resume delivery</Button>
          <Button disabled={active || retry.isPending || regenerate.isPending} onClick={() => setConfirmRegenerate(true)}>Regenerate…</Button>
        </div>
        {confirmRegenerate && (
          <div role="group" aria-label="Confirm report regeneration" className="border border-line p-3 flex flex-col gap-2">
            <p>Create a linked report revision for this window. Only an unchanged owned note can be replaced. Confirmed Telegram delivery will not repeat automatically.</p>
            <div className="flex flex-wrap gap-2">
              <Button disabled={regenerate.isPending} onClick={() => regenerate.mutate(runId, { onSuccess: () => setConfirmRegenerate(false) })}>Create revision</Button>
              <Button disabled={regenerate.isPending} onClick={() => setConfirmRegenerate(false)}>Cancel</Button>
            </div>
          </div>
        )}
        {(retry.isError || regenerate.isError) && <p role="alert" className="text-warn">{memoryError(retry.error ?? regenerate.error)}</p>}
        {savedReport && run.result && <>
          <p className={run.outcome === "PARTIAL" ? "text-warn" : "text-ink-muted"}>
            {run.outcome === "PARTIAL" && "Partial coverage. "}
            {run.result.frames_summarized} frames summarized;{" "}
            {run.result.frames_sampled} frames sampled;
            {" "}{run.result.periods_covered}/{run.result.periods_total} periods covered.
            {run.result.truncated && " Output was bounded or truncated."} Sampling is not exhaustive capture.
          </p>
          <p className="text-caption">Model: {run.result.model ?? "none — no model ran"}</p>
          <details>
            <summary className="cursor-pointer min-h-hit-min">Saved report, promotion and evidence IDs</summary>
            <div className="flex flex-col gap-3 pt-2">
              {/* The single saved-report renderer: it carries the report footer, the
                  source-frame chips and — because this is a persisted run — the
                  promotion surface (`03 §8e`, G4). */}
              <ReportView
                report={savedReport}
                request={null}
                run={run}
                showFooter={false}
                footerScope={savedRunFooterScope(run)}
                onOpenFrame={(frameId) => navigate(`/timeline/${frameId}`)}
              />
              <p className="text-caption break-words">Frames: {run.result.frame_ids.join(", ") || "none"}</p>
              <p className="text-caption break-words">Sessions: {run.result.session_ids.join(", ") || "none"}</p>
              <p className="text-caption text-ink-muted">IDs record generation-time evidence; retention may have removed the original frames.</p>
            </div>
          </details>
        </>}
      </>}
      <h4 className="font-display text-body">Delivery receipts</h4>
      {deliveries.isLoading && <p>Loading receipts…</p>}
      {deliveries.isError && <div><p role="alert">{memoryError(deliveries.error)}</p><Button onClick={() => deliveries.refetch()}>Reload receipts</Button></div>}
      {deliveries.data?.length === 0 && <p className="text-ink-muted">No delivery attempts recorded.</p>}
      <ul className="flex flex-col gap-2">{deliveries.data?.map((receipt) => <DeliveryReceipt key={receipt.id} receipt={receipt} />)}</ul>
    </section>
  );
}

/**
 * The single project note (`03 §8e`, G4): status, the unsupported multiple-notes
 * state, and the one manual create action. Creation makes an EMPTY note — nothing is
 * written into it until a promotion is confirmed — and the app never renames or
 * deletes the file afterwards.
 */
function ProjectNoteSection() {
  const status = useProjectNoteStatus();
  const create = useCreateProjectNote();
  const [slug, setSlug] = useState("");
  const trimmed = slug.trim();
  const slugError =
    trimmed.length > 0 && !/^[a-z0-9][a-z0-9_-]{0,63}$/.test(trimmed)
      ? "Use lowercase letters, numbers, - or _, starting with a letter or number."
      : null;
  const canCreate = trimmed.length > 0 && !slugError && !create.isPending;

  return (
    <section aria-label="Project note" className="flex flex-col gap-3 border border-line rounded-panel p-4">
      <h3 className="font-display text-subtitle">Project note</h3>
      <p className="text-caption text-ink-muted">
        Promotion appends dated entries from a saved report into this one note. The app
        never creates, renames or deletes it for you, and it never rewrites existing
        lines.
      </p>

      <div className="min-h-12 text-caption" aria-live="polite">
        {status.isLoading ? (
          <p className="text-ink-muted">Checking for a project note…</p>
        ) : status.isError ? (
          <div className="flex flex-col items-start gap-2">
            <p role="alert" className="text-warn">
              Couldn't check the project note — {promotionError(status.error)}
            </p>
            <Button size="sm" onClick={() => status.refetch()}>Retry</Button>
          </div>
        ) : status.data?.count === 0 ? (
          <p className="text-ink-muted">No project note yet.</p>
        ) : status.data && status.data.count > 1 ? (
          <div role="alert" className="flex flex-col gap-1 text-warn">
            <p>
              More than one project note was found ({status.data.count}). Promotion
              needs exactly one.
            </p>
            <p>
              Keep one file in ScreenSearch/Projects/ and move or remove the others.
              ScreenSearch will not delete or rename them.
            </p>
          </div>
        ) : status.data?.path ? (
          <p className="text-ink-muted">
            Project note:{" "}
            <span className="break-all font-mono text-data">
              {forwardPath(status.data.path)}
            </span>
          </p>
        ) : null}
      </div>

      {status.data?.count === 0 && (
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (!canCreate) return;
            create.mutate(trimmed, { onSuccess: () => setSlug("") });
          }}
        >
          <Field
            label="Project note name"
            value={slug}
            onChange={(event) => setSlug(event.currentTarget.value)}
            placeholder="project-alpha"
            disabled={create.isPending}
            error={slugError}
            hint="Creates an empty note at ScreenSearch/Projects/<name>.md (no .md needed). Lowercase letters, numbers, - and _ only; the note stays empty until you promote and confirm."
            autoComplete="off"
            spellCheck={false}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button type="submit" disabled={!canCreate} aria-busy={create.isPending}>
              {create.isPending ? "Creating…" : "Create project note"}
            </Button>
            <span className="text-caption text-ink-muted">
              The app never names, renames or deletes this file for you.
            </span>
          </div>
        </form>
      )}

      {create.isError && (
        <p role="alert" className="text-caption text-warn">
          Couldn't create the note — {promotionError(create.error)}
        </p>
      )}
      {create.isSuccess && (
        <p role="status" className="text-caption text-ok">
          Empty note created
          {create.data?.path ? ` at ${forwardPath(create.data.path)}` : ""}. Open a
          saved report to promote items into it.
        </p>
      )}
    </section>
  );
}

export function MemoryPanel() {
  const status = useMemoryStatus();
  const runs = useMemoryRuns();
  const [draft, setDraft] = useState<MemorySettings | null>(null);
  const [token, setToken] = useState("");
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const save = useSetMemorySettings();
  const runNow = useMemoryMutation(cmd.runMemoryReport);
  const saveToken = useMemoryMutation(cmd.setTelegramToken);
  const clearToken = useMemoryMutation(cmd.clearTelegramToken);
  const reset = useMemoryMutation(cmd.resetMemorySourceIdentity);
  const preview = useMemorySchedulePreview(draft);

  useEffect(() => {
    if (status.data) setDraft((previous) => previous ?? displaySettings(status.data.settings));
  }, [status.data]);

  const set = <K extends keyof MemorySettings>(key: K, value: MemorySettings[K]) =>
    setDraft((previous) => previous ? { ...previous, [key]: value } : previous);
  const busy = save.isPending || saveToken.isPending || clearToken.isPending || reset.isPending;
  const loaded = Boolean(draft && status.data);
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(status.data ? displaySettings(status.data.settings) : undefined);
  const active = status.data?.worker_running || status.data?.latest_run?.phase === "queued" || status.data?.latest_run?.phase === "running";
  const selected = selectedRun ?? status.data?.latest_run?.run_id ?? null;
  const actionError = save.error ?? runNow.error ?? saveToken.error ?? clearToken.error ?? reset.error;
  const reportTimezone = status.data?.settings.timezone;
  const today = todayInZone(reportTimezone);
  const yesterday = previousDate(today);
  const selectedEnd = endDate || startDate;
  const dateError = startDate && selectedEnd < startDate
    ? "End date must be on or after the start date."
    : today && (startDate > today || selectedEnd > today) ? "Choose today or an earlier date." : null;
  const canRun = loaded && !dirty && !busy && !active && !runNow.isPending && Boolean(reportTimezone && status.data?.settings.output_folder);

  return (
    <Panel group title="Memory reports" action={<Chip tone={active ? "accent" : "neutral"}>{active ? "working" : status.data?.settings.enabled ? "scheduled" : "off"}</Chip>}>
      <div className="flex flex-col gap-4">
        <p className="text-caption text-ink-muted">Generate source-backed reports for a day or date range in a selected folder, with optional Telegram digests. Inference stays on this computer; schedules do not download missing models.</p>
        <div className="min-h-12" aria-live="polite">
          {status.isLoading && <p>Loading memory settings…</p>}
          {status.isError && <div><p role="alert" className="text-warn">Could not load memory status: {memoryError(status.error)}</p><Button onClick={() => status.refetch()}>Retry status</Button></div>}
          {status.data && !status.data.settings.enabled && <p className="text-ink-muted">Daily scheduling is off. Choose a timezone and folder before saving or running a report.</p>}
          {status.data?.settings.enabled && <p>Scheduled at {status.data.settings.local_time} in {status.data.settings.timezone}.</p>}
          {status.data?.worker_error && <p role="alert" className="text-warn">Report worker: {status.data.worker_error.message}</p>}
        </div>
        <div className="min-h-12 text-caption" aria-live="polite">
          {status.data?.latest_run ? (
            <p>
              Last run: {reportDates(status.data.latest_run)} · {status.data.latest_run.outcome ?? status.data.latest_run.phase}.
              {status.data.latest_run.outcome === "NO_DATA" && " No usable report evidence was available for this window, so no Telegram digest was sent."}
              {status.data.latest_run.error && ` ${status.data.latest_run.error.message}`}
              {" "}Open report history below for coverage and separate delivery receipts.
            </p>
          ) : <p className="text-ink-muted">{status.isLoading ? "Checking last run…" : status.data ? "No recorded report runs." : "Last run unavailable."}</p>}
        </div>
        <Toggle label="Schedule daily memory" checked={draft?.enabled ?? false} disabled={!loaded || busy} onChange={(v) => set("enabled", v)} hint="Starts with the first full day after activation; changes apply when you save. Disabled periods are not backfilled." />
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <Field label="Local report time" type="time" step={60} value={draft?.local_time ?? ""} disabled={!loaded || busy} onChange={(e) => set("local_time", e.currentTarget.value.slice(0, 5) || null)} />
          <Field label="Timezone (IANA)" value={draft?.timezone ?? ""} placeholder="Europe/Paris" disabled={!loaded || busy} onChange={(e) => set("timezone", e.currentTarget.value || null)} hint="Fixed to your selection, even if the computer timezone changes." />
        </div>
        <Field label="Output folder (absolute path)" placeholder="C:/Users/YourName/Documents/Notes" value={draft?.output_folder ?? ""} disabled={!loaded || busy} onChange={(e) => set("output_folder", forwardPath(e.currentTarget.value) || null)} hint="Daily notes go into ScreenSearch/Daily; selected-date reports go into ScreenSearch/Reports. ScreenSearch/Index.md links them. Edited or unrelated files are preserved." />
        <div className="min-h-12 text-caption text-ink-muted" aria-live="polite">
          {!draft?.timezone || !draft.local_time ? <p>Select time and timezone to preview the first report.</p> : preview.isError ? <p className="text-warn">{memoryError(preview.error)}</p> : preview.isFetching ? <p>Calculating first eligible report…</p> : preview.data ? <p>First eligible day for a new activation: {preview.data.local_date}. Due {new Date(preview.data.due_at).toLocaleString(undefined, { timeZone: draft.timezone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" })} ({draft.timezone}). Saving unchanged scheduling settings preserves the existing activation.</p> : <p>Complete the schedule to preview its first report.</p>}
        </div>
        <Toggle label="Send Telegram digests" checked={draft?.telegram_enabled ?? false} disabled={!loaded || busy} onChange={(v) => set("telegram_enabled", v)} hint="Explicitly permits saved-report digest content to leave this device for Telegram and the selected conversation. Off by default." />
        <p className="text-caption text-ink-muted">Text digests contain no screenshots or raw OCR. Credential-pattern masking is not comprehensive PII detection. No-data, blocked and failed reports are not sent.</p>
        <Field label="Telegram chat ID" value={draft?.telegram_chat_id ?? ""} disabled={!loaded || busy} onChange={(e) => set("telegram_chat_id", e.currentTarget.value || null)} />
        <Field label="Telegram bot token (write only)" type="password" autoComplete="new-password" value={token} disabled={!loaded || busy} onChange={(e) => setToken(e.currentTarget.value)} hint="Saved in Windows Credential Manager, never returned here. Saving a token does not send a test message." />
        <div className="flex flex-wrap gap-2 items-center">
          <Button disabled={!token.trim() || !loaded || busy} onClick={() => saveToken.mutate(token, { onSuccess: () => { setToken(""); saveToken.reset(); } })}>Save token</Button>
          <Button disabled={!status.data?.telegram_credential_available || busy} onClick={() => clearToken.mutate(undefined)}>Clear saved token</Button>
          <span className="text-caption text-ink-muted">{!status.data ? "Credential status unavailable" : status.data.telegram_credential_available ? "Saved credential available" : "No saved credential available"}</span>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={!draft || !dirty || busy || (draft.enabled && (!preview.data || preview.isFetching || preview.isError))} onClick={() => draft && save.mutate(draft, { onSuccess: (result) => setDraft(displaySettings(result.settings)) })}>{save.isPending ? "Saving…" : "Save memory settings"}</Button>
          <Button disabled={!loaded || busy || !dirty} onClick={() => status.data && setDraft(displaySettings(status.data.settings))}>Discard unsaved changes</Button>
        </div>
        {dirty && <p className="text-caption text-ink-muted">Unsaved changes. Save before running a report.</p>}
        <section aria-label="Run a memory report" className="flex flex-col gap-3 border border-line rounded-panel p-4">
          <h3 className="font-display text-subtitle">Run a report</h3>
          <p className="text-caption text-ink-muted">Uses the saved timezone, output folder and Telegram settings. Choosing dates does not generate or send anything.</p>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field label="Report date / start date" type="date" value={startDate} max={today ?? undefined} disabled={!loaded || runNow.isPending} onChange={(e) => setStartDate(e.currentTarget.value)} />
            <Field label="End date (optional)" type="date" value={endDate} min={startDate || undefined} max={today ?? undefined} disabled={!loaded || runNow.isPending} onChange={(e) => setEndDate(e.currentTarget.value)} hint="Leave empty for a single day. Both dates are included." error={dateError} />
          </div>
          <div className="min-h-12 text-caption" aria-live="polite">
            {!reportTimezone ? <p>Select and save a timezone and output folder before running.</p> : startDate ? <>
              <p>Selected: {startDate === selectedEnd ? startDate : `${startDate} through ${selectedEnd} (inclusive)`} · {reportTimezone}.</p>
              {selectedEnd === today && <p className="text-warn">Today is still in progress. This report ends when you start it and is labeled PARTIAL when evidence is available. It does not cover the full day.</p>}
            </> : <p>Choose a report date, or run the previous completed day{yesterday ? ` (${yesterday}, ${reportTimezone})` : ""}.</p>}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button disabled={!canRun || !startDate || Boolean(dateError)} onClick={() => runNow.mutate({ start_date: startDate, end_date: selectedEnd }, { onSuccess: (run) => setSelectedRun(run.run_id) })}>{runNow.isPending ? "Starting report…" : "Run selected dates"}</Button>
            <Button disabled={!canRun} onClick={() => runNow.mutate(undefined, { onSuccess: (run) => setSelectedRun(run.run_id) })}>Run previous day{yesterday ? ` (${yesterday})` : ""}</Button>
          </div>
        </section>
        <ProjectNoteSection />
        {actionError !== null && <p role="alert" className="text-warn">{memoryError(actionError)}</p>}
        {status.data?.missed_windows.length ? <div><h3 className="font-display text-subtitle">Older missed windows</h3><p>These windows were outside the automatic backfill cap and are not reported as covered.</p><ul>{status.data.missed_windows.map((window) => <li key={`${window.start}-${window.end}`}>{timestamp(window.start)} – {timestamp(window.end)}</li>)}</ul></div> : null}
        <details>
          <summary className="cursor-pointer min-h-hit-min font-display text-subtitle">Report history and delivery receipts</summary>
          <section aria-label="Report history" className="flex flex-col gap-3">
          {runs.isLoading && <p>Loading report history…</p>}
          {runs.isError && <div><p role="alert">{memoryError(runs.error)}</p><Button onClick={() => runs.refetch()}>Reload history</Button></div>}
          {runs.data?.length === 0 && <p className="text-ink-muted">No reports yet. Choose dates to run a report or enable a daily schedule.</p>}
          <div className="flex flex-wrap gap-2">{runs.data?.map((run) => <Button key={run.run_id} aria-pressed={selected === run.run_id} onClick={() => setSelectedRun(run.run_id)}>{reportDates(run)} · {run.outcome ?? run.phase}{run.revision_of ? " · revision" : ""}</Button>)}</div>
          {runs.data?.length === 100 && <p className="text-caption text-ink-muted">Showing the 100 most recent runs.</p>}
          {selected && <RunDetails key={selected} runId={selected} />}
          </section>
        </details>
        <details>
          <summary className="cursor-pointer min-h-hit-min">Profile identity for cloned installations</summary>
          <div className="flex flex-col gap-3">
            <p className="break-all text-caption font-mono">Source: {status.data?.source_id ?? "unavailable"}</p>
            <p>Moves and restores keep identity. For an independently active clone, disable scheduling and Telegram, wait for active work to finish, then reset identity. Old reports and note ownership stay unchanged; configure a new credential afterward.</p>
            <Button disabled={!loaded || active || busy || status.data?.settings.enabled || status.data?.settings.telegram_enabled} onClick={() => setConfirmReset(true)}>Reset cloned profile identity…</Button>
            {confirmReset && <div role="group" aria-label="Confirm profile identity reset" className="border border-warn p-3 flex flex-col gap-2"><p>Create a new source identity for future reports? Existing notes remain owned by the previous identity and may conflict.</p><div className="flex flex-wrap gap-2"><Button disabled={busy} onClick={() => reset.mutate(undefined, { onSuccess: () => setConfirmReset(false) })}>Confirm new identity</Button><Button disabled={busy} onClick={() => setConfirmReset(false)}>Cancel</Button></div></div>}
          </div>
        </details>
      </div>
    </Panel>
  );
}
