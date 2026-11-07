// The one Settings row (UI_REFERENCE §3): registry label, one-line plain-language
// effect, the control slot, a "Changed" marker with reset-to-default, live
// status, and a failed-save explanation with Retry. `SettingSection` is the same
// chrome for rows that are an action or a status rather than one of the keys.
// Every control sits in one fixed column (`settingRowLayout`), so switches,
// selects and fields line up across a category whatever the copy beside them is.

import type { ReactNode } from "react";

import { Button, Chip } from "../../components/primitives";
import { cn } from "../../lib/cn";
import { settingAnchorId, settingDef, type SettingKey } from "./registry";
import { settingRowLayout } from "./settingRowLayout";

export interface SettingRowProps {
  /** Label, effect and anchor id come from the registry. */
  settingKey: SettingKey;
  /** The control — a P2 primitive or an imported domain component. */
  control: ReactNode;
  /** Saved value differs from the default: show the marker and a reset action. */
  changed?: boolean;
  onReset?: () => void;
  /** Live inline status (hotkey registered, API listening, model loaded…). */
  status?: ReactNode;
  /** Plain-language failed-save message; the typed value stays in the control. */
  error?: string | null;
  onRetry?: () => void;
  saving?: boolean;
  /** Extra detail under the row (read-only readouts, warnings). */
  children?: ReactNode;
  /**
   * The control is a P2 primitive that renders its own visible label
   * (`Toggle`, `Field`, `Select`, `Segmented`, `HotkeyField`, …): this row then
   * omits its heading so the label is not printed twice, and the control must
   * receive `def.label` as its label and `def.effect` as its `hint`. The marker,
   * status, failure text and detail stay in the row chrome.
   */
  labelInControl?: boolean;
}

export function SettingRow({
  settingKey,
  control,
  changed = false,
  onReset,
  status,
  error,
  onRetry,
  saving = false,
  children,
  labelInControl = false,
}: SettingRowProps) {
  const def = settingDef(settingKey);
  const layout = settingRowLayout({ labelInControl });
  return (
    <div
      id={settingAnchorId(settingKey)}
      tabIndex={-1}
      aria-busy={saving || undefined}
      className={cn(
        "grid min-h-row min-w-0 grid-cols-12 items-start gap-gap",
        "border-b border-line-subtle py-3 outline-none last:border-b-0",
      )}
    >
      <div className={cn("col-span-12 min-w-0", labelInControl ? "order-2" : "md:col-span-7")}>
        {labelInControl ? null : (
          <>
            <h2 className="text-t-ui font-medium text-fg-primary">{def.label}</h2>
            <p className="mt-1 max-w-prose text-t-sm text-fg-secondary">
              {def.effect}
            </p>
          </>
        )}
        {status ? (
          <div className="mt-1 min-w-0 text-t-xs text-fg-tertiary">{status}</div>
        ) : null}
        {error ? (
          <div
            role="alert"
            className="mt-2 flex min-w-0 flex-wrap items-center gap-2 text-t-sm text-status-danger-fg"
          >
            <span className="min-w-0 flex-1">{error}</span>
            {onRetry ? (
              <Button size="sm" variant="ghost" onClick={onRetry}>
                Retry
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
      <div className={layout.controlCell}>
        <div className={layout.controlSlot}>{control}</div>
        {changed ? <Chip tone="neutral">Changed</Chip> : null}
        {changed && onReset ? (
          <Button size="sm" variant="ghost" onClick={onReset}>
            Reset to default
          </Button>
        ) : null}
        <span aria-live="polite" className="text-t-xs text-fg-tertiary">
          {saving ? "Saving…" : ""}
        </span>
      </div>
      {children ? <div className="col-span-12 min-w-0">{children}</div> : null}
    </div>
  );
}

export interface SettingSectionProps {
  title: string;
  description?: string;
  children: ReactNode;
}

/** A group inside a category: actions, a status readout, an embedded panel. */
export function SettingSection({
  title,
  description,
  children,
}: SettingSectionProps) {
  return (
    <section className="flex min-w-0 flex-col gap-2 border-b border-line-subtle py-3 last:border-b-0">
      <div className="min-w-0">
        <h2 className="text-t-ui font-medium text-fg-primary">{title}</h2>
        {description ? (
          <p className="mt-1 max-w-prose text-t-sm text-fg-secondary">
            {description}
          </p>
        ) : null}
      </div>
      <div className="min-w-0">{children}</div>
    </section>
  );
}
