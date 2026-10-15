// ApiPanel — the Settings · HTTP API surface (0.3.0 PR7; UI_REFERENCE §4/§5, `03 §7c`).
//
// Off by default. Enabling binds 0.0.0.0:<port> and mints a bearer token; the panel
// reveals/copies/regenerates it. A port already in use is the loud guided-change state:
// the server does not start (enabled && !running && last_error), and the panel shows the
// warning + a "pick another port" retry — mirroring the D6 hotkey-conflict pattern.
//
// Five states (UI_REFERENCE §4): loading → skeleton; error → load failed + retry;
// off (empty-off) → "API disabled" + the threat-model copy; enabling (partial) →
// "Binding…"; populated → "Listening on 0.0.0.0:<port>" + token controls.
import { useEffect, useState } from "react";

import { Button, Chip, Field, Panel, Skeleton, Toggle } from "../primitives";
import { useApiStatus } from "../../lib/ipc/queries";
import {
  useRegenerateApiToken,
  useSetApiConfig,
} from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";
import { API_THREAT_MODEL } from "../../lib/apiCopy";

const DEFAULT_PORT = 43210;
const PORT_MIN = 1024;
const PORT_MAX = 65535;

export function ApiPanel() {
  const status = useApiStatus();
  const setConfig = useSetApiConfig();
  const regenerate = useRegenerateApiToken();

  const [portDraft, setPortDraft] = useState(String(DEFAULT_PORT));
  const [revealed, setRevealed] = useState(false);

  // Seed the port field from the loaded status once it arrives (and whenever the backend
  // reports a different port), without fighting the user mid-edit.
  const loadedPort = status.data?.port;
  useEffect(() => {
    if (loadedPort != null) setPortDraft(String(loadedPort));
  }, [loadedPort]);

  // Loading → skeleton that matches the final layout (no spinner-only screens, §4).
  if (status.isLoading || !status.data) {
    if (status.isError) {
      return (
        <Panel group title="HTTP API">
          <div className="flex flex-col gap-3">
            <span className="text-body text-ink-muted font-body">
              Could not load the HTTP API status.
            </span>
            <div>
              <Button variant="secondary" onClick={() => status.refetch()}>
                Retry
              </Button>
            </div>
          </div>
        </Panel>
      );
    }
    return (
      <Panel group title="HTTP API">
        <div className="flex flex-col gap-3">
          <Skeleton className="h-6 w-40" />
          <Skeleton className="h-16 w-full" />
        </div>
      </Panel>
    );
  }

  const s = status.data;
  const busy = setConfig.isPending || regenerate.isPending;
  const bindFailed = s.enabled && !s.running && Boolean(s.last_error);

  // Clamp to the backend's accepted band so a stray value never fails IPC deserialization
  // into `u16` (the backend clamps to the floor too; this keeps the UI honest).
  const parsedPort = (): number => {
    const n = Number.parseInt(portDraft, 10);
    if (!Number.isInteger(n)) return DEFAULT_PORT;
    return Math.max(PORT_MIN, Math.min(PORT_MAX, n));
  };

  const toggle = (next: boolean) => {
    setConfig.mutate(
      { enabled: next, port: next ? parsedPort() : null },
      {
        onSuccess: (result) => {
          if (!next) {
            toast.info("HTTP API disabled.");
          } else if (result.running) {
            toast.success(`HTTP API listening on 0.0.0.0:${result.port}.`);
          }
          // A bind failure (enabled but not running) already surfaced a warning toast
          // from the backend; the inline warning below carries the guided retry.
        },
        onError: (e) => toast.error(`Could not change the HTTP API: ${String(e)}`),
      },
    );
  };

  // (Re)start the server on the drafted port — used both to retry after a bind failure and
  // to apply a port change while the server is running (a config change is a restart).
  const restartOnPort = () => {
    setConfig.mutate(
      { enabled: true, port: parsedPort() },
      {
        onSuccess: (result) => {
          if (result.running) {
            toast.success(`HTTP API listening on 0.0.0.0:${result.port}.`);
          }
        },
        onError: (e) => toast.error(`Could not restart the HTTP API: ${String(e)}`),
      },
    );
  };

  // While running, the drafted port differs from the live one → offer an explicit apply.
  const portChangedWhileRunning = s.running && parsedPort() !== s.port;

  const copyToken = async () => {
    if (!s.token) return;
    try {
      await navigator.clipboard.writeText(s.token);
      toast.success("Token copied to the clipboard.");
    } catch {
      toast.error("Could not copy the token.");
    }
  };

  const regenerateToken = () => {
    regenerate.mutate(undefined, {
      onSuccess: () => toast.success("Generated a new token."),
      onError: (e) => toast.error(`Could not regenerate the token: ${String(e)}`),
    });
  };

  const statusChip = setConfig.isPending ? (
    <Chip tone="accent">{setConfig.variables?.enabled ? "binding…" : "stopping…"}</Chip>
  ) : s.running ? (
    <Chip tone="ok">listening</Chip>
  ) : bindFailed ? (
    <Chip tone="warn">port in use</Chip>
  ) : s.enabled ? (
    <Chip tone="warn">not listening</Chip>
  ) : (
    <Chip tone="neutral">disabled</Chip>
  );

  return (
    <Panel group title="HTTP API" action={statusChip}>
      <div className="flex flex-col gap-4">
        {/* Section intro — plain language, from the user's side (UI_REFERENCE §9). */}
        <p className="text-caption text-ink-muted font-body">
          Lets scripts and agents on this computer or a trusted network read your
          history through a token-protected API. Leave it off unless you use one.
        </p>
        <Toggle
          label="Enable HTTP API"
          checked={s.enabled}
          onChange={toggle}
          disabled={busy}
          hint="Listens on all IPv4 interfaces (0.0.0.0). Off by default."
        />

        <p className="text-caption text-warn font-body">{API_THREAT_MODEL}</p>

        {/* Off (empty-off): nothing else to show — the toggle + threat model is the state. */}
        {!s.enabled && (
          <p className="text-caption text-ink-faint font-body">
            API disabled — nothing is listening.
          </p>
        )}

        {/* Enabling (partial). */}
        {setConfig.isPending && (
          <p className="text-caption text-ink-muted font-body">
            {setConfig.variables?.enabled
              ? `Binding 0.0.0.0:${parsedPort()}…`
              : "Stopping the HTTP API…"}
          </p>
        )}

        {/* Port field: editable while enabling/retrying or when off. */}
        {s.enabled && (
          <Field
            label="Port"
            type="number"
            min={1024}
            max={65535}
            value={portDraft}
            onChange={(e) => setPortDraft(e.currentTarget.value)}
            disabled={busy}
            error={bindFailed ? s.last_error : null}
            hint="All IPv4 interfaces (not configurable). Changing the port restarts the server."
          />
        )}

        {/* Apply a port change while running — otherwise the edited port is inert (D6 mirror). */}
        {portChangedWhileRunning && (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={restartOnPort} disabled={busy}>
              Restart on {parsedPort()}
            </Button>
            <span className="text-caption text-ink-faint">
              Port {parsedPort()} differs from the running port {s.port}.
            </span>
          </div>
        )}

        {/* Loud guided-change: the port was in use, so offer a retry (D6 mirror). */}
        {bindFailed && (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={restartOnPort} disabled={busy}>
              Retry on this port
            </Button>
            <span className="text-caption text-ink-faint">
              Pick another port above, then retry.
            </span>
          </div>
        )}

        {/* Populated: listening — show the bearer token controls. */}
        {s.running && (
          <div className="flex flex-col gap-2">
            <span className="text-body text-ink font-body">
              Listening on 0.0.0.0:{s.port} (all IPv4 interfaces)
            </span>
            <p className="text-caption text-ink-muted">
              Local clients use <code className="font-mono">http://127.0.0.1:{s.port}</code>.
              Other computers use this computer’s network address and port {s.port};
              0.0.0.0 is a listen address, not a client destination. Local MCP stays on 127.0.0.1.
            </p>
            <p className="text-caption text-ink-muted">
              Remote access may need a Windows Firewall inbound rule for this port on
              your trusted network. ScreenSearch does not change firewall rules. An HTTP
              401 means the server was reached but the token was rejected; a connection
              timeout may mean the address, routing or firewall needs checking.
            </p>
            <label className="text-caption text-ink-muted font-body">
              Bearer token
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-chip border border-line bg-base px-3 py-2 font-mono text-data text-ink">
                {s.token
                  ? revealed
                    ? s.token
                    : "•".repeat(24)
                  : "no token"}
              </code>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setRevealed((r) => !r)}
                aria-pressed={revealed}
              >
                {revealed ? "Hide" : "Reveal"}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={copyToken}
                disabled={!s.token}
              >
                Copy
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={regenerateToken}
                disabled={busy}
              >
                Regenerate
              </Button>
            </div>
            <span className="text-caption text-ink-faint">
              Send it as <code className="font-mono">Authorization: Bearer …</code>.
              Regenerating takes effect immediately.
            </span>
          </div>
        )}
      </div>
    </Panel>
  );
}
