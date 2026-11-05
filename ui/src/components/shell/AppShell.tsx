// AppShell — the persistent v2 frame around every route (UI_REFERENCE §3/§8): the
// StatusRail across the top, the ReadinessBanner when a subsystem needs attention, the
// NavRail down the left, and ONE scrolling pane holding the routed `<Outlet>`. Both
// rails are fixed; no route introduces a second vertical scroller.
//
// It also mounts, once: the live-event subscription, the global ⌘K palette shortcut, the
// `open_moment` deep link, the command palette, the toast host, and the theme applier that
// maps `ui.appearance` onto `<html data-theme>`.
import { useEffect, useRef } from "react";
import { Outlet, useNavigate } from "react-router-dom";

import { StatusRail } from "./StatusRail";
import { NavRail } from "./NavRail";
import { ReadinessBanner } from "./ReadinessBanner";
import { CommandPalette } from "./CommandPalette";
import { ToastHost } from "./ToastHost";
import { DevStateBadge } from "./DevStateBadge";
import { ScrollContainerProvider } from "./ScrollContainerContext";
import { useAppliedTheme } from "../../app/theme";
import { useLiveEvents } from "../../lib/ipc/useLiveEvents";
import { listenTo } from "../../lib/ipc/events";
import { useUiStore } from "../../state/uiStore";

export function AppShell() {
  useLiveEvents();
  useAppliedTheme();
  const navigate = useNavigate();
  const togglePalette = useUiStore((s) => s.togglePalette);
  // The single scroll context for every route (UI_REFERENCE §8). `relative` makes
  // <main> the offsetParent so a route can measure a child's `offsetTop` as the
  // virtualizer scrollMargin (Recall). `scrollbar-gutter: stable` reserves the 10px
  // scrollbar track so its appearance never reflows centered content (±10px CLS).
  // `contain: paint` (paint only — never layout, which would perturb that offsetTop)
  // gives the compositor an explicit paint boundary between the scrolling content and
  // the fixed NavRail — the app-side mitigation for the WebView2 ghost-rail artifact
  // (07 #106); safe because the fixed overlays are siblings of <main>, not descendants.
  //
  // Those two declarations are the only bracketed utilities here: they carry no design
  // value (no colour, size, radius or font), and both are load-bearing for the
  // one-scroll and no-CLS acceptance rules — there is no token for either property.
  const mainRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        togglePalette();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [togglePalette]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    listenTo("open_moment", (event) => {
      navigate(`/timeline/${event.frame_id}`);
    })
      .then((u) => {
        if (active) unlisten = u;
        else u();
      })
      .catch(() => {
        /* no Tauri runtime in browser dev */
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [navigate]);

  return (
    <div className="flex h-full flex-col bg-surface-app text-fg-primary">
      <StatusRail />
      <ReadinessBanner />
      <div className="flex min-h-0 flex-1">
        <NavRail />
        <main
          ref={mainRef}
          className="relative min-w-0 flex-1 overflow-y-auto [scrollbar-gutter:stable] [contain:paint]"
        >
          <ScrollContainerProvider value={mainRef}>
            <Outlet />
          </ScrollContainerProvider>
        </main>
      </div>
      <CommandPalette />
      <ToastHost />
      {import.meta.env.DEV && <DevStateBadge />}
    </div>
  );
}
