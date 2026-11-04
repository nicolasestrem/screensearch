// Dev-only design-system kitchen (/kitchen). It renders the real primitives
// against the real tokens, in the shapes the product uses them: a settings
// sample with one row per Settings v2 category, a search result list, a session
// card, an Ask answer with citations, and the full primitive gallery.
//
// Two live switches sit in the header: density (--row-h / --control-h via
// `data-density`) and theme (`data-theme`, with ?theme= read on mount so a
// screenshot run can deep-link a theme). Neither touches router state.
//
// The route is gated on `import.meta.env.DEV` in router.tsx: Vite folds the
// guard to `false` and the dynamic import, this file and KitchenPrimitives all
// tree-shake out of a production build.
import { useEffect, useState, type ReactNode } from "react";

import {
  Badge,
  Button,
  Checkbox,
  Chip,
  Expander,
  Field,
  ListRow,
  Panel,
  RadioGroup,
  Segmented,
  Select,
  Slider,
  Table,
  Toggle,
  type TableColumn,
} from "../components/primitives";
import {
  IconCapture,
  IconChevronRight,
  IconRecall,
  IconSearch,
  IconSparkle,
} from "../components/primitives/icons";
import { KitchenGallery } from "./KitchenPrimitives";

const NOOP = () => {};

type KitchenTheme = "pine-light" | "pine-dark" | "ink-light" | "ink-dark";
type KitchenDensity = "comfortable" | "compact";

const KITCHEN_THEMES: Array<{ id: KitchenTheme; label: string }> = [
  { id: "pine-light", label: "Pine, light" },
  { id: "pine-dark", label: "Pine, dark" },
  { id: "ink-light", label: "Ink, light" },
  { id: "ink-dark", label: "Ink, dark" },
];

const KITCHEN_DENSITIES: Array<{ id: KitchenDensity; label: string }> = [
  { id: "comfortable", label: "Comfortable" },
  { id: "compact", label: "Compact" },
];

/** Section shell: heading, one line of intent, then the sample. */
function KitchenSection({
  id,
  title,
  intro,
  children,
}: {
  id: string;
  title: string;
  intro: string;
  children: ReactNode;
}) {
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="grid gap-3">
      <div>
        <h2 id={`${id}-title`} className="m-0 font-text text-t-h3 font-medium text-fg-primary">
          {title}
        </h2>
        <p className="m-0 mt-1 max-w-prose text-t-ui text-fg-secondary">{intro}</p>
      </div>
      {children}
    </section>
  );
}

/**
 * One settings row: label + plain-language effect on the left, the control on
 * the right, and the autosave marker instead of a Save button. This is the only
 * row pattern Settings v2 allows.
 */
function KitchenSettingsRow({
  label,
  effect,
  saved = false,
  children,
}: {
  label: string;
  effect: string;
  saved?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-3 border-b border-line-subtle py-4 last:border-b-0 md:grid-cols-12 md:gap-6">
      <div className="md:col-span-5">
        <div className="flex items-center gap-2">
          <p className="m-0 text-t-ui font-medium text-fg-primary">{label}</p>
          {saved ? <Badge tone="ok">Saved</Badge> : null}
        </div>
        <p className="m-0 mt-1 text-t-xs text-fg-secondary">{effect}</p>
      </div>
      <div className="grid gap-3 md:col-span-7">{children}</div>
    </div>
  );
}

