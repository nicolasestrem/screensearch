// Frame state for one settings category (`UI_REFERENCE §4`, 0.4.5 D9).
//
// The frame has four states: loading, error, partial, populated. The fifth §4
// state — `empty` — is an in-frame condition in Settings, not a blank pane: the
// paused-capture line (Capture), "Nothing excluded yet" (Privacy) and General's
// "every control holds a value" are all rendered by the row that owns them, so a
// category never hides its sibling rows to show one empty list.
import type { UseQueryResult } from "@tanstack/react-query";

import type { Settings } from "../../bindings/Settings";

export type CategoryFrameState = "loading" | "error" | "partial" | "populated";

/** The read-hook flags the frame needs; a `useSettings()` result satisfies it. */
export type SettingsRead = Pick<
  UseQueryResult<Settings>,
  "data" | "error" | "isError" | "isPending" | "isRefetchError"
>;

/**
 * Resolve the frame state. A secondary status probe that failed while the
 * settings themselves arrived is `partial`: the rows stay usable and the frame
 * says which status is missing.
 */
export function categoryFrameState(
  read: SettingsRead,
  statusFailed = false,
): CategoryFrameState {
  if (read.isError) return "error";
  if (read.isPending || read.data === undefined) return "loading";
  if (statusFailed || read.isRefetchError) return "partial";
  return "populated";
}

/** A read failure as one plain sentence fragment, never a stack or an object dump. */
export function failureMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message.trim();
  }
  if (typeof error === "string" && error.trim().length > 0) return error.trim();
  return "the backend did not respond";
}
