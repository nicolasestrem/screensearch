// Session drill-in (/timeline/session/:id) — additive recall over persisted
// sessions. Base detail and lazy title/summary start together: metadata paints as
// soon as the inference-free read returns while intelligence fills in inline. The
// work-graph context (the entities this session touched, the thread it was
// clustered into and that thread's connections) is one `get_session_graph` read;
// P9's `EntityChips` / `ConnectionsPanel` are placed from `components/intelligence`,
// never reimplemented here.
import { useNavigate, useParams } from "react-router-dom";

import {
  Button,
  Chip,
  ErrorState,
  Panel,
  Skeleton,
} from "../components/primitives";
import { FrameTile, ReportView } from "../components/domain";
import {
  ConnectionsPanel,
  EntityChips,
  type ConnectionsState,
} from "../components/intelligence";
import { IconArrowLeft, IconSparkle } from "../components/icons";
import { useSession, useSessionGraph } from "../lib/ipc/queries";
import { useSessionRecap } from "../lib/ipc/useSessionRecap";
import { absoluteTime } from "../lib/time";
import type { Session } from "../bindings/Session";
import type { SessionArtifactRole } from "../bindings/SessionArtifactRole";

const KIND_LABEL: Record<Session["kind"], string> = {
  focus: "Focus",
  meeting: "Meeting",
  ai: "AI",
  other: "Other",
};

const ROLE_LABEL: Record<SessionArtifactRole, string> = {
  user: "You",
  agent: "Agent",
};

// One threshold for the whole app so "low confidence" cannot mean two things: the
// Thread page's sessions list says so at the same value. A low-confidence session
// keeps its numeric percentage and adds one plain line (`03 §7e`, D11) — never a
// score, never a warning colour.
const LOW_CONFIDENCE = 0.7;

function SessionSkeleton() {
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-4 p-6">
      <Skeleton className="h-8 w-40" />
      <Skeleton className="h-40 w-full" />
      <Skeleton className="h-32 w-full" />
      <Skeleton className="h-64 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  );
}

function sessionTitle(session: Session): string {
  return session.title?.trim() || "Untitled session";
}