/** Settings v2: eight categories, one row each, autosave with a Saved marker. */
function KitchenSettingsSample() {
  const [startup, setStartup] = useState(true);
  const [pauseOnLock, setPauseOnLock] = useState(true);
  const [includeChrome, setIncludeChrome] = useState(false);
  const [requireToken, setRequireToken] = useState(true);
  const [interval, setInterval] = useState(8);
  const [exportFormat, setExportFormat] = useState("md");
  const [captureTuning, setCaptureTuning] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const markSaved = (row: string) => setSaved(row);

  return (
    <section aria-labelledby="kitchen-settings-title" className="grid gap-2">
      <div className="flex items-baseline justify-between gap-3 border-b border-line-subtle pb-3">
        <h2 id="kitchen-settings-title" className="m-0 text-t-h2 font-semibold text-fg-primary">
          Settings
        </h2>
        <Badge tone="ok">Saved</Badge>
      </div>
      <div className="grid">
        <KitchenSettingsRow
          label="General"
          effect="Profile name and whether ScreenSearch starts with Windows."
          saved={saved === "general"}
        >
          <Field
            label="Profile name"
            hint="Shown in exports and the tray tooltip."
            defaultValue="Workstation"
            onChange={() => markSaved("general")}
          />
          <Toggle
            label="Open at login"
            checked={startup}
            onChange={(next) => {
              setStartup(next);
              markSaved("general");
            }}
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Capture"
          effect="How often a frame is kept, and which displays it comes from."
          saved={saved === "capture"}
        >
          <Slider
            label="Capture interval"
            value={interval}
            onChange={(next) => {
              setInterval(next);
              markSaved("capture");
            }}
            min={2}
            max={60}
            unit="s"
            hint="The timer only spends a frame when the screen changed."
          />
          <Segmented
            label="Monitors"
            options={[
              { value: "all", label: "All displays" },
              { value: "active", label: "Active window" },
            ]}
            defaultValue="all"
            onChange={() => markSaved("capture")}
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Privacy"
          effect="What is never captured, and what pauses capture."
          saved={saved === "privacy"}
        >
          <Toggle
            label="Pause when the screen locks"
            checked={pauseOnLock}
            onChange={(next) => {
              setPauseOnLock(next);
              markSaved("privacy");
            }}
            hint="ScreenSearch always excludes its own window."
          />
          <Field
            label="Excluded app rule"
            error="The pattern must end with .exe"
            defaultValue="slack"
            onChange={() => markSaved("privacy")}
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Intelligence"
          effect="What the reader sees on a recall: content text, or the raw buffer."
          saved={saved === "intelligence"}
        >
          <Segmented
            label="Recall text"
            options={[
              { value: "content", label: "Content" },
              { value: "raw", label: "Raw text" },
            ]}
            defaultValue="content"
            onChange={() => markSaved("intelligence")}
          />
          <Checkbox
            label="Include app chrome"
            checked={includeChrome}
            onChange={(next) => {
              setIncludeChrome(next);
              markSaved("intelligence");
            }}
            hint="Adds toolbars and menus to the searchable text."
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Models"
          effect="Which vision and answer tiers run on this machine."
          saved={saved === "models"}
        >
          <Select
            label="Vision tier"
            options={[
              { value: "fast", label: "Fast" },
              { value: "balanced", label: "Balanced" },
              { value: "quality", label: "Quality" },
            ]}
            defaultValue="balanced"
            onChange={() => markSaved("models")}
          />
          <Select
            label="Answer tier"
            options={[{ value: "small", label: "Small (not downloaded)" }]}
            defaultValue="small"
            hint="Download the model to change this tier."
            disabled
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Data"
          effect="How long frames stay, and what an export looks like."
          saved={saved === "data"}
        >
          <Field
            label="Retention days"
            type="number"
            hint="Applies from the next sweep, never mid-range."
            defaultValue="30"
            onChange={() => markSaved("data")}
          />
          <RadioGroup
            label="Export format"
            orientation="horizontal"
            options={[
              { value: "md", label: "Markdown" },
              { value: "json", label: "JSON" },
            ]}
            value={exportFormat}
            onChange={(next) => {
              setExportFormat(next);
              markSaved("data");
            }}
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Integrations"
          effect="The local HTTP API other tools can call."
          saved={saved === "integrations"}
        >
          <Field
            label="HTTP API port"
            hint="Loopback only; the window never leaves 127.0.0.1."
            defaultValue="8765"
            onChange={() => markSaved("integrations")}
          />
          <Checkbox
            label="Require a token"
            checked={requireToken}
            onChange={(next) => {
              setRequireToken(next);
              markSaved("integrations");
            }}
          />
        </KitchenSettingsRow>

        <KitchenSettingsRow
          label="Advanced"
          effect="File-only tuning: advanced.toml is the control surface, there is no Save."
          saved={saved === "advanced"}
        >
          <Expander
            title="Capture tuning"
            intro="Only change these when capture skips frames."
            open={captureTuning}
            onToggle={() => {
              setCaptureTuning(!captureTuning);
              markSaved("advanced");
            }}
          >
            <div className="grid gap-3">
              <Checkbox
                label="Run UI Automation on interactive windows"
                checked={false}
                onChange={NOOP}
                hint="uia_run_on_interactive in advanced.toml."
              />
              <Field label="Frame poll" defaultValue="250" hint="Milliseconds while an app is interactive." />
              <div className="flex items-center gap-2 text-t-xs text-fg-tertiary">
                <span>File</span>
                <span className="num">%APPDATA%\ScreenSearch\advanced.toml</span>
              </div>
            </div>
          </Expander>
        </KitchenSettingsRow>
      </div>
    </section>
  );
}

