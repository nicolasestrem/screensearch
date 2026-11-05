// Capture (`UI_REFERENCE §4`, `03 §8`): how often a screenshot is taken, which
// displays are captured, and the two global chords. The §4 empty cell for this
// category is the paused-capture line: every row stays editable while capture is
// stopped, and the line says so.
import { useState } from "react";

import { HotkeyField } from "../../components/domain";
import { Field } from "../../components/primitives";
import { useHotkeyStatus, useMonitors, useReadiness, useSettings } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { categoryFrameState, failureMessage } from "./frameState";
import {
  chordsConflict,
  hotkeyRegistration,
  registrationFailure,
  useHotkeyFailureToasts,
  type HotkeyKey,
  type HotkeyRegistration,
} from "./hotkeys";
import { MonitorPicker } from "./MonitorPicker";
import { SettingRow } from "./SettingRow";
import { useSettingField } from "./useSettingField";

function IntervalRow() {
  const field = useSettingField<"capture_interval_ms">("capture.interval_ms");
  const min = field.def.range?.min ?? 0;
  const max = field.def.range?.max ?? Number.MAX_SAFE_INTEGER;
  const [draft, setDraft] = useState<string | null>(null);
  const [rangeError, setRangeError] = useState<string | null>(null);
  const shown = draft ?? (field.value === undefined ? "" : String(field.value));

  // Out of range is refused, never clamped: the typed value and its explanation
  // stay in place until the user fixes them (`UI_REFERENCE §4`, no silent clamp).
  function commitDraft() {
    if (draft === null) return;
    const text = draft.trim();
    const next = Number(text);
    if (!/^\d+$/.test(text) || !Number.isSafeInteger(next)) {
      setRangeError(
        `Enter a whole number of milliseconds between ${min.toLocaleString("en-US")} and ${max.toLocaleString("en-US")}.`,
      );
      return;
    }
    if (next < min || next > max) {
      setRangeError(
        `${next.toLocaleString("en-US")} ms is outside the allowed range — use ${min.toLocaleString("en-US")} to ${max.toLocaleString("en-US")} ms.`,
      );
      return;
    }
    setRangeError(null);
    setDraft(null);
    field.commit(next);
  }

  return (
    <SettingRow
      settingKey="capture.interval_ms"
      labelInControl
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <Field
          label={field.def.label}
          type="number"
          inputMode="numeric"
          min={min}
          max={max}
          step={1}
          value={shown}
          error={rangeError}
          hint={
            <>
              {field.def.effect} Between {min.toLocaleString("en-US")} and{" "}
              {max.toLocaleString("en-US")} ms.
            </>
          }
          onChange={(event) => {
            setDraft(event.currentTarget.value);
            setRangeError(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitDraft();
            }
          }}
          onBlur={commitDraft}
        />
      }
    />
  );
}

function MonitorsRow() {
  const field = useSettingField<"capture_monitors">("capture.monitors");
  const monitors = useMonitors();
  const readFailure = monitors.isError
    ? `Couldn’t read the connected displays: ${failureMessage(monitors.error)}. Your selection is unchanged.`
    : null;
  return (
    <SettingRow
      settingKey="capture.monitors"
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <MonitorPicker
          label={field.def.label}
          monitors={monitors.data}
          selected={field.value ?? []}
          loading={monitors.isPending}
          error={readFailure}
          onChange={(next) => field.commit(next)}
        />
      }
    />
  );
}

interface HotkeyRowProps {
  settingKey: HotkeyKey;
  label: string;
  effect: string;
  value: string;
  defaultValue: string;
  onChange(chord: string): void;
  changed: boolean;
  onReset(): void;
  saving: boolean;
  saveError: string | null;
  onRetry(): void;
  registration: HotkeyRegistration;
  checking: boolean;
  /** The other chord, when both are set to the same combination. */
  conflictWith: string | null;
}

