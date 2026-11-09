# TODO — active deferred work

> Pending, deliberately-deferred work that is **planned and specced** but not yet implemented.
> Each entry is self-contained enough to pick up cold (e.g. after a context reset). Closed items
> move to `specs/archive/TODO.v0.4.5.md` (items closed after the v0.4.5 release; earlier closures
> were recorded in `07_KNOWN_GAPS.md` / `08_CHANGELOG_AI.md`). Newest first.

---

## TODO-6 — Settings defaults for the reset-to-default marker via IPC (deferred from P7, 0.4.5)

**Status:** OPEN. Deferred from P7 (`ui/src/routes/settings/`) because no defaults source exists
over IPC.

### The suggestion
`get_settings` returns values; nothing returns the as-built defaults. P7's per-row reset marker and
"Reset to default" need a baseline, so the UI carries `SETTINGS_DEFAULTS` — 26 entries mirrored from
`crates/traits/src/ipc.rs` (the `Default for Settings` impl) with the divergence of `storage.max_width`
(`0`) and `storage.retention_days` (`30`) resolved in code's favour where the spec table disagreed.
A `get_settings_defaults` command (or a `defaults` field on the settings payload) would remove the
mirror entirely and make the marker authoritative.

### Why it's deferred (the trade-offs)
- **A new IPC command is a P3-owned surface**, and P7's write set is three UI paths; adding a command
  would have widened both the traits and the Tauri command list.
- **The mirror is currently exact and cited:** `SETTINGS_DEFAULTS` is the only copy, authored at one
  site, and references `crates/traits/src/ipc.rs:632` so a reader can check it. Divergence risk is
  real but bounded: a change to the Rust defaults silently makes the marker wrong, with no test to
  catch it.
- **Nothing blocks on it:** reset writes a value and the autosave path already proves the write, so
  the marker is a UI convenience, not a data path.

### If ever implemented
- Return defaults from the same place that owns `Default for Settings` (no second table), keep
  `SETTINGS_DEFAULTS` as the fallback while the query is in flight, and add a test that compares the
  IPC payload's default for every one of the 26 UI keys against the registry's `default` field.

---

## TODO-5 — History deletion command and its confirm row in Settings (deferred from P7, 0.4.5)

**Status:** OPEN. Deferred from P7: Data ships **export** only. `03 §8`/`UI_REFERENCE §4` name a
destructive-action confirm for Settings, and P7 implements it for export and for the legacy token
regeneration, but deletion of captured history has no command to call.

### The suggestion
A `delete_history`-style command behind a typed scope (all history / before a timestamp / one
session), a `ConfirmAction` row in Data naming exactly what is removed, and a truthful post-delete
state (counts refreshed from the store, not optimistic).

### Why it's deferred (the trade-offs)
- **No IPC surface exists.** Adding one touches the traits, the Tauri command list and the store —
  outside P7's three owned UI paths, and a destructive data path deserves its own phase with its own
  acceptance run rather than a rider on a UI phase.
- **A disabled control or a stub is forbidden** (`P7.md`, `04` — no placeholders), so the honest
  outcome was to ship export plus the storage rows and record the gap here.
- **Safety:** an irreversible delete needs the confirm copy, the scope and the recovery story
  (exports, retention) designed together; shipping half of that is worse than shipping none.

### If ever implemented
- Put the command behind the same locked path as the other store mutations, delete inside one
  transaction, refresh the footprint/row counts from the store afterwards, and prove with a test that
  a partial failure leaves the database unchanged.

---

## TODO-4 — `throttleStatus` is not invalidated by a settings save (deferred from P7, 0.4.5)

**Status:** OPEN, found during P7's native acceptance. `useSetSettings.onSettled`
(`ui/src/lib/ipc/mutations.ts`) invalidates `settings`, `hotkeyStatus` and `apiStatus` only, so any
surface built on `useThrottleStatus()` keeps showing pre-save state until the next `throttle_changed`
event.

### The suggestion
Add `throttleStatus` to that invalidation list (and audit the rest of the map the same way: a key
whose writes affect another query's payload needs that query invalidated, otherwise the UI shows a
stale projection of a value the user just changed).

