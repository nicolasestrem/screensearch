// Settings · Data (`UI_REFERENCE §4`, `03 §8`): what is stored, for how long, and how
// it leaves the machine. Storage rows autosave like every other row; export and the
// memory reports are actions rather than keys, so they sit in sections of their own.
import { RetentionControl } from "../../components/domain";
// The barrel does not export MemoryPanel; the deep import is the documented way to
// reach it (see `components/domain/index.ts`).
import { MemoryPanel } from "../../components/domain/MemoryPanel";
import { useSettings, useStorageStats } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { ExportRow } from "./ExportRow";
import { formatBytes } from "./formatBytes";
import { categoryFrameState, failureMessage } from "./frameState";
import { NumberFieldRow } from "./NumberFieldRow";
import { SettingRow, SettingSection } from "./SettingRow";
import { useSettingField } from "./useSettingField";

export function DataCategory() {
  const settings = useSettings();
  const storage = useStorageStats();
  const retention = useSettingField<"storage_retention_days">("storage.retention_days");

  // Four frame states come from the shared resolver; `empty` for this category is
  // the embedded report panel's own "nothing saved yet" copy, not a blank pane.
  const state = categoryFrameState(settings, storage.isError);

  const footprint = storage.data
    ? `Screenshots use ${formatBytes(storage.data.frame_bytes)}; the database ${formatBytes(storage.data.db_bytes)}.`
    : storage.isPending
      ? "Reading the storage footprint…"
      : null;

  return (
    <CategoryState
      category="data"
      state={state}
      rows={2}
      error={{
        title: "Couldn’t read your storage settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="The storage footprint couldn’t be read, so the sizes below are missing. Every row still saves."
    >
      <NumberFieldRow
        settingKey="storage.max_width"
        zeroNote="0 — every capture keeps its native width; nothing is scaled."
      />

      <SettingRow
        settingKey="storage.retention_days"
        labelInControl
        changed={retention.changed}
        onReset={retention.reset}
        saving={retention.saving}
        error={retention.error}
        onRetry={retention.retry}
        status={footprint}
        control={
          <RetentionControl
            days={retention.value ?? retention.defaultValue}
            onChange={retention.commit}
          />
        }
      />

      <ExportRow />

      {/*
        The one deliberate exception to the row pattern (maintainer decision,
        2026-11-05): MemoryPanel is a self-contained surface with its own draft, its
        own save button, its own run controls and its own report history. It is
        embedded whole and unmodified rather than converted into rows, so there is
        one implementation of report scheduling and delivery. Its `empty` case is
        also in-frame — with no saved runs the panel says so and still explains the
        setup above it.
      */}
      <SettingSection
        title="Memory and reports"
        description="Daily and weekly reports, Telegram digests and the saved report history. This panel ships as it is, so it keeps its own save button instead of one row per field."
      >
        <MemoryPanel />
      </SettingSection>
    </CategoryState>
  );
}
