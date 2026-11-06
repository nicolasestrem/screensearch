// Ask v2 — the persisted conversation surface (UI_REFERENCE §4 "Ask v2 conversation";
// 03 §7i.3 citations, §7i.4 multi-turn). Three parts, in one column pair:
//
//   left   the saved conversations (`ask_threads`), newest first, deep-linked through
//          `?thread=<id>`; selecting one is a link, deleting one is a row action.
//   main   the selected conversation's turns in `ordinal` order, its live streaming turn,
//          and the composer for the next turn.
//   scope  every turn states exactly what it was applied to; the composer states what the
//          next turn will use and lets a person change it (window · entity · thread).
//
// Two rules are load-bearing and are the reason this file is explicit about them:
//
//  1. CITATIONS (§7i.3, kickoff "do not render it as an answer"). Every answer cites both
//     sessions and frames. An answer whose `citations` is NULL or empty is a DEFECT, so it
//     is never rendered as a bare answer: the prose is followed by an explicit
//     "Citations were not recorded for this turn" note. See `TurnBlock`.
//  2. NO LOST TURN (§7i.4). A turn whose `answer` is NULL and which is not the turn that is
//     streaming right now keeps its question in the thread with a Retry that re-submits the
//     same question into the same conversation.
//
// House rules applied here: TanStack Query owns every server read, `?thread=` (and the
// optional `?scopeThread=`) are the only view state in the URL, skeletons reserve the final
// geometry, the shell is the one scroll context (nothing here scrolls itself), and only v2
// semantic tokens are used. `components/domain` is read-only in this phase (P10 owns it), so
// the answer prose is rendered pre-wrapped instead of through `AnswerStream`: that component
// is bound to the legacy single-turn frame-only citations (`useAsk`'s view model), which
// cannot express a session citation without editing it.
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { Link, useSearchParams } from "react-router-dom";

import {
  Button,
  Dialog,
  EmptyState,
  ErrorState,
  Expander,
  ListRow,
  Panel,
  Select,
  Skeleton,
  Textarea,
  TextInput,
} from "../components/primitives";
import {
  EntityChips,
  EvidenceLink,
  ScopeChips,
  formatCount,
  type EntityChipItem,
} from "../components/intelligence";
// Direct module import (not the `components/domain` barrel): the premade cards are the one
// legacy piece this route reuses, and the barrel would pull the whole v1 domain set into the
// /ask chunk. P10 converts the cards; nothing here edits them.
import { PromptCardGrid } from "../components/domain/PromptCardGrid";
import { useAskConversation } from "../lib/ipc/useAskConversation";
import type { AskStreamError } from "../lib/ipc/useAskConversation";
import { useDeleteAskThread } from "../lib/ipc/mutations";
import {
  useAskThreads,
  useAskTurns,
  useEntities,
  useEntityList,
  useThreadList,
} from "../lib/ipc/queries";
import { absoluteTime, localDateMidnight, relativeTime } from "../lib/time";
import type { AskCitations } from "../bindings/AskCitations";
import type { AskConversationRequest } from "../bindings/AskConversationRequest";
import type { AskThreadSummary } from "../bindings/AskThreadSummary";
import type { AskTurn } from "../bindings/AskTurn";
import type { AskTurnStartError } from "../bindings/AskTurnStartError";
import type { AskWindowScope } from "../bindings/AskWindowScope";
import type { EntityListQuery } from "../bindings/EntityListQuery";
import type { EntitySummary } from "../bindings/EntitySummary";
import type { ThreadListQuery } from "../bindings/ThreadListQuery";
import type { TimeRange } from "../bindings/TimeRange";

/** Generation budget for one answer, the same 2048 the legacy Ask uses: the default answer
 *  models reason before answering, and a smaller budget truncated the answer to nothing. */
const ASK_MAX_TOKENS = 2048;

/** Work-graph threads, for the thread chip's title. The page needs the untruncated title of
 *  any thread a turn was scoped to, so it reads one page and builds a map. */
const THREAD_TITLES_QUERY: ThreadListQuery = {
  from_ms: null,
  to_ms: null,
  entity_id: null,
  limit: 1000,
};

/** The graph's entities, for the entity-scope picker (`03 §7i.1`: Ask works on an empty
 *  graph, it just has no entity scope to offer). */
const ENTITY_CATALOG_QUERY: EntityListQuery = { kind: null, query: null, limit: 200 };

/** A historical turn is not editable, so its ScopeChips never renders a clear control. */
const NOOP = () => undefined;

const TEXT_LINK =
  "rounded-sm text-fg-primary underline decoration-line-subtle underline-offset-2 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:decoration-line-strong " +
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

const ROW_LINK =
  "flex w-full min-h-hit-min items-center truncate text-t-body font-medium text-fg-primary " +
  "motion-safe:transition-colors duration-fast ease-standard hover:text-accent-strong " +
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

/** The composer's window mode: the three `AskWindowScope` variants, before they are built. */
type WindowMode = "inherit" | "all" | "range";

/** The entity picker's read, honestly reduced to the states the composer renders. */
type CatalogState = "loading" | "empty" | "error" | "populated";

