// Settings v2 registry — the one static description of the closed 26-key set
// (`03 §8`, 0.4.5 D6). Pure TypeScript: no React, no JSX, no runtime imports, so
// `node --experimental-strip-types` can import it directly for the acceptance check.
import type { Settings } from "../../bindings/Settings";

export type SettingCategoryId =
  | "general" | "capture" | "privacy" | "intelligence"
  | "models" | "data" | "integrations" | "advanced";

export interface SettingCategory {
  id: SettingCategoryId;
  label: string;
  summary: string;
}

/** The eight categories in sidebar order (`UI_REFERENCE §3`). */
export const SETTING_CATEGORIES: readonly SettingCategory[] = [
  { id: "general", label: "General", summary: "How ScreenSearch starts, closes and looks." },
  { id: "capture", label: "Capture", summary: "What gets captured, how often, and the chords that reach it." },
  { id: "privacy", label: "Privacy", summary: "What ScreenSearch never captures." },
  { id: "intelligence", label: "Intelligence", summary: "How enrichment, retrieval and sessions behave." },
  { id: "models", label: "Models", summary: "Which local models run, and how they are loaded." },
  { id: "data", label: "Data", summary: "What is stored, for how long, and how it leaves the machine." },
  { id: "integrations", label: "Integrations", summary: "The local HTTP API and the credentials that reach it." },
  { id: "advanced", label: "Advanced", summary: "Tuning that lives in the config file rather than here." },
];

export type SettingKey =
  | "app.run_at_startup" | "app.close_to_tray" | "ui.appearance"
  | "capture.interval_ms" | "capture.monitors" | "overlay.hotkey" | "marks.hotkey"
  | "privacy.excluded_apps" | "privacy.pause_on_lock"
  | "answer.thinking" | "enrich.vision_timer_enabled" | "enrich.vision_timer_interval_ms"
  | "reports.daily_top_k" | "retrieval.default_top_k"
  | "sessions.gap_close_secs" | "sessions.min_len_secs" | "text.include_chrome_default"
  | "models.answer_tier" | "models.vision_tier"
  | "storage.max_width" | "storage.retention_days"
  | "api.enabled" | "api.bind_address" | "api.port" | "api.token"
  | "throttle.enabled";

/** Every key that maps to a field of the persisted `Settings` struct. */
export type SettingsFieldKey = Exclude<SettingKey, "api.token">;

export type SettingsField = keyof Settings;

export interface NumberRange {
  min: number;
  max: number;
  unit?: string;
  /**
   * A sentinel `min` that is the only value allowed below this floor. The kernel
   * keeps `0` as "native / no downscale" and clamps every other value up to 320
   * (`crates/kernel/src/settings.rs` `sanitize_settings`), so a UI that accepted
   * 1-319 would report a save for a width the backend never stores.
   */
  zeroOrAtLeast?: number;
  /** What the sentinel means, for the rejection message ("native width"). */
  zeroLabel?: string;
}

/** Why a numeric entry cannot be saved, or null when it is acceptable. */
export function numberEntryError(
  def: SettingDef,
  parsed: number,
): string | null {
  if (!Number.isInteger(parsed)) return `${def.label} needs a whole number.`;
  const range = def.range;
  if (!range) return null;
  if (parsed < range.min || parsed > range.max) {
    return (
      `${def.label} must be between ${range.min} and ${range.max}` +
      `${range.unit ? ` ${range.unit}` : ""}.`
    );
  }
  const floor = range.zeroOrAtLeast;
  if (floor !== undefined && parsed !== range.min && parsed < floor) {
    const unit = range.unit ? ` ${range.unit}` : "";
    const meaning = range.zeroLabel ? ` (${range.zeroLabel})` : "";
    return `${def.label} must be ${range.min}${meaning} or at least ${floor}${unit}.`;
  }
  return null;
}

export interface SettingDef {
  key: SettingKey;
  category: SettingCategoryId;
  /** The `Settings` field this row writes; null only for the credential row. */
  field: SettingsField | null;
  /** What the user controls, sentence case (`UI_REFERENCE §9`). */
  label: string;
  /** One plain-language line saying what changes for the user. */
  effect: string;
  /** Numeric limits, mirrored from the backend clamps (`03 §8`). */
  range?: NumberRange;
}

/**
 * Exactly 26 definitions, one per `UI` key of `03 §8`, `ui.appearance` included.
 * Category membership follows that table's `Where` column; `overlay.hotkey` and
 * `marks.hotkey` sit in Capture by maintainer decision (2026-11-05) so the
 * cross-chord warning lives beside both chords.
 */
