// PromotionPanel — the selectable surface of a saved report (specs/03 §8e, G4).
//
// Every top-level bullet under the four source headings becomes a checkbox. Ticks
// build a `PromotionSelection`; `prepare_promotion` returns the exact entry blocks
// that a confirmation would append, and only `confirm_promotion` writes. All states
// are defined here: loading (note status / preview), empty (no promotable bullets),
// error (status, prepare, confirm), partial (report outcome), and populated.
import { useEffect, useMemo, useRef, useState } from "react";

import { Button, Chip } from "../primitives";
import { PromotionDialog } from "./PromotionDialog";
import { cn } from "../../lib/cn";
import {
  SECTION_LABELS,
  groupPromotableItems,
  parsePromotableItems,
  selectedPromotionItems,
} from "../../lib/promotion";
import {
  promotionError,
  useConfirmPromotion,
  usePreparePromotion,
  useProjectNoteStatus,
} from "../../lib/ipc/promotion";
import type { PromotionProposal } from "../../bindings/PromotionProposal";
import type { PromotionReceipt } from "../../bindings/PromotionReceipt";
import type { ReportRun } from "../../bindings/ReportRun";

/** Backend command errors are `{ code, message }`; transport failures are not. */
function failureCode(error: unknown): string | null {
  return error !== null && typeof error === "object" && "code" in error
    ? String((error as { code: unknown }).code)
    : null;
}

export interface PromotionPanelProps {
  /** The persisted run whose saved markdown is being read. */
  run: ReportRun;
}

