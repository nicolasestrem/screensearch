// Promotion lane checks (`03 §8e`, G4): the parser must expose exactly the
// top-level bullets of the four source headings; the IPC wrappers must call the
// frozen command names with the frozen argument keys; the panel/dialog must render
// every state without a DOM harness.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";

import {
  groupPromotableItems,
  parsePromotableItems,
  selectedPromotionItems,
} from "../src/lib/promotion.ts";
import * as commands from "../src/lib/ipc/commands.ts";

register("./ui-loader.mjs", import.meta.url);
const { PromotionPanel } = await import("../src/components/domain/PromotionPanel.tsx");
const { PromotionDialog } = await import("../src/components/domain/PromotionDialog.tsx");

const MARKDOWN = `# Weekly synthesis — 2026-10-12 to 2026-10-18

Three days were covered; two were empty.

## Activity
- Shipped the promotion UI [1]
- Reviewed the frozen seams and the UI reference [2]
  - nested detail is not selectable
- Tightened the conflict copy [3]

## Explicitly evidenced decisions
- Keep promotion append-only [4]
- Date entries with the run's saved timezone [5]

## Open questions
- Whether provenance should cite the note hash [6]

## Next steps
- Verify the conflict path against an edited note [7]

## Coverage
- 5 of 7 days had usable evidence
`;

const NOTE = {
  path: "C:/Users/Ada/Documents/Notes/ScreenSearch/Projects/project-alpha.md",
  exists: true,
  count: 1,
};

const PROPOSAL = {
  selection: {
    run_id: "run-1",
    items: [{ section: "decisions", text: "Keep promotion append-only [4]" }],
  },
  run_id: "run-1",
  note_path: NOTE.path,
  note_exists: true,
  note_hash: "hash-before",
  entry_date: "2026-10-18",
  entries: [
    {
      section: "decisions",
      heading_date: "2026-10-18",
      items: ["Keep promotion append-only [4]"],
      provenance: "run run-1 · frames 12, 19",
    },
    {
      section: "log",
      heading_date: "2026-10-18",
      items: ["Shipped the promotion UI [1]"],
      provenance: "run run-1 · frames 3",
    },
  ],
};

function renderPanel(markdown, noteStatus, overrides = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, retryOnMount: false } },
  });
  if (noteStatus !== undefined) {
    client.setQueryData(["projectNoteStatus"], noteStatus);
  }
  const run = {
    run_id: "run-1",
    kind: "weekly_synthesis",
    phase: "finished",
    outcome: "OK",
    result: { markdown },
    ...overrides,
  };
  const html = renderToStaticMarkup(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(PromotionPanel, { run }),
    ),
  );
  client.clear();
  return html;
}

test("parser selects only top-level dash bullets under the four source headings", () => {
  const items = parsePromotableItems(MARKDOWN);
  assert.equal(items.length, 7);
  assert.deepEqual(
    items.map((item) => item.section),
    ["log", "log", "log", "decisions", "decisions", "open_questions", "next_steps"],
  );
  assert.equal(items[0].text, "Shipped the promotion UI [1]");
  assert.ok(!items.some((item) => item.text.includes("nested detail")));
  assert.ok(!items.some((item) => item.text.includes("usable evidence")));
});

test("parser ignores fenced code, star bullets and non-level-2 heading changes", () => {
  const markdown = [
    "## Activity",
    "```",
    "- not a report bullet",
    "```",
    "* not a dash bullet",
    "-not a bullet",
    "- a real bullet",
    "### A sub-heading keeps the section",
    "- another real bullet",
    "# A level-1 heading closes it",
    "- outside any section",
  ].join("\n");
  const items = parsePromotableItems(markdown);
  assert.deepEqual(
    items.map((item) => item.text),
    ["a real bullet", "another real bullet"],
  );
});

test("grouping keeps report order; selection is emitted in canonical note order", () => {
  const items = parsePromotableItems(MARKDOWN);
  const groups = groupPromotableItems(items);
  assert.deepEqual(
    groups.map((group) => group.section),
    ["log", "decisions", "open_questions", "next_steps"],
  );
  assert.equal(groups[0].heading, "Activity");
  assert.equal(groups[0].items.length, 3);

  const selected = new Set([
    items[0].id, // Activity → Log (first in the report)
    items[6].id, // Next steps
    items[3].id, // Decisions
  ]);
  const wire = selectedPromotionItems(items, selected);
  assert.deepEqual(
    wire.map((item) => item.section),
    ["decisions", "next_steps", "log"],
  );
  assert.equal(wire[0].text, "Keep promotion append-only [4]");
});

