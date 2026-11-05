// One numeric Settings row (Intelligence and Data). The value is a draft until the
// user presses Enter or leaves the field, an out-of-range or non-whole entry gets an
// inline error and is NOT saved (no silent clamp), and a failed write keeps the typed
// value on screen with the row's Retry, all through `useSettingField`.
import { useState } from "react";

import { Field } from "../../components/primitives";
import { settingDef, type SettingsField } from "./registry";
import { SettingRow } from "./SettingRow";
import { useSettingField } from "./useSettingField";

/** The numeric UI keys these rows cover; every one writes a whole-number field. */
export type NumericSettingKey =
  | "reports.daily_top_k"
  | "retrieval.default_top_k"
  | "sessions.gap_close_secs"
  | "sessions.min_len_secs"
  | "storage.max_width"
  | "storage.retention_days";

/** The `Settings` field each of those keys writes (all `number`). */
export type NumericSettingsField = Extract<
  SettingsField,
  | "reports_daily_top_k"
  | "retrieval_default_top_k"
  | "sessions_gap_close_secs"
  | "sessions_min_len_secs"
  | "storage_max_width"
  | "storage_retention_days"
>;

export interface NumberFieldRowProps {
  settingKey: NumericSettingKey;
  /** Live note shown while the saved value is 0 (e.g. "Native width — nothing is scaled"). */
  zeroNote?: string;
}

export function NumberFieldRow({ settingKey, zeroNote }: NumberFieldRowProps) {
  const api = useSettingField<NumericSettingsField>(settingKey);
  const def = settingDef(settingKey);
  const [draft, setDraft] = useState<string | null>(null);
  const [rangeError, setRangeError] = useState<string | null>(null);

  const shown = api.value;
  const text = draft ?? (shown === undefined ? "" : String(shown));

  /** Validate the draft, then commit it; a rejected draft stays in the field. */
  function commitDraft(): void {
    if (draft === null) return;
    const trimmed = draft.trim();
    const parsed = Number(trimmed);
    const range = def.range;
    if (trimmed === "" || !Number.isInteger(parsed)) {
      setRangeError(`${def.label} needs a whole number.`);
      return;
    }
    if (range && (parsed < range.min || parsed > range.max)) {
      setRangeError(
        `${def.label} must be between ${range.min} and ${range.max}` +
          `${range.unit ? ` ${range.unit}` : ""}.`,
      );
      return;
    }
    setRangeError(null);
    setDraft(null);
    if (parsed !== shown) api.commit(parsed);
  }

  return (
    <SettingRow
      settingKey={settingKey}
      labelInControl
      changed={api.changed}
      onReset={() => {
        // Clear the draft too, so a rejected entry cannot outlive the reset.
        setDraft(null);
        setRangeError(null);
        api.reset();
      }}
      saving={api.saving}
      error={api.error}
      onRetry={api.retry}
      status={zeroNote !== undefined && shown === 0 ? zeroNote : null}
      control={
        <Field
          className="w-40"
          label={def.label}
          hint={def.effect}
          type="number"
          inputMode="numeric"
          step={1}
          min={def.range?.min}
          max={def.range?.max}
          value={text}
          error={rangeError}
          onChange={(event) => {
            setDraft(event.currentTarget.value);
            if (rangeError !== null) setRangeError(null);
          }}
          onBlur={commitDraft}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitDraft();
            } else if (event.key === "Escape") {
              setDraft(null);
              setRangeError(null);
            }
          }}
        />
      }
    />
  );
}
