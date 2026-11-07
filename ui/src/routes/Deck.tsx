// Deck (/) — the at-a-glance home (UI_REFERENCE §3/§4): capture status, today's
// activity, and where to jump back in. Drives onboarding. All five states:
//   loading   → skeletons matching the final layout
//   error     → readiness probe failed (kernel offline) → retry
//   empty     → no captures yet → "start capture" onboarding
//   partial   → capturing but nothing enriched yet → pending note + queue meter
//   populated → today's aggregates + recent frames + density minimap
import { useNavigate } from "react-router-dom";

import {
  Button,
  Chip,
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
} from "../components/primitives";
import {
  FrameTile,
  IntentionsStrip,
  JobQueueMeter,
  TimelineMinimap,
  WhereWasICard,
} from "../components/domain";
import { IconCapture } from "../components/icons";
import { useCaptureControl } from "../lib/ipc/mutations";
import {
  useFrames,
  useInsights,
  useJobStats,
  useReadiness,
  useTimeline,
} from "../lib/ipc/queries";
import { toast } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";
import { statusLabel, statusTone } from "../lib/status";
import { todayRange } from "../lib/timeRanges";
import type { ComponentStatus } from "../bindings/ComponentStatus";

const RECENT_LIMIT = 8;
const TODAY_BUCKETS = 96; // 15-minute slices across the day

// Mirrors the populated Deck layout (hero · Today/Queue · WhereWasI/Intentions · recents)
// so loading -> populated reserves the final dimensions and doesn't shift (D9 no-CLS).
function DeckSkeleton() {
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
      <Skeleton className="h-28 w-full" />
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Skeleton className="h-48 w-full" />
        <Skeleton className="h-48 w-full" />
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Skeleton className="h-40 w-full" />
        <Skeleton className="h-40 w-full" />
      </div>
      <Skeleton className="h-64 w-full" />
    </div>
  );
}

