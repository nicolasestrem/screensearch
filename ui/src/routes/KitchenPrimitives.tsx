// Dev-only primitive gallery for /kitchen (UI_REFERENCE §5). Every primitive in
// the barrel appears here with the states it can actually take: the static
// `preview` prop covers hover / active / focus, the real props cover disabled,
// invalid, checked and open, and the data-bearing primitives (Combobox, Slider,
// Table, ListRow) show all five data states. The route is gated on
// `import.meta.env.DEV` in router.tsx, so none of this ships.
//
// Every class here is semantic: colours come from --surface / --fg / --status /
// --accent tokens through Tailwind, sizes from the type and spacing scales. No
// raw hex, no px font size, no arbitrary px value (that is what tokens.test.mjs
// fences inside the primitives; this file keeps the same discipline).
import { useState, type ReactNode } from "react";

import {
  Badge,
  Button,
  Checkbox,
  Chip,
  Combobox,
  Dialog,
  Divider,
  EmptyState,
  ErrorState,
  Expander,
  Field,
  IconButton,
  Kbd,
  ListRow,
  Panel,
  Popover,
  RadioGroup,
  Segmented,
  Select,
  Sheet,
  Skeleton,
  Slider,
  Spinner,
  Table,
  Tabs,
  Textarea,
  TextInput,
  Toast,
  Toggle,
  Tooltip,
  type TableColumn,
} from "../components/primitives";
import {
  IconAlert,
  IconCapture,
  IconCheck,
  IconClose,
  IconDatabase,
  IconDeck,
  IconMore,
  IconPlus,
  IconRecall,
  IconRefresh,
  IconSearch,
  IconSettings,
  IconSparkle,
  IconTag,
} from "../components/primitives/icons";
import type { ToastItem } from "../state/toastStore";

const NOOP = () => {};

/** A panel-like surface without a border or radius: state samples sit on the
 *  panel surface rather than inside a second card, so a nested control is the
 *  only bordered element in the block. */
function KitchenSurface({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 bg-surface-raised px-pane py-3">
      <div className="mb-2 flex items-center justify-between gap-3">
        <p className="m-0 text-t-ui font-medium text-fg-primary">{title}</p>
        {action}
      </div>
      {children}
    </section>
  );
}

/** Component name, sentence case, above each specimen group. */
function KitchenSpecimen({
  name,
  note,
  children,
}: {
  name: string;
  note?: string;
  children: ReactNode;
}) {
  return (
    <section className="grid gap-3 border-t border-line-subtle pt-4 first:border-t-0 first:pt-0">
      <div>
        <h3 className="m-0 text-t-xs font-medium text-fg-secondary">{name}</h3>
        {note ? <p className="m-0 text-t-xs text-fg-tertiary">{note}</p> : null}
      </div>
      <div className="flex flex-wrap items-start gap-4">{children}</div>
    </section>
  );
}

/** One labelled state inside a specimen group. */
function KitchenState({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1">
      <span className="text-t-xs text-fg-tertiary">{label}</span>
      {children}
    </div>
  );
}

function KitchenExpanderDemo() {
  const [open, setOpen] = useState(true);
  return (
    <div className="grid w-full max-w-xl gap-4">
      <Expander
        title="Capture tuning"
        intro="Only change these when capture skips frames."
        open={open}
        onToggle={() => setOpen(!open)}
      >
        <p className="m-0 text-t-ui text-fg-secondary">
          Poll every 250 ms while an interactive app is in front.
        </p>
      </Expander>
      <Expander
        title="Text extraction"
        intro="The OCR pass that makes frames searchable."
        open={false}
        onToggle={NOOP}
        preview="hover"
      >
        <p className="m-0 text-t-ui text-fg-secondary">Never shown: the group is closed.</p>
      </Expander>
      <Expander
        title="Windows input"
        intro="The UIA hint read from interactive controls."
        open={false}
        onToggle={NOOP}
        preview="focus"
      >
        <p className="m-0 text-t-ui text-fg-secondary">Never shown: the group is closed.</p>
      </Expander>
    </div>
  );
}

