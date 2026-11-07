// Recall (/recall) — hybrid search + grounded Ask + recall Reports (UI_REFERENCE §3/§4).
// One screen, three modes. Search lists hybrid hits (virtualized — no full-list DOM, §8);
// Ask streams a grounded answer with collapsible thinking, premade prompt cards, and
// citation tiles; Reports builds a daily/weekly/custom summary over content text with
// live progress, source-frame chips, copy + .md download. Every state is explicit:
// search → invite / loading / no-match / error / results; ask → invite(+cards) /
// streaming / done / error; reports → invite / generating / done / error. A banner
// flags degraded modes. Never a zero-result dead end.
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { useNavigate } from "react-router-dom";
import { useVirtualizer, type Virtualizer } from "@tanstack/react-virtual";

import { useShellScrollContainer } from "../components/shell/ScrollContainerContext";

import {
  Button,
  Chip,
  EmptyState,
  ErrorState,
  Skeleton,
} from "../components/primitives";
import {
  SearchResult,
  AnswerStream,
  PromptCardGrid,
  ReportBuilder,
  ReportView,
} from "../components/domain";
import { IconRecall, IconSparkle, IconInsights } from "../components/icons";
import { useReadiness, useSearch, useSettings } from "../lib/ipc/queries";
import { useAsk } from "../lib/ipc/useAsk";
import { useReport } from "../lib/ipc/useReport";
import { useUiStore } from "../state/uiStore";
import { cn } from "../lib/cn";
import type { SearchQuery } from "../bindings/SearchQuery";
import type { SearchHit } from "../bindings/SearchHit";

type Mode = "search" | "ask" | "reports";

const SEARCH_LIMIT = 100;
const SEARCH_DEBOUNCE_MS = 250;
// Generation budget (n_predict) for an Ask reply. The default answer models are *reasoning*
// models that emit a `<think>…</think>` trace before the answer; 512 was exhausted mid-thought,
// so the answer was truncated to nothing (only the Thinking box showed). 2048 leaves room to
// finish reasoning *and* produce the answer while still preserving most of the 8K context
// window for retrieved snippets (`answer.rs::build_messages` reserves this from the budget).
const ASK_MAX_TOKENS = 2048;
const ROW_ESTIMATE = 104;

const MODES: { value: Mode; label: string; icon: ReactNode }[] = [
  { value: "search", label: "Search", icon: <IconRecall size={16} /> },
  { value: "ask", label: "Ask", icon: <IconSparkle size={16} /> },
  { value: "reports", label: "Reports", icon: <IconInsights size={16} /> },
];