export const SETTING_DEFS: readonly SettingDef[] = [
  // General
  { key: "app.run_at_startup", category: "general", field: "app_run_at_startup",
    label: "Launch at startup",
    effect: "Starts ScreenSearch when you sign in to Windows." },
  { key: "app.close_to_tray", category: "general", field: "app_close_to_tray",
    label: "Close to tray",
    effect: "Closing the window keeps ScreenSearch running in the tray, so capture continues." },
  { key: "ui.appearance", category: "general", field: "ui_appearance",
    label: "Theme",
    effect: "Picks the colours of the whole app; the system options follow your Windows light or dark choice." },
  // Capture
  { key: "capture.interval_ms", category: "capture", field: "capture_interval_ms",
    label: "Interval between captures",
    effect: "How often a screenshot is taken while capture is running.",
    range: { min: 250, max: 3_600_000, unit: "ms" } },
  { key: "capture.monitors", category: "capture", field: "capture_monitors",
    label: "Monitors to capture",
    effect: "Which displays are captured; an empty choice captures every monitor." },
  { key: "overlay.hotkey", category: "capture", field: "overlay_hotkey",
    label: "Overlay hotkey",
    effect: "The chord that summons the Flow overlay from anywhere." },
  { key: "marks.hotkey", category: "capture", field: "marks_hotkey",
    label: "Mark-this-moment hotkey",
    effect: "The chord that marks the current moment without opening a window." },
  // Privacy
  { key: "privacy.excluded_apps", category: "privacy", field: "privacy_excluded_apps",
    label: "Apps never captured",
    effect: "Screenshots are skipped while one of these apps is in front." },
  { key: "privacy.pause_on_lock", category: "privacy", field: "privacy_pause_on_lock",
    label: "Pause capture when the screen locks",
    effect: "Capture stops while Windows is locked and resumes when you unlock." },
  // Intelligence
  { key: "answer.thinking", category: "intelligence", field: "answer_thinking",
    label: "Show reasoning while answering",
    effect: "Turns on the slower, more careful answer mode; off answers faster." },
  { key: "enrich.vision_timer_enabled", category: "intelligence", field: "enrich_vision_timer_enabled",
    label: "Tag screenshots on a timer",
    effect: "Runs vision tagging in the background on a schedule instead of only on demand." },
  { key: "enrich.vision_timer_interval_ms", category: "intelligence", field: "enrich_vision_timer_interval_ms",
    label: "Vision timer interval",
    effect: "How often the scheduled vision pass runs.",
    range: { min: 60_000, max: 86_400_000, unit: "ms" } },
  { key: "reports.daily_top_k", category: "intelligence", field: "reports_daily_top_k",
    label: "Frames sampled per period",
    effect: "How many frames a recall report reads for each active period.",
    range: { min: 1, max: 1_000 } },
  { key: "retrieval.default_top_k", category: "intelligence", field: "retrieval_default_top_k",
    label: "Search results to retrieve",
    effect: "How many moments Ask reads before it answers.",
    range: { min: 1, max: 100 } },
  { key: "sessions.gap_close_secs", category: "intelligence", field: "sessions_gap_close_secs",
    label: "Session gap to close",
    effect: "Idle gaps shorter than this stay inside one session.",
    range: { min: 60, max: 3_600, unit: "s" } },
  { key: "sessions.min_len_secs", category: "intelligence", field: "sessions_min_len_secs",
    label: "Shortest session",
    effect: "Sessions shorter than this are left out of the timeline.",
    range: { min: 30, max: 3_600, unit: "s" } },
  { key: "text.include_chrome_default", category: "intelligence", field: "text_include_chrome_default",
    label: "Include window chrome in search by default",
    effect: "Search starts by matching menus and titles as well as page text." },
  // Models
  { key: "models.answer_tier", category: "models", field: "models_answer_tier",
    label: "Answer model",
    effect: "Which local model answers your questions." },
  { key: "models.vision_tier", category: "models", field: "models_vision_tier",
    label: "Vision model",
    effect: "Which local model describes your screenshots." },
  // Data
  { key: "storage.max_width", category: "data", field: "storage_max_width",
    label: "Stored image width",
    effect: "Screenshots wider than this are scaled down; 0 keeps the native width.",
    range: {
      min: 0, max: 7_680, unit: "px", zeroOrAtLeast: 320, zeroLabel: "native width",
    } },
  { key: "storage.retention_days", category: "data", field: "storage_retention_days",
    label: "Keep screenshots for",
    effect: "Days before a screenshot is deleted and only its text stays; 0 keeps them forever.",
    range: { min: 0, max: 3_650, unit: "days" } },
  // Integrations
  { key: "api.enabled", category: "integrations", field: "api_enabled",
    label: "HTTP API",
    effect: "Lets other programs on your machine read ScreenSearch over HTTP." },
  { key: "api.bind_address", category: "integrations", field: "api_bind_address",
    label: "Bind address",
    effect: "The network address the API listens on; 127.0.0.1 keeps it on this machine." },
  { key: "api.port", category: "integrations", field: "api_port",
    label: "Port",
    effect: "The port the API listens on.",
    range: { min: 1_024, max: 65_535 } },
  { key: "api.token", category: "integrations", field: null,
    label: "Access token",
    effect: "The credential an external client presents to read your history." },
  // Advanced
  { key: "throttle.enabled", category: "advanced", field: "throttle_enabled",
    label: "Back off under load",
    effect: "Pauses vision tagging and image indexing while the CPU or GPU is busy." },
];

