// Persistent main-window upgrade notice. An overlay preserves route geometry; the
// backend marker survives navigation, closing to tray, disablement and restart.
import { Link } from "react-router-dom";

import { Button } from "../primitives";
import { API_THREAT_MODEL } from "../../lib/apiCopy";
import { useApiStatus } from "../../lib/ipc/queries";
import { useAcknowledgeApiNetworkNotice } from "../../lib/ipc/mutations";

export function ApiNetworkNotice() {
  const status = useApiStatus();
  const acknowledge = useAcknowledgeApiNetworkNotice();
  const pending = status.data?.network_notice_pending;
  const unavailable = status.isError && !status.data;

  if (!pending && !unavailable) return null;

  return (
    <section
      aria-labelledby="api-network-notice-title"
      aria-live="polite"
      className="fixed bottom-4 right-4 left-4 sm:left-auto sm:max-w-lg z-overlay rounded-panel border border-warn bg-overlay p-4 font-body text-body text-ink"
    >
      <h2 id="api-network-notice-title" className="font-display text-subtitle">
        {unavailable ? "API upgrade notice unavailable" : "Your API network access has changed"}
      </h2>
      {unavailable ? (
        <div className="mt-2 flex flex-col gap-3">
          <p>Could not check the API upgrade notice. Retry to check its status.</p>
          <div>
            <Button onClick={() => status.refetch()} disabled={status.isFetching}>
              {status.isFetching ? "Checking…" : "Retry"}
            </Button>
          </div>
        </div>
      ) : (
        <div className="mt-2 flex flex-col gap-3">
          <p>
            Your API was enabled before this update. It now binds to all IPv4
            interfaces (0.0.0.0) when enabled, allowing connections from other
            reachable computers.
          </p>
          <p>{API_THREAT_MODEL}</p>
          <p className="text-caption text-ink-muted">
            {status.data?.running
              ? `Currently listening on 0.0.0.0:${status.data.port}.`
              : status.data?.enabled
                ? "The API is enabled but is not currently listening. Check Settings for its status."
                : "The API is currently disabled; nothing is listening."}
          </p>
          {acknowledge.isError && (
            <p role="alert" className="text-caption text-warn">
              Could not save your acknowledgement. The notice remains pending.
              Try again while the main window is visible.
            </p>
          )}
          {status.isError && (
            <p role="alert" className="text-caption text-warn">
              Could not refresh API status; the state above is the last known state.
            </p>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={() => acknowledge.mutate()}
              disabled={acknowledge.isPending}
            >
              {acknowledge.isPending ? "Saving…" : "I understand"}
            </Button>
            <Link
              to="/settings"
              className="inline-flex items-center min-h-hit-min text-ink underline"
            >
              {status.data?.enabled ? "Open Settings to disable API" : "Open API Settings"}
            </Link>
          </div>
        </div>
      )}
    </section>
  );
}
