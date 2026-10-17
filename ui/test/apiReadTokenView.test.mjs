import assert from "node:assert/strict";
import { register } from "node:module";
import { readFileSync } from "node:fs";
import { URL } from "node:url";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
register("./ui-loader.mjs", import.meta.url);
const { ApiReadTokenControls } = await import("../src/components/domain/ApiReadTokenControls.tsx");

const listener = { enabled: true, running: true, last_error: null, bind_address: "127.0.0.1", port: 43210, token: "legacy-test-token" };
function render(status, apiStatus = listener, failed = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, retryOnMount: false } } });
  const key = ["apiReadTokenStatus"];
  if (status !== undefined) client.setQueryData(key, status);
  if (failed) client.getQueryCache().build(client, { queryKey: key }).setState({ status: "error", error: new Error("safe test failure") });
  const markup = renderToStaticMarkup(React.createElement(QueryClientProvider, { client }, React.createElement(ApiReadTokenControls, { apiStatus: apiStatus ?? undefined })));
  client.clear();
  return markup;
}
function button(markup, label) {
  const tag = markup.match(new RegExp(`<button\\b[^>]*>${label}</button>`));
  assert.ok(tag, `${label} is a real keyboard-operable button`);
  return tag[0];
}

test("loading reserves credential field/actions without falsely showing an absent token", () => {
  const html = render(undefined);
  assert.match(html, /Loading read-token availability/);
  assert.match(html, /aria-busy="true"/);
  assert.doesNotMatch(html, /No read token configured/);
  for (const label of ["Create", "Reveal", "Copy", "Rotate", "Revoke"]) assert.match(button(html, `${label} read token`), / disabled=""/);
});

test("empty allows independent create while the API is disabled", () => {
  const html = render({ configured: false }, { ...listener, enabled: false, running: false });
  assert.match(html, /No read token configured/);
  assert.match(html, /API disabled/);
  assert.doesNotMatch(button(html, "Create read token"), / disabled=""/);
  for (const label of ["Reveal", "Copy", "Rotate", "Revoke"]) assert.match(button(html, `${label} read token`), / disabled=""/);
});

test("populated is hidden and offers labeled reveal/copy/rotate/revoke controls", () => {
  const html = render({ configured: true });
  assert.match(html, /Read token configured/);
  assert.match(html, /Read token hidden/);
  assert.doesNotMatch(html, /legacy-test-token/);
  assert.match(button(html, "Create read token"), / disabled=""/);
  for (const label of ["Reveal", "Copy", "Rotate", "Revoke"]) assert.doesNotMatch(button(html, `${label} read token`), / disabled=""/);
  assert.match(button(html, "Reveal read token"), /aria-pressed="false"/);
  assert.match(html, /aria-labelledby=/);
  assert.match(html, /role="group" aria-label="Read-only token actions"/);
});

test("partial states show credential availability separately from off/bind-failed/unknown listener", () => {
  for (const [api, text] of [
    [{ ...listener, enabled: false, running: false }, /API disabled/],
    [{ ...listener, running: false, last_error: "test bind failure" }, /API bind failed/],
    [null, /API listener status is unavailable/],
  ]) {
    const html = render({ configured: true }, api);
    assert.match(html, /Read token configured/);
    assert.match(html, text);
    assert.doesNotMatch(button(html, "Revoke read token"), / disabled=""/);
  }
});

test("error offers status retry; failed refresh keeps previously known controls", () => {
  const unknown = render(undefined, listener, true);
  assert.match(unknown, /availability unknown/);
  assert.match(unknown, /role="alert"/);
  assert.doesNotMatch(button(unknown, "Retry read-token status"), / disabled=""/);
  assert.match(button(unknown, "Create read token"), / disabled=""/);
  const stale = render({ configured: true }, listener, true);
  assert.match(stale, /previous status may be out of date/);
  assert.doesNotMatch(button(stale, "Revoke read token"), / disabled=""/);
});

test("copy warns about scope, plain HTTP and admission-only revocation; embedding preserves legacy surface", () => {
  const html = render({ configured: true });
  for (const phrase of [/raw and filtered history/, /stored images and saved reports/, /cannot Ask/, /change marks/, /capture now/, /retry reports or deliver/, /Plain HTTP is not encrypted/, /subsequent requests/, /already authorized streams/, /never enable the API/]) assert.match(html, phrase);
  const panel = readFileSync(new URL("../src/components/domain/ApiPanel.tsx", import.meta.url), "utf8");
  assert.equal(panel.match(/<ApiReadTokenControls/g)?.length, 3, "read controls also exist during legacy status loading/error");
  for (const preserved of [/useRegenerateApiToken/, /useSetApiConfig/, /copyToken/, /Bearer token/]) assert.match(panel, preserved);
});
