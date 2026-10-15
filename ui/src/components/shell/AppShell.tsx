// AppShell — the persistent frame around every route (UI_REFERENCE §3): the
// StatusRail (top), ReadinessBanner (conditional), NavRail (left), the routed
// <Outlet>, the CommandPalette overlay, and the toast viewport. Mounts the single
// live-event subscription once, and the global ⌘K palette shortcut.
import { useEffect, useRef } from "react";
import { Outlet, useNavigate } from "react-router-dom";

import { StatusRail } from "./StatusRail";
import { NavRail } from "./NavRail";
import { ReadinessBanner } from "./ReadinessBanner";
import { CommandPalette } from "./CommandPalette";
import { DevStateBadge } from "./DevStateBadge";
import { ScrollContainerProvider } from "./ScrollContainerContext";
import { ToastViewport } from "../primitives";
import { useLiveEvents } from "../../lib/ipc/useLiveEvents";
import { listenTo } from "../../lib/ipc/events";
import { useUiStore } from "../../state/uiStore";

export function AppShell() {
  useLiveEvents();
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
    <div className="flex flex-col h-full">
      <StatusRail />
      <ReadinessBanner />
      <div className="flex flex-1 min-h-0">
        <NavRail />
        <main
          ref={mainRef}
          className="relative flex-1 min-w-0 overflow-y-auto [scrollbar-gutter:stable] [contain:paint]"
        >
          <ScrollContainerProvider value={mainRef}>
            <Outlet />
          </ScrollContainerProvider>
        </main>
      </div>
      <CommandPalette />
      <ToastViewport />
      {import.meta.env.DEV && <DevStateBadge />}
    </div>
  );
}
