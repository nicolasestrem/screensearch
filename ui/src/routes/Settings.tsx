// Settings (/settings) — the two-tier IA (0.3.2 PR5, D6; UI_REFERENCE §3/§4).
// **Essentials** (always visible, in the D6 order): Capture · Hotkeys · Privacy ·
// Models · Storage · App · Data. **Advanced** (collapsed by default, one `Expander`
// per group, per-session state in the ui store): Enrichment & scheduling · Sessions ·
// Performance throttle · Text filtering · Reports & retrieval. Every section opens with
// one plain-language sentence (§9).
//
// Settings v2 (0.4.5 D6, `03 §8`): this page edits **only** the 25 `UI` keys. The
// `file-only` keys carry no control here — not a disabled one, not a placeholder row —
// because they live in `advanced.toml` and are surfaced read-only by the
// `get_advanced_config` projection (P7 rebuilds this page around categories). The
// removed groups are therefore gone rather than greyed out.
//
// Presentation-first (D7): zero key renames, zero semantic changes — the form still
// edits one draft of the typed `Settings` binding; Save persists the whole draft
// (optimistic + reconcile via useSetSettings). Model tiers additionally hot-apply the
// moment they change (useSetModelTier). Every field labels *when* it takes effect. The
// Hotkeys section carries the gap-#100 inline cross-chord conflict warning (fires live
// while the overlay and marks chords match; clears when they differ) — the loud
// registration-failure warning stays the save-time safety net.
//
// States (§4): loading → skeleton; error → load failed + retry; partial → models
// still downloading (noted in the Models panel); populated → the form. Settings has
// no empty state. A failed Save keeps the form and explains via a toast.
import { useEffect, useState, type ChangeEvent } from "react";

import {
  Button,
  Chip,
  Expander,
  Field,
  Panel,
  Skeleton,
  Toggle,
  ErrorState,
} from "../components/primitives";
import {
  ApiPanel,
  AppPanel,
  DEFAULT_OVERLAY_HOTKEY,
  DEFAULT_MARKS_HOTKEY,
  HotkeyField,
  ModelPanel,
  ModelTierPicker,
  RetentionControl,
  ScheduleControl,
  SetupDetails,
} from "../components/domain";
import {
  useMonitors,
  useReadiness,
  useSettings,
  useHotkeyStatus,
  useTextFilterStats,
  useThrottleStatus,
} from "../lib/ipc/queries";
import {
  useExportData,
  useSetModelTier,
  useSetSettings,
} from "../lib/ipc/mutations";
import { toast } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";
import type { Settings } from "../bindings/Settings";
import type { ModelLane } from "../bindings/ModelLane";
import type { ModelTier } from "../bindings/ModelTier";
import { MemoryPanel } from "../components/domain/MemoryPanel";

const TIER_LABEL: Record<ModelTier, string> = {
  default: "Default",
  quality: "Quality",
};

// When each setting takes effect (the backend applies them at different points).
const APPLY_NOW = "Applies now.";
const APPLY_ASK = "Applies to your next question.";
const APPLY_CAPTURE = "Applies on the next capture start.";
const APPLY_SESSION = "Changes apply on the next session pass.";

/** Parse a comma-separated list of non-negative monitor indices; empty → []. */
function parseIntList(raw: string): number[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .map(Number)
    .filter((n) => Number.isFinite(n) && n >= 0)
    .map((n) => Math.round(n));
}

