//! The COM **MTA** worker thread that owns the `IUIAutomation` instance and services
//! recognize requests. UI Automation is free-threaded and Microsoft recommends an MTA, so
//! (unlike the OCR STA worker) this initializes `COINIT_MULTITHREADED`. All UIA calls run
//! here; the async side ([`crate::UiaTextProvider::recognize`]) only sends plain `Send`
//! data over a channel, so no COM pointer ever crosses a thread boundary.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use tokio::sync::oneshot;
use traits::{normalize_text, OcrResult, TextRole, TextSource, TextSpan};

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, CUIAutomation, IUIAutomation, IUIAutomation2,
    IUIAutomationCacheRequest, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, TreeScope_Element, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId,
    UIA_NamePropertyId, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsIconic};

use crate::{classify, geometry, monitors, window, UiaBudget, CONFIDENCE_UNKNOWN};

/// A recognize request handed to the MTA worker. Carries only `Send` plain data — never
/// the captured pixels (UIA reads the live tree, not the bitmap) and never a COM pointer.
pub(crate) struct Request {
    /// Captured monitor pixel size (== the monitor's resolution) for `[0,1]` mapping.
    pub width: u32,
    pub height: u32,
    /// Normalized `[0,1]` target-window rect within the captured monitor (PR3), used to
    /// drop spans that fall outside the foreground window. `None` ⇒ no containment filter.
    pub target_rect: Option<[f32; 4]>,
    pub monitor_index: u32,
    /// Foreground window handle (as `i64`) at capture time, or `None` if capture couldn't
    /// resolve it. The worker compares it to `GetForegroundWindow` at recognition time and
    /// bails (→ OCR) on a mismatch, so a focus change between capture and recognize can't
    /// attribute another window's text to this frame (`07` #48).
    pub foreground_hwnd: Option<i64>,
    /// Per-request cancellation flag. The caller sets it when its hard timeout fires so the
    /// worker abandons a wedged walk at its next deadline checkpoint instead of hammering the
    /// (already struggling) target to completion. Shared `Arc` — the caller keeps a clone.
    pub cancel: Arc<AtomicBool>,
    /// A `tokio` oneshot so the async caller awaits the reply directly (no `spawn_blocking`):
    /// on a caller-side timeout the receiver is dropped and `send` below simply fails.
    pub resp: oneshot::Sender<WalkReply>,
}

/// One walk's reply: the outcome plus whether it ran over budget, so the caller can feed the
/// per-app circuit breaker. An over-budget walk that still succeeded returns its text
/// (`outcome` is `Ok`) but must count as "bad" for the breaker — hence `over_budget` is
/// surfaced separately rather than folded into the `Result`. (The elapsed time itself is
/// consumed in `worker_main` for the rate-limited over-budget warning and doesn't cross here.)
pub(crate) struct WalkReply {
    pub outcome: Result<OcrResult>,
    /// The walk consumed at least the full latency budget — the target app was heavy. The
    /// soft deadline only fires *between* cross-process calls, so an over-budget walk was
    /// almost certainly truncated mid-tree.
    pub over_budget: bool,
}

/// Hard caps so a pathological accessibility tree can't blow the latency budget or memory.
/// (The node cap is `budget.max_nodes`, a clamped setting — `07` #71.)
const MAX_DEPTH: u32 = 40;
const MAX_SPANS: usize = 10_000;
/// Upper bound on *pending* (pushed-but-not-yet-visited) elements. `max_nodes` only bounds
/// nodes popped; without this a single node with a huge sibling fan-out (virtualized
/// lists/grids commonly expose tens of thousands) — or a malformed provider returning a
/// *cyclic* sibling chain — could push unboundedly. Paired with the per-descent deadline
/// check below, this keeps the walk inside its latency + memory budget for any tree shape.
const MAX_STACK: usize = 16_000;

/// Rate-limit the "walk exceeded budget" warning: only every Nth occurrence is logged so a
/// sustained-pressure session doesn't spam the log one line per frame.
static OVER_BUDGET_WARNS: AtomicU64 = AtomicU64::new(0);
const WARN_EVERY: u64 = 64;

