// Divider - a hairline between content groups (UI_REFERENCE §2).
//
// One pixel from the token scale (--space-px) in --border-subtle. Horizontal by
// default; `vertical` stands on its own in a row of controls. Purely structural,
// so it is a separator role and carries no text.
import type { HTMLAttributes } from "react";
import { cn } from "../../lib/cn";

export interface DividerProps extends HTMLAttributes<HTMLDivElement> {
  /** Draw a vertical rule instead of a horizontal one. */
  vertical?: boolean;
}

export function Divider({ vertical = false, className, ...rest }: DividerProps) {
  return (
    <div
      {...rest}
      role="separator"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      className={cn(
        "shrink-0 bg-line-subtle",
        vertical ? "h-full w-px self-stretch" : "h-px w-full",
        className,
      )}
    />
  );
}
