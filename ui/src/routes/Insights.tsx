// Insights (/insights) — cited insight cards grouped by type (0.4.5 P9 rewrite).
//
// One job (UI_REFERENCE §3): surface a pattern worth knowing, each with the
// evidence that supports it. The five payloads (`03 §7j.1`) are typed statements
// — time and session counts on a thread, something unresolved, a repeat
// sighting, similar sessions, a thread picked up again — and none of them is a
// series over time, so there is no honest chart to draw: a chart would have to
// aggregate across kinds and order findings against each other, which `03 §7j.4`
// (0.4.5 D13) forbids. Grouping is the only structure this screen adds.
//
//   order     the five kinds in a fixed order; `generated_at` desc inside a
//             group (row id breaks a tie), and nothing else — never a rank.
//   window    lives in the URL (`?window=today|7d|30d|custom&from=&to=`), so a
//             reload or a shared link keeps the picked window. `custom` is two
//             date inputs resolved to the half-open `[start, end)` the wire type
//             expects.
//   states    loading   the real header + group skeletons (real headings, no
//                       invented counts, card geometry reserved);
//             error     retry that keeps the picked window;
//             empty     "Not enough history yet" when no pass has ever written a
//                       row, else "No insights for this window";
//             partial   `status.pending_jobs > 0`, labelled, and the rows that
//                       do exist render normally — nothing is filled in;
//             populated the five groups, each with its count as text.
//
// A group with no rows still renders "None in this window", so an empty type is a
// statement rather than a silence. `InsightCard` already labels a single stale
// row; when every row in the window is stale the page says so once at the top
// instead of turning it into an alarm. Nothing here notifies, badges, streaks,
// scores, grades or ranks (0.4.5 D13).
import { useMemo, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";

import {
  Button,
  EmptyState,
  ErrorState,
  Field,
  Panel,
  Segmented,
  Skeleton,
} from "../components/primitives";
import { formatCount, InsightCard } from "../components/intelligence";
import { IconInsights } from "../components/icons";
import type { Insight } from "../bindings/Insight";
import type { InsightKind } from "../bindings/InsightKind";
import type { TimeRange } from "../bindings/TimeRange";
import { useEntities, useInsightList } from "../lib/ipc/queries";
import { absoluteDate } from "../lib/time";
import { lastDaysRange, todayRange } from "../lib/timeRanges";

/** The command clamps `limit` to `1..=1000`; asking for the full page and saying so
 *  when it comes back full is the only honest way to know a window was truncated. */
const INSIGHT_LIMIT = 1000;

/** The five kinds in the one order this screen renders them (`03 §7j.1`). */
const GROUP_ORDER: InsightKind[] = [
  "thread_activity",
  "open_loop",
  "recurrence",
  "semantic_neighbour",
  "resumption",
];

/** The group heading: the plain-language name of the kind, never the token. */
const GROUP_HEADING: Record<InsightKind, string> = {
  thread_activity: "Thread activity",
  open_loop: "Open loops",
  recurrence: "Recurrence",
  semantic_neighbour: "Semantic neighbours",
  resumption: "Resumption",
};

type WindowChoice = "today" | "7d" | "30d" | "custom";

const DEFAULT_WINDOW: WindowChoice = "7d";

const WINDOW_OPTIONS = [
  { value: "today", label: "Today" },
  { value: "7d", label: "7 days" },
  { value: "30d", label: "30 days" },
  { value: "custom", label: "Custom" },
];

function isWindowChoice(value: string | null): value is WindowChoice {
  return WINDOW_OPTIONS.some((option) => option.value === value);
}

/** Local midnight of the day containing `ms` (`Date` arithmetic, so DST is the host's). */
function startOfLocalDay(ms: number): number {
  const at = new Date(ms);
  return new Date(at.getFullYear(), at.getMonth(), at.getDate()).getTime();
}

/** The first instant of the next local day — the exclusive end of that day. */
function nextLocalMidnight(ms: number): number {
  const at = new Date(ms);
  return new Date(at.getFullYear(), at.getMonth(), at.getDate() + 1).getTime();
}

/** A `YYYY-MM-DD` date input value in local time; `null` when it is not a real day. */
function parseLocalDay(value: string | null): number | null {
  if (!value) return null;
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]) - 1;
  const day = Number(match[3]);
  const stamp = new Date(year, month, day).getTime();
  const check = new Date(stamp);
  // `2026-02-31` rolls over to March; only a day that reads back unchanged is real.
  if (
    check.getFullYear() !== year ||
    check.getMonth() !== month ||
    check.getDate() !== day
  ) {
    return null;
  }
  return stamp;
}

