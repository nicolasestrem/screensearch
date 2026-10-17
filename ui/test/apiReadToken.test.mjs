import assert from "node:assert/strict";
import { setTimeout } from "node:timers";
import test from "node:test";
import { QueryClient, MutationObserver } from "@tanstack/react-query";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import * as commands from "../src/lib/ipc/commands.ts";
import { readTokenActionOptions } from "../src/lib/ipc/apiReadTokenOptions.ts";

const secret = "test-only-read-secret-not-for-cache";
const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
function fixture(overrides = {}) {
  const calls = [];
  const received = [];
  const copied = [];
  let configured = false;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const api = {
    createApiReadToken: async () => { calls.push("create"); configured = true; return { configured }; },
    rotateApiReadToken: async () => { calls.push("rotate"); return { configured }; },
    revokeApiReadToken: async () => { calls.push("revoke"); configured = false; return { configured }; },
    revealApiReadToken: async () => { calls.push("reveal"); return { token: secret }; },
    ...overrides,
  };
  const callbacks = {
    reveal: (value) => received.push(value),
    copy: async (value) => { copied.push(value); },
    isActive: () => true,
  };
  return { client, calls, received, copied, api, callbacks };
}

test("typed read-token wrappers invoke only their frozen commands", async () => {
  globalThis.window = {};
  const calls = [];
  mockIPC((command) => {
    calls.push(command);
    return command === "reveal_api_read_token" ? { token: secret } : { configured: true };
  });
  try {
    for (const name of ["getApiReadTokenStatus", "createApiReadToken", "rotateApiReadToken", "revokeApiReadToken"]) {
      assert.deepEqual(await commands[name](), { configured: true });
    }
    assert.deepEqual(await commands.revealApiReadToken(), { token: secret });
    assert.deepEqual(calls, ["get_api_read_token_status", "create_api_read_token", "rotate_api_read_token", "revoke_api_read_token", "reveal_api_read_token"]);
  } finally { clearMocks(); delete globalThis.window; }
});

test("create/rotate/revoke update availability only, never enable/rebind or fetch secrets", async () => {
  const f = fixture();
  const observer = new MutationObserver(f.client, readTokenActionOptions(f.client, f.api, f.callbacks));
  for (const action of ["create", "rotate", "revoke"]) {
    assert.equal(await observer.mutate(action), undefined);
    assert.deepEqual(f.client.getQueryData(["apiReadTokenStatus"]), { configured: action !== "revoke" });
  }
  assert.deepEqual(f.calls, ["create", "rotate", "revoke"]);
  assert.deepEqual(f.received, []);
  assert.deepEqual(f.copied, []);
  f.client.clear();
});

test("explicit reveal/copy never put a secret into TanStack query or mutation caches", async () => {
  const f = fixture();
  const snapshots = [];
  const inspect = () => snapshots.push(JSON.stringify({ queries: f.client.getQueryCache().getAll(), mutations: f.client.getMutationCache().getAll().map((m) => m.state) }));
  const stop = f.client.getMutationCache().subscribe(inspect);
  const options = readTokenActionOptions(f.client, f.api, f.callbacks);
  assert.equal(options.gcTime, 0);
  assert.equal(options.retry, false);
  const observer = new MutationObserver(f.client, options);
  assert.equal(await observer.mutate("reveal"), undefined);
  assert.equal(await observer.mutate("copy"), undefined);
  assert.deepEqual(f.received, [secret]);
  assert.deepEqual(f.copied, [secret]);
  assert.ok(snapshots.every((snapshot) => !snapshot.includes(secret)));
  stop(); observer.reset(); await tick();
  assert.equal(f.client.getMutationCache().getAll().length, 0);
  f.client.clear();
});

test("failed actions preserve availability and sanitize errors before mutation caching", async () => {
  const f = fixture({ rotateApiReadToken: async () => { throw new Error(secret); } });
  f.client.setQueryData(["apiReadTokenStatus"], { configured: true });
  const observer = new MutationObserver(f.client, readTokenActionOptions(f.client, f.api, f.callbacks));
  await assert.rejects(observer.mutate("rotate"), /Could not rotate the read token/);
  assert.deepEqual(f.client.getQueryData(["apiReadTokenStatus"]), { configured: true });
  const error = observer.getCurrentResult().error;
  assert.ok(!String(error).includes(secret));
  assert.ok(!String(error.cause).includes(secret));
  f.client.clear();
});

test("unmounted controls discard a late reveal and do not write the clipboard", async () => {
  for (const action of ["reveal", "copy"]) {
    let resolve;
    const f = fixture({ revealApiReadToken: () => new Promise((done) => { resolve = done; }) });
    let active = true;
    f.callbacks.isActive = () => active;
    const observer = new MutationObserver(f.client, readTokenActionOptions(f.client, f.api, f.callbacks));
    const pending = observer.mutate(action);
    await tick();
    active = false;
    resolve({ token: secret });
    await pending;
    assert.deepEqual(f.received, []);
    assert.deepEqual(f.copied, []);
    f.client.clear();
  }
});

test("clipboard errors are safe and retry requires a new explicit fetch", async () => {
  const f = fixture();
  f.callbacks.copy = async () => { throw new Error(secret); };
  const observer = new MutationObserver(f.client, readTokenActionOptions(f.client, f.api, f.callbacks));
  await assert.rejects(observer.mutate("copy"), /Could not copy the read token/);
  assert.ok(!String(observer.getCurrentResult().error).includes(secret));
  f.callbacks.copy = async (value) => f.copied.push(value);
  await observer.mutate("copy");
  assert.deepEqual(f.calls, ["reveal", "reveal"]);
  f.client.clear();
});