/// Worker entry point: init COM (MTA), create the automation object once, then service
/// requests until the channel closes (the provider was dropped). `in_flight` is shared with
/// the async provider: this worker — the sole authority — sets it on receiving a request and
/// clears it (RAII) when the walk returns, so `recognize` can skip a frame to OCR whenever a
/// walk is already running, even after the caller's hard timeout abandoned the future.
///
/// `shutdown` is the provider-level cancellation flag: once set (by `UiaTextProvider::shutdown`
/// on teardown), a walk in progress bails at its next deadline checkpoint and any queued
/// request is answered with an error instead of started. The worker still exits only when the
/// channel closes (all `SyncSender` clones dropped) → `CoUninitialize`; `shutdown` just makes
/// that exit prompt even if a heavy walk was under way.
pub(crate) fn worker_main(
    rx: mpsc::Receiver<Request>,
    ready: mpsc::Sender<Result<()>>,
    budget: UiaBudget,
    in_flight: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    // SAFETY: COM apartment init for the dedicated UIA worker thread. UIA prefers MTA.
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        let _ = ready.send(Err(anyhow::anyhow!("CoInitializeEx(MTA) failed: {e}")));
        return;
    }
    // RAII: pair the successful `CoInitializeEx` with `CoUninitialize` on *every* exit path —
    // the `CoCreateInstance` failure return, the normal channel-closed exit, and an
    // unexpected panic in `read_foreground` (which would otherwise skip a manual call).
    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: balances the CoInitializeEx above; runs once on thread teardown.
            unsafe { CoUninitialize() };
        }
    }
    let _com = ComApartment;

    // SAFETY: standard COM object creation; `CUIAutomation` is the documented CLSID.
    let automation: IUIAutomation =
        match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
            Ok(a) => a,
            Err(e) => {
                let _ = ready.send(Err(anyhow::anyhow!(
                    "CoCreateInstance(CUIAutomation) failed: {e}"
                )));
                return; // `_com` drops → CoUninitialize.
            }
        };

    // Best-effort per-call timeouts (IUIAutomation2, Windows 8+, so always present on our
    // Win11 target). A single cross-process UIA call to an unresponsive/hung provider would
    // otherwise block the worker indefinitely — the in-walk deadline only fires *between*
    // calls, not during one. Bounding both to the (clamped) latency budget keeps one hung
    // call from wedging the worker; the call just errors and that element is skipped.
    if let Ok(a2) = automation.cast::<IUIAutomation2>() {
        let ms = budget.latency_ms.clamp(1, u32::MAX as u64) as u32;
        // SAFETY: setters on the live automation object on the COM thread; failure is benign.
        unsafe {
            let _ = a2.SetConnectionTimeout(ms);
            let _ = a2.SetTransactionTimeout(ms);
        }
    }
    let _ = ready.send(Ok(()));

    // RAII flag-clearer so `in_flight` is reset on every exit path of a walk — including a
    // panic in `read_foreground` — never leaving the guard stuck "busy" (which would wedge
    // every future frame onto the OCR fallback).
    struct InFlight<'a>(&'a AtomicBool);
    impl Drop for InFlight<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    while let Ok(req) = rx.recv() {
        // Teardown in progress: refuse the queued walk (don't touch the target app) and let
        // the loop spin until the channel closes and the worker exits.
        if shutdown.load(Ordering::Acquire) {
            let _ = req.resp.send(WalkReply {
                outcome: Err(anyhow::anyhow!("UIA worker shutting down")),
                over_budget: false,
            });
            continue;
        }
        in_flight.store(true, Ordering::Release);
        let _guard = InFlight(&in_flight);
        // Time the whole walk (incl. `ElementFromHandle`/cache setup) — the wall-clock the
        // target's UI thread was blocked, which is what the breaker reasons about.
        let started = Instant::now();
        let outcome = read_foreground(&automation, &req, &budget, &shutdown);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let over_budget = elapsed_ms >= budget.latency_ms;
        // A walk that consumed its full latency budget was almost certainly truncated
        // mid-tree (the soft deadline only fires between calls), i.e. the target app is heavy.
        // Surface it — rate-limited — so sustained pressure is visible in the log without
        // per-frame spam. (Hoisted here from `read_foreground` so it also covers over-budget
        // *error* walks, not just successful ones.)
        if over_budget
            && OVER_BUDGET_WARNS
                .fetch_add(1, Ordering::Relaxed)
                .is_multiple_of(WARN_EVERY)
        {
            tracing::warn!(
                elapsed_ms,
                budget_ms = budget.latency_ms,
                warn_every = WARN_EVERY,
                "uia walk hit its latency budget; target app under load (rate-limited)"
            );
        }
        let _ = req.resp.send(WalkReply {
            outcome,
            over_budget,
        });
        // `_guard` drops → in_flight = false, admitting the next frame.
    }

    // `_com` drops here (channel closed, provider gone) → CoUninitialize.
}

