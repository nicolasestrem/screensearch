// The Settings row column model (`UI_REFERENCE §3`): one fixed control column.
//
// 0.4.5 QA finding: in Settings > General the `Launch at startup` toggle sat
// further left than `Close to tray`. Both rows render their control into the
// row's control cell, and that cell was a content-sized flex item (`min-w-0`,
// no width). `Toggle` lays out as `flex items-start justify-between`, so inside
// a shrink-to-fit box `justify-between` has no slack: the switch landed
// immediately after the hint text, whose width differs per row. `Select` and
// `Field` sized themselves to their own content instead, so no two rows shared a
// column.
//
// The row now places every control in one column of a fixed width, taken from
// the density spacing scale, and never sizes that column from the row's copy or
// from the marker that appears beside it:
//
//   - `controlSlot` is the same class string for both variants: full width at
//     narrow layouts, the `md:w-96` token (`--space-96`, 384px) from `md` up, and
//     `max-w-full` so a narrow grid cell clamps the column instead of letting it
//     overflow. A `Toggle` inside it spans the whole column, so its
//     `justify-between` puts the switch on that column's right edge on every row;
//     a `Select` or `Field` fills the same column. No width utility here is
//     content-derived (`w-auto`, `w-fit`, `w-max`, `basis-auto`), so neither the
//     copy nor the `Changed` marker can move it.
//   - `controlCell` stays a wrapping flex line, so the marker, `Reset to
//     default` and the `Saving…` live region sit after the column and wrap to a
//     second line instead of squeezing it.

export type SettingRowVariant = "stacked" | "split";

export interface SettingRowLayoutInput {
  /** The control renders its own visible label, so the row stacks both cells. */
  labelInControl: boolean;
}

export interface SettingRowLayout {
  variant: SettingRowVariant;
  /** Grid cell holding the control column plus the marker, reset and live status. */
  controlCell: string;
  /** The fixed column every control is placed in. */
  controlSlot: string;
}

/**
 * The control column. `md:w-96` is a density spacing token, not a literal: the
 * `96` key resolves to `var(--space-96)` in `styles/tokens.css`.
 */
export const SETTING_CONTROL_SLOT_CLASS =
  "min-w-0 w-full max-w-full md:w-96";

const CONTROL_CELL: Record<SettingRowVariant, string> = {
  stacked: "col-span-12 order-1 flex min-w-0 flex-wrap items-center gap-2",
  split:
    "col-span-12 flex min-w-0 flex-wrap items-center gap-2 md:col-span-5 md:justify-end",
};

export function settingRowVariant(labelInControl: boolean): SettingRowVariant {
  return labelInControl ? "stacked" : "split";
}

/**
 * Resolve the row's control placement. The variant is the only input: the column
 * must not depend on the label, the effect line or any live state.
 */
export function settingRowLayout({
  labelInControl,
}: SettingRowLayoutInput): SettingRowLayout {
  const variant = settingRowVariant(labelInControl);
  return {
    variant,
    controlCell: CONTROL_CELL[variant],
    controlSlot: SETTING_CONTROL_SLOT_CLASS,
  };
}
