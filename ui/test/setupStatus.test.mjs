// Setup visibility (03 §7g) — pure-logic tests for the revision ordering guard,
// the chip/details summarizer, and the phase-only announcement. No DOM, no
// React: everything imports from src/lib/setup.ts (plain TypeScript, handled
// by the strip-types runner).
import assert from "node:assert/strict";
import test from "node:test";

import {
  SETUP_OPERATION_LABELS,
  newerSetupStatus,
  setupAnnouncement,
  setupBytesText,
  setupChipStepText,
  setupDoneText,
  setupFailureNextStep,
  setupFormatBytes,
  setupStageText,
  summarizeSetup,
  setupDetailsSteps,
  setupFailureCause,
} from "../src/lib/setup.ts";

function step(overrides) {
  return {
    operation: "engine",
    phase: "checking",
    attempt: 1,
    downloaded_bytes: null,
    total_bytes: null,
    error: null,
    ...overrides,
  };
}

function status(revision, steps) {
  return { revision, steps };
}

// ── newerSetupStatus (the single guard used by the event path and the queryFn) ──

test("newerSetupStatus: an empty cache accepts any incoming snapshot", () => {
  const incoming = status(1, []);
  assert.equal(newerSetupStatus(incoming, null), true);
  assert.equal(newerSetupStatus(incoming, undefined), true);
});

test("newerSetupStatus: an older revision never overwrites a newer cache", () => {
  const newer = status(7, []);
  const older = status(5, []);
  assert.equal(newerSetupStatus(older, newer), false);
});

test("newerSetupStatus: an equal revision is idempotent, never replaces", () => {
  const a = status(3, []);
  assert.equal(newerSetupStatus(a, a), false);
});

test("newerSetupStatus: a higher revision replaces", () => {
  assert.equal(newerSetupStatus(status(6, []), status(5, [])), true);
});

test("newerSetupStatus: a null candidate never replaces", () => {
  assert.equal(newerSetupStatus(null, status(5, [])), false);
});

test("newerSetupStatus: null-vs-null is a no-op (no snapshot, no write)", () => {
  assert.equal(newerSetupStatus(null, null), false);
});

// ── summarizeSetup ────────────────────────────────────────────────────────────

test("summarizeSetup: zero presence when every reported step is done or nothing reports", () => {
  assert.equal(summarizeSetup(null), null);
  assert.equal(summarizeSetup(status(1, [])), null);
  assert.equal(
    summarizeSetup(
      status(9, [
        step({ operation: "engine", phase: "done" }),
        step({ operation: "embeddings", phase: "done" }),
        step({ operation: "vision_model", phase: "done" }),
        step({ operation: "answer_model", phase: "done" }),
      ]),
    ),
    null,
  );
});

test("summarizeSetup: failed outranks active", () => {
  const summary = summarizeSetup(
    status(
      4,
      [
        step({ operation: "engine", phase: "downloading", downloaded_bytes: 1, total_bytes: 10 }),
        step({ operation: "vision_model", phase: "failed", error: "boom" }),
      ],
    ),
  );
  assert.equal(summary.tone, "failed");
  assert.equal(summary.label, "Setup failed");
  assert.equal(summary.stepText, null);
  assert.equal(summary.progress, null);
  assert.equal(summary.operation, "vision_model");
});

test("summarizeSetup: active with known bytes is determinate and floored/clamped 0..100", () => {
  const summary = summarizeSetup(
    status(
      2,
      [step({ operation: "engine", phase: "downloading", downloaded_bytes: 1_234_567, total_bytes: 8_388_608 })],
    ),
  );
  assert.equal(summary.tone, "active");
  assert.equal(summary.label, "Preparing local AI");
  assert.deepEqual(summary.progress, {
    kind: "determinate",
    value: Math.max(0, Math.min(100, Math.floor((1_234_567 / 8_388_608) * 100))),
  });
});

test("summarizeSetup: percentages clamp — at and past total reads 100, never more", () => {
  const at = summarizeSetup(
    status(2, [step({ operation: "engine", phase: "downloading", downloaded_bytes: 100, total_bytes: 100 })]),
  );
  const past = summarizeSetup(
    status(3, [step({ operation: "engine", phase: "downloading", downloaded_bytes: 150, total_bytes: 100 })]),
  );
  assert.equal(at.progress.value, 100);
  assert.equal(past.progress.value, 100);
});

test("summarizeSetup: no percentage when total_bytes is unknown (indeterminate)", () => {
  const summary = summarizeSetup(
    status(2, [
      step({ operation: "engine", phase: "downloading", downloaded_bytes: 1024, total_bytes: null }),
    ]),
  );
  assert.deepEqual(summary.progress, { kind: "indeterminate" });
  assert.match(summary.stepText, /Downloading engine$/);
  assert.doesNotMatch(summary.stepText, /%/);
});

