// CommandPalette (⌘K) — jump-to plus actions (UI_REFERENCE §3). Opening is wired
// globally in AppShell; this owns the query filter, keyboard navigation (↑/↓ to move,
// Enter to run, Esc to close) and focus. The entries themselves live in
// `useShellCommands`, which is where "which commands exist" is decided.
//
// Filtering matches the label and its keywords, so "theme" finds Settings · General and
// "unload" finds the model action. An empty result is an invitation, never a blank pane.
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";

import { cn } from "../../lib/cn";
import { useUiStore } from "../../state/uiStore";
import { useShellCommands, type Command } from "./useShellCommands";

export function CommandPalette() {
  const open = useUiStore((s) => s.paletteOpen);
  const close = useUiStore((s) => s.closePalette);
  const commands = useShellCommands();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const openerRef = useRef<HTMLElement | null>(null);
  // Restore focus to the opener only on *dismiss* (Esc / backdrop), not when a command
  // runs — a command navigates or acts, and the destination (e.g. Recall's autofocused
  // search input) should own focus; restoring would steal it back to the ⌘K trigger.
  const restoreFocusRef = useRef(true);
  const listboxId = "command-palette-listbox";

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((command) =>
      [command.label, ...command.keywords].join(" ").toLowerCase().includes(q),
    );
  }, [commands, query]);

  // Reset and focus the input each time the palette opens, and restore focus to the
  // element that opened it (the ⌘K trigger, or wherever focus was when the hotkey fired)
  // on close — modal focus best practice, so keyboard users aren't dropped to the top of
  // the page.
  useEffect(() => {
    if (!open) return;
    openerRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    restoreFocusRef.current = true; // default to restore; run() opts out
    setQuery("");
    setActive(0);
    const timer = setTimeout(() => inputRef.current?.focus(), 0);
    return () => {
      clearTimeout(timer);
      // Only on dismiss, and only if the opener is still in the document (a command on a
      // now-unmounted page would otherwise be a detached, focus-stealing no-op).
      if (restoreFocusRef.current && openerRef.current?.isConnected) {
        openerRef.current.focus();
      }
    };
  }, [open]);

  // Keep the active index within the (possibly shrunk) filtered list. Reset to 0 when the
  // list is empty so a later non-empty list starts highlighted — a bare
  // `Math.min(active, …)` latches at -1 after an ArrowDown over a zero-match query, which
  // then makes Enter run `filtered[-1]` (undefined) once matches return.
  useEffect(() => {
    setActive((current) =>
      filtered.length > 0
        ? Math.min(Math.max(0, current), filtered.length - 1)
        : 0,
    );
  }, [filtered.length]);

  if (!open) return null;

  const run = (command: Command) => {
    restoreFocusRef.current = false; // the command's destination/action owns focus next
    close();
    command.run();
  };

  const activeOption = filtered[active]
    ? `command-option-${filtered[active].id}`
    : undefined;

  const onKeyDown = (event: ReactKeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "Tab") {
      // The input is the only focusable element in the dialog (options are listbox
      // options reached through aria-activedescendant), so Tab must not walk focus out
      // behind the scrim.
      event.preventDefault();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((current) => Math.min(current + 1, filtered.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => Math.max(current - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const command = filtered[active];
      if (command) run(command);
    }
  };

  return (
    <div
      className="fixed inset-0 z-overlay flex items-start justify-center"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="absolute inset-0 bg-surface-scrim"
        onClick={close}
        aria-hidden="true"
      />
      <div
        className={cn(
          "relative mt-24 w-full max-w-lg overflow-hidden rounded-lg",
          "border border-line-subtle bg-surface-raised shadow-panel",
        )}
        onKeyDown={onKeyDown}
      >
        <div className="p-2">
          <input
            ref={inputRef}
            type="text"
            role="combobox"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Go to a screen, find a setting, or run a command"
            aria-label="Command palette query"
            aria-expanded="true"
            aria-autocomplete="list"
            aria-controls={listboxId}
            aria-activedescendant={activeOption}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            className={cn(
              "w-full min-h-control rounded-md border border-line-control bg-surface-inset px-3",
              "text-t-body font-text text-fg-primary caret-accent placeholder:text-fg-tertiary",
              "transition-colors duration-fast ease-standard motion-reduce:transition-none",
              "hover:border-line-strong",
              "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
            )}
          />
        </div>
        <ul
          id={listboxId}
          role="listbox"
          aria-label="Commands"
          className="max-h-80 overflow-y-auto px-2 pb-2"
          onMouseDown={(event) => event.preventDefault()}
        >
          {filtered.length === 0 ? (
            <li className="px-3 py-2 text-t-sm font-text text-fg-tertiary">
              No command matches “{query.trim()}”.
            </li>
          ) : (
            filtered.map((command, index) => {
              const Icon = command.icon;
              return (
                <li
                  key={command.id}
                  id={`command-option-${command.id}`}
                  role="option"
                  aria-selected={index === active}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => run(command)}
                  className={cn(
                    "flex min-h-row min-w-0 cursor-pointer items-center gap-3 rounded-md px-3",
                    "text-t-ui font-text",
                    index === active
                      ? "bg-accent-wash text-accent"
                      : "text-fg-primary hover:bg-surface-hover",
                  )}
                >
                  <Icon size={18} />
                  <span className="min-w-0 truncate">{command.label}</span>
                </li>
              );
            })
          )}
        </ul>
      </div>
    </div>
  );
}
