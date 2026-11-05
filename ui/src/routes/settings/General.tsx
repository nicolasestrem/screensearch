// General (`UI_REFERENCE §4`, `03 §8`): launch at startup, close to tray, the
// theme, plus the version/update section the 0.3.2 App panel carried. The §4
// empty cell for this category is "—" (every control holds a value), so the frame
// implements loading, error, partial and populated.
import { APPEARANCE_OPTIONS } from "../../app/theme";
import type { UiAppearance } from "../../bindings/UiAppearance";
import { Select, Toggle } from "../../components/primitives";
import { useSettings, useUpdateStatus } from "../../lib/ipc/queries";
import { AppUpdates } from "./AppUpdates";
import { CategoryState } from "./CategoryState";
import { categoryFrameState, failureMessage } from "./frameState";
import { SettingRow } from "./SettingRow";
import { useSettingField } from "./useSettingField";

function StartupRow() {
  const field = useSettingField<"app_run_at_startup">("app.run_at_startup");
  return (
    <SettingRow
      settingKey="app.run_at_startup"
      labelInControl
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <Toggle
          label={field.def.label}
          hint={field.def.effect}
          checked={field.value ?? field.defaultValue}
          onChange={(next) => field.commit(next)}
        />
      }
    >
      {field.error ? (
        <p
          role="status"
          className="mt-1 max-w-prose border-l-2 border-status-warn-border bg-status-warn-bg px-3 py-2 text-t-sm text-status-warn-fg"
        >
          Windows refused the launch-at-login change, so nothing was stored —
          ScreenSearch{" "}
          {field.savedValue
            ? "still starts with Windows"
            : "still does not start with Windows"}
          .
        </p>
      ) : null}
    </SettingRow>
  );
}

function CloseToTrayRow() {
  const field = useSettingField<"app_close_to_tray">("app.close_to_tray");
  return (
    <SettingRow
      settingKey="app.close_to_tray"
      labelInControl
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <Toggle
          label={field.def.label}
          hint={field.def.effect}
          checked={field.value ?? field.defaultValue}
          onChange={(next) => field.commit(next)}
        />
      }
    />
  );
}

function AppearanceRow() {
  const field = useSettingField<"ui_appearance">("ui.appearance");
  return (
    <SettingRow
      settingKey="ui.appearance"
      labelInControl
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <Select
          label={field.def.label}
          hint={field.def.effect}
          options={APPEARANCE_OPTIONS.map((option) => ({
            value: option.value,
            label: option.label,
          }))}
          value={field.value ?? field.defaultValue}
          onChange={(event) =>
            field.commit(event.currentTarget.value as UiAppearance)
          }
        />
      }
    />
  );
}

export function GeneralCategory() {
  const settings = useSettings();
  const updates = useUpdateStatus();
  return (
    <CategoryState
      category="general"
      rows={3}
      state={categoryFrameState(settings, updates.isError)}
      error={{
        title: "Couldn’t read your general settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="A status check failed, so anything shown as live below may be out of date. Your settings loaded."
    >
      <StartupRow />
      <CloseToTrayRow />
      <AppearanceRow />
      <AppUpdates />
    </CategoryState>
  );
}