test("promotion commands use the frozen snake_case names and argument keys", async () => {
  globalThis.window = {};
  const calls = [];
  mockIPC((command, args) => {
    calls.push({ command, args });
    switch (command) {
      case "project_note_status":
        return { ...NOTE, path: null, exists: false, count: 0 };
      case "create_project_note":
        return NOTE;
      case "prepare_promotion":
        return PROPOSAL;
      case "confirm_promotion":
        return {
          path: NOTE.path,
          hash: "hash-after",
          conflict: false,
          error: null,
          recovery_path: null,
        };
      default:
        return null;
    }
  });
  try {
    const selection = PROPOSAL.selection;
    await commands.getProjectNoteStatus();
    await commands.createProjectNote("project-alpha");
    const prepared = await commands.preparePromotion(selection);
    const receipt = await commands.confirmPromotion(prepared);
    assert.deepEqual(
      calls.map((call) => call.command),
      [
        "project_note_status",
        "create_project_note",
        "prepare_promotion",
        "confirm_promotion",
      ],
    );
    assert.deepEqual(calls[1].args, { slug: "project-alpha" });
    assert.deepEqual(calls[2].args, { selection });
    assert.deepEqual(calls[3].args, { proposal: prepared });
    assert.equal(receipt.conflict, false);
  } finally {
    clearMocks();
    delete globalThis.window;
  }
});

test("populated report: one checkbox per promotable bullet and an explicit target", () => {
  const html = renderPanel(MARKDOWN, NOTE);
  assert.equal((html.match(/type="checkbox"/g) ?? []).length, 7);
  assert.match(html, /Activity → Log/);
  assert.match(html, /Target note:/);
  assert.match(html, /project-alpha\.md/);
  assert.match(html, /Select items to promote\./);
  assert.doesNotMatch(html, /nested detail is not selectable/);
});

test("empty and blocked states are honest before any selection exists", () => {
  const empty = renderPanel("## Activity\n\nNo evidence in this window.\n", NOTE);
  assert.match(empty, /no promotable items/i);
  assert.doesNotMatch(empty, /type="checkbox"/);

  const blocked = renderPanel(MARKDOWN, NOTE, { outcome: "NO_DATA" });
  assert.match(blocked, /Promotion needs a finished report/);
  assert.doesNotMatch(blocked, /type="checkbox"/);

  const noNote = renderPanel(MARKDOWN, { path: null, exists: false, count: 0 });
  assert.match(noNote, /No project note yet/);
  assert.match(noNote, /Create one in Settings/);

  const many = renderPanel(MARKDOWN, { path: NOTE.path, exists: true, count: 3 });
  assert.match(many, /More than one project note was found \(3\)/);
});

test("confirm dialog previews entry blocks, note path and promotion date", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: null,
      preparing: false,
      confirming: false,
      prepareError: null,
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  assert.match(html, /role="dialog"/);
  assert.match(html, /aria-modal="true"/);
  assert.match(html, /Confirm promotion/);
  assert.match(html, /2026-10-18/);
  assert.match(html, /project-alpha\.md/);
  assert.match(html, /Keep promotion append-only \[4\]/);
  assert.match(html, /frames 3/);
  assert.match(html, />Cancel</);
  assert.match(html, />Confirm</);
});

test("prepare rejection shows the backend error and offers a retry, never a confirm", () => {
  const backendError =
    "The project note changed since the last app write; restore it or choose a different output folder.";
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: null,
      preparing: false,
      confirming: false,
      prepareError: backendError,
      prepareErrorCode: "FILE_CONFLICT",
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  // The backend's own message is shown verbatim, never a stringified exception.
  assert.ok(html.includes(backendError), "the backend error should be shown verbatim");
  assert.doesNotMatch(html, /\[object Object\]/);
  // Re-preparing only rechecks: an edited note stays protected until the user
  // restores the app-written version or picks another output folder.
  assert.match(html, /Preparing again only rechecks the note/);
  assert.match(html, /restore the last app-written version/);
  assert.match(html, /choose a different output folder in Settings/);
  // The selection is still held by the panel; the dialog offers only retry/cancel.
  assert.match(html, />Try again</);
  assert.match(html, />Cancel</);
  assert.doesNotMatch(html, />Confirm</);
});

test("non-conflict prepare rejection keeps the conflict guidance out", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: null,
      preparing: false,
      confirming: false,
      prepareError: "The saved report is no longer available.",
      prepareErrorCode: "NOT_FOUND",
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  assert.match(html, /The saved report is no longer available\./);
  assert.doesNotMatch(html, /Preparing again only rechecks the note/);
  assert.doesNotMatch(html, />Confirm</);
});

test("persisted ownership conflict says re-preparing cannot clear an edited note", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: {
        path: NOTE.path,
        hash: null,
        conflict: true,
        error: {
          code: "FILE_CONFLICT",
          message:
            "The project note was edited after the app's last promotion; promotion cannot append to it.",
        },
        // A guard conflict is caught before any byte moves: the backend attaches
        // no recovery copy to it.
        recovery_path: null,
      },
      preparing: false,
      confirming: false,
      prepareError: null,
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  assert.match(html, /The project note changed/);
  assert.match(html, /Nothing was written/);
  // A note the app already owns stays protected: re-preparing only rechecks it.
  assert.match(html, /cannot clear an\s+ownership conflict/);
  assert.match(html, /appends only to the version it last wrote/);
  assert.match(html, /Restore that version/);
  assert.match(html, /choose a different output folder in Settings/);
  assert.doesNotMatch(html, /re-reads the note as it is now/);
  assert.doesNotMatch(html, /current text/);
  assert.match(html, />Prepare again</);
  assert.match(html, />Close</);
  assert.doesNotMatch(html, />Confirm</);
});

