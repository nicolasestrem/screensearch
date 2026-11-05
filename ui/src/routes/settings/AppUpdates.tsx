// The version + update section General inherits from the 0.3.2 App panel
// (`UI_REFERENCE §3`, §4 "Settings · App"). The updater is pull-based and quiet:
// `idle` renders no status line at all, nothing installs without a user-initiated
// restart, and a check that cannot even start gets a toast while its persistent
// state stays inline.
import type { UpdateStatus } from "../../bindings/UpdateStatus";
import { Button, Skeleton } from "../../components/primitives";
import {
  useCheckForUpdates,
  useRestartToApplyUpdate,
} from "../../lib/ipc/mutations";
import { useUpdateStatus } from "../../lib/ipc/queries";
import { useAppVersion } from "../../lib/useAppVersion";
import { toast } from "../../state/toastStore";
import { failureMessage } from "./frameState";
import { SettingSection } from "./SettingRow";

/** The version a pending update carries; only three updater states carry one. */
function pendingVersion(status: UpdateStatus): string | null {
  switch (status.state) {
    case "available":
    case "downloading":
    case "ready":
      return status.version;
    default:
      return null;
  }
}

export function AppUpdates() {
  const status = useUpdateStatus();
  const check = useCheckForUpdates();
  const restart = useRestartToApplyUpdate();
  const version = useAppVersion();
  const data = status.data;
  const state = data?.state ?? null;
  const checking = check.isPending || state === "checking";
  // A background download is in flight for available/downloading: the manual
  // command has already returned, so the check stays disabled until it settles.
  const busy = checking || state === "available" || state === "downloading";
  const pending = data ? pendingVersion(data) : null;

  function runCheck() {
    check.mutate(undefined, {
      onError: (error) =>
        toast.error(`Couldn’t check for updates: ${failureMessage(error)}`),
    });
  }

  function applyUpdate() {
    // Success hands the process to the passive installer, so only the error path
    // is ever observed here.
    restart.mutate(undefined, {
      onError: (error) =>
        toast.error(`Couldn’t install the update: ${failureMessage(error)}`),
    });
  }

  return (
    <SettingSection
      title="App and updates"
      description="Updates download quietly in the background and install only when you restart — nothing updates on its own."
    >
      <div className="flex min-w-0 flex-col gap-3">
        <p className="min-w-0 text-t-sm text-fg-secondary">
          Version{" "}
          <span
            className={
              version
                ? "font-mono text-t-data text-fg-primary"
                : "font-mono text-t-data text-fg-tertiary"
            }
          >
            {version ?? "unavailable outside the desktop app"}
          </span>
        </p>

        {data === undefined ? (
          status.isError ? (
            <div
              role="status"
              className="flex min-w-0 flex-wrap items-center gap-2 text-t-sm text-status-warn-fg"
            >
              <span className="min-w-0">
                Couldn’t read the update status:{" "}
                {failureMessage(status.error)}.
              </span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void status.refetch()}
              >
                Retry
              </Button>
            </div>
          ) : (
            <Skeleton w="third" h="control" />
          )
        ) : (
          <>
            {state === "checking" ? (
              <p className="min-w-0 text-t-sm text-fg-secondary">
                Checking for updates…
              </p>
            ) : null}

            {(state === "available" || state === "downloading") && pending ? (
              <p className="min-w-0 text-t-sm text-fg-secondary">
                Version {pending} is downloading in the background.
              </p>
            ) : null}

            {state === "ready" && pending ? (
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <span className="min-w-0 text-t-sm text-fg-primary">
                  Version {pending} is ready — restart to install it.
                </span>
                <Button
                  variant="primary"
                  onClick={applyUpdate}
                  disabled={restart.isPending}
                >
                  {restart.isPending ? "Restarting…" : "Restart to update"}
                </Button>
              </div>
            ) : null}

            {data.state === "error" ? (
              <div
                role="status"
                className="flex min-w-0 flex-col gap-1 text-t-sm text-status-warn-fg"
              >
                <span className="min-w-0">Couldn’t check for updates.</span>
                <span className="min-w-0 text-t-xs text-fg-tertiary">
                  {data.message}
                </span>
              </div>
            ) : null}

            {state !== "ready" ? (
              <div className="min-w-0">
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={runCheck}
                  disabled={busy}
                >
                  {checking
                    ? "Checking…"
                    : state === "error"
                      ? "Try again"
                      : "Check for updates"}
                </Button>
              </div>
            ) : null}
          </>
        )}
      </div>
    </SettingSection>
  );
}
