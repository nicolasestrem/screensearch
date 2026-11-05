// Per-field autosave (settings v2, 0.4.5 D9): one row commits one value through
// the same write path the old screen used, keeps a failed value in place, and
// pushes an undo toast that re-commits the previous value through that path.
import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import type { ModelLane } from "../../bindings/ModelLane";
import type { ModelTier } from "../../bindings/ModelTier";
import type { Settings } from "../../bindings/Settings";
import { pushActionToast } from "../../components/shell/actionToast";
import { useSetModelTier, useSetSettings } from "../../lib/ipc/mutations";
import { queryKeys } from "../../lib/ipc/queryKeys";
import { useSettings } from "../../lib/ipc/queries";
import {
  freshSettingsSnapshot,
  settingsWriteQueue,
} from "./settingsWrites";
import {
  SETTINGS_DEFAULTS,
  settingDef,
  type SettingDef,
  type SettingsField,
  type SettingsFieldKey,
} from "./registry";

export interface CommitOptions {
  /** Skip the success toast (used by undo, which would otherwise stack toasts). */
  silent?: boolean;
}

export interface SettingFieldApi<F extends SettingsField> {
  def: SettingDef;
  /** The optimistic value; the failed value when the last save failed. */
  value: Settings[F] | undefined;
  /** What the cache holds right now. */
  savedValue: Settings[F] | undefined;
  defaultValue: Settings[F];
  /** `savedValue` differs from the default (arrays compare element by element). */
  changed: boolean;
  saving: boolean;
  /** Plain-language explanation of the last failed save, or null. */
  error: string | null;
  commit(next: Settings[F], options?: CommitOptions): void;
  /** Re-commit the value whose save failed. */
  retry(): void;
  reset(): void;
}

export interface SettingsPatchApi {
  commitPatch(
    patch: Partial<Settings>,
    label: string,
    options?: CommitOptions,
  ): void;
  saving: boolean;
  error: string | null;
  retry(): void;
}

/** Deep-equal for the value shapes `Settings` holds (primitives and arrays of them). */
function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, i) => Object.is(item, b[i]));
  }
  return false;
}

/** The plain-language reason a write failed, without echoing any secret. */
function reason(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message.trim();
  }
  if (typeof error === "string" && error.trim().length > 0) return error.trim();
  return "the backend did not accept it";
}

/**
 * The one failure sentence every row shows: what failed, why, and that the typed
 * value was kept. `label` is sentence-case copy, so it reads lowercased inline.
 */
function saveFailure(label: string, error: unknown): string {
  const inline = `${label.charAt(0).toLowerCase()}${label.slice(1)}`;
  return `Couldn’t save ${inline}: ${reason(error)}. Your value is kept — Retry.`;
}

/**
 * One settings field, autosaving. `models.*` keeps its hot-apply path
 * (`set_model_tier`); every other key sends the whole struct with one field
 * overridden, because `set_settings` is a whole-struct write.
 */