function HotkeyRow(props: HotkeyRowProps) {
  const failure =
    props.registration.present && !props.registration.registered
      ? registrationFailure(props.registration.error)
      : null;
  const status = failure ? null : props.registration.registered ? (
    <span className="text-status-ok-fg">
      Registered — {props.registration.chord ?? props.value} works from any app.
    </span>
  ) : props.checking ? (
    "Checking registration…"
  ) : null;

  return (
    <SettingRow
      settingKey={props.settingKey}
      labelInControl
      status={status}
      changed={props.changed}
      onReset={props.onReset}
      saving={props.saving}
      error={props.saveError}
      onRetry={props.onRetry}
      control={
        <HotkeyField
          label={props.label}
          hint={props.effect}
          value={props.value}
          defaultValue={props.defaultValue}
          onChange={props.onChange}
          error={failure}
        />
      }
    >
      {props.conflictWith ? (
        <p
          role="status"
          className="mt-1 min-w-0 border-l-2 border-status-warn-border bg-status-warn-bg px-3 py-2 text-t-sm text-status-warn-fg"
        >
          Both chords are set to {props.value} — the {props.conflictWith} needs a
          different combination.
        </p>
      ) : null}
    </SettingRow>
  );
}

function HotkeyRows() {
  const overlay = useSettingField<"overlay_hotkey">("overlay.hotkey");
  const marks = useSettingField<"marks_hotkey">("marks.hotkey");
  const statuses = useHotkeyStatus();
  useHotkeyFailureToasts(statuses.data);
  const overlayChord = overlay.value ?? overlay.defaultValue;
  const marksChord = marks.value ?? marks.defaultValue;
  const conflict = chordsConflict(overlayChord, marksChord);
  const checking = statuses.isPending;

  return (
    <>
      <HotkeyRow
        settingKey="overlay.hotkey"
        label={overlay.def.label}
        effect={overlay.def.effect}
        value={overlayChord}
        defaultValue={overlay.defaultValue}
        onChange={overlay.commit}
        changed={overlay.changed}
        onReset={overlay.reset}
        saving={overlay.saving}
        saveError={overlay.error}
        onRetry={overlay.retry}
        registration={hotkeyRegistration(statuses.data, "overlay.hotkey")}
        checking={checking}
        conflictWith={conflict ? "mark-this-moment hotkey" : null}
      />
      <HotkeyRow
        settingKey="marks.hotkey"
        label={marks.def.label}
        effect={marks.def.effect}
        value={marksChord}
        defaultValue={marks.defaultValue}
        onChange={marks.commit}
        changed={marks.changed}
        onReset={marks.reset}
        saving={marks.saving}
        saveError={marks.error}
        onRetry={marks.retry}
        registration={hotkeyRegistration(statuses.data, "marks.hotkey")}
        checking={checking}
        conflictWith={conflict ? "overlay hotkey" : null}
      />
    </>
  );
}

export function CaptureCategory() {
  const settings = useSettings();
  const monitors = useMonitors();
  const hotkeys = useHotkeyStatus();
  const readiness = useReadiness();
  const paused = readiness.data?.capture.status === "disabled";
  const state = categoryFrameState(
    settings,
    monitors.isError || hotkeys.isError || readiness.isError,
  );

  return (
    <CategoryState
      category="capture"
      rows={4}
      state={state}
      error={{
        title: "Couldn’t read your capture settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="A status check failed, so the display, hotkey and capture-state lines below may be out of date. Your settings loaded."
    >
      {paused ? (
        <p
          role="status"
          data-empty="capture-paused"
          className="mb-3 min-w-0 border-l-2 border-line-strong bg-surface-inset px-3 py-2 text-t-sm text-fg-secondary"
        >
          Paused — capture is stopped, so nothing is being recorded right now.
          Every row below still applies the next time you start capture.
        </p>
      ) : null}
      <IntervalRow />
      <MonitorsRow />
      <HotkeyRows />
    </CategoryState>
  );
}