test("summarizeSetup: embeddings preparing is indeterminate and never a percentage", () => {
  const summary = summarizeSetup(
    status(3, [step({ operation: "embeddings", phase: "preparing" })]),
  );
  assert.deepEqual(summary.progress, { kind: "indeterminate" });
  assert.equal(summary.stepText, "Preparing search model");
});

test("summarizeSetup: a determinate active step wins over an indeterminate one", () => {
  const summary = summarizeSetup(
    status(
      5,
      [
        step({ operation: "embeddings", phase: "preparing" }),
        step({ operation: "vision_model", phase: "downloading", downloaded_bytes: 1, total_bytes: 2 }),
      ],
    ),
  );
  assert.equal(summary.operation, "vision_model");
  assert.equal(summary.progress.kind, "determinate");
});

test("summarizeSetup: the active chip copy per operation", () => {
  const cases = [
    ["engine", "checking", null, null, "Checking engine"],
    ["engine", "downloading", 4200, 10000, "Downloading engine 42%"],
    ["engine", "downloading", 4200, null, "Downloading engine"],
    ["engine", "verifying", null, null, "Verifying engine"],
    ["engine", "installing", null, null, "Installing engine"],
    ["embeddings", "preparing", null, null, "Preparing search model"],
    ["vision_model", "downloading", 4200, 10000, "Downloading vision model 42%"],
    ["answer_model", "downloading", 0, 10000, "Downloading answer model 0%"],
  ];
  for (const [operation, phase, dl, total, expected] of cases) {
    assert.equal(
      setupChipStepText(step({ operation, phase, downloaded_bytes: dl, total_bytes: total })),
      expected,
      `${operation}/${phase}`,
    );
  }
});

test("summarizeSetup: zero bytes of a known total reads 0%, not indeterminate", () => {
  const summary = summarizeSetup(
    status(1, [step({ operation: "engine", phase: "downloading", downloaded_bytes: 0, total_bytes: 100 })]),
  );
  assert.deepEqual(summary.progress, { kind: "determinate", value: 0 });
});

// ── announcement: phase changes only, never byte ticks ────────────────────────

test("setupAnnouncement: byte ticks leave the announcement unchanged", () => {
  const tweak = (bytes) =>
    status(1, [step({ operation: "engine", phase: "downloading", downloaded_bytes: bytes, total_bytes: 100 })]);
  const a = setupAnnouncement(tweak(10));
  const b = setupAnnouncement(tweak(90));
  assert.notEqual(a, "");
  assert.equal(a, b, "progress must not re-announce");
});

test("setupAnnouncement: a phase change is announced", () => {
  const downloading = status(1, [step({ operation: "engine", phase: "downloading" })]);
  const installing = status(2, [step({ operation: "engine", phase: "installing" })]);
  assert.notEqual(setupAnnouncement(downloading), setupAnnouncement(installing));
});

test("setupAnnouncement: all-done snapshot says setup is complete; an empty snapshot stays silent", () => {
  const done = setupAnnouncement(
    status(9, [
      step({ operation: "engine", phase: "done" }),
      step({ operation: "embeddings", phase: "done" }),
    ]),
  );
  assert.equal(done, "Local AI setup complete");

  // Nothing has reported yet — announcing "complete" would be false.
  const empty = setupAnnouncement(status(10, []));
  assert.equal(empty, "");
});

test("setupAnnouncement: failures and operations are named plainly", () => {
  const text = setupAnnouncement(
    status(
      4,
      [
        step({ operation: "engine", phase: "failed", error: "boom" }),
        step({ operation: "embeddings", phase: "preparing" }),
      ],
    ),
  );
  assert.equal(text, "Engine failed; Search model preparing");
});

// ── details copy ──────────────────────────────────────────────────────────────

test("stage text per phase", () => {
  const cases = [
    ["checking", "Checking"],
    ["downloading", "Downloading"],
    ["verifying", "Verifying"],
    ["installing", "Installing"],
    ["preparing", "Preparing"],
    ["done", "Done"],
    ["failed", "Failed"],
  ];
  for (const [phase, expected] of cases) {
    assert.equal(setupStageText(step({ phase })), expected, phase);
  }
});

test("done copy per operation", () => {
  assert.equal(setupDoneText("engine"), "Engine available");
  assert.equal(setupDoneText("embeddings"), "Search model ready");
  assert.equal(setupDoneText("vision_model"), "Downloaded");
  assert.equal(setupDoneText("answer_model"), "Downloaded");
});

