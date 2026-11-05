// Settings · Advanced (`UI_REFERENCE §4`, `03 §8`): the one `UI` key that lives
// here — the throttle master switch with its live readout — above the read-only
// projection of every `file-only` key (`get_advanced_config`). Nothing in this
// category writes the tuning file: it is edited by hand and read once at startup,
// so a change applies after a restart.
//
// The `empty` cell of `UI_REFERENCE §4` ("no tuning keys set → everything is at its
// default") is stated inside this frame. The file list is the only content that can
// be empty, and the throttle row is a control that stays reachable either way.
import { Button, Toggle } from "../../components/primitives";
import { useAdvancedConfig, useSettings } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { FileOnlyList } from "./FileOnlyList";
import { SettingRow, SettingSection } from "./SettingRow";
import { ThrottleReadout } from "./ThrottleReadout";
import { useSettingField } from "./useSettingField";

export function AdvancedCategory() {
  const settings = useSettings();
  const advanced = useAdvancedConfig();
  const throttle = useSettingField<"throttle_enabled">("throttle.enabled");

  // Data first here too: a failed background refetch keeps the last payload, so it is
  // a partial state (the rows stay) rather than a category-wide error panel.
  if (settings.data === undefined) {
    if (settings.isError) {
      return (
        <CategoryState
          category="advanced"
          state="error"
          error={{
            title: "Couldn’t load the throttle setting",
            message:
              "The saved value couldn’t be read, so the switch can’t show what it currently is. Nothing was changed.",
            onRetry: () => {
              settings.refetch();
              advanced.refetch();
            },
          }}
        />
      );
    }
    return <CategoryState category="advanced" state="loading" rows={2} />;
  }
  if (throttle.value === undefined) {
    return (
      <CategoryState
        category="advanced"
        state="error"
        error={{
          title: "Couldn’t load the throttle setting",
          message:
            "The saved value couldn’t be read, so the switch can’t show what it currently is. Nothing was changed.",
          onRetry: () => {
            settings.refetch();
            advanced.refetch();
          },
        }}
      />
    );
  }
  if (advanced.data === undefined && advanced.isError) {
    return (
      <CategoryState
        category="advanced"
        state="error"
        error={{
          title: "Couldn’t load the tuning values",
          message:
            "The advanced values couldn’t be read, so no row can show what it currently is. Nothing was changed.",
          onRetry: () => advanced.refetch(),
        }}
      />
    );
  }
  if (advanced.isPending) {
    return <CategoryState category="advanced" state="loading" rows={2} />;
  }

  const view = advanced.data;
  const fromFile = view?.from_file.length ?? 0;
  // Counted from the projection itself, so a section or field the backend drops
  // (`03 §8` load rule) needs no change here.
  const total =
    view === undefined
      ? 0
      : Object.values(view.config).reduce(
          (count, section) => count + Object.keys(section).length,
          0,
        );
  const fileUnreadable = advanced.isError;
  const partlyFromFile = !fileUnreadable && fromFile > 0 && fromFile < total;
  const frameState: "partial" | "populated" =
    fileUnreadable || partlyFromFile ? "partial" : "populated";
  const partialNote = fileUnreadable
    ? "The tuning file couldn’t be read, so its values are missing here; the compiled defaults still apply. The throttle switch below still saves."
    : `${fromFile} of ${total} values come from the file; the rest hold their built-in defaults.`;

  return (
    <CategoryState
      category="advanced"
      state={frameState}
      partialNote={partialNote}
    >
      <SettingRow
        settingKey="throttle.enabled"
        labelInControl
        changed={throttle.changed}
        onReset={() => throttle.reset()}
        saving={throttle.saving}
        error={throttle.error}
        onRetry={throttle.retry}
        control={
          <Toggle
            label={throttle.def.label}
            hint={throttle.def.effect}
            checked={throttle.value === true}
            onChange={(next) => throttle.commit(next)}
          />
        }
      >
        <ThrottleReadout enabled={throttle.value === true} />
      </SettingRow>

      <SettingSection
        title="Tuning file values"
        description="The file is the control surface: these are not editable here and there is no Save."
      >
        {view !== undefined && fromFile === 0 ? (
          <p
            role="status"
            data-state="empty"
            className="mb-3 min-w-0 border-l-2 border-line-strong bg-surface-inset px-3 py-2 text-t-sm text-fg-secondary"
          >
            Everything is at its default — the tuning file sets none of these values.
          </p>
        ) : null}
        {view !== undefined ? <FileOnlyList view={view} /> : null}
        {fileUnreadable ? (
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <span className="text-t-sm text-status-warn-fg">
              The tuning file couldn’t be read. The compiled defaults still apply.
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => advanced.refetch()}
            >
              Retry
            </Button>
          </div>
        ) : null}
      </SettingSection>
    </CategoryState>
  );
}
