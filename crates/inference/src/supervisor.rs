//! The `ModelSupervisor` — owns the single `llama-server` sidecar's whole lifecycle
//! (`03 §6`): Job-Object binding (no orphan), startup reap of a prior run's stray,
//! lazy spawn on first request, `/health` gating, idle eviction, and model switching
//! (stop + restart with a new GGUF; vision adds `--mmproj`). It broadcasts
//! [`SidecarStatus`] transitions for the readiness panel.
//!
//! Concurrency model: one process at a time. [`acquire`](ModelSupervisor::acquire)
//! ensures the requested model is running (spawning or switching as needed) and hands
//! back a [`Lease`] carrying a [`SidecarClient`]; the lease counts the request as
//! in-flight so the idle-evictor never pulls the model out from under a live request.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use tokio::sync::{broadcast, Mutex, OwnedSemaphorePermit, Semaphore};
use traits::{FlashAttnSetting, KvCacheType, SidecarState, SidecarStatus};

use crate::client::SidecarClient;
use crate::flags::{FlashAttnKind, SidecarCaps};
use crate::job_object::JobObject;
use crate::models::ModelSpec;
use crate::process;

/// How often the idle-evictor wakes to check the model's idle time.
const EVICT_TICK: Duration = Duration::from_secs(5);
/// How long to wait between `/health` polls while a model loads.
const HEALTH_POLL: Duration = Duration::from_millis(250);
/// Max time to wait for a killed sidecar to exit (so its VRAM is freed) on a switch.
const PROCESS_EXIT_WAIT: Duration = Duration::from_secs(3);
/// How many times to (re)spawn the sidecar before surfacing a startup failure.
const SPAWN_ATTEMPTS: u32 = 3;
/// Large enough to allow practical same-model parallelism while letting a model switch
/// drain every request slot before terminating the old process.
const REQUEST_GATE_PERMITS: u32 = 1024;

/// Static configuration for a [`ModelSupervisor`].
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Path to the resolved `llama-server.exe` (under app-data, per the runtime
    /// download). Used to launch and always included as a reap sentinel.
    pub binary: PathBuf,
    /// Additional exact `llama-server.exe` image paths this app previously installed.
    /// Startup reap checks these alongside [`Self::binary`] so an override toggle does
    /// not leave an app-owned sidecar running from the old normal/override install.
    pub reap_binaries: Vec<PathBuf>,
    /// Where the child's pid is recorded for the startup reap.
    pub pidfile: PathBuf,
    /// Optional file to capture the sidecar's stdout+stderr (truncated per spawn). `None`
    /// discards them (the prior behavior). The composition root points this at
    /// `<sidecar dir>/llama-server.log` so a spawn/crash diagnostic is recoverable.
    pub sidecar_log: Option<PathBuf>,
    /// Stop the sidecar after this long with no in-flight request (`sidecar.idle_ttl_secs`).
    pub idle_ttl: Duration,
    /// Max time to wait for `/health` after a spawn before declaring failure.
    pub health_timeout: Duration,
    /// Which memory-tuning flags the bundled `llama-server` accepts (probed once at
    /// init). `build_args` consults this so it only emits flags the binary understands —
    /// the binary auto-updates to the latest release, so its flag set is not fixed.
    pub caps: SidecarCaps,
    /// Committed-RAM ceiling (bytes) that triggers a sidecar recycle; `0` disables it.
    /// Resolved once at construction from `sidecar.recycle_*` (like `idle_ttl`).
    pub recycle_ceiling_bytes: u64,
}

/// A currently-running sidecar process and the client bound to it.
struct SidecarProcess {
    child: process::SuspendedChild,
    client: SidecarClient,
    spec: ModelSpec,
}

/// A handle to the running sidecar for the duration of one request. While a lease is
/// alive the request is counted in-flight (eviction is blocked); dropping it records
/// activity so the idle clock restarts from the request's end.
pub struct Lease {
    client: SidecarClient,
    in_flight: Arc<AtomicUsize>,
    last_activity: Arc<StdMutex<Instant>>,
    _permit: RequestPermit,
}

impl Lease {
    /// The client bound to the leased sidecar.
    pub fn client(&self) -> &SidecarClient {
        &self.client
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        if let Ok(mut g) = self.last_activity.lock() {
            *g = Instant::now();
        }
    }
}

/// Serializes sidecar use so a model switch cannot terminate a process while another
/// request is still streaming through it. Normal requests acquire one permit and may
/// run concurrently; any path that stops the child drains all permits first.
#[derive(Clone)]
pub struct RequestGate {
    permits: Arc<Semaphore>,
    capacity: u32,
}

pub struct RequestPermit {
    _permit: OwnedSemaphorePermit,
}

impl RequestGate {
    pub fn new() -> Self {
        Self::with_capacity(REQUEST_GATE_PERMITS)
    }

    fn with_capacity(capacity: u32) -> Self {
        assert!(capacity > 0, "request gate capacity must be non-zero");
        Self {
            permits: Arc::new(Semaphore::new(capacity as usize)),
            capacity,
        }
    }

    pub async fn enter(&self) -> Result<RequestPermit> {
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .context("sidecar request gate closed")?;
        Ok(RequestPermit { _permit: permit })
    }

    pub async fn enter_for_model_switch(&self) -> Result<RequestPermit> {
        let permit = self
            .permits
            .clone()
            .acquire_many_owned(self.capacity)
            .await
            .context("sidecar request gate closed")?;
        Ok(RequestPermit { _permit: permit })
    }
}

impl Default for RequestGate {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestPermit {
    fn into_single(mut self) -> Self {
        if self._permit.num_permits() == 1 {
            return self;
        }
        let permit = self
            ._permit
            .split(1)
            .expect("exclusive sidecar permit should contain at least one permit");
        Self { _permit: permit }
    }
}

/// Owns the sidecar lifecycle. Construct via [`ModelSupervisor::new`] inside a Tokio
/// runtime (it spawns the idle-evictor task).
pub struct ModelSupervisor {
    config: SupervisorConfig,
    job: JobObject,
    state: Mutex<Option<SidecarProcess>>,
    in_flight: Arc<AtomicUsize>,
    last_activity: Arc<StdMutex<Instant>>,
    events: broadcast::Sender<SidecarStatus>,
    shutdown: AtomicBool,
    /// When set, the idle-evictor holds off (the kernel's idle vision backfill is draining
    /// the backlog and wants the model kept warm). Cleared when the backlog is empty or
    /// the user resumes, after which normal idle eviction frees the VRAM (`03 §5/§6`).
    backfill_active: AtomicBool,
    /// Set by a manual "Load model" and cleared by "Unload": the user explicitly asked to
    /// keep this model resident, so the idle-TTL must not evict it (that was the
    /// "evicted right after I downloaded it" surprise). Unload or app exit clears it.
    pinned: AtomicBool,
    gate: RequestGate,
}

impl ModelSupervisor {
    /// Creates the supervisor: a `KILL_ON_JOB_CLOSE` job, a startup reap of any stray
    /// `llama-server` this app left behind, and the background idle-evictor. Must run
    /// inside a Tokio runtime.
    pub fn new(config: SupervisorConfig) -> Result<Arc<Self>> {
        let job = JobObject::new().context("create job object")?;

        // Startup reap: kill a stray sidecar from a prior run, identified by pidfile +
        // exact image-path sentinels (never an unrelated process) — `03 §6`.
        let mut reap_binaries = config.reap_binaries.clone();
        reap_binaries.push(config.binary.clone());
        if reap_stray_any(&config.pidfile, &reap_binaries) {
            tracing::warn!("startup reap terminated a stray sidecar from a prior run");
        }

        let (events, _rx) = broadcast::channel(64);
        let me = Arc::new(Self {
            config,
            job,
            state: Mutex::new(None),
            in_flight: Arc::new(AtomicUsize::new(0)),
            last_activity: Arc::new(StdMutex::new(Instant::now())),
            events,
            shutdown: AtomicBool::new(false),
            backfill_active: AtomicBool::new(false),
            pinned: AtomicBool::new(false),
            gate: RequestGate::new(),
        });
        me.clone().spawn_evictor();
        Ok(me)
    }

