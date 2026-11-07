// AnswerStream — renders the streamed `ask` answer (UI_REFERENCE §4 Recall/ask).
// Folds the useAsk view-model into: a collapsible *thinking* trace (native
// <details>, so keyboard + a11y come free), the answer prose (rendered through the shared
// `AnswerProse` markdown renderer, themed with the `prose-deck` typography tokens), and the
// reviewed source-frame context as clickable tiles that open each source moment. The
// markdown setup lives in `answerProseMarkdown.ts`, so the Ask v2 conversation renders an
// answer the same way this stream does instead of keeping a second copy. Markdown still only
// reaches this component through a route-split module (§8), so it stays out of the entry chunk.
// The idle phase renders nothing — the Recall screen owns the empty "ask a question"
// invitation.
import { useEffect, useRef, useState } from "react";

import type { AskPhase } from "../../lib/ipc/useAsk";
import { AnswerProse } from "./AnswerProse";
import { ANSWER_PROSE_TYPOGRAPHY } from "./answerProseMarkdown";
import { CitationTile } from "./CitationTile";
import { ErrorState } from "../primitives";

/** Nearest scrollable ancestor of `el` (the shell `<main>` in the Recall route, the
 *  overlay's own pane in the Flow overlay) — the element to follow while the reasoning
 *  trace streams, now that the trace grows inline instead of scrolling itself (D9, #59). */
function nearestScrollable(el: HTMLElement): HTMLElement | null {
  for (let n = el.parentElement; n; n = n.parentElement) {
    const oy = getComputedStyle(n).overflowY;
    if (oy === "auto" || oy === "scroll") {
      return n;
    }
  }
  return null;
}

export interface AnswerStreamProps {
  phase: AskPhase;
  thinking: string;
  answer: string;
  /** Frame ids supplied to the answer model as reviewed context. */
  citations: number[];
  error: string | null;
  /** Re-run the last question (shown on error). */
  onRetry?: () => void;
  /** Router-free opener used by both the main Recall route and the Flow overlay. */
  onOpenFrame: (frameId: number) => void;
}

export function AnswerStream({
  phase,
  thinking,
  answer,
  citations,
  error,
  onRetry,
  onOpenFrame,
}: AnswerStreamProps) {
  const streaming = phase === "streaming";

  // The thinking trace is a *controlled* <details>: expanded while a new answer
  // streams in, then left as the user last set it. Tying `open` straight to
  // `streaming` would yank the panel shut the instant streaming finished — exactly
  // when the user may want to read it. Hooks run unconditionally, before any early
  // return (Rules-of-Hooks gate).
  const [thinkingOpen, setThinkingOpen] = useState(true);
  const wasStreaming = useRef(false);
  useEffect(() => {
    if (streaming && !wasStreaming.current) setThinkingOpen(true);
    wasStreaming.current = streaming;
  }, [streaming]);

  // Auto-follow the stream: while it arrives, keep the newest content in view so it doesn't
  // scroll off the bottom — but only when the user is already near the bottom, so scrolling
  // up to re-read isn't yanked back down. Anchored to a sentinel at the very end of the
  // stream, so it follows BOTH the reasoning trace and the answer prose below it (the trace
  // now grows inline with no inner scroll, #59, so a long trace would otherwise pin the user
  // at its end while the answer streams past the fold). The nearest scrollable ancestor is
  // the shell <main> in the route, the overlay's own pane in the Flow overlay.
  const streamEndRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const el = streamEndRef.current;
    if (!el || !streaming) {
      scrollerRef.current = null;
      return;
    }
    // Resolve the scroller once per stream, not per token: this effect fires on every
    // streamed token and nearestScrollable walks ancestors with getComputedStyle. The
    // scrollable ancestor is stable while a stream runs, so cache it (reset when streaming ends).
    if (!scrollerRef.current) scrollerRef.current = nearestScrollable(el);
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const nearBottom =
      scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 48;
    if (nearBottom) scroller.scrollTop = scroller.scrollHeight;
  }, [thinking, answer, streaming]);

  // When the answer finishes streaming, move focus to the answer region so keyboard
  // users land on the result instead of being stranded at the query input, and screen
  // readers announce it (the prose isn't aria-live — that would read every streamed
  // token). It's a non-interactive region focused programmatically, so the visible ring
  // is suppressed; the answer prose is itself the obvious result.
  const answerRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (phase === "done") answerRef.current?.focus();
  }, [phase]);

  if (phase === "idle") return null;

  if (phase === "error") {
    return (
      <ErrorState
        title="Couldn't answer that"
        message={
          error ??
          "The answer model is unavailable. Make sure the inference sidecar is loaded, then try again."
        }
        onRetry={onRetry}
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {thinking && (
        <details
          open={thinkingOpen}
          onToggle={(e) =>
            setThinkingOpen((e.currentTarget as HTMLDetailsElement).open)
          }
          className="rounded-md border border-line-subtle bg-surface-app"
        >
          <summary className="cursor-pointer select-none px-3 py-2 text-t-xs text-fg-secondary">
            Thinking
          </summary>
          <pre className="whitespace-pre-wrap break-words px-3 pb-3 text-t-xs text-fg-tertiary">
            {thinking}
          </pre>
        </details>
      )}

      <div
        ref={answerRef}
        tabIndex={-1}
        role="region"
        aria-label="Answer"
        className={`${ANSWER_PROSE_TYPOGRAPHY} max-w-none focus-visible:outline-none`}
      >
        <AnswerProse text={answer} />
        {streaming && (
          <span
            className="ml-0.5 inline-block h-4 w-2 bg-accent align-text-bottom"
            aria-label="Answering…"
          />
        )}
      </div>

      {citations.length > 0 && (
        <div className="flex flex-col gap-2">
          <span className="text-t-xs font-medium text-fg-secondary">
            Frames checked
          </span>
          {/* Wrap onto rows instead of a nested horizontal scroller (D9 — one scroll
              context; Windows draws a permanent scrollbar for overflow-x-auto). */}
          <div className="flex flex-wrap gap-2">
            {citations.map((id) => (
              <CitationTile key={id} frameId={id} onOpenFrame={onOpenFrame} />
            ))}
          </div>
        </div>
      )}

      {/* Follow anchor for the auto-scroll effect above — tracks the true bottom of the stream. */}
      <div ref={streamEndRef} aria-hidden="true" />
    </div>
  );
}
