// Settings · Models (`UI_REFERENCE §4`, `03 §8`): which local model each lane uses,
// what the engine currently holds, and the setup operations behind them. The tier
// picks keep their existing hot-apply path (`set_model_tier`), so a change lands
// immediately and its undo returns through the same command.
import type { ModelLane } from "../../bindings/ModelLane";
import type { SidecarStatus } from "../../bindings/SidecarStatus";
import { ModelPanel, ModelTierPicker, SetupDetails } from "../../components/domain";
import { useReadiness, useSettings, useSetupStatus, useSidecarStatus } from "../../lib/ipc/queries";
import { setupDetailsSteps } from "../../lib/setup";
import { sidecarStateLabel, statusLabel } from "../../lib/status";
import { CategoryState } from "./CategoryState";
import { categoryFrameState, failureMessage } from "./frameState";
import { SettingRow, SettingSection } from "./SettingRow";
import { useSettingField } from "./useSettingField";

const LANE_NAME: Record<ModelLane, string> = {
  answer: "Answer model",
  vision: "Vision model",
};

/** One truthful line about what the engine holds for a lane right now. */
function laneStatusText(
  lane: ModelLane,
  status: SidecarStatus | null,
  detail: string | null,
): string {
  const name = LANE_NAME[lane];
  if (!status) {
    return detail
      ? `${name} isn’t loaded — ${detail}.`
      : `${name} isn’t loaded; it loads the first time it is needed.`;
  }
  if (status.lane !== lane) {
    if (status.lane === null) {
      return `${name} isn’t loaded — the engine is ${sidecarStateLabel(status.state).toLowerCase()}.`;
    }
    return `${name} isn’t loaded — the engine currently holds the ${LANE_NAME[status.lane].toLowerCase()}.`;
  }
  const model = status.model ? ` — ${status.model}` : "";
  if (status.state === "ready") return `Loaded${model}.`;
  if (status.state === "evicted" || status.state === "stopped") {
    return `Not resident${model}; it reloads when needed.`;
  }
  return `${sidecarStateLabel(status.state)}${model}.`;
}

function LaneStatus({ lane }: { lane: ModelLane }) {
  const sidecar = useSidecarStatus();
  const readiness = useReadiness();

  if (sidecar.isError) {
    const engine = readiness.data?.sidecar;
    return (
      <>
        {engine
          ? `Engine ${statusLabel(engine.status).toLowerCase()}${engine.detail ? ` — ${engine.detail}` : ""}.`
          : "Engine status is unavailable right now."}
      </>
    );
  }
  return (
    <>
      {laneStatusText(lane, sidecar.data ?? null, readiness.data?.sidecar.detail ?? null)}
    </>
  );
}

export function ModelsCategory() {
  const settings = useSettings();
  const readiness = useReadiness();
  const setup = useSetupStatus();
  const answer = useSettingField<"models_answer_tier">("models.answer_tier");
  const vision = useSettingField<"models_vision_tier">("models.vision_tier");

  // Four frame states come from the shared resolver. `empty` for this category is
  // the setup section keeping zero presence (no heading, no chip) while the tier
  // rows stay; its own states come from `get_setup_status` inside `SetupDetails`.
  const setupSteps = setupDetailsSteps(setup.data);
  const showSetup = setup.isPending || setup.isError || setupSteps.length > 0;

  const state = categoryFrameState(settings, readiness.isError);

  return (
    <CategoryState
      category="models"
      state={state}
      rows={2}
      error={{
        title: "Couldn’t read your model settings",
        message: failureMessage(settings.error),
        onRetry: () => void settings.refetch(),
      }}
      partialNote="Live engine status is unavailable, so the lines below may be out of date. The tiers still apply when you change them."
    >
      <SettingRow
        settingKey="models.answer_tier"
        changed={answer.changed}
        onReset={answer.reset}
        saving={answer.saving}
        error={answer.error}
        onRetry={answer.retry}
        status={<LaneStatus lane="answer" />}
        control={
          <ModelTierPicker
            lane="answer"
            value={answer.value ?? answer.defaultValue}
            onChange={answer.commit}
            hint="Applies now."
          />
        }
      />

      <SettingRow
        settingKey="models.vision_tier"
        changed={vision.changed}
        onReset={vision.reset}
        saving={vision.saving}
        error={vision.error}
        onRetry={vision.retry}
        status={<LaneStatus lane="vision" />}
        control={
          <ModelTierPicker
            lane="vision"
            value={vision.value ?? vision.defaultValue}
            onChange={vision.commit}
            hint="Applies now."
          />
        }
      />

      <SettingSection
        title="Model engine"
        description="What the local engine holds right now, with a manual load or unload. Loading keeps the next answer instant; unloading frees video memory."
      >
        <ModelPanel />
      </SettingSection>

      {showSetup ? (
        <SettingSection
          title="Setup"
          description="Model preparation running in this session, one operation per row, with the next step when one fails."
        >
          <SetupDetails />
        </SettingSection>
      ) : null}
    </CategoryState>
  );
}