### Why it's deferred (the trade-offs)
- **`ui/src/lib/ipc` is not P7's write set.** P7 worked around it inside its own surface: the Advanced
  throttle readout now takes its on/off line from the **saved** setting (`<ThrottleReadout
  enabled={throttle.value === true} />`) and shows an honest intermediate line ("Turning on — waiting
  for the first pressure sample.") while the probe lags, instead of printing "Throttle is off" after
  the switch was turned on.
- **The workaround is correct but local:** any future readout built on `useThrottleStatus` inherits the
  same staleness, so the fix belongs in the shared invalidation map, not in each consumer.

### If ever implemented
- Invalidate `throttleStatus` from `onSettled` and drop the `enabled` prop from `ThrottleReadout`;
  add a test that asserts the invalidation set covers every query keyed on a settable setting.

---

## TODO-3 — Settings-level hotkey-conflict check for the mark/overlay chords (deferred from PR #75, 0.3.0 PR6)

**Status:** OPEN. Deferred **by decision**, not by time. Reviewer suggestion from PR #75
(claude hand review). Low value given the existing OS-level guard; recorded so the deferral is
deliberate.

**P7 evidence (2026-11-05):** on a clean profile the seeded default chord `Ctrl+Alt+M` failed OS
registration on a real machine (`HotKey already registered`), and Settings showed the failure on the
row plus a toast with the stored value unchanged — TODO-3's premise, confirmed live. The UI-side
cross-chord warning now exists; the settings-layer half is unchanged.

**0.4.5 QA note (2026-11-07, PR #169 — fact fix):** the registration path was rebuilt as one
`apply_hotkey` over a `HotkeyEnv` seam (`src-tauri/src/overlay.rs:227`): chord ownership is detected
before any attempt (`is_ours`, canonical comparison through the plugin's own shortcut parser), a
failed attempt never clears a live chord, and a repeated failure for an unchanged chord retries
silently. The OS gate below the seam is unchanged — `register_overlay_hotkey`/`register_marks_hotkey`
(`on_shortcut`) surface a genuine conflict through `emit_hotkey_warning`
(`src-tauri/src/overlay.rs:624`) — and `sanitize_settings`
(`crates/kernel/src/settings.rs:208-217`) still accepts the same chord silently, so this TODO's
premise stands; only the "second registration fails" description above predates the seam.

### The suggestion
`sanitize_settings` (`crates/kernel/src/settings.rs`) accepts `overlay_hotkey` and `marks_hotkey`
set to the **same chord** silently. A settings-layer cross-check could reject/flag the collision
earlier and more cheaply than an OS round-trip.

### Why it's deferred (the trade-offs)
- **The conflict is already caught, loudly.** When two chords collide, the second
  `global_shortcut().on_shortcut(...)` registration fails at OS-registration time and surfaces the
  D6 warning toast + a `failed` `HotkeyStatus` (`src-tauri/src/overlay.rs`
  `register_*_hotkey` → `emit_hotkey_warning`). The user sees it immediately; nothing is silently
  dropped. This is the same path every other unavailable-chord case flows through.
- **Consistency.** Every hotkey availability decision currently lives at registration time, keyed by
  the actual OS response — the only source of truth for "is this chord free" (a chord can also
  collide with a *third-party* global hotkey, which a settings-layer app-internal check can't know).
  Adding a partial app-internal check duplicates that logic while still not being authoritative.
- **Not hot / not a correctness bug.** Settings save is user-initiated and rare; the collision is
  recoverable (pick another chord) and observable.

### If ever implemented — do it without fragmenting the source of truth
- Treat a settings-layer check as a *fast-path UX hint* (e.g. inline field warning "same as Flow
  overlay"), not as the authority — keep the OS-registration path as the real gate so third-party
  collisions still surface.
- Mirror the check in both the Rust `sanitize_settings` and the UI hotkey module if it's meant to
  block save, and add a round-trip test. The UI half is not new: `chordsConflict` was built in
  0.3.2 PR5 inside the single-file Settings screen, and P7 re-homed it to
  `ui/src/routes/settings/hotkeys.ts` (case-insensitive, modifier-order-insensitive) where it warns on
  the two hotkey rows while the chords match and clears when they differ. The backend half —
  `crates/kernel/src/settings.rs` `sanitize_settings` accepting the same chord silently — is still
  open.

### Also considered, rejected (not a TODO)
- **Done/Dismiss in `IntentionsStrip` share one resolve op** (claude review): this is the documented
  backend design — dismiss = resolve-with-no-action, the same `resolve_mark` call
  (`crates/store/src/marks.rs`). Two labels, one op, by intent. No change warranted.

### Files
- `crates/kernel/src/settings.rs` (`sanitize_settings`) · `src-tauri/src/overlay.rs` (the live
  registration gate that already handles this) · `ui/src/routes/settings/hotkeys.ts`
  (`chordsConflict`, if the check should also live client-side).

### References
- PR #75 review (claude hand review, finding #3) · `07_KNOWN_GAPS.md` (0.3.0 PR6 decisions) ·
  D6 loud-conflict decision.

---

## TODO-2 — Bulk span-merge for the purged-span backfill (deferred from `fix/degrade-to-text-db-growth`, `07` #73a)

**Status:** OPEN. Deferred **by decision**, not by time. Reviewer suggestion from PR #67
(gemini-code-assist, flagged "high" against its N+1 guideline). Low value here; recorded so the
deferral is deliberate.

### The suggestion
Replace the per-frame `store.merge_frame_spans_to_lines(id)` calls in the one-time backfill
(`src-tauri/src/lib.rs::merge_purged_spans_once`) with a single bulk method
`merge_batch_frame_spans_to_lines(frame_ids)` that fetches all spans for a 256-id batch with one
`WHERE frame_id IN (…)` query and merges them inside one transaction — turning N per-frame
round-trips into ~1 per batch.

### Why it's deferred (the trade-offs the guideline misses for this codebase)
- **Embedded SQLite, no network hop.** The N+1 "round-trip" cost the guideline targets is a
  networked-DB concern. Here every query is an in-process `rusqlite` call against a local file; the
  per-call overhead is a prepared-statement step, not a network round-trip.
- **Neither path is hot.** `merge_purged_spans_once` is a one-time backfill that drains the
  pre-existing purged backlog once, then watermarks off forever (`maintenance.purged_spans_merged`).
  The inline degrade is one frame per retention-sweep iteration (hourly). No steady-state cost.
- **Per-frame transactions are intentional.** They keep each write-lock window tiny so the sweep /
  backfill never blocks live capture writes, and — critically — they give **per-frame failure
  isolation**: the current `clean_drain` logic skips a single busy frame and retries just that frame
  next launch. A single `IN`-clause transaction rolls back the **whole** 256-batch on one busy
  frame; since the same frame poisons the batch on every retry, the backfill could **never
  converge**. A naive bulk rewrite is strictly worse resilience.

### If ever implemented — do it without regressing resilience
- Bound the batch inside one transaction, but keep per-frame error isolation: a failed
  `replace_text_spans` for one id must not abort the others' commits (e.g. `SAVEPOINT` per id, or
  collect failures and still commit the good ones while withholding the watermark). Verify a
  poisoned frame can't strand the whole batch's progress.
- Keep the retention sweep's atomic `degrade_frame_to_text` per-frame regardless — it interleaves
  with per-frame file deletion + the `image_purged` flag and can't be batched.

### Files
- `crates/store/src/records.rs` (new `merge_batch_frame_spans_to_lines`; `read_text_spans` already
  exists — add a batched read or loop reads inside one tx)
- `src-tauri/src/lib.rs::merge_purged_spans_once` (call the batch method per 256-id page)

### References
- PR #67 review threads (gemini-code-assist on `records.rs` `merge_frame_spans_to_lines` and on
  `lib.rs` `merge_purged_spans_once`) · `07_KNOWN_GAPS.md` #73a · `08_CHANGELOG_AI.md` (2026-07-01).