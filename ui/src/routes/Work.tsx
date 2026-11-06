// Work (`/work`) — the thread list: what is in flight, most recently touched first
// (`03 §7h.4`, `UI_REFERENCE §3`/`§4`). One read owns the whole screen: `list_threads`
// returns the rows *and* the backfill progress, so the "N of M sessions clustered" line
// can never disagree with the rows it frames.
//
// View state lives in the URL: `?sort=last|time` and the `?entity=<id>` filter that an
// entity chip or an insight's evidence link sets. That is also why "Try again" keeps
// them — the retry refetches the same key instead of resetting a local control.
//
// Sort is real (`docs/0.4.5.md` P9): the store orders by `last_seen_at` desc, and
// "Time spent" re-orders the very same rows by `active_ms` desc with `id` desc as the
// tiebreak, so two equal figures never swap places between renders.
//
// Nothing on this screen writes `threads` or `thread_sessions`. Rename, merge and split
// are the three `thread_overrides` commands, and the clusterer applies them afterwards
// (`03 §7h.5`); the list is therefore re-read from each mutation's cache invalidation
// and never patched by hand. The merge and split confirmations themselves live in
// `components/intelligence/threadOverrides`, shared with the Thread page, so the same
// sentence about the same destructive step is stated once and on both screens.
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { Link, useSearchParams } from "react-router-dom";

import {
  Button,
  Chip,
  EmptyState,
  ErrorState,
  Panel,
  Segmented,
  Skeleton,
  TextInput,
  type SegmentedOption,
} from "../components/primitives";
import {
  MergeThreadDialog,
  SplitThreadDialog,
  formatCount,
  formatDuration,
} from "../components/intelligence";
import { IconWork } from "../components/icons";
import { useEntityList, useThreadList, useThreadPage } from "../lib/ipc/queries";
import { useRenameThread } from "../lib/ipc/mutations";
import { absoluteTime, relativeTime } from "../lib/time";
import { toast } from "../state/toastStore";
import type { GraphProgress } from "../bindings/GraphProgress";
import type { ThreadSummary } from "../bindings/ThreadSummary";

/** `03 §7h.4`: a thread clustered only on `confidence >= 0.7` edges is certain enough to
 *  present without a caveat; below it the row says so instead of implying agreement. */
const LOW_CONFIDENCE = 0.7;

/** One page of threads: the graph holds one row per subject, so 200 covers a long
 *  history and the command clamps anything above its own ceiling. */
const THREAD_LIST_LIMIT = 200;

/** The entity catalog page the filter's chip resolves its name from. */
const ENTITY_CATALOG_LIMIT = 1000;

type SortMode = "last" | "time";

const SORT_OPTIONS: SegmentedOption[] = [
  { value: "last", label: "Last touched" },
  { value: "time", label: "Time spent" },
];

/** The row link is the row's whole text column, so its figures are inside the link to
 *  the page that proves them. */
const ROW_LINK =
  "block min-w-0 rounded-md py-0.5 " +
  "motion-safe:transition-colors duration-fast ease-standard " +
  "hover:bg-surface-hover " +
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

