// Badge - a status label that always carries its shape (UI_REFERENCE §1/§7).
//
// ok / warn / danger take the three-part status token families and always render
// their icon (check circle, warn triangle, alert circle), so status is never
// colour alone. Pine's status-ok family is a neutral treatment and ink's is
// green; both come from the same tokens, so there is no theme branch here.
// neutral and accent are tone only and add no glyph.
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../lib/cn";
import { IconAlert, IconCheckCircle, IconWarn } from "./icons";

export type BadgeTone = "ok" | "warn" | "danger" | "neutral" | "accent";

const TONES: Record<BadgeTone, string> = {
  ok: "border-status-ok-border bg-status-ok-bg text-status-ok-fg",
  warn: "border-status-warn-border bg-status-warn-bg text-status-warn-fg",
  danger: "border-status-danger-border bg-status-danger-bg text-status-danger-fg",
  neutral: "border-line-subtle bg-surface-inset text-fg-secondary",
  accent: "border-accent-line bg-accent-wash text-accent",
};

const ICONS: Record<BadgeTone, ReactNode> = {
  ok: <IconCheckCircle size={12} />,
  warn: <IconWarn size={12} />,
  danger: <IconAlert size={12} />,
  neutral: null,
  accent: null,
};

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
  children: ReactNode;
}

export function Badge({ tone = "neutral", className, children, ...rest }: BadgeProps) {
  const icon = ICONS[tone];
  return (
    <span
      {...rest}
      className={cn(
        "inline-flex max-w-full items-center gap-1 whitespace-nowrap",
        "rounded-pill border px-2 py-1.5 font-text text-t-xs font-medium",
        TONES[tone],
        className,
      )}
    >
      {icon}
      <span className="truncate">{children}</span>
    </span>
  );
}