test("failure next-step copy: engine and embeddings restart, model lanes keep Load", () => {
  assert.equal(
    setupFailureNextStep("engine"),
    "Resolve the problem, then restart ScreenSearch.",
  );
  assert.equal(
    setupFailureNextStep("embeddings"),
    "Resolve the problem, then restart ScreenSearch.",
  );
  for (const lane of ["vision_model", "answer_model"]) {
    assert.equal(
      setupFailureNextStep(lane),
      "Use Load to try again — the download resumes where it stopped.",
    );
  }
});

test("bytes line: '4.8 MB of 32 MB' with a known total, bare received bytes otherwise, empty outside downloading", () => {
  assert.equal(
    setupBytesText(
      step({ phase: "downloading", downloaded_bytes: 5_000_000, total_bytes: 34_000_000 }),
      setupFormatBytes,
    ),
    "4.8 MB of 32 MB",
  );
  assert.equal(
    setupBytesText(
      step({ phase: "downloading", downloaded_bytes: 5_000_000, total_bytes: null }),
      setupFormatBytes,
    ),
    "4.8 MB",
    "the length is unknown — show received bytes only",
  );
  assert.equal(
    setupBytesText(
      step({ phase: "downloading", downloaded_bytes: null, total_bytes: 1000 }),
      setupFormatBytes,
    ),
    "",
    "nothing received yet — no invented figure",
  );
  assert.equal(
    setupBytesText(step({ phase: "preparing" }), setupFormatBytes),
    "",
    "embeddings never shows bytes — this fails loudly if it ever appears",
  );
});

test("operation labels are the plain names", () => {
  assert.deepEqual(SETUP_OPERATION_LABELS, {
    engine: "Engine",
    embeddings: "Search model",
    vision_model: "Vision model",
    answer_model: "Answer model",
  });
});

test("setupFormatBytes formats the way the rail already does", () => {
  assert.equal(setupFormatBytes(0), "0 B");
  // Same rule as ModelPanel/StatusRail: one decimal below 10, whole numbers from 10 up.
  assert.equal(setupFormatBytes(5_000_000), "4.8 MB");
  assert.equal(setupFormatBytes(13_000_000), "12 MB");
  assert.equal(setupFormatBytes(1_073_741_824), "1.0 GB");
});

test("setupDetailsSteps: zero presence once every operation is done; all rows while one is active or failed", () => {
  const allDone = status(5, [
    step({ operation: "engine", phase: "done" }),
    step({ operation: "embeddings", phase: "done" }),
  ]);
  assert.deepEqual(setupDetailsSteps(allDone), [], "an idle, all-done snapshot lists nothing");
  assert.deepEqual(setupDetailsSteps(status(1, [])), []);
  assert.deepEqual(setupDetailsSteps(null), []);

  const oneActive = status(6, [
    step({ operation: "engine", phase: "done" }),
    step({ operation: "embeddings", phase: "preparing" }),
  ]);
  assert.equal(setupDetailsSteps(oneActive).length, 2, "completed rows stay while another operation runs");

  const oneFailed = status(7, [
    step({ operation: "engine", phase: "failed", error: "HTTP 404" }),
    step({ operation: "embeddings", phase: "done" }),
  ]);
  assert.equal(setupDetailsSteps(oneFailed).length, 2, "a failure keeps the list visible");
});

test("setupFailureCause: a visible plain cause from the real error text, never invented", () => {
  assert.equal(
    setupFailureCause(
      "llama-server unavailable: download https://github.com/ggml-org/llama.cpp/releases/download/b0/nope.zip returned HTTP 404 Not Found",
    ),
    "The download server answered HTTP 404 Not Found.",
  );
  assert.equal(
    setupFailureCause("llama.cpp release query returned an error status: HTTP status client error (403 Forbidden)"),
    "HTTP status client error (403 Forbidden)",
    "no 'HTTP <code>' token: fall back to the innermost error, not a guessed cause",
  );
  assert.equal(
    setupFailureCause("verify sidecar asset llama.zip: sha256 mismatch: expected aa, got bb"),
    "The download didn’t match its published checksum, so it wasn’t installed.",
  );
  assert.equal(setupFailureCause("create dir: access is denied (os error 5)"), "Access is denied (os error 5)");
  assert.equal(setupFailureCause(null), "");
  assert.equal(setupFailureCause("   "), "");
  const long = setupFailureCause("x".repeat(400));
  assert.ok(long.length <= 160, "capped to one line");
  assert.ok(!/offline/i.test(setupFailureCause("error sending request: connection refused")), "never infers offline");
});
