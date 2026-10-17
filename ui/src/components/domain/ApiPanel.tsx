// ApiPanel — the Settings · HTTP API surface (0.3.0 PR7; UI_REFERENCE §4/§5, `03 §7c`).
//
// Off by default. Enabling binds the configured IPv4/IPv6 address (default 127.0.0.1) and mints a bearer token; the panel
// reveals/copies/regenerates it. A port already in use is the loud guided-change state:
// the server does not start (enabled && !running && last_error), and the panel shows the
// warning + a "pick another port" retry — mirroring the D6 hotkey-conflict pattern.
//
// Five states (UI_REFERENCE §4): loading → skeleton; error → load failed + retry;
// off (empty-off) → "API disabled" + the threat-model copy; enabling (partial) →
// "Binding…"; populated → "Listening on <configured-address>:<port>" + token controls.
import { useEffect, useState } from "react";

import { Button, Chip, Field, Panel, Skeleton, Toggle } from "../primitives";
import { useApiStatus } from "../../lib/ipc/queries";
import {
  useRegenerateApiToken,
  useSetApiConfig,
} from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";
import { API_THREAT_MODEL } from "../../lib/apiCopy";
import { ApiReadTokenControls } from "./ApiReadTokenControls";

const DEFAULT_PORT = 43210;
const socketAddress = (address: string, port: number) =>
  `${address.includes(":") ? `[${address}]` : address}:${port}`;
const PORT_MIN = 1024;
const PORT_MAX = 65535;

export function ApiPanel() {
  const status = useApiStatus();
  const setConfig = useSetApiConfig();
  const regenerate = useRegenerateApiToken();

  const [portDraft, setPortDraft] = useState(String(DEFAULT_PORT));
  const [revealed, setRevealed] = useState(false);
  const [addressDraft, setAddressDraft] = useState("127.0.0.1");

  // Seed the port field from the loaded status once it arrives (and whenever the backend
  // reports a different port), without fighting the user mid-edit.
  const loadedPort = status.data?.port;
  useEffect(() => {
    if (loadedPort != null) setPortDraft(String(loadedPort));
  }, [loadedPort]);

  const loadedAddress = status.data?.bind_address;
  useEffect(() => {
    if (loadedAddress != null) setAddressDraft(loadedAddress);
  }, [loadedAddress]);

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
            <ApiReadTokenControls />
          </div>
        </Panel>
      );
    }
    return (
      <Panel group title="HTTP API">
        <div className="flex flex-col gap-3">
          <Skeleton className="h-6 w-40" />
          <Skeleton className="h-16 w-full" />
          <ApiReadTokenControls />
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
      { enabled: next, port: next ? parsedPort() : null, bindAddress: next ? addressDraft.trim() : null },
      {
        onSuccess: (result) => {
          if (!next) {
            toast.info("HTTP API disabled.");
          } else if (result.running) {
            toast.success(`HTTP API listening on port ${result.port}. See the panel for client connection details.`);
          }
          // A bind failure (enabled but not running) already surfaced a warning toast
          // from the backend; the inline warning below carries the guided retry.
        },
        onError: (e) => toast.error(`Could not change the HTTP API: ${String(e)}`),
      },
    );
  };

  // Save the selected endpoint, restarting only when the API is enabled.
  const applyEndpoint = () => {
    setConfig.mutate(
      { enabled: s.enabled, port: parsedPort(), bindAddress: addressDraft.trim() },
      {
        onSuccess: (result) => {
          if (result.running) {
            toast.success(`HTTP API listening on port ${result.port}. See the panel for client connection details.`);
          } else if (!result.enabled) {
            toast.success("API address and port saved; the API remains disabled.");
          }
        },
        onError: (e) => toast.error(`Could not apply the API address and port: ${String(e)}`),
      },
    );
  };

  // Draft changes are applied explicitly; enabling also applies the selected endpoint.
  const endpointChanged = parsedPort() !== s.port || addressDraft.trim() !== s.bind_address;

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
    <Chip tone="warn">bind failed</Chip>
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
          hint="Off by default. The default address 127.0.0.1 accepts connections only from this computer."
        />

        <p className="text-caption text-warn font-body">{API_THREAT_MODEL}</p>

        {/* Disabled profiles can configure the endpoint before enabling. */}
        {!s.enabled && (
          <p className="text-caption text-ink-faint font-body">
            API disabled — nothing is listening.
          </p>
        )}

        {/* Enabling (partial). */}
        {setConfig.isPending && (
          <p className="text-caption text-ink-muted font-body">
            {setConfig.variables?.enabled
              ? `Binding ${socketAddress(addressDraft.trim(), parsedPort())}…`
              : s.running ? "Stopping the HTTP API…" : "Saving API settings…"}
          </p>
        )}

        <Field
          label="Bind address"
          value={addressDraft}
          onChange={(event) => setAddressDraft(event.currentTarget.value)}
          disabled={busy}
          spellCheck={false}
          autoCapitalize="none"
          hint="IPv4 or IPv6 literal only, without a port or brackets. Use 127.0.0.1 or ::1 for this computer; choose 0.0.0.0, :: or a network-interface address explicitly for network access."
        />
        {/* Address and port remain editable while disabled. */}
        <Field
            label="Port"
            type="number"
            min={1024}
            max={65535}
            value={portDraft}
            onChange={(e) => setPortDraft(e.currentTarget.value)}
            disabled={busy}
            error={bindFailed ? s.last_error : null}
            hint="Changing the address or port restarts an enabled server."
          />

        {/* Explicitly apply address/port changes without changing enabled state. */}
        {endpointChanged && (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={applyEndpoint} disabled={busy}>
              {s.enabled ? "Apply address and restart" : "Save address and port"}
            </Button>
            <span className="text-caption text-ink-faint">
              Configured endpoint: {socketAddress(s.bind_address, s.port)}.
            </span>
          </div>
        )}

        {/* Loud guided-change: the port was in use, so offer a retry (D6 mirror). */}
        {bindFailed && (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={applyEndpoint} disabled={busy}>
              Retry with this address and port
            </Button>
            <span className="text-caption text-ink-faint">
              Check that the address belongs to this computer and the port is available, then retry.
            </span>
          </div>
        )}

        {setConfig.isError && (
          <p role="alert" className="text-caption text-warn">Could not apply API settings: {String(setConfig.error)}</p>
        )}

        {/* Populated: listening — show the bearer token controls. */}
        {s.running && (
          <div className="flex flex-col gap-2">
            <span className="text-body text-ink font-body">
              Listening on {socketAddress(s.bind_address, s.port)}
            </span>
            <p className="text-caption text-ink-muted">
              {s.bind_address === "0.0.0.0" || s.bind_address === "::" ? (
                <>Wildcard listen addresses are not client destinations. Use this computer’s actual network address and port {s.port}; for local clients use {s.bind_address === "::" ? "::1" : "127.0.0.1"}.</>
              ) : (
                <>Clients use <code className="font-mono">http://{socketAddress(s.bind_address, s.port)}</code>.</>
              )}
              {" "}MCP accepts loopback destinations only. Its default 127.0.0.1 URL requires a 127.0.0.1 or 0.0.0.0 listener. For IPv6 loopback, configure the MCP URL with [::1].
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
        <ApiReadTokenControls apiStatus={s} />
      </div>
    </Panel>
  );
}