/** Recall search results: six rows, measured values, one selected. */
function KitchenSearchSample() {
  const results = [
    {
      id: "r1",
      title: "Q3 proposal with the pricing table",
      meta: "Word · 12 Oct, 14:32",
      trailing: "94%",
      icon: <IconRecall size={16} />,
    },
    {
      id: "r2",
      title: "Grafana dashboard: capture latency p95",
      meta: "Firefox · 12 Oct, 14:18",
      trailing: "88%",
      icon: <IconCapture size={16} />,
    },
    {
      id: "r3",
      title: "npm run test: 82 passing in the ui package",
      meta: "Terminal · 12 Oct, 13:59",
      trailing: "81%",
      icon: <IconSparkle size={16} />,
    },
    {
      id: "r4",
      title: "Token fence review comments",
      meta: "VS Code · 12 Oct, 13:44",
      trailing: "77%",
      icon: <IconSearch size={16} />,
    },
    {
      id: "r5",
      title: "Contrast ratio table for four themes",
      meta: "Firefox · 12 Oct, 13:31",
      trailing: "72%",
      icon: <IconCapture size={16} />,
    },
    {
      id: "r6",
      title: "Session card draft, five-column metrics",
      meta: "Figma · 12 Oct, 13:07",
      trailing: "68%",
      icon: <IconRecall size={16} />,
    },
  ];

  return (
    <Panel
      title="Results for “design tokens”"
      action={<span className="num text-t-xs text-fg-tertiary">148 frames</span>}
      flush
    >
      <div className="grid">
        {results.map((result) => (
          <ListRow
            key={result.id}
            as="button"
            icon={result.icon}
            title={result.title}
            meta={result.meta}
            trailing={
              <>
                <span className="num">{result.trailing}</span>
                <IconChevronRight size={14} />
              </>
            }
            selected={result.id === "r2"}
          />
        ))}
      </div>
    </Panel>
  );
}

interface KitchenSessionRow {
  id: string;
  frame: string;
  time: string;
  app: string;
  text: string;
  match: number;
}

const KITCHEN_SESSION_ROWS: KitchenSessionRow[] = [
  { id: "s1", frame: "041238", time: "14:28:04", app: "VS Code", text: "tokens.css: contrast fence", match: 96 },
  { id: "s2", frame: "041744", time: "14:29:31", app: "Terminal", text: "npm run test -- contrast", match: 91 },
  { id: "s3", frame: "042105", time: "14:31:02", app: "Firefox", text: "WCAG 2.2 contrast minimum", match: 87 },
  { id: "s4", frame: "042548", time: "14:32:48", app: "Explorer", text: "ui/src/styles/tokens.css", match: 79 },
  { id: "s5", frame: "042912", time: "14:34:12", app: "Teams", text: "Design review moved to 15:00", match: 64 },
];

const KITCHEN_SESSION_COLUMNS: Array<TableColumn<KitchenSessionRow>> = [
  { id: "frame", header: "Frame", numeric: true },
  { id: "time", header: "Time", numeric: true },
  { id: "app", header: "App" },
  { id: "text", header: "On-screen text" },
  { id: "match", header: "Match", numeric: true, cell: (row) => `${row.match}%` },
];

/** Session card: Panel header, a Chip for the state, one metrics table. */
function KitchenSessionSample() {
  return (
    <Panel
      title="Session · screensearch, VS Code"
      action={
        <div className="flex items-center gap-2">
          <Chip tone="accent" dot>
            Capturing
          </Chip>
          <Chip tone="ok">Saved</Chip>
        </div>
      }
      flush
    >
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2 px-pane py-3 text-t-xs text-fg-secondary">
        <span>
          <span className="num">09:14</span> to <span className="num">14:32</span>
        </span>
        <span>
          <span className="num">1,208</span> frames
        </span>
        <span>
          <span className="num">37</span> marks
        </span>
        <span className="num">4.2 GB retained</span>
      </div>
      <Table
        columns={KITCHEN_SESSION_COLUMNS}
        rows={KITCHEN_SESSION_ROWS}
        getRowId={(row) => row.id}
        caption="Key frames from this session"
        selectedRowId="s3"
        stickyHeader={false}
      />
    </Panel>
  );
}

