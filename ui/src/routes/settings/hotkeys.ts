// Hotkey helpers for the Capture category: the cross-chord conflict check
// (gap `07` #100) and the live registration status of the two global chords.
// Both helpers lived inside the single Settings screen this phase replaced
// (`git show 6d906a7:ui/src/routes/Settings.tsx`, lines 106-118 and 417-434 — `HEAD` cannot
// be used since this phase deleted that file), so
// they are re-homed here rather than duplicated per row.
import { useEffect, useRef } from "react";

import type { HotkeyStatus } from "../../bindings/HotkeyStatus";
import { toast } from "../../state/toastStore";
import { settingDef } from "./registry";

/** The two global chords the shell registers (`03 §7d`). */
export const HOTKEY_KEYS = ["overlay.hotkey", "marks.hotkey"] as const;

export type HotkeyKey = (typeof HOTKEY_KEYS)[number];

/** One chord's last registration attempt, as the backend reported it. */
export interface HotkeyRegistration {
  /** The registration snapshot has an entry for this chord at all. */
  present: boolean;
  registered: boolean;
  chord: string | null;
  error: string | null;
}

/**
 * Canonical form of a chord, or null when it holds no key at all.
 *
 * `HotkeyField` records chords with a fixed modifier order and an uppercase key,
 * but a persisted value can be hand-edited in the settings table, so the
 * comparison is defensive: case-insensitive, modifier-order-insensitive, key last.
 */
export function normalizeChord(chord: string): string | null {
  const parts = chord
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter((part) => part.length > 0);
  if (parts.length === 0) return null;
  const key = parts[parts.length - 1];
  const modifiers = parts.slice(0, -1).sort();
  return [...modifiers, key].join("+");
}

/**
 * Whether two chords are the same combination (gap `07` #100). Empty values never
 * conflict: a chord the user has cleared registers nothing.
 */
export function chordsConflict(a: string, b: string): boolean {
  const na = normalizeChord(a);
  const nb = normalizeChord(b);
  return na !== null && na === nb;
}

/** The registration row for one chord, from the `get_hotkey_status` snapshot. */
export function hotkeyRegistration(
  statuses: readonly HotkeyStatus[] | undefined,
  key: HotkeyKey,
): HotkeyRegistration {
  const row = statuses?.find((status) => status.id === key);
  if (!row) {
    return { present: false, registered: false, chord: null, error: null };
  }
  return {
    present: true,
    registered: row.registered,
    chord: row.chord,
    error: row.error,
  };
}

/** What a registration failure means for the user, in one sentence. */
export function registrationFailure(error: string | null): string {
  const cause = error ?? "Windows did not register this chord";
  return `${cause.replace(/[.]+$/, "")}. Pick a different combination.`;
}

/**
 * Toast a registration failure when it *appears*, so a chord that stops working is
 * never silent (`03 §8`, D6 of 0.3.2). The inline warning carried by each row is the
 * durable surface; the first snapshot after mount stays quiet so opening Settings on
 * an already-broken chord does not fire a toast the user cannot connect to an action.
 */
export function useHotkeyFailureToasts(
  statuses: readonly HotkeyStatus[] | undefined,
): void {
  const seen = useRef<Map<string, string> | null>(null);

  useEffect(() => {
    if (!statuses) return;
    const current = new Map<string, string>();
    for (const key of HOTKEY_KEYS) {
      const row = statuses.find((status) => status.id === key);
      if (row && !row.registered) {
        current.set(key, `${row.chord}\u0000${row.error ?? ""}`);
      }
    }
    const previous = seen.current;
    seen.current = current;
    if (!previous) return;
    for (const key of HOTKEY_KEYS) {
      const signature = current.get(key);
      if (signature === undefined || previous.get(key) === signature) continue;
      const row = statuses.find((status) => status.id === key);
      toast.warning(
        `${settingDef(key).label} did not register. ${registrationFailure(row?.error ?? null)}`,
      );
    }
  }, [statuses]);
}
