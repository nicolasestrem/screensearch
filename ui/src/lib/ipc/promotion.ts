// TanStack Query bindings for promotion into the project note (specs/03 §8e, G4).
// Prepare and confirm are mutations, not queries: a proposal is never persisted, so
// caching one would outlive the note state it was built against. The project-note
// status is a query because both the Memory reports panel and the report view read
// the same single note.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import * as cmd from "./commands";
import type { PromotionProposal } from "../../bindings/PromotionProposal";
import type { PromotionSelection } from "../../bindings/PromotionSelection";

export const promotionKeys = {
  noteStatus: ["projectNoteStatus"] as const,
};

/** The single project note (0 notes / one note / unsupported multiple). */
export function useProjectNoteStatus() {
  return useQuery({
    queryKey: promotionKeys.noteStatus,
    queryFn: cmd.getProjectNoteStatus,
  });
}

/** Create the empty user-named note; refreshes the shared status on success. */
export function useCreateProjectNote() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => cmd.createProjectNote(slug),
    onSuccess: (status) => {
      // Flip the shared status line to the new path immediately; the invalidate
      // keeps every other reader honest if the backend derived more than we got.
      qc.setQueryData(promotionKeys.noteStatus, status);
      void qc.invalidateQueries({ queryKey: promotionKeys.noteStatus });
    },
  });
}

/** Build the preview only. This call writes nothing and persists no proposal. */
export function usePreparePromotion() {
  return useMutation({
    mutationFn: (selection: PromotionSelection) => cmd.preparePromotion(selection),
  });
}

/** The only write path: re-checks the selection-time note hash inside the backend. */
export function useConfirmPromotion() {
  return useMutation({
    mutationFn: (proposal: PromotionProposal) => cmd.confirmPromotion(proposal),
    gcTime: 0,
  });
}

/** Structured command errors keep their message; transport errors stay readable. */
export function promotionError(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}
