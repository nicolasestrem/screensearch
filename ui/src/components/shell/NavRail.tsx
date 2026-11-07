// NavRail (left) — the fixed primary rail (UI_REFERENCE §3/§8, 0.4.5 D7). It
// renders `NAV_ENTRIES` in order and nothing else: `/timeline` is Recall's second
// surface, and Ask/Work arrive from P9 by extending that list, not this file.
//
// The active entry carries `aria-current="page"`, the selected surface and a 2px
// leading accent bar, so the state is never colour-only. Keyboard (§7): a roving
// tabindex makes the rail a single Tab stop; Arrow Up/Down (wrapping) plus
// Home/End move focus between links, Enter follows one.
//
// The footer is quiet, by design: the Command affordance for ⌘K, the lifecycle
// quick actions, the updater presence, and the version link to the repo (0.3.1 D4).
import { Link, useLocation } from "react-router-dom";
import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";

import { cn } from "../../lib/cn";
import { NAV_ENTRIES, activeNavIndex } from "../../app/nav";
import { useUiStore } from "../../state/uiStore";
import { useAppVersion } from "../../lib/useAppVersion";
import { openExternal } from "../../lib/openExternal";
import { Kbd } from "../primitives";
import { UpdateIndicator } from "./UpdateIndicator";
import { QuickActions } from "./QuickActions";

/** The project's public repo — opened by the NavRail version link (0.3.1 D4, #57). */
const REPO_URL = "https://github.com/nicolasestrem/screensearch";

const ENTRY = cn(
  "flex min-h-row min-w-0 items-center gap-3 rounded-md border-l-2 px-3",
  "text-t-ui font-text transition-colors duration-fast ease-standard motion-reduce:transition-none",
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
);

const ENTRY_ACTIVE = "border-accent bg-accent-wash text-accent";
const ENTRY_IDLE =
  "border-transparent text-fg-secondary hover:bg-surface-hover hover:text-fg-primary active:bg-surface-selected";

/** Footer controls share one quiet shape: raised surface, subtle border, no accent. */
const FOOTER_CONTROL = cn(
  "flex min-h-control w-full min-w-0 items-center justify-between gap-2 rounded-md px-3",
  "border border-line-subtle bg-surface-raised text-t-sm font-text text-fg-secondary",
  "transition-colors duration-fast ease-standard motion-reduce:transition-none",
  "hover:border-line-strong hover:bg-surface-hover hover:text-fg-primary",
  "active:bg-surface-selected",
  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
);

export function NavRail() {
  const openPalette = useUiStore((s) => s.openPalette);
  const version = useAppVersion();
  const { pathname } = useLocation();
  const linkRefs = useRef<Array<HTMLAnchorElement | null>>([]);
  const activeIndex = activeNavIndex(pathname);
  // Roving tabindex: exactly one link sits in the Tab order at a time (seeded to the
  // current route, or the first entry when the route owns none). Arrow keys move focus
  // without leaving the rail.
  const [focusIndex, setFocusIndex] = useState(() =>
    activeIndex < 0 ? 0 : activeIndex,
  );

  // Keep the tab stop on the current route when navigation happens *outside* the rail
  // (palette, an in-app link, back/forward) — otherwise the next Tab into the rail lands
  // on a stale link. Arrow-key moves don't change the path, so they aren't clobbered.
  useEffect(() => {
    if (activeIndex >= 0) setFocusIndex(activeIndex);
  }, [activeIndex]);

  const focusItem = (index: number) => {
    const next = (index + NAV_ENTRIES.length) % NAV_ENTRIES.length; // wrap at both ends
    setFocusIndex(next);
    linkRefs.current[next]?.focus();
  };

  const onKeyDown = (
    event: ReactKeyboardEvent<HTMLAnchorElement>,
    index: number,
  ) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focusItem(index + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focusItem(index - 1);
        break;
      case "Home":
        event.preventDefault();
        focusItem(0);
        break;
      case "End":
        event.preventDefault();
        focusItem(NAV_ENTRIES.length - 1);
        break;
      default:
        break;
    }
  };

  return (
    <nav
      aria-label="Primary"
      // `relative z-rail isolate` gives the rail its own stacking context / compositing
      // boundary: an isolated surface repaints atomically when a banner mount/unmount
      // shifts the row — the app-side mitigation for the WebView2 ghost-rail stale-surface
      // artifact (07 #106; finally consumes the `--z-rail` token).
      className={cn(
        "relative z-rail isolate flex w-56 shrink-0 flex-col justify-between gap-gap",
        "border-r border-line-subtle bg-surface-chrome px-2 py-3",
      )}
    >
      <ul className="flex min-w-0 flex-col gap-1">
        {NAV_ENTRIES.map(({ to, label, icon: Icon, match }, index) => {
          const current = match(pathname);
          return (
            <li key={to} className="min-w-0">
              <Link
                to={to}
                ref={(element) => {
                  linkRefs.current[index] = element;
                }}
                aria-current={current ? "page" : undefined}
                tabIndex={index === focusIndex ? 0 : -1}
                onFocus={() => setFocusIndex(index)}
                onKeyDown={(event) => onKeyDown(event, index)}
                className={cn(ENTRY, current ? ENTRY_ACTIVE : ENTRY_IDLE)}
              >
                <Icon size={18} />
                <span className="min-w-0 truncate">{label}</span>
              </Link>
            </li>
          );
        })}
      </ul>

      <div className="flex min-w-0 flex-col gap-2">
        <button type="button" onClick={openPalette} className={FOOTER_CONTROL}>
          <span>Command</span>
          <Kbd>Ctrl K</Kbd>
        </button>
        {/* Lifecycle quick actions (0.3.2 PR3, #57): load/unload the answer model and
            start/stop vision tagging, through the same commands as the tray. */}
        <QuickActions />
        {/* Updater presence (0.3.2 PR2, #69): a quiet dot while an update exists — never a
            count — plus the manual check. Zero presence otherwise. */}
        <UpdateIndicator />
        {/* Version footer link (0.3.1 D4, #57): the running app version, opening the repo in
            the OS browser (Tauri v2 does not open plain `target="_blank"` anchors). Hidden
            with no Tauri runtime (browser dev → null). Quiet by design — telemetry, not a
            primary action — but still a real link with the global focus ring. */}
        {version && (
          <a
            href={REPO_URL}
            onClick={(event) => {
              event.preventDefault();
              openExternal(REPO_URL);
            }}
            className={cn(
              "flex min-h-control items-center justify-center rounded-md px-3",
              "font-mono text-t-data text-fg-tertiary",
              "transition-colors duration-fast ease-standard motion-reduce:transition-none",
              "hover:bg-surface-hover hover:text-fg-primary",
              "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
            )}
          >
            v{version}
          </a>
        )}
      </div>
    </nav>
  );
}
