// Navigation metadata — one ordered list with two readers (UI_REFERENCE §3,
// 0.4.5 D7): the NavRail renders it, and the command palette builds its
// jump-to entries from it. Keeping it here (not inside the rail) is what lets
// P9 add Ask and Work without restructuring the rail.
//
// The rail order is Deck · Ask · Work · Recall · Insights · Settings. Ask and
// Work have no route yet, and a rail entry that leads nowhere is forbidden
// (§10.1), so they are deliberately absent — never placeholders. P9 inserts
// both **between Deck and Recall** by adding two entries to this array; nothing
// else changes (the rail renders whatever is here):
//
//   { to: "/ask",  label: "Ask",  icon: IconAsk,  match: (p) => under(p, "/ask") },
//   { to: "/work", label: "Work", icon: IconWork, match: (p) => under(p, "/work") },
//
// `/timeline` and its detail routes are Recall's second surface, not a rail
// entry (§3): Recall's `match` covers them so the rail still shows where the
// user is.
import type { ComponentType } from "react";

import {
  IconDeck,
  IconInsights,
  IconRecall,
  IconSettings,
} from "../components/icons";

export interface NavEntry {
  /** Route the entry navigates to. */
  to: string;
  /** Sentence-case rail label; also the palette's jump-to name. */
  label: string;
  icon: ComponentType<{ size?: number }>;
  /** Whether this entry owns `pathname` (it and every descendant route). */
  match: (pathname: string) => boolean;
}

/** True for the path itself and every route nested under it. */
function under(pathname: string, base: string): boolean {
  return pathname === base || pathname.startsWith(`${base}/`);
}

/** The rail entries, in rail order. */
export const NAV_ENTRIES: readonly NavEntry[] = [
  {
    to: "/",
    label: "Deck",
    icon: IconDeck,
    match: (pathname) => pathname === "/",
  },
  {
    to: "/recall",
    label: "Recall",
    icon: IconRecall,
    match: (pathname) => under(pathname, "/recall") || under(pathname, "/timeline"),
  },
  {
    to: "/insights",
    label: "Insights",
    icon: IconInsights,
    match: (pathname) => under(pathname, "/insights"),
  },
  {
    to: "/settings",
    label: "Settings",
    icon: IconSettings,
    match: (pathname) => under(pathname, "/settings"),
  },
];

/** Index of the entry owning `pathname`, or -1 when no entry matches. */
export function activeNavIndex(pathname: string): number {
  return NAV_ENTRIES.findIndex((entry) => entry.match(pathname));
}