/** The local calendar day of `ms`, in the form a date input expects. */
function isoDay(ms: number): string {
  const at = new Date(ms);
  const month = String(at.getMonth() + 1).padStart(2, "0");
  const day = String(at.getDate()).padStart(2, "0");
  return `${at.getFullYear()}-${month}-${day}`;
}

interface CustomDays {
  /** Local midnight of the first included day. */
  from: number;
  /** Local midnight of the last included day. */
  to: number;
}

/**
 * The two days a `custom` window covers. A missing or unparsable date falls back
 * to the last seven whole days, so a link that carries only `window=custom` shows
 * a real window (and the inputs show exactly that window) rather than an empty
 * form. A start after the end collapses to that one day: the window is half-open,
 * and an inverted range would ask the store for nothing.
 */
function customDays(fromParam: string | null, toParam: string | null): CustomDays {
  const week = lastDaysRange(7);
  const fallbackFrom = startOfLocalDay(week.start);
  const fallbackTo = startOfLocalDay(week.end - 1);
  const from = parseLocalDay(fromParam) ?? fallbackFrom;
  const rawTo = parseLocalDay(toParam) ?? fallbackTo;
  return { from, to: rawTo < from ? from : rawTo };
}

/** The window actually read for a URL state; `custom` is non-null only for `custom`. */
function resolveWindow(
  choice: WindowChoice,
  fromParam: string | null,
  toParam: string | null,
): { range: TimeRange; custom: CustomDays | null } {
  switch (choice) {
    case "today":
      return { range: todayRange(), custom: null };
    case "30d":
      return { range: lastDaysRange(30), custom: null };
    case "custom": {
      const custom = customDays(fromParam, toParam);
      return {
        range: { start: custom.from, end: nextLocalMidnight(custom.to) },
        custom,
      };
    }
    case "7d":
    default:
      return { range: lastDaysRange(7), custom: null };
  }
}

/**
 * The entity ids a window's payloads mention: a recurrence names one, and any
 * evidence row of kind `entity` names one. Names are resolved for those ids so a
 * card shows the entity instead of echoing an id.
 */
function mentionedEntityIds(rows: Insight[]): number[] {
  const ids = new Set<number>();
  for (const row of rows) {
    if (row.payload.kind === "recurrence") ids.add(row.payload.entity_id);
    for (const item of row.evidence) {
      if (item.kind === "entity") ids.add(item.ref_id);
    }
  }
  // Sorted so the read's query key is stable whatever order the payloads arrive in.
  return [...ids].sort((a, b) => a - b);
}

