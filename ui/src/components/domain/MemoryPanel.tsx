import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import { handleExternalLinkClick } from "../../lib/openExternal";
import { Button, Chip, Field, Panel, Toggle } from "../primitives";
import type { MemorySettings } from "../../bindings/MemorySettings";
import type { ReportDelivery } from "../../bindings/ReportDelivery";
import * as cmd from "../../lib/ipc/commands";
import {
  memoryError, useMemoryDeliveries, useMemoryMutation, useMemoryRun,
  useMemoryRuns, useMemorySchedulePreview, useMemoryStatus,
} from "../../lib/ipc/memory";

function timestamp(value: number) {
  return new Date(value).toLocaleString();
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
      {receipt.path && <p className="break-all font-mono text-data">{receipt.path}</p>}
      {receipt.error && <p className="text-warn">{receipt.error.message}</p>}
      {receipt.recovery_path && (
        <p className="break-all">Preserved recovery file: {receipt.recovery_path}</p>
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
  const run = detail.data;
  const active = run?.phase === "queued" || run?.phase === "running";
  return (
    <section aria-label="Selected report" className="flex flex-col gap-3 border border-line rounded-panel p-4">
      {!run && <p>{detail.isError ? memoryError(detail.error) : "Loading report…"}</p>}
      {detail.isError && <Button onClick={() => detail.refetch()}>Reload report</Button>}
      {run && <>
        <h3 className="font-display text-subtitle">{run.local_date} · {run.outcome ?? run.phase}</h3>
        <p className="text-caption text-ink-muted">{timestamp(run.window_start)} – {timestamp(run.window_end)} · {run.timezone}</p>
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
        {run.result && <>
          <p className={run.outcome === "PARTIAL" ? "text-warn" : "text-ink-muted"}>
            {run.outcome === "PARTIAL" && "Partial coverage. "}
            {run.result.frames_summarized} of {run.result.frames_sampled} sampled frames summarized;
            {" "}{run.result.periods_covered}/{run.result.periods_total} periods covered.
            {run.result.truncated && " Output was bounded or truncated."} Sampling is not exhaustive capture.
          </p>
          <p className="text-caption">Model: {run.result.model ?? "none — no model ran"}</p>
          <details>
            <summary className="cursor-pointer min-h-hit-min">Saved report and evidence IDs</summary>
            <div className="prose prose-deck max-w-none break-words">
              <ReactMarkdown components={{
                a: ({ href, children }) => <a href={href} target="_blank" rel="noreferrer" onClick={(event) => handleExternalLinkClick(event, href)}>{children}</a>,
                img: ({ alt }) => <span>{alt ? `[Image omitted: ${alt}]` : "[Image omitted]"}</span>,
              }}>{run.result.markdown}</ReactMarkdown>
            </div>
            <p className="text-caption break-words">Frames: {run.result.frame_ids.join(", ") || "none"}</p>
            <p className="text-caption break-words">Sessions: {run.result.session_ids.join(", ") || "none"}</p>
            <p className="text-caption text-ink-muted">IDs record generation-time evidence; retention may have removed the original frames.</p>
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

export function MemoryPanel() {
  const status = useMemoryStatus();
  const runs = useMemoryRuns();
  const [draft, setDraft] = useState<MemorySettings | null>(null);
  const [token, setToken] = useState("");
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const save = useMemoryMutation(cmd.setMemorySettings);
  const runNow = useMemoryMutation(cmd.runMemoryReport);
  const saveToken = useMemoryMutation(cmd.setTelegramToken);
  const clearToken = useMemoryMutation(cmd.clearTelegramToken);
  const reset = useMemoryMutation(cmd.resetMemorySourceIdentity);
  const preview = useMemorySchedulePreview(draft);

  useEffect(() => {
    if (status.data) setDraft((previous) => previous ?? status.data.settings);
  }, [status.data]);

  const set = <K extends keyof MemorySettings>(key: K, value: MemorySettings[K]) =>
    setDraft((previous) => previous ? { ...previous, [key]: value } : previous);
  const busy = save.isPending || saveToken.isPending || clearToken.isPending || reset.isPending;
  const loaded = Boolean(draft && status.data);
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(status.data?.settings);
  const active = status.data?.worker_running || status.data?.latest_run?.phase === "queued" || status.data?.latest_run?.phase === "running";
  const selected = selectedRun ?? status.data?.latest_run?.run_id ?? null;
  const actionError = save.error ?? runNow.error ?? saveToken.error ?? clearToken.error ?? reset.error;

  return (
    <Panel group title="Memory reports" action={<Chip tone={active ? "accent" : "neutral"}>{active ? "working" : status.data?.settings.enabled ? "scheduled" : "off"}</Chip>}>
      <div className="flex flex-col gap-4">
        <p className="text-caption text-ink-muted">Generate source-backed daily notes in a selected folder, with optional Telegram digests. Inference stays on this computer; schedules do not download missing models.</p>
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
              Last run: {status.data.latest_run.local_date} · {status.data.latest_run.outcome ?? status.data.latest_run.phase}.
              {status.data.latest_run.error && ` ${status.data.latest_run.error.message}`}
              {" "}Open report history below for coverage and separate delivery receipts.
            </p>
          ) : <p className="text-ink-muted">{status.isLoading ? "Checking last run…" : status.data ? "No recorded report runs." : "Last run unavailable."}</p>}
        </div>
        <Toggle label="Schedule daily memory" checked={draft?.enabled ?? false} disabled={!loaded || busy} onChange={(v) => set("enabled", v)} hint="Starts with the first full day after activation; changes apply when you save. Disabled periods are not backfilled." />
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <Field label="Local report time" type="time" value={draft?.local_time ?? ""} disabled={!loaded || busy} onChange={(e) => set("local_time", e.currentTarget.value || null)} />
          <Field label="Timezone (IANA)" value={draft?.timezone ?? ""} placeholder="Europe/Paris" disabled={!loaded || busy} onChange={(e) => set("timezone", e.currentTarget.value || null)} hint="Fixed to your selection, even if the computer timezone changes." />
        </div>
        <Field label="Output folder (absolute path)" value={draft?.output_folder ?? ""} disabled={!loaded || busy} onChange={(e) => set("output_folder", e.currentTarget.value || null)} hint="Notes go into ScreenSearch/Daily; ScreenSearch/Index.md links them. Edited or unrelated files are preserved." />
        <div className="min-h-12 text-caption text-ink-muted" aria-live="polite">
          {!draft?.timezone || !draft.local_time ? <p>Select time and timezone to preview the first report.</p> : preview.isError ? <p className="text-warn">{memoryError(preview.error)}</p> : preview.isFetching ? <p>Calculating first eligible report…</p> : preview.data ? <p>First eligible day for a new activation: {preview.data.local_date}. Due {new Date(preview.data.due_at).toLocaleString(undefined, { timeZone: draft.timezone })} ({draft.timezone}). Saving unchanged scheduling settings preserves the existing activation.</p> : <p>Complete the schedule to preview its first report.</p>}
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
          <Button disabled={!draft || !dirty || busy || (draft.enabled && (!preview.data || preview.isFetching || preview.isError))} onClick={() => draft && save.mutate(draft, { onSuccess: (result) => setDraft(result.settings) })}>{save.isPending ? "Saving…" : "Save memory settings"}</Button>
          <Button disabled={!loaded || busy || !dirty} onClick={() => status.data && setDraft(status.data.settings)}>Discard unsaved changes</Button>
          <Button disabled={!loaded || dirty || busy || active || runNow.isPending || !status.data?.settings.timezone || !status.data.settings.output_folder} onClick={() => runNow.mutate(undefined, { onSuccess: (run) => setSelectedRun(run.run_id) })}>Run previous day now</Button>
        </div>
        {dirty && <p className="text-caption text-ink-muted">Unsaved changes. Save before running a report.</p>}
        {actionError !== null && <p role="alert" className="text-warn">{memoryError(actionError)}</p>}
        {status.data?.missed_windows.length ? <div><h3 className="font-display text-subtitle">Older missed windows</h3><p>These windows were outside the automatic backfill cap and are not reported as covered.</p><ul>{status.data.missed_windows.map((window) => <li key={`${window.start}-${window.end}`}>{timestamp(window.start)} – {timestamp(window.end)}</li>)}</ul></div> : null}
        <details>
          <summary className="cursor-pointer min-h-hit-min font-display text-subtitle">Report history and delivery receipts</summary>
          <section aria-label="Report history" className="flex flex-col gap-3">
          {runs.isLoading && <p>Loading report history…</p>}
          {runs.isError && <div><p role="alert">{memoryError(runs.error)}</p><Button onClick={() => runs.refetch()}>Reload history</Button></div>}
          {runs.data?.length === 0 && <p className="text-ink-muted">No reports yet. Run the previous day manually or enable a schedule.</p>}
          <div className="flex flex-wrap gap-2">{runs.data?.map((run) => <Button key={run.run_id} aria-pressed={selected === run.run_id} onClick={() => setSelectedRun(run.run_id)}>{run.local_date} · {run.outcome ?? run.phase}{run.revision_of ? " · revision" : ""}</Button>)}</div>
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