export function Component() {
  const navigate = useNavigate();
  const readiness = useReadiness();
  const jobStats = useJobStats();
  const capture = useCaptureControl();
  const setSelectedRange = useUiStore((s) => s.setSelectedRange);

  const today = todayRange();
  const recentsRange = { start: 0, end: today.end }; // newest captures, all-time
  const recents = useFrames(recentsRange, RECENT_LIMIT);
  const insights = useInsights(today, TODAY_BUCKETS);
  const todayDensity = useTimeline(today, TODAY_BUCKETS);

  if (readiness.isLoading) return <DeckSkeleton />;

  if (readiness.isError || !readiness.data) {
    return (
      <div className="p-6">
        <ErrorState
          title="Can't reach the kernel"
          message={String(readiness.error ?? "The backend isn't responding.")}
          onRetry={() => readiness.refetch()}
        />
      </div>
    );
  }

  const captureStatus: ComponentStatus = readiness.data.capture.status;
  const captureOn =
    captureStatus === "ready" || captureStatus === "initializing";
  const startCapture = () =>
    capture.mutate("start", {
      onSuccess: () => toast.success("Capture started"),
      onError: (e) => toast.error(String(e)),
    });
  const stopCapture = () =>
    capture.mutate("stop", {
      onSuccess: () => toast.info("Capture stopped"),
      onError: (e) => toast.error(String(e)),
    });

  const hasHistory = (recents.data?.length ?? 0) > 0;
  const total = insights.data?.total_frames ?? 0;
  const tagged = insights.data?.tagged_frames ?? 0;
  const enrichmentPending = total > 0 && tagged === 0;

  const captureHero = (
    <Panel
      title="Capture"
      action={
        <Chip tone={statusTone(captureStatus)} dot>
          {statusLabel(captureStatus)}
        </Chip>
      }
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <p className="text-t-body text-fg-secondary">
          {captureOn
            ? "Recording your screen. Everything stays on this device."
            : "Capture is off. Start it to begin recording your screen."}
          {readiness.data.capture.detail
            ? ` (${readiness.data.capture.detail})`
            : ""}
        </p>
        {captureOn ? (
          <Button
            variant="secondary"
            onClick={stopCapture}
            disabled={capture.isPending}
          >
            Stop capture
          </Button>
        ) : (
          <Button
            variant="primary"
            leadingIcon={<IconCapture size={16} />}
            onClick={startCapture}
            disabled={capture.isPending}
          >
            Start capture
          </Button>
        )}
      </div>
    </Panel>
  );

  // Empty / onboarding: no frames exist yet at all.
  if (recents.isSuccess && !hasHistory) {
    return (
      <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
        {captureHero}
        <Panel title="Get started">
          <EmptyState
            icon={<IconCapture size={28} />}
            title={
              captureOn ? "Waiting for the first capture" : "No captures yet"
            }
            description={
              captureOn
                ? "Capture is running — your first frame will appear here in a moment."
                : "Start capture to begin recording. Captured screens become searchable by text and meaning."
            }
            action={
              captureOn ? undefined : (
                <Button
                  variant="primary"
                  leadingIcon={<IconCapture size={16} />}
                  onClick={startCapture}
                >
                  Start capture
                </Button>
              )
            }
          />
        </Panel>
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
      {captureHero}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Panel
          title="Today"
          action={
            enrichmentPending ? (
              <Chip tone="warn">enrichment pending</Chip>
            ) : tagged > 0 ? (
              <Chip tone="ok">{tagged} tagged</Chip>
            ) : undefined
          }
        >
          {insights.isLoading ? (
            <Skeleton className="h-24 w-full" />
          ) : insights.isError ? (
            <ErrorState
              title="Couldn't load today's activity"
              message={String(insights.error)}
              onRetry={() => insights.refetch()}
            />
          ) : total === 0 ? (
            <p className="text-t-body text-fg-secondary">
              No captures today yet.
            </p>
          ) : (
            <div className="flex flex-col gap-4">
              <div className="flex items-baseline gap-2">
                <span className="font-mono text-t-display text-fg-primary">{total}</span>
                <span className="text-t-body text-fg-secondary">
                  captures today
                </span>
              </div>
              {todayDensity.isError ? (
                <ErrorState
                  title="Couldn't load today's density"
                  message={String(todayDensity.error)}
                  onRetry={() => todayDensity.refetch()}
                />
              ) : todayDensity.isLoading ? (
                // Reserve the minimap band so it doesn't push the chips down when the
                // density query resolves after insights (D9 no-CLS).
                <Skeleton className="h-7 w-full" />
              ) : todayDensity.data && todayDensity.data.length > 0 ? (
                <TimelineMinimap
                  buckets={todayDensity.data}
                  range={today}
                  onSeek={(t) => {
                    setSelectedRange(today);
                    navigate(`/timeline?t=${t}`);
                  }}
                />
              ) : null}
              {insights.data && insights.data.top_apps.length > 0 && (
                <div className="flex min-h-7 flex-wrap gap-2">
                  {insights.data.top_apps.slice(0, 5).map((a, i) => (
                    <Chip key={a.app ?? `unknown-${i}`} tone="neutral">
                      {a.app ?? "Unknown"} · {a.count}
                    </Chip>
                  ))}
                </div>
              )}
            </div>
          )}
        </Panel>

        <Panel title="Enrichment queue">
          {jobStats.isError ? (
            <ErrorState
              title="Couldn't load queue"
              message={String(jobStats.error)}
              onRetry={() => jobStats.refetch()}
            />
          ) : jobStats.data ? (
            <JobQueueMeter stats={jobStats.data} />
          ) : (
            <Skeleton className="h-12 w-full" />
          )}
        </Panel>
      </div>

      {/* 0.3.0 flow recall (PR6): resume the last sustained context, and act on marks. */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <WhereWasICard onOpen={(id) => navigate(`/timeline/${id}`)} />
        <IntentionsStrip onOpen={(id) => navigate(`/timeline/${id}`)} />
      </div>

      <Panel
        title="Jump back in"
        action={
          <Button
            variant="ghost"
            size="sm"
            onClick={() => navigate("/timeline")}
          >
            Open timeline
          </Button>
        }
      >
        {recents.isLoading ? (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
            {Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="aspect-video w-full" />
            ))}
          </div>
        ) : recents.isError ? (
          <ErrorState
            title="Couldn't load recent captures"
            message={String(recents.error)}
            onRetry={() => recents.refetch()}
          />
        ) : recents.data && recents.data.length > 0 ? (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
            {recents.data.map((f) => (
              <FrameTile key={f.frame_id} frame={f} />
            ))}
          </div>
        ) : (
          <p className="text-t-body text-fg-secondary">
            Recent captures will appear here.
          </p>
        )}
      </Panel>
    </div>
  );
}
