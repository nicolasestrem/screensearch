// Independent credential controls, embedded in ApiPanel even when listener status
// fails. Secret lifetime is confined to this mounted component, never Query state.
import { useEffect, useId, useRef, useState } from "react";
import type { ApiStatus } from "../../bindings/ApiStatus";
import { useApiReadTokenAction, useApiReadTokenStatus } from "../../lib/ipc/apiReadToken";
import type { ReadTokenAction } from "../../lib/ipc/apiReadTokenOptions";
import { Button, Skeleton } from "../primitives";

const results: Record<ReadTokenAction, string> = {
  create: "Read token created and hidden. Reveal or copy it to configure your client. API settings are unchanged.",
  rotate: "Read token rotated and hidden. Update your clients with the new token. The legacy token and API settings are unchanged.",
  revoke: "Read token revoked for subsequent requests. Already authorized streams may continue. The legacy token and API settings are unchanged.",
  reveal: "Read token revealed. Hide it when finished, or press Escape.",
  copy: "Read token copied to the clipboard. Clipboard contents remain until you replace or clear them.",
};
const progress: Record<ReadTokenAction, string> = {
  create: "Creating read token…", rotate: "Rotating read token…",
  revoke: "Revoking read token…", reveal: "Revealing read token…", copy: "Copying read token…",
};

