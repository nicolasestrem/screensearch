// Theme for `ui.appearance` (UI_REFERENCE §1/§2, `03 §8`). Four concrete token
// blocks ship — `pine-light` (the `:root` default), `pine-dark`, `ink-light`,
// `ink-dark` — and the two `*-system` values are a stored *intent*, not a fifth
// theme: they resolve through `prefers-color-scheme` here, in the applier.
//
// The applier owns exactly one attribute, `<html data-theme>`. It never touches
// `data-density`: density is comfortable only in 0.4.5 (the closed key set has no
// density key), so leaving the attribute unset is the contract, not an omission.
import { useEffect } from "react";

import type { UiAppearance } from "../bindings/UiAppearance";
import { useSettings } from "../lib/ipc/queries";

/** The six stored values of `ui.appearance`, in binding order. */
export const APPEARANCE_OPTIONS: readonly { value: UiAppearance; label: string }[] = [
  { value: "pine-light", label: "Pine light" },
  { value: "pine-dark", label: "Pine dark" },
  { value: "pine-system", label: "Pine, follow system" },
  { value: "ink-light", label: "Ink light" },
  { value: "ink-dark", label: "Ink dark" },
  { value: "ink-system", label: "Ink, follow system" },
];

/** The OS light/dark preference the `*-system` values follow. */
const DARK_SCHEME = "(prefers-color-scheme: dark)";

/** The theme shown before settings arrive, and after the shell unmounts. */
const DEFAULT_APPEARANCE: UiAppearance = "pine-light";

/**
 * Apply `ui.appearance` to the document root. Reads the settings cache (TanStack
 * Query owns it), so a change made in Settings repaints every screen without a
 * reload. Unmounting removes the attribute, which restores the `:root` block —
 * `pine-light`.
 */
export function useAppliedTheme(): void {
  const appearance = useSettings().data?.ui_appearance ?? DEFAULT_APPEARANCE;

  useEffect(() => {
    const root = document.documentElement;
    const [accent, mode] = appearance.split("-");
    const followsSystem = mode === "system";
    // Only a `*-system` choice needs the media query, and only it needs a listener:
    // the four concrete values are applied verbatim.
    const media = followsSystem ? window.matchMedia(DARK_SCHEME) : null;

    const apply = () => {
      if (!followsSystem) {
        root.dataset.theme = appearance;
        return;
      }
      root.dataset.theme = media?.matches ? `${accent}-dark` : `${accent}-light`;
    };

    apply();
    media?.addEventListener("change", apply);

    return () => {
      media?.removeEventListener("change", apply);
      delete root.dataset.theme;
    };
  }, [appearance]);
}
