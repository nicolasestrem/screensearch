// Moment (/timeline/:id) — one captured frame in full (UI_REFERENCE §3/§4). Deep-
// linkable: the image, recognized text, vision tags, context, and the on-demand
// "Tag with vision" action. States: loading skeleton; error (load failed) → retry;
// "gone" (unknown / deleted id) → explain + back; partial (vision not yet tagged) →
// queue action (inside MomentDetail); populated. A context strip + prev/next walk
// the neighbouring captures. When the frame belongs to a session, that session's
// work-graph context (entities + its thread's connections) renders in P9's shared
// `EntityChips` / `ConnectionsPanel`; without a session neither panel renders.
import { useLocation, useNavigate, useParams } from "react-router-dom";

import { Button, ErrorState, Panel, Skeleton } from "../components/primitives";
import { FrameTile, MomentDetail } from "../components/domain";
import {
  ConnectionsPanel,
  EntityChips,
  type ConnectionsState,
} from "../components/intelligence";
import {
  IconArrowLeft,
  IconChevronLeft,
  IconChevronRight,
} from "../components/icons";
import { unresolvedSharedEntityIds } from "../lib/connectionNames";
import {
  useEntities,
  useFrame,
  useFrameContext,
  useFrameSpans,
  useSessionGraph,
} from "../lib/ipc/queries";
import { useEnqueueVision } from "../lib/ipc/mutations";
import { toast } from "../state/toastStore";

const NEIGHBOUR_HALF_MS = 30 * 60 * 1000; // ±30 min context window
const NEIGHBOUR_EACH = 12; // closest captures to load on each side of the anchor

function sessionReturnPath(state: unknown): string | null {
  if (!state || typeof state !== "object" || !("fromSession" in state)) {
    return null;
  }
  const value = (state as { fromSession?: unknown }).fromSession;
  return typeof value === "string" && /^\/timeline\/session\/\d+$/.test(value)
    ? value
    : null;
}

