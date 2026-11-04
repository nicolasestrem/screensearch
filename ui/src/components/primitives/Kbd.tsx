// Kbd - one key or chord in running text (UI_REFERENCE §2).
//
// Sans, not mono: a key cap is a label, and mono is reserved for measured
// values. Inset surface + control border, 20px visual from the token scale, one
// small radius. Non-interactive, so it has no hit target and no focus ring.
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";

export interface KbdProps extends HTMLAttributes<HTMLElement> {
  children: ReactNode;
}

export function Kbd({ className, children, ...rest }: KbdProps) {
  return (
    <kbd
      {...rest}
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-sm",
        "border border-line-control bg-surface-inset px-1.5 py-1",
        "font-text text-t-xs text-fg-secondary",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
