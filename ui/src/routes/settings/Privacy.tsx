// Privacy (`UI_REFERENCE §4`, `03 §8`): the apps ScreenSearch never captures and
// the pause-while-locked switch. The §4 empty cell — "no excluded apps" — is the
// list's own invitation: it sits inside the row beside the add field, so the
// pause switch stays reachable.
import { useState } from "react";

import { Button, Chip, Field, Toggle } from "../../components/primitives";
import { useSettings } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { categoryFrameState, failureMessage } from "./frameState";
import { SettingRow } from "./SettingRow";
import { useSettingField } from "./useSettingField";

interface ExcludedAppsProps {
  /** Saved names, in the order the backend holds them. */
  apps: readonly string[];
  onAdd(name: string): void;
  onRemove(name: string): void;
}

/**
 * The exclusion list: one removable chip per app, plus an add field. An "add" is a
 * command, not a setting value, so it commits on Enter or the Add button rather
 * than blurring away a half-typed name.
 */
function ExcludedApps({ apps, onAdd, onRemove }: ExcludedAppsProps) {
  const [draft, setDraft] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const name = draft.trim();

  function add() {
    if (name.length === 0) return;
    const duplicate = apps.find(
      (app) => app.toLowerCase() === name.toLowerCase(),
    );
    if (duplicate) {
      setProblem(`“${duplicate}” is already excluded.`);
      return;
    }
    onAdd(name);
    setDraft("");
    setProblem(null);
  }

  return (
    <div className="flex min-w-0 flex-col gap-3">
      {apps.length === 0 ? (
        <p
          data-empty="no-exclusions"
          className="max-w-prose text-t-sm text-fg-secondary"
        >
          Nothing excluded yet. ScreenSearch always skips its own window; add an
          app below and screenshots pause while that app is in front.
        </p>
      ) : (
        <ul
          aria-label="Apps never captured"
          className="flex min-w-0 flex-wrap items-center gap-2"
        >
          {apps.map((app) => (
            <li key={app} className="min-w-0">
              <Chip
                onRemove={() => onRemove(app)}
                removeLabel={`Stop excluding ${app}`}
              >
                {app}
              </Chip>
            </li>
          ))}
        </ul>
      )}

      <div className="flex min-w-0 flex-wrap items-end gap-2">
        <div className="min-w-0 flex-1">
          <Field
            label="Add an app"
            placeholder="1Password"
            value={draft}
            hint="The app’s name as Windows shows it. Screenshots are skipped while it is in front."
            error={problem}
            onChange={(event) => {
              setDraft(event.currentTarget.value);
              setProblem(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                add();
              }
            }}
          />
        </div>
        <Button
          size="sm"
          variant="secondary"
          onClick={add}
          disabled={name.length === 0}
        >
          Add app
        </Button>
      </div>
    </div>
  );
}

function ExcludedAppsRow() {
  const field = useSettingField<"privacy_excluded_apps">(
    "privacy.excluded_apps",
  );
  const apps = field.value ?? [];
  return (
    <SettingRow
      settingKey="privacy.excluded_apps"
      changed={field.changed}
      onReset={field.reset}
      saving={field.saving}
      error={field.error}
      onRetry={field.retry}
      control={
        <ExcludedApps
          apps={apps}
          onAdd={(name) => field.commit([...apps, name])}
          onRemove={(name) =>
            field.commit(apps.filter((candidate) => candidate !== name))
          }
        />
      }
    />
  );
}

function PauseOnLockRow() {
  const field = useSettingField<"privacy_pause_on_lock">(
    "privacy.pause_on_lock",
  );
  return (
    <SettingRow
      settingKey="privacy.pause_on_lock"
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

export function PrivacyCategory() {
  const settings = useSettings();
  return (
    <CategoryState
      category="privacy"
      rows={2}
      state={categoryFrameState(settings)}
      error={{
        title: "Couldn’t read your privacy settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="The last settings refresh failed, so these rows may be out of date."
    >
      <ExcludedAppsRow />
      <PauseOnLockRow />
    </CategoryState>
  );
}
