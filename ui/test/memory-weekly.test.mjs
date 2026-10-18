// Weekly synthesis UI (`03 §8e`, G5/G6): the panel must expose a default-off
// weekly schedule beside the daily one, preview the first complete ISO week,
// let the user run the most recent complete week while the schedule is off,
// show latest-week status and older gaps separately from daily runs, and keep
// weekly Telegram a separate explicit opt-in over the shared credential/chat.
//
// These tests use the existing SSR harness (renderToStaticMarkup + a primed
// QueryClient) because the UI test runner intentionally has no DOM framework.
import assert from "node:assert/strict";
import { register } from "node:module";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";

register("./ui-loader.mjs", import.meta.url);
const { MemoryPanel, ReportHistory } = await import("../src/components/domain/MemoryPanel.tsx");
const { memoryKeys } = await import("../src/lib/ipc/memory.ts");
const { promotionKeys } = await import("../src/lib/ipc/promotion.ts");

const BASE_SETTINGS = {
  enabled: true,
  local_time: "08:00",
  timezone: "Europe/Paris",
  output_folder: "C:/Notes",
  telegram_enabled: false,
  telegram_chat_id: null,
  weekly_enabled: false,
  weekly_local_time: "08:00",
  weekly_timezone: "Europe/Paris",
  weekly_telegram_enabled: false,
};

const PREVIEW = {
  local_date: "2026-W43",
  window_start: Date.UTC(2026, 9, 18, 22),
  window_end: Date.UTC(2026, 9, 25, 22),
  due_at: Date.UTC(2026, 9, 26, 6),
};

function reportRun(overrides = {}) {
  return {
    requested_range: null,
    run_id: "run-daily",
    source_id: "source-1",
    kind: "daily",
    schedule_id: null,
    revision_of: null,
    window_start: Date.UTC(2026, 9, 16, 22),
    window_end: Date.UTC(2026, 9, 17, 22),
    timezone: "Europe/Paris",
    local_date: "2026-10-17",
    phase: "finished",
    outcome: "OK",
    error: null,
    result: null,
    config_snapshot: "{}",
    created_at: Date.UTC(2026, 9, 17, 22),
    updated_at: Date.UTC(2026, 9, 17, 22),
    ...overrides,
  };
}

const DAILY_RUN = reportRun({ run_id: "daily-1" });
const WEEKLY_RUN = reportRun({
  run_id: "weekly-1",
  kind: "weekly_synthesis",
  local_date: "2026-W41",
  outcome: "PARTIAL",
  window_start: Date.UTC(2026, 9, 4, 22),
  window_end: Date.UTC(2026, 9, 11, 22),
});

function makeStatus(overrides = {}) {
  return {
    settings: BASE_SETTINGS,
    source_id: "source-1",
    latest_run: null,
    missed_windows: [],
    worker_running: false,
    worker_error: null,
    telegram_credential_available: true,
    weekly_latest_run: null,
    weekly_missed_windows: [],
    ...overrides,
  };
}

function renderPanel({ settings = BASE_SETTINGS, status = makeStatus(), runs = [], weeklyPreview } = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, retryOnMount: false, refetchOnMount: false, staleTime: Infinity } },
  });
  client.setQueryData(memoryKeys.status, status);
  client.setQueryData(memoryKeys.runs, runs);
  client.setQueryData(promotionKeys.noteStatus, {
    path: "C:/Notes/ScreenSearch/Projects/project-alpha.md",
    exists: true,
    count: 1,
  });
  if (weeklyPreview !== undefined) client.setQueryData(["memory-weekly-preview", settings.weekly_local_time, settings.weekly_timezone, settings.output_folder], weeklyPreview);
  const html = renderToStaticMarkup(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(
        MemoryRouter,
        null,
        React.createElement(MemoryPanel, { initialSettings: settings }),
      ),
    ),
  );
  client.clear();
  return html;
}

const escape = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function button(markup, label) {
  const match = markup.match(new RegExp(`<button\\b[^>]*>${escape(label)}</button>`));
  assert.ok(match, `${label} is a real keyboard-operable button`);
  return match[0];
}

function toggle(markup, label) {
  const labelMatch = markup.match(new RegExp(`<label id="([^"]+)"[^>]*>\\s*${escape(label)}\\s*</label>`));
  assert.ok(labelMatch, `${label} has a visible toggle label`);
  const match = markup.match(
    new RegExp(`<button[^>]*role="switch"[^>]*aria-labelledby="${labelMatch[1]}"[^>]*>`),
  );
  assert.ok(match, `${label} is a switch`);
  return match[0];
}