export function PromotionPanel({ run }: PromotionPanelProps) {
  const markdown = run.result?.markdown ?? "";
  const items = useMemo(() => parsePromotableItems(markdown), [markdown]);
  const groups = useMemo(() => groupPromotableItems(items), [items]);

  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(
    () => new Set<string>(),
  );
  const [proposal, setProposal] = useState<PromotionProposal | null>(null);
  const [receipt, setReceipt] = useState<PromotionReceipt | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const noteStatus = useProjectNoteStatus();
  const prepare = usePreparePromotion();
  const confirm = useConfirmPromotion();

  // After a successful promotion the selection bar collapses (taking the Promote
  // button with it), so the dialog cannot restore focus to its opener. Return focus
  // to the promotion region itself once the dialog has unmounted.
  const sectionRef = useRef<HTMLElement>(null);
  const [focusSection, setFocusSection] = useState(false);
  useEffect(() => {
    if (!focusSection) return;
    sectionRef.current?.focus();
    setFocusSection(false);
  }, [focusSection]);

  const selection = selectedPromotionItems(items, selectedIds);
  const selectedCount = selection.length;
  const sectionCount = new Set(selection.map((item) => item.section)).size;
  // A stale note-status entry (an old output folder, or the first-run "no folder"
  // error) must not enable promotion while the settings save refetches it.
  const noteReady = noteStatus.data?.count === 1 && !noteStatus.isFetching;
  const promotable =
    run.result !== null &&
    run.phase === "finished" &&
    (run.outcome === "OK" || run.outcome === "PARTIAL");

  const toggleItem = (id: string, checked: boolean) => {
    // A changed selection invalidates a failed preview attempt's message.
    if (prepare.isError) prepare.reset();
    setSelectedIds((previous) => {
      const next = new Set(previous);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const clearSelection = () => {
    setSelectedIds(new Set<string>());
    prepare.reset();
  };

  const promote = () => {
    if (selection.length === 0) return;
    prepare.mutate(
      { run_id: run.run_id, items: selection },
      {
        onSuccess: (next) => {
          setProposal(next);
          setReceipt(null);
          setDialogOpen(true);
        },
      },
    );
  };

  const confirmProposal = () => {
    if (!proposal) return;
    confirm.mutate(proposal, { onSuccess: (next) => setReceipt(next) });
  };

  // A conflict changes the note under the proposal; re-preparing is the only way
  // forward. The current selection is preserved, so the user reviews again rather
  // than re-ticking everything.
  const reprepare = () => {
    if (selection.length === 0) return;
    setReceipt(null);
    prepare.mutate(
      { run_id: run.run_id, items: selection },
      { onSuccess: (next) => setProposal(next) },
    );
  };

  const closeDialog = () => {
    setDialogOpen(false);
    setProposal(null);
    setReceipt(null);
    prepare.reset();
    confirm.reset();
  };

  const finishDialog = () => {
    setSelectedIds(new Set<string>());
    closeDialog();
    setFocusSection(true);
  };

  return (
    <section
      ref={sectionRef}
      tabIndex={-1}
      aria-label="Promote to project note"
      className="flex flex-col gap-3 rounded-panel border border-line p-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="font-display text-subtitle text-ink">
          Promote to project note
        </h3>
        <Chip tone={promotable ? "neutral" : "warn"}>
          {run.kind === "weekly_synthesis" ? "weekly synthesis" : "daily report"}
        </Chip>
      </div>

      <p className="text-caption text-ink-muted">
        Tick bullets from this saved report, preview the dated entries, then confirm.
        Nothing is written to the note until you confirm.
      </p>

      {run.outcome === "PARTIAL" && (
        <p className="text-caption text-warn">
          Partial coverage. Entries reflect only the periods and frames this report
          covered.
        </p>
      )}

      {!promotable ? (
        <p className="text-caption text-ink-muted">
          {run.result === null
            ? "This run has no saved report to promote yet."
            : run.outcome === null
              ? "Promotion needs a report with OK or partial coverage. This run finished without an outcome."
              : `Promotion needs a finished report with OK or partial coverage. This run is ${run.outcome}.`}
        </p>
      ) : items.length === 0 ? (
        <p className="text-caption text-ink-muted">
          This report has no promotable items. Promotion reads the top-level bullets
          under Activity, Explicitly evidenced decisions, Open questions and Next
          steps.
        </p>
      ) : (
        <>
          <div className="min-h-6 text-caption" aria-live="polite">
            {noteStatus.isLoading ? (
              <p className="text-ink-muted">Checking the project note…</p>
            ) : noteStatus.isError ? (
              <p role="alert" className="text-danger">
                Couldn't check the project note — {promotionError(noteStatus.error)}.{" "}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => noteStatus.refetch()}
                >
                  Retry
                </Button>
              </p>
            ) : noteStatus.data?.count === 0 ? (
              <p className="text-warn">
                No project note yet. Create one in Settings · Memory reports, then
                come back to promote.
              </p>
            ) : noteStatus.data && noteStatus.data.count > 1 ? (
              <p role="alert" className="text-warn">
                More than one project note was found ({noteStatus.data.count}).
                Promotion needs exactly one note in ScreenSearch/Projects/. Keep one
                file and move or remove the others — ScreenSearch never deletes or
                renames them.
              </p>
            ) : noteStatus.data?.path ? (
              <p className="text-ink-muted">
                Target note:{" "}
                <span className="break-all font-mono text-data">
                  {noteStatus.data.path.replaceAll("\\", "/")}
                </span>
              </p>
            ) : null}
          </div>

          <div
            className="flex min-h-12 flex-wrap items-center justify-between gap-2 border-b border-line pb-3"
            aria-live="polite"
          >
            {selectedCount === 0 ? (
              <span className="text-caption text-ink-faint">
                Select items to promote.
              </span>
            ) : (
              <>
                <div className="flex flex-wrap items-center gap-2">
                  <Chip tone="accent">{selectedCount} selected</Chip>
                  <span className="text-caption text-ink-muted">
                    {sectionCount} section{sectionCount === 1 ? "" : "s"}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={clearSelection}
                    disabled={prepare.isPending}
                  >
                    Clear
                  </Button>
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={promote}
                    disabled={!noteReady || prepare.isPending}
                    aria-busy={prepare.isPending}
                  >
                    {prepare.isPending ? "Preparing preview…" : "Promote"}
                  </Button>
                </div>
              </>
            )}
          </div>

          <div className="flex flex-col gap-4">
            {groups.map((group) => (
              <fieldset key={group.section} className="flex min-w-0 flex-col gap-1">
                <legend className="eyebrow mb-1">
                  {group.heading} → {SECTION_LABELS[group.section]}
                </legend>
                <ul className="flex flex-col">
                  {group.items.map((item) => {
                    const checked = selectedIds.has(item.id);
                    return (
                      <li key={item.id} className="min-w-0">
                        <label
                          className={cn(
                            "flex min-h-hit-min items-start gap-3 py-1 text-body font-body",
                            prepare.isPending
                              ? "cursor-not-allowed opacity-60"
                              : "cursor-pointer",
                            checked ? "text-ink" : "text-ink-muted hover:text-ink",
                          )}
                        >
                          <input
                            type="checkbox"
                            className="mt-1 h-4 w-4 shrink-0 accent-accent"
                            checked={checked}
                            disabled={prepare.isPending}
                            onChange={(event) =>
                              toggleItem(item.id, event.currentTarget.checked)
                            }
                          />
                          <span className="min-w-0 break-words">{item.text}</span>
                        </label>
                      </li>
                    );
                  })}
                </ul>
              </fieldset>
            ))}
          </div>

          {prepare.isError && (
            <div role="alert" className="flex flex-col gap-2 text-caption">
              <p className="text-danger">
                Couldn't prepare the promotion — {promotionError(prepare.error)}.
                Nothing was written.
              </p>
              {failureCode(prepare.error) === "FILE_CONFLICT" && (
                <p className="text-ink-muted">
                  Preparing again only rechecks the note. The app appends only to
                  exactly one project note, and only to the version it last wrote for
                  this profile. An edited note or more than one note in Projects/ keeps
                  reporting a conflict: restore the last app-written version, keep one
                  note, or choose a different output folder in Settings · Memory
                  reports.
                </p>
              )}
              <Button size="sm" onClick={promote}>
                Try again
              </Button>
            </div>
          )}
        </>
      )}

      {dialogOpen && proposal && (
        <PromotionDialog
          proposal={proposal}
          receipt={receipt}
          preparing={prepare.isPending}
          confirming={confirm.isPending}
          prepareError={prepare.isError ? promotionError(prepare.error) : null}
          prepareErrorCode={failureCode(prepare.error)}
          confirmError={confirm.isError ? promotionError(confirm.error) : null}
          onConfirm={confirmProposal}
          onCancel={closeDialog}
          onReprepare={reprepare}
          onDone={finishDialog}
        />
      )}
    </section>
  );
}