export function ApiReadTokenControls({ apiStatus }: { apiStatus?: ApiStatus }) {
  const id = useId();
  const status = useApiReadTokenStatus();
  const [secret, setSecret] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [lastAction, setLastAction] = useState<ReadTokenAction | null>(null);
  const mounted = useRef(true);
  const inFlight = useRef(false);
  const actionGroup = useRef<HTMLDivElement | null>(null);
  const action = useApiReadTokenAction({
    reveal: setSecret,
    copy: (token) => navigator.clipboard.writeText(token),
    isActive: () => mounted.current,
  });
  const reset = action.reset;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      // Local state is discarded on unmount; reset detaches any mutation observer.
      reset();
    };
  }, [reset]);

  const configured = status.data?.configured;
  useEffect(() => {
    if (!configured) setSecret(null);
  }, [configured]);

  const run = (next: ReadTokenAction) => {
    if (inFlight.current) return;
    if (configured === undefined || (next === "create" ? configured : !configured)) return;
    inFlight.current = true;
    let succeeded = false;
    setError(null);
    setMessage("");
    setLastAction(next);
    // Hide before attempting changes, including failed saves. Never leave an old
    // token visible after rotation/revocation or automatically reveal its successor.
    if (next === "create" || next === "rotate" || next === "revoke") setSecret(null);
    action.mutate(next, {
      onSuccess: () => {
        succeeded = true;
        setMessage(results[next]);
      },
      onError: (failure) => setError(failure.message),
      onSettled: () => {
        inFlight.current = false;
        action.reset();
        // Disabling a pending button can drop browser focus. Restore it after the
        // settled render, moving to the next usable action after create/revoke.
        const focusAction = succeeded && next === "revoke" ? "create"
          : succeeded && next === "create" ? "reveal" : next;
        requestAnimationFrame(() => actionGroup.current?.querySelector<HTMLButtonElement>(`[data-read-action="${focusAction}"]`)?.focus());
      },
    });
  };
  const busy = action.isPending;
  // A failed refresh with a previously known value preserves the controls; unknown
  // availability never implies "absent" and cannot trigger an accidental create.
  const unknown = configured === undefined;
  const hide = () => { setSecret(null); setMessage("Read token hidden."); };
  const listener = !apiStatus ? "API listener status is unavailable; creating a credential does not establish connectivity."
    : !apiStatus.enabled ? "API disabled — the read credential does not enable a listener."
    : apiStatus.running ? "API listening — clients still need a reachable address and the read token."
    : apiStatus.last_error ? "API bind failed — the read credential is independent; correct the address or port above."
    : "API enabled but not listening — the credential does not establish connectivity.";

  return (
    <section aria-labelledby={`${id}-title`} aria-describedby={`${id}-scope`} className="flex flex-col gap-3 border-t border-line pt-4"
      onKeyDown={(event) => { if (event.key === "Escape" && secret) { hide(); event.stopPropagation(); } }}>
      <h3 id={`${id}-title`} className="text-subtitle text-ink font-display">Read-only token</h3>
      <p id={`${id}-scope`} className="text-caption text-ink-muted font-body">
        Give a memory agent access to captured raw and filtered history, stored images and saved reports.
        This token cannot Ask or generate answers/reports, change marks, capture now, retry reports or deliver content.
      </p>
      <p className="text-caption text-warn font-body">
        Plain HTTP is not encrypted: network observers may intercept tokens and history, including for explicitly configured remote wrapper clients.
        Keep the token private. Rotation and revocation affect subsequent requests, not guaranteed cancellation of already authorized streams.
      </p>
      <p className="text-caption text-ink-muted font-body">{listener}</p>
      <div aria-busy={status.isLoading} className="flex flex-col gap-2">
        <div className="min-h-hit-min flex items-center" role="status" aria-live="polite">
          {status.isLoading ? <><Skeleton className="h-6 w-40" /><span className="sr-only">Loading read-token availability…</span></>
            : <span className="text-caption text-ink-muted">{unknown ? "Read-token availability unknown."
              : configured ? "Read token configured." : "No read token configured. Create one to give an agent read-only access."}</span>}
        </div>
        <div id={`${id}-value`} className="min-h-hit-min min-w-0 rounded-chip border border-line bg-base px-3 py-2 font-mono text-data text-ink">
          {status.isLoading ? <Skeleton className="h-6 w-full" />
            : <code className="block min-h-6 break-all">{secret && configured ? secret : configured ? "Read token hidden" : unknown ? "Availability unknown" : "No read token"}</code>}
        </div>
        <div ref={actionGroup} className="flex flex-wrap items-center gap-2" role="group" aria-label="Read-only token actions">
          <Button data-read-action="create" size="sm" variant="secondary" disabled={busy || unknown || configured} onClick={() => run("create")}>Create read token</Button>
          <Button data-read-action="reveal" size="sm" variant="ghost" disabled={busy || !configured} aria-controls={`${id}-value`} aria-pressed={Boolean(secret)}
            onClick={() => secret ? hide() : run("reveal")}>{secret ? "Hide read token" : "Reveal read token"}</Button>
          <Button data-read-action="copy" size="sm" variant="ghost" disabled={busy || !configured} onClick={() => run("copy")}>Copy read token</Button>
          <Button data-read-action="rotate" size="sm" variant="ghost" disabled={busy || !configured} onClick={() => run("rotate")}>Rotate read token</Button>
          <Button data-read-action="revoke" size="sm" variant="danger" disabled={busy || !configured} onClick={() => run("revoke")}>Revoke read token</Button>
        </div>
        <p className="text-caption text-ink-muted font-body">
          Create works only when absent. Rotate replaces the existing read token; update your clients afterward.
          Revoke removes only this credential. These actions never enable the API, change its address/port or replace the legacy token.
        </p>
        <div className="min-h-hit-min text-caption font-body" role="status" aria-live="polite">
          {busy && lastAction ? progress[lastAction] : message}
        </div>
        {(status.isError || error) && <div role="alert" className="flex flex-col gap-2 text-caption text-warn">
          {status.isError && <div className="flex flex-wrap items-center gap-2">
            <span>Could not refresh read-token availability. Any previous status may be out of date.</span>
            <Button size="sm" variant="secondary" disabled={busy || status.isFetching} onClick={() => status.refetch()}>Retry read-token status</Button>
          </div>}
          {error && <div className="flex flex-wrap items-center gap-2">
            <span>{error}</span>
            <Button size="sm" variant="secondary" disabled={busy || unknown || !lastAction || (lastAction === "create" ? configured : !configured)} onClick={() => { if (lastAction) run(lastAction); }}>Retry read-token action</Button>
          </div>}
        </div>}
      </div>
    </section>
  );
}