/** Parse a comma-separated list of names, trimmed; empty entries dropped. */
function parseStrList(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/**
 * Whether two hotkey chords are the same combination (gap `07` #100, the Settings-side
 * cross-chord conflict check — UI-only, `03 §7d`). `HotkeyField` records canonical chords
 * (fixed modifier order + uppercase key), but a persisted value can be hand-edited, so the
 * comparison is defensive: case-insensitive, modifier-order-insensitive, key compared last.
 */
function chordsConflict(a: string, b: string): boolean {
  const norm = (chord: string): string | null => {
    const parts = chord
      .split("+")
      .map((p) => p.trim().toLowerCase())
      .filter((p) => p.length > 0);
    if (parts.length === 0) return null;
    const key = parts[parts.length - 1];
    const mods = parts.slice(0, -1).sort();
    return [...mods, key].join("+");
  };
  const na = norm(a);
  const nb = norm(b);
  return na !== null && na === nb;
}

/**
 * Clamp/round the numeric settings into valid ranges before they're persisted. Field
 * `min`/`max` are advisory — a user can type or paste out of range — and the integer
 * Rust fields reject a JSON float, so the form sanitises on save: every integer field
 * is rounded and clamped. Every field here is a `UI` key (`03 §8`, 0.4.5 D6); the
 * `file-only` knobs are clamped backend-side by `AdvancedConfig::sanitized` and have
 * no control or clamp on this page. Returns a new object; never mutates the input.
 */
function sanitizeSettings(s: Settings): Settings {
  const clampInt = (v: number, lo: number, hi: number) =>
    Math.min(hi, Math.max(lo, Math.round(Number.isFinite(v) ? v : lo)));
  return {
    ...s,
    capture_interval_ms: clampInt(s.capture_interval_ms, 250, 3_600_000),
    // 0 = native (no downscale) — mirrors the backend sentinel; any other value clamps.
    storage_max_width:
      s.storage_max_width === 0 ? 0 : clampInt(s.storage_max_width, 320, 7680),
    storage_retention_days: clampInt(s.storage_retention_days, 0, 3650),
    enrich_vision_timer_interval_ms: clampInt(
      s.enrich_vision_timer_interval_ms,
      60_000,
      86_400_000,
    ),
    // Retrieval + recall reports — mirror the backend clamps (03 §8).
    retrieval_default_top_k: clampInt(s.retrieval_default_top_k, 1, 100),
    reports_daily_top_k: clampInt(s.reports_daily_top_k, 1, 1_000),
    overlay_hotkey: s.overlay_hotkey.trim() || DEFAULT_OVERLAY_HOTKEY,
    // An empty mark chord falls back to the default (mirrors kernel settings.rs).
    marks_hotkey: s.marks_hotkey.trim() || DEFAULT_MARKS_HOTKEY,
    // 0.4.0 PR4 sessions keys — mirror the backend final clamps.
    sessions_min_len_secs: clampInt(s.sessions_min_len_secs, 30, 3_600),
    sessions_gap_close_secs: clampInt(s.sessions_gap_close_secs, 60, 3_600),
  };
}

// Reserves roughly the settings form's above-the-fold height (header + the first
// Essentials panels) so the loading -> populated transition doesn't jump the visible
// region (D9 no-CLS). Advanced groups collapse to slim rows, approximated by the
// short skeletons at the end.
function SettingsSkeleton() {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <Skeleton className="h-12 w-full" />
      <Skeleton className="h-56 w-full" />
      <Skeleton className="h-64 w-full" />
      <Skeleton className="h-40 w-full" />
      <Skeleton className="h-48 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  );
}

/**
 * Per-app suppression-rate readout (PR3 guardrail, `03 §3b`): makes silent
 * over-suppression observable. All view states — loading, error, empty, populated.
 */
function SuppressionReadout() {
  const stats = useTextFilterStats();
  if (stats.isLoading) return <Skeleton className="h-16 w-full" />;
  if (stats.isError) {
    return (
      <p className="text-caption text-ink-muted">
        Couldn't load suppression rates: {String(stats.error)}
      </p>
    );
  }
  const rows = stats.data ?? [];
  if (rows.length === 0) {
    return (
      <p className="text-caption text-ink-muted">
        No filtered captures yet — start capture to see how much chrome each app
        sheds.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2">
      <p className="text-caption text-ink-muted">
        Share of recognised text dropped as chrome/system/background, per
        foreground app. A very high rate can signal over-suppression — raw text
        and “include app chrome” always recover it.
      </p>
      <ul className="flex flex-col gap-1">
        {rows.slice(0, 12).map((r) => (
          <li
            key={r.app ?? "(unknown)"}
            className="flex items-center justify-between gap-3"
          >
            <span className="text-body text-ink truncate">
              {r.app ?? "(unknown app)"}
            </span>
            <span className="text-caption text-ink-muted tabular-nums">
              {Math.round(r.rate * 100)}% · {r.suppressed_spans}/{r.total_spans}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

const THROTTLE_LEVEL_LABEL = ["Normal", "High", "Sustained"] as const;

/**
 * Live enrichment-throttle status (former PR5, `03 §8`): current CPU/GPU pressure, the
 * throttle level, and a truthful "GPU not monitored" state when Windows exposes no GPU
 * performance counters. All view states — loading, error, off, populated.
 */
function ThrottleReadout() {
  const status = useThrottleStatus();
  if (status.isLoading) return <Skeleton className="h-12 w-full" />;
  if (status.isError) {
    return (
      <p className="text-caption text-ink-muted">
        Couldn't load throttle status: {String(status.error)}
      </p>
    );
  }
  const s = status.data;
  if (!s || !s.enabled) {
    return (
      <p className="text-caption text-ink-muted">
        Throttle is off — enrichment always runs at full speed.
      </p>
    );
  }
  const cpu = s.sample ? `${Math.round(s.sample.cpu_pct)}%` : "—";
  const gpu = !s.gpu_monitored
    ? "GPU not monitored"
    : s.sample?.gpu_pct != null
      ? `${Math.round(s.sample.gpu_pct)}%`
      : "—";
  const levelLabel = THROTTLE_LEVEL_LABEL[s.level] ?? "Normal";
  return (
    <div className="flex flex-col gap-1">
      <p className="text-body text-ink tabular-nums">
        CPU {cpu} · GPU {gpu} · Level: {levelLabel}
      </p>
      {!s.gpu_monitored && (
        <p className="text-caption text-ink-muted">
          Your GPU exposes no Windows performance counters; throttling uses CPU
          pressure only.
        </p>
      )}
      {s.level > 0 && (
        <p className="text-caption text-ink-muted">
          Throttling active — vision tagging and image embeds are paused
          {s.level >= 2 ? " and text-embed concurrency is reduced" : ""};
          capture, OCR, and storage keep running.
        </p>
      )}
    </div>
  );
}

/** One plain-language intro sentence per section (UI_REFERENCE §3/§9). */
function SectionIntro({ children }: { children: string }) {
  return <p className="text-caption text-ink-muted font-body">{children}</p>;
}

export function Component() {
  const settings = useSettings();
  const readiness = useReadiness();
  const monitors = useMonitors();
  const hotkeyStatus = useHotkeyStatus();
  const setSettings = useSetSettings();
  const setTier = useSetModelTier();
  const exportData = useExportData();
  // Advanced-tier expander state (D6): collapsed by default, per-session in the ui store.
  const settingsExpanded = useUiStore((s) => s.settingsExpanded);
  const toggleSettingsGroup = useUiStore((s) => s.toggleSettingsGroup);

  // The form's working copy and the last-saved snapshot it's diffed against. The two
  // free-text list fields keep their own raw buffers so typing (trailing commas etc.)
  // isn't fought by re-serialising the parsed array back into the input.
  const [draft, setDraft] = useState<Settings | null>(null);
  const [baseline, setBaseline] = useState<Settings | null>(null);
  const [monitorsText, setMonitorsText] = useState("");
  const [appsText, setAppsText] = useState("");

  // Keep the saved-snapshot in sync with the backend on every refetch so the `dirty`
  // diff stays accurate (e.g. after a tier hot-apply invalidation, or an optimistic
  // save settling). The editable draft is seeded only once — a later refetch must not
  // clobber in-progress edits.
  useEffect(() => {
    if (!settings.data) return;
    const data = settings.data;
    setBaseline(data);
    if (draft === null) {
      setDraft(data);
      setMonitorsText(data.capture_monitors.join(", "));
      setAppsText(data.privacy_excluded_apps.join(", "));
    } else {
      // Two groups are edited through their own self-contained round-trips, not this
      // form's draft: the App toggles (run-at-startup / close-to-tray, via AppPanel; they
      // have fallible OS side effects and apply immediately) and the HTTP API settings
      // (enabled / port / bind address, via ApiPanel's `set_api_config`, which starts,
      // stops and rebinds the listener). Mirror their latest persisted values into the
      // draft so they never register as unsaved here and a bulk Save of other fields never
      // reverts them. That revert matters most for the API: `set_settings` hot-applies a
      // changed `api_*` value, so a stale draft would stop a listener the user just
      // started. Only these five fields are touched — in-progress edits to the form's own
      // fields are preserved, and an unchanged draft keeps its reference (no re-render
      // loop).
      setDraft((cur) =>
        cur &&
        (cur.app_run_at_startup !== data.app_run_at_startup ||
          cur.app_close_to_tray !== data.app_close_to_tray ||
          cur.api_enabled !== data.api_enabled ||
          cur.api_port !== data.api_port ||
          cur.api_bind_address !== data.api_bind_address)
          ? {
              ...cur,
              app_run_at_startup: data.app_run_at_startup,
              app_close_to_tray: data.app_close_to_tray,
              api_enabled: data.api_enabled,
              api_port: data.api_port,
              api_bind_address: data.api_bind_address,
            }
          : cur,
      );
    }
  }, [settings.data, draft]);

  if (settings.isError && !draft) {
    return (
      <div className="p-6">
        <ErrorState
          title="Couldn't load settings"
          message={String(settings.error)}
          onRetry={() => settings.refetch()}
        />
      </div>
    );
  }

  if (!draft || !baseline) return <SettingsSkeleton />;

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setDraft((d) => (d ? { ...d, [key]: value } : d));

  const patch = (p: Partial<Settings>) =>
    setDraft((d) => (d ? { ...d, ...p } : d));

  // Numeric inputs: a cleared field yields NaN — fall back to 0 (a transient value the
  // user types over; out-of-range values are clamped on save) rather than ignoring the
  // change, which would snap the controlled input back to its old value. `intHandler`
  // rounds for the integer-typed Rust fields (a stray float is rejected by serde).
  const intHandler =
    <K extends keyof Settings>(key: K) =>
    (e: ChangeEvent<HTMLInputElement>) => {
      const v = e.currentTarget.valueAsNumber;
      if (Number.isFinite(v)) set(key, Math.round(v) as Settings[K]);
      else if (e.currentTarget.value === "") set(key, 0 as Settings[K]);
    };

  // Tier changes hot-apply immediately (persisted + applied to the live provider via
  // set_model_tier); the draft updates optimistically and reverts if the call fails.
  const changeTier = (lane: ModelLane, tier: ModelTier) => {
    const key = lane === "vision" ? "models_vision_tier" : "models_answer_tier";
    const prev = draft[key];
    set(key, tier);
    setTier.mutate(
      { lane, tier },
      {
        onSuccess: () => {
          setBaseline((b) => (b ? { ...b, [key]: tier } : b));
          toast.success(
            `${lane === "vision" ? "Vision" : "Answer"} model → ${TIER_LABEL[tier]}`,
          );
        },
        onError: (e) => {
          set(key, prev);
          toast.error(String(e));
        },
      },
    );
  };

  const save = () => {
    const clean = sanitizeSettings(draft);
    const clamped = JSON.stringify(clean) !== JSON.stringify(draft);
    setDraft(clean); // reflect any clamped/rounded values back into the form
    setSettings.mutate(clean, {
      onSuccess: () => {
        setBaseline(clean);
        toast.success(
          clamped
            ? "Settings saved (some values clamped to valid ranges)"
            : "Settings saved",
        );
      },
      onError: (e) => toast.error(String(e)),
    });
  };

  const reset = () => {
    setDraft(baseline);
    setMonitorsText(baseline.capture_monitors.join(", "));
    setAppsText(baseline.privacy_excluded_apps.join(", "));
  };

  const dirty = JSON.stringify(draft) !== JSON.stringify(baseline);
  const saving = setSettings.isPending;
  const toggleMonitor = (index: number) => {
    let next: number[];
    if (draft.capture_monitors.length === 0 && monitors.data?.length) {
      next = monitors.data.map((m) => m.index).filter((i) => i !== index);
    } else {
      next = draft.capture_monitors.includes(index)
        ? draft.capture_monitors.filter((m) => m !== index)
        : [...draft.capture_monitors, index].sort((a, b) => a - b);
    }
    if (monitors.data?.length && next.length === monitors.data.length) {
      next = [];
    }
    set("capture_monitors", next);
    setMonitorsText(next.join(", "));
  };
  const overlayHotkeyStatus = (hotkeyStatus.data ?? []).find(
    (h) => h.id === "overlay.hotkey",
  );
  const overlayHotkeyWarning =
    overlayHotkeyStatus && !overlayHotkeyStatus.registered
      ? `${overlayHotkeyStatus.error ?? "Windows did not register this shortcut."} Try a different combination.`
      : null;
  const marksHotkeyStatus = (hotkeyStatus.data ?? []).find(
    (h) => h.id === "marks.hotkey",
  );
  const marksHotkeyWarning =
    marksHotkeyStatus && !marksHotkeyStatus.registered
      ? `${marksHotkeyStatus.error ?? "Windows did not register this shortcut."} Try a different combination.`
      : null;
  // Gap #100: the proactive cross-chord check, live against the draft — it fires while
  // the user records a colliding chord and clears the moment the chords differ. The
  // registration-failure warnings above stay the loud safety net for a saved collision.
  const hotkeysConflict = chordsConflict(draft.overlay_hotkey, draft.marks_hotkey);

  // Partial state — surface that models may still be starting: while the readiness
  // probe is in flight, or either lane is still "unknown" (pre-init) / "initializing".
  // Optional chaining guards a partially-populated readiness payload.
  const modelsLoading =
    readiness.isLoading ||
    readiness.data?.embed_model?.status === "initializing" ||
    readiness.data?.embed_model?.status === "unknown" ||
    readiness.data?.sidecar?.status === "initializing" ||
    readiness.data?.sidecar?.status === "unknown";

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col">
          <span className="eyebrow">Settings</span>
          <span className="text-body text-ink-muted font-body">
            Everything stays on this device.{" "}
            {dirty ? "You have unsaved changes." : "All changes saved."}
          </span>
        </div>
        <div className="flex items-center gap-2">
          {dirty && <Chip tone="warn">unsaved</Chip>}
          <Button variant="ghost" onClick={reset} disabled={!dirty || saving}>
            Reset
          </Button>
          <Button variant="primary" onClick={save} disabled={!dirty || saving}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      {/* ————— Essentials (always visible; D6 order) ————— */}

      <Panel group title="Capture">
        <div className="flex flex-col gap-4">
          <SectionIntro>
            What ScreenSearch records: how often the screen is sampled and
            which monitors.
          </SectionIntro>
          <Field
            label="Capture interval (ms)"
            type="number"
            min={250}
            value={draft.capture_interval_ms}
            onChange={intHandler("capture_interval_ms")}
            hint={`How often the screen is sampled. ${APPLY_CAPTURE}`}
          />
          <Field
            label="Monitors"
            value={monitorsText}
            onChange={(e) => {
              setMonitorsText(e.currentTarget.value);
              set("capture_monitors", parseIntList(e.currentTarget.value));
            }}
            hint={`Comma-separated monitor indices (0-based). Empty = all monitors. ${APPLY_CAPTURE}`}
          />
          {monitors.data && monitors.data.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {monitors.data.map((m) => {
                const selected =
                  draft.capture_monitors.length === 0 ||
                  draft.capture_monitors.includes(m.index);
                return (
                  <button
                    key={m.index}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => toggleMonitor(m.index)}
                    className={`rounded-chip border px-3 min-h-hit-min text-caption font-display uppercase tracking-eyebrow ${
                      selected
                        ? "border-accent text-accent bg-accent-wash"
                        : "border-line text-ink-muted hover:text-ink"
                    }`}
                  >
                    {m.name || `Monitor ${m.index}`} · {m.width}×{m.height}
                    {m.is_primary ? " · primary" : ""}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </Panel>

      <Panel group title="Hotkeys">
        <div className="flex flex-col gap-4">
          <SectionIntro>
            Keyboard shortcuts that work anywhere: summon the Flow overlay or
            mark a moment without switching apps.
          </SectionIntro>
          <HotkeyField
            label="Flow overlay"
            value={draft.overlay_hotkey}
            onChange={(v) => set("overlay_hotkey", v)}
            error={overlayHotkeyWarning}
            hint={`Default is ${DEFAULT_OVERLAY_HOTKEY}. AltGr keyboards may report AltGr as Ctrl+Alt. ${APPLY_NOW}`}
          />
          <HotkeyField
            label="Mark this moment"
            value={draft.marks_hotkey}
            onChange={(v) => set("marks_hotkey", v)}
            defaultValue={DEFAULT_MARKS_HOTKEY}
            error={marksHotkeyWarning}
            hint={`Captures the current screen and flags it as an intention. Default is Ctrl+Alt+M. ${APPLY_NOW}`}
          />
          {hotkeysConflict && (
            <p role="status" className="text-caption text-warn">
              Both shortcuts are set to {draft.overlay_hotkey} — the Flow
              overlay and Mark this moment need different combinations. Change
              one of them.
            </p>
          )}
        </div>
      </Panel>

      <Panel group title="Privacy">
        <div className="flex flex-col gap-4">
          <SectionIntro>
            What is never captured: apps to exclude, and pausing while the
            workstation is locked.
          </SectionIntro>
          <Field
            label="Excluded apps"
            value={appsText}
            onChange={(e) => {
              setAppsText(e.currentTarget.value);
              set("privacy_excluded_apps", parseStrList(e.currentTarget.value));
            }}
            hint={`Comma-separated app names; captures are skipped while one is in the foreground. ${APPLY_CAPTURE}`}
          />
          <Toggle
            label="Pause on lock"
            checked={draft.privacy_pause_on_lock}
            onChange={(v) => set("privacy_pause_on_lock", v)}
            hint={`Stop capturing while the workstation is locked. ${APPLY_CAPTURE}`}
          />
        </div>
      </Panel>

      <Panel
        group
        title="Models"
        action={
          modelsLoading ? <Chip tone="warn">models loading…</Chip> : undefined
        }
      >
        <div className="flex flex-col gap-4">
          <SectionIntro>
            The AI models that tag your screens and answer your questions: pick
            a tier per lane, and load or unload the answer model.
          </SectionIntro>
          <ModelTierPicker
            lane="vision"
            value={draft.models_vision_tier}
            onChange={(t) => changeTier("vision", t)}
            hint={APPLY_NOW}
            disabled={setTier.isPending}
          />
          <ModelTierPicker
            lane="answer"
            value={draft.models_answer_tier}
            onChange={(t) => changeTier("answer", t)}
            hint={APPLY_NOW}
            disabled={setTier.isPending}
          />
          <Toggle
            label="Show model thinking"
            checked={draft.answer_thinking}
            onChange={(v) => set("answer_thinking", v)}
            hint={`Stream the answer model's reasoning before its reply. ${APPLY_ASK}`}
          />
          {/* Engine status + manual load/unload (the D6 "load/unload" of this section). */}
          <div className="border-t border-line pt-4">
            <ModelPanel />
          </div>
          {/* Setup operations (03 §7g): one row per operation with stage, bytes
              and plain failure copy. Renders nothing when there is nothing to
              set up (§4: details list nothing). */}
          <SetupDetails />
        </div>
      </Panel>

      <Panel group title="Storage">
        <div className="flex flex-col gap-4">
          <SectionIntro>
            How long screenshots are kept and how large they are stored.
          </SectionIntro>
          <Field
            label="Max width (px)"
            type="number"
            min={0}
            value={draft.storage_max_width}
            onChange={intHandler("storage_max_width")}
            hint={`Captures wider than this are downscaled. 0 = native resolution (no downscale) — best for ultra-wide monitors. ${APPLY_CAPTURE}`}
          />
          <RetentionControl
            days={draft.storage_retention_days}
            onChange={(d) => set("storage_retention_days", d)}
          />
        </div>
      </Panel>

      {/* App (0.3.2): the app's lifecycle home — run-at-startup, close-to-tray,
          auto-update status/check, version + repo link. Its own command surface
          (not the bulk Save); final Essentials placement per D6. */}
      <AppPanel />

      {/* Data (D6): the local API and export, adjacent as one conceptual section.
          The export stays its own panel so it reads as independent of the API —
          it works with the API off (UI_REFERENCE §5). */}
      <ApiPanel />
      <MemoryPanel />

      <Panel group title="Data export">
        <div className="flex flex-col gap-3">
          <span className="text-body text-ink-muted font-body">
            Export your captured frames, content text, and marks to a JSON file in your
            Downloads folder. Screenshots are not included. Works with the API off.
          </span>
          <div>
            <Button
              variant="secondary"
              disabled={exportData.isPending}
              onClick={() =>
                exportData.mutate(
                  { from: null, to: null },
                  {
                    onSuccess: (r) =>
                      toast.success(
                        `Exported ${r.frames} frames and ${r.marks} marks → ${r.path}`,
                      ),
                    onError: (e) => toast.error(`Export failed: ${String(e)}`),
                  },
                )
              }
            >
              {exportData.isPending ? "Exporting…" : "Export…"}
            </Button>
          </div>
        </div>
      </Panel>

      {/* ————— Advanced (collapsed by default; one expander per group — D6) ————— */}

      <div className="flex flex-col gap-1 pt-2">
        <span className="eyebrow">Advanced</span>
        <span className="text-caption text-ink-muted font-body">
          Everything below runs on sensible defaults — open a group only when
          you need to change how it works.
        </span>
      </div>

      <Expander
        title="Enrichment & scheduling"
        intro="When the heavier AI work runs: scheduled vision tagging."
        open={!!settingsExpanded["enrichment"]}
        onToggle={() => toggleSettingsGroup("enrichment")}
      >
        <div className="flex flex-col gap-4">
          <ScheduleControl
            timerEnabled={draft.enrich_vision_timer_enabled}
            timerIntervalMs={draft.enrich_vision_timer_interval_ms}
            onChange={patch}
          />
        </div>
      </Expander>

      <Expander
        title="Sessions"
        intro="How short context runs are joined and which runs become recallable sessions."
        open={!!settingsExpanded["sessions"]}
        onToggle={() => toggleSettingsGroup("sessions")}
      >
        <div className="flex flex-col gap-4">
          <Field
            label="Minimum session length (seconds)"
            type="number"
            min={30}
            max={3600}
            value={draft.sessions_min_len_secs}
            onChange={intHandler("sessions_min_len_secs")}
            hint={`Runs shorter than this stay as frames without a session. ${APPLY_SESSION}`}
          />
          <Field
            label="Close gap (seconds)"
            type="number"
            min={60}
            max={3600}
            value={draft.sessions_gap_close_secs}
            onChange={intHandler("sessions_gap_close_secs")}
            hint={`A context gap this long closes the current micro-run. ${APPLY_SESSION}`}
          />
        </div>
      </Expander>

      <Expander
        title="Performance throttle"
        intro="Automatically eases background AI work while your machine is busy."
        open={!!settingsExpanded["throttle"]}
        onToggle={() => toggleSettingsGroup("throttle")}
      >
        <div className="flex flex-col gap-4">
          <Toggle
            label="Throttle enrichment under load"
            checked={draft.throttle_enabled}
            onChange={(v) => set("throttle_enabled", v)}
            hint={`Pause heavy enrichment (vision tagging) and ease text-embed concurrency when the machine is under sustained CPU/GPU pressure, so your foreground work stays responsive. Capture, OCR, and storage never pause. ${APPLY_NOW}`}
          />
          <ThrottleReadout />
        </div>
      </Expander>

      <Expander
        title="Text filtering"
        intro="How repeated interface chrome (toolbars, menus, tabs) is kept out of your search results."
        open={!!settingsExpanded["text-filtering"]}
        onToggle={() => toggleSettingsGroup("text-filtering")}
      >
        <div className="flex flex-col gap-4">
          <Toggle
            label="Search app chrome by default"
            checked={draft.text_include_chrome_default}
            onChange={(v) => set("text_include_chrome_default", v)}
            hint={`When on, Recall searches raw / app-chrome text by default instead of just cleaned content. ${APPLY_NOW}`}
          />
          <div className="flex flex-col gap-2 border-t border-line pt-3">
            <span className="eyebrow">Per-app suppression</span>
            <SuppressionReadout />
          </div>
        </div>
      </Expander>

      <Expander
        title="Reports & retrieval"
        intro="How many captured frames a question or report reads: deeper digs cost more time."
        open={!!settingsExpanded["reports"]}
        onToggle={() => toggleSettingsGroup("reports")}
      >
        <div className="flex flex-col gap-4">
          <Field
            label="Ask retrieval depth"
            type="number"
            min={1}
            max={100}
            value={draft.retrieval_default_top_k}
            onChange={intHandler("retrieval_default_top_k")}
            hint={`How many frames an Ask retrieves for grounding. ${APPLY_ASK}`}
          />
          <Field
            label="Report frames per day"
            type="number"
            min={1}
            max={1000}
            value={draft.reports_daily_top_k}
            onChange={intHandler("reports_daily_top_k")}
            hint="Target frames sampled per active day in a report. Report depth scales with the range. Applies to your next report."
          />
        </div>
      </Expander>
    </div>
  );
}
