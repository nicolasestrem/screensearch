// The state frame every category renders (UI_REFERENCE §4). Frame states are
// `loading` (skeletons reserving the final row geometry), `error` (explains and
// offers one retry), `partial` (keeps the rows, says what is missing) and
// `populated`. Per the lead ruling of 2026-11-05, §4's `empty` cell is an in-frame
// condition, not a frame state: the category renders it as a row status
// (`SettingRow status`) or a section note inside `children`, with its siblings
// live. Never a blank pane, never hidden rows.
import type { ReactNode } from "react";

import { ErrorState, Skeleton } from "../../components/primitives";
import { SETTING_CATEGORIES, type SettingCategoryId } from "./registry";

export interface CategoryStateProps {
  category: SettingCategoryId;
  state: "loading" | "error" | "partial" | "populated";
  /** Skeleton rows to reserve while loading; defaults to three. */
  rows?: number;
  error?: { title: string; message: string; onRetry: () => void };
  /** What is missing and why, shown above the rows in the partial state. */
  partialNote?: string;
  children?: ReactNode;
}

export function CategoryState({
  category,
  state,
  rows = 3,
  error,
  partialNote,
  children,
}: CategoryStateProps) {
  const label =
    SETTING_CATEGORIES.find((entry) => entry.id === category)?.label ?? "Settings";

  return (
    <section aria-label={`${label} settings`} data-state={state} className="min-w-0">
      {state === "loading" ? (
        <div aria-busy="true" className="flex min-w-0 flex-col">
          {Array.from({ length: rows }, (_, index) => (
            <div
              key={index}
              className="grid min-h-row min-w-0 grid-cols-12 items-center gap-gap border-b border-line-subtle py-3 last:border-b-0"
            >
              <div className="col-span-12 flex min-w-0 flex-col gap-2 md:col-span-7">
                <Skeleton w="third" h="control" />
                <Skeleton w="half" h="1" />
              </div>
              <div className="col-span-12 min-w-0 md:col-span-5 md:justify-self-end">
                <Skeleton w="24" h="control" />
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {state === "error" && error ? (
        <ErrorState
          align="start"
          title={error.title}
          message={error.message}
          onRetry={error.onRetry}
        />
      ) : null}

      {state === "partial" ? (
        <p
          role="status"
          className="mb-3 min-w-0 border-l-2 border-line-strong bg-surface-inset px-3 py-2 text-t-sm text-fg-secondary"
        >
          {partialNote ?? "Some of these settings are still loading."}
        </p>
      ) : null}

      {state === "partial" || state === "populated" ? children : null}
    </section>
  );
}
