// The single live-event subscription manager, mounted once in AppShell. It keeps
// server-state caches fresh from the backend event stream (UI_REFERENCE §6):
//   • readiness_changed → patch the readiness cache directly (no refetch)
//   • job_progress      → patch the jobStats cache directly
//   • job_completed     → surgical frame/search/insights invalidation
//   • sidecar_status    → patch the sidecarStatus cache + refresh readiness
//   • capture_tick      → high-frequency; debounce-invalidate timeline + insights
// `answer_delta` is intentionally NOT handled here — useAsk owns it.
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { listenTo } from "./events";
import { queryKeys } from "./queryKeys";
import { toast as toastStore } from "../../state/toastStore";
import type { Readiness } from "../../bindings/Readiness";
import type { SetupStatus } from "../../bindings/SetupStatus";
import { newerSetupStatus } from "../setup";
import { memoryKeys } from "./memory";

/** Debounce window for coalescing capture_tick bursts into one invalidation. */
const TICK_DEBOUNCE_MS = 800;
/** Debounce window for coalescing job_progress bursts (e.g. an enrichment backlog drain). */
const ENRICH_DEBOUNCE_MS = 1000;

export function useLiveEvents() {
  const qc = useQueryClient();

  useEffect(() => {
    let active = true;
    const unlisteners: UnlistenFn[] = [];
    const track = (p: Promise<UnlistenFn>) =>
      p
        .then((u) => {
          if (active) unlisteners.push(u);
          else u();
        })
        .catch(() => {
          /* no Tauri runtime (dev): no live events */
        });

    track(
      listenTo("readiness_changed", (readiness) => {
        qc.setQueryData(queryKeys.readiness, readiness);
        if (readiness.sidecar.status === "ready") {
          qc.invalidateQueries({ queryKey: queryKeys.sidecarDevices });
        }
      }),
    );

    track(
      listenTo("job_progress", (stats) => {
        qc.setQueryData(queryKeys.jobStats, stats);
      }),
    );

    let enrichTimer: ReturnType<typeof setTimeout> | undefined;
    track(
      listenTo("job_completed", (completed) => {
        qc.setQueryData(queryKeys.jobStats, completed.stats);
        qc.invalidateQueries({ queryKey: queryKeys.frame(completed.frame_id) });
        if (completed.kind === "embed_text") {
          qc.invalidateQueries({ queryKey: queryKeys.searchPrefix });
        }
        if (completed.kind === "vision_tag") {
          if (enrichTimer) clearTimeout(enrichTimer);
          enrichTimer = setTimeout(() => {
            qc.invalidateQueries({ queryKey: queryKeys.insightsPrefix });
          }, ENRICH_DEBOUNCE_MS);
        }
      }),
    );

    track(
      listenTo("sidecar_status", (status) => {
        qc.setQueryData(queryKeys.sidecarStatus, status);
        // The kernel also re-emits readiness on a sidecar transition, but nudge a
        // refetch so the StatusRail stays truthful even if that event is missed.
        qc.invalidateQueries({ queryKey: queryKeys.readiness });
        qc.invalidateQueries({ queryKey: queryKeys.sidecarDevices });
      }),
    );

    track(
      listenTo("model_download", (status) => {
        qc.setQueryData(queryKeys.modelDownload, status);
        const model = status.model ?? `${status.lane} model`;
        if (status.phase === "done") {
          toastStore.success(`Downloaded ${model}`);
        } else if (status.phase === "failed") {
          toastStore.error(`Download failed: ${status.error ?? model}`);
        }
      }),
    );

    let tickTimer: ReturnType<typeof setTimeout> | undefined;
    track(
      listenTo("capture_tick", () => {
        if (tickTimer) clearTimeout(tickTimer);
        tickTimer = setTimeout(() => {
          qc.invalidateQueries({ queryKey: queryKeys.timelinePrefix });
          qc.invalidateQueries({ queryKey: queryKeys.insightsPrefix });
          // New frames change the newest-first lists (timeline thumbnails, deck
          // recents) and a viewed moment's neighbour context; refresh them alongside
          // the density ribbon.
          qc.invalidateQueries({ queryKey: queryKeys.framesPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.frameContextPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.sessionsPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.jobStats });
          qc.invalidateQueries({ queryKey: queryKeys.storageStats });
          const readiness = qc.getQueryData<Readiness>(queryKeys.readiness);
          if (readiness?.embed_model.status !== "ready") {
            qc.invalidateQueries({ queryKey: queryKeys.searchPrefix });
          }
        }, TICK_DEBOUNCE_MS);
      }),
    );

    track(
      listenTo("throttle_changed", (status) => {
        qc.setQueryData(queryKeys.throttleStatus, status);
      }),
    );

    track(
      listenTo("sessions_changed", () => {
        qc.invalidateQueries({ queryKey: queryKeys.sessionsPrefix });
      }),
    );

    track(
      listenTo("memory_status_changed", () => {
        qc.invalidateQueries({ queryKey: memoryKeys.all });
      }).then((unlisten) => {
        if (active) qc.invalidateQueries({ queryKey: memoryKeys.all });
        return unlisten;
      }),
    );

    // The first status request can precede async API startup. Subscribe, then
    // refresh once to cover a startup event emitted before registration. Cancel
    // an initial in-flight read first: invalidation alone could reuse that stale
    // request when the query has no data yet.
    const refreshApiStatus = async () => {
      if (!active) return;
      await qc.cancelQueries({ queryKey: queryKeys.apiStatus });
      if (active) {
        await qc.invalidateQueries({ queryKey: queryKeys.apiStatus });
      }
    };
    track(
      listenTo("api_status_changed", () => {
        void refreshApiStatus();
      }).then((unlisten) => {
        void refreshApiStatus();
        return unlisten;
      }),
    );

    // A mark was created / resolved / annotated (possibly from the overlay window's own
    // JS context via the mark hotkey) — refresh the Deck's Intentions strip.
    track(
      listenTo("marks_changed", () => {
        qc.invalidateQueries({ queryKey: queryKeys.marks });
      }),
    );

    // Auto-update state transitions (0.3.2 PR2). Mirror straight into the cache — no
    // toast, no nag: the updater is quiet and pull-based (`03 §11b`, D1).
    track(
      listenTo("update_status_changed", (status) => {
        qc.setQueryData(queryKeys.updateStatus, status);
      }),
    );

    track(
      listenTo("setup_changed", (incoming) => {
        // Revision guard (03 §7g): an older snapshot — a lagged, replayed, or
        // out-of-order event — never downgrades the cache. The event path and
        // the queryFn share the same pure predicate.
        const cached = qc.getQueryData<SetupStatus | null>(queryKeys.setupStatus);
        if (newerSetupStatus(incoming, cached)) {
          qc.setQueryData(queryKeys.setupStatus, incoming);
        }
      }),
    );

    track(
      listenTo("toast", (t) => {
        toastStore[t.level](t.message);
        if (t.message.toLowerCase().includes("retention")) {
          qc.invalidateQueries({ queryKey: queryKeys.storageStats });
          qc.invalidateQueries({ queryKey: queryKeys.framesPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.timelinePrefix });
          qc.invalidateQueries({ queryKey: queryKeys.insightsPrefix });
          // Degrade-to-text flips image_purged on existing rows, so a frame's singular
          // detail, its spans, and any search hits are now stale (they still point at the
          // just-deleted image). Refresh them so an open Moment/Recall switches to the text
          // reconstruction instead of fetching a now-missing image.
          qc.invalidateQueries({ queryKey: queryKeys.framePrefix });
          qc.invalidateQueries({ queryKey: queryKeys.frameSpansPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.searchPrefix });
          qc.invalidateQueries({ queryKey: queryKeys.sessionsPrefix });
        }
      }),
    );

    return () => {
      active = false;
      if (tickTimer) clearTimeout(tickTimer);
      if (enrichTimer) clearTimeout(enrichTimer);
      unlisteners.forEach((u) => u());
    };
  }, [qc]);
}
