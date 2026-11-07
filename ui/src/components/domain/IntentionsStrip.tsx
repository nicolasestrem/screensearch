// Deck "Intentions" strip (0.3.0 PR6; UI_REFERENCE §5): unresolved marks, newest-first.
// Each row: thumbnail (or "text kept" reconstruction via FrameImage), the note (or the
// frame's title/app as a fallback label), and its age. Actions: Open the Moment, Done, or
// Dismiss (resolve-with-no-action — the same op). NO badge counts anywhere (D14). Five
// states; tokens only. Navigation is the caller's (onOpen); resolve goes through the hook.
import { Button, EmptyState, ErrorState, Panel, Skeleton } from "../primitives";
import { FrameImage } from "./FrameImage";
import { DEFAULT_MARKS_HOTKEY } from "./HotkeyField";
import { IconMark } from "../icons";
import { useMarks, useSettings } from "../../lib/ipc/queries";
import { useResolveMark } from "../../lib/ipc/mutations";
import { relativeTime } from "../../lib/time";
import type { Mark } from "../../bindings/Mark";

export interface IntentionsStripProps {
  /** Open a marked frame's Moment (Deck wires this to `/timeline/:id`). */
  onOpen: (frameId: number) => void;
}

function markLabel(mark: Mark): string {
  return mark.note ?? mark.window_title ?? mark.app_hint ?? "Marked moment";
}

export function IntentionsStrip({ onOpen }: IntentionsStripProps) {
  const marks = useMarks();
  const settings = useSettings();
  const resolve = useResolveMark();
  const markHotkey = settings.data?.marks_hotkey ?? DEFAULT_MARKS_HOTKEY;

  const unresolved = (marks.data ?? []).filter((m) => m.resolved_at == null);

  return (
    <Panel title="Intentions">
      {marks.isLoading ? (
        <div className="flex flex-col gap-2">
          {Array.from({ length: 2 }, (_, i) => (
            <Skeleton key={i} className="h-16 w-full" />
          ))}
        </div>
      ) : marks.isError ? (
        <ErrorState
          title="Couldn't load marks"
          message={String(marks.error)}
          onRetry={() => marks.refetch()}
        />
      ) : unresolved.length === 0 ? (
        <EmptyState
          icon={<IconMark size={24} />}
          title="No open intentions"
          description={`Press ${markHotkey} to mark a moment you mean to come back to.`}
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {unresolved.map((mark) => {
            // Scope the disabled state to the row actually resolving: `variables` holds
            // the mark_id passed to the in-flight `mutate`, so other rows stay live.
            const rowPending =
              resolve.isPending && resolve.variables === mark.mark_id;
            return (
              <li
                key={mark.mark_id}
                className="flex items-center gap-3 rounded-sm border border-line-subtle bg-surface-pane p-2"
              >
                <FrameImage
                  imagePath={mark.image_path}
                  imagePurged={mark.image_purged}
                  alt=""
                  className="h-14 w-24 shrink-0 rounded-sm object-cover bg-surface-raised"
                />
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="truncate text-t-body text-fg-primary">
                    {markLabel(mark)}
                  </span>
                  <span className="font-mono text-t-data text-fg-tertiary">
                    {relativeTime(mark.created_at)}
                  </span>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => onOpen(mark.frame_id)}
                  >
                    Open
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => resolve.mutate(mark.mark_id)}
                    disabled={rowPending}
                  >
                    Done
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => resolve.mutate(mark.mark_id)}
                    disabled={rowPending}
                  >
                    Dismiss
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}