/**
 * Defaults mirrored from the as-built Rust `impl Default for Settings`
 * (`crates/traits/src/ipc.rs:632`), which wins over the `03 §8` table.
 */
export const SETTINGS_DEFAULTS: Settings = {
  app_run_at_startup: false,
  app_close_to_tray: true,
  overlay_hotkey: "Ctrl+Alt+Z",
  marks_hotkey: "Ctrl+Alt+M",
  ui_appearance: "pine-light",
  capture_interval_ms: 3_000,
  capture_monitors: [],
  privacy_excluded_apps: ["1Password", "KeePass", "Bitwarden"],
  privacy_pause_on_lock: true,
  enrich_vision_timer_enabled: false,
  enrich_vision_timer_interval_ms: 3_600_000,
  retrieval_default_top_k: 8,
  reports_daily_top_k: 40,
  sessions_min_len_secs: 120,
  sessions_gap_close_secs: 120,
  text_include_chrome_default: false,
  answer_thinking: true,
  models_vision_tier: "default",
  models_answer_tier: "default",
  storage_retention_days: 30,
  storage_max_width: 0,
  throttle_enabled: false,
  api_enabled: false,
  api_port: 43_210,
  api_bind_address: "127.0.0.1",
};

const BY_KEY = new Map<SettingKey, SettingDef>(SETTING_DEFS.map((def) => [def.key, def]));
const CATEGORY_ORDER = new Map<SettingCategoryId, number>(
  SETTING_CATEGORIES.map((category, index) => [category.id, index]),
);
const CATEGORY_IDS = new Set<string>(SETTING_CATEGORIES.map((category) => category.id));

export function settingDef(key: SettingKey): SettingDef {
  const def = BY_KEY.get(key);
  if (!def) throw new Error(`Unknown settings key: ${key}`);
  return def;
}

/** Definitions of one category, in registry order. */
export function settingsInCategory(id: SettingCategoryId): readonly SettingDef[] {
  return SETTING_DEFS.filter((def) => def.category === id);
}

/** DOM id a search result scrolls to and focuses. */
export function settingAnchorId(key: SettingKey): string {
  return `setting-${key.replace(/\./g, "-")}`;
}

export function isCategoryId(value: string | undefined): value is SettingCategoryId {
  return value !== undefined && CATEGORY_IDS.has(value);
}

export interface SettingMatch {
  def: SettingDef;
  on: "label" | "key" | "effect";
}

/** Which part of a definition a lowercase query hits first, if any. */
function matchOf(def: SettingDef, query: string): SettingMatch["on"] | null {
  if (def.label.toLowerCase().includes(query)) return "label";
  if (def.key.toLowerCase().includes(query)) return "key";
  // The persisted field name is the third spelling people search by
  // (`sessions_min_len_secs` for `sessions.min_len_secs`).
  if (def.field !== null && def.field.toLowerCase().includes(query)) return "key";
  if (def.effect.toLowerCase().includes(query)) return "effect";
  return null;
}

/**
 * Case-insensitive search over every definition, trimmed. Empty query → no results.
 * Order is category order, then registry order (the sort is stable).
 */
export function searchSettings(query: string): SettingMatch[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [];
  const matches: SettingMatch[] = [];
  for (const def of SETTING_DEFS) {
    const on = matchOf(def, q);
    if (on) matches.push({ def, on });
  }
  return matches.sort(
    (a, b) =>
      (CATEGORY_ORDER.get(a.def.category) ?? 0) -
      (CATEGORY_ORDER.get(b.def.category) ?? 0),
  );
}
