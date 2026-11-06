// Thread (`/work/:id`) — one thread explained: what it is, what is unfinished, what it
// connects to, and an Ask scoped to it (`03 §7h.4`/`§7h.5`/`§7j`/`§7i.2`,
// `UI_REFERENCE §3`/`§4`).
//
// One read owns the page: `get_thread_page` returns the member sessions, the entities, the
// open loops whose evidence names this thread, the connected threads and the backfill
// progress. So every figure below comes from a row that is either rendered here or linked
// from here — the header's session count and time spent are proven by the session rows
// directly under them (both links jump to that list), each session links to its own
// timeline, and an insight's evidence renders as `EvidenceLink`s.
//
// A merged-away thread is explained, never redirected or treated as missing: the row still
// exists as the audit trail of the merge (`03 §7h.5`), so the page names the survivor.
//
// Overrides are writes to `thread_overrides` only. Nothing in this UI inserts or updates a
// `threads`/`thread_sessions` row; the clusterer applies the override afterwards, which is
// why every action re-reads through the mutation's cache invalidation. The merge and split
// confirmations are the ones the Work list offers (`components/intelligence/threadOverrides`),
// so the same sentence about the same destructive step is stated once and on both screens.
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";

import {
  Button,
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
  Textarea,
  TextInput,
} from "../components/primitives";
import {
  ConnectionsPanel,
  EntityChips,
  EvidenceLink,
  InsightCard,
  MergeThreadDialog,
  SplitThreadDialog,
  evidenceHref,
  formatCount,
  formatDuration,
  type ConnectionsState,
} from "../components/intelligence";
import { IconArrowLeft, IconWork } from "../components/icons";
import { useThreadList, useThreadPage } from "../lib/ipc/queries";
import { useRenameThread } from "../lib/ipc/mutations";
import {
  useAskConversation,
  type AskConversationState,
} from "../lib/ipc/useAskConversation";
import { absoluteTime, relativeTime } from "../lib/time";
import { toast } from "../state/toastStore";
import type { ThreadSummary } from "../bindings/ThreadSummary";

/** `03 §7h.4`: the clustering threshold. Below it, the page says the grouping is weak
 *  instead of presenting the session or thread as settled. */
const LOW_CONFIDENCE = 0.7;

/** Mirrors the Ask screen's turn budget (`03 §7i.4`); the command clamps it too. */
const ASK_MAX_TOKENS = 2048;

/** The survivor picker reads the same page of threads the Work list does. */
const THREAD_LIST_LIMIT = 200;

const BACK_LINK =
  "inline-flex min-h-hit-min items-center gap-1 rounded-sm text-t-ui text-fg-secondary " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:text-fg-primary " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

const INLINE_LINK =
  "rounded-sm text-t-ui text-fg-secondary underline decoration-line-subtle underline-offset-2 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:text-fg-primary hover:decoration-line-strong " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

const NOTE_LINK =
  "rounded-sm text-t-sm text-fg-tertiary underline decoration-line-subtle underline-offset-2 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:text-fg-secondary " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

/** The one link a state offers as its next step (a survivor, a way back). */
const PRIMARY_LINK =
  "inline-flex min-h-hit-min items-center gap-1 rounded-sm text-t-ui text-fg-primary " +
  "underline decoration-line-strong underline-offset-2 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:decoration-line-strong " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

const ROW_LINK =
  "block min-w-0 rounded-md py-0.5 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:bg-surface-hover " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

const SESSION_KIND_LABEL: Record<string, string> = {
  focus: "Focus",
  meeting: "Meeting",
  ai: "AI",
  other: "Other",
};