export function Component() {
  const [searchParams, setSearchParams] = useSearchParams();

  const requested = searchParams.get("window");
  const choice: WindowChoice = isWindowChoice(requested)
    ? requested
    : DEFAULT_WINDOW;
  const { range, custom } = resolveWindow(
    choice,
    searchParams.get("from"),
    searchParams.get("to"),
  );

  const insights = useInsightList({
    from_ms: range.start,
    to_ms: range.end,
    kind: null,
    limit: INSIGHT_LIMIT,
  });

  const rows = useMemo(() => insights.data?.insights ?? [], [insights.data]);

  // The only grouping this screen does: the fixed kind order, newest generation
  // first inside a kind. Every group is present, so an empty type is stated.
  const groups = useMemo(() => {
    const byKind = new Map<InsightKind, Insight[]>();
    for (const kind of GROUP_ORDER) byKind.set(kind, []);
    for (const row of rows) byKind.get(row.kind)?.push(row);
    return GROUP_ORDER.map((kind) => {
      const groupRows = byKind.get(kind) ?? [];
      groupRows.sort((a, b) => b.generated_at - a.generated_at || b.id - a.id);
      return { kind, rows: groupRows };
    });
  }, [rows]);

  const entityIds = useMemo(() => mentionedEntityIds(rows), [rows]);
  const entities = useEntities(entityIds);
  const entityNames = useMemo(() => {
    const names: Record<number, string> = {};
    for (const entity of entities.data ?? []) names[entity.id] = entity.canonical_name;
    return names;
  }, [entities.data]);

  const chooseWindow = (next: string) => {
    const nextChoice: WindowChoice = isWindowChoice(next) ? next : DEFAULT_WINDOW;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("window", nextChoice);
        if (nextChoice === "custom") {
          // Custom starts from the days already on screen, so picking it never
          // changes what is rendered until a date is edited.
          params.set("from", isoDay(range.start));
          params.set("to", isoDay(range.end - 1));
        } else {
          params.delete("from");
          params.delete("to");
        }
        return params;
      },
      { replace: true },
    );
  };

  const setCustomDay = (edge: "from" | "to", value: string) => {
    if (!custom) return;
    const day = parseLocalDay(value);
    const from = edge === "from" && day !== null ? day : custom.from;
    const to = edge === "to" && day !== null ? day : custom.to;
    // Keep the window readable: when an edit would invert it, the other edge
    // follows, so the two inputs always describe the window that is fetched.
    const nextFrom = edge === "from" ? from : Math.min(from, to);
    const nextTo = edge === "from" ? Math.max(from, to) : to;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("window", "custom");
        params.set("from", isoDay(nextFrom));
        params.set("to", isoDay(nextTo));
        return params;
      },
      { replace: true },
    );
  };

  // The header is known before the read returns — the window and its days come
  // straight from the URL — so it renders in every state and never moves when the
  // rows arrive.
  const header = (
    <header className="flex min-w-0 flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <h1 className="text-t-h1 font-semibold text-fg-primary">Insights</h1>
        <p className="mt-1 max-w-prose text-t-ui text-fg-secondary">
          Patterns worth knowing from your own history, each with the evidence
          that supports it. Grouped by type, newest first inside a type.
        </p>
        <p className="mt-1 text-t-xs text-fg-tertiary">
          {absoluteDate(range.start)} — {absoluteDate(range.end - 1)}
        </p>
      </div>
      <div className="flex min-w-0 flex-col items-start gap-2">
        <Segmented
          label="Time window"
          options={WINDOW_OPTIONS}
          value={choice}
          onChange={chooseWindow}
        />
        {custom && (
          <div className="flex flex-wrap items-end gap-3">
            <Field
              className="w-40"
              label="From"
              type="date"
              value={isoDay(custom.from)}
              max={isoDay(Date.now())}
              onChange={(event) => setCustomDay("from", event.target.value)}
            />
            <Field
              className="w-40"
              label="To"
              type="date"
              value={isoDay(custom.to)}
              max={isoDay(Date.now())}
              onChange={(event) => setCustomDay("to", event.target.value)}
            />
          </div>
        )}
      </div>
    </header>
  );

  const page = (body: ReactNode) => (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
      {header}
      {body}
    </div>
  );

  if (insights.isLoading) {
    return page(
      <div className="flex min-w-0 flex-col gap-pane" aria-busy="true">
        <p role="status" className="sr-only">
          Loading insights…
        </p>
        {/* The five headings are fixed, so the loading state can reserve every
            group's geometry and its count line without inventing a count. */}
        {GROUP_ORDER.map((kind) => (
          <Panel key={kind} title={GROUP_HEADING[kind]} action={<Skeleton w="16" h="4" />}>
            <div className="grid gap-3">
              <Skeleton w="third" h="5" />
              <Skeleton w="half" h="4" />
              <Skeleton w="twoThirds" h="4" />
              <Skeleton w="quarter" h="4" />
              <Skeleton w="third" h="4" />
            </div>
          </Panel>
        ))}
      </div>,
    );
  }

  if (insights.isError || !insights.data) {
    // The picked window is in the URL, so a retry re-reads the same one.
    return page(
      <ErrorState
        title="Couldn't load insights"
        message={String(insights.error ?? "The insight read failed.")}
        onRetry={() => insights.refetch()}
      />,
    );
  }

  const { status } = insights.data;
  const partial = status.pending_jobs > 0;
  const allStale = rows.length > 0 && rows.every((row) => row.stale);
  const truncated = rows.length >= INSIGHT_LIMIT;

  // Empty, two honest cases: nothing has ever been generated (the status is a
  // global MAX, so `null` means no pass has written a row at all), or passes have
  // run and this window holds none of their findings.
  if (rows.length === 0) {
    return page(
      <>
        {partial && (
          <p role="status" className="m-0 text-t-ui text-fg-secondary">
            Still computing insights for this window —{" "}
            {formatCount(status.pending_jobs, "pass")} left
          </p>
        )}
        {status.last_generated_at === null ? (
          // No pass has ever written a row, so the only honest next step is to say
          // how rows appear — there is no control anywhere that would start one.
          <EmptyState
            icon={<IconInsights size={28} />}
            title="Not enough history yet"
            description={
              <>
                No insight has been written yet. Insights come from your own
                captured history and are computed on demand or while the app is
                idle.
              </>
            }
          />
        ) : (
          <EmptyState
            icon={<IconInsights size={28} />}
            title="No insights for this window"
            description={
              <>
                There are insights outside this window, or this stretch of
                history held none. Widen the window to include more of it.
              </>
            }
            action={
              choice === "30d" || choice === "custom" ? undefined : (
                <Button variant="secondary" onClick={() => chooseWindow("30d")}>
                  Look at the last 30 days
                </Button>
              )
            }
          />
        )}
      </>,
    );
  }

  return page(
    <>
      {partial && (
        <p role="status" className="m-0 text-t-ui text-fg-secondary">
          Still computing insights for this window —{" "}
          {formatCount(status.pending_jobs, "pass")} left
        </p>
      )}
      {allStale && (
        <p className="m-0 text-t-ui text-fg-secondary">
          These insights may be out of date.
        </p>
      )}
      {truncated && (
        <p className="m-0 text-t-xs text-fg-secondary">
          This window holds at least {formatCount(INSIGHT_LIMIT, "insight")}; only
          the first page is shown. Narrow the window to see the rest.
        </p>
      )}
      <div className="flex min-w-0 flex-col gap-pane">
        {groups.map((group) => (
          <Panel
            key={group.kind}
            title={GROUP_HEADING[group.kind]}
            action={
              <span className="text-t-xs text-fg-secondary">
                {formatCount(group.rows.length, "insight")}
              </span>
            }
          >
            {group.rows.length === 0 ? (
              <p className="m-0 text-t-ui text-fg-secondary">None in this window</p>
            ) : (
              <div className="flex min-w-0 flex-col gap-pane">
                {group.rows.map((row) => (
                  <div
                    key={row.id}
                    className="min-w-0 border-b border-line-subtle pb-pane last:border-b-0 last:pb-0"
                  >
                    <InsightCard insight={row} entityNames={entityNames} />
                  </div>
                ))}
              </div>
            )}
          </Panel>
        ))}
      </div>
    </>,
  );
}
