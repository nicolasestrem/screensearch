// The `ask` flow: a reducer that folds streamed `answer_delta` events into a
// stable view-model (UI_REFERENCE §6). Each stream is scoped by a request id so
// superseded deltas are ignored and reset can cancel the backend provider task.
import { useCallback, useEffect, useReducer, useRef } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { listenTo } from "./events";
import * as cmd from "./commands";
import type { AnswerDelta } from "../../bindings/AnswerDelta";
import type { AskRequest } from "../../bindings/AskRequest";

export type AskPhase = "idle" | "streaming" | "done" | "error";

export interface AskState {
  phase: AskPhase;
  /** Accumulated chain-of-thought (shown collapsed); empty when not requested. */
  thinking: string;
  /** Accumulated answer prose (markdown). */
  answer: string;
  /** Source frame ids supplied to the model, in first-seen order, deduplicated. */
  citations: number[];
  error: string | null;
}

const initial: AskState = {
  phase: "idle",
  thinking: "",
  answer: "",
  citations: [],
  error: null,
};

type AskAction =
  { type: "start" } | { type: "reset" } | { type: "delta"; delta: AnswerDelta };

function reducer(state: AskState, action: AskAction): AskState {
  switch (action.type) {
    case "reset":
      return initial;
    case "start":
      return { ...initial, phase: "streaming" };
    case "delta": {
      const d = action.delta;
      switch (d.type) {
        case "thinking":
          return { ...state, thinking: state.thinking + d.text };
        case "token":
          return { ...state, answer: state.answer + d.text };
        case "citation":
          // The current backend emits one id per frame included in the model prompt.
          // The UI labels these as checked context, not claim-level citations.
          return state.citations.includes(d.frame_id)
            ? state
            : { ...state, citations: [...state.citations, d.frame_id] };
        case "citations":
          // Ask v2 terminal citations (thread/session evidence). The UI keeps rendering
          // the same source-frame tiles it already collects from the per-`citation`
          // events, so this delta folds into no visible state.
          return state;
        case "done":
          // A `done` after an `error` must not resurrect the stream.
          return state.phase === "error" ? state : { ...state, phase: "done" };
        case "error":
          return { ...state, phase: "error", error: d.message };
      }
    }
  }
}

export interface UseAsk extends AskState {
  /** Start streaming an answer for `request`. */
  ask: (request: Omit<AskRequest, "request_id">) => Promise<void>;
  /** Clear back to idle. */
  reset: () => void;
}

export function useAsk(): UseAsk {
  const [state, dispatch] = useReducer(reducer, initial);
  const activeRequest = useRef<string | null>(null);

  // One persistent subscription for the lifetime of the hook; deltas always fold
  // into the latest state via the reducer.
  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;
    listenTo("answer_delta", (event) => {
      if (event.request_id === activeRequest.current) {
        dispatch({ type: "delta", delta: event.delta });
      }
    })
      .then((u) => {
        if (active) unlisten = u;
        else u();
      })
      .catch(() => {
        /* no Tauri runtime (dev): no live deltas */
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  // Release the in-flight guard once the stream reaches a terminal phase, so the
  // next ask() can start. cmd.ask resolves immediately (the answer arrives later
  // as deltas), so the guard can't key off the awaited promise.
  useEffect(() => {
    if (state.phase === "done" || state.phase === "error") {
      activeRequest.current = null;
    }
  }, [state.phase]);

  const ask = useCallback(async (request: Omit<AskRequest, "request_id">) => {
    if (activeRequest.current) {
      await cmd.cancelAsk(activeRequest.current).catch(() => undefined);
    }
    const requestId = `ask-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    activeRequest.current = requestId;
    dispatch({ type: "start" });
    try {
      await cmd.ask({ ...request, request_id: requestId });
    } catch (e) {
      // The command itself failed (e.g. sidecar not ready) before any delta.
      dispatch({ type: "delta", delta: { type: "error", message: String(e) } });
    }
  }, []);

  const reset = useCallback(() => {
    const requestId = activeRequest.current;
    if (requestId) {
      void cmd.cancelAsk(requestId).catch(() => undefined);
    }
    activeRequest.current = null;
    dispatch({ type: "reset" });
  }, []);

  return { ...state, ask, reset };
}
