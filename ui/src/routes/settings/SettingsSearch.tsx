// Settings search (gap 07 #103): finds a row by label, by dotted key or by the
// snake_case field name. Arrow keys move through results, Enter opens one, Escape
// clears the query and leaves the category where it was. An empty result is an
// invitation with two real examples, never a blank pane.
import { useId, useState, type KeyboardEvent } from "react";

import { Field } from "../../components/primitives";
import { cn } from "../../lib/cn";
import {
  searchSettings,
  SETTING_CATEGORIES,
  type SettingCategoryId,
  type SettingKey,
} from "./registry";

export interface SettingsSearchProps {
  onNavigate: (category: SettingCategoryId, key: SettingKey) => void;
}

/** Renders `text` with the first case-insensitive occurrence of `query` marked. */
function Highlight({ text, query }: { text: string; query: string }) {
  const at = query.length === 0 ? -1 : text.toLowerCase().indexOf(query.toLowerCase());
  if (at < 0) return <>{text}</>;
  const end = at + query.length;
  return (
    <>
      {text.slice(0, at)}
      <mark className="bg-mark-bg text-mark-fg">{text.slice(at, end)}</mark>
      {text.slice(end)}
    </>
  );
}

export function SettingsSearch({ onNavigate }: SettingsSearchProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const trimmed = query.trim();
  const results = searchSettings(query);
  const bounded = Math.min(active, Math.max(results.length - 1, 0));

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      setQuery("");
      setActive(0);
      return;
    }
    if (results.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive(Math.min(bounded + 1, results.length - 1));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive(Math.max(bounded - 1, 0));
      return;
    }
    if (event.key === "Enter") {
      const match = results[bounded];
      if (!match) return;
      event.preventDefault();
      onNavigate(match.def.category, match.def.key);
    }
  }

  return (
    <div className="min-w-0" role="search">
      <Field
        label="Search settings"
        placeholder="A name like hotkey, or a key like capture.interval_ms"
        type="search"
        value={query}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={results.length > 0}
        aria-controls={results.length > 0 ? listId : undefined}
        aria-activedescendant={
          results.length > 0 ? `${listId}-${bounded}` : undefined
        }
        onChange={(event) => {
          setQuery(event.target.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      {trimmed.length > 0 && results.length === 0 ? (
        <p role="status" className="mt-2 max-w-prose text-t-sm text-fg-secondary">
          No setting matches “{trimmed}”. Try a word like “hotkey” or a key like
          “capture.interval_ms”.
        </p>
      ) : null}
      {results.length > 0 ? (
        <ul
          id={listId}
          role="listbox"
          aria-label="Matching settings"
          className="mt-2 flex min-w-0 flex-col gap-1"
        >
          {results.map((match, index) => {
            const category =
              SETTING_CATEGORIES.find((entry) => entry.id === match.def.category)
                ?.label ?? match.def.category;
            return (
              <li key={match.def.key} role="none" className="min-w-0">
                <button
                  type="button"
                  role="option"
                  id={`${listId}-${index}`}
                  aria-selected={index === bounded}
                  onClick={() => onNavigate(match.def.category, match.def.key)}
                  onMouseEnter={() => setActive(index)}
                  className={cn(
                    "flex w-full min-w-0 flex-wrap items-center justify-between gap-2 rounded-md px-3 py-2 text-start",
                    "text-t-ui text-fg-primary transition-colors duration-fast ease-standard",
                    index === bounded ? "bg-surface-hover" : "bg-transparent",
                    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
                  )}
                >
                  <span className="flex min-w-0 flex-col">
                    <span className="min-w-0">
                      <Highlight text={match.def.label} query={trimmed} />
                    </span>
                    <span className="text-t-xs text-fg-tertiary">{category}</span>
                  </span>
                  <span className="min-w-0 shrink-0 font-mono text-t-data text-fg-secondary">
                    <Highlight text={match.def.key} query={trimmed} />
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </div>
  );
}