    /// Subscribe to sidecar lifecycle transitions (forwarded to `sidecar_status`).
    pub fn subscribe(&self) -> broadcast::Receiver<SidecarStatus> {
        self.events.subscribe()
    }

    /// Ensures `spec` is the running model (spawning or switching as needed) and
    /// returns a [`Lease`] to it. The lease keeps the request in-flight; the caller
    /// runs its HTTP request through `lease.client()`.
    pub async fn acquire(&self, spec: ModelSpec) -> Result<Lease> {
        loop {
            // Memory safety valve: if the running sidecar (same model) has grown past the
            // recycle ceiling, refresh it before serving — drain in-flight, kill (freeing the
            // leaked host RAM + VRAM via the one teardown path), respawn the same spec. This
            // is the only teardown that fires *during* a backfill drain (the leak's runaway
            // case), so it ignores keep-warm/pin; the immediate respawn keeps the model
            // resident. The `!needs_restart` guard means a genuine model switch falls through
            // to the exclusive-switch branch below instead.
            if self.over_recycle_ceiling(&spec).await {
                let permit = self.gate.enter_for_model_switch().await?;
                let mut guard = self.state.lock().await;
                // Re-check under the exclusive lock: another caller may have already recycled
                // or switched, or the process may have been replaced since the probe.
                if guard
                    .as_ref()
                    .is_some_and(|p| !needs_restart(&p.spec, &spec) && self.process_over_ceiling(p))
                {
                    let old = guard
                        .take()
                        .expect("sidecar present after over-ceiling check");
                    self.emit(SidecarState::Recycled, Some(&old.spec));
                    tracing::info!(
                        ceiling_bytes = self.config.recycle_ceiling_bytes,
                        "sidecar recycled at committed-RAM ceiling"
                    );
                    self.stop_child(old).await;
                    let proc = self.spawn_with_retries(&spec).await?;
                    *guard = Some(proc);
                    let lease = self.lease_from_state(
                        guard.as_ref().expect("sidecar present after recycle"),
                        permit.into_single(),
                    );
                    drop(guard);
                    return Ok(lease);
                }
                drop(guard);
                drop(permit);
                continue;
            }
            if self.needs_exclusive_switch(&spec).await {
                let permit = self.gate.enter_for_model_switch().await?;
                let mut guard = self.state.lock().await;
                if !guard
                    .as_ref()
                    .is_some_and(|p| needs_restart(&p.spec, &spec))
                {
                    drop(guard);
                    drop(permit);
                    continue;
                }
                if let Some(old) = guard.take() {
                    // Wait for the old process to fully exit before spawning the new model
                    // so its GPU memory is released first (avoids a VRAM-allocation race on
                    // model switch). A switch to a *different* model also drops any manual
                    // pin — it belonged to the model being replaced (a manual Load re-pins
                    // after this returns).
                    self.pinned.store(false, Ordering::SeqCst);
                    self.stop_child(old).await;
                }
                let proc = self.spawn_with_retries(&spec).await?;
                *guard = Some(proc);
                let lease = self.lease_from_state(
                    guard.as_ref().expect("sidecar present after switch"),
                    permit.into_single(),
                );
                drop(guard);
                return Ok(lease);
            }

            let permit = self.gate.enter().await?;
            let mut guard = self.state.lock().await;
            match guard.as_ref() {
                Some(running) if needs_restart(&running.spec, &spec) => {
                    drop(guard);
                    drop(permit);
                    continue;
                }
                Some(running) if running_sidecar_healthy(running).await => {
                    let lease = self.lease_from_state(running, permit);
                    drop(guard);
                    return Ok(lease);
                }
                Some(running) => {
                    let crashed_spec = running.spec.clone();
                    drop(guard);
                    drop(permit);

                    let permit = self.gate.enter_for_model_switch().await?;
                    let mut guard = self.state.lock().await;
                    if let Some(running) = guard.as_ref() {
                        if !needs_restart(&running.spec, &spec)
                            && running_sidecar_healthy(running).await
                        {
                            drop(guard);
                            drop(permit);
                            continue;
                        }
                        self.emit(SidecarState::Crashed, Some(&crashed_spec));
                    } else {
                        drop(guard);
                        drop(permit);
                        continue;
                    }
                    if let Some(old) = guard.take() {
                        self.stop_child(old).await;
                    }
                    let proc = self.spawn_with_retries(&spec).await?;
                    *guard = Some(proc);
                    let lease = self.lease_from_state(
                        guard
                            .as_ref()
                            .expect("sidecar present after crash recovery"),
                        permit.into_single(),
                    );
                    drop(guard);
                    return Ok(lease);
                }
                None => {
                    let proc = self.spawn_with_retries(&spec).await?;
                    *guard = Some(proc);
                    let lease = self.lease_from_state(
                        guard.as_ref().expect("sidecar present after initial spawn"),
                        permit,
                    );
                    drop(guard);
                    return Ok(lease);
                }
            }
        }
    }

    async fn needs_exclusive_switch(&self, spec: &ModelSpec) -> bool {
        self.state
            .lock()
            .await
            .as_ref()
            .is_some_and(|p| needs_restart(&p.spec, spec))
    }

    /// True when recycling is enabled and the running sidecar (same model as `spec`) has
    /// grown past the committed-RAM ceiling. Cheap no-op when the ceiling is 0 (disabled).
    async fn over_recycle_ceiling(&self, spec: &ModelSpec) -> bool {
        if self.config.recycle_ceiling_bytes == 0 {
            return false;
        }
        let guard = self.state.lock().await;
        guard
            .as_ref()
            .is_some_and(|p| !needs_restart(&p.spec, spec) && self.process_over_ceiling(p))
    }

    /// Probe the sidecar's committed bytes and compare against the ceiling.
    fn process_over_ceiling(&self, p: &SidecarProcess) -> bool {
        process::query_private_bytes(p.child.pid())
            .is_some_and(|rss| should_recycle(rss, self.config.recycle_ceiling_bytes))
    }

    fn lease_from_state(&self, running: &SidecarProcess, permit: RequestPermit) -> Lease {
        let client = running.client.clone();
        // Count the request in-flight **while still holding the state lock**: the
        // evictor re-checks `in_flight` under this same lock before killing, so it can
        // never evict the sidecar we are handing out (closes the drop→fetch_add race).
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut g) = self.last_activity.lock() {
            *g = Instant::now();
        }
        Lease {
            client,
            in_flight: self.in_flight.clone(),
            last_activity: self.last_activity.clone(),
            _permit: permit,
        }
    }

    /// Kills a running sidecar and waits (bounded) for the OS to release it, so its GPU
    /// memory is freed before the next model spawns.
    async fn stop_child(&self, old: SidecarProcess) {
        self.kill_and_confirm("model switch", old.child).await;
    }