function fieldValue(markup, label) {
  const labelMatch = markup.match(new RegExp(`<label for="([^"]+)"[^>]*>\\s*${escape(label)}\\s*</label>`));
  assert.ok(labelMatch, `${label} has a visible field label`);
  const match = markup.match(new RegExp(`<input[^>]*id="${labelMatch[1]}"[^>]*>`));
  assert.ok(match, `${label} renders an input`);
  return match[0];
}

test("weekly controls default off, show time/zone and the shared folder, and preview the first complete week", () => {
  const html = renderPanel({ weeklyPreview: PREVIEW });

  assert.match(html, /Weekly synthesis/);
  assert.match(toggle(html, "Schedule weekly synthesis"), /aria-checked="false"/);
  assert.match(toggle(html, "Send weekly Telegram digests"), /aria-checked="false"/);
  assert.match(fieldValue(html, "Weekly local time"), /value="08:00"/);
  assert.match(fieldValue(html, "Weekly timezone (IANA)"), /value="Europe\/Paris"/);
  assert.match(html, /ScreenSearch\/Weekly/);
  assert.match(html, /shared with daily reports/);
  assert.match(html, /First eligible week for a new activation: 2026-W43/);
  assert.doesNotMatch(button(html, "Run most recent complete week"), / disabled=""/);
  assert.match(html, /even while the weekly schedule is off/);

  // The existing daily lane and project promotion surface are untouched.
  assert.match(toggle(html, "Schedule daily memory"), /aria-checked="true"/);
  assert.match(fieldValue(html, "Local report time"), /value="08:00"/);
  assert.match(fieldValue(html, "Output folder (absolute path)"), /value="C:\/Notes"/);
  assert.match(html, /Run previous day/);
  assert.match(html, /Project note/);
  assert.match(html, /C:\/Notes\/ScreenSearch\/Projects\/project-alpha\.md/);
});

test("running the latest complete week stays available while weekly scheduling is off, and needs a weekly zone", () => {
  const off = { ...BASE_SETTINGS, weekly_enabled: false };
  const ready = renderPanel({ settings: off, status: makeStatus({ settings: off }) });
  assert.doesNotMatch(button(ready, "Run most recent complete week"), / disabled=""/);

  const noZone = { ...off, weekly_timezone: null };
  const blocked = renderPanel({ settings: noZone, status: makeStatus({ settings: noZone }) });
  assert.match(button(blocked, "Run most recent complete week"), / disabled=""/);
  assert.match(
    blocked,
    /Select and save a weekly timezone and shared output folder before running a weekly synthesis/,
  );
});

test("latest weekly status and older missed weeks stay distinct from the daily status", () => {
  const settings = {
    ...BASE_SETTINGS,
    weekly_enabled: true,
    weekly_local_time: "09:00",
    weekly_timezone: "Europe/Paris",
  };
  const status = makeStatus({
    settings,
    latest_run: DAILY_RUN,
    weekly_latest_run: WEEKLY_RUN,
    missed_windows: [{ start: Date.UTC(2026, 9, 1), end: Date.UTC(2026, 9, 2) }],
    weekly_missed_windows: [
      { start: Date.UTC(2026, 8, 21), end: Date.UTC(2026, 8, 28) },
      { start: Date.UTC(2026, 8, 14), end: Date.UTC(2026, 8, 21) },
    ],
  });
  const html = renderPanel({ settings, status, runs: [DAILY_RUN, WEEKLY_RUN], weeklyPreview: PREVIEW });

  assert.match(html, /Weekly schedule: 09:00 in Europe\/Paris/);
  assert.match(html, /Last daily run: 2026-10-17 · OK/);
  assert.match(html, /Last weekly synthesis: 2026-W41 · PARTIAL/);
  assert.match(html, /Saved daily coverage, evidence, or output is incomplete; see the saved synthesis for limitations/);
  assert.doesNotMatch(html, /Last daily run: 2026-W41/);
  assert.match(html, /Older missed daily windows/);
  assert.match(html, /Missed weeks/);
  assert.doesNotMatch(html, /Older missed weeks/);
  assert.match(html, /or their scheduled synthesis failed/);
  assert.match(html, /not reported as covered/);
});