export function Component() {
  const { id } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const frameId = id != null && /^\d+$/.test(id) ? Number(id) : null;
  const returnPath = sessionReturnPath(location.state);

  const detail = useFrame(frameId);
  const enqueue = useEnqueueVision();

  // Captures bracketing this frame (enabled once we know its capture time). Anchored
  // to `at` — the closest on each side — so prev/next reach the true neighbours even
  // in a busy session (a plain newest-first window would only return the latest
  // frames and drop the anchor, breaking navigation).
  const at = detail.data?.captured_at ?? 0;
  const neighbours = useFrameContext(
    at,
    NEIGHBOUR_HALF_MS,
    NEIGHBOUR_EACH,
    detail.data != null,
  );

  // Spans back the text+layout reconstruction shown when the screenshot has expired.
  // Only fetched for degraded frames (the common image case never pays for it).
  const spans = useFrameSpans(frameId, detail.data?.image_purged ?? false);

  // This frame's session work-graph context (entities, the owning thread and its
  // connections). Idle while the frame read is in flight and when the frame has no
  // session at all: a sessionless moment renders neither panel, never a placeholder.
  const graph = useSessionGraph(detail.data?.session?.id ?? null);
  // Same as Session: a connection's shared entity can belong to a sibling session in
  // the thread, which this frame's session never touched — resolve those ids by name.
  const graphEntityNames: Record<number, string> = {};
  for (const entity of graph.data?.entities ?? []) {
    graphEntityNames[entity.id] = entity.canonical_name;
  }
  const relatedEntities = useEntities(
    unresolvedSharedEntityIds(graph.data?.connected ?? [], graphEntityNames),
  );

  const queueVision = () => {
    if (frameId == null) return;
    enqueue.mutate(
      { kind: "frame", frame_id: frameId },
      {
        onSuccess: (n) =>
          toast.success(
            n > 0 ? "Vision queued for this frame" : "Already tagged or queued",
          ),
        onError: (e) => toast.error(String(e)),
      },
    );
  };

  const backToTimeline = (
    <Button
      variant="ghost"
      size="sm"
      leadingIcon={<IconArrowLeft size={16} />}
      onClick={() => navigate(returnPath ?? "/timeline")}
    >
      {returnPath ? "Session" : "Timeline"}
    </Button>
  );

  // Invalid route param → treat as a missing moment, not a crash.
  if (frameId == null) {
    return (
      <div className="p-6">
        <ErrorState
          title="Unknown moment"
          message="That timeline address isn't valid."
          onRetry={() => navigate("/timeline")}
          retryLabel="Back to timeline"
        />
      </div>
    );
  }

  if (detail.isLoading) {
    return (
      <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
        <Skeleton className="h-8 w-40" />
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1.6fr_1fr]">
          <Skeleton className="aspect-video w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      </div>
    );
  }

  if (detail.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="Couldn't load this moment"
          message={String(detail.error)}
          onRetry={() => detail.refetch()}
        />
      </div>
    );
  }

  // Unknown / deleted frame — `get_frame` returned null.
  if (!detail.data) {
    return (
      <div className="p-6">
        <ErrorState
          title="This moment is gone"
          message="The frame may have been deleted or never existed."
          onRetry={() => navigate("/timeline")}
          retryLabel="Back to timeline"
        />
      </div>
    );
  }

  // Prev / next within the loaded context window (ascending by capture time, anchor
  // excluded). `at` is this frame's capture time, so prev = the closest capture before
  // it and next = the closest after — derived by time, not by locating the anchor in
  // the list (it isn't there).
  const sorted = [...(neighbours.data ?? [])].sort(
    (a, b) => a.captured_at - b.captured_at,
  );
  const before = sorted.filter((f) => f.captured_at < at);
  const prev = before.length > 0 ? before[before.length - 1] : null;
  const next = sorted.find((f) => f.captured_at > at) ?? null;
  const context = sorted.filter((f) => f.frame_id !== frameId);

  const graphEntities = graph.data?.entities ?? [];
  const entityNames: Record<number, string> = { ...graphEntityNames };
  for (const entity of relatedEntities.data ?? []) {
    entityNames[entity.id] = entity.canonical_name;
  }
  // Same total function of the one read as Session and the Thread page: `partial`
  // outranks `empty` while clustering runs.
  const connectionsState: ConnectionsState = graph.isLoading
    ? "loading"
    : graph.isError || !graph.data
      ? "error"
      : graph.data.progress.running
        ? "partial"
        : graph.data.thread === null || graph.data.connected.length === 0
          ? "empty"
          : "populated";

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
      <div className="flex items-center justify-between gap-3">
        {backToTimeline}
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            leadingIcon={<IconChevronLeft size={16} />}
            disabled={!prev}
            onClick={() =>
              prev &&
              navigate(`/timeline/${prev.frame_id}`, {
                state: returnPath ? { fromSession: returnPath } : undefined,
              })
            }
          >
            Prev
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!next}
            onClick={() =>
              next &&
              navigate(`/timeline/${next.frame_id}`, {
                state: returnPath ? { fromSession: returnPath } : undefined,
              })
            }
          >
            Next
            <IconChevronRight size={16} />
          </Button>
        </div>
      </div>

      <MomentDetail
        detail={detail.data}
        onQueueVision={queueVision}
        queueing={enqueue.isPending}
        spans={spans.data}
      />

      {/* The frame's session context. A frame with no session omits both panels
          outright — the same "absent, not placeholdered" rule as the drill-in line
          inside MomentDetail (`UI_REFERENCE §4`). */}
      {detail.data.session ? (
        <>
          <Panel title="Entities in this session">
            {graph.isLoading ? (
              // Reserves the chip row's height so the panel does not jump on arrival.
              <Skeleton h="6" w="twoThirds" />
            ) : graph.isError || !graph.data ? (
              <p className="text-t-body text-fg-secondary">
                Couldn't load this session's entities.
              </p>
            ) : (
              <EntityChips
                entities={graphEntities.map((entity) => ({
                  id: entity.id,
                  kind: entity.kind,
                  canonical_name: entity.canonical_name,
                }))}
                emptyLabel={
                  graph.data.progress.running
                    ? "None recorded yet — clustering is still running."
                    : "No entity was recorded for this frame's session."
                }
              />
            )}
          </Panel>

          <ConnectionsPanel
            connections={graph.data?.connected ?? []}
            entityNames={entityNames}
            state={connectionsState}
            onRetry={() => void graph.refetch()}
          />
        </>
      ) : null}

      {context.length > 0 && (
        <Panel title="Around this moment">
          {/* Wrap onto rows instead of a nested horizontal scroller (D9 — one scroll
              context; Windows draws a permanent scrollbar for overflow-x-auto). The page
              scrolls as one column. */}
          <div className="flex flex-wrap gap-3">
            {context.map((f) => (
              <FrameTile key={f.frame_id} frame={f} className="w-40 shrink-0" />
            ))}
          </div>
        </Panel>
      )}
    </div>
  );
}
