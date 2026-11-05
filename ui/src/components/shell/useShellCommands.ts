// The command palette's entry list (UI_REFERENCE §3): jump-to destinations plus the
// lifecycle actions. Kept beside the palette rather than inside it so the palette file
// stays a keyboard/filtering surface, and so one place owns "which commands exist".
//
// Jump-to comes from `NAV_ENTRIES` (the rail and the palette can never drift apart) and
// from `SETTING_CATEGORIES`, which is how Settings stays reachable by name: "Settings ·
// Capture" lands on that category directly. `/timeline` keeps its own entry even though
// the rail folds it under Recall — a detail surface is still a destination people search
// for.
import { useMemo, type ComponentType } from "react";
import { useNavigate } from "react-router-dom";

import { NAV_ENTRIES } from "../../app/nav";
import { SETTING_CATEGORIES } from "../../routes/settings/registry";
import { useJobStats, useSidecarStatus } from "../../lib/ipc/queries";
import {
  useCancelVision,
  useCaptureControl,
  useEnqueueVision,
  useLoadModel,
  useUnloadModel,
} from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";
import {
  IconCapture,
  IconCpu,
  IconSettings,
  IconTag,
  IconTimeline,
} from "../icons";

export interface Command {
  id: string;
  label: string;
  /** Extra search terms: route path, category id, synonyms. */
  keywords: readonly string[];
  icon: ComponentType<{ size?: number }>;
  run: () => void;
}

export function useShellCommands(): Command[] {
  const navigate = useNavigate();
  const capture = useCaptureControl();
  const sidecar = useSidecarStatus();
  const jobs = useJobStats();
  const loadModel = useLoadModel();
  const unloadModel = useUnloadModel();
  const enqueueVision = useEnqueueVision();
  const cancelVision = useCancelVision();
  // Mirror QuickActions (NavRail footer): only the contextually valid half of each toggle
  // is listed, so the palette never offers a re-`preload()` that isn't guaranteed
  // idempotent ("Load" while loaded) or an "Unload"/"Stop" with nothing to act on (a noisy
  // error/no-op toast). The answer model is "loaded" only when it is the resident model.
  const answerLoaded =
    sidecar.data?.state === "ready" && sidecar.data?.lane === "answer";
  const visionActive =
    (jobs.data?.vision_pending ?? 0) + (jobs.data?.vision_running ?? 0) > 0;

  return useMemo<Command[]>(() => {
    const toggleCapture = (control: "start" | "stop") =>
      capture.mutate(control, {
        onSuccess: () =>
          toast.success(
            control === "start" ? "Capture started" : "Capture stopped",
          ),
        onError: (e) => toast.error(String(e)),
      });

    const list: Command[] = [
      ...NAV_ENTRIES.map((entry) => ({
        id: `nav-${entry.label.toLowerCase()}`,
        label: `Go to ${entry.label}`,
        keywords: [entry.to, entry.label],
        icon: entry.icon,
        run: () => navigate(entry.to),
      })),
      // Settings is one shell with eight categories: one entry per category, so a search
      // for a category name (or the job it does) lands on that pane in one step.
      ...SETTING_CATEGORIES.map((category) => ({
        id: `settings-${category.id}`,
        label: `Settings · ${category.label}`,
        keywords: ["settings", category.id, category.summary],
        icon: IconSettings,
        run: () => navigate(`/settings/${category.id}`),
      })),
      {
        id: "timeline",
        label: "Go to Timeline",
        keywords: ["timeline", "captures", "history", "recall"],
        icon: IconTimeline,
        run: () => navigate("/timeline"),
      },
      {
        id: "start-capture",
        label: "Start capture",
        keywords: ["capture", "record", "resume", "start"],
        icon: IconCapture,
        run: () => toggleCapture("start"),
      },
      {
        id: "stop-capture",
        label: "Stop capture",
        keywords: ["capture", "pause", "stop"],
        icon: IconCapture,
        run: () => toggleCapture("stop"),
      },
    ];

    // Lifecycle quick actions (0.3.2 PR3, #57) — the same commands as the tray and the
    // NavRail footer, reachable from the keyboard here too.
    if (answerLoaded) {
      list.push({
        id: "unload-answer-model",
        label: "Unload answer model",
        keywords: ["model", "answer", "unload", "free memory"],
        icon: IconCpu,
        run: () =>
          unloadModel.mutate(undefined, {
            onSuccess: () => toast.success("Answer model unloaded"),
            onError: (e) => toast.error(String(e)),
          }),
      });
    } else {
      list.push({
        id: "load-answer-model",
        label: "Load answer model",
        keywords: ["model", "answer", "load"],
        icon: IconCpu,
        run: () =>
          loadModel.mutate("answer", {
            onSuccess: () => toast.success("Answer model loaded"),
            onError: (e) => toast.error(String(e)),
          }),
      });
    }

    if (visionActive) {
      list.push({
        id: "stop-vision",
        label: "Stop vision tagging",
        keywords: ["vision", "tag", "stop", "cancel"],
        icon: IconTag,
        run: () =>
          cancelVision.mutate(undefined, {
            onSuccess: (n) =>
              n > 0
                ? toast.info(`Stopped vision tagging — cleared ${n} queued`)
                : toast.info("Stopped vision tagging"),
            onError: (e) => toast.error(String(e)),
          }),
      });
    } else {
      list.push({
        id: "start-vision",
        label: "Start vision tagging",
        keywords: ["vision", "tag", "start", "describe"],
        icon: IconTag,
        run: () =>
          enqueueVision.mutate(
            { kind: "range", start: 0, end: Date.now() },
            {
              onSuccess: (n) =>
                n > 0
                  ? toast.success(`Vision tagging started — ${n} frames queued`)
                  : toast.info("No untagged frames to tag"),
              onError: (e) => toast.error(String(e)),
            },
          ),
      });
    }

    return list;
  }, [
    navigate,
    capture,
    answerLoaded,
    visionActive,
    loadModel,
    unloadModel,
    enqueueVision,
    cancelVision,
  ]);
}