/** `/work/:id` carries an `i64` row id; anything else is a bad address, not a crash. */
function parseThreadId(raw: string | undefined): number | null {
  if (raw == null || !/^\d+$/.test(raw)) return null;
  const id = Number(raw);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

/** Typed failures read as one plain sentence each; a stream failure keeps its own words. */
function askErrorCopy(error: AskConversationState["error"]): string {
  if (error == null) return "The answer could not be produced.";
  if (error.kind === "no_answer_model") {
    return "The answer model is not loaded. Load one in Settings, then retry.";
  }
  if (error.kind === "thread_not_found") return "That conversation no longer exists.";
  return error.message;
}

/** Inline rename of the thread's own title; mounted only while editing. */
function ThreadRenameForm({
  thread,
  onClose,
}: {
  thread: ThreadSummary;
  onClose: () => void;
}) {
  const rename = useRenameThread();
  const [draft, setDraft] = useState(thread.title);
  // The field opens pre-filled with the current title, so the first focus selects it: typing
  // then replaces the name instead of appending to it.
  const selectedOnFocus = useRef(false);
  const trimmed = draft.trim();
  const unchanged = trimmed === thread.title;

  const commit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (trimmed.length === 0 || unchanged) return;
    rename.mutate(
      { thread_id: thread.id, title: trimmed },
      {
        onSuccess: () => {
          toast.success("Thread renamed");
          onClose();
        },
        onError: (error) =>
          toast.error(`Could not rename the thread: ${String(error)}`),
      },
    );
  };

  return (
    <form className="mt-1 flex min-w-0 flex-wrap items-center gap-2" onSubmit={commit}>
      <TextInput
        size="sm"
        autoFocus
        className="min-w-0 flex-1"
        aria-label={`New title for ${thread.title}`}
        value={draft}
        invalid={trimmed.length === 0}
        onChange={(event) => setDraft(event.target.value)}
        onFocus={(event) => {
          if (selectedOnFocus.current) return;
          selectedOnFocus.current = true;
          event.currentTarget.select();
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      />
      <Button
        size="sm"
        type="submit"
        variant="primary"
        loading={rename.isPending}
        disabled={trimmed.length === 0 || unchanged}
      >
        Save
      </Button>
      <Button size="sm" variant="ghost" onClick={onClose}>
        Cancel
      </Button>
    </form>
  );
}

/**
 * The merge survivor list is the one read this page does not already own: its own thread
 * read holds this thread, and the confirmation needs every other thread to pick between.
 * So the page owns that read and mounts it on open — the list is fetched when the
 * confirmation appears, not on every visit — while the dialog itself stays a shared,
 * fetching-nothing confirmation (`components/intelligence/threadOverrides`).
 */
function MergeThreadMount({
  source,
  onClose,
}: {
  source: ThreadSummary;
  onClose: () => void;
}) {
  const threads = useThreadList({
    from_ms: null,
    to_ms: null,
    entity_id: null,
    limit: THREAD_LIST_LIMIT,
  });
  return (
    <MergeThreadDialog
      open
      source={{
        id: source.id,
        title: source.title,
        sessionCount: source.session_count,
      }}
      survivors={threads.data?.threads ?? []}
      survivorsState={
        threads.isLoading ? "loading" : threads.isError ? "error" : "ready"
      }
      onRetrySurvivors={() => void threads.refetch()}
      onClose={onClose}
    />
  );
}

/**
 * Ask scoped to this thread (`03 §7i.2` stage 1). `scope_thread_id` is the whole point of
 * the panel: retrieval starts from this thread's sessions rather than the whole history,
 * and every other scope stays inherited (`03 §7i.4`). The answered turn links out to the
 * conversation it belongs to, where the persisted turns render in full.
 */
function ThreadAsk({
  threadId,
  threadTitle,
}: {
  threadId: number;
  threadTitle: string;
}) {
  const conversation = useAskConversation();
  const [question, setQuestion] = useState("");
  const [asked, setAsked] = useState<string | null>(null);
  const { phase, answer, citations, error, threadId: conversationId } =
    conversation.state;
  const streaming = phase === "streaming";

  const submit = (text: string) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) return;
    setAsked(trimmed);
    void conversation.ask({
      query: trimmed,
      thread_id: null,
      thinking: false,
      max_tokens: ASK_MAX_TOKENS,
      top_k: null,
      window: { mode: "inherit" },
      entities: null,
      scope_thread_id: threadId,
      clear_thread_scope: false,
    });
  };

  const citationCount =
    (citations?.sessions.length ?? 0) + (citations?.frames.length ?? 0);

  return (
    <Panel title="Ask about this thread">
      <div className="flex flex-col gap-3">
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            submit(question);
          }}
        >
          <Textarea
            aria-label={`Your question about ${threadTitle}`}
            placeholder="What did I leave unfinished here?"
            rows={2}
            value={question}
            onChange={(event) => setQuestion(event.target.value)}
            onKeyDown={(event) => {
              // Enter sends; Shift+Enter keeps the newline.
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                submit(question);
              }
            }}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="submit"
              variant="primary"
              loading={streaming}
              disabled={question.trim().length === 0}
            >
              Ask about this thread
            </Button>
            <span className="text-t-xs text-fg-tertiary">
              Retrieval starts from this thread’s sessions.
            </span>
          </div>
        </form>

        {asked != null && (
          <section className="flex flex-col gap-2 border-t border-line-subtle pt-3">
            <p className="m-0 text-t-ui font-medium text-fg-primary">{asked}</p>

            {streaming && answer.length === 0 && (
              <p role="status" className="m-0 text-t-sm text-fg-secondary">
                Assembling context from this thread…
              </p>
            )}

            {answer.length > 0 && (
              <p className="m-0 whitespace-pre-wrap text-t-body text-fg-primary">
                {answer}
              </p>
            )}

            {phase === "done" && (
              <p role="status" className="m-0 text-t-sm text-fg-secondary">
                Answer complete.
              </p>
            )}

            {phase === "error" && (
              <div className="flex flex-col gap-2">
                <p role="status" className="m-0 text-t-sm text-status-danger-fg">
                  {askErrorCopy(error)}
                </p>
                <div className="flex flex-wrap items-center gap-2">
                  <Button size="sm" onClick={() => submit(asked)}>
                    Retry
                  </Button>
                  <Button size="sm" variant="ghost" onClick={conversation.reset}>
                    Dismiss
                  </Button>
                </div>
              </div>
            )}

            {phase === "done" && (
              <div className="flex flex-col gap-2">
                {citationCount === 0 ? (
                  <p className="m-0 text-t-xs text-fg-tertiary">
                    No citation was recorded for this answer.
                  </p>
                ) : (
                  <ul className="m-0 flex list-none flex-wrap items-center gap-2 p-0">
                    {citations?.sessions.map((id) => (
                      <li key={`session-${id}`}>
                        <EvidenceLink kind="session" refId={id} label={`Session ${id}`} />
                      </li>
                    ))}
                    {citations?.frames.map((id) => (
                      <li key={`frame-${id}`}>
                        <EvidenceLink kind="frame" refId={id} label={`Frame ${id}`} />
                      </li>
                    ))}
                  </ul>
                )}
                {conversationId != null && (
                  <Link to={`/ask?thread=${conversationId}`} className={INLINE_LINK}>
                    Open in Ask
                  </Link>
                )}
              </div>
            )}
          </section>
        )}
      </div>
    </Panel>
  );
}

