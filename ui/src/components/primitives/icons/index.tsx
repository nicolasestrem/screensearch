// The one local SVG icon set (UI_REFERENCE §1, 0.4.5 D15). No icon package and
// no network fetch: every glyph is a stroked path on a 24 unit grid, colourless
// by design (it draws with currentColor so tokens decide colour) and sized by
// the `size` prop in px (default 18). Stroke weight is `--stroke-icon` (1.5) for
// every glyph, so a heavier icon is a spec change, not a call-site override.
//
// Icons are decorative by default (aria-hidden="true"): the labelled control
// owns the accessible name. Pass `role="img"` and a `aria-label` when a glyph
// stands alone as the only content.
import type { ReactNode, SVGProps } from "react";

export interface IconProps extends SVGProps<SVGSVGElement> {
  /** Rendered width/height in px. Default 18. */
  size?: number;
}

/** Shared wrapper: one geometry frame, one stroke weight. */
export function Svg({ size = 18, strokeWidth, style, children, ...rest }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
      style={{ strokeWidth: strokeWidth ?? "var(--stroke-icon, 1.5)", ...style }}
    >
      {children}
    </svg>
  );
}

/* ---- app navigation and screens (carried over from v1) ---- */

/** Deck - a gauge / at-a-glance dashboard. */
export const IconDeck = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 12a9 9 0 0 1 18 0" />
    <path d="M3 12v6h18v-6" />
    <path d="M12 12l4-3" />
  </Svg>
);

/** Recall - search + recall. */
export const IconRecall = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-3.5-3.5" />
  </Svg>
);

/** Mark - a bookmark (mark-this-moment / Intentions). */
export const IconMark = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1Z" />
  </Svg>
);

/** Timeline - a filmstrip / scan ribbon. */
export const IconTimeline = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="6" width="18" height="12" rx="1" />
    <path d="M7 6v12M17 6v12M3 10h4M3 14h4M17 10h4M17 14h4" />
  </Svg>
);

/** Insights - a bar chart. */
export const IconInsights = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 20V10M10 20V4M16 20v-8M22 20H2" />
  </Svg>
);

/** Settings - a gear. */
export const IconSettings = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2" />
  </Svg>
);

/* ---- close and chevrons ---- */

/** Close / dismiss. */
export const IconClose = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Svg>
);

/** Chevron right - list affordance, expander affordance. */
export const IconChevronRight = (p: IconProps) => (
  <Svg {...p}>
    <path d="M9 6l6 6-6 6" />
  </Svg>
);

/** Chevron left - previous / back-by-one. */
export const IconChevronLeft = (p: IconProps) => (
  <Svg {...p}>
    <path d="M15 6l-6 6 6 6" />
  </Svg>
);

/** Chevron down - select affordance, disclosure. */
export const IconChevronDown = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 9l6 6 6-6" />
  </Svg>
);

/** Chevron up - collapse / sort ascending affordance. */
export const IconChevronUp = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 15l6-6 6 6" />
  </Svg>
);

/** Arrow left - "back" navigation. */
export const IconArrowLeft = (p: IconProps) => (
  <Svg {...p}>
    <path d="M19 12H5M11 6l-6 6 6 6" />
  </Svg>
);

/** Arrow right - "forward" / "next" navigation. */
export const IconArrowRight = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5 12h14M13 6l6 6-6 6" />
  </Svg>
);

/* ---- media and content ---- */

/** Image - a frame thumbnail placeholder (no resolvable source). */
export const IconImage = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4" width="18" height="16" rx="1.5" />
    <circle cx="8.5" cy="9.5" r="1.5" />
    <path d="M5 19l5-5 3 3 3-4 3 4" />
  </Svg>
);

/** Sparkle - the grounded "Ask" action (RAG answer). */
export const IconSparkle = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3l1.8 4.9L18.9 9l-5.1 1.1L12 15l-1.8-4.9L5.1 9l5.1-1.1z" />
    <path d="M18 15l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" />
  </Svg>
);

/** Tag - a vision/label chip marker. */
export const IconTag = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 12V5a2 2 0 0 1 2-2h7l9 9-9 9z" />
    <circle cx="7.5" cy="7.5" r="1.3" fill="currentColor" stroke="none" />
  </Svg>
);

/** Capture - a record dot. */
export const IconCapture = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <circle cx="12" cy="12" r="3.5" fill="currentColor" stroke="none" />
  </Svg>
);

/** Play - start capture / preview playback. */
export const IconPlay = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8 4.8 19.2 12 8 19.2z" />
  </Svg>
);