/// Reads the foreground window's accessibility text into an [`OcrResult`] tagged
/// `engine = "uia"` with `source = TextSource::Uia` spans. Returns `Err` (so the composite
/// falls back to OCR) when this frame's monitor doesn't hold the foreground window
/// (`target_rect` is `None`), there is no usable foreground element, the latency budget is
/// exceeded, or the yield is below `min_text_chars`.
fn read_foreground(
    automation: &IUIAutomation,
    req: &Request,
    budget: &UiaBudget,
    shutdown: &AtomicBool,
) -> Result<OcrResult> {
    // On a multi-monitor capture, `target_rect` is `Some` only for the frame whose monitor
    // holds the foreground window (capture's `normalize_window_rect` returns `None` for every
    // other monitor — only the display whose region contains the window's centre gets a rect).
    // UIA always reads `GetForegroundWindow`, so with no target rect we cannot confirm the
    // foreground window is even on *this* frame's monitor; proceeding would tag a background
    // monitor's frame with the foreground window's text — silent data corruption, and UIA is
    // default-ON. Treat `None` as a UIA miss → OCR fallback, which reads this monitor's pixels.
    let Some(target_rect) = req.target_rect else {
        bail!("no target_rect (foreground window not on this monitor) — fall back to OCR");
    };

    // The foreground window "now"; capture→recognize are sequential, so this is the
    // captured one. Skip our own window class of failures the same way capture's gate does.
    // SAFETY: plain Win32 queries on this thread.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        bail!("no foreground window");
    }
    // SAFETY: IsIconic on a valid HWND.
    if unsafe { IsIconic(hwnd) }.as_bool() {
        bail!("foreground window minimized");
    }
    // Confirm the foreground window is still the one capture saw. A focus change between the
    // WGC frame and this recognize call would otherwise read a *different* window's text into
    // this frame — and `within_target` can't catch it when the new window overlaps the old
    // (captured) rect. Mismatch → bail → OCR for this frame. (`None` ⇒ capture couldn't record
    // a handle; the rect-containment check below remains the guard.)
    if let Some(captured) = req.foreground_hwnd {
        if hwnd.0 as isize as i64 != captured {
            bail!("foreground window changed since capture — fall back to OCR");
        }
    }

    // Never walk a Chromium/Electron window (`Chrome_WidgetWin_*`): the first UIA touch forces
    // the target to build its whole accessibility tree synchronously inside one uninterruptible
    // cross-process COM call that hangs its UI thread ("Not responding"), and nothing here can
    // abort it mid-call (proven live 2026-07-07 — the freeze survived killing our own process).
    // The class read is a cheap local `GetClassNameW`, no COM. Bail → OCR carries the frame
    // (`07` #93). This is the backstop for the `foreground_hwnd == None` / focus-edge cases the
    // composition-root fast-path can't see; the fast path handles the common case without even
    // dispatching here. A fast within-budget bail reads as `Neutral` to the breaker (no trip).
    if let Some(class) = window::class_name_of(hwnd) {
        if classify::is_chromium_window_class(&class) {
            tracing::debug!(%class, "UIA skipped: Chromium/Electron window; OCR carries this frame");
            bail!("chromium/electron window ({class}) — fall back to OCR");
        }
    }

    // SAFETY: ElementFromHandle on the foreground HWND; returns Err if it has no element.
    let root = unsafe { automation.ElementFromHandle(hwnd) }?;
    // Granular DFS with **per-node cache batching** (`07` #71). The old DFS made ~5–8 separate
    // synchronous cross-process COM calls PER NODE (one per property read, plus navigation), which
    // is what froze heavy Chromium/Electron a11y trees. Here each node's properties (+ its
    // `ValuePattern`) are fetched in ONE `BuildUpdatedCache` call and then read from the client
    // cache in-process, cutting the per-node cost to ~1 property call + navigation. Unlike a single
    // bulk `FindAllBuildCache(Subtree/Children)` pull — one *uninterruptible* call that overran the
    // budget on a wide-node window (measured live) — every call here is small, and the node cap +
    // soft deadline are checked between every node and sibling, so the walk always returns partial
    // text within budget on any tree shape. The one uncacheable cost — a document/editor's live
    // `TextPattern` visible ranges — stays live, gated, and capped.
    let cache = build_cache_request(automation, budget.control_view)?;
    // Control view (not raw view) is the load fix: a Chromium/Electron raw tree exposes one element
    // per inline text-run, so a long page or a large grid explodes into tens of thousands of nodes.
    // Control view collapses those to the elements that actually carry text. A clamped setting.
    // SAFETY: *ViewWalker are property accessors returning the shared walker for that view.
    let walker = unsafe {
        if budget.control_view {
            automation.ControlViewWalker()
        } else {
            automation.RawViewWalker()
        }
    }?;

    let mon_origin = monitors::monitor_origin(req.monitor_index).unwrap_or((0, 0));
    let start = Instant::now();
    let deadline = start + Duration::from_millis(budget.latency_ms.max(1));

    let mut spans: Vec<TextSpan> = Vec::new();
    let mut text = String::new();
    let mut line_index: u32 = 0;
    // Live `TextPattern` reads are the one uncacheable, costliest UIA call; cap how many a single
    // walk makes so a document-heavy page can't reopen the call storm (`07` #71).
    let mut textpattern_calls: u32 = 0;

    // Iterative DFS with an explicit stack (no recursion, no `.await`), bounded by node count,
    // depth, span count, and the soft latency deadline checked every node.
    let mut stack: Vec<(IUIAutomationElement, u32)> = vec![(root, 0)];
    let mut nodes: u32 = 0;
    while let Some((elem, depth)) = stack.pop() {
        // Stop on any bound: node cap, span cap, soft deadline, per-request cancel (the
        // caller's hard timeout fired), or provider shutdown (teardown). Cancel/shutdown let
        // us stop marshalling calls into a struggling target the moment we give up on it.
        if nodes >= budget.max_nodes
            || spans.len() >= MAX_SPANS
            || Instant::now() >= deadline
            || req.cancel.load(Ordering::Acquire)
            || shutdown.load(Ordering::Acquire)
        {
            break;
        }
        nodes += 1;

        // ONE cross-process call fetches all of this node's properties (+ the `ValuePattern`)
        // into a cached copy; every property read below is then in-process. On a transient
        // failure (e.g. the per-call timeout firing on a busy provider) we skip only THIS node's
        // own text — the descent below still runs, exactly as the old DFS did when a single
        // property read failed, so one node's cache-build hiccup can't prune its whole subtree.
        // SAFETY: BuildUpdatedCache on a live element on the COM thread.
        if let Ok(cached) = unsafe { elem.BuildUpdatedCache(&cache) } {
            // SAFETY: cached-property accessors on the freshly-cached element (in-process reads).
            let control_type = unsafe { cached.CachedControlType() }
                .map(|c| c.0)
                .unwrap_or(0);
            let is_password = unsafe { cached.CachedIsPassword() }
                .map(|b| b.as_bool())
                .unwrap_or(false);
            let is_offscreen = unsafe { cached.CachedIsOffscreen() }
                .map(|b| b.as_bool())
                .unwrap_or(false);

            if classify::should_emit(control_type, is_password, is_offscreen) {
                // Only probe the live `TextPattern` on document/text controls and only under the
                // per-walk cap; everything else uses the cached `Value`/`Name` (no round-trip).
                let allow_textpattern = classify::control_type_wants_textpattern(control_type)
                    && textpattern_calls < budget.max_textpattern_calls;
                if allow_textpattern {
                    textpattern_calls += 1;
                }
                if let Some(raw) = extract_text(&cached, allow_textpattern) {
                    let trimmed = raw.trim();
                    if !trimmed.is_empty() {
                        // SAFETY: cached bounding rect; on failure the span gets a zero box.
                        let (x, y, w, h) = match unsafe { cached.CachedBoundingRectangle() } {
                            Ok(r) => geometry::normalize_screen_rect(
                                r.left,
                                r.top,
                                r.right,
                                r.bottom,
                                mon_origin,
                                (req.width, req.height),
                            ),
                            Err(_) => (0.0, 0.0, 0.0, 0.0),
                        };
                        if within_target(target_rect, x, y, w, h) {
                            let (words, next) = classify::split_words(trimmed, line_index);
                            for (li, word) in words {
                                spans.push(TextSpan {
                                    normalized_text: normalize_text(&word),
                                    text: word,
                                    source: TextSource::Uia,
                                    role: TextRole::Unknown,
                                    x,
                                    y,
                                    w,
                                    h,
                                    line_index: li,
                                    is_searchable: true,
                                    suppress_reason: None,
                                });
                            }
                            if next > line_index {
                                text.push_str(trimmed);
                                text.push('\n');
                                line_index = next;
                            }
                        }
                    }
                }
            }
        }

        // Descend: push every child (bounded depth) — navigating the live element via the walker.
        // Runs for EVERY node, even one whose `BuildUpdatedCache` failed above: the lighter
        // single-hop navigation calls can succeed where a batched property fetch timed out, so a
        // transient cache-build failure must not drop the node's descendants (old-DFS parity).
        // Compute each next sibling before moving the current child onto the stack so no element is
        // cloned. The caps are re-checked *inside* this loop: a single node can have a massive
        // sibling fan-out, and a malformed provider can return a cyclic sibling chain
        // (`GetNextSiblingElement` never yielding `None`), so the once-per-pop outer checks alone
        // don't bound it. Each navigation call is small and interruptible, so the deadline check
        // here reliably stops a cycle or huge fan-out from spinning the COM thread past the budget.
        if depth < MAX_DEPTH {
            // SAFETY: tree-walk accessors on the live element; Err just means "no (further) child".
            if let Ok(first) = unsafe { walker.GetFirstChildElement(&elem) } {
                let mut child = first;
                loop {
                    if stack.len() >= MAX_STACK
                        || Instant::now() >= deadline
                        || req.cancel.load(Ordering::Acquire)
                        || shutdown.load(Ordering::Acquire)
                    {
                        break;
                    }
                    let next = unsafe { walker.GetNextSiblingElement(&child) }.ok();
                    stack.push((child, depth + 1));
                    match next {
                        Some(n) => child = n,
                        None => break,
                    }
                }
            }
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    tracing::debug!(nodes, spans = spans.len(), elapsed_ms, "uia walk complete");
    // NOTE: the rate-limited "hit its latency budget" warning now lives in `worker_main`,
    // which times the whole walk (including this fn's early bails) so it also covers
    // over-budget *error* walks and feeds the per-app circuit breaker (`over_budget`).

    let text = text.trim_end().to_string();
    let chars = text.chars().count();
    if chars < budget.min_text_chars {
        bail!(
            "uia thin yield ({chars} chars < {} min)",
            budget.min_text_chars
        );
    }
    Ok(OcrResult {
        text,
        mean_confidence: CONFIDENCE_UNKNOWN,
        engine: "uia".to_string(),
        spans,
    })
}

/// Builds the `IUIAutomationCacheRequest` the walk's per-node `BuildUpdatedCache` uses to
/// pre-fetch the metadata + `Name` it reads per element, so the per-node loop reads `Cached*`
/// getters in-process instead of making a cross-process COM call each (`07` #71). The field
/// *value* is intentionally excluded (read live, post-guard — see the value comment below).
/// `_Full` element mode keeps a live backing so `GetCurrentPattern` for the live `ValuePattern`
/// value and a document/editor's `TextPattern` visible ranges still work on the cached element.
///
/// `control_view` must match the walk's navigation view (below): a cache request's `TreeFilter`
/// defaults to the **control-view** condition, and "caching is performed only for elements that
/// appear in [that] view" (MS docs). So in raw-view mode (`capture.uia_view_control_only` off,
/// walk uses `RawViewWalker`), a raw-only element the walker navigates to would get its requested
/// properties skipped by the default filter — `Cached*` reads come back empty and its text is
/// silently lost to OCR. Set the filter to the same view we walk in so every navigated node is
/// eligible to cache (`07` #71).
fn build_cache_request(
    automation: &IUIAutomation,
    control_view: bool,
) -> Result<IUIAutomationCacheRequest> {
    // SAFETY: cache-request construction/config on the automation object on the COM thread.
    unsafe {
        let cache = automation.CreateCacheRequest()?;
        cache.AddProperty(UIA_ControlTypePropertyId)?;
        cache.AddProperty(UIA_NamePropertyId)?;
        cache.AddProperty(UIA_IsPasswordPropertyId)?;
        cache.AddProperty(UIA_IsOffscreenPropertyId)?;
        cache.AddProperty(UIA_BoundingRectanglePropertyId)?;
        // The field *value* (`ValuePattern` / `UIA_ValueValuePropertyId`) is deliberately NOT
        // cached here. `BuildUpdatedCache` pre-fetches every requested property for *every* walked
        // node, so caching the value would pull a password/offscreen field's text into this
        // process *before* the `should_emit` password/offscreen guard runs — breaking the crate's
        // visible-only / "password fields are never read" guarantee. Instead `extract_text` reads
        // the value **live** (`GetCurrentPattern(UIA_ValuePatternId)`), and it is only called after
        // the guard passes, so a masked/hidden field's value is never fetched (parity with the
        // pre-#71 live walk). Value-bearing inputs are a small fraction of nodes, so the live read
        // costs little; the bulk static-text nodes still batch their `Name` above (`07` #71).
        // Keep the filter in the SAME view the walk navigates (control-view default drops raw-only
        // nodes from the cache — see the docstring). Control view is the default, but set it
        // explicitly so the two views stay visibly in lock-step.
        let view_filter = if control_view {
            automation.ControlViewCondition()?
        } else {
            automation.RawViewCondition()?
        };
        cache.SetTreeFilter(&view_filter)?;
        // `_Element` scope: cache only this element's own properties, not its descendants'.
        // Each `BuildUpdatedCache` call in the walk refreshes one element; its children are
        // walked separately, so we only need the element's own props here — that keeps every
        // fetch bounded to a single node instead of an unbounded whole-subtree pull.
        cache.SetTreeScope(TreeScope_Element)?;
        cache.SetAutomationElementMode(AutomationElementMode_Full)?;
        Ok(cache)
    }
}

/// Extracts an element's text via the priority ladder: `TextPattern` **visible** ranges
/// (documents/editors — viewport text only, never the scrolled-off document) → `ValuePattern`
/// current value (inputs) → `Name` (labels, buttons, list items). Returns `None` when the
/// element exposes no non-empty text. `allow_textpattern` is `false` for non-text controls and
/// once the per-walk TextPattern cap is hit, so the costly live `TextPattern` probe is skipped
/// and only `ValuePattern`/`Name` are read (`07` #71).
///
/// The **Name** is read from the client-side cache (`CachedName`, populated by the walk's per-node
/// `BuildUpdatedCache`) — no cross-process cost. The **Value** and the `TextPattern` visible ranges
/// stay live cross-process reads: both are deliberately uncached so a field's text is only fetched
/// after `should_emit` clears the password/offscreen guard (the value would otherwise be prefetched
/// for every node), and text ranges can't be cached anyway. `TextPattern` is further gated to
/// document/editor controls and capped per walk; value-bearing inputs are a small fraction of nodes.
fn extract_text(elem: &IUIAutomationElement, allow_textpattern: bool) -> Option<String> {
    // GetCurrent/CachedPattern returns Err when the pattern is unsupported (windows-rs maps the
    // documented S_OK+NULL result to E_POINTER), so each `if let Ok` cleanly skips.
    // SAFETY: pattern/text accessors on the COM thread; cached getters read the client cache.
    unsafe {
        if allow_textpattern {
            if let Ok(unknown) = elem.GetCurrentPattern(UIA_TextPatternId) {
                if let Ok(text_pattern) = unknown.cast::<IUIAutomationTextPattern>() {
                    // Only the *visible* ranges, not `DocumentRange().GetText(-1)`: the document
                    // range returns the provider's whole document including scrolled-off text that
                    // was never in the captured frame, which both breaks OCR's "only what was
                    // visible" parity and (being a large yield) would wrongly suppress the OCR
                    // fallback. `GetVisibleRanges` returns just what's in the viewport (`07` #48).
                    if let Ok(ranges) = text_pattern.GetVisibleRanges() {
                        if let Ok(len) = ranges.Length() {
                            let mut acc = String::new();
                            for i in 0..len {
                                if let Ok(range) = ranges.GetElement(i) {
                                    if let Ok(bstr) = range.GetText(-1) {
                                        let part = bstr.to_string();
                                        if !part.trim().is_empty() {
                                            if !acc.is_empty() {
                                                acc.push('\n');
                                            }
                                            acc.push_str(&part);
                                        }
                                    }
                                }
                            }
                            if !acc.trim().is_empty() {
                                return Some(acc);
                            }
                        }
                    }
                }
            }
        }
        // Live `ValuePattern` value (inputs). Read live — NOT from the cache — so a field's value
        // is only ever fetched here, after `should_emit` cleared the password/offscreen guard;
        // caching it would prefetch masked/hidden values for every node (see `build_cache_request`).
        // `_Full` cache mode keeps the live backing, so `GetCurrentPattern` works on the cached elem.
        if let Ok(unknown) = elem.GetCurrentPattern(UIA_ValuePatternId) {
            if let Ok(value_pattern) = unknown.cast::<IUIAutomationValuePattern>() {
                if let Ok(bstr) = value_pattern.CurrentValue() {
                    let s = bstr.to_string();
                    if !s.trim().is_empty() {
                        return Some(s);
                    }
                }
            }
        }
        // Cached Name (labels, buttons, list items).
        if let Ok(bstr) = elem.CachedName() {
            let s = bstr.to_string();
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Whether a normalized span box is within the target-window rect (by center). The caller
/// (`read_foreground`) has already bailed when the rect is unknown, so this always receives a
/// concrete rect — a span whose centre falls outside the foreground window is dropped.
fn within_target(target: [f32; 4], x: f32, y: f32, w: f32, h: f32) -> bool {
    let [tx, ty, tw, th] = target;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    cx >= tx && cx <= tx + tw && cy >= ty && cy <= ty + th
}

#[cfg(test)]
mod tests {
    use super::within_target;

    #[test]
    fn within_target_filters_by_center() {
        let target = [0.25, 0.25, 0.5, 0.5]; // centered half-screen window
        assert!(within_target(target, 0.4, 0.4, 0.1, 0.1), "center inside");
        assert!(!within_target(target, 0.8, 0.8, 0.1, 0.1), "center outside");
    }
}
