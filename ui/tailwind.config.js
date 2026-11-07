import typography from "@tailwindcss/typography";

/** @type {import('tailwindcss').Config} */
// Tailwind theme maps utilities onto the CSS custom properties in
// src/styles/tokens.css (the v2 semantic, component and scale layers). Every
// key resolves to a custom property, so no component can name a colour, size,
// radius, shadow, layer or duration that the design system does not own.
//
// One vocabulary: the v2 keys (surface.*, the border set, fg.*, accent.*,
// action.*, status.*, mark.*, t-*, raised/panel/toast, row/control/gap/pane).
// The v1 vocabulary the deleted compat shim used to resolve is gone, so a
// legacy utility (bg-base, bare bg-surface, bg-overlay, bg-scrim, bare
// border-line, text-ink*, text-danger, font-display, text-caption,
// tracking-eyebrow, rounded-chip, ease-ui, shadow-scan) no longer compiles to
// anything.
//
// Themes are switched by <html data-theme> (pine-light default, pine-dark,
// ink-light, ink-dark), not by a media query, so `darkMode` is irrelevant.
export default {
  content: ["./index.html", "./overlay.html", "./src/**/*.{ts,tsx}"],
  theme: {
    colors: {
      transparent: "transparent",
      current: "currentColor",
      inherit: "inherit",

      surface: {
        app: "var(--surface-app)",
        chrome: "var(--surface-chrome)",
        pane: "var(--surface-pane)",
        raised: "var(--surface-raised)",
        inset: "var(--surface-inset)",
        hover: "var(--surface-hover)",
        selected: "var(--surface-selected)",
        scrim: "var(--surface-scrim)",
      },
      line: {
        subtle: "var(--border-subtle)",
        strong: "var(--border-strong)",
        control: "var(--border-control)",
        focus: "var(--border-focus)",
      },
      accent: {
        DEFAULT: "var(--accent)",
        strong: "var(--accent-strong)",
        wash: "var(--accent-wash)",
        line: "var(--accent-line)",
      },
      fg: {
        primary: "var(--text-primary)",
        secondary: "var(--text-secondary)",
        tertiary: "var(--text-tertiary)",
        disabled: "var(--text-disabled)",
        "on-accent": "var(--text-on-accent)",
      },
      action: {
        primary: "var(--action-primary-bg)",
        "primary-hover": "var(--action-primary-bg-hover)",
        "primary-fg": "var(--action-primary-fg)",
        danger: "var(--action-danger-bg)",
        "danger-hover": "var(--action-danger-bg-hover)",
        "danger-fg": "var(--action-danger-fg)",
      },
      status: {
        ok: {
          fg: "var(--status-ok-fg)",
          bg: "var(--status-ok-bg)",
          border: "var(--status-ok-border)",
        },
        warn: {
          fg: "var(--status-warn-fg)",
          bg: "var(--status-warn-bg)",
          border: "var(--status-warn-border)",
        },
        danger: {
          fg: "var(--status-danger-fg)",
          bg: "var(--status-danger-bg)",
          border: "var(--status-danger-border)",
        },
      },
      mark: {
        bg: "var(--mark-bg)",
        fg: "var(--mark-fg)",
      },
      focus: "var(--focus-ring)",
    },
    borderRadius: {
      // Literal zero, not a token reference: `rounded-none` is a hard square.
      none: "0",
      full: "var(--radius-pill)",
      sm: "var(--radius-sm)",
      md: "var(--radius-md)",
      lg: "var(--radius-lg)",
      pill: "var(--radius-pill)",
    },
    fontFamily: {
      text: "var(--font-text)",
      mono: "var(--font-mono)",
    },
    fontSize: {
      // v2 type scale: every size is a step of the token scale, none is a literal.
      "t-xs": ["var(--text-xs)", { lineHeight: "var(--leading-data)" }],
      "t-sm": ["var(--text-sm)", { lineHeight: "var(--leading-data)" }],
      "t-data": ["var(--text-data)", { lineHeight: "var(--leading-data)" }],
      "t-body": ["var(--text-body)", { lineHeight: "var(--leading-body)" }],
      "t-ui": ["var(--text-ui)", { lineHeight: "var(--leading-data)" }],
      "t-prose": ["var(--text-prose)", { lineHeight: "var(--leading-body)" }],
      "t-h3": ["var(--text-h3)", { lineHeight: "var(--leading-tight)" }],
      "t-h2": ["var(--text-h2)", { lineHeight: "var(--leading-tight)" }],
      "t-h1": ["var(--text-h1)", { lineHeight: "var(--leading-tight)" }],
      "t-display": ["var(--text-display)", { lineHeight: "var(--leading-tight)" }],
    },
    fontWeight: {
      regular: "var(--weight-regular)",
      medium: "var(--weight-medium)",
      semibold: "var(--weight-semibold)",
    },
    // Full token-backed scale: every Tailwind default key keeps the px value it
    // always had, so a utility is density-aware without moving a layout.
    spacing: {
      0: "var(--space-0)",
      px: "var(--space-px)",
      0.5: "var(--space-0_5)",
      1: "var(--space-1)",
      1.5: "var(--space-1_5)",
      2: "var(--space-2)",
      2.5: "var(--space-2_5)",
      3: "var(--space-3)",
      3.5: "var(--space-3_5)",
      4: "var(--space-4)",
      5: "var(--space-5)",
      6: "var(--space-6)",
      7: "var(--space-7)",
      8: "var(--space-8)",
      9: "var(--space-9)",
      10: "var(--space-10)",
      11: "var(--space-11)",
      12: "var(--space-12)",
      14: "var(--space-14)",
      16: "var(--space-16)",
      20: "var(--space-20)",
      24: "var(--space-24)",
      28: "var(--space-28)",
      32: "var(--space-32)",
      36: "var(--space-36)",
      40: "var(--space-40)",
      44: "var(--space-44)",
      48: "var(--space-48)",
      52: "var(--space-52)",
      56: "var(--space-56)",
      60: "var(--space-60)",
      64: "var(--space-64)",
      72: "var(--space-72)",
      80: "var(--space-80)",
      96: "var(--space-96)",
      // density and interactive metrics
      row: "var(--row-h)",
      control: "var(--control-h)",
      "control-sm": "var(--control-h-sm)",
      gap: "var(--grid-gap)",
      pane: "var(--pane-pad)",
      "hit-min": "var(--hit-min)",
    },
    extend: {
      letterSpacing: {
        tight: "var(--tracking-tight)",
      },
      zIndex: {
        base: "var(--z-base)",
        rail: "var(--z-rail)",
        overlay: "var(--z-overlay)",
        toast: "var(--z-toast)",
      },
      transitionTimingFunction: {
        standard: "var(--ease-standard)",
      },
      transitionDuration: {
        fast: "var(--duration-fast)",
        base: "var(--duration-base)",
        slow: "var(--duration-slow)",
      },
      boxShadow: {
        raised: "var(--shadow-raised)",
        // Named after the surface it lifts, not the overlay colour: Tailwind
        // would read `.shadow-overlay` as a shadow-COLOUR utility and emit no
        // shadow at all.
        panel: "var(--shadow-overlay)",
        toast: "var(--shadow-toast)",
      },
      borderColor: {
        DEFAULT: "var(--border-subtle)",
      },
      outlineColor: {
        focus: "var(--focus-ring)",
      },
      // Prose (answer bodies) themed onto v2 tokens.
      typography: () => ({
        deck: {
          css: {
            "--tw-prose-body": "var(--text-primary)",
            "--tw-prose-headings": "var(--text-primary)",
            "--tw-prose-lead": "var(--text-secondary)",
            "--tw-prose-links": "var(--accent)",
            "--tw-prose-bold": "var(--text-primary)",
            "--tw-prose-counters": "var(--text-secondary)",
            "--tw-prose-bullets": "var(--text-tertiary)",
            "--tw-prose-hr": "var(--border-subtle)",
            "--tw-prose-quotes": "var(--text-secondary)",
            "--tw-prose-quote-borders": "var(--border-subtle)",
            "--tw-prose-captions": "var(--text-tertiary)",
            "--tw-prose-code": "var(--text-primary)",
            "--tw-prose-pre-code": "var(--text-primary)",
            "--tw-prose-pre-bg": "var(--surface-inset)",
            "--tw-prose-th-borders": "var(--border-subtle)",
            "--tw-prose-td-borders": "var(--border-subtle)",
          },
        },
      }),
    },
  },
  plugins: [typography],
};