/** Stop - end capture / stop playback. */
export const IconStop = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6.5" y="6.5" width="11" height="11" rx="1.5" />
  </Svg>
);

/* ---- data and pipeline ---- */

/** Database - the data spine. */
export const IconDatabase = (p: IconProps) => (
  <Svg {...p}>
    <ellipse cx="12" cy="5" rx="8" ry="3" />
    <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
    <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
  </Svg>
);

/** Queue - stacked deferred work. */
export const IconQueue = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 7h16M4 12h16M4 17h10" />
  </Svg>
);

/** Sidecar / model - a processor. */
export const IconCpu = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6" y="6" width="12" height="12" rx="1" />
    <path d="M9 9h6v6H9z" />
    <path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" />
  </Svg>
);

/** Layers - stacked planes (grouping, depth). */
export const IconLayers = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3 3 7.5 12 12l9-4.5z" />
    <path d="M3 12.5 12 17l9-4.5" />
    <path d="M3 16.7 12 21.2l9-4.5" />
  </Svg>
);

/* ---- status and state ---- */

/** Check - a bare confirmation mark (ok). */
export const IconCheck = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4.5 12.5 9.5 17.5 19.5 6.5" />
  </Svg>
);

/** Check circle - ok, as a self-contained status. */
export const IconCheckCircle = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M8 12.3l2.8 2.8L16.4 9" />
  </Svg>
);

/** Warn - warning triangle (status warn, never a decorative accent). */
export const IconWarn = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 4 3.5 19.4h17z" />
    <path d="M12 9.6v4.2" />
    <path d="M12 17h.01" />
  </Svg>
);

/** Alert - danger, circled exclamation (status danger). */
export const IconAlert = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7.4v5.4" />
    <path d="M12 16.8h.01" />
  </Svg>
);

/** Info - neutral advisory, circled i. */
export const IconInfo = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11.2v5.6" />
    <path d="M12 7.6h.01" />
  </Svg>
);

/** Spinner - a three-quarter arc (animate it in the consumer). */
export const IconSpinner = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3a9 9 0 1 0 9 9" />
  </Svg>
);

/* ---- affordances ---- */

/** External link - opens away from the app. */
export const IconExternalLink = (p: IconProps) => (
  <Svg {...p}>
    <path d="M14 4h6v6" />
    <path d="M20 4l-8.5 8.5" />
    <path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
  </Svg>
);

/** Drag handle - reorder grip. */
export const IconDragHandle = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="9" cy="7" r="1.2" fill="currentColor" stroke="none" />
    <circle cx="15" cy="7" r="1.2" fill="currentColor" stroke="none" />
    <circle cx="9" cy="12" r="1.2" fill="currentColor" stroke="none" />
    <circle cx="15" cy="12" r="1.2" fill="currentColor" stroke="none" />
    <circle cx="9" cy="17" r="1.2" fill="currentColor" stroke="none" />
    <circle cx="15" cy="17" r="1.2" fill="currentColor" stroke="none" />
  </Svg>
);

/** More - overflow menu, three dots across. */
export const IconMore = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="6" cy="12" r="1.3" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none" />
    <circle cx="18" cy="12" r="1.3" fill="currentColor" stroke="none" />
  </Svg>
);

/** Plus - add. */
export const IconPlus = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 5v14M5 12h14" />
  </Svg>
);

/** Minus - remove, collapse. */
export const IconMinus = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5 12h14" />
  </Svg>
);

/* ---- actions ---- */

/** Eye - reveal a secret / sensitive value. */
export const IconEye = (p: IconProps) => (
  <Svg {...p}>
    <path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z" />
    <circle cx="12" cy="12" r="3" />
  </Svg>
);

/** Eye off - hide a secret / sensitive value. */
export const IconEyeOff = (p: IconProps) => (
  <Svg {...p}>
    <path d="M2.5 12S6 5.5 12 5.5c1.4 0 2.6.4 3.7.9" />
    <path d="M18.4 8.8c2 1.8 3.1 3.2 3.1 3.2S18 18.5 12 18.5c-1.6 0-3-.4-4.2-1" />
    <path d="M4 4l16 16" />
  </Svg>
);

/** Search - query field. */
export const IconSearch = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-4.3-4.3" />
  </Svg>
);

/** Refresh - re-read / re-run. */
export const IconRefresh = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 12a8 8 0 0 1 13.7-5.6L20 8.5" />
    <path d="M20 4v4.5h-4.5" />
    <path d="M20 12a8 8 0 0 1-13.7 5.6L4 15.5" />
    <path d="M4 20v-4.5h4.5" />
  </Svg>
);