export function Component() {
  const { id } = useParams();
  const navigate = useNavigate();
  const sessionId = id != null && /^\d+$/.test(id) ? Number(id) : null;
  const base = useSession(sessionId, false);
  const summary = useSession(sessionId, true);
  const recap = useSessionRecap(sessionId);
  // The session's work-graph context. Idle until the id is valid, so the invalid
  // and gone branches below ask for nothing.
  const graph = useSessionGraph(sessionId);

  if (sessionId == null) {
    return (
      <div className="p-6">
        <ErrorState
          title="Unknown session"
          message="That session address isn't valid."
          onRetry={() => navigate("/timeline")}
          retryLabel="Back to timeline"
        />
      </div>
    );
  }

  if (base.isLoading) return <SessionSkeleton />;

  if (base.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="Couldn't load this session"
          message={String(base.error)}
          onRetry={() => base.refetch()}
        />
      </div>
    );
  }

  if (!base.data) {
    return (
      <div className="p-6">
        <ErrorState
          title="This session is gone"
          message="The session may have been regenerated, deleted, or never existed."
          onRetry={() => navigate("/timeline")}
          retryLabel="Back to timeline"
        />
      </div>
    );
  }

  const detail = summary.data ?? base.data;
  const session = detail.session;
  const summaryText = session.summary?.trim() || null;
  const needsSummary = !base.data.session.summary?.trim();
  const isOpen = session.ended_at == null;
  const momentState = { fromSession: `/timeline/session/${sessionId}` };
  const recapRange = {
    start: session.started_at,
    end: Math.max(session.started_at + 1, session.ended_at ?? Date.now()),
  };

  const graphEntities = graph.data?.entities ?? [];
  const entityNames: Record<number, string> = {};
  for (const entity of graphEntities) {
    entityNames[entity.id] = entity.canonical_name;
  }
  // The panel's five states are a total function of that one read, the same
  // expression the Thread page uses: `partial` outranks `empty` while clustering
  // runs, because "no connections" is a claim the read cannot make yet.
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
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Button
          variant="ghost"
          size="sm"
          leadingIcon={<IconArrowLeft size={16} />}
          onClick={() => navigate("/timeline")}
        >
          Timeline
        </Button>
        <Chip tone="neutral">Session {session.id}</Chip>
      </div>

      <Panel
        title="Session"
        action={<Chip tone="neutral">{KIND_LABEL[session.kind]}</Chip>}
      >
        <div className="flex flex-col gap-3">
          <h1 className="break-words text-t-h1 font-semibold text-fg-primary">
            {sessionTitle(session)}
          </h1>
          <div className="flex flex-wrap gap-2">
            {session.tool ? <Chip tone="ok">{session.tool}</Chip> : null}
            {session.host ? <Chip tone="neutral">{session.host}</Chip> : null}
            <Chip tone="neutral">
              Confidence {(session.confidence * 100).toFixed(1)}%
            </Chip>
          </div>
          <p className="font-mono text-t-data text-fg-secondary">
            {absoluteTime(session.started_at)} —{" "}
            {session.ended_at
              ? absoluteTime(session.ended_at)
              : "still running"}
          </p>
          {isOpen ? (
            <p className="text-t-body text-fg-secondary">
              Still running — boundaries are not final.
            </p>
          ) : null}
          {session.confidence < LOW_CONFIDENCE ? (
            <p className="text-t-xs text-fg-tertiary">
              Low confidence — this session's boundaries are approximate.
            </p>
          ) : null}
        </div>
      </Panel>

      <Panel title="Summary">
        {summaryText ? (
          <p className="whitespace-pre-wrap break-words text-t-body text-fg-primary">
            {summaryText}
          </p>
        ) : needsSummary && summary.isError ? (
          <div className="flex flex-col items-start gap-3">
            <p className="text-t-body text-fg-secondary">
              Couldn't generate this session summary: {String(summary.error)}
            </p>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => summary.refetch()}
            >
              Retry summary
            </Button>
          </div>
        ) : (
          <div className="flex flex-col gap-2" aria-label="Generating summary">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
          </div>
        )}
      </Panel>

      <Panel
        title="Representative frames"
        action={<Chip tone="neutral">{detail.frame_count} total</Chip>}
      >
        {detail.frames.length > 0 ? (
          <div className="flex flex-wrap gap-3">
            {detail.frames.map((frame) => (
              <FrameTile
                key={frame.frame_id}
                frame={frame}
                className="w-40 shrink-0"
                navigationState={momentState}
              />
            ))}
          </div>
        ) : (
          <p className="text-t-body text-fg-secondary">
            No representative frames are available for this session.
          </p>
        )}
      </Panel>

      <Panel title="Exchanges">
        {detail.exchanges.length > 0 ? (
          <div className="flex flex-col gap-3">
            {detail.exchanges.map((exchange) => (
              <article
                key={exchange.id}
                className="flex flex-col gap-2 rounded-md border border-line-subtle bg-surface-inset p-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-t-xs font-medium text-fg-secondary">
                    {exchange.role ? ROLE_LABEL[exchange.role] : "Exchange"}
                  </span>
                  {exchange.frame_id ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        navigate(`/timeline/${exchange.frame_id}`, {
                          state: momentState,
                        })
                      }
                    >
                      Open moment
                    </Button>
                  ) : null}
                </div>
                <p className="whitespace-pre-wrap break-words text-t-body text-fg-primary">
                  {exchange.content}
                </p>
              </article>
            ))}
          </div>
        ) : (
          <p className="text-t-body text-fg-secondary">
            No exchanges captured for this session.
          </p>
        )}
      </Panel>

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
                : "No entity was recorded for this session."
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

      <Panel title="Recap">
        {recap.isGenerating ? (
          <div className="flex flex-col gap-3" aria-live="polite">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-2/3" />
            <p className="font-mono text-t-data text-fg-secondary">
              {recap.progress
                ? `${recap.progress.stage} · ${recap.progress.done}/${recap.progress.total}`
                : "Preparing this session…"}
            </p>
            <div>
              <Button variant="secondary" size="sm" onClick={recap.cancel}>
                Cancel
              </Button>
            </div>
          </div>
        ) : recap.isError ? (
          <ErrorState
            title="Couldn't generate the recap"
            message={String(recap.error)}
            onRetry={recap.generate}
          />
        ) : recap.isSuccess && recap.result ? (
          <div className="flex flex-col gap-4">
            <ReportView
              report={recap.result}
              request={null}
              footerScope={{
                timeRange: recapRange,
                activeFilter: `session id: ${session.id}`,
              }}
              onOpenFrame={(frameId) =>
                navigate(`/timeline/${frameId}`, { state: momentState })
              }
            />
            <div>
              <Button variant="secondary" size="sm" onClick={recap.generate}>
                Generate again
              </Button>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-start gap-3">
            <p className="text-t-body text-fg-secondary">
              Generate a coverage-first report over this session's captured frames.
            </p>
            <Button
              variant="primary"
              leadingIcon={<IconSparkle size={16} />}
              onClick={recap.generate}
            >
              Generate recap
            </Button>
          </div>
        )}
      </Panel>
    </div>
  );
}
