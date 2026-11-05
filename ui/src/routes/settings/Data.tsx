// Settings · Data (`UI_REFERENCE §4`, `03 §8`): what is stored, for how long, and how
// it leaves the machine. Storage rows autosave like every other row; export and the
// memory reports are actions rather than keys, so they sit in sections of their own.
// The barrel does not export MemoryPanel; the deep import is the documented way to
// reach it (see `components/domain/index.ts`).
import { MemoryPanel } from "../../components/domain/MemoryPanel";
import { useSettings, useStorageStats } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { ExportRow } from "./ExportRow";
import { formatBytes } from "./formatBytes";
import { categoryFrameState, failureMessage } from "./frameState";
import { NumberFieldRow } from "./NumberFieldRow";
import { SettingSection } from "./SettingRow";

export function DataCategory() {
  const settings = useSettings();
  const storage = useStorageStats();

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
        {footprint ? (
          <p className="min-w-0 text-t-xs text-fg-tertiary">{footprint}</p>
        ) : null}
        <NumberFieldRow
        settingKey="storage.max_width"
        zeroNote="0 — every capture keeps its native width; nothing is scaled."
      />

      {/*
        Retention is a NumberFieldRow, not `RetentionControl`: that component calls
        `onChange` on every keystroke, so a user replacing 30 with 365 would persist
        a three-day window mid-typing, and the hourly retention sweep would delete
        the screenshots it covers — irreversibly, before the edit is even finished
        (P7 review, PR #161). The row keeps a draft, validates it and commits on
        Enter or blur, which is the same contract as every other numeric setting.
      */}
      <NumberFieldRow
        settingKey="storage.retention_days"
        zeroNote="0 — screenshots are kept forever; only their text stays searchable."
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