export function Component() {
  const navigate = useNavigate();
  const [mode, setMode] = useState<Mode>("search");
  const [text, setText] = useState("");
  const [debounced, setDebounced] = useState("");
  const selectedRange = useUiStore((s) => s.selectedRange);

  const readiness = useReadiness();
  const settings = useSettings();
  const ask = useAsk();
  const report = useReport();
  const openFrame = useCallback(
    (frameId: number) => {
      navigate(`/timeline/${frameId}`);
    },
    [navigate],
  );

  // Content-text (default) vs raw/app-chrome search (03 §3b). `null` follows the
  // user's configured default (`text.include_chrome_default`) until they toggle it.
  const [chromeOverride, setChromeOverride] = useState<boolean | null>(null);
  const includeChrome =
    chromeOverride ?? settings.data?.text_include_chrome_default ?? false;

  // Debounce keystrokes into the committed search term (live search, bounded).
  useEffect(() => {
    const t = setTimeout(() => setDebounced(text), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [text]);

  const query: SearchQuery = useMemo(
    // Default retrieval is cleaned content text; `include_chrome` also searches raw /
    // app-chrome text so suppressed static terms are still findable (03 §3b).
    () => ({
      text: debounced,
      limit: SEARCH_LIMIT,
      time_range: selectedRange,
      include_chrome: includeChrome,
    }),
    [debounced, selectedRange, includeChrome],
  );
  const search = useSearch(query, mode === "search");

  const hits = mode === "search" ? (search.data ?? []) : [];
  // Virtualize against the shell <main> scroll container, not a nested pane, so Recall
  // keeps one scroll context per route (UI_REFERENCE §8, D9). `scrollMargin` is the
  // offset of the list's start within <main> (its `offsetTop`, since <main> is the
  // positioned offsetParent); re-measured whenever the height above the list changes
  // (mode switch, header wrap at narrow widths) so the rows never drift.
  const mainRef = useShellScrollContainer();
  const listWrapRef = useRef<HTMLDivElement>(null);
  const [listOffset, setListOffset] = useState(0);
  useLayoutEffect(() => {
    const el = listWrapRef.current;
    const scroller = mainRef?.current;
    if (!el || !scroller) return;
    const measure = () => setListOffset(el.offsetTop);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(scroller);
    if (el.parentElement) ro.observe(el.parentElement);
    return () => ro.disconnect();
  }, [mode, mainRef]);

  const virtualizer = useVirtualizer({
    count: hits.length,
    getScrollElement: () => mainRef?.current ?? null,
    estimateSize: () => ROW_ESTIMATE,
    overscan: 8,
    scrollMargin: listOffset,
  });

  // Fill + submit an Ask question (shared by the form and the premade cards). The
  // per-request `top_k` is left null → the configured `retrieval.default_top_k`.
  const askQuery = (q: string) => {
    const question = q.trim();
    if (!question) return;
    ask.ask({
      query: question,
      thinking: settings.data?.answer_thinking ?? true,
      max_tokens: ASK_MAX_TOKENS,
      top_k: null,
    });
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (mode === "ask") {
      askQuery(text);
    } else {
      setDebounced(text); // commit immediately on Enter
    }
  };

  const switchMode = (next: Mode) => {
    setMode(next);
    if (next === "ask") ask.reset();
  };

  const embedReady = readiness.data?.embed_model.status === "ready";
  const sidecarStatus = readiness.data?.sidecar.status;
  const sidecarDown =
    sidecarStatus === "unavailable" || sidecarStatus === "error";

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col p-6">
      {/* Sticky header — the mode toggle + query input stay put while results scroll,
          so the page keeps a single scroll context (the shell <main>, UI_REFERENCE §8).
          `pb-4` makes its opaque background butt flush against the content below, so
          scrolling rows are fully hidden behind it (no peek-through seam). */}
      <div className="sticky top-0 z-rail flex flex-col gap-4 bg-surface-app pb-4">
        {/* Mode toggle. */}
        <div className="flex gap-1" role="tablist" aria-label="Recall mode">
          {MODES.map((m) => (
            <button
              key={m.value}
              type="button"
              role="tab"
              aria-selected={mode === m.value}
              onClick={() => switchMode(m.value)}
              className={cn(
                "inline-flex items-center gap-2 rounded-sm px-3 min-h-hit-min text-t-xs font-semibold",
                "transition-colors duration-fast ease-standard",
                mode === m.value
                  ? "bg-accent-wash text-accent"
                  : "text-fg-secondary hover:text-fg-primary hover:bg-surface-hover",
              )}
            >
              {m.icon}
              {m.label}
            </button>
          ))}
        </div>

        {/* Query input (search + ask). Reports has its own range builder instead. */}
        {mode !== "reports" && (
          <form onSubmit={submit} className="flex gap-2">
            <input
              type="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              autoFocus
              placeholder={
                mode === "search"
                  ? "Search your screen history…"
                  : "Ask a question about what you've seen…"
              }
              aria-label={mode === "search" ? "Search query" : "Question"}
              className="min-w-0 flex-1 rounded-sm border border-line-control bg-surface-inset px-3 min-h-hit-min text-t-body text-fg-primary placeholder:text-fg-tertiary transition-colors duration-fast ease-standard focus:border-accent"
            />
            <Button
              type="submit"
              variant="primary"
              disabled={mode === "ask" && ask.phase === "streaming"}
            >
              {mode === "search"
                ? "Search"
                : ask.phase === "streaming"
                  ? "Asking…"
                  : "Ask"}
            </Button>
          </form>
        )}
      </div>

      {/* Non-sticky sections scroll under the sticky header as one column. */}
      <div className="flex flex-col gap-4">
        {/* Reports range builder. */}
        {mode === "reports" && (
          <ReportBuilder
            busy={report.phase === "generating"}
            onCancel={report.cancel}
            onGenerate={report.generate}
          />
        )}

        {/* Content vs raw/app-chrome retrieval (search mode only, 03 §3b). */}
        {mode === "search" && (
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              role="switch"
              aria-checked={includeChrome}
              onClick={() => setChromeOverride(!includeChrome)}
              className={cn(
                "inline-flex items-center gap-2 rounded-sm border px-3 min-h-hit-min text-t-xs",
                "transition-colors duration-fast ease-standard",
                includeChrome
                  ? "border-accent text-accent bg-accent-wash"
                  : "border-line-control text-fg-secondary hover:text-fg-primary",
              )}
            >
              {includeChrome
                ? "Including app chrome + raw text"
                : "Content text only"}
            </button>
            <span className="text-t-xs text-fg-tertiary">
              {includeChrome
                ? "Searching everything on screen, including toolbars and labels."
                : "Ignoring static toolbars, taskbars, and repeated labels."}
            </span>
          </div>
        )}

        {/* Degraded-mode status (truthful, not blocking). A permanently-reserved row so
            the chip appears in place and never pushes the body down (D9 no-CLS; only the
            readiness banner may reserve, UI_REFERENCE §8). */}
        <div role="status" aria-live="polite" className="flex min-h-8 items-center">
          {mode === "search" && readiness.data && !embedReady && (
            <Chip tone="warn">
              Searching text only — semantic search lights up once the embedding
              model loads
            </Chip>
          )}
          {mode !== "search" && sidecarDown && (
            <Chip tone="warn">
              Answer model not loaded
              {readiness.data?.sidecar.detail
                ? ` — ${readiness.data.sidecar.detail}`
                : ""}
            </Chip>
          )}
        </div>

        {/* Body grows inline; the shell <main> is the one scroll context (D9). */}
        <div ref={listWrapRef}>
          {mode === "search" ? (
            <SearchBody
              query={query}
              isFetching={search.isFetching}
              isError={search.isError}
              error={search.error}
              onRetry={() => search.refetch()}
              hits={hits}
              virtualizer={virtualizer}
            />
          ) : mode === "ask" ? (
            ask.phase === "idle" ? (
              <div className="flex flex-col gap-6">
                <EmptyState
                  icon={<IconSparkle size={28} />}
                  title="Ask about what you've seen"
                  description="Questions are answered from your captured screens, with the source frames cited. Try a card below, or ask your own."
                />
                <PromptCardGrid
                  onPick={(p) => {
                    setText(p);
                    askQuery(p);
                  }}
                />
              </div>
            ) : (
              <AnswerStream
                phase={ask.phase}
                thinking={ask.thinking}
                answer={ask.answer}
                citations={ask.citations}
                error={ask.error}
                onRetry={() => askQuery(text)}
                onOpenFrame={openFrame}
              />
            )
          ) : (
            <ReportBody
              phase={report.phase}
              progress={report.progress}
              result={report.result}
              request={report.request}
              error={report.error}
              onOpenFrame={openFrame}
            />
          )}
        </div>
      </div>
    </div>
  );
}

interface ReportBodyProps {
  phase: ReturnType<typeof useReport>["phase"];
  progress: ReturnType<typeof useReport>["progress"];
  result: ReturnType<typeof useReport>["result"];
  request: ReturnType<typeof useReport>["request"];
  error: string | null;
  onOpenFrame: (frameId: number) => void;
}

function ReportBody({
  phase,
  progress,
  result,
  request,
  error,
  onOpenFrame,
}: ReportBodyProps) {
  if (phase === "idle") {
    return (
      <EmptyState
        icon={<IconInsights size={28} />}
        title="Build a recall report"
        description="Pick a range above and Generate. Reports summarize your captured content — not toolbars or app chrome — and cite the frames behind every point."
      />
    );
  }
  if (phase === "error") {
    return (
      <ErrorState
        title="Couldn't build that report"
        message={
          error ??
          "The answer model is unavailable. Make sure the inference sidecar is loaded, then try again."
        }
      />
    );
  }
  if (phase === "generating") {
    return (
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2 text-t-body text-fg-secondary">
          <span
            className="inline-block h-3 w-3 rounded-full bg-accent"
            aria-hidden
          />
          {progress ? progress.stage : "Starting…"}
          {progress && progress.total > 0
            ? ` (${progress.done}/${progress.total})`
            : ""}
        </div>
        <span className="text-t-xs text-fg-tertiary">
          Reports summarize active periods in bounded passes, so larger ranges
          can take a little longer.
        </span>
      </div>
    );
  }
  // done
  return result ? (
    <ReportView report={result} request={request} onOpenFrame={onOpenFrame} />
  ) : null;
}

interface SearchBodyProps {
  query: SearchQuery;
  isFetching: boolean;
  isError: boolean;
  error: unknown;
  onRetry: () => void;
  hits: SearchHit[];
  virtualizer: Virtualizer<HTMLElement, Element>;
}

function SearchBody({
  query,
  isFetching,
  isError,
  error,
  onRetry,
  hits,
  virtualizer,
}: SearchBodyProps) {
  // Invite: nothing typed yet.
  if (query.text.trim().length === 0) {
    return (
      <EmptyState
        icon={<IconRecall size={28} />}
        title="Search your screen history"
        description="Find any moment by the text on screen or what it was about. Results link straight to the captured frame."
      />
    );
  }
  if (isError) {
    return (
      <ErrorState
        title="Search failed"
        message={String(error)}
        onRetry={onRetry}
      />
    );
  }
  if (isFetching && hits.length === 0) {
    return (
      <div className="flex flex-col gap-3">
        {Array.from({ length: 5 }, (_, i) => (
          <Skeleton key={i} className="h-24 w-full" />
        ))}
      </div>
    );
  }
  if (hits.length === 0) {
    return (
      <EmptyState
        icon={<IconRecall size={28} />}
        title="No matches"
        description="Try different words, or widen the time range from the timeline."
      />
    );
  }

  // Rows are positioned inside this relative box, whose top sits `scrollMargin` below the
  // scroll container origin; `row.start` is measured from that origin, so subtract the
  // margin to place each row within the box (TanStack Virtual window-scroller pattern).
  const scrollMargin = virtualizer.options.scrollMargin;
  return (
    <div
      className="relative w-full"
      style={{ height: virtualizer.getTotalSize() }}
    >
      {virtualizer.getVirtualItems().map((row) => {
        const hit = hits[row.index];
        return (
          <div
            key={hit.frame_id}
            data-index={row.index}
            ref={virtualizer.measureElement}
            className="absolute left-0 top-0 w-full pb-3"
            style={{ transform: `translateY(${row.start - scrollMargin}px)` }}
          >
            <SearchResult hit={hit} />
          </div>
        );
      })}
    </div>
  );
}
