// ReportView — renders a finished recall report (UI_REFERENCE §4 Recall/reports,
// `docs/0.2.0.md` PR6): the markdown body (react-markdown + GFM, prose-deck tokens),
// clickable source-frame chips, Copy + `.md` download, and an honest footer stating the
// app version, model, time span, filters, and coverage counts. 0.3.1 (D2/D3, #65): the
// download is a date-stamped file in Downloads via the `save_report_markdown` command
// (collision-safe `-2`/`-3`); 0.3.2 (#89): daily/weekly downloads carry the report kind
// in the stem. The footer is a single plain-text block used in BOTH
// places — on screen AND appended to the copied/saved markdown so the file is
// self-describing. A no-evidence report renders its honest message with no chips/footer.
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { isTauri } from "@tauri-apps/api/core";

import { CitationTile } from "./CitationTile";
import { PromotionPanel } from "./PromotionPanel";
import { Button } from "../primitives";
import { toast } from "../../state/toastStore";
import { saveReportMarkdown } from "../../lib/ipc/commands";
import { useAppVersion } from "../../lib/useAppVersion";
import { reportFileStem } from "../../lib/time";
import { buildReportFooter } from "../../lib/reportFooter";
import type { ReportFooterScope } from "../../lib/reportFooter";
import { handleExternalLinkClick } from "../../lib/openExternal";
import type { ReportKind } from "../../bindings/ReportKind";
import type { ReportResponse } from "../../bindings/ReportResponse";
import type { ReportRequest } from "../../bindings/ReportRequest";
import type { ReportRun } from "../../bindings/ReportRun";

export interface ReportViewProps {
  report: ReportResponse;
  /** The request the report was generated from — the footer's time-span + filter source
   *  (0.3.1 D3). `null` only in the unreachable "done with no request" state. */
  request: Omit<ReportRequest, "request_id"> | null;
  onOpenFrame: (frameId: number) => void;
  /** Optional non-Recall scope. Session Recap uses the same renderer/footer while
   *  naming its stable session id as the active filter. */
  footerScope?: ReportFooterScope;
  /** The persisted run this report belongs to, when it has one. A finished run with
   *  OK/partial coverage makes the saved report promotable into the project note
   *  (`03 §8e`, G4); on-demand reports have no run and stay read-only. */
  run?: ReportRun | null;
  /** Suppress the on-screen footer where the surrounding surface already states the
   *  same coverage (the copied/saved markdown still carries it). */
  showFooter?: boolean;
}

/** How many citation chips to render before collapsing the rest into a count. */
const CITATION_CAP = 24;

/** The report body with the footer appended as a trailing block, so the copied/saved
 *  file is self-describing. No footer (no-evidence report) → the bare body. */
function withFooter(markdown: string, footer: string | null): string {
  return footer ? `${markdown}\n\n---\n\n${footer}\n` : markdown;
}

/** Browser-dev fallback (no Tauri runtime): download via a Blob so a report is never
 *  lost when running the UI outside the packaged app. */
function blobDownload(filename: string, markdown: string) {
  const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

async function downloadReport(markdown: string, kind?: ReportKind) {
  const stem = reportFileStem(kind);
  if (isTauri()) {
    try {
      const path = await saveReportMarkdown(stem, markdown);
      toast.success(`Report saved → ${path}`);
    } catch (e) {
      toast.error(`Couldn't save the report — ${String(e)}`);
    }
    return;
  }
  blobDownload(`${stem}.md`, markdown);
}

async function copyMarkdown(markdown: string) {
  try {
    await navigator.clipboard.writeText(markdown);
    toast.success("Report copied to clipboard");
  } catch {
    toast.error("Couldn't copy — clipboard unavailable");
  }
}

export function ReportView({
  report,
  request,
  onOpenFrame,
  footerScope,
  run = null,
  showFooter = true,
}: ReportViewProps) {
  const appVersion = useAppVersion();
  const cited = report.cited_frame_ids;
  const shown = cited.slice(0, CITATION_CAP);
  const overflow = cited.length - shown.length;
  const footer = buildReportFooter(report, request, appVersion, footerScope);
  // Copy + save carry the same self-describing footer the screen shows (0.3.1 D3).
  const exportMarkdown = withFooter(report.markdown, footer);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="eyebrow">Report · {report.range_label}</span>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => copyMarkdown(exportMarkdown)}
          >
            Copy
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => downloadReport(exportMarkdown, request?.kind)}
          >
            Download .md
          </Button>
        </div>
      </div>

      <div className="prose prose-deck max-w-none break-words">
        <Markdown
          remarkPlugins={[remarkGfm]}
          components={{
            // Report text is model output: open links in the OS browser, never the app's
            // own WebView (which would unmount the UI). Tauri v2 ignores plain
            // `target="_blank"`, so http(s) links are routed through the opener plugin
            // (0.3.1 D4); browser-dev and non-http(s) links fall back to `target="_blank"`.
            a: ({ href, children }) => (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => handleExternalLinkClick(e, href)}
              >
                {children}
              </a>
            ),
            // Saved reports can embed images; never fetch them inside the app shell.
            img: ({ alt }) => (
              <span>{alt ? `[Image omitted: ${alt}]` : "[Image omitted]"}</span>
            ),
          }}
        >
          {report.markdown}
        </Markdown>
      </div>

      {/* A persisted run makes this saved report promotable (`03 §8e`, G4).
          On-demand reports (Recall, Session recap) never show this surface. */}
      {run && <PromotionPanel key={run.run_id} run={run} />}

      {cited.length > 0 && (
        <div className="flex flex-col gap-2">
          <span className="eyebrow">Source frames</span>
          {/* Wrap onto rows instead of a nested horizontal scroller (D9 — one scroll
              context; Windows draws a permanent scrollbar for overflow-x-auto). */}
          <div className="flex flex-wrap gap-2">
            {shown.map((id) => (
              <CitationTile key={id} frameId={id} onOpenFrame={onOpenFrame} />
            ))}
            {overflow > 0 && (
              <span className="flex shrink-0 items-center px-2 text-caption text-ink-muted">
                +{overflow} more
              </span>
            )}
          </div>
        </div>
      )}

      {showFooter && footer && (
        <div className="whitespace-pre-wrap break-words border-t border-line pt-2 text-caption text-ink-faint font-body">
          {footer}
        </div>
      )}
    </div>
  );
}