/** Save - write changes. */
export const IconSave = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 5a1 1 0 0 1 1-1h11l4 4v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1Z" />
    <path d="M8 4.5v5h8v-5" />
    <path d="M8 20v-6h8v6" />
  </Svg>
);

/** Copy - duplicate to the clipboard. */
export const IconCopy = (p: IconProps) => (
  <Svg {...p}>
    <rect x="9" y="9" width="11" height="11" rx="1.5" />
    <path d="M5.5 15H4.5A1.5 1.5 0 0 1 3 13.5v-9A1.5 1.5 0 0 1 4.5 3h9A1.5 1.5 0 0 1 15 4.5v1" />
  </Svg>
);

/** Undo - revert the last change. */
export const IconUndo = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8.5 4.5 3.5 9.5l5 5" />
    <path d="M3.5 9.5H14a5.5 5.5 0 0 1 0 11H10" />
  </Svg>
);

/** Scissors - cut / split. */
export const IconScissors = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="6" cy="6" r="2.5" />
    <circle cx="6" cy="18" r="2.5" />
    <path d="M8 7.6 20 18" />
    <path d="M8 16.4 20 6" />
  </Svg>
);

/** Trash - delete (pair with an undo or confirmation). */
export const IconTrash = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 7h16" />
    <path d="M9.5 7V4.5h5V7" />
    <path d="M6.5 7l.9 12.2a1 1 0 0 0 1 .8h7.2a1 1 0 0 0 1-.8L17.5 7" />
    <path d="M10.5 11v6M13.5 11v6" />
  </Svg>
);

/** Upload - send out. */
export const IconUpload = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 20V8M7 13l5-5 5 5" />
    <path d="M5 3h14" />
  </Svg>
);

/** Download - receive in. */
export const IconDownload = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3v12M7 10l5 5 5-5" />
    <path d="M5 21h14" />
  </Svg>
);

/* ---- data display and controls ---- */

/** Filter - narrow the set. */
export const IconFilter = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3.5 5h17l-6.5 7.6V20l-4 2v-9.4z" />
  </Svg>
);

/** Sort ascending - ordered by a field, low to high. */
export const IconSortAsc = (p: IconProps) => (
  <Svg {...p}>
    <path d="M7 20V5M3.5 8.5 7 5l3.5 3.5" />
    <path d="M13 7h8M13 12h6M13 17h4" />
  </Svg>
);

/** Sort descending - ordered by a field, high to low. */
export const IconSortDesc = (p: IconProps) => (
  <Svg {...p}>
    <path d="M7 4v15M3.5 15.5 7 19l3.5-3.5" />
    <path d="M13 7h8M13 12h6M13 17h4" />
  </Svg>
);

/** Grid - tile layout. */
export const IconGrid = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="3.5" width="7" height="7" rx="1" />
    <rect x="13.5" y="3.5" width="7" height="7" rx="1" />
    <rect x="3.5" y="13.5" width="7" height="7" rx="1" />
    <rect x="13.5" y="13.5" width="7" height="7" rx="1" />
  </Svg>
);

/** List - stacked rows. */
export const IconList = (p: IconProps) => (
  <Svg {...p}>
    <path d="M9 6h11M9 12h11M9 18h11" />
    <path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" />
  </Svg>
);

/** Calendar - a date. */
export const IconCalendar = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3.5" y="5" width="17" height="15.5" rx="1.5" />
    <path d="M8 2.5v5M16 2.5v5M3.5 10h17" />
  </Svg>
);

/** Clock - a time of day. */
export const IconClock = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5.5l3.5 2" />
  </Svg>
);

/* ---- density and theme controls ---- */

/** Keyboard - shortcut reference. */
export const IconKeyboard = (p: IconProps) => (
  <Svg {...p}>
    <rect x="2.5" y="6" width="19" height="12" rx="1.5" />
    <path d="M6 10h.01M9.5 10h.01M13 10h.01M16.5 10h.01" />
    <path d="M6 13.5h.01M9.5 13.5h.01M13 13.5h.01M16.5 13.5h.01" />
    <path d="M8 16.5h8" />
  </Svg>
);

/** Moon - dark theme. */
export const IconMoon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M20 14.6A8.5 8.5 0 0 1 9.4 4 8.5 8.5 0 1 0 20 14.6Z" />
  </Svg>
);

/** Sun - light theme. */
export const IconSun = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3 7 7M17 17l1.7 1.7M18.7 5.3 17 7M7 17l-1.7 1.7" />
  </Svg>
);
