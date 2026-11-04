// Skeleton - a layout-reserving block for loading states (UI_REFERENCE §4/§8).
//
// Inset surface, small radius, aria-hidden. The pulse is a state cue only and is
// disabled under prefers-reduced-motion. `w` / `h` take token scale steps so a
// caller never types a px value; className still accepts layout classes for
// call sites that predate the props.
import type { HTMLAttributes } from "react";
import { cn } from "../../lib/cn";

/** Width steps from the token spacing scale (plus fractional widths). */
export type SkeletonWidth =
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "8"
  | "10"
  | "12"
  | "16"
  | "24"
  | "32"
  | "quarter"
  | "third"
  | "half"
  | "twoThirds"
  | "full";

/** Height steps from the token spacing scale plus density-aware control heights. */
export type SkeletonHeight =
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "8"
  | "10"
  | "12"
  | "16"
  | "row"
  | "control"
  | "controlSm"
  | "hit";

const WIDTHS: Record<SkeletonWidth, string> = {
  "1": "w-1",
  "2": "w-2",
  "3": "w-3",
  "4": "w-4",
  "5": "w-5",
  "6": "w-6",
  "8": "w-8",
  "10": "w-10",
  "12": "w-12",
  "16": "w-16",
  "24": "w-24",
  "32": "w-32",
  quarter: "w-1/4",
  third: "w-1/3",
  half: "w-1/2",
  twoThirds: "w-2/3",
  full: "w-full",
};

const HEIGHTS: Record<SkeletonHeight, string> = {
  "1": "h-1",
  "2": "h-2",
  "3": "h-3",
  "4": "h-4",
  "5": "h-5",
  "6": "h-6",
  "8": "h-8",
  "10": "h-10",
  "12": "h-12",
  "16": "h-16",
  row: "h-row",
  control: "h-control",
  controlSm: "h-control-sm",
  hit: "h-hit-min",
};

export interface SkeletonProps extends HTMLAttributes<HTMLDivElement> {
  /** Fully round the block (avatar / status dot placeholders). */
  circle?: boolean;
  /** Token width step. */
  w?: SkeletonWidth;
  /** Token height step. */
  h?: SkeletonHeight;
}

export function Skeleton({
  circle = false,
  w,
  h,
  className,
  ...rest
}: SkeletonProps) {
  return (
    <div
      {...rest}
      aria-hidden="true"
      className={cn(
        "animate-pulse bg-surface-inset motion-reduce:animate-none",
        circle ? "rounded-pill" : "rounded-sm",
        w && WIDTHS[w],
        h && HEIGHTS[h],
        className,
      )}
    />
  );
}
