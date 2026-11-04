// Slider - a bounded numeric setting with a track and a numeric field beside it.
// The product rule is dual-purpose (docs/0.4.5.md P2 item 5): the track is for
// scanning and coarse dragging, the numeric field is for entering an exact
// value, and the two always show one number. The numeric field owns the spoken
// value (`aria-valuetext` on the track reads the field's text), arrows step,
// Home/End jump to the bounds, and a value that cannot be trusted is exposed via
// `aria-invalid` plus a message row.
//
// Styling: the track is a 4px groove centred inside the 32px control box and the
// thumb is a 12px pill, both built from the token spacing scale so density
// changes move them. Every pseudoelement colour is a semantic token; reduced
// motion kills the skeleton pulse.
import {
  useId,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import { IconAlert, IconInfo } from "./icons";
import { cn } from "../../lib/cn";

/** Static state for the /kitchen sheet; mirrors the live interaction states. */
export type SliderPreview = "hover" | "active" | "focus";

/** The five views a data-bearing primitive must be able to take (UI_REFERENCE §4). */
export type SliderState = "loading" | "empty" | "error" | "partial" | "populated";

const FOCUS_RING =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]";

const SKELETON = "animate-pulse bg-surface-inset motion-reduce:animate-none";

export interface SliderProps {
  /** Control label (sentence case, e.g. "Match threshold"). */
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  /** Appended to the spoken value, e.g. "%" or "frames". */
  unit?: string;
  /** Helper text below the row. */
  hint?: ReactNode;
  /** Message shown when the value is invalid; also sets `aria-invalid`. */
  error?: string | null;
  disabled?: boolean;
  /** Which view to render. Defaults to `populated`. */
  state?: SliderState;
  /** Renders `data-preview` so states can be inspected statically. */
  preview?: SliderPreview;
  className?: string;
  /** Field name for form submission of the numeric input. */
  name?: string;
}

/** Track styling: 4px groove, 12px pill thumb, both token-spaced. */
const TRACK = [
  "h-control w-full min-w-0 flex-1 cursor-pointer appearance-none bg-transparent",
  FOCUS_RING,
  "disabled:cursor-not-allowed",
  // WebKit / Blink
  "[&::-webkit-slider-runnable-track]:h-1 [&::-webkit-slider-runnable-track]:rounded-pill",
  "[&::-webkit-slider-runnable-track]:bg-surface-inset",
  "[&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3",
  "[&::-webkit-slider-thumb]:rounded-pill [&::-webkit-slider-thumb]:border",
  "[&::-webkit-slider-thumb]:border-line-strong [&::-webkit-slider-thumb]:bg-action-primary",
  "[&::-webkit-slider-thumb]:-mt-1",
  // Firefox
  "[&::-moz-range-track]:h-1 [&::-moz-range-track]:rounded-pill [&::-moz-range-track]:bg-surface-inset",
  "[&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:rounded-pill",
  "[&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:bg-action-primary",
  // Interaction twins: live states and their static /kitchen mirrors.
  "hover:[&::-webkit-slider-thumb]:bg-action-primary-hover",
  "group-data-[preview=hover]:[&::-webkit-slider-thumb]:bg-action-primary-hover",
  "hover:[&::-moz-range-thumb]:bg-action-primary-hover",
  "group-data-[preview=hover]:[&::-moz-range-thumb]:bg-action-primary-hover",
  "active:[&::-webkit-slider-thumb]:bg-action-primary-hover",
  "group-data-[preview=active]:[&::-webkit-slider-thumb]:bg-action-primary-hover",
  "disabled:[&::-webkit-slider-thumb]:bg-fg-disabled",
  "disabled:[&::-moz-range-thumb]:bg-fg-disabled",
].join(" ");

const NUMERIC = [
  "h-control w-24 shrink-0 rounded-md border bg-surface-inset px-2 text-right",
  "text-t-data font-mono tabular-nums text-fg-primary",
  "transition-colors duration-fast ease-standard",
  "border-line-control hover:border-line-strong",
  "group-data-[preview=hover]:border-line-strong",
  FOCUS_RING,
  "disabled:cursor-not-allowed disabled:border-line-subtle disabled:text-fg-disabled",
].join(" ");

