// UpdateIndicator — the NavRail-footer updater surface (0.3.2 PR2; UI_REFERENCE §3/§4/§5,
// `03 §11b`). Two quiet concerns, one component:
//
//   1. A **presence indicator** — a small tone dot + version — rendered ONLY when an
//      update exists (available / downloading / ready). It links to Settings, where the
//      update installs. Never a count (the no-badge-counts rule, D4); `idle`/`error`
//      render nothing here (an error surfaces only in the Settings · General line, §4).
//      No update → zero presence: the element does not exist, it is not "shown as zero".
//   2. A **manual "Check for updates"** control (the quick-menu affordance, #57 / gap
//      #99), shaped like the footer's Command button — a lifecycle control, not an
//      availability badge. It is hidden once an update is staged (`ready`): the action
//      then is Restart, reached via the presence dot's link to Settings; re-checking a
//      staged update could clobber it to `error` and strand the verified installer
//      (matches AppPanel, §4).
//
// Pull-based + quiet (D1): the check runs in the background; a found update downloads on
// its own and installs only from Settings on a user-initiated restart. No modal, no nag,
// no toast on success. Hidden entirely outside the packaged app (browser dev).
import { NavLink } from "react-router-dom";
import { isTauri } from "@tauri-apps/api/core";

import { cn } from "../../lib/cn";
import { useUpdateStatus } from "../../lib/ipc/queries";
import { useCheckForUpdates } from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";

const FOCUS =
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

export function UpdateIndicator() {
  // Hooks first (Rules of Hooks), then the runtime gate.
  const status = useUpdateStatus();
  const check = useCheckForUpdates();
  if (!isTauri()) return null;

  const state = status.data?.state ?? "idle";
  const version =
    status.data?.state === "available" ||
    status.data?.state === "downloading" ||
    status.data?.state === "ready"
      ? status.data.version
      : null;
  const checking = check.isPending || state === "checking";
  const hasUpdate = version != null;
  const ready = state === "ready";
  // Disable the manual check while a check or a background download is in flight.
  const busy = checking || state === "available" || state === "downloading";

  const runCheck = () => {
    check.mutate(undefined, {
      onError: (e) => toast.error(`Could not check for updates: ${String(e)}`),
    });
  };

  return (
    <>
      {/* Presence indicator — only while an update exists (§4). Quiet functional tone
          (status-ok when ready, secondary text while still downloading), never the bold
          accent, never a count. Links to Settings, where the restart-to-update lives. */}
      {hasUpdate && (
        <NavLink
          to="/settings"
          aria-label={
            ready
              ? `Update v${version} ready — open Settings to restart`
              : `Update v${version} downloading — open Settings`
          }
          className={cn(
            "flex min-h-control min-w-0 items-center gap-2 rounded-md px-3",
            "text-t-sm font-text transition-colors duration-fast ease-standard motion-reduce:transition-none",
            ready
              ? "text-status-ok-fg hover:bg-surface-hover"
              : "text-fg-secondary hover:bg-surface-hover hover:text-fg-primary",
            FOCUS,
          )}
        >
          <span
            className={cn(
              "h-2 w-2 shrink-0 rounded-pill bg-current",
              !ready && "opacity-70",
            )}
            aria-hidden="true"
          />
          <span className="font-mono text-t-data">v{version}</span>
          <span className="truncate">update</span>
        </NavLink>
      )}

      {/* Manual check — the quick-menu affordance, shaped like the Command button. Hidden
          once an update is staged (`ready`): the action then is Restart (via the dot's link
          to Settings), and re-checking could clobber the staged update to `error`. */}
      {!ready && (
        <button
          type="button"
          onClick={runCheck}
          disabled={busy}
          className={cn(
            "flex min-h-control w-full min-w-0 items-center justify-between gap-2 rounded-md px-3",
            "border border-line-subtle bg-surface-raised text-t-sm font-text text-fg-secondary",
            "transition-colors duration-fast ease-standard motion-reduce:transition-none",
            "hover:border-line-strong hover:bg-surface-hover hover:text-fg-primary",
            "active:bg-surface-selected",
            "disabled:pointer-events-none disabled:text-fg-disabled",
            FOCUS,
          )}
        >
          <span className="min-w-0 truncate">
            {checking ? "Checking…" : "Check for updates"}
          </span>
        </button>
      )}
    </>
  );
}
