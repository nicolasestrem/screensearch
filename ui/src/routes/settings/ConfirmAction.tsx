// Destructive confirm (0.4.5 D9): the dialog names the action and exactly what it
// destroys. It never leans on the autosave/undo toast — the user has to read and
// choose. Cancel takes focus so Enter does not destroy anything by reflex.
import { useEffect, useRef } from "react";

import { Button, Dialog } from "../../components/primitives";

export interface ConfirmActionProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  /** Names the action ("Regenerate token"). */
  title: string;
  /** Names exactly what is destroyed or written ("The old token stops working…"). */
  destroys: string;
  confirmLabel: string;
  pending?: boolean;
  danger?: boolean;
}

export function ConfirmAction({
  open,
  onCancel,
  onConfirm,
  title,
  destroys,
  confirmLabel,
  pending = false,
  danger = false,
}: ConfirmActionProps) {
  const actionsRef = useRef<HTMLDivElement>(null);

  // Dialog focuses its panel on mount; this effect runs after it (child effects
  // run first) and moves focus to Cancel once the panel exists.
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => {
      actionsRef.current?.querySelector("button")?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [open]);

  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={title}
      size="sm"
      actions={
        <div ref={actionsRef} className="flex flex-wrap items-center justify-end gap-2">
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant={danger ? "danger" : "primary"}
            loading={pending}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      }
    >
      <p className="text-t-ui text-fg-secondary">{destroys}</p>
    </Dialog>
  );
}