/** Ask answer: prose with two citations and clickable sources. */
function KitchenAskSample() {
  return (
    <Panel
      title="Ask"
      action={
        <Chip tone="neutral">
          Small answer model
        </Chip>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-3 text-t-prose text-fg-primary">
          <p className="m-0 max-w-prose">
            You last reviewed the contrast fence on Tuesday, while the token rewrite was
            still open: the notes say every semantic pair must clear 4.5:1 in all four
            themes <mark>1</mark>, and the failing pair was <span className="num">tertiary on selected</span>{" "}
            at 4.12:1 before the pine wash was darkened <mark>2</mark>.
          </p>
          <p className="m-0 max-w-prose">
            Two follow-ups were captured in the same session: the density switch has to
            reach the row height, and the kitchen needed a Saved badge instead of a Save
            button. Neither shows a completion mark, so both are still open.
          </p>
        </div>
        <div className="grid">
          <p className="m-0 px-pane pb-1 pt-2 text-t-xs font-medium text-fg-secondary">
            Sources
          </p>
          <ListRow
            as="a"
            href="/recall?q=contrast%20fence"
            icon={<IconCapture size={16} />}
            title="Token fence review · VS Code"
            meta="Screen 1 · 12 Oct"
            trailing={<span className="num">frame 041238</span>}
          />
          <ListRow
            as="a"
            href="/recall?q=contrast%20ratio"
            icon={<IconSearch size={16} />}
            title="WCAG contrast table · Firefox"
            meta="Screen 1 · 12 Oct"
            trailing={<span className="num">frame 041744</span>}
          />
        </div>
      </div>
    </Panel>
  );
}

export function Component() {
  const [theme, setTheme] = useState<KitchenTheme>("pine-light");
  const [density, setDensity] = useState<KitchenDensity>("comfortable");

  // Deep link a theme for screenshot runs; the router owns no state here.
  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get("theme");
    const match = KITCHEN_THEMES.find((entry) => entry.id === requested);
    if (match) setTheme(match.id);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.density = density;
  }, [density]);

  return (
    <div className="grid gap-8 p-pane">
      <header className="grid gap-4 border-b border-line-subtle pb-4">
        <div className="grid gap-1">
          <h1 className="m-0 font-text text-t-h1 font-medium text-fg-primary">Kitchen</h1>
          <p className="m-0 max-w-prose text-t-ui text-fg-secondary">
            Every primitive on the real tokens, in the shapes the product uses them. A dev
            route: it is gated on <span className="num">import.meta.env.DEV</span> and is
            absent from a production build.
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-6">
          <div className="grid gap-1">
            <span className="text-t-xs text-fg-tertiary">Theme</span>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Theme">
              {KITCHEN_THEMES.map((entry) => (
                <Button
                  key={entry.id}
                  size="sm"
                  variant={theme === entry.id ? "primary" : "secondary"}
                  aria-pressed={theme === entry.id}
                  onClick={() => setTheme(entry.id)}
                >
                  {entry.label}
                </Button>
              ))}
            </div>
          </div>

          <div className="grid gap-1">
            <span className="text-t-xs text-fg-tertiary">Density</span>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Density">
              {KITCHEN_DENSITIES.map((entry) => (
                <Button
                  key={entry.id}
                  size="sm"
                  variant={density === entry.id ? "primary" : "secondary"}
                  aria-pressed={density === entry.id}
                  onClick={() => setDensity(entry.id)}
                >
                  {entry.label}
                </Button>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-2 text-t-xs text-fg-tertiary">
            <span>Live document</span>
            <span className="num">
              data-theme={theme} data-density={density}
            </span>
          </div>
        </div>
      </header>

      <KitchenSection
        id="kitchen-settings"
        title="Settings"
        intro="One row per Settings v2 category: label, plain-language effect, control, autosave marker. There is no global Save."
      >
        <KitchenSettingsSample />
      </KitchenSection>

      <KitchenSection
        id="kitchen-search"
        title="Search results"
        intro="Six results with measured values right-aligned; the selected row carries the accent wash and a leading bar."
      >
        <KitchenSearchSample />
      </KitchenSection>

      <KitchenSection
        id="kitchen-session"
        title="Session card"
        intro="A session header, its state as a chip, then the key-frame metrics."
      >
        <KitchenSessionSample />
      </KitchenSection>

      <KitchenSection
        id="kitchen-ask"
        title="Ask answer"
        intro="Prose at 15px with two inline citations, then the sources as rows with frame numbers."
      >
        <KitchenAskSample />
      </KitchenSection>

      <KitchenSection
        id="kitchen-primitives"
        title="Primitives"
        intro="Every primitive in the barrel: default, hover, focus, disabled, checked or selected or open, invalid and loading as each one supports them."
      >
        <KitchenGallery />
      </KitchenSection>
    </div>
  );
}