export function Slider({
  label,
  value,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  unit,
  hint,
  error,
  disabled = false,
  state = "populated",
  preview,
  className,
  name,
}: SliderProps) {
  const id = useId();
  const labelId = `${id}-label`;
  const rangeId = `${id}-range`;
  const numberId = `${id}-number`;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;

  // The numeric field keeps whatever the user typed until it loses focus, so a
  // half-typed value is not rewritten under the caret.
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? String(Number(value.toFixed(3)));
  // `state="error"` is a view, so it carries a message even before a caller has
  // one to show; an explicit message always wins.
  const errorText =
    state === "error" ? (error ?? "Couldn't read this value.") : error;
  const invalid = Boolean(errorText);

  const describedBy =
    [hint && !errorText ? hintId : null, errorText ? errorId : null]
      .filter(Boolean)
      .join(" ") || undefined;

  function clamp(next: number): number {
    const bounded = Math.min(max, Math.max(min, next));
    const snapped = min + Math.round((bounded - min) / step) * step;
    // Trim float noise from the step grid: 0.1 steps otherwise read 0.30000000000000004.
    return Number(Math.min(max, Math.max(min, snapped)).toFixed(3));
  }

  function commit(next: number) {
    if (!Number.isFinite(next)) return;
    const bounded = clamp(next);
    if (bounded !== value) onChange(bounded);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    let next: number | null = null;
    if (event.key === "ArrowUp") next = value + step;
    else if (event.key === "ArrowDown") next = value - step;
    else if (event.key === "Home") next = min;
    else if (event.key === "End") next = max;
    if (next === null) return;
    event.preventDefault();
    setDraft(null);
    commit(next);
  }

  const labelClass = cn(
    "text-t-ui font-medium",
    disabled ? "text-fg-disabled" : "text-fg-secondary",
  );

  // Symbols attach ("40%"), words are separated ("12 frames").
  const valueText =
    unit === undefined
      ? shown
      : /^[%°‰]$/.test(unit)
        ? `${shown}${unit}`
        : `${shown} ${unit}`;

  const row = (
    <div className="flex items-center gap-3">
      <input
        id={rangeId}
        type="range"
        className={TRACK}
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-valuetext={valueText}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onChange={(event) => {
          setDraft(null);
          commit(Number(event.target.value));
        }}
        onKeyDown={onKeyDown}
      />
      <input
        id={numberId}
        type="number"
        inputMode="decimal"
        name={name}
        className={cn(NUMERIC, invalid && "border-status-danger-border")}
        min={min}
        max={max}
        step={step}
        value={shown}
        disabled={disabled}
        aria-labelledby={labelId}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onChange={(event) => {
          const text = event.target.value;
          setDraft(text);
          if (text.trim() === "") return;
          const parsed = Number(text);
          if (Number.isFinite(parsed)) commit(parsed);
        }}
        onBlur={() => setDraft(null)}
        onKeyDown={onKeyDown}
      />
    </div>
  );

  if (state === "loading") {
    return (
      <div
        className={cn("group flex flex-col gap-2", className)}
        data-preview={preview}
        data-state="loading"
        aria-busy="true"
      >
        <span id={labelId} className={labelClass}>
          {label}
        </span>
        <div className="flex items-center gap-3" aria-hidden="true">
          <div className={cn("h-1 flex-1 rounded-pill", SKELETON)} />
          <div className={cn("h-control w-24 rounded-md", SKELETON)} />
        </div>
        <span className={cn("h-3 w-24 rounded-sm", SKELETON)} aria-hidden="true" />
      </div>
    );
  }

  if (state === "empty") {
    return (
      <div
        className={cn("group flex flex-col gap-2", className)}
        data-preview={preview}
        data-state="empty"
      >
        <span id={labelId} className={labelClass}>
          {label}
        </span>
        <p className="text-t-sm text-fg-tertiary">Nothing to adjust yet.</p>
      </div>
    );
  }

  return (
    <div
      className={cn("group flex flex-col gap-2", className)}
      data-preview={preview}
      data-state={state}
    >
      <label id={labelId} htmlFor={rangeId} className={labelClass}>
        {label}
      </label>
      {row}
      {state === "partial" && !errorText && (
        <p className="flex items-start gap-1.5 text-t-xs text-fg-tertiary">
          <IconInfo size={14} className="mt-0.5 shrink-0" />
          <span>Partial value. The rest is still loading.</span>
        </p>
      )}
      {hint && !errorText && (
        <p id={hintId} className="text-t-xs text-fg-tertiary">
          {hint}
        </p>
      )}
      {errorText && (
        <p
          id={errorId}
          role="alert"
          className="flex items-start gap-1.5 text-t-xs text-status-danger-fg"
        >
          <IconAlert size={14} className="mt-0.5 shrink-0" />
          <span>{errorText}</span>
        </p>
      )}
    </div>
  );
}
