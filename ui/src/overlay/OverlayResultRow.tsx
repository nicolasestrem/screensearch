import type { SearchHit } from "../bindings/SearchHit";
import { FrameImage } from "../components/domain/FrameImage";
import { HighlightedSnippet } from "../components/domain/HighlightedSnippet";
import { cn } from "../lib/cn";
import { absoluteTime, relativeTime } from "../lib/time";

export interface OverlayResultRowProps {
  hit: SearchHit;
  id: string;
  selected: boolean;
  onOpen: () => void;
  onHover: () => void;
}

export function OverlayResultRow({
  hit,
  id,
  selected,
  onOpen,
  onHover,
}: OverlayResultRowProps) {
  return (
    <button
      id={id}
      type="button"
      role="option"
      aria-selected={selected}
      title={absoluteTime(hit.captured_at)}
      onClick={onOpen}
      onMouseEnter={onHover}
      className={cn(
        "flex w-full items-center gap-3 rounded-sm border p-2 text-left",
        "motion-safe:transition-colors duration-fast ease-standard",
        selected
          ? "border-accent bg-accent-wash"
          : "border-line-subtle bg-surface-raised hover:border-line-strong hover:bg-surface-hover",
      )}
    >
      <FrameImage
        imagePath={hit.image_path}
        imagePurged={hit.image_purged}
        alt=""
        className="h-14 w-24 shrink-0 rounded-sm bg-surface-inset object-cover"
      />
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="line-clamp-2 text-t-body text-fg-primary">
          {hit.snippet ? (
            <HighlightedSnippet text={hit.snippet} />
          ) : (
            <span className="text-fg-tertiary">No text recognized</span>
          )}
        </span>
        <span className="flex min-w-0 items-center gap-2 text-t-xs text-fg-tertiary">
          <span className="font-mono text-t-data">
            {relativeTime(hit.captured_at)}
          </span>
          {hit.app_hint && (
            <>
              <span aria-hidden="true">.</span>
              <span className="truncate">{hit.app_hint}</span>
            </>
          )}
        </span>
      </span>
    </button>
  );
}
