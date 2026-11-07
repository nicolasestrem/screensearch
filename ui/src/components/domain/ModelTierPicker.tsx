// ModelTierPicker (UI_REFERENCE §5) — a per-lane segmented control for the model
// tier (Default / Quality, `00 §E`; 0.3.0 retired Beta). It owns the choice, not the persistence:
// the parent decides what a change means (Settings fires `set_model_tier` for an
// immediate hot-apply, then folds the value into the saved form). Mirrors the
// segmented range presets used on the Timeline so the instrument language is shared.
import type { ReactNode } from "react";

import type { ModelLane } from "../../bindings/ModelLane";
import type { ModelTier } from "../../bindings/ModelTier";
import { Tooltip } from "../primitives";
import { cn } from "../../lib/cn";

const TIERS: { value: ModelTier; label: string }[] = [
  { value: "default", label: "Default" },
  { value: "quality", label: "Quality" },
];

const LANE_LABEL: Record<ModelLane, string> = {
  vision: "Vision model",
  answer: "Answer model",
};

// The actual model behind each (lane, tier), so a hover/focus tooltip tells the user what
// "Default / Quality" resolve to. Mirrors the source of truth in
// `crates/inference/src/models.rs::repo_for` (and `specs/MODEL_REGISTRY.md`) — display
// names only (repo org + `-GGUF` suffix dropped); keep in sync if the registry changes.
const MODEL_NAMES: Record<ModelLane, Record<ModelTier, string>> = {
  vision: {
    default: "Qwen3-VL-4B-Instruct",
    quality: "Qwen3-VL-8B-Instruct",
  },
  answer: {
    default: "Ministral-3-3B-Reasoning",
    quality: "Qwen3-4B-Thinking",
  },
};

export interface ModelTierPickerProps {
  lane: ModelLane;
  value: ModelTier;
  onChange: (tier: ModelTier) => void;
  /** Helper text below the control (e.g. "Applies now"). */
  hint?: ReactNode;
  disabled?: boolean;
}

export function ModelTierPicker({
  lane,
  value,
  onChange,
  hint,
  disabled = false,
}: ModelTierPickerProps) {
  const label = LANE_LABEL[lane];
  return (
    <div className="flex flex-col gap-2">
      <span className="text-t-xs text-fg-secondary">{label}</span>
      <div role="group" aria-label={`${label} tier`} className="flex gap-1">
        {TIERS.map((t) => (
          <Tooltip
            key={t.value}
            label={MODEL_NAMES[lane][t.value]}
            side="bottom"
          >
            <button
              type="button"
              aria-pressed={value === t.value}
              aria-label={`${t.label} — ${MODEL_NAMES[lane][t.value]}`}
              disabled={disabled}
              onClick={() => onChange(t.value)}
              className={cn(
                "inline-flex items-center rounded-sm px-3 min-h-hit-min font-text text-t-xs font-semibold",
                "transition-colors duration-fast ease-standard motion-reduce:transition-none disabled:opacity-50 disabled:pointer-events-none",
                value === t.value
                  ? "bg-accent-wash text-accent"
                  : "text-fg-secondary hover:text-fg-primary hover:bg-surface-raised",
              )}
            >
              {t.label}
            </button>
          </Tooltip>
        ))}
      </div>
      {hint && <span className="text-t-xs text-fg-tertiary">{hint}</span>}
    </div>
  );
}