    /// Kills `child`, waits (bounded) for the OS to release the process so its VRAM/RAM is
    /// actually freed, removes the pidfile, and logs whether the kill took effect. Every
    /// teardown path (idle eviction, manual unload, model switch, shutdown) routes through
    /// here so a kill that silently fails is visible instead of leaving VRAM occupied
    /// while the UI claims the model is gone (`03 §6`). Polls rather than blocking the
    /// executor on `WaitForSingleObject`.
    async fn kill_and_confirm(&self, reason: &str, child: process::SuspendedChild) {
        let pid = child.pid();
        let killed = child.kill();
        let deadline = Instant::now() + PROCESS_EXIT_WAIT;
        while process::pid_alive(pid) && Instant::now() < deadline {
            tokio::time::sleep(HEALTH_POLL).await;
        }
        let still_alive = process::pid_alive(pid);
        if still_alive {
            // Keep the pidfile: the process outlived the kill, so leave the record on disk
            // for the next launch's `reap_stray_any` to find and terminate. Deleting it here
            // would strand the orphan (and its VRAM) with nothing pointing the reaper at it.
            tracing::warn!(
                reason,
                pid,
                killed,
                "sidecar kill did not terminate the process within the exit wait; VRAM may \
                 still be held"
            );
        } else {
            let _ = std::fs::remove_file(&self.config.pidfile);
            tracing::debug!(reason, pid, killed, "sidecar process terminated");
        }
    }

    /// Spawns the sidecar, retrying a few times. Each attempt allocates a fresh port,
    /// which also hardens against the (rare) race where the chosen ephemeral port is
    /// taken between `free_port` and `llama-server` binding it. `spawn_for` kills the
    /// child on any failure, so a retry never leaks a process.
    async fn spawn_with_retries(&self, spec: &ModelSpec) -> Result<SidecarProcess> {
        let mut last_err = None;
        for attempt in 1..=SPAWN_ATTEMPTS {
            match self.spawn_for(spec).await {
                Ok(proc) => return Ok(proc),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "sidecar spawn failed; retrying");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("sidecar spawn failed")))
    }

    /// Stops the sidecar and disables further eviction work (called on app exit).
    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let mut guard = self.state.lock().await;
        if let Some(p) = guard.take() {
            self.kill_and_confirm("shutdown", p.child).await;
        }
        self.emit(SidecarState::Stopped, None);
    }

    /// Eagerly loads `spec` (spawning or switching as needed) so the next real request is
    /// instant — the manual "Load model" control. The lease is dropped immediately, which
    /// starts the idle clock; the model then stays resident until the idle-TTL or a manual
    /// [`Self::unload`] reclaims it.
    pub async fn preload(&self, spec: ModelSpec) -> Result<()> {
        let _lease = self.acquire(spec).await?;
        // Pin: an explicit Load means "keep this resident" — don't idle-evict it until the
        // user unloads (otherwise the model the user just waited to download/load would be
        // evicted the moment the idle-TTL elapsed).
        //
        // Publish the pin **before dropping `_lease`**: the lease holds a request-gate permit,
        // and `unload_reasoned` reads/clears the pin only after draining that gate. Holding the
        // permit across this store is what makes the pin visible to a relief that wins the gate
        // afterwards, so a preload that lands while relief is waiting is never killed
        // (`03 §6` rule A).
        self.pinned.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Manually stops the resident sidecar, freeing its VRAM/RAM now — the "Unload" control.
    /// Drains the request gate first so a live request is never torn out, then kills the
    /// process and reports `Evicted` (the next request lazily respawns). No-op when nothing
    /// is resident. An explicit Unload also clears the "keep resident" pin, inside that same
    /// gated section, so it cannot interleave with a concurrent preload's pin publication.
    pub async fn unload(&self) {
        if self.unload_reasoned("manual unload", false).await {
            tracing::info!("sidecar unloaded on request");
        }
    }

    /// Stops the resident sidecar for `reason`: drains the request gate so a live request is
    /// never torn out, kills the child through the one teardown path, and reports `Evicted`.
    /// Returns whether a process was actually resident and released, so a caller can tell a
    /// real memory reclaim from a no-op on an empty supervisor.
    ///
    /// `skip_if_pinned` is the level-2 relief rule A (`03 §6`): an explicit user load outranks
    /// relief. Both pin writers are decided **under the gate**, which is what makes that rule
    /// linearizable instead of best-effort:
    /// - a load publishes the pin while it still holds its lease permit ([`Self::preload`]), so
    ///   once this call owns every permit the pin it reads can no longer be revoked by an
    ///   in-flight load — a preload that lands while relief waits is guaranteed to be seen;
    /// - a manual unload clears the pin here rather than before the gate. Clearing it earlier
    ///   would let a concurrent preload publish the pin between the clear and the kill, leaving
    ///   a still-pinned-but-not-resident supervisor whose stale pin suppresses every later
    ///   relief and idle eviction.
    async fn unload_reasoned(&self, reason: &str, skip_if_pinned: bool) -> bool {
        let Ok(_permit) = self.gate.enter_for_model_switch().await else {
            // The gate only closes if the supervisor is tearing down; a manual unload still
            // means "not pinned", so a later relief is not blocked by a stale pin.
            if !skip_if_pinned {
                self.pinned.store(false, Ordering::SeqCst);
            }
            return false;
        };
        if skip_if_pinned && self.pinned.load(Ordering::SeqCst) {
            tracing::info!("level-2 memory relief skipped: model pinned by an explicit load");
            return false;
        }
        if !skip_if_pinned {
            self.pinned.store(false, Ordering::SeqCst);
        }
        let mut guard = self.state.lock().await;
        if let Some(p) = guard.take() {
            self.kill_and_confirm(reason, p.child).await;
            self.emit(SidecarState::Evicted, Some(&p.spec));
            return true;
        }
        false
    }

    /// Spawns `llama-server` for `spec`, binds it to the job **before** resuming, then
    /// waits for `/health`. On any failure the child is killed so nothing leaks.
    async fn spawn_for(&self, spec: &ModelSpec) -> Result<SidecarProcess> {
        let port = free_port().context("allocate sidecar port")?;
        let args = build_args(spec, port, &self.config.caps);
        self.emit(SidecarState::Starting, Some(spec));

        let child = process::spawn_suspended(
            &self.config.binary,
            &args,
            self.config.sidecar_log.as_deref(),
        )
        .with_context(|| format!("spawn {}", self.config.binary.display()))?;
        // Assign-before-resume: no window in which the child runs unbound (`03 §6`).
        if let Err(e) = self.job.assign(child.process_handle()) {
            child.kill();
            return Err(e);
        }
        if let Err(e) = child.resume() {
            child.kill();
            return Err(e);
        }
        let _ = std::fs::write(&self.config.pidfile, child.pid().to_string());

        let base = format!("http://127.0.0.1:{port}");
        let client = SidecarClient::new(base);

        let deadline = Instant::now() + self.config.health_timeout;
        loop {
            if !process::pid_alive(child.pid()) {
                self.emit(SidecarState::Crashed, Some(spec));
                bail!("llama-server exited during startup");
            }
            if client.health().await {
                break;
            }
            if Instant::now() >= deadline {
                child.kill();
                self.emit(SidecarState::Crashed, Some(spec));
                bail!(
                    "llama-server did not become healthy within {:?}",
                    self.config.health_timeout
                );
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }

        self.emit(SidecarState::Ready, Some(spec));
        Ok(SidecarProcess {
            child,
            client,
            spec: spec.clone(),
        })
    }

    fn spawn_evictor(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(EVICT_TICK).await;
                if self.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                self.maybe_evict().await;
            }
        });
    }

    /// Stops the sidecar if it has been idle past the TTL, nothing is in flight, and the
    /// kernel's idle backfill is not asking us to stay warm.
    async fn maybe_evict(&self) {
        let elapsed = self
            .last_activity
            .lock()
            .map(|g| g.elapsed())
            .unwrap_or_default();
        if !should_evict(
            self.in_flight.load(Ordering::SeqCst),
            elapsed,
            self.config.idle_ttl,
            self.backfill_active.load(Ordering::SeqCst) || self.pinned.load(Ordering::SeqCst),
        ) {
            return;
        }
        let mut guard = self.state.lock().await;
        // Re-check *all* keep-warm signals under the lock: a request, an idle-backfill
        // keep-warm, or a manual pin may have arrived between the unlocked `should_evict`
        // check and here. Without re-checking `backfill_active`/`pinned`, a keep-warm set in
        // that window would still be evicted — reintroducing cold-start churn mid-drain.
        if self.in_flight.load(Ordering::SeqCst) > 0
            || self.backfill_active.load(Ordering::SeqCst)
            || self.pinned.load(Ordering::SeqCst)
        {
            return;
        }
        if let Some(p) = guard.take() {
            self.kill_and_confirm("idle TTL", p.child).await;
            self.emit(SidecarState::Evicted, Some(&p.spec));
            tracing::info!("sidecar evicted after idle TTL");
        }
    }

    /// Broadcasts a lifecycle transition. `spec` (the model this transition concerns)
    /// supplies both the human label and the lane, so the readiness panel can show *which*
    /// model is — or was last — resident. `None` only for the no-model-yet `Stopped`.
    fn emit(&self, state: SidecarState, spec: Option<&ModelSpec>) {
        let _ = self.events.send(SidecarStatus {
            state,
            model: spec.map(model_label),
            lane: spec.map(|s| s.lane),
        });
    }
}