function parseId(value: string | null): number | null {
  if (value === null) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

/**
 * Whether two turns ran over the same scope. The record stores the scope that was applied, so
 * equality with the previous turn is the only evidence §7i.4 inheritance leaves behind: the
 * window and the resolved entity set (order is not part of an entity scope).
 */
function sameScope(a: AskTurn, b: AskTurn): boolean {
  const windowsMatch =
    a.window === null
      ? b.window === null
      : b.window !== null && a.window.start === b.window.start && a.window.end === b.window.end;
  if (!windowsMatch || a.resolved_entities.length !== b.resolved_entities.length) return false;
  const other = new Set(b.resolved_entities);
  return a.resolved_entities.every((id) => other.has(id));
}

/** `ask_threads.title` is NULL until the first turn names the thread (`ask_turn` trims the
 *  question); a row that predates that keeps an honest generic name rather than inventing one. */
function conversationLabel(thread: AskThreadSummary): string {
  const title = thread.title?.trim();
  return title ? title : "Untitled conversation";
}

/** Entity ids to chips. An id the graph read no longer returns (a folded or deleted entity) is
 *  still shown, by id, so the applied scope is never silently shortened. */
function toChipItems(
  ids: number[],
  byId: Map<number, EntitySummary>,
): EntityChipItem[] {
  return ids.map((id) => {
    const entity = byId.get(id);
    return entity
      ? { id, kind: entity.kind, canonical_name: entity.canonical_name }
      : { id, kind: "unknown", canonical_name: `Entity ${id}` };
  });
}

// ---------------------------------------------------------------------------------------
// Citations (§7i.3): sessions first, then the frames, both as the shared evidence links so
// the destination mapping stays in one place (`evidenceHref`).
// ---------------------------------------------------------------------------------------

function Citations({ citations }: { citations: AskCitations }) {
  return (
    <nav aria-label="Citations" className="min-w-0">
      <ul className="m-0 flex list-none flex-wrap items-center gap-2 p-0">
        {citations.sessions.map((id) => (
          <li key={`session-${id}`} className="min-w-0">
            <EvidenceLink kind="session" refId={id} />
          </li>
        ))}
        {citations.frames.map((id) => (
          <li key={`frame-${id}`} className="min-w-0">
            <EvidenceLink kind="frame" refId={id} />
          </li>
        ))}
      </ul>
    </nav>
  );
}

// ---------------------------------------------------------------------------------------
// One persisted turn: question, answer, citations, and the scope it was actually applied to.
// ---------------------------------------------------------------------------------------

interface TurnBlockProps {
  turn: AskTurn;
  /** The turn before it in `ordinal` order, for the inherited-scope note. */
  previous: AskTurn | undefined;
  entityById: Map<number, EntitySummary>;
  threadTitles: Map<number, string>;
  onRetry: (turn: AskTurn) => void;
}

function TurnBlock({
  turn,
  previous,
  entityById,
  threadTitles,
  onRetry,
}: TurnBlockProps) {
  const answer = turn.answer ?? "";
  const hasAnswer = answer.trim() !== "";
  const citations = turn.citations;
  const citationCount =
    citations === null ? 0 : citations.sessions.length + citations.frames.length;

  // The record carries the scope that was applied, not whether the question set it or the
  // previous turn handed it over: a turn whose scope equals the previous turn's is the one
  // that reused them (§7i.4). Nothing else about the history is claimed.
  const inherited = previous !== undefined && sameScope(previous, turn);

  return (
    <li className="grid min-w-0 gap-3 border-b border-line-subtle py-pane first:pt-0 last:border-b-0 last:pb-0">
      <header className="grid min-w-0 gap-1">
        <h3 className="m-0 max-w-prose text-t-body font-medium text-fg-primary">
          {turn.question}
        </h3>
        <p className="m-0 text-t-xs text-fg-tertiary">
          <time
            dateTime={new Date(turn.created_at).toISOString()}
            title={absoluteTime(turn.created_at)}
          >
            {relativeTime(turn.created_at)}
          </time>
        </p>
      </header>

      {hasAnswer ? (
        <>
          <p className="m-0 max-w-prose whitespace-pre-wrap text-t-body text-fg-primary">
            {answer}
          </p>
          {citations !== null && citationCount > 0 ? (
            <Citations citations={citations} />
          ) : (
            // §7i.3 / kickoff: an answer with no recorded citations is a defect, never a bare
            // answer. The note is part of the turn, not a hint that can be dismissed.
            <p role="note" className="m-0 text-t-xs text-fg-secondary">
              Citations were not recorded for this turn.
            </p>
          )}
        </>
      ) : (
        <div className="grid min-w-0 gap-2">
          <p className="m-0 text-t-sm text-fg-secondary">
            No answer was recorded for this turn.
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="secondary" onClick={() => onRetry(turn)}>
              Retry
            </Button>
            <span className="text-t-xs text-fg-tertiary">
              Asks the same question again in this conversation.
            </span>
          </div>
        </div>
      )}

      <ScopeChips
        window={turn.window}
        entities={toChipItems(turn.resolved_entities, entityById)}
        threadTitle={
          turn.scope_thread_id === null
            ? null
            : (threadTitles.get(turn.scope_thread_id) ?? `Thread ${turn.scope_thread_id}`)
        }
        inherited={inherited}
        // A past turn is read-only history; `onClear` never renders a control at this setting.
        editable={false}
        onClear={NOOP}
      />
    </li>
  );
}

// ---------------------------------------------------------------------------------------
// The live turn. It occupies the same slot the persisted turn will land in, so the streamed
// answer is replaced in place and nothing above or below moves when the rows are refetched.
// ---------------------------------------------------------------------------------------

function StreamingTurn({
  question,
  answer,
  citations,
  reading,
}: {
  question: string | null;
  answer: string;
  citations: AskCitations | null;
  /** `false` once the stream finished: the text stays until the persisted turn replaces it. */
  reading: boolean;
}) {
  const hasAnswer = answer.trim() !== "";
  return (
    <div className="grid min-w-0 gap-3 py-pane" aria-busy={reading}>
      <div className="grid min-w-0 gap-1">
        {question !== null && (
          <p className="m-0 max-w-prose text-t-body font-medium text-fg-primary">{question}</p>
        )}
        {reading && (
          <p role="status" className="m-0 text-t-sm text-fg-secondary">
            Reading…
          </p>
        )}
      </div>
      {/* Reserved answer slot: three prose lines of room while no token has arrived. */}
      <div className="grid min-h-24 min-w-0 content-start gap-2">
        {hasAnswer ? (
          <p className="m-0 max-w-prose whitespace-pre-wrap text-t-body text-fg-primary">
            {answer}
          </p>
        ) : (
          <div aria-hidden="true" className="grid gap-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-11/12" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        )}
      </div>
      {citations !== null &&
        citations.sessions.length + citations.frames.length > 0 && (
          <Citations citations={citations} />
        )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// The invitation (§4 empty state): a heading, one line, and the premade cards. It is used
// both when no conversation exists and inside a saved conversation that has no turns yet.
// ---------------------------------------------------------------------------------------

function Invitation({
  onPick,
  disabled,
}: {
  onPick: (prompt: string) => void;
  disabled: boolean;
}) {
  return (
    <div className="grid min-w-0 gap-4">
      <EmptyState
        align="start"
        title="Ask anything about your own history"
        description="Every answer cites the sessions and moments it came from."
      />
      <fieldset className="min-w-0 border-0 p-0" disabled={disabled}>
        <PromptCardGrid onPick={onPick} />
      </fieldset>
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// The composer: the next turn's scope (editable) and its question.
// ---------------------------------------------------------------------------------------

interface ComposerProps {
  query: string;
  onQueryChange: (value: string) => void;
  canSubmit: boolean;
  onSubmit: () => void;
  streaming: boolean;
  onStop: () => void;
  error: AskTurnStartError | AskStreamError | null;
  onRetryStart: () => void;
  onStartNew: () => void;
  /** The window the next turn will run over, resolved for display. */
  window: TimeRange | null;
  windowMode: WindowMode;
  onWindowModeChange: (mode: WindowMode) => void;
  rangeStart: string;
  rangeEnd: string;
  onRangeStartChange: (value: string) => void;
  onRangeEndChange: (value: string) => void;
  startInvalid: boolean;
  endInvalid: boolean;
  rangeMessage: string | null;
  /** Whether the conversation already has a turn the next one can inherit a window from. */
  hasPreviousTurn: boolean;
  entities: EntityChipItem[];
  onRemoveEntity: (id: number) => void;
  onClearEntities: () => void;
  onAddEntity: (id: number) => void;
  entityOverrideActive: boolean;
  catalogState: CatalogState;
  catalogItems: EntityChipItem[];
  onRetryCatalog: () => void;
  threadTitle: string | null;
  onClearThreadScope: () => void;
  textareaWrapRef: RefObject<HTMLDivElement>;
}

function Composer({
  query,
  onQueryChange,
  canSubmit,
  onSubmit,
  streaming,
  onStop,
  error,
  onRetryStart,
  onStartNew,
  window,
  windowMode,
  onWindowModeChange,
  rangeStart,
  rangeEnd,
  onRangeStartChange,
  onRangeEndChange,
  startInvalid,
  endInvalid,
  rangeMessage,
  hasPreviousTurn,
  entities,
  onRemoveEntity,
  onClearEntities,
  onAddEntity,
  entityOverrideActive,
  catalogState,
  catalogItems,
  onRetryCatalog,
  threadTitle,
  onClearThreadScope,
  textareaWrapRef,
}: ComposerProps) {
  const fieldId = useId();
  const questionId = `${fieldId}-question`;
  const hintId = `${fieldId}-hint`;
  const fromId = `${fieldId}-from`;
  const toId = `${fieldId}-to`;
  // The scope editor stays out of the way until the chips or their own control ask for it;
  // every chip removal routes here, so no chip is a dead affordance.
  const [scopeOpen, setScopeOpen] = useState(false);

  const applied = new Set(entities.map((entity) => entity.id));
  const addable = catalogItems.filter((entity) => !applied.has(entity.id));

  const addOptions = [
    {
      value: "",
      label: addable.length === 0 ? "Every entity is already in scope" : "Add an entity",
    },
    ...addable.map((entity) => ({ value: String(entity.id), label: entity.canonical_name })),
  ];

  let errorBlock: ReactNode = null;
  if (error !== null) {
    if (error.kind === "no_answer_model") {
      errorBlock = (
        <div
          role="alert"
          className="grid min-w-0 gap-1 rounded-md border border-status-danger-border px-3 py-2"
        >
          <p className="m-0 text-t-ui font-medium text-fg-primary">Answer model not loaded</p>
          <p className="m-0 text-t-sm text-fg-secondary">
            Load it in{" "}
            <Link to="/settings/models" className={TEXT_LINK}>
              Settings → Models
            </Link>
            , then ask again.
          </p>
        </div>
      );
    } else if (error.kind === "thread_not_found") {
      errorBlock = (
        <div
          role="alert"
          className="grid min-w-0 gap-2 rounded-md border border-status-danger-border px-3 py-2"
        >
          <p className="m-0 text-t-ui font-medium text-fg-primary">
            That conversation no longer exists
          </p>
          <p className="m-0 text-t-sm text-fg-secondary">
            It was deleted, so the question was not saved.
          </p>
          <div>
            <Button size="sm" variant="secondary" onClick={onStartNew}>
              Start a new conversation
            </Button>
          </div>
        </div>
      );
    } else if (error.kind === "failed") {
      errorBlock = (
        <div
          role="alert"
          className="grid min-w-0 gap-2 rounded-md border border-status-danger-border px-3 py-2"
        >
          <p className="m-0 text-t-ui font-medium text-fg-primary">The question didn’t start</p>
          <p className="m-0 break-words text-t-sm text-fg-secondary">{error.message}</p>
          <div>
            <Button size="sm" variant="secondary" onClick={onRetryStart}>
              Retry
            </Button>
          </div>
        </div>
      );
    } else {
      // A stream that failed or was stopped: the persisted turn keeps the question with its
      // own retry, so this line reports the interruption without a second alarm.
      errorBlock = (
        <p role="status" className="m-0 text-t-sm text-fg-secondary">
          {error.message}
        </p>
      );
    }
  }

  return (
    <div className="grid min-w-0 gap-3 border-t border-line-subtle pt-pane">
      {errorBlock}

      <div className="grid min-w-0 gap-2">
        <label htmlFor={questionId} className="text-t-ui font-medium text-fg-primary">
          Your question
        </label>
        <div ref={textareaWrapRef} className="min-w-0">
          <Textarea
            id={questionId}
            rows={3}
            value={query}
            aria-describedby={hintId}
            placeholder="What did I work on last week?"
            onChange={(event) => onQueryChange(event.target.value)}
            onKeyDown={(event) => {
              // Enter asks, Shift+Enter adds a line (the textarea's native newline). An IME
              // composition's Enter commits the composition, so it never submits.
              if (event.key !== "Enter" || event.shiftKey) return;
              if (event.nativeEvent.isComposing) return;
              if (!canSubmit) return;
              event.preventDefault();
              onSubmit();
            }}
          />
        </div>
        <span id={hintId} className="text-t-xs text-fg-tertiary">
          Enter asks · Shift + Enter adds a line
        </span>
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2">
        {/* One stream at a time: while any turn answers, the composer offers Stop instead of
            Ask, so a second question can never silently cancel the first. */}
        {streaming ? (
          <Button variant="secondary" onClick={onStop}>
            Stop answering
          </Button>
        ) : (
          <Button variant="primary" onClick={onSubmit} disabled={!canSubmit}>
            Ask
          </Button>
        )}
      </div>

      <ScopeChips
        window={window}
        entities={entities}
        threadTitle={threadTitle}
        editable
        onEditWindow={() => setScopeOpen(true)}
        onEditEntities={() => setScopeOpen(true)}
        onClear={onClearThreadScope}
      />

      <Expander
        title="Change the scope"
        intro="Set what the next question looks at. A follow-up keeps the last turn’s window and entities unless you change them here."
        open={scopeOpen}
        onToggle={() => setScopeOpen((open) => !open)}
      >
        <div className="grid min-w-0 gap-4">
          <Select
            label="Time window"
            value={windowMode}
            options={[
              { value: "inherit", label: "Same window as the last turn" },
              { value: "all", label: "All history" },
              { value: "range", label: "A date range" },
            ]}
            hint={
              windowMode === "all"
                ? "Every session counts."
                : windowMode === "range"
                  ? "Both dates are included."
                  : hasPreviousTurn
                    ? "Uses the window the last turn ran over."
                    : "The first turn of a conversation reads all history."
            }
            onChange={(event) => onWindowModeChange(event.target.value as WindowMode)}
          />

          {windowMode === "range" && (
            <div className="grid min-w-0 gap-2 sm:grid-cols-2">
              <div className="grid min-w-0 gap-1">
                <label htmlFor={fromId} className="text-t-sm font-medium text-fg-primary">
                  From
                </label>
                <TextInput
                  id={fromId}
                  type="date"
                  value={rangeStart}
                  invalid={startInvalid}
                  onChange={(event) => onRangeStartChange(event.target.value)}
                />
              </div>
              <div className="grid min-w-0 gap-1">
                <label htmlFor={toId} className="text-t-sm font-medium text-fg-primary">
                  To
                </label>
                <TextInput
                  id={toId}
                  type="date"
                  value={rangeEnd}
                  invalid={endInvalid}
                  onChange={(event) => onRangeEndChange(event.target.value)}
                />
              </div>
            </div>
          )}

          {rangeMessage !== null && (
            <p role="alert" className="m-0 text-t-xs text-status-danger-fg">
              {rangeMessage}
            </p>
          )}

          <div className="grid min-w-0 gap-2">
            <p className="m-0 text-t-sm font-medium text-fg-primary">Entity scope</p>
            {catalogState === "error" ? (
              <div className="flex flex-wrap items-center gap-2">
                <p className="m-0 text-t-sm text-fg-secondary">
                  Couldn’t read the work graph’s entities.
                </p>
                <Button size="sm" variant="secondary" onClick={onRetryCatalog}>
                  Retry
                </Button>
              </div>
            ) : catalogState === "loading" ? (
              <p role="status" className="m-0 text-t-sm text-fg-secondary">
                Reading the work graph…
              </p>
            ) : catalogState === "empty" ? (
              // §7i.1: the graph can be empty and Ask still answers — the entity scope simply
              // degrades to plain text, and the question is submitted with `entities: null`.
              <p className="m-0 text-t-sm text-fg-secondary">
                Entity scope unavailable until the work graph has entities
              </p>
            ) : (
              <>
                <EntityChips
                  entities={entities}
                  removable
                  onRemove={onRemoveEntity}
                  emptyLabel="No entity scope — the planner reads it from the question"
                />
                <div className="flex flex-wrap items-end gap-2">
                  <div className="min-w-0 flex-1">
                    <Select
                      label="Add an entity"
                      value=""
                      options={addOptions}
                      disabled={addable.length === 0}
                      onChange={(event) => {
                        const id = Number(event.target.value);
                        if (Number.isInteger(id) && id > 0) onAddEntity(id);
                      }}
                    />
                  </div>
                  {entityOverrideActive && (
                    <Button size="md" variant="ghost" onClick={onClearEntities}>
                      Clear entity scope
                    </Button>
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </Expander>
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// The route.
// ---------------------------------------------------------------------------------------

export function Component() {
  const [searchParams, setSearchParams] = useSearchParams();
  const selectedId = parseId(searchParams.get("thread"));
  // The Thread page deep-links a conversation from its scoped Ask; `scopeThread` is the
  // work-graph thread that conversation starts scoped to. `?thread=` selects a saved
  // conversation, `?scopeThread=` seeds a new one — two different ids, two different params.
  const urlScopeThreadId = parseId(searchParams.get("scopeThread"));

  const threads = useAskThreads();
  const turns = useAskTurns(selectedId);
  const conversation = useAskConversation();
  const deleteThread = useDeleteAskThread();
  const workThreads = useThreadList(THREAD_TITLES_QUERY);
  const catalog = useEntityList(ENTITY_CATALOG_QUERY);

  const [query, setQuery] = useState("");
  const [windowMode, setWindowMode] = useState<WindowMode>("inherit");
  const [rangeStart, setRangeStart] = useState("");
  const [rangeEnd, setRangeEnd] = useState("");
  const [entityOverride, setEntityOverride] = useState<number[] | null>(null);
  const [threadScopeCleared, setThreadScopeCleared] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<AskThreadSummary | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [lastRequest, setLastRequest] = useState<
    Omit<AskConversationRequest, "request_id"> | null
  >(null);
  const textareaWrapRef = useRef<HTMLDivElement>(null);

  const threadList = threads.data ?? [];
  // The store returns `ordinal` order; sorting here makes the render independent of that
  // guarantee, so a turn can never appear out of place.
  const turnList = useMemo(
    () => [...(turns.data ?? [])].sort((a, b) => a.ordinal - b.ordinal),
    [turns.data],
  );
  const lastTurn = turnList.length === 0 ? undefined : turnList[turnList.length - 1];

  const catalogItems = useMemo<EntityChipItem[]>(
    () =>
      (catalog.data ?? []).map((entity) => ({
        id: entity.id,
        kind: entity.kind,
        canonical_name: entity.canonical_name,
      })),
    [catalog.data],
  );

  const catalogState: CatalogState = catalog.isPending
    ? "loading"
    : catalog.isError
      ? "error"
      : catalogItems.length === 0
        ? "empty"
        : "populated";

  // What the next turn will run over: the composer's explicit choice, or — for `inherit` —
  // the window the last turn actually ran. Nothing here guesses a scope the request will not
  // send (the request is built from the same values in `submitQuestion`).
  const effectiveEntityIds = useMemo(
    () => entityOverride ?? lastTurn?.resolved_entities ?? [],
    [entityOverride, lastTurn],
  );

  // One entity read for every id the page draws: each turn's resolved entities plus the
  // composer's scope.
  const requestedEntityIds = useMemo(() => {
    const ids = new Set<number>();
    for (const turn of turnList) for (const id of turn.resolved_entities) ids.add(id);
    for (const id of effectiveEntityIds) ids.add(id);
    return [...ids].sort((a, b) => a - b);
  }, [turnList, effectiveEntityIds]);
  const entities = useEntities(requestedEntityIds);

  // Names for every entity id the page draws: the catalog covers the picker, and the id read
  // covers scopes that name entities older than the catalog page, so no chip has to guess.
  const entityById = useMemo(() => {
    const map = new Map<number, EntitySummary>();
    for (const entity of catalog.data ?? []) map.set(entity.id, entity);
    for (const entity of entities.data ?? []) map.set(entity.id, entity);
    return map;
  }, [catalog.data, entities.data]);

  const threadTitles = useMemo(() => {
    const map = new Map<number, string>();
    for (const thread of workThreads.data?.threads ?? []) map.set(thread.id, thread.title);
    return map;
  }, [workThreads.data]);

  // The next turn's thread scope: an explicit `?scopeThread=` deep link wins, else the scope
  // the last turn ran with (§7i.4 inherits it). Clearing the chip switches the request to
  // `clear_thread_scope`, which is the only way to run a follow-up unscoped.
  const scopeThreadId = threadScopeCleared
    ? null
    : (urlScopeThreadId ?? lastTurn?.scope_thread_id ?? null);
  const scopeThreadTitle =
    scopeThreadId === null
      ? null
      : (threadTitles.get(scopeThreadId) ?? `Thread ${scopeThreadId}`);

  // `rangeEndMs` is the end date's own midnight, so the ordering check compares calendar days
  // rather than the exclusive bound; `rangeEndExclusiveMs` is the half-open end the request
  // carries, one local calendar day later (`localDateMidnight`, DST-safe — never `+ DAY_MS`).
  const rangeStartMs = localDateMidnight(rangeStart);
  const rangeEndMs = localDateMidnight(rangeEnd);
  const rangeEndExclusiveMs = localDateMidnight(rangeEnd, 1);
  const rangeMode = windowMode === "range";
  const rangeIncomplete = rangeMode && (rangeStartMs === null || rangeEndMs === null);
  const rangeOutOfOrder =
    rangeMode && rangeStartMs !== null && rangeEndMs !== null && rangeEndMs < rangeStartMs;
  const rangeInvalid = rangeIncomplete || rangeOutOfOrder;
  const rangeMessage = rangeOutOfOrder
    ? "The end date has to be on or after the start date."
    : rangeIncomplete && (rangeStart !== "" || rangeEnd !== "")
      ? "Pick both dates to run this question over a range."
      : null;

  const windowScope = useMemo<AskWindowScope>(() => {
    if (windowMode === "all") return { mode: "all" };
    if (
      rangeMode &&
      rangeStartMs !== null &&
      rangeEndMs !== null &&
      rangeEndExclusiveMs !== null &&
      rangeEndMs >= rangeStartMs
    ) {
      // Half-open `[start, end)`: the end date the person picked is included in full, in any
      // zone — a 23h or 25h local day still ends at the next local midnight.
      return { mode: "range", start: rangeStartMs, end: rangeEndExclusiveMs };
    }
    return { mode: "inherit" };
  }, [windowMode, rangeMode, rangeStartMs, rangeEndMs, rangeEndExclusiveMs]);

  const displayWindow = useMemo<TimeRange | null>(() => {
    if (windowScope.mode === "all") return null;
    if (windowScope.mode === "range") {
      return { start: windowScope.start, end: windowScope.end };
    }
    // An incomplete range is never sent (`canSubmit` is false until both dates parse), so the
    // chips keep describing the inherited window instead of inventing a range.
    return lastTurn?.window ?? null;
  }, [windowScope, lastTurn]);

  const appliedEntities = useMemo(
    () => toChipItems(effectiveEntityIds, entityById),
    [effectiveEntityIds, entityById],
  );

  const streaming = conversation.state.phase === "streaming";
  // The conversation hook returns a fresh object each render; its `ask` and `reset` callbacks
  // are stable, so callbacks built here depend on those instead of the object identity.
  const askTurn = conversation.ask;
  const resetConversation = conversation.reset;

  const focusComposer = useCallback(() => {
    // The Textarea primitive forwards no ref (it is a plain function component), so the
    // composer is reached through its wrapper after an action moves the page's focus anchor.
    const node = textareaWrapRef.current?.querySelector("textarea");
    node?.focus();
  }, []);

  const submitQuestion = useCallback(
    (text: string): boolean => {
      const value = text.trim();
      if (value === "" || streaming || rangeInvalid) return false;
      const request: Omit<AskConversationRequest, "request_id"> = {
        query: value,
        // `request_id` is stamped by `useAskConversation` (it keys the stream deltas and the
        // terminal `citations` delta), so the view never invents one.
        // `?thread=` is the conversation a turn appends to; no selection starts a new one
        // (`ask_turn` creates the row and names it after this question).
        thread_id: selectedId,
        thinking: false,
        max_tokens: ASK_MAX_TOKENS,
        top_k: null,
        window: windowScope,
        // An empty graph has no entity scope to send, so the question keeps the plain-text
        // path (`03 §7i.1`) instead of pushing an override the graph cannot honour.
        entities: catalogState === "empty" ? null : entityOverride,
        scope_thread_id: scopeThreadId,
        clear_thread_scope: threadScopeCleared,
      };
      setLastRequest(request);
      void askTurn(request);
      return true;
    },
    [
      askTurn,
      catalogState,
      entityOverride,
      rangeInvalid,
      scopeThreadId,
      selectedId,
      streaming,
      threadScopeCleared,
      windowScope,
    ],
  );

  const retryTurn = useCallback(
    (turn: AskTurn) => {
      const request: Omit<AskConversationRequest, "request_id"> = {
        query: turn.question,
        thread_id: turn.thread_id,
        thinking: false,
        max_tokens: ASK_MAX_TOKENS,
        top_k: null,
        // §7i.4: a retry reuses the conversation's own scope rather than inventing one.
        window: { mode: "inherit" },
        entities: null,
        scope_thread_id: null,
        clear_thread_scope: false,
      };
      setLastRequest(request);
      void askTurn(request);
    },
    [askTurn],
  );

  // Scope and window overrides are edits to *this* conversation's retrieval, not to the page, so
  // a new conversation identity drops them — otherwise the next question pushes the previous
  // conversation's cleared thread scope, entity or window onto a conversation that never asked
  // for it.
  const resetScopeOverrides = () => {
    setWindowMode("inherit");
    setRangeStart("");
    setRangeEnd("");
    setEntityOverride(null);
    setThreadScopeCleared(false);
  };
  // Adjusted during render, not in an effect: an effect runs after paint, so one committed frame
  // (and any submit taken from it) would still carry the previous conversation's scope. The key
  // also covers "New conversation" (it clears `?thread=`) and a `?scopeThread=` deep link.
  const scopeKey = `${selectedId ?? "new"}:${urlScopeThreadId ?? "none"}`;
  const [scopeKeySeen, setScopeKeySeen] = useState(scopeKey);
  if (scopeKeySeen !== scopeKey) {
    setScopeKeySeen(scopeKey);
    // Adopting the conversation this page just created is not a switch: the key moves from
    // `new:` to that conversation's id a second after the first question is asked, and resetting
    // here would discard what the user set for the *next* question while the answer arrived —
    // including a thread chip they had deliberately cleared.
    const adopting = selectedId !== null && selectedId === conversation.state.threadId;
    if (!adopting) resetScopeOverrides();
  }

  const startNewConversation = useCallback(() => {
    resetConversation();
    resetScopeOverrides();
    setLastRequest(null);
    const next = new URLSearchParams(searchParams);
    next.delete("thread");
    setSearchParams(next);
    focusComposer();
  }, [focusComposer, resetConversation, searchParams, setSearchParams]);

  // A new conversation has no id until `ask_turn` creates one; adopting it into `?thread=`
  // makes the turn deep-linkable and lets the persisted rows take over from the live stream.
  // Only adopted while nothing is selected: switching to another conversation mid-stream
  // must never be undone by this effect.
  const liveThreadId = conversation.state.threadId;
  const phase = conversation.state.phase;
  useEffect(() => {
    if (selectedId !== null || liveThreadId === null) return;
    if (phase !== "streaming" && phase !== "done") return;
    const next = new URLSearchParams(searchParams);
    next.set("thread", String(liveThreadId));
    // The deep-link seed has now been consumed: it existed to scope the question about to be
    // asked, and from here the turn's persisted scope is the truth. Leaving it in the URL made it
    // win every later turn (`?scopeThread=` outranks the conversation's own scope), so clearing
    // the chip and asking once did not survive the adoption that followed.
    next.delete("scopeThread");
    setSearchParams(next, { replace: true });
  }, [liveThreadId, phase, searchParams, selectedId, setSearchParams]);

  const confirmDelete = async () => {
    const target = deleteTarget;
    if (target === null) return;
    setDeleteError(null);
    try {
      await deleteThread.mutateAsync(target.id);
    } catch (error) {
      setDeleteError(
        error instanceof Error ? error.message : "The conversation could not be deleted.",
      );
      return;
    }
    setDeleteTarget(null);
    // The deleted conversation may be the one the live stream still holds (a finished turn keeps
    // its id and text in `conversation.state`). Leaving it there lets the adoption effect put the
    // deleted id straight back into `?thread=`, so the composer would then ask into a conversation
    // that no longer exists (`thread_not_found`) instead of starting a new one.
    if (target.id === liveThreadId) {
      resetConversation();
      setLastRequest(null);
    }
    if (target.id !== selectedId) return;
    // Select the next conversation, else the previous one, else the empty state.
    const index = threadList.findIndex((thread) => thread.id === target.id);
    const nextThread =
      (index >= 0 ? threadList[index + 1] : undefined) ??
      (index > 0 ? threadList[index - 1] : undefined) ??
      null;
    const next = new URLSearchParams(searchParams);
    if (nextThread === null) next.delete("thread");
    else next.set("thread", String(nextThread.id));
    setSearchParams(next, { replace: true });
    if (nextThread === null) focusComposer();
  };

  const closeDelete = () => {
    setDeleteTarget(null);
    setDeleteError(null);
  };

  // The live turn renders only in the conversation it belongs to, so switching threads while
  // an answer streams cannot show another conversation's words here.
  const liveMatchesSelection =
    liveThreadId === null ? selectedId === null : liveThreadId === selectedId;
  const persistedLiveTurn =
    conversation.state.turnId !== null &&
    turnList.some(
      (turn) =>
        turn.id === conversation.state.turnId &&
        turn.answer !== null &&
        turn.answer.trim() !== "",
    );
  const liveVisible =
    liveMatchesSelection &&
    !persistedLiveTurn &&
    (phase === "streaming" ||
      // A finished stream keeps its text until the persisted rows arrive; a finished stream
      // with no text is a failed turn, and the persisted row already carries its retry.
      (phase === "done" && conversation.state.answer.trim() !== ""));

  // While a turn streams, its own persisted row is still NULL; hiding it keeps the live block
  // as the single rendering of that turn (a NULL row would otherwise read as a failed one).
  const visibleTurns = turnList.filter((turn) => {
    if (!streaming || liveThreadId === null || turn.thread_id !== liveThreadId) return true;
    if (conversation.state.turnId !== null && turn.id === conversation.state.turnId) return false;
    return turn.answer !== null;
  });

  const unanswered = visibleTurns.filter(
    (turn) => turn.answer === null || turn.answer.trim() === "",
  ).length;

  let conversationBody;
  if (selectedId === null) {
    conversationBody = (
      <Invitation
        onPick={(prompt) => {
          if (submitQuestion(prompt)) setQuery("");
        }}
        disabled={streaming || rangeInvalid}
      />
    );
  } else if (turns.isPending) {
    conversationBody = (
      <div className="grid min-w-0 gap-3" aria-busy="true">
        <p role="status" className="sr-only">
          Loading this conversation
        </p>
        {[0, 1].map((row) => (
          <div key={row} className="grid min-w-0 gap-2 border-b border-line-subtle pb-pane last:border-b-0">
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ))}
      </div>
    );
  } else if (turns.isError) {
    conversationBody = (
      <ErrorState
        align="start"
        title="Couldn’t load this conversation"
        message="Retry to read its turns."
        onRetry={() => void turns.refetch()}
      />
    );
  } else if (visibleTurns.length === 0 && !liveVisible) {
    conversationBody = (
      <Invitation
        onPick={(prompt) => {
          if (submitQuestion(prompt)) setQuery("");
        }}
        disabled={streaming || rangeInvalid}
      />
    );
  } else {
    conversationBody = (
      <div className="grid min-w-0 gap-3">
        {unanswered > 0 && (
          // The partial state: the conversation arrived, but not every turn has an answer.
          <p className="m-0 text-t-xs text-fg-secondary">
            {formatCount(unanswered, "turn")} {unanswered === 1 ? "has" : "have"} no answer
            yet — retry {unanswered === 1 ? "it" : "them"} below.
          </p>
        )}
        <ol className="m-0 flex list-none flex-col p-0">
          {visibleTurns.map((turn, index) => (
            <TurnBlock
              key={turn.id}
              turn={turn}
              previous={index > 0 ? visibleTurns[index - 1] : undefined}
              entityById={entityById}
              threadTitles={threadTitles}
              onRetry={retryTurn}
            />
          ))}
        </ol>
      </div>
    );
  }

  const selectedSummary =
    selectedId === null ? undefined : threadList.find((thread) => thread.id === selectedId);
  const conversationTitle =
    selectedId === null
      ? "New conversation"
      : selectedSummary
        ? conversationLabel(selectedSummary)
        : `Conversation ${selectedId}`;

  return (
    <div className="mx-auto flex min-w-0 max-w-6xl flex-col gap-pane px-pane py-pane">
      <header className="min-w-0">
        <h1 className="text-t-h1 font-semibold text-fg-primary">Ask</h1>
        <p className="mt-1 max-w-prose text-t-ui text-fg-secondary">
          Ask across your threads, sessions and moments. Every answer cites what it read.
        </p>
      </header>

      <div className="grid min-w-0 grid-cols-12 gap-gap">
        <aside className="col-span-12 min-w-0 lg:col-span-4 xl:col-span-3">
          <Panel
            flush
            title="Conversations"
            action={
              <Button size="sm" variant="secondary" onClick={startNewConversation}>
                New conversation
              </Button>
            }
          >
            {threads.isPending ? (
              <div aria-busy="true">
                <p role="status" className="sr-only">
                  Loading conversations
                </p>
                <ul className="m-0 list-none p-0">
                  {[0, 1, 2].map((row) => (
                    <li key={row}>
                      <ListRow state="loading" />
                    </li>
                  ))}
                </ul>
              </div>
            ) : threads.isError ? (
              <ErrorState
                align="start"
                title="Couldn’t load conversations"
                message="Retry to read the conversations saved on this machine."
                onRetry={() => void threads.refetch()}
              />
            ) : threadList.length === 0 ? (
              <EmptyState
                align="start"
                title="No conversations yet"
                description="Ask a question and it is saved here."
              />
            ) : (
              <ul className="m-0 list-none p-0">
                {threadList.map((thread) => {
                  const selected = thread.id === selectedId;
                  const label = conversationLabel(thread);
                  return (
                    <li
                      key={thread.id}
                      className="flex min-w-0 items-center gap-1 border-b border-line-subtle px-1 last:border-b-0"
                    >
                      <ListRow
                        as="div"
                        selected={selected}
                        className="min-w-0 flex-1"
                        title={
                          <Link
                            to={`/ask?thread=${thread.id}`}
                            aria-current={selected ? "page" : undefined}
                            className={ROW_LINK}
                          >
                            <span className="truncate">{label}</span>
                          </Link>
                        }
                        meta={`${formatCount(thread.turn_count, "turn")} · ${relativeTime(
                          thread.last_turn_at ?? thread.created_at,
                        )}`}
                      />
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={`Delete the conversation ${label}`}
                        onClick={() => {
                          setDeleteError(null);
                          setDeleteTarget(thread);
                        }}
                      >
                        Delete
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
          </Panel>
        </aside>

        <div className="col-span-12 min-w-0 lg:col-span-8 xl:col-span-9">
          <Panel
            title={conversationTitle}
            action={
              selectedId !== null && turnList.length > 0 ? (
                <span className="text-t-xs text-fg-tertiary">
                  {formatCount(turnList.length, "turn")}
                </span>
              ) : undefined
            }
          >
            <div className="grid min-w-0 gap-3">
              {conversationBody}

              {liveVisible && (
                <StreamingTurn
                  question={
                    lastRequest !== null &&
                    (selectedId === null || lastRequest.thread_id === selectedId)
                      ? lastRequest.query
                      : null
                  }
                  answer={conversation.state.answer}
                  citations={conversation.state.citations}
                  reading={streaming}
                />
              )}

              <Composer
                query={query}
                onQueryChange={setQuery}
                canSubmit={query.trim() !== "" && !streaming && !rangeInvalid}
                onSubmit={() => {
                  if (submitQuestion(query)) setQuery("");
                }}
                streaming={streaming}
                onStop={conversation.cancel}
                error={conversation.state.error}
                onRetryStart={() => {
                  if (lastRequest !== null) void askTurn(lastRequest);
                }}
                onStartNew={startNewConversation}
                window={displayWindow}
                windowMode={windowMode}
                onWindowModeChange={setWindowMode}
                rangeStart={rangeStart}
                rangeEnd={rangeEnd}
                onRangeStartChange={setRangeStart}
                onRangeEndChange={setRangeEnd}
                startInvalid={rangeInvalid && rangeStart !== ""}
                endInvalid={rangeInvalid && rangeEnd !== ""}
                rangeMessage={rangeMessage}
                hasPreviousTurn={lastTurn !== undefined}
                entities={appliedEntities}
                onRemoveEntity={(id) => {
                  // Removing from the applied scope is explicit: the remaining ids are sent
                  // as an override, and removing the last one clears the entity scope.
                  setEntityOverride(effectiveEntityIds.filter((entityId) => entityId !== id));
                }}
                onClearEntities={() => setEntityOverride([])}
                onAddEntity={(id) => {
                  if (effectiveEntityIds.includes(id)) return;
                  setEntityOverride([...effectiveEntityIds, id]);
                }}
                entityOverrideActive={entityOverride !== null}
                catalogState={catalogState}
                catalogItems={catalogItems}
                onRetryCatalog={() => void catalog.refetch()}
                threadTitle={scopeThreadTitle}
                onClearThreadScope={() => setThreadScopeCleared(true)}
                textareaWrapRef={textareaWrapRef}
              />
            </div>
          </Panel>
        </div>
      </div>

      <Dialog
        open={deleteTarget !== null}
        onClose={closeDelete}
        title="Delete this conversation?"
        description={
          deleteTarget === null
            ? undefined
            : `This deletes the conversation and its ${formatCount(
                deleteTarget.turn_count,
                "turn",
              )}. Frames, sessions, the work graph and insights stay.`
        }
        actions={
          <>
            <Button variant="ghost" onClick={closeDelete}>
              Cancel
            </Button>
            <Button
              variant="danger"
              loading={deleteThread.isPending}
              onClick={() => void confirmDelete()}
            >
              Delete conversation
            </Button>
          </>
        }
      >
        {deleteError !== null && (
          <p role="alert" className="m-0 break-words text-t-sm text-status-danger-fg">
            {deleteError}
          </p>
        )}
      </Dialog>
    </div>
  );
}
