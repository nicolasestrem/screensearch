// Deck "Jump back" card (0.3.0 PR6; UI_REFERENCE §5): the last sustained context before
// the current detour, opening the Moment. Additive to the existing "Jump back in" recents
// grid. Self-contained Panel with all five states; tokens only. Navigation is the caller's
// (onOpen) so this stays router-agnostic like the rest of the domain layer.
import { EmptyState, ErrorState, Panel, Skeleton } from "../primitives";
import { FrameImage } from "./FrameImage";
import { IconMark } from "../icons";
import { useWhereWasI } from "../../lib/ipc/queries";
import { clockTime } from "../../lib/time";
import type { UiResumeContext } from "../../bindings/UiResumeContext";

export interface WhereWasICardProps {
  /** Open the resumed context's Moment (Deck wires this to `/timeline/:id`). */
  onOpen: (frameId: number) => void;
}

function headline(ctx: UiResumeContext): string {
  return ctx.window_title ? `${ctx.window_title} — ${ctx.app}` : ctx.app;
}

export function WhereWasICard({ onOpen }: WhereWasICardProps) {
  const resume = useWhereWasI();

  return (
    <Panel title="Where was I?">
      {resume.isLoading ? (
        <div className="flex items-center gap-3">
          <Skeleton className="h-16 w-28 shrink-0" />
          <div className="flex flex-1 flex-col gap-2">
            <Skeleton className="h-5 w-2/3" />
            <Skeleton className="h-4 w-1/3" />
          </div>
        </div>
      ) : resume.isError ? (
        <ErrorState
          title="Couldn't check where you were"
          message={String(resume.error)}
          onRetry={() => resume.refetch()}
        />
      ) : !resume.data ? (
        <EmptyState
          icon={<IconMark size={24} />}
          title="Nothing to resume yet"
          description="After a focused stretch of work, a browser detour, and a return, the last sustained context shows up here."
        />
      ) : (
        <button
          type="button"
          onClick={() => onOpen(resume.data!.frame_id)}
          className="flex w-full items-center gap-3 rounded-sm border border-line-subtle bg-surface-pane p-3 text-left transition-colors duration-fast ease-standard hover:border-accent"
        >
          <FrameImage
            imagePath={resume.data.image_path}
            imagePurged={resume.data.image_purged}
            alt=""
            className="h-16 w-28 shrink-0 rounded-sm object-cover bg-surface-raised"
          />
          <span className="flex min-w-0 flex-1 flex-col gap-1">
            <span className="text-t-xs font-medium text-fg-tertiary">
              Jump back
            </span>
            <span className="truncate text-t-body text-fg-primary">
              {headline(resume.data)}
            </span>
            <span className="font-mono text-t-data text-fg-tertiary">
              until {clockTime(resume.data.span_end)}
            </span>
            {resume.data.session ? (
              <span className="font-mono text-t-data text-fg-secondary">
                session {clockTime(resume.data.session.started_at)}–
                {resume.data.session.ended_at
                  ? clockTime(resume.data.session.ended_at)
                  : "still running"}
              </span>
            ) : null}
          </span>
        </button>
      )}
    </Panel>
  );
}
