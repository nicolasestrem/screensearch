// Promotion parsing and ordering for the single project note (specs/03 §8e, G4).
//
// The saved report is the selectable surface: only top-level `- ` bullets under the
// four source headings become items, and each source heading maps to one fixed
// target section of the note. This module is pure (no DOM, no IPC) so the mapping
// can be unit-tested and so the component never guesses what "verbatim" means:
// `item.text` is exactly the text after `- ` in the saved markdown, which is what
// `prepare_promotion` re-validates.
import type { PromotionItem } from "../bindings/PromotionItem";
import type { PromotionSection } from "../bindings/PromotionSection";

/** Source heading (as written in the saved report) → target note section. */
export const SOURCE_SECTION: Record<string, PromotionSection> = {
  activity: "log",
  "explicitly evidenced decisions": "decisions",
  "open questions": "open_questions",
  "next steps": "next_steps",
};

/** Human labels for the note's fixed sections. */
export const SECTION_LABELS: Record<PromotionSection, string> = {
  decisions: "Decisions",
  open_questions: "Open questions",
  next_steps: "Next steps",
  log: "Log",
};

/** Canonical note order (`PromotionProposal.entries` is built in this order). */
export const SECTION_ORDER: readonly PromotionSection[] = [
  "decisions",
  "open_questions",
  "next_steps",
  "log",
];

/** One selectable bullet, with a stable id scoped to a single saved report. */
export interface PromotableItem {
  /** Unique within one report; stable across re-renders of the same markdown. */
  id: string;
  /** The source heading exactly as written (e.g. "Explicitly evidenced decisions"). */
  heading: string;
  section: PromotionSection;
  /** Bullet text verbatim from the saved markdown (without the leading "- "). */
  text: string;
}

/** Items grouped by their source heading, in the report's own order. */
export interface PromotableGroup {
  section: PromotionSection;
  heading: string;
  items: PromotableItem[];
}

/** ATX heading text without the closing hashes Markdown permits (`## Activity ##`). */
function headingText(line: string): string {
  return line.replace(/^##[ \t]+/, "").replace(/[ \t]+#+[ \t]*$/, "").trim();
}

type FenceCharacter = "`" | "~";

interface FenceDelimiter {
  character: FenceCharacter;
  length: number;
  rest: string;
}

/**
 * A line that can open or close a fenced block: at most three leading spaces,
 * then a run of at least three backticks or tildes, then an optional info string.
 */
function fenceDelimiter(line: string): FenceDelimiter | null {
  const match = /^( {0,3})(`{3,}|~{3,})(.*)$/.exec(line.replace(/[\r\n]+$/, ""));
  if (!match) return null;
  const run = match[2];
  return {
    character: run.startsWith("`") ? "`" : "~",
    length: run.length,
    rest: match[3],
  };
}

/**
 * Mirrors the backend `MarkdownFence`: a fence closes only on the same marker,
 * a run at least as long as the opener, with nothing but whitespace after it.
 * Every other delimiter-looking line inside the region is content, so a fake
 * `## heading` in a fence is never a section boundary.
 */
function createMarkdownFence(): { containsLine(line: string): boolean } {
  let open: { character: FenceCharacter; length: number } | null = null;
  return {
    containsLine(line: string): boolean {
      const delimiter = fenceDelimiter(line);
      if (!delimiter) return open !== null;
      if (open === null) {
        // A backtick fence's info string cannot contain a backtick.
        if (delimiter.character === "`" && delimiter.rest.includes("`")) return false;
        open = { character: delimiter.character, length: delimiter.length };
        return true;
      }
      if (
        delimiter.character === open.character &&
        delimiter.length >= open.length &&
        delimiter.rest.trim() === ""
      ) {
        open = null;
      }
      return true;
    },
  };
}

/**
 * Extract every top-level `- ` bullet under the four promotion source headings.
 *
 * Deliberately narrow: nested bullets, continuation paragraphs and fenced code are
 * not selectable because `prepare_promotion` accepts exactly the top-level bullets
 * of the saved run markdown. A heading is a level-2 (`##`) heading; a level-1
 * heading closes the section, while deeper headings do not.
 */
export function parsePromotableItems(markdown: string): PromotableItem[] {
  const items: PromotableItem[] = [];
  let section: PromotionSection | null = null;
  let heading = "";
  const fence = createMarkdownFence();

  for (const rawLine of markdown.split(/\r?\n/)) {
    // Fenced code can contain `- ` and `## ` lines that are not report content.
    if (fence.containsLine(rawLine)) continue;

    const line = rawLine.trimEnd();
    if (line.startsWith("## ") || line.startsWith("##\t")) {
      const title = headingText(line);
      const target = SOURCE_SECTION[title.toLowerCase()];
      section = target ?? null;
      heading = target ? title : "";
      continue;
    }
    if (line.startsWith("# ") || line === "#") {
      section = null;
      heading = "";
      continue;
    }
    if (!section) continue;

    const bullet = /^- (.+)$/.exec(line);
    if (!bullet) continue;
    items.push({
      id: `${section}:${items.length}`,
      heading,
      section,
      // Verbatim: the backend re-validates this exact text against the saved run.
      text: bullet[1],
    });
  }

  return items;
}

/** Group parsed items for display, keeping the report's heading order. */
export function groupPromotableItems(items: PromotableItem[]): PromotableGroup[] {
  const groups: PromotableGroup[] = [];
  const bySection = new Map<PromotionSection, PromotableGroup>();
  for (const item of items) {
    let group = bySection.get(item.section);
    if (!group) {
      group = { section: item.section, heading: item.heading, items: [] };
      bySection.set(item.section, group);
      groups.push(group);
    }
    group.items.push(item);
  }
  return groups;
}

/**
 * The wire selection, in the note's canonical section order. Selection order in the
 * UI is irrelevant: the same ticks always produce the same proposal request.
 */
export function selectedPromotionItems(
  items: PromotableItem[],
  selectedIds: ReadonlySet<string>,
): PromotionItem[] {
  return SECTION_ORDER.flatMap((section) =>
    items
      .filter((item) => item.section === section && selectedIds.has(item.id))
      .map((item) => ({ section, text: item.text })),
  );
}

/**
 * The provenance line is written as Markdown italics (`*Source: …*`). The preview
 * renders the intended sentence, not the asterisks; the note itself keeps them.
 */
export function plainProvenance(provenance: string): string {
  const trimmed = provenance.trim();
  if (trimmed.length > 2 && trimmed.startsWith("*") && trimmed.endsWith("*")) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/** Short display form of a content hash (never a secret; purely informational). */
export function shortHash(hash: string | null): string | null {
  return hash ? hash.slice(0, 12) : null;
}
