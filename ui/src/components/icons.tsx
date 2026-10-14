// Inline SVG icons (no web fonts — UI_REFERENCE §1/§8). Stroke-based, sized in em
// so they scale with surrounding text; color via currentColor. Each is decorative
// by default (aria-hidden); the labelled control owns the accessible name.
import type { ReactNode, SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Svg({
  size = 18,
  children,
  ...rest
}: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  );
}

/** Deck — a gauge / at-a-glance dashboard. */
export const IconDeck = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 12a9 9 0 0 1 18 0" />
    <path d="M3 12v6h18v-6" />
    <path d="M12 12l4-3" />
  </Svg>
);

/** Recall — search + recall. */
export const IconRecall = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-3.5-3.5" />
  </Svg>
);

/** Mark — a bookmark (mark-this-moment / Intentions). */
export const IconMark = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1Z" />
  </Svg>
);

/** Timeline — a filmstrip / scan ribbon. */
export const IconTimeline = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="6" width="18" height="12" rx="1" />
    <path d="M7 6v12M17 6v12M3 10h4M3 14h4M17 10h4M17 14h4" />
  </Svg>
);

/** Insights — a bar chart. */
export const IconInsights = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 20V10M10 20V4M16 20v-8M22 20H2" />
  </Svg>
);

/** Settings — a gear. */
export const IconSettings = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2" />
  </Svg>
);

/** Close / dismiss. */
export const IconClose = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Svg>
);

/** Chevron right — list affordance. */
export const IconChevronRight = (p: IconProps) => (
  <Svg {...p}>
    <path d="M9 6l6 6-6 6" />
  </Svg>
);

/** Chevron left — previous / back-by-one. */
export const IconChevronLeft = (p: IconProps) => (
  <Svg {...p}>
    <path d="M15 6l-6 6 6 6" />
  </Svg>
);

/** Arrow left — "back" navigation. */
export const IconArrowLeft = (p: IconProps) => (
  <Svg {...p}>
    <path d="M19 12H5M11 6l-6 6 6 6" />
  </Svg>
);

/** Image — a frame thumbnail placeholder (no resolvable source). */
export const IconImage = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="4" width="18" height="16" rx="1.5" />
    <circle cx="8.5" cy="9.5" r="1.5" />
    <path d="M5 19l5-5 3 3 3-4 3 4" />
  </Svg>
);

/** Sparkle — the grounded "Ask" action (RAG answer). */
export const IconSparkle = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3l1.8 4.9L18.9 9l-5.1 1.1L12 15l-1.8-4.9L5.1 9l5.1-1.1z" />
    <path d="M18 15l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" />
  </Svg>
);

/** Tag — a vision/label chip marker. */
export const IconTag = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 12V5a2 2 0 0 1 2-2h7l9 9-9 9z" />
    <circle cx="7.5" cy="7.5" r="1.3" fill="currentColor" stroke="none" />
  </Svg>
);

/** Capture — a record dot. */
export const IconCapture = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <circle cx="12" cy="12" r="3.5" fill="currentColor" stroke="none" />
  </Svg>
);

/** Database — the data spine. */
export const IconDatabase = (p: IconProps) => (
  <Svg {...p}>
    <ellipse cx="12" cy="5" rx="8" ry="3" />
    <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
    <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
  </Svg>
);

/** Queue — stacked deferred work. */
export const IconQueue = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 7h16M4 12h16M4 17h10" />
  </Svg>
);

/** Sidecar / model — a processor. */
export const IconCpu = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6" y="6" width="12" height="12" rx="1" />
    <path d="M9 9h6v6H9z" />
    <path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3" />
  </Svg>
);

/** Download — an arrow into a tray (model fetch in progress). */
export const IconDownload = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3v12M7 10l5 5 5-5" />
    <path d="M5 21h14" />
  </Svg>
);
