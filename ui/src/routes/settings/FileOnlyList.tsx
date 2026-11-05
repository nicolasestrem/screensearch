// Read-only projection of the `file-only` settings (`03 §8`, `UI_REFERENCE §4`):
// one group per `[section]`, the file's own path from `AdvancedConfigView.path`,
// every effective value, and which of them the file supplied (`from_file`). Key
// names are built as `${section}.${field}` from the object, never typed out.
//
// Nothing here is writable: the file is the control surface, it is read once at
// startup, and this screen has no Save.
import type { AdvancedConfig } from "../../bindings/AdvancedConfig";
import type { AdvancedConfigView } from "../../bindings/AdvancedConfigView";
import { Chip } from "../../components/primitives";

/** How a stored value reads: measured numbers as-is, flags as On/Off. */
function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "Not set";
  if (typeof value === "boolean") return value ? "On" : "Off";
  if (typeof value === "number") return String(value);
  if (typeof value === "string") return value.trim().length === 0 ? "Not set" : value;
  return String(value);
}

/** Every `[section]` of the effective config, each with its field/value pairs. */
function sectionsOf(config: AdvancedConfig): { name: string; fields: [string, unknown][] }[] {
  return Object.entries(config).map(([name, fields]) => ({
    name,
    fields: Object.entries(fields as Record<string, unknown>),
  }));
}

export interface FileOnlyListProps {
  /** The effective config, its path and the keys the file supplied. */
  view: AdvancedConfigView;
}

export function FileOnlyList({ view }: FileOnlyListProps) {
  const sections = sectionsOf(view.config);
  const total = sections.reduce((count, section) => count + section.fields.length, 0);
  const supplied = new Set(view.from_file);

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <div className="flex min-w-0 flex-col gap-1">
        <p className="max-w-prose text-t-sm text-fg-secondary">
          These values are edited in{" "}
          <span className="break-all font-mono text-t-data text-fg-primary">
            {view.path}
          </span>{" "}
          and read once at startup, so a change applies the next time ScreenSearch
          starts. This screen never writes the file.
        </p>
        <p className="min-w-0 text-t-xs text-fg-tertiary tabular-nums">
          {view.from_file.length === 0
            ? `The file sets none of these ${total} values; every one holds its built-in default.`
            : `${view.from_file.length} of ${total} values come from the file; the rest hold their built-in defaults.`}
        </p>
      </div>

      {sections.map((section) => (
        <div key={section.name} className="min-w-0">
          <h3 className="text-t-ui font-medium text-fg-primary">
            {section.name.charAt(0).toUpperCase() + section.name.slice(1)}
          </h3>
          <dl className="mt-1 flex min-w-0 flex-col">
            {section.fields.map(([field, value]) => {
              const key = `${section.name}.${field}`;
              const fromFile = supplied.has(key);
              return (
                <div
                  key={key}
                  className="grid min-w-0 grid-cols-12 items-baseline gap-gap border-b border-line-subtle py-2 last:border-b-0"
                >
                  <dt className="col-span-12 min-w-0 break-all font-mono text-t-data text-fg-secondary md:col-span-7">
                    {key}
                  </dt>
                  <dd className="col-span-12 flex min-w-0 flex-wrap items-baseline gap-2 md:col-span-5 md:justify-end">
                    <span className="min-w-0 break-all font-mono text-t-data tabular-nums text-fg-primary">
                      {formatValue(value)}
                    </span>
                    {fromFile ? (
                      <Chip tone="accent">From file</Chip>
                    ) : (
                      <span className="text-t-xs text-fg-tertiary">Default</span>
                    )}
                  </dd>
                </div>
              );
            })}
          </dl>
        </div>
      ))}
    </div>
  );
}
