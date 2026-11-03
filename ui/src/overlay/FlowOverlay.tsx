import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  Button,
  EmptyState,
  ErrorState,
  Skeleton,
} from "../components/primitives";
import { IconRecall } from "../components/icons";
import { hideOverlay, openMoment, overlayShownAck } from "../lib/ipc/commands";
import { listenTo } from "../lib/ipc/events";
import { queryKeys } from "../lib/ipc/queryKeys";
import { useAsk } from "../lib/ipc/useAsk";
import {
  useAdvancedConfig,
  useSearch,
  useSettings,
  useWhereWasI,
} from "../lib/ipc/queries";
import { cn } from "../lib/cn";
import type { SearchQuery } from "../bindings/SearchQuery";
import type { SearchHit } from "../bindings/SearchHit";
import { OverlayResultRow } from "./OverlayResultRow";
import { OverlayAsk } from "./OverlayAsk";
import { WhereWasIStrip } from "./WhereWasIStrip";

type Mode = "search" | "ask";

const SEARCH_DEBOUNCE_MS = 250;
const ASK_MAX_TOKENS = 2048;
const LISTBOX_ID = "flow-overlay-results";

export function FlowOverlay() {
  const inputRef = useRef<HTMLInputElement>(null);
  const qc = useQueryClient();
  const [mode, setMode] = useState<Mode>("search");
  const [text, setText] = useState("");
  const [debounced, setDebounced] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const settings = useSettings();
  const ask = useAsk();
  const resetAsk = ask.reset;
  // The overlay's result depth is a `file-only` key (`overlay.max_results`, `03 §8`):
  // it comes from the read-only advanced projection, never a literal. Until that load
  // lands the search stays idle instead of guessing a depth.
  const advanced = useAdvancedConfig();
  const resultLimit = advanced.data?.config.overlay.max_results;
  const normalizedText = text.trim();
  const committedSearchText = debounced.trim();
  const searchWaitingForCommit =
    mode === "search" &&
    normalizedText.length > 0 &&
    normalizedText !== committedSearchText;
  const searchMatchesInput =
    mode === "search" &&
    normalizedText.length > 0 &&
    normalizedText === committedSearchText;

  useEffect(() => {
    const t = setTimeout(() => setDebounced(text), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [text]);

  const query: SearchQuery = useMemo(
    () => ({
      text: debounced,
      // Placeholder only: the search is disabled below until `resultLimit` is known,
      // so this value is never sent to the backend.
      limit: resultLimit ?? 0,
      time_range: null,
      include_chrome: false,
    }),
    [debounced, resultLimit],
  );
  const search = useSearch(
    query,
    mode === "search" && resultLimit != null,
    true,
  );
  // Empty-query search state shows the where-was-i strip (`03 §7b`); the query only runs
  // when that strip is visible, and refreshes on each summon (it depends on live context).
  const showResume = mode === "search" && normalizedText.length === 0;
  const resume = useWhereWasI(showResume);
  const hits = searchMatchesInput ? (search.data ?? []) : [];
  const activeHit = searchMatchesInput
    ? hits[Math.min(activeIndex, Math.max(hits.length - 1, 0))]
    : undefined;
  const busy =
    (mode === "search" &&
      normalizedText.length > 0 &&
      (searchWaitingForCommit || search.isFetching)) ||
    ask.phase === "streaming";

  useEffect(() => {
    setActiveIndex(0);
  }, [text, mode]);

  useEffect(() => {
    if (activeIndex >= hits.length) {
      setActiveIndex(Math.max(hits.length - 1, 0));
    }
  }, [activeIndex, hits.length]);

  useEffect(() => {
    let active = true;
    const unlisteners: Array<() => void> = [];
    const track = (p: Promise<() => void>) =>
      p
        .then((u) => {
          if (active) unlisteners.push(u);
          else u();
        })
        .catch(() => {
          /* no Tauri runtime in browser dev */
        });

    track(
      listenTo("overlay_shown", () => {
        // Each summon re-checks where-was-i (it reflects the live foreground context).
        qc.invalidateQueries({ queryKey: queryKeys.whereWasI });
        inputRef.current?.focus();
        inputRef.current?.select();
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            void overlayShownAck().catch(() => undefined);
          });
        });
      }),
    );
    track(
      listenTo("overlay_hidden", () => {
        resetAsk();
      }),
    );

    return () => {
      active = false;
      unlisteners.forEach((u) => u());
    };
  }, [resetAsk, qc]);

  const openFrame = (frameId: number) => {
    void openMoment(frameId).catch(() => undefined);
  };

  const submitAsk = () => {
    const question = text.trim();
    if (!question) return;
    setMode("ask");
    void ask.ask({
      query: question,
      thinking: settings.data?.answer_thinking ?? true,
      max_tokens: ASK_MAX_TOKENS,
      top_k: null,
    });
  };

  const toggleMode = () => {
    setMode((m) => {
      const next = m === "search" ? "ask" : "search";
      if (next === "search") ask.reset();
      return next;
    });
  };

  const handleTextChange = (value: string) => {
    if (value.startsWith("?")) {
      setMode("ask");
      setText(value.slice(1).trimStart());
      return;
    }
    setText(value);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void hideOverlay().catch(() => undefined);
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      toggleMode();
      return;
    }
    if (e.key === "?" && text.length === 0) {
      e.preventDefault();
      setMode("ask");
      return;
    }
    if (mode === "search" && e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, Math.max(hits.length - 1, 0)));
      return;
    }
    if (mode === "search" && e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (mode === "ask") {
        submitAsk();
      } else if (showResume && resume.data) {
        // Empty query + a resume context → jump straight back to it.
        openFrame(resume.data.frame_id);
      } else if (searchWaitingForCommit) {
        setDebounced(text);
      } else if (activeHit) {
        openFrame(activeHit.frame_id);
      }
    }
  };

  return (
    <div className="flex h-full items-start justify-center p-3">
      <section className="w-full overflow-hidden rounded-panel border border-line bg-overlay shadow-scan">
        <div
          className={cn(
            "h-1 bg-accent shadow-scan",
            busy && "scanlines scanlines-drift",
          )}
          aria-hidden="true"
        />
        <div className="scanlines flex flex-col gap-3 p-3">
          <div className="flex items-center gap-2">
            <input
              ref={inputRef}
              value={text}
              onChange={(e) => handleTextChange(e.currentTarget.value)}
              onKeyDown={handleKeyDown}
              role="combobox"
              aria-label={mode === "search" ? "Flow search" : "Flow ask"}
              aria-controls={LISTBOX_ID}
              aria-expanded={mode === "search"}
              aria-activedescendant={
                mode === "search" && activeHit
                  ? optionId(activeHit.frame_id)
                  : undefined
              }
              autoFocus
              placeholder={
                mode === "search"
                  ? "Search screen history"
                  : "Ask about captured screens"
              }
              className="min-w-0 flex-1 rounded-chip border border-line bg-base px-3 min-h-hit-min text-body text-ink placeholder:text-ink-faint font-body transition-colors duration-fast ease-ui focus:border-accent"
            />
            <Button
              variant={mode === "search" ? "primary" : "secondary"}
              size="sm"
              onClick={() => {
                setMode("search");
                ask.reset();
              }}
            >
              Search
            </Button>
            <Button
              variant={mode === "ask" ? "primary" : "secondary"}
              size="sm"
              onClick={() => setMode("ask")}
            >
              Ask
            </Button>
          </div>

          <div className="max-h-96 min-h-72 overflow-auto rounded-panel border border-line bg-base">
            {mode === "search" ? (
              <SearchOverlayBody
                text={text}
                hits={hits}
                isFetching={search.isFetching || searchWaitingForCommit}
                isError={search.isError}
                error={search.error}
                onRetry={() => search.refetch()}
                onOpen={openFrame}
                activeIndex={activeIndex}
                onActiveIndexChange={setActiveIndex}
                resume={resume}
              />
            ) : (
              <OverlayAsk
                question={text}
                ask={ask}
                onSubmit={submitAsk}
                onOpenFrame={openFrame}
              />
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

interface SearchOverlayBodyProps {
  text: string;
  hits: SearchHit[];
  isFetching: boolean;
  isError: boolean;
  error: unknown;
  onRetry: () => void;
  onOpen: (frameId: number) => void;
  activeIndex: number;
  onActiveIndexChange: (index: number) => void;
  resume: ReturnType<typeof useWhereWasI>;
}

function SearchOverlayBody({
  text,
  hits,
  isFetching,
  isError,
  error,
  onRetry,
  onOpen,
  activeIndex,
  onActiveIndexChange,
  resume,
}: SearchOverlayBodyProps) {
  if (text.trim().length === 0) {
    // Empty query → the where-was-i strip (`03 §7b`) instead of a plain prompt.
    return <WhereWasIStrip resume={resume} />;
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
      <div className="flex flex-col gap-2 p-3">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} className="flex items-center gap-3">
            <Skeleton className="h-14 w-24 shrink-0" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-5 w-full" />
              <Skeleton className="h-4 w-2/3" />
            </div>
          </div>
        ))}
      </div>
    );
  }
  if (hits.length === 0) {
    return (
      <EmptyState
        icon={<IconRecall size={24} />}
        title="No matches"
        description="Try different words, or open Recall for filters."
      />
    );
  }

  return (
    <div
      id={LISTBOX_ID}
      role="listbox"
      aria-label="Flow search results"
      className="flex flex-col gap-2 p-2"
    >
      {hits.map((hit, index) => (
        <OverlayResultRow
          key={hit.frame_id}
          id={optionId(hit.frame_id)}
          hit={hit}
          selected={index === activeIndex}
          onOpen={() => onOpen(hit.frame_id)}
          onHover={() => onActiveIndexChange(index)}
        />
      ))}
    </div>
  );
}

function optionId(frameId: number): string {
  return `flow-option-${frameId}`;
}
