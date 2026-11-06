// Thread overrides (`03 §7h.5`): the two confirmations that rewrite a grouping —
// folding one thread into a survivor, and lifting sessions out into a thread of
// their own. Both screens that offer them (the Work list and the Thread page)
// render these, so the sentence a destructive action states exists once instead of
// twice and cannot drift between the two places it is offered.
//
// Neither component fetches: the caller owns the read (the Work list's rows, the
// Thread page's own detail) and hands the rows in together with the state of that
// read, so a dialog never claims "there is no other thread" while the list is
// still arriving. Each dialog owns the mutation it confirms, its pending state,
// its failure text inline (`role="status"`, inside the dialog that failed) and the
// rule that decides when confirming is allowed: a merge needs a survivor, a split
// needs at least one session.
//
// Nothing here writes `threads` or `thread_sessions`. The three override commands
// are the only writes, the clusterer applies them afterwards, and a caller
// re-reads through the mutations' own cache invalidation.
import { useState, type ReactNode } from "react";

import {
  Button,
  Checkbox,
  Dialog,
  ErrorState,
  Select,
  Skeleton,
  type SelectOption,
} from "../primitives";
import { relativeTime } from "../../lib/time";
import { toast } from "../../state/toastStore";
import { useMergeThreads, useSplitThread } from "../../lib/ipc/mutations";
import { formatCount } from "./format";
import type { ThreadSessionRef } from "../../bindings/ThreadSessionRef";

/** `sessions.kind` tokens (`03 §7e`) as the two words the picker shows. */
const SESSION_KIND_LABEL: Record<string, string> = {
  focus: "Focus",
  meeting: "Meeting",
  ai: "AI",
  other: "Other",
};

/** The state of the one read a dialog cannot own: `ready` draws the list, the other
 *  two say why it is not there yet instead of drawing an empty one. */
export type OverrideListState = "ready" | "loading" | "error";

/** The thread being folded away. `sessionCount` is the figure the confirmation states. */
export interface ThreadOverrideSource {
  id: number;
  title: string;
  sessionCount: number;
}

/** A candidate survivor: the rows the caller already read, ordered as it read them. */
export interface ThreadOverrideSurvivor {
  id: number;
  title: string;
  session_count: number;
}

export interface MergeThreadDialogProps {
  open: boolean;
  source: ThreadOverrideSource;
  /** Every row the caller holds; the dialog drops the source itself. */
  survivors: ThreadOverrideSurvivor[];
  onClose: () => void;
  /** Runs after the command succeeds and before the dialog closes, for a page that
   *  wants to follow up — the rows themselves re-read through the invalidation. */
  onMerged?: () => void;
  /** Defaults to `ready`: a caller whose rows are already on screen has nothing to say. */
  survivorsState?: OverrideListState;
  onRetrySurvivors?: () => void;
}

/**
 * "3 sessions move from A to B" — the sentence the merge dialog has to state, with
 * the blank filled while no survivor is picked yet (`§9`: say what happens, plainly).
 */
function mergeSentence(
  source: ThreadOverrideSource,
  survivor: ThreadOverrideSurvivor | null,
): string {
  return `${formatCount(source.sessionCount, "session")} ${
    source.sessionCount === 1 ? "moves" : "move"
  } from ${source.title} to ${survivor ? survivor.title : "the thread you pick"}`;
}

/**
 * Merge confirm (`03 §7h.5`): names the survivor, counts the sessions that move and
 * states both ends of the fold. Merging rewrites the grouping the user is reading, so
 * the confirmation is the only way the override is written.
 */