function KitchenDialogDemo() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open dialog</Button>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Delete 1,208 frames?"
        description="Marks and saved reports stay. Frames before 12 October go first."
        actions={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button variant="danger" onClick={() => setOpen(false)}>
              Delete frames
            </Button>
          </>
        }
      >
        <p className="m-0 text-t-ui text-fg-secondary">
          Deleting is permanent; the next capture starts a fresh range.
        </p>
      </Dialog>
    </>
  );
}

function KitchenSheetDemo() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open sheet</Button>
      <Sheet
        open={open}
        onClose={() => setOpen(false)}
        title="Filters"
        description="Narrow this search by app, range and content type."
        side="right"
        actions={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>
              Reset
            </Button>
            <Button variant="primary" onClick={() => setOpen(false)}>
              Apply
            </Button>
          </>
        }
      >
        <div className="grid gap-3">
          <Field label="App" placeholder="Any app" />
          <Field label="Range" hint="Relative to now." defaultValue="Last 7 days" />
        </div>
      </Sheet>
    </>
  );
}

interface KitchenFrameRow {
  id: string;
  frame: string;
  app: string;
  text: string;
  match: number;
}

const KITCHEN_FRAMES: KitchenFrameRow[] = [
  { id: "f-1", frame: "041238", app: "VS Code", text: "token fence test", match: 94 },
  { id: "f-2", frame: "041724", app: "Firefox", text: "WCAG contrast ratio", match: 88 },
  { id: "f-3", frame: "042105", app: "Terminal", text: "npm run test", match: 81 },
  { id: "f-4", frame: "042548", app: "Explorer", text: "tokens.css", match: 74 },
];

const KITCHEN_FRAME_COLUMNS: Array<TableColumn<KitchenFrameRow>> = [
  { id: "frame", header: "Frame", numeric: true },
  { id: "app", header: "App" },
  { id: "text", header: "On-screen text" },
  { id: "match", header: "Match", numeric: true, cell: (row) => `${row.match}%` },
];

const KITCHEN_TOAST: ToastItem = { id: 1, level: "success", message: "Settings saved" };

function KitchenTableStates() {
  return (
    <div className="grid w-full gap-6">
      <KitchenState label="populated, row selected">
        <Table
          columns={KITCHEN_FRAME_COLUMNS}
          rows={KITCHEN_FRAMES}
          getRowId={(row) => row.id}
          caption="Frames matching the token fence"
          selectedRowId="f-2"
        />
      </KitchenState>
      <KitchenState label="hover">
        <Table
          columns={KITCHEN_FRAME_COLUMNS}
          rows={KITCHEN_FRAMES.slice(0, 2)}
          getRowId={(row) => row.id}
          preview="hover"
        />
      </KitchenState>
      <KitchenState label="partial">
        <Table
          columns={KITCHEN_FRAME_COLUMNS}
          rows={KITCHEN_FRAMES}
          getRowId={(row) => row.id}
          state="partial"
          total={148}
        />
      </KitchenState>
      <KitchenState label="loading">
        <Table columns={KITCHEN_FRAME_COLUMNS} rows={[]} state="loading" loadingRows={3} />
      </KitchenState>
      <KitchenState label="empty">
        <Table
          columns={KITCHEN_FRAME_COLUMNS}
          rows={[]}
          state="empty"
          emptyTitle="No frames in this range"
          emptyDescription="Widen the range or start a capture."
          emptyAction={<Button variant="primary">Start capture</Button>}
        />
      </KitchenState>
      <KitchenState label="error">
        <Table
          columns={KITCHEN_FRAME_COLUMNS}
          rows={[]}
          state="error"
          errorTitle="Couldn't reach the kernel"
          errorMessage="The capture service is not answering. Restart ScreenSearch."
          onRetry={NOOP}
        />
      </KitchenState>
    </div>
  );
}

