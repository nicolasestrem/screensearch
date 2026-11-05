// QuickActions — the NavRail-footer quick menu (0.3.2 PR3, #57; UI_REFERENCE §3). Two
// lifecycle toggles that mirror the tray's quick actions and round-trip through the same
// commands: load/unload the answer model, and start/stop vision tagging. Quiet by design
// (raised surface, functional text, never the accent — telemetry-footer discipline, like
// the version link). Hidden entirely outside the packaged app (browser dev).
//
// State is read, not tracked: the answer-model label follows the live `sidecar_status`
// (loaded = the answer model is the resident sidecar model); the vision label follows the
// live job counts (active = any vision_tag job pending/running). Both caches are kept live
// by useLiveEvents, so the labels flip on their own after the tray or Settings act too.
import { isTauri } from "@tauri-apps/api/core";

import { cn } from "../../lib/cn";
import { useJobStats, useSidecarStatus } from "../../lib/ipc/queries";
import {
  useCancelVision,
  useEnqueueVision,
  useLoadModel,
  useUnloadModel,
} from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";

// One quiet footer control shape, shared with the Command and update buttons in NavRail.
const QUICK_ACTION = cn(
  "flex min-h-control w-full min-w-0 items-center justify-between gap-2 rounded-md px-3",
  "border border-line-subtle bg-surface-raised text-t-sm font-text text-fg-secondary",
  "transition-colors duration-fast ease-standard motion-reduce:transition-none",
  "hover:border-line-strong hover:bg-surface-hover hover:text-fg-primary",
  "active:bg-surface-selected",
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
  "disabled:pointer-events-none disabled:text-fg-disabled",
);

export function QuickActions() {
  // Hooks first (Rules of Hooks), then the runtime gate.
  const sidecar = useSidecarStatus();
  const jobs = useJobStats();
  const loadModel = useLoadModel();
  const unloadModel = useUnloadModel();
  const enqueueVision = useEnqueueVision();
  const cancelVision = useCancelVision();
  if (!isTauri()) return null;

  // The answer model is "loaded" only when it is the resident sidecar model (one model at
  // a time) — a resident vision model correctly reads as "Load answer model".
  const answerLoaded =
    sidecar.data?.state === "ready" && sidecar.data?.lane === "answer";
  const modelBusy = loadModel.isPending || unloadModel.isPending;

  const visionActive =
    (jobs.data?.vision_pending ?? 0) + (jobs.data?.vision_running ?? 0) > 0;
  const visionBusy = enqueueVision.isPending || cancelVision.isPending;

  const toggleModel = () => {
    if (answerLoaded) {
      unloadModel.mutate(undefined, {
        onSuccess: () => toast.success("Answer model unloaded"),
        onError: (e) => toast.error(String(e)),
      });
    } else {
      loadModel.mutate("answer", {
        onSuccess: () => toast.success("Answer model loaded"),
        onError: (e) => toast.error(String(e)),
      });
    }
  };

  const toggleVision = () => {
    if (visionActive) {
      cancelVision.mutate(undefined, {
        onSuccess: (n) =>
          toast.info(
            n > 0
              ? `Stopped vision tagging — cleared ${n} queued`
              : "Stopped vision tagging",
          ),
        onError: (e) => toast.error(String(e)),
      });
    } else {
      // Start = tag the untagged backlog now (capped server-side); Stop cancels it.
      enqueueVision.mutate(
        { kind: "range", start: 0, end: Date.now() },
        {
          onSuccess: (n) =>
            n > 0
              ? toast.success(`Vision tagging started — ${n} frames queued`)
              : toast.info("No untagged frames to tag"),
          onError: (e) => toast.error(String(e)),
        },
      );
    }
  };

  return (
    <>
      <button
        type="button"
        onClick={toggleModel}
        disabled={modelBusy}
        className={QUICK_ACTION}
      >
        <span className="min-w-0 truncate">
          {answerLoaded ? "Unload answer model" : "Load answer model"}
        </span>
      </button>
      <button
        type="button"
        onClick={toggleVision}
        disabled={visionBusy}
        className={QUICK_ACTION}
      >
        <span className="min-w-0 truncate">
          {visionActive ? "Stop vision tagging" : "Start vision tagging"}
        </span>
      </button>
    </>
  );
}
