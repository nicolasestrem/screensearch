// FrameTile — a clickable thumbnail card for a FrameMeta (deck recents, moment
// neighbours). Opens the moment at /timeline/:id. Time is shown relative on the
// face with the absolute value on hover (UI_REFERENCE §9); the foreground app, when
// known, labels the capture. Tokens only; the whole tile is one ≥32px hit target.
import { Link } from "react-router-dom";

import type { FrameMeta } from "../../bindings/FrameMeta";
import { FrameImage } from "./FrameImage";
import { absoluteTime, clockTime } from "../../lib/time";
import { cn } from "../../lib/cn";

export interface FrameTileProps {
  frame: FrameMeta;
  className?: string;
  /** Optional router state for round-trips such as Session → Moment → Session. */
  navigationState?: unknown;
}

export function FrameTile({
  frame,
  className,
  navigationState,
}: FrameTileProps) {
  return (
    <Link
      to={`/timeline/${frame.frame_id}`}
      state={navigationState}
      title={absoluteTime(frame.captured_at)}
      className={cn(
        "group flex flex-col gap-2 rounded-md border border-line-subtle bg-surface-pane p-2",
        "transition-colors duration-fast ease-standard hover:border-line-strong",
        className,
      )}
    >
      <FrameImage
        imagePath={frame.image_path}
        imagePurged={frame.image_purged}
        alt={`Capture at ${clockTime(frame.captured_at)}`}
        className="aspect-video w-full rounded-sm object-cover bg-surface-raised"
      />
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-t-data text-fg-secondary">
          {clockTime(frame.captured_at)}
        </span>
        {frame.app_hint && (
          <span className="truncate text-t-xs text-fg-tertiary">
            {frame.app_hint}
          </span>
        )}
      </div>
    </Link>
  );
}