export function KitchenGallery() {
  const [combined, setCombined] = useState(false);
  const [interval, setInterval] = useState(8);

  return (
    <div className="grid gap-6">
      <KitchenSpecimen name="Button" note="primary, secondary, ghost, danger, hover, focus, disabled, loading">
        <KitchenState label="default">
          <Button>Run capture</Button>
        </KitchenState>
        <KitchenState label="primary">
          <Button variant="primary">Start capture</Button>
        </KitchenState>
        <KitchenState label="danger">
          <Button variant="danger">Delete history</Button>
        </KitchenState>
        <KitchenState label="ghost">
          <Button variant="ghost">Skip</Button>
        </KitchenState>
        <KitchenState label="hover">
          <Button preview="hover">Hover</Button>
        </KitchenState>
        <KitchenState label="active">
          <Button preview="active">Pressed</Button>
        </KitchenState>
        <KitchenState label="focus">
          <Button preview="focus">Focused</Button>
        </KitchenState>
        <KitchenState label="loading">
          <Button loading>Indexing</Button>
        </KitchenState>
        <KitchenState label="disabled">
          <Button disabled>Unavailable</Button>
        </KitchenState>
        <KitchenState label="small">
          <Button size="sm">Compact</Button>
        </KitchenState>
      </KitchenSpecimen>

      <KitchenSpecimen name="Icon button" note="toggle pressed, hover, focus, disabled">
        <KitchenState label="default">
          <IconButton label="Search">
            <IconSearch />
          </IconButton>
        </KitchenState>
        <KitchenState label="pressed">
          <IconButton label="Pin this moment" active>
            <IconTag />
          </IconButton>
        </KitchenState>
        <KitchenState label="hover">
          <IconButton label="Refresh" preview="hover">
            <IconRefresh />
          </IconButton>
        </KitchenState>
        <KitchenState label="focus">
          <IconButton label="More" preview="focus">
            <IconMore />
          </IconButton>
        </KitchenState>
        <KitchenState label="disabled">
          <IconButton label="Add a capture" disabled>
            <IconPlus />
          </IconButton>
        </KitchenState>
      </KitchenSpecimen>

      <KitchenSpecimen name="Panel" note="header, action slot, flush body">
        <div className="grid w-full max-w-xl gap-4">
          <Panel title="Capture" action={<Chip tone="ok" dot>Running</Chip>}>
            <p className="m-0 text-t-ui text-fg-secondary">
              Eight frames a minute from the active display.
            </p>
          </Panel>
          <Panel title="Recent frames" flush>
            <div className="grid">
              {KITCHEN_FRAMES.slice(0, 2).map((row) => (
                <ListRow
                  key={row.id}
                  icon={<IconCapture size={16} />}
                  title={row.app}
                  meta={row.text}
                  trailing={<span className="num">{row.frame}</span>}
                  className="rounded-none border-b border-line-subtle last:border-b-0"
                />
              ))}
            </div>
          </Panel>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Expander" note="open, closed, hover, focus">
        <KitchenExpanderDemo />
      </KitchenSpecimen>

      <KitchenSpecimen name="Chip" note="five tones, status dot, removable, preview states">
        <KitchenState label="tones">
          <div className="flex flex-wrap gap-2">
            <Chip>Neutral</Chip>
            <Chip tone="accent">Accent</Chip>
            <Chip tone="ok" dot>Saved</Chip>
            <Chip tone="warn" dot>Paused</Chip>
            <Chip tone="danger" dot>Failed</Chip>
          </div>
        </KitchenState>
        <KitchenState label="removable">
          <Chip onRemove={NOOP} removeLabel="Remove excluded app">
            slack.exe
          </Chip>
        </KitchenState>
        <KitchenState label="hover">
          <Chip onRemove={NOOP} removeLabel="Remove hover specimen" preview="hover">
            Hover
          </Chip>
        </KitchenState>
        <KitchenState label="focus">
          <Chip onRemove={NOOP} removeLabel="Remove focus specimen" preview="focus">
            Focus
          </Chip>
        </KitchenState>
      </KitchenSpecimen>

      <KitchenSpecimen name="Field" note="hint, error, required, disabled, small">
        <div className="grid w-full max-w-sm gap-4">
          <Field label="Profile name" hint="Shown in exports and the tray tooltip." defaultValue="Workstation" />
          <Field label="Retention days" hint="Applies from the next sweep." defaultValue="30" />
          <Field
            label="Excluded app rule"
            error="The pattern must end with .exe"
            defaultValue="slack"
          />
          <Field label="Device label" required defaultValue="DESKTOP-7Q2" />
          <Field label="Model path" hint="Set by the installer." defaultValue="C:/models" disabled />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Text input" note="hover, focus, invalid, disabled, small">
        <div className="grid w-full max-w-xs gap-4">
          <KitchenState label="default">
            <TextInput aria-label="Query" placeholder="Search frames" />
          </KitchenState>
          <KitchenState label="hover">
            <TextInput aria-label="Query" preview="hover" defaultValue="token fence" />
          </KitchenState>
          <KitchenState label="focus">
            <TextInput aria-label="Query" preview="focus" defaultValue="token fence" />
          </KitchenState>
          <KitchenState label="invalid">
            <TextInput aria-label="Port" invalid defaultValue="port" />
          </KitchenState>
          <KitchenState label="disabled">
            <TextInput aria-label="Token" defaultValue="••••••••" disabled />
          </KitchenState>
          <KitchenState label="small">
            <TextInput aria-label="Tag" size="sm" placeholder="Add a tag" />
          </KitchenState>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Textarea" note="hover, focus, invalid, disabled">
        <div className="grid w-full max-w-sm gap-4">
          <KitchenState label="default">
            <Textarea aria-label="Report prompt" rows={2} placeholder="Summarise the last week" />
          </KitchenState>
          <KitchenState label="focus">
            <Textarea aria-label="Report prompt" rows={2} preview="focus" defaultValue="Summarise the last week" />
          </KitchenState>
          <KitchenState label="invalid">
            <Textarea aria-label="Report prompt" rows={2} invalid defaultValue="Summarise" />
          </KitchenState>
          <KitchenState label="disabled">
            <Textarea aria-label="Report prompt" rows={2} disabled value="Read only" readOnly />
          </KitchenState>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Select" note="hint, error, disabled, hover, focus">
        <div className="grid w-full max-w-xs gap-4">
          <Select
            label="Vision tier"
            options={[
              { value: "fast", label: "Fast" },
              { value: "balanced", label: "Balanced" },
              { value: "quality", label: "Quality" },
            ]}
            defaultValue="balanced"
            hint="Higher tiers tag more detail per frame."
          />
          <Select
            label="Answer tier"
            options={[{ value: "small", label: "Small" }]}
            error="That tier is not downloaded yet."
          />
          <Select
            label="Theme"
            options={[
              { value: "pine-light", label: "Pine, light" },
              { value: "pine-dark", label: "Pine, dark" },
            ]}
            defaultValue="pine-light"
            preview="focus"
          />
          <Select label="Language" options={[{ value: "en", label: "English" }]} defaultValue="en" disabled />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Combobox" note="all five data states, disabled, focus">
        <div className="grid w-full max-w-xs gap-4">
          <KitchenState label="populated">
            <Combobox
              label="App"
              options={[
                { value: "code", label: "VS Code" },
                { value: "firefox", label: "Firefox" },
                { value: "terminal", label: "Terminal" },
              ]}
              value="code"
              onChange={NOOP}
            />
          </KitchenState>
          <KitchenState label="partial">
            <Combobox
              label="App"
              options={[{ value: "code", label: "VS Code" }]}
              value="code"
              onChange={NOOP}
              state="partial"
              hint="Reading two of three app lists."
            />
          </KitchenState>
          <KitchenState label="loading">
            <Combobox label="App" options={[]} value="" onChange={NOOP} state="loading" />
          </KitchenState>
          <KitchenState label="empty">
            <Combobox label="App" options={[]} value="" onChange={NOOP} state="empty" />
          </KitchenState>
          <KitchenState label="error">
            <Combobox
              label="App"
              options={[]}
              value=""
              onChange={NOOP}
              state="error"
              error="The capture service is not answering."
            />
          </KitchenState>
          <KitchenState label="focus">
            <Combobox
              label="App"
              options={[{ value: "code", label: "VS Code" }]}
              value="code"
              onChange={NOOP}
              preview="focus"
            />
          </KitchenState>
          <KitchenState label="disabled">
            <Combobox
              label="App"
              options={[{ value: "code", label: "VS Code" }]}
              value="code"
              onChange={NOOP}
              disabled
            />
          </KitchenState>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Toggle" note="off, on, hover, focus, disabled">
        <div className="grid w-full max-w-sm gap-4">
          <Toggle label="Pause when the screen locks" checked={false} onChange={NOOP} />
          <Toggle
            label="Open at login"
            checked
            onChange={NOOP}
            hint="Closing the window keeps ScreenSearch running."
          />
          <Toggle label="Event-driven capture" checked preview="hover" onChange={NOOP} />
          <Toggle label="Send diagnostics" checked={false} preview="focus" onChange={NOOP} />
          <Toggle label="HTTP API" checked={false} onChange={NOOP} disabled />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Checkbox" note="checked, unchecked, mixed, error, hover, focus, disabled">
        <div className="grid w-full max-w-sm gap-4">
          <Checkbox label="Include app chrome" checked onChange={NOOP} />
          <Checkbox label="Include raw text" checked={false} onChange={NOOP} hint="Slower, more literal results." />
          <Checkbox label="Some monitors" indeterminate checked={false} onChange={NOOP} />
          <Checkbox
            label="Keep audio"
            checked={false}
            onChange={NOOP}
            error="Audio capture is not available on this build."
          />
          <Checkbox label="Tag frames on capture" checked preview="hover" onChange={NOOP} />
          <Checkbox label="Index on idle" checked={false} preview="focus" onChange={NOOP} />
          <Checkbox label="Sync to a folder" checked={false} onChange={NOOP} disabled />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Radio group" note="vertical, horizontal, error, hover, focus, disabled">
        <div className="grid w-full max-w-md gap-4">
          <RadioGroup
            label="Export format"
            options={[
              { value: "md", label: "Markdown", hint: "Plain text with frame links." },
              { value: "json", label: "JSON" },
            ]}
            value="md"
            onChange={NOOP}
          />
          <RadioGroup
            label="Capture scope"
            orientation="horizontal"
            options={[
              { value: "content", label: "Content" },
              { value: "raw", label: "Raw text" },
              { value: "all", label: "Everything", disabled: true },
            ]}
            value="content"
            onChange={NOOP}
          />
          <RadioGroup
            label="Retention"
            options={[{ value: "30", label: "30 days" }]}
            value="30"
            onChange={NOOP}
            error="Pick a retention window first."
          />
          <RadioGroup
            label="Encryption"
            options={[
              { value: "on", label: "On" },
              { value: "off", label: "Off" },
            ]}
            value="on"
            onChange={NOOP}
            preview="focus"
          />
          <RadioGroup
            label="Sync"
            options={[{ value: "off", label: "Off" }]}
            value="off"
            onChange={NOOP}
            disabled
          />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Slider" note="all five data states, hover, active, focus, disabled">
        <div className="grid w-full max-w-sm gap-5">
          <Slider
            label="Capture interval"
            value={interval}
            onChange={setInterval}
            min={2}
            max={60}
            unit="s"
            hint="How often a frame is kept while the screen changes."
          />
          <Slider
            label="Match threshold"
            value={42}
            onChange={NOOP}
            unit="%"
            preview="hover"
          />
          <Slider label="Thumbnail width" value={640} onChange={NOOP} min={320} max={1920} unit="px" preview="active" />
          <Slider label="Batch size" value={8} onChange={NOOP} preview="focus" />
          <Slider
            label="Capture interval"
            value={8}
            onChange={NOOP}
            state="loading"
            unit="s"
          />
          <Slider label="Match threshold" value={0} onChange={NOOP} state="empty" unit="%" />
          <Slider label="Match threshold" value={0} onChange={NOOP} state="error" unit="%" />
          <Slider label="Match threshold" value={42} onChange={NOOP} state="partial" unit="%" hint="Two of three indexes answered." />
          <Slider label="Capture interval" value={8} onChange={NOOP} unit="s" disabled />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Segmented" note="selected, hover, focus, disabled option">
        <div className="grid w-full max-w-xl gap-4">
          <Segmented
            label="Monitors"
            options={[
              { value: "all", label: "All displays" },
              { value: "active", label: "Active window" },
              { value: "one", label: "Display 1" },
            ]}
            defaultValue="all"
          />
          <Segmented
            label="Recall text"
            options={[
              { value: "content", label: "Content" },
              { value: "raw", label: "Raw" },
              { value: "off", label: "Off", disabled: true },
            ]}
            defaultValue="content"
            preview="hover"
          />
          <Segmented
            label="Density"
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "compact", label: "Compact" },
            ]}
            defaultValue="comfortable"
            preview="focus"
          />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Tabs" note="selected, hover, focus, disabled tab">
        <div className="w-full max-w-xl">
          <Tabs
            label="Settings sections"
            items={[
              {
                id: "general",
                label: "General",
                content: <p className="m-0 text-t-ui text-fg-secondary">Profile, startup and theme.</p>,
              },
              {
                id: "capture",
                label: "Capture",
                content: <p className="m-0 text-t-ui text-fg-secondary">Interval, monitors and hotkeys.</p>,
              },
              { id: "beta", label: "Beta", disabled: true },
            ]}
            defaultValue="general"
          />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Skeleton" note="row, control, circle and text placeholders">
        <div className="grid w-full max-w-xl gap-3">
          <div className="flex items-center gap-3">
            <Skeleton circle h="8" w="8" />
            <div className="grid flex-1 gap-2">
              <Skeleton h="3" w="half" />
              <Skeleton h="2" w="third" />
            </div>
            <Skeleton h="3" w="12" />
          </div>
          <Skeleton h="row" w="full" />
          <div className="flex items-center gap-2">
            <Skeleton h="control" w="16" />
            <Skeleton h="control" w="24" />
            <Skeleton h="controlSm" w="12" />
          </div>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Spinner" note="inline and block sizes with live status">
        <KitchenState label="small">
          <Spinner size="sm" label="Loading frames" />
        </KitchenState>
        <KitchenState label="medium">
          <Spinner label="Preparing the answer model" />
        </KitchenState>
      </KitchenSpecimen>

      <KitchenSpecimen name="Empty state" note="centred page and start-aligned card">
        <div className="grid w-full gap-4">
          <EmptyState
            icon={<IconSearch size={20} />}
            title="No matches"
            description="Try different words, widen the range, or include app chrome."
            action={<Button variant="primary">Widen the range</Button>}
          />
          <KitchenSurface title="Excluded apps" action={<Chip tone="ok">Saved</Chip>}>
            <EmptyState
              align="start"
              icon={<IconSettings size={18} />}
              title="Nothing excluded yet"
              description="Apps you exclude are never captured."
              action={<Button size="sm">Add an app</Button>}
            />
          </KitchenSurface>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Error state" note="retry action, replacement action, hover, focus">
        <div className="grid w-full gap-4">
          <ErrorState
            title="Couldn't reach the kernel"
            message="The capture service is not answering. Restart ScreenSearch, then try again."
            onRetry={NOOP}
          />
          <KitchenSurface title="Recall">
            <ErrorState
              align="start"
              title="The answer model is not loaded"
              message="Load it to ask questions about this machine."
              action={<Button size="sm">Load answer model</Button>}
            />
          </KitchenSurface>
          <ErrorState
            align="start"
            title="Hover preview"
            message="This specimen renders the retry button's hover state."
            onRetry={NOOP}
            preview="hover"
          />
          <ErrorState
            align="start"
            title="Focus preview"
            message="This specimen renders the retry button's focus ring."
            onRetry={NOOP}
            preview="focus"
          />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Table" note="populated, selected row, hover, partial, loading, empty, error">
        <KitchenTableStates />
      </KitchenSpecimen>

      <KitchenSpecimen name="List row" note="selected, hover, focus and all five data states">
        <div className="grid w-full max-w-xl gap-3">
          <div className="grid">
            <ListRow
              as="button"
              icon={<IconRecall size={16} />}
              title="Proposal draft in Notion"
              meta="Firefox · 14:32"
              trailing={<span className="num">92%</span>}
              selected
            />
            <ListRow
              as="button"
              icon={<IconDeck size={16} />}
              title="Build log"
              meta="Terminal · 14:31"
              trailing={<span className="num">88%</span>}
            />
            <ListRow
              as="button"
              icon={<IconDatabase size={16} />}
              title="Token fence failure"
              meta="VS Code · 14:28"
              trailing={<span className="num">81%</span>}
              preview="hover"
            />
            <ListRow
              as="button"
              icon={<IconSearch size={16} />}
              title="Keyboard focus"
              meta="VS Code · 14:27"
              trailing={<span className="num">74%</span>}
              preview="focus"
            />
            <ListRow state="partial" title="Two of three indexes" meta="Firefox · 14:25" />
            <ListRow state="loading" icon={<IconSparkle size={16} />} trailing={<span />} />
            <ListRow state="empty" />
            <ListRow state="error" title="Couldn't load this row" meta="Retry from the Recall panel" />
          </div>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Tooltip" note="hover and focus show the label; disabled suppresses it">
        <KitchenState label="default">
          <Tooltip label="Refresh the frame list">
            <IconButton label="Refresh">
              <IconRefresh />
            </IconButton>
          </Tooltip>
        </KitchenState>
        <KitchenState label="hover">
          <Tooltip label="Runs on the next idle slot" preview="hover">
            <Button size="sm">Schedule</Button>
          </Tooltip>
        </KitchenState>
        <KitchenState label="focus">
          <Tooltip label="Only marks made today" preview="focus">
            <Button size="sm">Today</Button>
          </Tooltip>
        </KitchenState>
        <KitchenState label="disabled">
          <Tooltip label="Never shown" disabled preview="hover">
            <Button size="sm" disabled>
              Muted
            </Button>
          </Tooltip>
        </KitchenState>
      </KitchenSpecimen>

      <KitchenSpecimen name="Popover" note="closed, open, hover, focus">
        <div className="flex flex-wrap items-start gap-16">
          <Popover label="Search filters" trigger="Filters">
            <div className="grid gap-2">
              <Field label="App" placeholder="Any app" />
              <Button size="sm" variant="primary">
                Apply
              </Button>
            </div>
          </Popover>
          <Popover label="Open specimen" trigger="Open by default" defaultOpen align="center">
            <p className="m-0 text-t-ui text-fg-secondary">An anchored dialog surface.</p>
          </Popover>
          <Popover label="Hover specimen" trigger="Hover" preview="hover">
            <p className="m-0 text-t-ui text-fg-secondary">Trigger hover.</p>
          </Popover>
          <Popover label="Focus specimen" trigger="Focus" preview="focus">
            <p className="m-0 text-t-ui text-fg-secondary">Trigger focus ring.</p>
          </Popover>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Dialog" note="portalled modal: press the trigger for the open state">
        <KitchenDialogDemo />
      </KitchenSpecimen>

      <KitchenSpecimen name="Sheet" note="portalled side panel: press the trigger for the open state">
        <KitchenSheetDemo />
      </KitchenSpecimen>

      <KitchenSpecimen name="Badge" note="five tones, each with its status glyph">
        <div className="flex flex-wrap items-center gap-2">
          <Badge tone="neutral">Unsaved</Badge>
          <Badge tone="accent">Changed</Badge>
          <Badge tone="ok">Saved</Badge>
          <Badge tone="warn">Paused</Badge>
          <Badge tone="danger">Failed</Badge>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Kbd" note="single key and chord">
        <div className="flex flex-wrap items-center gap-2">
          <Kbd>Ctrl</Kbd>
          <Kbd>Shift</Kbd>
          <Kbd>K</Kbd>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Divider" note="horizontal and vertical">
        <div className="grid w-full gap-4">
          <Divider />
          <div className="flex h-8 items-center gap-3 text-t-ui text-fg-secondary">
            <span>Search</span>
            <Divider vertical />
            <span>Timeline</span>
            <Divider vertical />
            <span>Reports</span>
          </div>
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Toast" note="four levels, an action, and a loading toast">
        <div className="grid w-full max-w-md gap-3">
          <Toast toast={{ ...KITCHEN_TOAST, level: "info", message: "Capture resumed" }} onClose={NOOP} duration={0} />
          <Toast
            toast={KITCHEN_TOAST}
            onClose={NOOP}
            duration={0}
            action={{ label: "Undo", onAction: NOOP }}
          />
          <Toast
            toast={{ ...KITCHEN_TOAST, level: "warning", message: "A monitor was unplugged" }}
            onClose={NOOP}
            duration={0}
          />
          <Toast
            toast={{ ...KITCHEN_TOAST, level: "error", message: "Couldn't save the interval" }}
            onClose={NOOP}
            duration={0}
          />
          <Toast
            toast={{ ...KITCHEN_TOAST, level: "info", message: "Indexing 148 frames" }}
            onClose={NOOP}
            duration={0}
            loading
          />
          <Toast toast={{ ...KITCHEN_TOAST, message: "Hovered" }} onClose={NOOP} duration={0} preview="hover" />
        </div>
      </KitchenSpecimen>

      <KitchenSpecimen name="Combined row" note="a settings row built from Field, Toggle and Checkbox">
        <KitchenSurface title="Autosaved row" action={<Badge tone="ok">Saved</Badge>}>
          <div className="grid gap-3">
            <Field label="Capture interval" hint="Seconds between kept frames." defaultValue="8" />
            <Toggle
              label="Event-driven capture"
              checked={combined}
              onChange={setCombined}
              hint="Capture on window changes as well as on the timer."
            />
            <Checkbox label="Capture only the prompt" checked={combined} onChange={NOOP} />
            <div className="flex items-center gap-2">
              <IconCheck size={14} className="text-status-ok-fg" />
              <span className="text-t-xs text-fg-secondary">Remembered on the next capture start.</span>
              <IconAlert size={14} className="ml-auto text-status-warn-fg" />
            </div>
            <Divider />
            <div className="flex items-center gap-2 text-t-xs text-fg-tertiary">
              <IconClose size={12} />
              <span>Esc closes, Enter saves</span>
              <Kbd>Esc</Kbd>
            </div>
          </div>
        </KitchenSurface>
      </KitchenSpecimen>
    </div>
  );
}
