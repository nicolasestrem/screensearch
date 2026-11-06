// The Ask conversation flow (`03 §7i.4`): one streaming turn inside a persisted
// conversation. Modelled on `useAsk` — a reducer folding request-scoped `answer_delta`
// events and an in-flight guard released on a terminal phase — plus the conversation's
// identity (`threadId`/`turnId` from `ask_turn`), the terminal `citations` delta, and the
// cache invalidation that lets the persisted rows (the record) replace the live stream.
import { useCallback, useEffect, useReducer, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { listenTo } from "./events";
import * as cmd from "./commands";
import { queryKeys } from "./queryKeys";
import type { AnswerDelta } from "../../bindings/AnswerDelta";
import type { AskCitations } from "../../bindings/AskCitations";
import type { AskConversationRequest } from "../../bindings/AskConversationRequest";
import type { AskTurnStartError } from "../../bindings/AskTurnStartError";

/** Streaming phase of one conversation turn; `done` and `error` are terminal. */
export type AskConversationPhase = "idle" | "streaming" | "done" | "error";

/**
 * A failure that is not the command's start error: the stream itself reported one, or the
 * turn was stopped locally. Tagged `stream` so a view can tell it from the start errors
 * (`thread_not_found` / `no_answer_model` / `failed`) that arrive before any delta.
 */
export interface AskStreamError {
  kind: "stream";
  message: string;
}

export interface AskConversationState {
  phase: AskConversationPhase;
  /** The id this turn's deltas are keyed by; `null` before the first ask. */
  requestId: string | null;
  /** The conversation the turn belongs to: the requested thread, or the one `ask_turn`
   *  created for a new conversation (known once the command resolves or the terminal delta
   *  arrives). */
  threadId: number | null;
  /** The appended turn; `null` until the command resolves. */
  turnId: number | null;
  /** Whether this turn asked for chain-of-thought — the view shows an indicator, not the
   *  trace. */
  thinking: boolean;
  /** Accumulated answer prose (markdown). */
  answer: string;
  /** The turn's citations, folded from the terminal `citations` delta; `null` until it
   *  arrives. The persisted turn carries the same pair, so a reload re-renders it. */
  citations: AskCitations | null;
  error: AskTurnStartError | AskStreamError | null;
}

const initial: AskConversationState = {
  phase: "idle",
  requestId: null,
  threadId: null,
  turnId: null,
  thinking: false,
  answer: "",
  citations: null,
  error: null,
};

type AskConversationAction =
  | { type: "start"; requestId: string; threadId: number | null; thinking: boolean }
  | { type: "started"; threadId: number; turnId: number }
  | { type: "fail"; error: AskTurnStartError | AskStreamError }
  | { type: "delta"; delta: AnswerDelta }
  | { type: "reset" };

function reducer(
  state: AskConversationState,
  action: AskConversationAction,
): AskConversationState {
  switch (action.type) {
    case "reset":
      return initial;
    case "start":
      return {
        ...initial,
        phase: "streaming",
        requestId: action.requestId,
        threadId: action.threadId,
        thinking: action.thinking,
      };
    case "started":
      return { ...state, threadId: action.threadId, turnId: action.turnId };
    case "fail":
      return { ...state, phase: "error", error: action.error };
    case "delta": {
      const d = action.delta;
      switch (d.type) {
        case "thinking":
          // The conversation view shows *that* the model is thinking, never the trace.
          return state;
        case "token":
          return { ...state, answer: state.answer + d.text };
        case "citation":
          // Per-frame citation events are the transient form; the terminal `citations`
          // delta carries the same frames as the persisted pair, which is what a
          // conversation renders and what survives a reload.
          return state;
        case "citations":
          return {
            ...state,
            citations: { sessions: d.sessions, frames: d.frames },
            // A delta can beat the command's resolved ids; filling them here keeps the
            // terminal invalidation keyed to the right conversation either way.
            threadId: state.threadId ?? d.thread_id,
            turnId: state.turnId ?? d.turn_id,
          };
        case "done":
          // A `done` after an `error` must not resurrect the stream.
          return state.phase === "error" ? state : { ...state, phase: "done" };
        case "error":
          return {
            ...state,
            phase: "error",
            error: { kind: "stream", message: d.message },
          };
      }
    }
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * Read the `ask_turn` rejection as its typed error. Tauri serializes a typed command error
 * as a plain object rather than wrapping it in `Error`, so the shape is checked field by
 * field; anything else — a closed bridge, a missing store — becomes a `stream` failure that
 * keeps its own message.
 */
function asStartError(error: unknown): AskTurnStartError | AskStreamError {
  if (isObject(error)) {
    if (error.kind === "no_answer_model") return { kind: "no_answer_model" };
    if (error.kind === "thread_not_found" && typeof error.thread_id === "number") {
      return { kind: "thread_not_found", thread_id: error.thread_id };
    }
    if (error.kind === "failed" && typeof error.message === "string") {
      return { kind: "failed", message: error.message };
    }
  }
  return { kind: "stream", message: String(error) };
}

export interface UseAskConversation {
  state: AskConversationState;
  /** Start one turn. A turn already streaming is cancelled first, so two streams never
   *  fold into the same state. */
  ask: (request: Omit<AskConversationRequest, "request_id">) => Promise<void>;
  /** Stop the streaming turn. Its answer stays NULL, so the question remains in the thread
   *  with a retry — the same state a failed turn renders. */
  cancel: () => void;
  /** Cancel and clear back to idle. */
  reset: () => void;
}

export function useAskConversation(): UseAskConversation {
  const [state, dispatch] = useReducer(reducer, initial);
  const qc = useQueryClient();
  const activeRequest = useRef<string | null>(null);

  // One persistent subscription for the hook's lifetime; deltas always fold into the latest
  // state through the reducer, and the request-id scope drops a superseded turn's tail.
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

  // A terminal phase releases the in-flight guard (the command resolves immediately, so the
  // guard cannot key off the awaited promise) and refreshes the persisted record: the turn's
  // `answer`/`citations` rows and the list's `turn_count`/`last_turn_at` both changed. The
  // stream is the live view; the rows are the record (`§7i.4`).
  useEffect(() => {
    if (state.phase !== "done" && state.phase !== "error") return;
    activeRequest.current = null;
    if (state.threadId != null) {
      void qc.invalidateQueries({ queryKey: queryKeys.askTurns(state.threadId) });
    }
    void qc.invalidateQueries({ queryKey: queryKeys.askThreads });
  }, [state.phase, state.threadId, qc]);

  const ask = useCallback(
    async (request: Omit<AskConversationRequest, "request_id">) => {
      const inFlight = activeRequest.current;
      if (inFlight) {
        await cmd.cancelAsk(inFlight).catch(() => undefined);
      }
      const requestId = `turn-${Date.now().toString(36)}-${Math.random()
        .toString(36)
        .slice(2)}`;
      activeRequest.current = requestId;
      dispatch({
        type: "start",
        requestId,
        threadId: request.thread_id,
        thinking: request.thinking,
      });
      try {
        const started = await cmd.askTurn({ ...request, request_id: requestId });
        // `ask_turn` assembles context before it resolves, so by now a newer question (or a
        // cancel/reset) may own the state. Its ids belong to the request that was superseded,
        // and writing them here would hand the live state someone else's conversation while
        // this request's own tokens are dropped by the delta scope below — the same guard the
        // deltas use, applied to the command's outcome.
        if (activeRequest.current !== requestId) return;
        dispatch({
          type: "started",
          threadId: started.thread_id,
          turnId: started.turn_id,
        });
      } catch (error) {
        // The command itself failed (unknown conversation, no model, planner error) before
        // the turn existed; the question stays with the caller for a retry. A superseded
        // request's failure says nothing about the one now streaming.
        if (activeRequest.current !== requestId) return;
        dispatch({ type: "fail", error: asStartError(error) });
      }
    },
    [],
  );

  const cancel = useCallback(() => {
    const requestId = activeRequest.current;
    if (!requestId) return;
    activeRequest.current = null;
    void cmd.cancelAsk(requestId).catch(() => undefined);
    dispatch({ type: "fail", error: { kind: "stream", message: "Answer stopped" } });
  }, []);

  const reset = useCallback(() => {
    const requestId = activeRequest.current;
    activeRequest.current = null;
    if (requestId) {
      void cmd.cancelAsk(requestId).catch(() => undefined);
    }
    dispatch({ type: "reset" });
  }, []);

  return { state, ask, cancel, reset };
}
