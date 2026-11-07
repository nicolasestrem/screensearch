import { useId, useState, type KeyboardEvent } from "react";

import { Button } from "../primitives";
import { cn } from "../../lib/cn";

// "Ctrl+Alt+Z": the original "Ctrl+Alt+Space" collided with Claude Desktop's quick-entry
// shortcut. Must match the Rust default (`Settings::default().overlay_hotkey`).
export const DEFAULT_OVERLAY_HOTKEY = "Ctrl+Alt+Z";
export const DEFAULT_MARKS_HOTKEY = "Ctrl+Alt+M";

const MODIFIER_CODES = new Set([
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "MetaLeft",
  "MetaRight",
  "ShiftLeft",
  "ShiftRight",
]);

const KEY_TOKENS: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

export interface HotkeyFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  defaultValue?: string;
  hint?: string;
  error?: string | null;
}

export function HotkeyField({
  label,
  value,
  onChange,
  defaultValue = DEFAULT_OVERLAY_HOTKEY,
  hint,
  error,
}: HotkeyFieldProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const [recording, setRecording] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const shownError = localError ?? error;

  const record = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      setRecording(false);
      setLocalError(null);
      return;
    }
    const chord = chordFromEvent(e);
    if ("error" in chord) {
      setLocalError(chord.error);
      return;
    }
    onChange(chord.value);
    setRecording(false);
    setLocalError(null);
  };

  return (
    <div className="flex flex-col gap-2">
      <label id={id} className="text-t-xs text-fg-secondary">
        {label}
      </label>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          aria-labelledby={id}
          aria-describedby={
            cn(hint ? hintId : null, shownError ? errorId : null) || undefined
          }
          aria-invalid={shownError ? true : undefined}
          onClick={() => {
            setRecording(true);
            setLocalError(null);
          }}
          onKeyDown={record}
          className={cn(
            "inline-flex min-h-hit-min min-w-48 items-center rounded-sm border bg-surface-inset px-3",
            "font-text text-t-sm text-fg-primary transition-colors duration-fast ease-standard motion-reduce:transition-none",
            recording
              ? "border-accent text-accent"
              : "border-line-control hover:border-line-strong",
            shownError && "border-status-danger-border",
          )}
        >
          {recording ? "Recording..." : value}
        </button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => onChange(defaultValue)}
        >
          Reset
        </Button>
      </div>
      <span className="text-t-xs text-fg-tertiary" aria-live="polite">
        {recording ? "Press a modified key combination." : hint}
      </span>
      {hint && !recording && (
        <span id={hintId} className="sr-only">
          {hint}
        </span>
      )}
      {shownError && (
        <span id={errorId} className="text-t-xs text-status-danger-fg">
          {shownError}
        </span>
      )}
    </div>
  );
}

type ChordResult = { value: string } | { error: string };

function chordFromEvent(e: KeyboardEvent<HTMLButtonElement>): ChordResult {
  if (MODIFIER_CODES.has(e.code)) {
    return { error: "Press another key with the modifier." };
  }
  const key = keyToken(e.code);
  if (!key) {
    return { error: "That key cannot be recorded here." };
  }
  const modifiers = [
    e.ctrlKey ? "Ctrl" : null,
    e.altKey ? "Alt" : null,
    e.shiftKey ? "Shift" : null,
    e.metaKey ? "Super" : null,
  ].filter((m): m is string => Boolean(m));
  if (modifiers.length === 0) {
    return { error: "Add Ctrl, Alt, Shift, or Super." };
  }
  return { value: [...modifiers, key].join("+") };
}

function keyToken(code: string): string | null {
  if (code.startsWith("Key") && code.length === 4)
    return code.slice(3).toUpperCase();
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return KEY_TOKENS[code] ?? null;
}