/// Whether switching from `running` to `requested` requires a sidecar restart (a
/// different GGUF or projector). Same model → reuse the running process.
pub fn needs_restart(running: &ModelSpec, requested: &ModelSpec) -> bool {
    running.gguf_path != requested.gguf_path
        || running.mmproj_path != requested.mmproj_path
        || running.ngl != requested.ngl
        || running.device != requested.device
        || running.ctx_size != requested.ctx_size
        || running.kv_cache_type != requested.kv_cache_type
        || running.flash_attn != requested.flash_attn
}

/// Pure idle predicate (extracted for testing): idle once `elapsed >= ttl`.
pub fn idle_expired(elapsed: Duration, ttl: Duration) -> bool {
    elapsed >= ttl
}

/// Pure eviction predicate (extracted for testing): evict only when nothing is in flight,
/// the model has been idle past the TTL, **and** the kernel's idle backfill is not asking
/// us to stay warm. Keeping the model loaded during a backfill drain is the whole point of
/// the keep-warm flag (`03 §5/§6`).
pub fn should_evict(
    in_flight: usize,
    elapsed: Duration,
    ttl: Duration,
    backfill_active: bool,
) -> bool {
    in_flight == 0 && !backfill_active && idle_expired(elapsed, ttl)
}

/// Pure recycle predicate (extracted for testing, like `should_evict`): recycle once the
/// sidecar's committed RAM reaches the ceiling. `ceiling_bytes == 0` disables recycling.
pub fn should_recycle(rss_bytes: u64, ceiling_bytes: u64) -> bool {
    ceiling_bytes != 0 && rss_bytes >= ceiling_bytes
}

/// Auto recycle ceiling from total physical RAM: half of RAM, clamped to
/// `[8 GiB, max(RAM − 6 GiB, 8 GiB)]`. The **8 GiB floor always wins**, so the auto ceiling can
/// never sit below the vision model's ~6.8 GB warmup baseline — a lower ceiling (the 3–6 GiB the
/// old inverted-band fallback produced on 8–12 GiB machines) makes `over_recycle_ceiling` true
/// right after every reload, a silent recycle-every-frame loop that breaks tagging. On <14 GiB
/// machines the ceiling is therefore pinned at 8 GiB: recycle rarely/never fires and idle-TTL does
/// the bounding; such machines should use the Default 4B vision tier (`07` #72). Matches the
/// explicit floor in `kernel::settings::sanitize_settings`.
pub fn auto_recycle_ceiling(total_ram_bytes: u64) -> u64 {
    const GIB: u64 = 1 << 30;
    let lo = 8 * GIB;
    let hi = total_ram_bytes.saturating_sub(6 * GIB).max(lo);
    (total_ram_bytes / 2).clamp(lo, hi)
}

/// Resolve the configured recycle settings to a byte ceiling for `SupervisorConfig`:
/// `0` when disabled; an explicit MiB value; otherwise auto-derived from total RAM.
pub fn resolve_recycle_ceiling(enabled: bool, rss_mb: u32) -> u64 {
    if !enabled {
        return 0;
    }
    if rss_mb > 0 {
        return (rss_mb as u64) * 1024 * 1024;
    }
    auto_recycle_ceiling(process::total_physical_ram())
}