/** `?entity=` carries an `entities.id`; anything else is no filter at all. */
function readEntityId(raw: string | null): number | null {
  if (raw == null || !/^\d+$/.test(raw)) return null;
  const id = Number(raw);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

/** Real sort over the rows the store returned, with `id` desc as the stable tiebreak. */
function sortThreads(threads: ThreadSummary[], sort: SortMode): ThreadSummary[] {
  return [...threads].sort((a, b) => {
    const primary =
      sort === "time" ? b.active_ms - a.active_ms : b.last_seen_at - a.last_seen_at;
    return primary !== 0 ? primary : b.id - a.id;
  });
}

/**
 * "3 of 12 sessions clustered" (`UI_REFERENCE §4`). Composed here rather than through
 * `formatCount`: that helper pluralises its own noun, and this sentence's noun belongs to
 * the total, so "1 of 12 sessions clustered" is right and "1 of 12 session clustered" is
 * not.
 */
function clusteredLine(progress: GraphProgress): string {
  const total = progress.sessions_total;
  return `${progress.sessions_clustered} of ${total} ${
    total === 1 ? "session" : "sessions"
  } clustered`;
}

/** A keystroke belongs to the field, not to the screen, while one of these has focus. */
function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

/** The counts line: announced, above the rows, and never a row count the read does not
 *  know — the graph reports sessions clustered, not threads pending. */
function ClusteredLine({ progress }: { progress: GraphProgress }) {
  return (
    <p
      role="status"
      className="m-0 border-b border-line-subtle px-pane py-2 text-t-xs text-fg-secondary"
    >
      {clusteredLine(progress)}
    </p>
  );
}

/**
 * Inline rename. Mounted only while a row is being renamed, so the mutation hook exists
 * for the duration of one edit and the row's own title stays the untouched record.
 *
 * Enter commits (the form's submit), Escape cancels, and an empty or unchanged title
 * never reaches the command: the store rejects a whitespace-only title (`03 §7h.5`), and
 * "unchanged" is not a change.
 */
function RenameForm({
  thread,
  onClose,
}: {
  thread: ThreadSummary;
  onClose: () => void;
}) {
  const rename = useRenameThread();
  const [draft, setDraft] = useState(thread.title);
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
    <form className="flex min-w-0 flex-wrap items-center gap-2 py-0.5" onSubmit={commit}>
      <TextInput
        size="sm"
        autoFocus
        className="min-w-0 flex-1"
        aria-label={`New title for ${thread.title}`}
        value={draft}
        invalid={trimmed.length === 0}
        onChange={(event) => setDraft(event.target.value)}
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

/** One thread row (`§4`): title, last touched, session count, time spent, and an honest
 *  low-confidence line — muted text, never a badge, count or score (`0.4.5 D13`). */
function ThreadRow({
  thread,
  renaming,
  onStartRename,
  onCancelRename,
  onMerge,
  onSplit,
}: {
  thread: ThreadSummary;
  renaming: boolean;
  onStartRename: (id: number) => void;
  onCancelRename: () => void;
  onMerge: (thread: ThreadSummary) => void;
  onSplit: (thread: ThreadSummary) => void;
}) {
  const titleRef = useRef<HTMLAnchorElement | null>(null);
  const wasRenaming = useRef(false);

  // Rename ends where it started: Escape and Save both put the focus back on the row's
  // title instead of dropping it onto the document.
  useEffect(() => {
    if (wasRenaming.current && !renaming) titleRef.current?.focus();
    wasRenaming.current = renaming;
  }, [renaming]);

  return (
    <li
      data-thread-id={thread.id}
      className="border-b border-line-subtle px-3 last:border-b-0"
    >
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3 py-2">
        <div className="min-w-0">
          {renaming ? (
            <RenameForm thread={thread} onClose={onCancelRename} />
          ) : (
            <Link
              ref={titleRef}
              to={`/work/${thread.id}`}
              className={ROW_LINK}
              onKeyDown={(event) => {
                if (
                  (event.key === "r" || event.key === "R") &&
                  !event.metaKey &&
                  !event.ctrlKey &&
                  !event.altKey
                ) {
                  event.preventDefault();
                  onStartRename(thread.id);
                }
              }}
            >
              <span className="block truncate text-t-body font-medium text-fg-primary">
                {thread.title}
              </span>
              <span className="mt-0.5 block text-t-xs text-fg-secondary">
                Last touched{" "}
                <span title={absoluteTime(thread.last_seen_at)}>
                  {relativeTime(thread.last_seen_at)}
                </span>
                {" · "}
                {formatCount(thread.session_count, "session")}
                {" · "}
                {formatDuration(thread.active_ms)} spent
              </span>
              {thread.confidence < LOW_CONFIDENCE && (
                <span className="mt-0.5 block text-t-xs text-fg-tertiary">
                  Low confidence — few shared entities
                </span>
              )}
            </Link>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {!renaming && (
            <>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Rename ${thread.title}`}
                onClick={() => onStartRename(thread.id)}
              >
                Rename
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Merge ${thread.title} into another thread`}
                onClick={() => onMerge(thread)}
              >
                Merge…
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Split ${thread.title}`}
                onClick={() => onSplit(thread)}
              >
                Split…
              </Button>
            </>
          )}
        </div>
      </div>
    </li>
  );
}

/** Rows that reserve the geometry a populated row occupies: one title line, one meta
 *  line, and the action group's height (never a fake row *count*). */
function ThreadListSkeleton() {
  return (
    <div aria-busy="true">
      <p role="status" className="sr-only">
        Loading threads
      </p>
      <ul className="m-0 list-none p-0">
        {[0, 1, 2, 3].map((row) => (
          <li key={row} className="border-b border-line-subtle px-3 py-2 last:border-b-0">
            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
              <div className="grid min-w-0 gap-1">
                <Skeleton h="4" w="half" />
                <Skeleton h="3" w="twoThirds" />
              </div>
              <Skeleton h="row" w="24" />
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * The split dialog's sessions are the one row this page cannot hand over: the list
 * carries counts, not member sessions. So this page owns that read and mounts it on
 * open — one thread, not one per row — while the confirmation itself is shared
 * (`components/intelligence/threadOverrides`) and identical to the Thread page's.
 */
function SplitThreadMount({
  thread,
  onClose,
}: {
  thread: ThreadSummary;
  onClose: () => void;
}) {
  const page = useThreadPage(thread.id);
  return (
    <SplitThreadDialog
      open
      thread={thread}
      sessions={page.data?.detail?.sessions ?? []}
      sessionsState={page.isLoading ? "loading" : page.isError ? "error" : "ready"}
      onRetrySessions={() => void page.refetch()}
      onClose={onClose}
    />
  );
}

export function Component() {
  const [searchParams, setSearchParams] = useSearchParams();
  // Both are ephemeral view state, so an absent or unreadable value is the default
  // rather than an error: `?sort=` anything but `time` is "last touched".
  const sort: SortMode = searchParams.get("sort") === "time" ? "time" : "last";
  const entityId = readEntityId(searchParams.get("entity"));

  const list = useThreadList({
    from_ms: null,
    to_ms: null,
    entity_id: entityId,
    limit: THREAD_LIST_LIMIT,
  });
  // The filter chip has to name the entity it filters on; an id the catalog no longer
  // holds still reads as `Entity 42` instead of losing the chip (`§4`).
  const entities = useEntityList(
    { kind: null, query: null, limit: ENTITY_CATALOG_LIMIT },
    true,
  );

  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [mergeSource, setMergeSource] = useState<ThreadSummary | null>(null);
  const [splitSource, setSplitSource] = useState<ThreadSummary | null>(null);

  const rows = useMemo(
    () => sortThreads(list.data?.threads ?? [], sort),
    [list.data, sort],
  );
  const progress = list.data?.progress ?? null;
  const entityName =
    entityId == null
      ? null
      : (entities.data?.find((entity) => entity.id === entityId)?.canonical_name ??
        `Entity ${entityId}`);

  const setSort = (next: string) => {
    const params = new URLSearchParams(searchParams);
    if (next === "time") params.set("sort", "time");
    else params.delete("sort");
    setSearchParams(params, { replace: true });
  };

  const clearEntity = () => {
    const params = new URLSearchParams(searchParams);
    params.delete("entity");
    setSearchParams(params, { replace: true });
  };

  const dialogOpen = mergeSource != null || splitSource != null;

  /**
   * Screen-level shortcuts: `s` cycles the sort and `r` starts renaming the row the focus
   * is in. Both stay out of the way while a field has focus, while a modifier is held and
   * while a dialog owns the keyboard. Enter is the row link's own behaviour, so opening a
   * row needs no handler here.
   */
  const onPageKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.defaultPrevented || dialogOpen) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (isTypingTarget(event.target)) return;

    if (event.key === "s" || event.key === "S") {
      event.preventDefault();
      setSort(sort === "last" ? "time" : "last");
      return;
    }

    if (event.key === "r" || event.key === "R") {
      const row =
        event.target instanceof Element
          ? event.target.closest("[data-thread-id]")
          : null;
      const id =
        row instanceof HTMLElement ? Number(row.dataset.threadId) : Number.NaN;
      if (Number.isSafeInteger(id)) {
        event.preventDefault();
        setRenamingId(id);
      }
    }
  };

  let body: ReactNode;
  if (list.isLoading) {
    body = <ThreadListSkeleton />;
  } else if (list.isError || !list.data) {
    body = (
      <ErrorState
        align="start"
        title="Could not load your threads"
        message="The work graph read failed. Trying again keeps the current sort and filter."
        onRetry={() => void list.refetch()}
      />
    );
  } else if (rows.length === 0) {
    // An empty graph and an entity filter that matched nothing are different statements,
    // so the filtered case says which one it is and offers the way back.
    body =
      entityId == null ? (
        <>
          {progress?.running && <ClusteredLine progress={progress} />}
          <EmptyState
            align="start"
            icon={<IconWork />}
            title="No threads yet"
            description="They appear once enough sessions share a subject."
          />
        </>
      ) : (
        <>
          {progress?.running && <ClusteredLine progress={progress} />}
          <EmptyState
            align="start"
            icon={<IconWork />}
            title="No threads touched this entity"
            description={`Nothing clustered around ${entityName} yet. Clear the filter to see every thread.`}
            action={
              <Button size="sm" onClick={clearEntity}>
                Clear the filter
              </Button>
            }
          />
        </>
      );
  } else {
    body = (
      <>
        {progress?.running && <ClusteredLine progress={progress} />}
        <ul className="m-0 list-none p-0">
          {rows.map((thread) => (
            <ThreadRow
              key={thread.id}
              thread={thread}
              renaming={renamingId === thread.id}
              onStartRename={setRenamingId}
              onCancelRename={() => setRenamingId(null)}
              onMerge={setMergeSource}
              onSplit={setSplitSource}
            />
          ))}
        </ul>
      </>
    );
  }

  return (
    <div
      className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane"
      onKeyDown={onPageKeyDown}
    >
      <header className="min-w-0">
        <h1 className="text-t-h1 font-semibold text-fg-primary">Work</h1>
        <p className="mt-1 max-w-prose text-t-ui text-fg-secondary">
          What is in flight. Open a thread to see its sessions, entities and open loops.
        </p>
      </header>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <Segmented label="Sort" options={SORT_OPTIONS} value={sort} onChange={setSort} />
        {entityName != null && (
          <Chip onRemove={clearEntity} removeLabel="Clear the entity filter">
            {`Entity: ${entityName}`}
          </Chip>
        )}
      </div>

      <Panel flush>{body}</Panel>

      {mergeSource && (
        <MergeThreadDialog
          open
          source={{
            id: mergeSource.id,
            title: mergeSource.title,
            sessionCount: mergeSource.session_count,
          }}
          survivors={rows}
          onClose={() => setMergeSource(null)}
        />
      )}
      {splitSource && (
        <SplitThreadMount thread={splitSource} onClose={() => setSplitSource(null)} />
      )}
    </div>
  );
}