test("stale preview conflict explains re-preparing without bypassing ownership", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: {
        path: NOTE.path,
        hash: null,
        conflict: true,
        error: {
          code: "FILE_CONFLICT",
          message:
            "The project note changed since the preview; prepare the promotion again.",
        },
        recovery_path: null,
      },
      preparing: false,
      confirming: false,
      prepareError: null,
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  // Re-preparing is useful for a stale preview, but never authorizes edits to
  // an already-owned note. Both cases use typed conflict handling, not message parsing.
  assert.match(html, /Preparing again rechecks a stale preview/);
  assert.match(html, /had not adopted this note yet/);
  assert.match(html, /previews its current bytes instead/);
  assert.match(html, /cannot clear an\s+ownership conflict/);
  assert.match(html, /appends only to the version it last wrote/);
  assert.match(html, />Prepare again</);
  assert.doesNotMatch(html, />Confirm</);
});

test("prepare retry stays clickable even when the stale proposal lost its note", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: { ...PROPOSAL, note_path: null, note_exists: false, note_hash: null },
      receipt: null,
      preparing: false,
      confirming: false,
      prepareError: "Create a project note first.",
      prepareErrorCode: "NOT_FOUND",
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  // The fresh prepare call re-resolves the note; the old proposal's note state
  // must not suppress the retry button.
  assert.match(html, />Try again<\/button>/);
  assert.doesNotMatch(html, /<button[^>]*\sdisabled(?:=""|\s|>)/);
});

test("success receipt states what was written and dismisses as Done", () => {
  const html = renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt: {
        path: NOTE.path,
        hash: "0123456789abcdef",
        conflict: false,
        error: null,
        recovery_path: null,
      },
      preparing: false,
      confirming: false,
      prepareError: null,
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
  assert.match(html, /Promoted to the project note/);
  assert.match(html, /2 items appended for 2026-10-18/);
  assert.match(html, /0123456789ab/);
  assert.match(html, />Done</);
  assert.doesNotMatch(html, />Confirm</);
});

const RECOVERY = "C:/Notes/ScreenSearch/Projects/.screensearch-recovery-1.md";

function renderReceipt(receipt) {
  return renderToStaticMarkup(
    React.createElement(PromotionDialog, {
      proposal: PROPOSAL,
      receipt,
      preparing: false,
      confirming: false,
      prepareError: null,
      confirmError: null,
      onConfirm: () => {},
      onCancel: () => {},
      onReprepare: () => {},
      onDone: () => {},
    }),
  );
}

test("an installed append that displaced a concurrent version is reported as written", () => {
  // Concurrent swap (03 §8c): the writer installed the promotion and kept the
  // version it displaced, so the receipt carries both a hash and a copy.
  const html = renderReceipt({
    path: NOTE.path,
    hash: "0123456789abcdef",
    conflict: true,
    error: {
      code: "FILE_CONFLICT",
      message:
        "The entries were appended, but the note changed or could not be verified while they were being written. The version the write replaced is kept in the recovery copy.",
    },
    recovery_path: RECOVERY,
  });
  assert.match(html, /Promoted — check the recovery copy/);
  assert.match(html, /2 items appended for 2026-10-18/);
  assert.match(html, /written · recovery copy/);
  assert.match(html, /\.screensearch-recovery-1\.md/);
  assert.match(html, /that edit is in the copy, not\s+in the note/);
  assert.match(html, /exactly one note in Projects\//);
  assert.doesNotMatch(html, /Nothing was written/);
  assert.doesNotMatch(html, /nothing written/);
  assert.doesNotMatch(html, /left untouched/);
  // The append is in the note: offering to promote it again would duplicate it.
  assert.match(html, />Done</);
  assert.doesNotMatch(html, />Prepare again</);
  assert.doesNotMatch(html, />Try again</);
});

test("a conflict that left a recovery copy never claims the note is untouched", () => {
  const html = renderReceipt({
    path: NOTE.path,
    hash: null,
    conflict: true,
    error: {
      code: "FILE_CONFLICT",
      message:
        "note changed during installation; inspect the destination and retained recovery copy",
    },
    recovery_path: RECOVERY,
  });
  assert.match(html, /This promotion is not in the note as previewed/);
  assert.match(html, /not appended/);
  assert.match(html, /\.screensearch-recovery-1\.md/);
  assert.match(html, /Check the note\s+and the copy before preparing again/);
  assert.doesNotMatch(html, /Nothing was written/);
  assert.doesNotMatch(html, /exactly as you left it/);
  assert.doesNotMatch(html, /left\s+untouched/);
  assert.match(html, />Prepare again</);
  assert.doesNotMatch(html, />Done</);
});
