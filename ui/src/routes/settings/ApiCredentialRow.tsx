// The content of the `api.token` row (`03 §7c`, `03 §7f`, `UI_REFERENCE §12`): the
// legacy bearer token (reveal, copy, regenerate behind a confirm that names what
// stops working) and, independently, the read credential. The read credential is
// embedded unmodified: its bytes never enter a query cache, it hands the token to a
// local receiver only, and it never enables the API or changes address/port.
//
// Nothing here echoes a token into copy, an error or a status line: reveal is the
// only way to see one, and regeneration hides whatever was shown before.
import { useState } from "react";

import type { ApiStatus } from "../../bindings/ApiStatus";
import { ApiReadTokenControls } from "../../components/domain/ApiReadTokenControls";
import { Button } from "../../components/primitives";
import { API_THREAT_MODEL } from "../../lib/apiCopy";
import { useRegenerateApiToken } from "../../lib/ipc/mutations";
import { ConfirmAction } from "./ConfirmAction";

/** Fixed-width mask, so hiding a token never changes the row's geometry. */
const MASK = "••••••••••••••••";

export interface ApiCredentialRowProps {
  /** Live listener/credential status; undefined until `get_api_status` resolves. */
  apiStatus: ApiStatus | undefined;
}

export function ApiCredentialRow({ apiStatus }: ApiCredentialRowProps) {
  const regenerate = useRegenerateApiToken();
  const [revealed, setRevealed] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [message, setMessage] = useState("");
  const [failure, setFailure] = useState<string | null>(null);

  const token = apiStatus?.token ?? null;
  const unknown = apiStatus === undefined;
  const busy = regenerate.isPending;

  const copy = async (): Promise<void> => {
    if (token === null) return;
    try {
      await navigator.clipboard.writeText(token);
      setFailure(null);
      setMessage("Bearer token copied to the clipboard.");
    } catch {
      setMessage("");
      setFailure(
        "Couldn't copy the token. Reveal it and copy it by hand.",
      );
    }
  };

  const regenerateNow = (): void => {
    regenerate.mutate(undefined, {
      onSuccess: () => {
        setConfirming(false);
        // The old token is gone: never leave it — or its successor — on screen.
        setRevealed(false);
        setFailure(null);
        setMessage(
          "New bearer token generated. Reveal or copy it to update your clients.",
        );
      },
      onError: (error: unknown) => {
        setConfirming(false);
        setMessage("");
        setFailure(
          `Couldn't regenerate the token: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      },
    });
  };

  return (
    <div className="flex w-full min-w-0 flex-col gap-4">
      <section aria-label="Bearer token" className="flex min-w-0 flex-col gap-2">
        <h3 className="text-t-ui font-medium text-fg-primary">Bearer token</h3>
        <code className="min-w-0 break-all rounded-sm border border-line-subtle bg-surface-inset px-3 py-2 font-mono text-t-data text-fg-primary">
          {unknown
            ? "Reading the credential…"
            : token === null
              ? "No token yet — one is generated the first time you enable the API."
              : revealed
                ? token
                : MASK}
        </code>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            aria-pressed={revealed}
            disabled={unknown || token === null}
            onClick={() => {
              setRevealed((shown) => !shown);
              setMessage("");
            }}
          >
            {revealed ? "Hide" : "Reveal"}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={unknown || token === null || busy}
            onClick={() => void copy()}
          >
            Copy
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={unknown || token === null || busy}
            onClick={() => setConfirming(true)}
          >
            Regenerate
          </Button>
        </div>
        <p className="text-t-xs text-fg-tertiary">
          Send it as <code className="font-mono">Authorization: Bearer …</code>.
          Regenerating takes effect immediately.
        </p>
        <p className="text-t-xs text-status-warn-fg">{API_THREAT_MODEL}</p>
        <span
          role="status"
          aria-live="polite"
          className="min-w-0 text-t-xs text-fg-tertiary"
        >
          {message}
        </span>
        {failure ? (
          <p role="alert" className="min-w-0 text-t-sm text-status-danger-fg">
            {failure}
          </p>
        ) : null}
      </section>

      <ApiReadTokenControls apiStatus={apiStatus} />

      <ConfirmAction
        open={confirming}
        onCancel={() => setConfirming(false)}
        onConfirm={regenerateNow}
        title="Regenerate the bearer token"
        destroys="Every client still sending the current bearer token stops working: the token is replaced immediately and the old one is deleted. The read credential is not affected."
        confirmLabel="Regenerate token"
        pending={busy}
        danger
      />
    </div>
  );
}
