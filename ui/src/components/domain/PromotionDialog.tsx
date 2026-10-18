// PromotionDialog — the explicit confirmation step for promotion (specs/03 §8e, G4).
//
// Shows exactly what `confirm_promotion` will append (one dated entry block per
// touched section, the target note and the promotion date) before any write. It is
// a real modal: initial focus on Cancel, Escape/backdrop dismiss while idle, Tab is
// trapped inside, and focus returns to the Promote button on unmount. A conflict
// receipt is rendered as an explanation, never as an error the UI "fixes" by
// rewriting the user's note.
import {
  useEffect,
  useId,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";

import { Button, Chip } from "../primitives";
import { SECTION_LABELS, plainProvenance, shortHash } from "../../lib/promotion";
import type { PromotionProposal } from "../../bindings/PromotionProposal";
import type { PromotionReceipt } from "../../bindings/PromotionReceipt";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface PromotionDialogProps {
  proposal: PromotionProposal;
  /** `null` while the preview is still awaiting confirmation. */
  receipt: PromotionReceipt | null;
  /** A fresh preview is being built (conflict recovery). */
  preparing: boolean;
  /** `confirm_promotion` is in flight. */
  confirming: boolean;
  /** Failure to build the preview (shown only in the preview view). */
  prepareError: string | null;
  /** Backend error code for `prepareError`, when the failure carried one. */
  prepareErrorCode?: string | null;
  /** Failure thrown by `confirm_promotion`. */
  confirmError: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  onReprepare: () => void;
  onDone: () => void;
}

export function PromotionDialog({
  proposal,
  receipt,
  preparing,
  confirming,
  prepareError,
  prepareErrorCode = null,
  confirmError,
  onConfirm,
  onCancel,
  onReprepare,
  onDone,
}: PromotionDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const openerRef = useRef<Element | null>(null);

  // Move focus in on open; restore it to the triggering control on close (the
  // Promote button stays mounted while the dialog is open).
  useEffect(() => {
    openerRef.current = document.activeElement;
    // Safe default: the first footer control is Cancel/Close/Done — never the
    // primary write action — so an accidental Enter cannot confirm anything.
    const initial =
      dialogRef.current?.querySelector<HTMLElement>("footer button") ??
      dialogRef.current;
    initial?.focus();
    return () => {
      const opener = openerRef.current;
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, []);

  // When the view changes (a receipt replaces the preview), keep focus inside the
  // dialog: the previously focused footer button has unmounted.
  useEffect(() => {
    if (!receipt) return;
    dialogRef.current?.querySelector<HTMLElement>("footer button")?.focus();
  }, [receipt]);

  const busy = confirming || preparing;
  const conflict = receipt?.conflict === true;
  const failed = receipt !== null && !receipt.conflict && receipt.error !== null;
  const succeeded = receipt !== null && !receipt.conflict && receipt.error === null;
  const canConfirm = proposal.note_path !== null && proposal.note_exists;
  const itemCount = proposal.entries.reduce((total, entry) => total + entry.items.length, 0);
  // After a completed write, dismissing means "done": the selection must not stay
  // ticked, or a second Escape-dismiss could promote the same items again.
  const dismiss = succeeded ? onDone : onCancel;

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      if (!busy) {
        event.preventDefault();
        dismiss();
      }
      return;
    }
    if (event.key !== "Tab") return;
    const root = dialogRef.current;
    if (!root) return;
    const focusable = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (node) => node.offsetParent !== null,
    );
    if (focusable.length === 0) {
      event.preventDefault();
      root.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !root.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !root.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  };

  const title = conflict
    ? "The project note changed"
    : failed
      ? "Promotion failed"
      : succeeded
        ? "Promoted to the project note"
        : "Confirm promotion";

  const description = conflict
    ? "Nothing was written. The note is exactly as you left it."
    : failed
      ? "The note was not appended. Your selection is still here."
      : succeeded
        ? "The receipt below records what was written."
        : confirmError
          ? "No receipt was returned, so the write is unconfirmed. Check the note before trying again."
          : "Review the entries before they are appended to the note.";

  const chip = conflict
    ? "nothing written"
    : failed
      ? "not written"
      : succeeded
        ? "written"
        : "preview";

  return (
    <div className="fixed inset-0 z-overlay flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-scrim"
        aria-hidden="true"
        onClick={() => {
          if (!busy) dismiss();
        }}
      />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={handleKeyDown}
        className="relative flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-panel border border-line bg-overlay"
      >
        <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-3">
          <div className="flex flex-col gap-1">
            <h2 id={titleId} className="font-display text-subtitle text-ink">
              {title}
            </h2>
            <p id={descriptionId} className="text-caption text-ink-muted">
              {description}
            </p>
          </div>
          <Chip tone={conflict || failed ? "warn" : succeeded ? "ok" : "neutral"}>
            {chip}
          </Chip>
        </header>

        <div className="flex flex-col gap-4 overflow-y-auto p-4">
          {conflict ? (
            <div className="flex flex-col gap-3 text-body">
              <p>
                This project note changed after you made the selection, so the preview
                is out of date. Promotion never overwrites: the current note was left
                untouched and no entries were appended.
              </p>
              <p className="break-all font-mono text-data text-ink-muted">
                {receipt?.path ?? proposal.note_path ?? "No project note"}
              </p>
              {receipt?.recovery_path && (
                <p className="break-all text-caption text-ink-muted">
                  Preserved copy: {receipt.recovery_path}
                </p>
              )}
              <p className="text-caption text-ink-muted">
                Preparing again only rechecks the note. It cannot clear a conflict on a
                note you edited: the app appends only to the version it last wrote for
                this profile. Restore the last app-written version of the note, or
                choose a different output folder in Settings · Memory reports. If the
                folder or note location changed instead, preparing again previews the
                note that is there now.
              </p>
              {prepareError && (
                <p role="alert" className="text-caption text-danger">
                  Couldn't build a new preview: {prepareError}
                </p>
              )}
            </div>
          ) : failed ? (
            <div className="flex flex-col gap-3 text-body">
              <p>No entries were appended; the note is unchanged.</p>
              {receipt?.error && (
                <p role="alert" className="text-caption text-danger">
                  {receipt.error.message}
                </p>
              )}
              <p className="break-all font-mono text-data text-ink-muted">{receipt?.path}</p>
              {receipt?.recovery_path && (
                <p className="break-all text-caption text-ink-muted">
                  Preserved copy: {receipt.recovery_path}
                </p>
              )}
            </div>
          ) : succeeded && receipt ? (
            <div className="flex flex-col gap-3 text-body">
              <p>
                {itemCount} item{itemCount === 1 ? "" : "s"} appended for{" "}
                {proposal.entry_date}. Earlier lines in the note are unchanged.
              </p>
              <p className="break-all font-mono text-data">{receipt.path}</p>
              {shortHash(receipt.hash) && (
                <p className="text-caption text-ink-muted">
                  New content hash{" "}
                  <span className="font-mono" title={receipt.hash ?? undefined}>
                    {shortHash(receipt.hash)}…
                  </span>
                </p>
              )}
            </div>
          ) : (
            <>
              <dl className="grid grid-cols-1 gap-3 text-caption sm:grid-cols-2">
                <div className="flex flex-col gap-1">
                  <dt className="text-ink-muted">Target note</dt>
                  <dd className="break-all font-mono text-data">
                    {proposal.note_path ?? "No project note"}
                  </dd>
                </div>
                <div className="flex flex-col gap-1">
                  <dt className="text-ink-muted">Promotion date</dt>
                  <dd className="font-mono text-data">{proposal.entry_date}</dd>
                </div>
              </dl>

              {!canConfirm && (
                <p role="alert" className="text-caption text-warn">
                  This note is not available right now — it may have been moved or
                  removed after selection. Nothing will be written. Restore one project
                  note, then prepare the promotion again.
                </p>
              )}

              {prepareError ? (
                <div className="flex flex-col gap-2 text-body">
                  <p role="alert" className="text-caption text-danger">
                    Couldn't build the preview: {prepareError}
                  </p>
                  {prepareErrorCode === "FILE_CONFLICT" && (
                    <p className="text-caption text-ink-muted">
                      Preparing again only rechecks the note. The app appends only to
                      exactly one project note, and only to the version it last wrote for
                      this profile. An edited note or more than one note in Projects/
                      keeps reporting a conflict: restore the last app-written version,
                      keep one note, or choose a different output folder in Settings ·
                      Memory reports.
                    </p>
                  )}
                </div>
              ) : (
                <div className="flex flex-col gap-4">
                  {proposal.entries.map((entry) => (
                    <section
                      key={entry.section}
                      aria-label={`${SECTION_LABELS[entry.section]} entry`}
                      className="flex flex-col gap-1 border-t border-line pt-3 first:border-t-0 first:pt-0"
                    >
                      <h3 className="font-display text-body text-ink">
                        {SECTION_LABELS[entry.section]}
                      </h3>
                      <p className="font-mono text-caption text-ink-muted">
                        ### {entry.heading_date}
                      </p>
                      <ul className="flex list-disc flex-col gap-1 pl-5 text-body text-ink">
                        {entry.items.map((text, index) => (
                          <li key={`${entry.section}-${index}`} className="break-words">
                            {text}
                          </li>
                        ))}
                      </ul>
                      <p className="break-words text-caption italic text-ink-faint">
                        {plainProvenance(entry.provenance)}
                      </p>
                    </section>
                  ))}
                </div>
              )}

              {confirmError && (
                <p role="alert" className="text-caption text-danger">
                  Couldn't promote: {confirmError}
                </p>
              )}
            </>
          )}
        </div>

        <footer className="flex flex-wrap justify-end gap-2 border-t border-line px-4 py-3">
          {conflict ? (
            <>
              <Button size="sm" onClick={onCancel} disabled={preparing}>
                Close
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={onReprepare}
                disabled={preparing}
                aria-busy={preparing}
              >
                {preparing ? "Preparing…" : "Prepare again"}
              </Button>
            </>
          ) : failed ? (
            <>
              <Button size="sm" onClick={onCancel}>
                Close
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={onConfirm}
                disabled={confirming}
                aria-busy={confirming}
              >
                {confirming ? "Promoting…" : "Try again"}
              </Button>
            </>
          ) : succeeded ? (
            <Button variant="primary" size="sm" onClick={onDone}>
              Done
            </Button>
          ) : prepareError ? (
            <>
              <Button size="sm" onClick={onCancel} disabled={preparing}>
                Cancel
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={onReprepare}
                disabled={preparing || !canConfirm}
                aria-busy={preparing}
              >
                {preparing ? "Preparing…" : "Try again"}
              </Button>
            </>
          ) : (
            <>
              <Button size="sm" onClick={onCancel} disabled={confirming || preparing}>
                Cancel
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={onConfirm}
                disabled={confirming || preparing || !canConfirm}
                aria-busy={confirming}
              >
                {confirming ? "Confirming…" : "Confirm"}
              </Button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}