export function MergeThreadDialog({
  open,
  source,
  survivors,
  onClose,
  onMerged,
  survivorsState = "ready",
  onRetrySurvivors,
}: MergeThreadDialogProps) {
  const merge = useMergeThreads();
  const [target, setTarget] = useState("");
  const others = survivors.filter((thread) => thread.id !== source.id);
  const survivor = others.find((thread) => String(thread.id) === target) ?? null;

  const options: SelectOption[] = [
    { value: "", label: "Pick the thread that keeps its name" },
    ...others.map((thread) => ({
      value: String(thread.id),
      label: `${thread.title} · ${formatCount(thread.session_count, "session")}`,
    })),
  ];

  const confirm = () => {
    if (!survivor) return;
    merge.mutate(
      { source_thread_id: source.id, target_thread_id: survivor.id },
      {
        onSuccess: () => {
          toast.success("Threads merged");
          onMerged?.();
          onClose();
        },
      },
    );
  };

  let picker: ReactNode;
  if (survivorsState === "loading") {
    picker = (
      <div className="flex flex-col gap-2">
        <p role="status" className="m-0 text-t-sm text-fg-secondary">
          Loading the other threads…
        </p>
        <Skeleton h="control" w="full" />
      </div>
    );
  } else if (survivorsState === "error") {
    picker = (
      <ErrorState
        align="start"
        title="Could not load the other threads"
        message="Retry to pick a survivor; nothing has been merged."
        onRetry={onRetrySurvivors}
      />
    );
  } else {
    picker = (
      <Select
        label="Survivor"
        value={target}
        options={options}
        disabled={others.length === 0}
        hint={
          others.length === 0
            ? "There is no other thread to merge into yet."
            : undefined
        }
        onChange={(event) => setTarget(event.target.value)}
      />
    );
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Merge this thread into another"
      description={mergeSentence(source, survivor)}
      actions={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={confirm}
            disabled={!survivor}
            loading={merge.isPending}
          >
            Merge threads
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="m-0 text-t-sm text-fg-secondary">
          Every session of “{source.title}” moves to the thread you pick, and the emptied
          thread stops appearing in Work while the merge stays on record. This choice beats
          the clusterer from then on.
        </p>
        {picker}
        {merge.isError && (
          <p role="status" className="m-0 text-t-sm text-status-danger-fg">
            Could not merge the threads: {String(merge.error)}
          </p>
        )}
      </div>
    </Dialog>
  );
}

export interface SplitThreadDialogProps {
  open: boolean;
  thread: { id: number; title: string };
  /** This thread's member sessions, as its own read returns them. */
  sessions: ThreadSessionRef[];
  onClose: () => void;
  /** Runs after the command succeeds and before the dialog closes. */
  onSplit?: () => void;
  sessionsState?: OverrideListState;
  onRetrySessions?: () => void;
}

/**
 * Split picker (`03 §7h.5`): the thread's own sessions, and at least one has to be
 * picked before the override can be written — the confirmation is the only write.
 */
export function SplitThreadDialog({
  open,
  thread,
  sessions,
  onClose,
  onSplit,
  sessionsState = "ready",
  onRetrySessions,
}: SplitThreadDialogProps) {
  const split = useSplitThread();
  const [chosen, setChosen] = useState<number[]>([]);

  const confirm = () => {
    if (chosen.length === 0) return;
    split.mutate(
      { thread_id: thread.id, session_ids: chosen },
      {
        onSuccess: () => {
          toast.success("Sessions moved into a new thread");
          onSplit?.();
          onClose();
        },
      },
    );
  };

  let body: React.ReactNode;
  if (sessionsState === "loading") {
    body = (
      <div className="flex flex-col gap-2">
        <p role="status" className="m-0 text-t-sm text-fg-secondary">
          Loading this thread’s sessions…
        </p>
        {[0, 1, 2].map((row) => (
          <Skeleton key={row} h="control" w="full" />
        ))}
      </div>
    );
  } else if (sessionsState === "error") {
    body = (
      <ErrorState
        align="start"
        title="Could not load this thread’s sessions"
        message="Retry to read them; nothing has been moved."
        onRetry={onRetrySessions}
      />
    );
  } else if (sessions.length === 0) {
    body = (
      <p className="m-0 text-t-sm text-fg-secondary">
        This thread has no sessions to move.
      </p>
    );
  } else {
    body = (
      <ul className="m-0 flex list-none flex-col gap-2 p-0">
        {sessions.map((session) => (
          <li key={session.session_id}>
            <Checkbox
              label={session.title?.trim() || "Untitled session"}
              hint={`${SESSION_KIND_LABEL[session.kind] ?? session.kind} · ${relativeTime(
                session.started_at,
              )}`}
              checked={chosen.includes(session.session_id)}
              onChange={(next) =>
                setChosen((current) =>
                  next
                    ? [...current, session.session_id]
                    : current.filter((id) => id !== session.session_id),
                )
              }
            />
          </li>
        ))}
      </ul>
    );
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Split sessions into a new thread"
      description={`Move the sessions you pick out of ${thread.title} into a thread of their own.`}
      actions={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={confirm}
            disabled={chosen.length === 0}
            loading={split.isPending}
          >
            {chosen.length === 0
              ? "Split sessions"
              : `Split ${formatCount(chosen.length, "session")}`}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {body}
        {split.isError && (
          <p role="status" className="m-0 text-t-sm text-status-danger-fg">
            Could not split the thread: {String(split.error)}
          </p>
        )}
      </div>
    </Dialog>
  );
}
