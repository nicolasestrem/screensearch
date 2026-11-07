// PromptCardGrid — the premade Ask cards (UI_REFERENCE §5, `docs/0.2.0.md` PR6).
// A grid of one-click starting questions; clicking a card fills the Ask input and
// submits it through the existing `ask` flow. Genuinely new in 0.2.0 (the old UI had
// none). Tokens only; each card is a ≥hit-min touch target.
export interface PromptCardGridProps {
  /** Fill + submit the chosen prompt through the Ask flow. */
  onPick: (prompt: string) => void;
}

interface Card {
  title: string;
  blurb: string;
  prompt: string;
}

// The five premade cards from UI_REFERENCE §5. The prompt is what gets asked; it reads
// `content_text` like any Ask, so answers are grounded in real captures and cited.
const CARDS: Card[] = [
  {
    title: "Day Recap",
    blurb: "What you worked on today",
    prompt: "Give me a recap of what I worked on today, grouped by topic.",
  },
  {
    title: "Standup Update",
    blurb: "A short bulleted update",
    prompt:
      "Write a concise standup update of what I did, as a short bulleted list of concrete items.",
  },
  {
    title: "Time Breakdown",
    blurb: "Where your time went",
    prompt:
      "Break down how I spent my time today across the apps and activities I had open.",
  },
  {
    title: "Top of Mind",
    blurb: "The themes you kept returning to",
    prompt:
      "What topics and tasks were most on my mind, based on what I had open and read?",
  },
  {
    title: "AI Habits",
    blurb: "How you used AI tools",
    prompt:
      "Summarize how I used AI tools and assistants — what I asked and what for.",
  },
];

export function PromptCardGrid({ onPick }: PromptCardGridProps) {
  return (
    <div className="flex flex-col gap-3">
      <span className="text-t-xs font-medium text-fg-secondary">
        Try a quick question
      </span>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {CARDS.map((card) => (
          <button
            key={card.title}
            type="button"
            onClick={() => onPick(card.prompt)}
            className="flex min-h-hit-min flex-col items-start gap-0.5 rounded-sm border border-line-subtle bg-surface-pane px-3 py-2 text-left transition-colors duration-fast ease-standard hover:border-accent hover:bg-surface-hover"
          >
            <span className="text-t-body font-semibold text-fg-primary">
              {card.title}
            </span>
            <span className="text-t-xs text-fg-secondary">
              {card.blurb}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
