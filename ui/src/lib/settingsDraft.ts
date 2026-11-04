import type { Settings } from "../bindings/Settings";

/**
 * Settings keys edited outside the Settings form's own draft, through their own
 * self-contained round-trips:
 *
 * - `app_run_at_startup` / `app_close_to_tray`: AppPanel applies them immediately because
 *   they have fallible OS side effects.
 * - `api_enabled` / `api_port` / `api_bind_address`: ApiPanel's `set_api_config` starts,
 *   stops and rebinds the listener.
 *
 * The form must mirror their persisted values instead of holding its own copy. Otherwise
 * a bulk Save of unrelated fields writes the stale copy back; for the API that stops a
 * listener the user just started, because `set_settings` hot-applies a changed `api_*`
 * value (0.4.5 P3, settings v2).
 */
export const EXTERNALLY_EDITED_KEYS = [
  "app_run_at_startup",
  "app_close_to_tray",
  "api_enabled",
  "api_port",
  "api_bind_address",
] as const satisfies readonly (keyof Settings)[];

/**
 * Copies the externally edited keys from the latest persisted `data` into the form's
 * `draft`, leaving every other in-progress edit untouched.
 *
 * Returns the **same reference** when nothing differs, so a React state update with the
 * result is a no-op and the reconcile effect cannot loop.
 */
export function mirrorExternallyEditedKeys(draft: Settings, data: Settings): Settings {
  if (EXTERNALLY_EDITED_KEYS.every((key) => draft[key] === data[key])) {
    return draft;
  }
  const next: Settings = { ...draft };
  for (const key of EXTERNALLY_EDITED_KEYS) {
    (next as Record<string, unknown>)[key] = data[key];
  }
  return next;
}
