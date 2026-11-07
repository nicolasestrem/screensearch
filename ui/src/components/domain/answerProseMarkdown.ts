// answerProse - the ONE markdown setup for answer prose (UI_REFERENCE §4 ask/recall,
// §5 component inventory). Every surface that shows a model-written answer renders it
// through `AnswerProse`, which is a thin wrapper over this module: the remark plugin list
// and the component overrides live here, so a persisted turn, a streaming turn, the Recall
// answer stream and the Flow overlay cannot drift into two different renderings.
//
// Typography is CSS-only. The call site puts `ANSWER_PROSE_TYPOGRAPHY` on the answer
// container and @tailwindcss/typography's `deck` variant (`tailwind.config.js`) maps every
// `--tw-prose-*` variable onto a v2 semantic token, so headings, bold, lists, inline code,
// links, quotes and tables are themed without a per-element class:
//
//   <div className={`${ANSWER_PROSE_TYPOGRAPHY} max-w-prose`}>
//     <AnswerProse text={answer} />
//   </div>
//
// `ANSWER_PROSE_ELEMENTS` is the machine-readable projection of that mapping: one row per
// markdown element the renderer emits, naming the semantic token that colours it and the
// `--tw-prose-*` variable it comes from. `test/answer-prose.test.mjs` checks every row
// against `tailwind.config.js` (so this table cannot drift from the shipped CSS) and puts
// every row through the contrast fence in all four themes.
//
// Only `a` needs a component override, and it is not styling: answer text is model output,
// so a link must open in the OS browser, never navigate the app's own WebView (which would
// unmount the whole UI). Tauri v2 ignores plain `target="_blank"`, so http(s) links route
// through the opener plugin (0.3.1 D4); browser-dev and non-http(s) links fall back to the
// native `target="_blank"`. Used by the Ask v2 conversation, the Recall answer stream and
// the Flow overlay.
import { createElement, type MouseEvent } from "react";
import type { Components, Options } from "react-markdown";
import remarkGfm from "remark-gfm";

import { handleExternalLinkClick } from "../../lib/openExternal";

/** The typography pair every answer container carries. `prose` is the base typography
 *  scopes; `prose-deck` is the project's token mapping of it (never a second palette). */
export const ANSWER_PROSE_TYPOGRAPHY = "prose prose-deck";

/** GFM on top of CommonMark: tables, strikethrough, autolinks and task lists are what the
 *  answer models actually emit, and a missing table plugin renders a pipe soup. */
export const ANSWER_PROSE_REMARK_PLUGINS: NonNullable<Options["remarkPlugins"]> = [
  remarkGfm,
];

/** The one override: route model-output links out of the WebView. */
export const ANSWER_PROSE_COMPONENTS: Components = {
  a: ({ href, children }) =>
    createElement(
      "a",
      {
        href,
        target: "_blank",
        rel: "noopener noreferrer",
        onClick: (event: MouseEvent<HTMLAnchorElement>) =>
          handleExternalLinkClick(event, href),
      },
      children,
    ),
};

/**
 * The `deck` typography variant of `tailwind.config.js`, projected into data: one row per
 * `--tw-prose-*` variable the plugin emits. `from` is that variable, `token` is the
 * semantic colour it resolves to, `kind` says what the colour paints, and `elements` lists
 * the markdown tags that row governs (pseudo-elements are spelled out).
 *
 * `kind` decides how the row is fenced: `text` is checked against WCAG AA on every surface
 * an answer can appear on, `surface` is a background (a fenced code block's inset), and
 * `border` is a decorative rule - a blockquote's edge, a row separator - which WCAG 1.4.11
 * does not require when it carries no meaning of its own. `lead` and `captions` are part of
 * the variant but unreachable from markdown, so they are mapped and fenced, never dropped.
 */
export const ANSWER_PROSE_ELEMENTS = {
  body: {
    from: "--tw-prose-body",
    token: "--text-primary",
    kind: "text",
    elements: ["p", "ul", "ol", "li", "em", "del", "table", "td"],
  },
  bold: {
    from: "--tw-prose-bold",
    token: "--text-primary",
    kind: "text",
    elements: ["strong"],
  },
  headings: {
    from: "--tw-prose-headings",
    token: "--text-primary",
    kind: "text",
    elements: ["h1", "h2", "h3", "th"],
  },
  links: {
    from: "--tw-prose-links",
    token: "--accent",
    kind: "text",
    elements: ["a"],
  },
  counters: {
    from: "--tw-prose-counters",
    token: "--text-secondary",
    kind: "text",
    elements: ["ol > li::marker"],
  },
  bullets: {
    from: "--tw-prose-bullets",
    token: "--text-tertiary",
    kind: "text",
    elements: ["ul > li::marker"],
  },
  hr: {
    from: "--tw-prose-hr",
    token: "--border-subtle",
    kind: "border",
    elements: ["hr"],
  },
  quotes: {
    from: "--tw-prose-quotes",
    token: "--text-secondary",
    kind: "text",
    elements: ["blockquote"],
  },
  "quote-borders": {
    from: "--tw-prose-quote-borders",
    token: "--border-subtle",
    kind: "border",
    elements: ["blockquote (border)"],
  },
  captions: {
    from: "--tw-prose-captions",
    token: "--text-tertiary",
    kind: "text",
    elements: ["figcaption"],
  },
  code: {
    from: "--tw-prose-code",
    token: "--text-primary",
    kind: "text",
    elements: ["code"],
  },
  "pre-code": {
    from: "--tw-prose-pre-code",
    token: "--text-primary",
    kind: "text",
    elements: ["pre > code"],
  },
  "pre-bg": {
    from: "--tw-prose-pre-bg",
    token: "--surface-inset",
    kind: "surface",
    elements: ["pre"],
  },
  "th-borders": {
    from: "--tw-prose-th-borders",
    token: "--border-subtle",
    kind: "border",
    elements: ["th (border)"],
  },
  "td-borders": {
    from: "--tw-prose-td-borders",
    token: "--border-subtle",
    kind: "border",
    elements: ["td (border)"],
  },
  lead: {
    from: "--tw-prose-lead",
    token: "--text-secondary",
    kind: "text",
    elements: ["p.lead"],
  },
} as const;

/** The semantic surfaces an answer can be rendered on inside the shell (the content pane,
 *  a panel, a raised card, a selected row, the overlay chrome, the code-block inset). */
export const ANSWER_PROSE_SURFACES = [
  "--surface-app",
  "--surface-chrome",
  "--surface-pane",
  "--surface-raised",
  "--surface-inset",
  "--surface-hover",
  "--surface-selected",
] as const;
