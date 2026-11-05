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
  // Order matters. TanStack keeps the last successful payload when a background
  // refetch fails, and that failure sets `isError` as well as `isRefetchError`. A
  // checked flag first would therefore replace a usable category with an error
  // panel, unmounting rows and discarding live drafts. So: only a read that never
  // produced data is an `error`; a failure on top of data is `partial`.
  if (read.data === undefined) {
    if (read.isError) return "error";
    return "loading";
  }
  if (read.isError || read.isRefetchError || statusFailed) {
    return "partial";
  }
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
