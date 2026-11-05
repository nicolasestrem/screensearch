// The Export action in Data (`UI_REFERENCE §4` 'Settings · Data'). `export_data`
// streams one JSON file holding every stored frame and mark into the user's
// Downloads folder; it reads the history directly, so it works with the HTTP API
// off. Export is not destructive, but it writes a file, so it goes behind the same
// confirm pattern (0.4.5 D9) and the dialog names exactly what is written and that
// the file is not protected. A failed export keeps the section, explains, and
// offers a retry instead of a silent no-op.
import { useState } from "react";

import { Button } from "../../components/primitives";
import { useExportData } from "../../lib/ipc/mutations";
import { toast } from "../../state/toastStore";
import { ConfirmAction } from "./ConfirmAction";
import { formatBytes } from "./formatBytes";
import { SettingSection } from "./SettingRow";

/** The plain-language reason an export failed. */
function reason(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message.trim();
  }
  if (typeof error === "string" && error.trim().length > 0) return error.trim();
  return "the backend didn’t finish writing the file";
}

export function ExportRow() {
  const exportData = useExportData();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const result = exportData.data;
  const failure = exportData.isError ? reason(exportData.error) : null;

  function run(): void {
    setConfirmOpen(false);
    exportData.mutate(
      { from: null, to: null },
      {
        onSuccess: (written) =>
          toast.success(
            `Exported ${written.frames} frames and ${written.marks} marks to ${written.path}`,
          ),
      },
    );
  }

  return (
    <SettingSection
      title="Export"
      description="Writes one JSON file with every stored frame, its text and every mark. It reads the history directly, so it works while the local API is off."
    >
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <Button
          variant="secondary"
          loading={exportData.isPending}
          onClick={() => setConfirmOpen(true)}
        >
          Export history…
        </Button>
        <span aria-live="polite" className="min-w-0 text-t-xs text-fg-tertiary">
          {exportData.isPending
            ? "Writing the file…"
            : result
              ? `Last export: ${result.frames} frames, ${result.marks} marks, ${formatBytes(result.bytes)} → ${result.path}`
              : "Nothing has been exported from this window yet."}
        </span>
      </div>

      {failure ? (
        <div
          role="alert"
          className="mt-2 flex min-w-0 flex-wrap items-center gap-2 text-t-sm text-status-danger-fg"
        >
          <span className="min-w-0 flex-1">
            Couldn’t export: {failure}. Nothing was changed — the history is intact.
          </span>
          <Button size="sm" variant="ghost" onClick={run}>
            Try again
          </Button>
        </div>
      ) : null}

      <ConfirmAction
        open={confirmOpen}
        onCancel={() => setConfirmOpen(false)}
        onConfirm={run}
        pending={exportData.isPending}
        title="Export all history"
        destroys="Writes one JSON file with every stored frame, its recognised text and every mark into your Downloads folder. The file is not encrypted, so anything that can open it can read your history. Nothing in ScreenSearch is deleted."
        confirmLabel="Export history"
      />
    </SettingSection>
  );
}
