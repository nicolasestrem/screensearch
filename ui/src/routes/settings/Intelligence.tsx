// Settings · Intelligence (`UI_REFERENCE §4`, `03 §8`): how enrichment, retrieval and
// sessions behave — the eight keys whose `Where` column names this category. Every
// row autosaves; the vision schedule pairs two keys in one control and writes them
// through `useSettingsPatch`, so one edit gets one toast and one undo for the pair.
import { useState } from "react";

import type { Settings } from "../../bindings/Settings";
import { ScheduleControl } from "../../components/domain";
import { Chip, Toggle } from "../../components/primitives";
import { useSettings, useSettingsDefaults, useTextFilterStats } from "../../lib/ipc/queries";
import { CategoryState } from "./CategoryState";
import { categoryFrameState, failureMessage } from "./frameState";
import { NumberFieldRow } from "./NumberFieldRow";
import { nextPendingToken } from "./pendingFieldValues";
import { clearedIfOwned } from "./pendingState";
import { SETTINGS_DEFAULTS, settingDef } from "./registry";
import { SettingRow } from "./SettingRow";
import { SuppressionReadout } from "./SuppressionReadout";
import { useSettingField, useSettingsPatch } from "./useSettingField";

const TIMER_KEY = "enrich.vision_timer_enabled" as const;
const INTERVAL_KEY = "enrich.vision_timer_interval_ms" as const;
const MS_PER_MINUTE = 60_000;

