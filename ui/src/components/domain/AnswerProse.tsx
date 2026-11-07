// AnswerProse - renders one answer body (a saved turn, a streaming turn, a Recall answer)
// from a `string` (UI_REFERENCE §4/§5). This is deliberately a thin wrapper: the markdown
// options live in `answerProseMarkdown.ts` so every answer surface renders identically and the
// setup can be asserted without a DOM.
//
// The CALLER owns the container, because the answer's frame differs per surface: the Ask
// conversation wraps it in `ANSWER_PROSE_TYPOGRAPHY max-w-prose`, the Recall stream uses it
// as the focusable answer region. Rendering the prose wrapper here instead would impose a
// second max-width inside those containers.
//
// No `whitespace-pre-wrap`: markdown decides the line structure, so a `- item` renders as a
// list and `**bold**` as emphasis rather than as the literal characters the model wrote.
import Markdown from "react-markdown";

import {
  ANSWER_PROSE_COMPONENTS,
  ANSWER_PROSE_REMARK_PLUGINS,
} from "./answerProseMarkdown";

export interface AnswerProseProps {
  /** The answer exactly as the model streamed it or the turn recorded it. */
  text: string;
}

export function AnswerProse({ text }: AnswerProseProps) {
  return (
    <Markdown
      remarkPlugins={ANSWER_PROSE_REMARK_PLUGINS}
      components={ANSWER_PROSE_COMPONENTS}
    >
      {text}
    </Markdown>
  );
}