export function useSettingField<F extends SettingsField>(
  key: SettingsFieldKey,
): SettingFieldApi<F> {
  const def = settingDef(key);
  const field = def.field as SettingsField;
  const settings = useSettings();
  const qc = useQueryClient();
  const setSettings = useSetSettings();
  const setTier = useSetModelTier();
  const [pendingValue, setPendingValue] = useState<Settings[F] | null>(null);
  const [failure, setFailure] = useState<{
    value: Settings[F];
    message: string;
  } | null>(null);

  const savedValue = settings.data?.[field] as Settings[F] | undefined;
  const defaultValue = SETTINGS_DEFAULTS[field] as Settings[F];
  const lane: ModelLane | null =
    field === "models_answer_tier"
      ? "answer"
      : field === "models_vision_tier"
        ? "vision"
        : null;

  function commit(next: Settings[F], options?: CommitOptions): void {
    const current =
      qc.getQueryData<Settings>(queryKeys.settings) ?? settings.data;
    if (!current) {
      setFailure({
        value: next,
        message: saveFailure(def.label, new Error("settings have not loaded yet")),
      });
      return;
    }
    setPendingValue(next);
    // Queued: `set_settings` writes the whole struct, so an earlier write's refetch
    // must never be able to erase this value from the base of a later one (see
    // `settingsWrites.ts`). The undo payload is read from the fresh base, so Undo
    // restores what the backend actually had.
    void settingsWriteQueue.enqueue(async () => {
      const base = await freshSettingsSnapshot(qc, queryKeys.settings);
      if (!base) {
        setPendingValue(null);
        setFailure({
          value: next,
          message: saveFailure(def.label, new Error("settings could not be re-read")),
        });
        return;
      }
      const previous = base[field] as Settings[F];
      const onSuccess = () => {
        setPendingValue(null);
        setFailure(null);
        if (options?.silent !== true) {
          pushActionToast("success", `${def.label} updated`, {
            label: "Undo",
            onAction: () => commit(previous, { silent: true }),
          });
        }
      };
      const onError = (error: unknown) => {
        setPendingValue(null);
        setFailure({ value: next, message: saveFailure(def.label, error) });
      };
      if (lane !== null) {
        // `set_model_tier` has no optimistic phase of its own; write the cache the
        // way `set_settings` does so the picker does not flash the old tier while
        // the settings refetch lands.
        qc.setQueryData<Settings>(queryKeys.settings, { ...base, [field]: next });
        try {
          await setTier.mutateAsync({ lane, tier: next as ModelTier });
          onSuccess();
        } catch (error) {
          qc.setQueryData<Settings>(queryKeys.settings, {
            ...base,
            [field]: previous,
          });
          onError(error);
        }
        return;
      }
      try {
        await setSettings.mutateAsync({ ...base, [field]: next });
        onSuccess();
      } catch (error) {
        onError(error);
      }
    });
  }

  const value = failure ? failure.value : (pendingValue ?? savedValue);

  return {
    def,
    value,
    savedValue,
    defaultValue,
    changed: savedValue !== undefined && !sameValue(savedValue, defaultValue),
    saving: lane !== null ? setTier.isPending : setSettings.isPending,
    error: failure ? failure.message : null,
    commit,
    retry: () => {
      if (failure) commit(failure.value);
    },
    reset: () => commit(defaultValue),
  };
}

/**
 * Multi-key autosave for controls that edit more than one field at once
 * (the vision schedule pairs an enabled flag with its interval). One toast,
 * one undo that restores every patched value.
 */
export function useSettingsPatch(): SettingsPatchApi {
  const settings = useSettings();
  const qc = useQueryClient();
  const setSettings = useSetSettings();
  const [error, setError] = useState<string | null>(null);
  const last = useRef<{ patch: Partial<Settings>; label: string } | null>(null);

  function commitPatch(
    patch: Partial<Settings>,
    label: string,
    options?: CommitOptions,
  ): void {
    const current =
      qc.getQueryData<Settings>(queryKeys.settings) ?? settings.data;
    if (!current) {
      setError(saveFailure(label, new Error("settings have not loaded yet")));
      return;
    }
    last.current = { patch, label };
    // Same rule as the single-field path: queued, and built from a snapshot that
    // already contains every settled write, so a patch cannot revert a neighbour.
    void settingsWriteQueue.enqueue(async () => {
      const base = await freshSettingsSnapshot(qc, queryKeys.settings);
      if (!base) {
        setError(saveFailure(label, new Error("settings could not be re-read")));
        return;
      }
      const previous: Partial<Settings> = {};
      for (const patchKey of Object.keys(patch) as SettingsField[]) {
        Object.assign(previous, { [patchKey]: base[patchKey] });
      }
      try {
        await setSettings.mutateAsync({ ...base, ...patch });
        setError(null);
        if (options?.silent !== true) {
          pushActionToast("success", `${label} updated`, {
            label: "Undo",
            onAction: () => commitPatch(previous, label, { silent: true }),
          });
        }
      } catch (cause) {
        setError(saveFailure(label, cause));
      }
    });
  }

  return {
    commitPatch,
    saving: setSettings.isPending,
    error,
    retry: () => {
      if (last.current) {
        commitPatch(last.current.patch, last.current.label, { silent: true });
      }
    },
  };
}