/** Header + panel skeleton that reserves the populated geometry (no layout shift). */
function ThreadSkeleton() {
  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
      <p role="status" className="sr-only">
        Loading this thread
      </p>
      <div className="flex flex-col gap-2">
        <Skeleton h="4" w="third" />
        <Skeleton h="6" w="half" />
        <Skeleton h="3" w="twoThirds" />
      </div>
      <Panel title="Sessions" flush>
        <ul className="m-0 list-none p-0">
          {[0, 1, 2].map((row) => (
            <li key={row} className="border-b border-line-subtle px-3 py-2 last:border-b-0">
              <div className="grid min-w-0 gap-1">
                <Skeleton h="4" w="half" />
                <Skeleton h="3" w="third" />
              </div>
            </li>
          ))}
        </ul>
      </Panel>
    </div>
  );
}

function ThreadView({ threadId }: { threadId: number }) {
  const page = useThreadPage(threadId);
  const navigate = useNavigate();
  const [renaming, setRenaming] = useState(false);
  const [mergeOpen, setMergeOpen] = useState(false);
  const [splitOpen, setSplitOpen] = useState(false);
  const headingRef = useRef<HTMLHeadingElement | null>(null);
  const wasRenaming = useRef(false);

  // Rename ends where it started: Escape and Save both return the focus to the title that
  // is now on screen, rather than dropping it onto the document.
  useEffect(() => {
    if (wasRenaming.current && !renaming) headingRef.current?.focus();
    wasRenaming.current = renaming;
  }, [renaming]);

  // The connections panel's five states are a total function of the page's one read, so it
  // can never claim a connection the read does not hold. `partial` outranks `empty` while
  // clustering runs: "no connections" is a claim that read cannot make yet.
  const connectionsState: ConnectionsState = page.isLoading
    ? "loading"
    : page.isError || !page.data
      ? "error"
      : page.data.progress.running
        ? "partial"
        : page.data.connected.length === 0
          ? "empty"
          : "populated";

  if (page.isLoading) return <ThreadSkeleton />;

  if (page.isError || !page.data) {
    return (
      <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
        <ErrorState
          align="start"
          title="Could not load this thread"
          message="The work graph read failed. Retrying keeps this address."
          onRetry={() => void page.refetch()}
        />
      </div>
    );
  }

  const data = page.data;
  const survivor = data.merged_into;

  // A missing row and a folded-away row are different truths: the second one still has a
  // record and a live survivor, and saying "it no longer exists" would hide the merge.
  if (data.detail == null) {
    if (survivor == null) {
      return (
        <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
          <EmptyState
            align="start"
            icon={<IconWork />}
            title="That thread no longer exists"
            description="It was neither clustered nor merged, so there is nothing to open. The thread list has everything the graph holds now."
            action={
              <Button size="sm" onClick={() => navigate("/work")}>
                Back to Work
              </Button>
            }
          />
        </div>
      );
    }

    return (
      <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
        <Link to="/work" className={BACK_LINK}>
          <IconArrowLeft size={16} />
          Work
        </Link>
        <Panel title="This thread was merged">
          <div className="flex flex-col gap-3">
            <p className="m-0 text-t-ui text-fg-secondary">
              Its sessions now belong to {survivor.title}. The merge is recorded as an
              override, so it keeps winning over the clusterer.
            </p>
            <p className="m-0 flex flex-wrap items-center gap-2 text-t-sm text-fg-secondary">
              <span>{formatCount(survivor.session_count, "session")}</span>
              <span aria-hidden="true">·</span>
              <span>{formatDuration(survivor.active_ms)} spent</span>
              <span aria-hidden="true">·</span>
              <span title={absoluteTime(survivor.last_seen_at)}>
                last touched {relativeTime(survivor.last_seen_at)}
              </span>
            </p>
            <Link to={`/work/${survivor.id}`} className={PRIMARY_LINK}>
              Open {survivor.title}
            </Link>
          </div>
        </Panel>
      </div>
    );
  }

  const detail = data.detail;
  const thread = detail.thread;
  const entityNames: Record<number, string> = {};
  for (const entity of detail.entities) {
    entityNames[entity.entity_id] = entity.canonical_name;
  }

  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
      <header className="min-w-0">
        <Link to="/work" className={BACK_LINK}>
          <IconArrowLeft size={16} />
          Work
        </Link>

        {renaming ? (
          <ThreadRenameForm thread={thread} onClose={() => setRenaming(false)} />
        ) : (
          <h1
            ref={headingRef}
            tabIndex={-1}
            className="mt-1 text-t-h1 font-semibold text-fg-primary"
          >
            {thread.title}
          </h1>
        )}

        <p className="mt-1 text-t-ui text-fg-secondary">
          <a href="#thread-sessions" className={INLINE_LINK}>
            <span title={absoluteTime(thread.last_seen_at)}>
              Last touched {relativeTime(thread.last_seen_at)}
            </span>
            {" · "}
            {formatCount(thread.session_count, "session")}
            {" · "}
            {formatDuration(thread.active_ms)} spent
          </a>
        </p>

        {thread.confidence < LOW_CONFIDENCE && (
          <p className="mt-1">
            <a href="#thread-entities" className={NOTE_LINK}>
              Low confidence — few shared entities
            </a>
          </p>
        )}

        <div className="mt-2 flex flex-wrap items-center gap-1">
          <Button size="sm" variant="ghost" onClick={() => setRenaming(true)}>
            Rename
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setMergeOpen(true)}>
            Merge…
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setSplitOpen(true)}>
            Split…
          </Button>
        </div>
      </header>

      {data.progress.running && (
        <p
          role="status"
          className="m-0 border-l-2 border-line-strong bg-surface-inset px-3 py-2 text-t-sm text-fg-secondary"
        >
          Still clustering — some sessions may not appear yet.
        </p>
      )}

      <Panel title="Sessions" id="thread-sessions" className="scroll-mt-4" flush>
        {detail.sessions.length === 0 ? (
          <p className="m-0 px-pane py-3 text-t-sm text-fg-secondary">
            No sessions belong to this thread yet.
          </p>
        ) : (
          <ul className="m-0 list-none p-0">
            {detail.sessions.map((session) => (
              <li
                key={session.session_id}
                className="border-b border-line-subtle px-3 py-2 last:border-b-0"
              >
                <Link
                  to={evidenceHref("session", session.session_id)}
                  className={ROW_LINK}
                >
                  <span className="block truncate text-t-body font-medium text-fg-primary">
                    {session.title?.trim() || "Untitled session"}
                  </span>
                  <span className="mt-0.5 block text-t-xs text-fg-secondary">
                    {SESSION_KIND_LABEL[session.kind] ?? session.kind}
                    {" · "}
                    <span title={absoluteTime(session.started_at)}>
                      {relativeTime(session.started_at)}
                    </span>
                    {" · "}
                    {session.ended_at == null
                      ? "still running"
                      : formatDuration(session.ended_at - session.started_at)}
                  </span>
                  {session.confidence < LOW_CONFIDENCE && (
                    <span className="mt-0.5 block text-t-xs text-fg-tertiary">
                      Low confidence — few shared entities
                    </span>
                  )}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title="Entities in this thread" id="thread-entities" className="scroll-mt-4">
        <EntityChips
          entities={detail.entities.map((entity) => ({
            id: entity.entity_id,
            kind: entity.kind,
            canonical_name: entity.canonical_name,
          }))}
          emptyLabel="No entity was recorded for this thread."
        />
      </Panel>

      <Panel title="Open loops">
        {data.open_loops.length === 0 ? (
          <p className="m-0 text-t-sm text-fg-secondary">
            {data.progress.running
              ? "None recorded yet — the graph is still clustering, so an open loop may not have been written."
              : "No open loops in this thread."}
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {data.open_loops.map((insight) => (
              <li key={insight.id}>
                <InsightCard insight={insight} entityNames={entityNames} />
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <ConnectionsPanel
        connections={data.connected}
        entityNames={entityNames}
        state={connectionsState}
        onRetry={() => void page.refetch()}
      />

      <ThreadAsk threadId={thread.id} threadTitle={thread.title} />

      {mergeOpen && (
        <MergeThreadMount source={thread} onClose={() => setMergeOpen(false)} />
      )}
      {splitOpen && (
        <SplitThreadDialog
          open
          thread={thread}
          sessions={detail.sessions}
          onClose={() => setSplitOpen(false)}
        />
      )}
    </div>
  );
}

export function Component() {
  const { id } = useParams();
  const navigate = useNavigate();
  const threadId = parseThreadId(id);

  if (threadId == null) {
    return (
      <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
        <ErrorState
          align="start"
          title="That thread address is not valid"
          message="A thread address is a number, like /work/12."
          action={
            <Button size="sm" onClick={() => navigate("/work")}>
              Back to Work
            </Button>
          }
        />
      </div>
    );
  }

  // Keyed on the id: another thread's address is another page, so the rename draft, the
  // open dialogs and the scoped conversation must not survive the navigation.
  return <ThreadView key={threadId} threadId={threadId} />;
}
