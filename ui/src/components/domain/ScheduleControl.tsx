// ScheduleControl (UI_REFERENCE §5) — the deferred-vision enrichment schedule.
// Vision tagging is never real-time (`03 §5`): on-demand tagging (from a Moment) is
// always available; the timer lane is the one editable opt-in (`03 §8` settings v2,
// 0.4.5 D6). The idle lane and its batch size are `file-only` keys now
// (`enrich.vision_idle_*`, `enrich.vision_batch_size`), tuned in `advanced.toml` -
// they deliberately have no control here. The timer threshold is surfaced in minutes
// for readability and converted to the stored unit (ms) on edit, and applies on the
// next app restart, labelled honestly here.
import { Field, Toggle } from "../primitives";

export interface ScheduleControlProps {
  timerEnabled: boolean;
  timerIntervalMs: number;
  onChange: (
    patch: Partial<{
      enrich_vision_timer_enabled: boolean;
      enrich_vision_timer_interval_ms: number;
    }>,
  ) => void;
}

export function ScheduleControl({
  timerEnabled,
  timerIntervalMs,
  onChange,
}: ScheduleControlProps) {
  // Shown in minutes; a cleared field shows 0 (a transient value to type over) rather
  // than snapping back - the Settings save clamps the threshold to its valid range.
  const timerMinutes = Math.round(timerIntervalMs / 60_000);

  return (
    <div className="flex flex-col gap-4">
      <p className="text-caption text-ink-faint font-body">
        Vision tagging never runs in real time. On-demand tagging (from a
        moment) is always available — turn on a schedule to also tag untagged
        frames in the background.
      </p>

      <Toggle
        label="Tag on a timer"
        checked={timerEnabled}
        onChange={(v) => onChange({ enrich_vision_timer_enabled: v })}
        hint="Periodically tag a batch of untagged frames."
      />
      {timerEnabled && (
        <Field
          label="Timer interval (minutes)"
          type="number"
          min={1}
          value={timerMinutes}
          onChange={(e) => {
            const m = e.currentTarget.valueAsNumber;
            if (Number.isFinite(m) && m >= 1) {
              onChange({
                enrich_vision_timer_interval_ms: Math.round(m) * 60_000,
              });
            } else if (e.currentTarget.value === "") {
              onChange({ enrich_vision_timer_interval_ms: 0 });
            }
          }}
          hint="Applies on restart."
        />
      )}
    </div>
  );
}
