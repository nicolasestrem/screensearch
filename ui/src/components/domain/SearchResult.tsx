// SearchResult — one hybrid-search hit as a list row: a small thumbnail, the
// matched OCR snippet, and capture context (UI_REFERENCE §5). Opens the moment at
// /timeline/:id. The FTS arm wraps matched terms in `[...]` (see store::search's
// `snippet(...)`); we render those segments in the accent so the match is visible
// without a second highlight pass. Vector-only hits carry a plain snippet (no
// brackets) and render as-is.
import { Link } from "react-router-dom";

import type { SearchHit } from "../../bindings/SearchHit";
import { FrameImage } from "./FrameImage";
import { HighlightedSnippet } from "./HighlightedSnippet";
import { absoluteTime, relativeTime } from "../../lib/time";
export { HighlightedSnippet } from "./HighlightedSnippet";

export interface SearchResultProps {
  hit: SearchHit;
  onOpenFrame?: (frameId: number) => void;
}

export function SearchResult({ hit, onOpenFrame }: SearchResultProps) {
  const className =
    "flex gap-3 rounded-md border border-line-subtle bg-surface-pane p-3 transition-colors duration-fast ease-standard hover:border-line-strong";
  const body = (
    <>
      <FrameImage
        imagePath={hit.image_path}
        imagePurged={hit.image_purged}
        alt=""
        className="h-16 w-28 shrink-0 rounded-sm object-cover bg-surface-raised"
      />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p className="line-clamp-2 text-t-body text-fg-primary">
          {hit.snippet ? (
            <HighlightedSnippet text={hit.snippet} />
          ) : (
            <span className="text-fg-tertiary">No text recognized</span>
          )}
        </p>
        <div className="flex items-center gap-2 text-t-xs text-fg-tertiary">
          <span className="font-mono text-t-data">
            {relativeTime(hit.captured_at)}
          </span>
          {hit.app_hint && (
            <>
              <span aria-hidden="true">·</span>
              <span className="truncate">{hit.app_hint}</span>
            </>
          )}
        </div>
      </div>
    </>
  );

  if (onOpenFrame) {
    return (
      <button
        type="button"
        title={absoluteTime(hit.captured_at)}
        onClick={() => onOpenFrame(hit.frame_id)}
        className={`${className} w-full text-left`}
      >
        {body}
      </button>
    );
  }

  return (
    <Link
      to={`/timeline/${hit.frame_id}`}
      title={absoluteTime(hit.captured_at)}
      className={className}
    >
      {body}
    </Link>
  );
}