impl traits::BackfillControl for ModelSupervisor {
    fn set_backfill_active(&self, active: bool) {
        self.backfill_active.store(active, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl traits::PressureRelief for ModelSupervisor {
    /// Frees the resident model for the kernel's level-2 memory relief (`03 §5/§6`). Relief
    /// outranks the idle backfill's keep-warm — a drain must not hold RAM/VRAM while the
    /// machine is out of memory — but never an explicit user pin: a model the user asked to
    /// keep resident stays loaded and is reported as not released. `backfill_active` is
    /// deliberately left untouched, so the scheduler's later re-arm only suppresses future
    /// idle eviction and can never resurrect the model this call just released.
    async fn release_memory(&self) -> bool {
        // One path, one decision: `unload_reasoned` takes the request gate first and only then
        // consults the pin, so a preload that lands while relief is waiting cannot be killed.
        if !self.unload_reasoned("level-2 memory relief", true).await {
            return false;
        }
        tracing::info!("sidecar released on level-2 memory relief");
        true
    }
}

/// A running sidecar can be reused only when both the OS process and the HTTP health
/// endpoint are alive. Anything else is treated as a crash/hang and respawned.
pub fn can_reuse_running_sidecar(process_alive: bool, health_ok: bool) -> bool {
    process_alive && health_ok
}

async fn running_sidecar_healthy(running: &SidecarProcess) -> bool {
    let process_alive = process::pid_alive(running.child.pid());
    let health_ok = if process_alive {
        running.client.health().await
    } else {
        false
    };
    can_reuse_running_sidecar(process_alive, health_ok)
}

/// Reaps a stray sidecar a prior run left behind: reads `pidfile`, and only if that
/// pid is alive **and** its image path is our installed `expected_exe` does it
/// terminate the process (`03 §6` — never kill an unrelated process that recycled the
/// pid). Returns whether a process was killed. Stale/foreign pidfiles are cleaned up.
pub fn reap_stray(pidfile: &Path, expected_exe: &Path) -> bool {
    reap_stray_any(pidfile, &[expected_exe.to_path_buf()])
}

/// Like [`reap_stray`], but accepts every exact sidecar executable path the app owns
/// (the selected binary plus prior normal/override installs).
pub fn reap_stray_any(pidfile: &Path, expected_exes: &[PathBuf]) -> bool {
    let Ok(content) = std::fs::read_to_string(pidfile) else {
        return false;
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(pidfile);
        return false;
    };
    if !process::pid_alive(pid) {
        let _ = std::fs::remove_file(pidfile);
        return false;
    }
    let is_ours = process::image_path(pid)
        .is_some_and(|p| expected_exes.iter().any(|expected| paths_eq(&p, expected)));
    if is_ours {
        let killed = process::terminate(pid);
        let _ = std::fs::remove_file(pidfile);
        killed
    } else {
        // Pid recycled to an unrelated process — leave it and the pidfile alone.
        false
    }
}

/// Case-insensitive path comparison (Windows paths are case-insensitive).
fn paths_eq(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// The launch arguments for `llama-server` (`MODEL_REGISTRY §4`): model + host/port +
/// GPU offload, plus the same-repo projector on the vision lane, plus the memory-tuning
/// flags (`--ctx-size`, `--flash-attn`, `--cache-type-k/-v`) — but only those `caps`
/// reports the bundled binary actually accepts, so a future auto-updated build that
/// renames or drops a flag degrades gracefully instead of failing to spawn.
fn build_args(spec: &ModelSpec, port: u16, caps: &SidecarCaps) -> Vec<String> {
    let mut args = vec![
        "--model".to_string(),
        spec.gguf_path.to_string_lossy().into_owned(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "-ngl".to_string(),
        spec.ngl.to_string(),
    ];
    if let Some(mmproj) = &spec.mmproj_path {
        args.push("--mmproj".to_string());
        args.push(mmproj.to_string_lossy().into_owned());
    }
    if let Some(device) = &spec.device {
        args.push("--device".to_string());
        args.push(device.clone());
    }
    // Pin the context window (the dominant VRAM lever). `spec.ctx_size` is already the
    // resolved value (the auto sentinel was substituted in `resolve_spec`).
    if caps.ctx_size {
        args.push("--ctx-size".to_string());
        args.push(spec.ctx_size.to_string());
    }
    let flash = push_flash_attn(&mut args, caps.flash_attn_kind, spec.flash_attn);
    push_kv_cache(&mut args, caps, spec.kv_cache_type, flash);
    args
}

/// Whether flash attention ends up active and, when it doesn't, *why* — so a quantized-KV
/// downgrade can be diagnosed accurately (a binary limitation vs. the user's own choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashState {
    /// Flash attention will be on — quantized KV is safe to emit.
    Active,
    /// The binary has no `--flash-attn` flag, so it can't be enabled.
    BinaryUnsupported,
    /// The binary supports it, but the user set `FlashAttnSetting::Off`.
    UserDisabled,
}

/// Appends the `--flash-attn` flag in whatever spelling the binary accepts and reports the
/// resulting [`FlashState`] (which gates KV-cache quantization).
///
/// On a value-taking binary the three settings map to distinct args: `Auto` emits
/// `--flash-attn auto` (defer to llama.cpp's own readiness check), `On` emits
/// `--flash-attn on` (force it), and `Off` emits `--flash-attn off`. `auto` resolves to
/// on for every build that advertises the flag, so it still counts as active for the
/// purpose of unlocking quantized KV. A legacy bare-switch binary has no `auto` spelling,
/// so `Auto`/`On` both append the bare flag. When the binary lacks the flag entirely an
/// explicit `On` is warned about (the user asked for something unavailable) rather than
/// dropped silently.
fn push_flash_attn(
    args: &mut Vec<String>,
    kind: FlashAttnKind,
    setting: FlashAttnSetting,
) -> FlashState {
    match (kind, setting) {
        (FlashAttnKind::Unsupported, FlashAttnSetting::On) => {
            tracing::warn!(
                "flash attention was explicitly enabled in settings, but this llama-server \
                 build does not support --flash-attn; the setting is ignored"
            );
            FlashState::BinaryUnsupported
        }
        (FlashAttnKind::Unsupported, _) => FlashState::BinaryUnsupported,
        (FlashAttnKind::BoolFlag, FlashAttnSetting::Off) => FlashState::UserDisabled,
        (FlashAttnKind::BoolFlag, _) => {
            args.push("--flash-attn".to_string());
            FlashState::Active
        }
        (FlashAttnKind::EnumOnOffAuto, FlashAttnSetting::Off) => {
            args.push("--flash-attn".to_string());
            args.push("off".to_string());
            FlashState::UserDisabled
        }
        (FlashAttnKind::EnumOnOffAuto, FlashAttnSetting::Auto) => {
            args.push("--flash-attn".to_string());
            args.push("auto".to_string());
            FlashState::Active
        }
        (FlashAttnKind::EnumOnOffAuto, FlashAttnSetting::On) => {
            args.push("--flash-attn".to_string());
            args.push("on".to_string());
            FlashState::Active
        }
    }
}

/// Appends `--cache-type-k`/`--cache-type-v` only for a quantized type **and** only when
/// flash attention is active (quantized KV requires it). `f16` (the llama.cpp default) is
/// left implicit, and a quantized request without flash attention degrades to that
/// default with a warning — phrased to match *why* flash is off — rather than producing an
/// arg list the binary would reject.
fn push_kv_cache(args: &mut Vec<String>, caps: &SidecarCaps, kv: KvCacheType, flash: FlashState) {
    if !kv.is_quantized() {
        return; // f16 is the default; nothing to pass.
    }
    match flash {
        FlashState::Active => {}
        FlashState::BinaryUnsupported => {
            tracing::warn!(
                "KV-cache quantization requires flash attention, which this llama-server \
                 build does not support; using the default f16 KV cache"
            );
            return;
        }
        FlashState::UserDisabled => {
            tracing::warn!(
                "KV-cache quantization requires flash attention, which is turned off in \
                 settings; using the default f16 KV cache"
            );
            return;
        }
    }
    if !caps.cache_type_k && !caps.cache_type_v {
        tracing::warn!(
            "KV-cache quantization is configured, but this llama-server build advertises \
             neither --cache-type-k nor --cache-type-v; using the default f16 KV cache"
        );
        return;
    }
    if caps.cache_type_k {
        args.push("--cache-type-k".to_string());
        args.push(kv.as_arg().to_string());
    }
    if caps.cache_type_v {
        args.push("--cache-type-v".to_string());
        args.push(kv.as_arg().to_string());
    }
}

/// A short label (the GGUF filename) for status events.
fn model_label(spec: &ModelSpec) -> String {
    spec.gguf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("model")
        .to_string()
}

/// Allocates a free ephemeral TCP port on the loopback for the sidecar to bind.
fn free_port() -> Result<u16> {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").context("bind ephemeral loopback port")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ModelLane, ModelTier};
    use std::future::Future;
    use std::task::Context as TaskContext;
    use traits::BackfillControl;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn spec(gguf: &str, mmproj: Option<&str>) -> ModelSpec {
        ModelSpec {
            lane: if mmproj.is_some() {
                ModelLane::Vision
            } else {
                ModelLane::Answer
            },
            tier: ModelTier::Default,
            gguf_path: PathBuf::from(gguf),
            mmproj_path: mmproj.map(PathBuf::from),
            ngl: 99,
            device: None,
            ctx_size: 4096,
            kv_cache_type: KvCacheType::Q8_0,
            flash_attn: FlashAttnSetting::Auto,
        }
    }

    /// Caps for a modern binary that advertises every memory-tuning flag with a
    /// value-taking `--flash-attn`.
    fn caps_full() -> SidecarCaps {
        SidecarCaps {
            ctx_size: true,
            cache_type_k: true,
            cache_type_v: true,
            flash_attn_kind: FlashAttnKind::EnumOnOffAuto,
        }
    }

    /// Returns the value following `flag` in an arg list, if present.
    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|w| w[0] == flag)
            .map(|w| w[1].as_str())
    }

    /// A scratch directory unique to this test and process — never a fixed, globally shared
    /// temp path. It is created lazily by the sidecar log open, so concurrent test runs cannot
    /// collide.
    fn unique_scratch_dir(tag: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "ssv2c-supervisor-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// Config for a supervisor whose `llama-server.exe` is only ever *recorded*, never
    /// launched: every test below injects a resident stand-in child instead. The pidfile lives
    /// in the scratch dir, so the startup reap is a no-op.
    fn config_in(dir: &Path) -> SupervisorConfig {
        SupervisorConfig {
            binary: dir.join("llama-server.exe"),
            reap_binaries: Vec::new(),
            pidfile: dir.join("llama-server.pid"),
            sidecar_log: Some(dir.join("llama-server.log")),
            idle_ttl: Duration::from_secs(180),
            health_timeout: Duration::from_secs(1),
            caps: caps_full(),
            recycle_ceiling_bytes: 0,
        }
    }

    /// A stand-in for a resident sidecar: a real, harmless Windows child (`ping`, the same
    /// fixture the `reap` integration test uses) spawned **suspended**, bound to the
    /// supervisor's own Job Object, then resumed — the exact `spawn_for` discipline. No model
    /// is loaded, nothing is downloaded, and no `llama-server` is launched; the point is a live
    /// OS process the teardown path can really kill.
    fn spawn_stand_in(supervisor: &ModelSupervisor, dir: &Path) -> process::SuspendedChild {
        let program = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32").join("ping.exe"))
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\ping.exe"));
        let log = dir.join("stand-in.log");
        let args = ["-n", "120", "127.0.0.1"].map(String::from);
        let child = process::spawn_suspended(&program, &args, Some(&log))
            .expect("spawn the ping stand-in suspended");
        if let Err(e) = supervisor.job.assign(child.process_handle()) {
            // Never leave a suspended child behind, exactly like `spawn_for`.
            child.kill();
            panic!("bind the stand-in to the supervisor's job object: {e:#}");
        }
        child.resume().expect("resume the bound stand-in");
        child
    }

    /// A supervisor with one resident stand-in sidecar, plus that sidecar's spec and pid.
    /// Only the *fixture* is injected — the relief, unload and gate paths under test are the
    /// production ones, operating on a real process bound to the real Job Object.
    async fn resident_supervisor(
        tag: &str,
        client: SidecarClient,
    ) -> (Arc<ModelSupervisor>, ModelSpec, u32, PathBuf) {
        let dir = unique_scratch_dir(tag);
        let supervisor = ModelSupervisor::new(config_in(&dir)).expect("build the supervisor");
        let model = spec(r"C:\m\stand-in.gguf", None);
        let child = spawn_stand_in(&supervisor, &dir);
        let pid = child.pid();
        *supervisor.state.lock().await = Some(SidecarProcess {
            child,
            client,
            spec: model.clone(),
        });
        (supervisor, model, pid, dir)
    }

    /// Bounded safety net after the production teardown has already waited: a slow kernel
    /// cannot flake the suite, but a process that really survived is a failure.
    async fn assert_dead(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while process::pid_alive(pid) {
            assert!(Instant::now() < deadline, "process {pid} survived teardown");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Test teardown: stop anything still resident (the pin test deliberately keeps its child
    /// alive for its assertions), then remove the scratch dir.
    async fn retire(supervisor: Arc<ModelSupervisor>, dir: PathBuf) {
        supervisor.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn restart_only_on_model_change() {
        let a = spec(r"C:\m\a.gguf", None);
        let a2 = spec(r"C:\m\a.gguf", None);
        let b = spec(r"C:\m\b.gguf", None);
        assert!(!needs_restart(&a, &a2), "same gguf → reuse");
        assert!(needs_restart(&a, &b), "different gguf → restart");

        let v1 = spec(r"C:\m\v.gguf", Some(r"C:\m\mmproj1.gguf"));
        let v2 = spec(r"C:\m\v.gguf", Some(r"C:\m\mmproj2.gguf"));
        assert!(needs_restart(&v1, &v2), "different projector → restart");
    }

    #[test]
    fn idle_predicate() {
        assert!(idle_expired(
            Duration::from_secs(200),
            Duration::from_secs(180)
        ));
        assert!(!idle_expired(
            Duration::from_secs(10),
            Duration::from_secs(180)
        ));
    }

    #[test]
    fn evict_predicate_respects_inflight_backfill_and_ttl() {
        let ttl = Duration::from_secs(180);
        let past = Duration::from_secs(200);
        let recent = Duration::from_secs(10);
        // Idle past TTL, nothing in flight, no backfill → evict.
        assert!(should_evict(0, past, ttl, false));
        // A request in flight → never evict.
        assert!(!should_evict(1, past, ttl, false));
        // Backfill draining the backlog → keep warm even when idle past TTL.
        assert!(!should_evict(0, past, ttl, true));
        // Not idle long enough → don't evict.
        assert!(!should_evict(0, recent, ttl, false));
    }

    #[tokio::test]
    async fn relief_releases_a_resident_model_even_under_keep_warm() {
        // Level-2 contract (`03 §5/§6`) against a real OS process: relief outranks the
        // backfill's keep-warm and the resident child (the RAM/VRAM stand-in) actually dies.
        let client = SidecarClient::new("http://127.0.0.1:1");
        let (supervisor, _, pid, dir) = resident_supervisor("relief-keepwarm", client).await;
        supervisor.set_backfill_active(true);
        let relief: Arc<dyn traits::PressureRelief> = supervisor.clone();

        assert!(
            relief.release_memory().await,
            "relief must report releasing the resident child"
        );
        assert_dead(pid).await;
        assert!(
            supervisor.state.lock().await.is_none(),
            "no sidecar may remain resident after relief"
        );
        assert!(!supervisor.pinned.load(Ordering::SeqCst));
        assert!(
            supervisor.backfill_active.load(Ordering::SeqCst),
            "relief must not disarm the scheduler's keep-warm"
        );

        // Keep-warm only suppresses idle eviction; it cannot spawn. A later evictor pass on the
        // released supervisor therefore leaves it empty ("cannot resurrect"), and a second
        // relief reports a no-op without touching keep-warm.
        supervisor.maybe_evict().await;
        assert!(supervisor.state.lock().await.is_none());
        assert!(
            !relief.release_memory().await,
            "nothing resident → level-2 relief reports no release"
        );
        assert!(supervisor.backfill_active.load(Ordering::SeqCst));

        retire(supervisor, dir).await;
    }

    #[tokio::test]
    async fn preload_pins_the_resident_model_and_relief_skips_it() {
        // Rule A through the production `preload` path: the pin is published by `preload`
        // itself while it still holds its request-gate permit, so relief — which had to drain
        // that gate before reading the pin — observes it and leaves the child running.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let client = SidecarClient::new(server.uri());
        let (supervisor, model, pid, dir) = resident_supervisor("preload-pin", client).await;

        supervisor
            .preload(model)
            .await
            .expect("preload reuses the healthy resident stand-in");
        assert!(
            supervisor.pinned.load(Ordering::SeqCst),
            "an explicit load publishes the user's pin"
        );

        let relief: Arc<dyn traits::PressureRelief> = supervisor.clone();
        assert!(
            !relief.release_memory().await,
            "an explicitly pinned model is never released by relief"
        );
        assert!(
            process::pid_alive(pid),
            "the pinned resident child must still be running"
        );
        assert!(
            supervisor.state.lock().await.is_some(),
            "the pinned model stays resident"
        );
        assert!(
            supervisor.pinned.load(Ordering::SeqCst),
            "relief never clears the pin: only an explicit unload does that"
        );

        retire(supervisor, dir).await;
    }

    #[tokio::test]
    async fn manual_unload_releases_the_pinned_model_and_clears_the_pin() {
        // The other half of rule A: the user's own Unload always means unload, so it clears the
        // pin and kills the resident child that relief had to leave alone.
        let client = SidecarClient::new("http://127.0.0.1:1");
        let (supervisor, _, pid, dir) = resident_supervisor("manual-unload", client).await;
        supervisor.pinned.store(true, Ordering::SeqCst);
        let relief: Arc<dyn traits::PressureRelief> = supervisor.clone();
        assert!(!relief.release_memory().await);
        assert!(
            process::pid_alive(pid),
            "relief cannot free a user-pinned model"
        );

        supervisor.unload().await;
        assert_dead(pid).await;
        assert!(supervisor.state.lock().await.is_none());
        assert!(
            !supervisor.pinned.load(Ordering::SeqCst),
            "an explicit unload clears the pin so later relief can act"
        );
        assert!(
            !relief.release_memory().await,
            "after Unload there is nothing left to release"
        );

        retire(supervisor, dir).await;
    }

    /// Deterministic regression for the pin/gate ordering: an unload that has not yet won the
    /// exclusive gate must not have cleared the pin. Clearing it earlier let a concurrent
    /// preload publish the pin between the clear and the kill, leaving a stale pin on a
    /// supervisor with no resident model. The future is polled exactly once with a no-op waker,
    /// so it runs to the gate's first await without depending on scheduler timing.
    #[tokio::test]
    async fn unload_clears_the_pin_only_after_it_owns_the_gate() {
        let client = SidecarClient::new("http://127.0.0.1:1");
        let (supervisor, _, pid, dir) = resident_supervisor("gate-order", client).await;
        supervisor.pinned.store(true, Ordering::SeqCst);

        // Stand-in for a preload that still owns its lease permit — the window in which
        // `preload` publishes the pin.
        let lease = supervisor
            .gate
            .enter()
            .await
            .expect("take one request permit");

        let mut unload = Box::pin(supervisor.unload());
        let waker = futures::task::noop_waker();
        let mut cx = TaskContext::from_waker(&waker);
        assert!(
            unload.as_mut().poll(&mut cx).is_pending(),
            "the manual unload must block on the exclusive gate"
        );
        assert!(
            supervisor.pinned.load(Ordering::SeqCst),
            "an unload that does not yet own the gate must not have cleared the pin"
        );

        // The concurrent preload publishes its pin and releases the lease; the unload then wins
        // the gate and must serialise its clear against that publication.
        drop(lease);
        unload.await;

        assert!(
            !supervisor.pinned.load(Ordering::SeqCst),
            "the unload that owned the gate clears the pin"
        );
        assert_dead(pid).await;
        assert!(supervisor.state.lock().await.is_none());

        retire(supervisor, dir).await;
    }

    /// Deterministic companion: relief makes no decision while a load still holds its lease,
    /// and once the load publishes the pin under that lease, relief must observe it. This is
    /// the guard against hoisting the pin read above the gate again.
    #[tokio::test]
    async fn relief_waits_for_a_loads_lease_before_deciding() {
        let client = SidecarClient::new("http://127.0.0.1:1");
        let (supervisor, _, pid, dir) = resident_supervisor("relief-lease", client).await;
        let relief: Arc<dyn traits::PressureRelief> = supervisor.clone();

        // A load's lease permit: `preload` publishes the pin before dropping exactly this.
        let lease = supervisor
            .gate
            .enter()
            .await
            .expect("take one request permit");
        let mut release = relief.release_memory();
        let waker = futures::task::noop_waker();
        let mut cx = TaskContext::from_waker(&waker);
        assert!(
            release.as_mut().poll(&mut cx).is_pending(),
            "relief must wait for the load's lease"
        );
        assert!(
            process::pid_alive(pid),
            "relief must not kill while a load holds its lease"
        );

        // The load publishes its pin (as `preload` does) and then drops the lease.
        supervisor.pinned.store(true, Ordering::SeqCst);
        drop(lease);
        assert!(
            !release.await,
            "relief must observe the pin the load published while it waited"
        );
        assert!(
            process::pid_alive(pid),
            "the pinned resident child survives relief"
        );

        retire(supervisor, dir).await;
    }

    #[tokio::test]
    async fn request_gate_allows_concurrent_regular_requests() {
        let gate = RequestGate::with_capacity(2);
        let first = gate.enter().await.expect("first request enters");
        let second = gate.enter().await.expect("second request enters");

        let gate_for_third = gate.clone();
        let third_wait = tokio::spawn(async move {
            let _third = gate_for_third
                .enter()
                .await
                .expect("third enters once a request drops");
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !third_wait.is_finished(),
            "only capacity, not one, should limit regular request concurrency"
        );

        drop(first);
        tokio::time::timeout(Duration::from_secs(1), third_wait)
            .await
            .expect("third request should proceed after one slot frees")
            .expect("third request task should not panic");
        drop(second);
    }

    #[tokio::test]
    async fn switch_gate_waits_for_active_request_to_drop() {
        let gate = RequestGate::with_capacity(2);
        let first = gate.enter().await.expect("first request enters");
        let second = gate.enter().await.expect("second request enters");

        let gate_for_switch = gate.clone();
        let switch_wait = tokio::spawn(async move {
            let switch = gate_for_switch
                .enter_for_model_switch()
                .await
                .expect("switch enters once first request drops");
            assert_eq!(
                switch._permit.num_permits(),
                2,
                "switch should drain every request permit"
            );
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !switch_wait.is_finished(),
            "model switch must wait while another request lease is active"
        );

        drop(first);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !switch_wait.is_finished(),
            "model switch must wait for every active request lease"
        );

        drop(second);
        tokio::time::timeout(Duration::from_secs(1), switch_wait)
            .await
            .expect("switch should proceed after active request drops")
            .expect("switch task should not panic");
    }

    #[test]
    fn running_sidecar_is_reused_only_when_process_and_health_are_alive() {
        assert!(can_reuse_running_sidecar(true, true));
        assert!(!can_reuse_running_sidecar(false, true));
        assert!(!can_reuse_running_sidecar(true, false));
        assert!(!can_reuse_running_sidecar(false, false));
    }

    #[test]
    fn restart_on_tuning_change() {
        let base = spec(r"C:\m\a.gguf", None);

        let mut ctx = base.clone();
        ctx.ctx_size = 2048;
        assert!(needs_restart(&base, &ctx), "ctx_size change → restart");

        let mut kv = base.clone();
        kv.kv_cache_type = KvCacheType::F16;
        assert!(needs_restart(&base, &kv), "kv_cache_type change → restart");

        let mut fa = base.clone();
        fa.flash_attn = FlashAttnSetting::Off;
        assert!(needs_restart(&base, &fa), "flash_attn change → restart");
    }

    #[test]
    fn build_args_adds_mmproj_only_for_vision() {
        let answer = build_args(&spec(r"C:\m\a.gguf", None), 8080, &caps_full());
        assert!(!answer.iter().any(|a| a == "--mmproj"));
        assert!(answer
            .windows(2)
            .any(|w| w[0] == "--port" && w[1] == "8080"));

        let vision = build_args(
            &spec(r"C:\m\v.gguf", Some(r"C:\m\mmproj.gguf")),
            8080,
            &caps_full(),
        );
        assert!(vision
            .windows(2)
            .any(|w| w[0] == "--mmproj" && w[1] == r"C:\m\mmproj.gguf"));
    }

    #[test]
    fn build_args_adds_device_when_configured() {
        let mut answer = spec(r"C:\m\a.gguf", None);
        answer.device = Some("Vulkan0".to_string());

        let args = build_args(&answer, 8080, &caps_full());

        assert!(args
            .windows(2)
            .any(|w| w[0] == "--device" && w[1] == "Vulkan0"));
    }

    #[test]
    fn build_args_emits_full_tuning_when_supported() {
        // Modern binary + quantized KV + flash Auto → all four flags. Auto defers to
        // llama.cpp (`--flash-attn auto`), which still counts as active for quantized KV.
        let mut s = spec(r"C:\m\a.gguf", None);
        s.ctx_size = 8192;
        let args = build_args(&s, 8080, &caps_full());
        assert_eq!(value_after(&args, "--ctx-size"), Some("8192"));
        assert_eq!(value_after(&args, "--flash-attn"), Some("auto"));
        assert_eq!(value_after(&args, "--cache-type-k"), Some("q8_0"));
        assert_eq!(value_after(&args, "--cache-type-v"), Some("q8_0"));
    }

    #[test]
    fn build_args_distinguishes_auto_from_on_flash_attn() {
        // On a value-taking binary, `Auto` and `On` must produce distinct args so the
        // setting is observable: Auto → `auto` (defer), On → `on` (force).
        let mut auto = spec(r"C:\m\a.gguf", None);
        auto.flash_attn = FlashAttnSetting::Auto;
        assert_eq!(
            value_after(&build_args(&auto, 8080, &caps_full()), "--flash-attn"),
            Some("auto")
        );

        let mut on = spec(r"C:\m\a.gguf", None);
        on.flash_attn = FlashAttnSetting::On;
        let on_args = build_args(&on, 8080, &caps_full());
        assert_eq!(value_after(&on_args, "--flash-attn"), Some("on"));
        // Forcing flash on still unlocks quantized KV.
        assert_eq!(value_after(&on_args, "--cache-type-v"), Some("q8_0"));
    }

    #[test]
    fn build_args_uses_bare_flash_attn_for_bool_flag() {
        // Legacy binary spells --flash-attn as a bare switch (no value), and flash is
        // active, so quantized KV is still emitted.
        let caps = SidecarCaps {
            flash_attn_kind: FlashAttnKind::BoolFlag,
            ..caps_full()
        };
        let args = build_args(&spec(r"C:\m\a.gguf", None), 8080, &caps);
        assert!(args.iter().any(|a| a == "--flash-attn"));
        // The token after --flash-attn must NOT be a value (it's the next real flag).
        assert_ne!(value_after(&args, "--flash-attn"), Some("on"));
        assert_eq!(value_after(&args, "--cache-type-k"), Some("q8_0"));
    }

    #[test]
    fn build_args_omits_kv_quant_without_flash() {
        // No flash support → quantized KV would be rejected, so it is dropped (f16
        // default), and no --flash-attn is emitted. Context is still pinned.
        let caps = SidecarCaps {
            flash_attn_kind: FlashAttnKind::Unsupported,
            ..caps_full()
        };
        let args = build_args(&spec(r"C:\m\a.gguf", None), 8080, &caps);
        assert!(!args.iter().any(|a| a == "--flash-attn"));
        assert!(!args.iter().any(|a| a == "--cache-type-k"));
        assert!(!args.iter().any(|a| a == "--cache-type-v"));
        assert_eq!(value_after(&args, "--ctx-size"), Some("4096"));
    }

    #[test]
    fn build_args_drops_explicit_on_flash_when_binary_unsupported() {
        // User forces flash On but the binary has no flag: it must not appear, quantized KV
        // is dropped to f16, and context is still pinned. (The arm also logs a warn so the
        // ignored setting is diagnosable; the args are what we assert here.)
        let caps = SidecarCaps {
            flash_attn_kind: FlashAttnKind::Unsupported,
            ..caps_full()
        };
        let mut s = spec(r"C:\m\a.gguf", None);
        s.flash_attn = FlashAttnSetting::On;
        let args = build_args(&s, 8080, &caps);
        assert!(!args.iter().any(|a| a == "--flash-attn"));
        assert!(!args.iter().any(|a| a == "--cache-type-k"));
        assert!(!args.iter().any(|a| a == "--cache-type-v"));
        assert_eq!(value_after(&args, "--ctx-size"), Some("4096"));
    }

    #[test]
    fn build_args_omits_ctx_when_unsupported() {
        let caps = SidecarCaps {
            ctx_size: false,
            ..caps_full()
        };
        let args = build_args(&spec(r"C:\m\a.gguf", None), 8080, &caps);
        assert!(!args.iter().any(|a| a == "--ctx-size"));
    }

    #[test]
    fn build_args_leaves_f16_kv_implicit() {
        // f16 is the llama.cpp default, so no --cache-type-* is passed even with full caps.
        let mut s = spec(r"C:\m\a.gguf", None);
        s.kv_cache_type = KvCacheType::F16;
        let args = build_args(&s, 8080, &caps_full());
        assert!(!args.iter().any(|a| a == "--cache-type-k"));
        assert!(!args.iter().any(|a| a == "--cache-type-v"));
    }

    const GIB: u64 = 1 << 30;

    #[test]
    fn should_recycle_only_at_or_above_ceiling() {
        assert!(!super::should_recycle(9 * GIB, 10 * GIB));
        assert!(super::should_recycle(10 * GIB, 10 * GIB));
        assert!(super::should_recycle(11 * GIB, 10 * GIB));
    }

    #[test]
    fn should_recycle_disabled_when_ceiling_zero() {
        assert!(!super::should_recycle(64 * GIB, 0));
    }

    #[test]
    fn auto_ceiling_is_half_ram_within_band() {
        // The 8 GiB floor always wins, so the auto ceiling never drops below the 8B vision model's
        // ~6.8 GB warmup baseline — no recycle-every-frame loop. On <14 GiB it pins at 8 GiB
        // (recycle effectively idles; idle-TTL bounds RAM; such machines want the 4B tier).
        assert_eq!(super::auto_recycle_ceiling(32 * GIB), 16 * GIB); // big box: RAM/2
        assert_eq!(super::auto_recycle_ceiling(16 * GIB), 8 * GIB); // RAM/2 == floor
        assert_eq!(super::auto_recycle_ceiling(14 * GIB), 8 * GIB); // boundary: RAM-6 == floor
        assert_eq!(super::auto_recycle_ceiling(12 * GIB), 8 * GIB); // small box: pinned to floor
        assert_eq!(super::auto_recycle_ceiling(8 * GIB), 8 * GIB); // tiny: floored, no panic
    }

    #[test]
    fn resolve_ceiling_branches() {
        assert_eq!(super::resolve_recycle_ceiling(false, 0), 0); // disabled
        assert_eq!(super::resolve_recycle_ceiling(false, 9000), 0); // disabled wins
        assert_eq!(
            super::resolve_recycle_ceiling(true, 9000),
            9000 * 1024 * 1024
        ); // explicit MiB
        assert!(super::resolve_recycle_ceiling(true, 0) > 0); // auto from real RAM
    }
}