test("a NO_DATA weekly run is honest and never claims a weekly Telegram digest was sent", () => {
  const weekly = reportRun({
    run_id: "weekly-empty",
    kind: "weekly_synthesis",
    local_date: "2026-W40",
    outcome: "NO_DATA",
  });
  const html = renderPanel({
    status: makeStatus({ weekly_latest_run: weekly }),
    runs: [weekly],
    weeklyPreview: PREVIEW,
  });

  assert.match(html, /Last weekly synthesis: 2026-W40 · NO_DATA/);
  assert.match(html, /No usable weekly evidence was available, so no weekly Telegram digest was sent/);
});

test("weekly Telegram is a separate explicit consent over the shared credential and chat destination", () => {
  const settings = {
    ...BASE_SETTINGS,
    telegram_enabled: false,
    telegram_chat_id: "-1001234567890",
    weekly_telegram_enabled: true,
  };
  const html = renderPanel({ settings, status: makeStatus({ settings }) });

  assert.match(toggle(html, "Send weekly Telegram digests"), /aria-checked="true"/);
  assert.match(toggle(html, "Send Telegram digests"), /aria-checked="false"/);
  assert.match(html, /saved weekly synthesis content to leave this device/);
  assert.match(html, /same saved bot credential and chat ID as daily reports/);
  assert.match(html, /Shared Telegram destination: -1001234567890 · saved bot credential available/);
  assert.match(fieldValue(html, "Telegram chat ID"), /value="-1001234567890"/);
  assert.match(button(html, "Save token"), / disabled=""/);
  assert.match(html, /Credential-pattern masking is not comprehensive PII detection/);
  assert.match(html, /enabling this does not send a test message/);
});

test("weekly preview availability does not block saving valid settings", () => {
  const saved = { ...BASE_SETTINGS, enabled: false, weekly_enabled: false };
  const draft = { ...saved, weekly_enabled: true };

  const blocked = renderPanel({ settings: draft, status: makeStatus({ settings: saved }), weeklyPreview: undefined });
  assert.doesNotMatch(button(blocked, "Save memory settings"), / disabled=""/);

  const ready = renderPanel({ settings: draft, status: makeStatus({ settings: saved }), weeklyPreview: PREVIEW });
  assert.doesNotMatch(button(ready, "Save memory settings"), / disabled=""/);

  const noFolder = { ...draft, output_folder: null };
  const missing = renderPanel({
    settings: noFolder,
    status: makeStatus({ settings: saved }),
    weeklyPreview: PREVIEW,
  });
  assert.match(button(missing, "Save memory settings"), / disabled=""/);
  assert.match(missing, /Select the shared output folder above before enabling weekly synthesis/);
});

test("different weekly and daily zones disclose saved-daily coverage limitations", () => {
  const settings = { ...BASE_SETTINGS, timezone: "Europe/Paris", weekly_timezone: "America/New_York" };
  const html = renderPanel({ settings, status: makeStatus({ settings }) });
  assert.match(html, /Saved dailies are matched by timezone; a different weekly timezone may leave days uncovered/);
});

test("report history filters daily reports and weekly syntheses by run kind", () => {
  const renderHistory = (initialFilter) =>
    renderToStaticMarkup(
      React.createElement(ReportHistory, {
        runs: [DAILY_RUN, WEEKLY_RUN],
        selected: null,
        onSelect: () => {},
        initialFilter,
      }),
    );

  const all = renderHistory("all");
  assert.match(all, /All runs \(2\)/);
  assert.match(all, /Daily reports \(1\)/);
  assert.match(all, /Weekly synthesis \(1\)/);
  assert.match(all, /2026-10-17 · OK · daily report/);
  assert.match(all, /2026-W41 · PARTIAL · weekly synthesis/);

  const daily = renderHistory("daily");
  assert.match(button(daily, "Daily reports (1)"), /aria-pressed="true"/);
  assert.match(button(daily, "All runs (2)"), /aria-pressed="false"/);
  assert.match(daily, /2026-10-17 · OK/);
  assert.doesNotMatch(daily, /2026-W41/);

  const weekly = renderHistory("weekly_synthesis");
  assert.match(button(weekly, "Weekly synthesis (1)"), /aria-pressed="true"/);
  assert.match(weekly, /2026-W41 · PARTIAL/);
  assert.doesNotMatch(weekly, /2026-10-17/);
});