export function IntelligenceCategory() {
  const settings = useSettings();
  const defaultsQuery = useSettingsDefaults();
  // The combined schedule's changed markers and reset payloads read the backend's
  // as-built defaults (`get_settings_defaults`); the mirrored table only covers the
  // in-flight window, where it keeps today's behaviour.
  const defaults = defaultsQuery.data ?? SETTINGS_DEFAULTS;
  const stats = useTextFilterStats();
  const thinking = useSettingField<"answer_thinking">("answer.thinking");
  const chrome = useSettingField<"text_include_chrome_default">(
    "text.include_chrome_default",
  );
  const schedule = useSettingsPatch();
  // Only the range explanation lives here. It is deliberately separate from the save
  // outcome: `useSetSettings` is optimistic, so a comparison against `intervalMs` cannot
  // tell a persisted value from a pending one, and a toggle-only patch must not erase the
  // reason the drafted interval was left out (review, PR #162).
  const [rangeError, setRangeError] = useState<string | null>(null);
  // The interval is a draft until blur/Enter: `ScheduleControl` emits on every
  // keystroke, so committing directly made clearing the field throw the number back
  // and out-of-range entries vanish instead of staying correctable (review, PR #161).
  const [intervalDraft, setIntervalDraft] = useState<{
    value: number;
    issued: number;
  } | null>(null);

  const timerDef = settingDef(TIMER_KEY);
  const intervalDef = settingDef(INTERVAL_KEY);
  const timerEnabled = settings.data?.enrich_vision_timer_enabled ?? false;
  const intervalMs = settings.data?.enrich_vision_timer_interval_ms ?? 0;
  const minutes = Math.round(intervalMs / MS_PER_MINUTE);

  // Four frame states come from the shared resolver; `empty` for this category is
  // the schedule row's "nothing scheduled" status, so the rows never hide.
  const state = categoryFrameState(settings, stats.isError);

  /**
   * `ScheduleControl`'s toggle is a discrete decision and saves at once; the interval
   * field only updates the draft. A cleared field reports 0, which stays visible (and
   * correctable) rather than being replaced by the saved value.
   */
  function onScheduleChange(patch: Partial<Settings>): void {
    if (patch.enrich_vision_timer_interval_ms !== undefined) {
      // A fresh edit supersedes any draft, so it gets a fresh ownership token.
      setIntervalDraft({
        value: patch.enrich_vision_timer_interval_ms,
        issued: nextPendingToken(),
      });
      if (intervalInRange(patch.enrich_vision_timer_interval_ms)) setRangeError(null);
      return;
    }
    // The toggle saves at once, and carries any drafted interval with it: toggling
    // before blurring must not discard the number the user just typed.
    const drafted = intervalDraft;
    const typed = drafted !== null && drafted.value !== intervalMs ? drafted : null;
    if (typed !== null && !intervalInRange(typed.value)) {
      // The interval is invalid on its own, so it must not veto the toggle: save the
      // toggle, keep the number on screen, and keep the reason it was left out.
      setRangeError(rangeNote());
      void commitSchedule(patch);
      return;
    }
    const merged =
      typed === null
        ? patch
        : { ...patch, enrich_vision_timer_interval_ms: typed.value };
    void commitSchedule(merged).then((stored) => {
      // Retire only the draft this save carried, and only once it is stored. Clearing it up
      // front and restoring it on failure could resurrect a draft the user had since reset or
      // replaced; leaving it until success means there is nothing to resurrect (review, PR #162).
      if (!stored || typed === null) return;
      setIntervalDraft((current) =>
        current !== null && current.issued === typed.issued ? null : current,
      );
    });
  }

  /** Commit the drafted interval once the user is done with it. */
  function commitIntervalDraft(): void {
    if (intervalDraft === null) return;
    const { value: drafted, issued } = intervalDraft;
    if (drafted === intervalMs) {
      setIntervalDraft((current) => clearedIfOwned(current, issued));
      return;
    }
    void commitSchedule({ enrich_vision_timer_interval_ms: drafted }).then(
      (stored) => {
        // On failure the draft stays, so the number the user typed is still there to
        // correct beside the error — a rollback would contradict the message. On success
        // it retires only this draft: typing the next value while the save was queued
        // must not be undone by it.
        if (stored) setIntervalDraft((current) => clearedIfOwned(current, issued));
      },
    );
  }

  /**
   * `ScheduleControl` reports a cleared interval as 0 and lets a value above the
   * registry maximum through; the backend rejects both, so an interval outside the
   * range is explained inline instead of firing a write that cannot succeed.
   */
  function intervalInRange(ms: number): boolean {
    const range = intervalDef.range;
    if (!range) return true;
    return ms >= range.min && ms <= range.max;
  }

  function rangeNote(): string {
    const range = intervalDef.range;
    const low = range ? Math.ceil(range.min / MS_PER_MINUTE) : 0;
    const high = range ? Math.floor(range.max / MS_PER_MINUTE) : 0;
    return `${intervalDef.label} must be between ${low} and ${high} minutes. The interval was not saved.`;
  }

  function commitSchedule(patch: Partial<Settings>): Promise<boolean> {
    const nextInterval = patch.enrich_vision_timer_interval_ms;
    if (nextInterval !== undefined && !intervalInRange(nextInterval)) {
      setRangeError(rangeNote());
      return Promise.resolve(false);
    }
    // A patch that carries a valid interval supersedes the explanation; a toggle-only
    // patch says nothing about the interval, so its note stays until the draft changes.
    if (nextInterval !== undefined) setRangeError(null);
    // The toast names the key the edit actually touched; the undo behind it restores
    // every field the patch carried.
    return schedule.commitPatch(
      patch,
      patch.enrich_vision_timer_interval_ms !== undefined
        ? intervalDef.label
        : timerDef.label,
    );
  }

  return (
    <CategoryState
      category="intelligence"
      state={state}
      rows={8}
      error={{
        title: "Couldn’t read your intelligence settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="The per-app suppression readout is unavailable right now; every row below still saves."
    >
      <SettingRow
        settingKey="answer.thinking"
        labelInControl
        changed={thinking.changed}
        onReset={thinking.reset}
        saving={thinking.saving}
        error={thinking.error}
        onRetry={thinking.retry}
        control={
          <Toggle
            label={thinking.def.label}
            hint={thinking.def.effect}
            checked={thinking.value ?? thinking.defaultValue}
            onChange={thinking.commit}
          />
        }
      />

      {/*
        The timer pairs `enrich.vision_timer_enabled` with its interval, so one
        control owns both keys and reports a single patch. `ScheduleControl` renders
        its own labels (it takes no label/hint props), hence `labelInControl`.
      */}
      <SettingRow
        settingKey={TIMER_KEY}
        labelInControl
        changed={timerEnabled !== defaults.enrich_vision_timer_enabled}
        onReset={() => {
          setIntervalDraft(null);
          setRangeError(null);
          schedule.commitPatch(
            { enrich_vision_timer_enabled: defaults.enrich_vision_timer_enabled },
            timerDef.label,
          );
        }}
        saving={schedule.saving}
        error={schedule.error}
        onRetry={schedule.retry}
        status={
          timerEnabled
            ? `Scheduled — a background pass runs every ${minutes} minute${minutes === 1 ? "" : "s"}.`
            : "Nothing is scheduled — enrichment runs on demand, and tagging a single moment stays available."
        }
        control={
          <div
            onBlur={commitIntervalDraft}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                commitIntervalDraft();
              } else if (event.key === "Escape") {
                setIntervalDraft(null);
                setRangeError(null);
              }
            }}
          >
            <ScheduleControl
              timerEnabled={timerEnabled}
              timerIntervalMs={intervalDraft?.value ?? intervalMs}
              onChange={onScheduleChange}
            />
          </div>
        }
      />

      {/*
        The interval has no editor of its own: the schedule above edits it, so this
        row is a readback that keeps the key searchable, resettable and truthfully
        marked as changed rather than offering a second control for one value.
      */}
      <SettingRow
        settingKey={INTERVAL_KEY}
        changed={intervalMs !== defaults.enrich_vision_timer_interval_ms}
        onReset={() => {
          // The reset is a deliberate discard of the drafted number, so the draft and its
          // explanation retire with it — no effect watching the value, which cannot tell a
          // persisted write from an optimistic one.
          setIntervalDraft(null);
          setRangeError(null);
          schedule.commitPatch(
            { enrich_vision_timer_interval_ms: defaults.enrich_vision_timer_interval_ms },
            intervalDef.label,
          );
        }}
        saving={schedule.saving}
        error={rangeError ?? schedule.error}
        status={
          timerEnabled
            ? "Set by the schedule above."
            : "Set by the schedule above; it applies once the timer is on."
        }
        control={
          <Chip>
            <span className="font-mono text-t-data tabular-nums">
              {`${minutes} min`}
            </span>
          </Chip>
        }
      />

      <NumberFieldRow settingKey="reports.daily_top_k" />
      <NumberFieldRow settingKey="retrieval.default_top_k" />
      <NumberFieldRow settingKey="sessions.gap_close_secs" />
      <NumberFieldRow settingKey="sessions.min_len_secs" />

      <SettingRow
        settingKey="text.include_chrome_default"
        labelInControl
        changed={chrome.changed}
        onReset={chrome.reset}
        saving={chrome.saving}
        error={chrome.error}
        onRetry={chrome.retry}
        control={
          <Toggle
            label={chrome.def.label}
            hint={chrome.def.effect}
            checked={chrome.value ?? chrome.defaultValue}
            onChange={chrome.commit}
          />
        }
      >
        <SuppressionReadout />
      </SettingRow>
    </CategoryState>
  );
}
