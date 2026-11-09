//! `embeddings` — [`EmbeddingProvider`] via fastembed (in-process ONNX, **no
//! Python**, `01 §5`, `03 §3`).
//!
//! Text: EmbeddingGemma-300M (768-dim, quantized) — *cannot batch*, so the impl
//! embeds one input at a time (`01 §6`, `MODEL_REGISTRY §3/§5`). Text is the only
//! embedding lane; the optional image lane was removed in 0.3.0 PR4
//! (`docs/0.3.0.md`, `MODEL_REGISTRY §3`).
//!
//! ## Concurrency
//! fastembed's `TextEmbedding` is a plain `Send` ONNX handle with no thread affinity
//! (unlike the COM-STA-bound WinRT OCR), but `embed` takes `&mut self` and blocks the
//! CPU. The lane is therefore an `Arc<Mutex<…>>` whose lock is taken *inside* a
//! [`tokio::task::spawn_blocking`] closure — never across an `.await` (same discipline
//! as `store::with_conn`, and the same rule [`LazyEmbedder`] follows for its own
//! `std`-sync slot locks).
//!
//! ## Residency (`03 §5`/`§6`, `03 §8`)
//! A resident EmbeddingGemma session holds ~1.2 GiB of private bytes while completely idle,
//! and the app keeps the embedder alive for its whole life (kernel slot + store handle), so
//! the model can never be freed by dropping the provider. [`LazyEmbedder`] therefore owns the
//! residency policy: the session loads on first use — or eagerly via
//! [`LazyEmbedder::load_now`], so the first search stays fast — and is released again after
//! `enrich.embed_idle_ttl_secs` of no embedding. [`FastEmbedProvider`] stays the concrete model.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use tokio::sync::Mutex as TokioMutex;
use traits::{Embedding, EmbeddingProvider};

/// Vector dimensionality the text model produces, and the `vec0` schema's `FLOAT[768]`
/// (`03 §4`). Mirrors `store::EMBEDDING_DIM` without a cross-crate dep (the store
/// validates length on upsert).
pub const EMBED_DIM: usize = 768;

/// Provenance label written to `embeddings.model` (`03 §4`).
const TEXT_MODEL_NAME: &str = "embeddinggemma-300m-q";

/// Idle-sweep cadence for [`LazyEmbedder::spawn_idle_unloader`]. The TTL is minutes, so a
/// 30 s tick bounds the overshoot without the loop costing anything.
const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// One concrete text-embedding model behind a [`LazyEmbedder`].
///
/// Split out from [`FastEmbedProvider`] so the residency policy (load once, unload when idle)
/// is testable without an ONNX session or a network download: tests implement this trait and
/// [`LazyEmbedder`] never touches fastembed directly.
pub trait EmbedModel: Send + Sync + 'static {
    /// Loads the model, downloading into `cache_dir` on first use. **Blocking** — the caller
    /// runs it inside [`tokio::task::spawn_blocking`].
    fn load(cache_dir: PathBuf) -> Result<Self>
    where
        Self: Sized;

    /// Embeds every input, returning one vector per input in the same order. **Blocking**
    /// (CPU-bound) — the caller runs it inside [`tokio::task::spawn_blocking`].
    fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// In-process fastembed provider wrapping the text embedding lane. Satisfies the
/// [`EmbeddingProvider`] contract (`03 §3`).
pub struct FastEmbedProvider {
    text: Arc<Mutex<TextEmbedding>>,
}

impl FastEmbedProvider {
    /// Eagerly loads the text model, downloading to `cache_dir` on first use.
    /// **Blocking** — invoke inside `spawn_blocking` so the launch thread isn't held on
    /// a multi-hundred-MB download (`03 §5`).
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let text = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::EmbeddingGemma300MQ)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false),
        )
        .map_err(|e| anyhow!("failed to load text embedding model: {e}"))?;

        tracing::info!("fastembed provider loaded");
        Ok(Self {
            text: Arc::new(Mutex::new(text)),
        })
    }
}

/// The model loop, shared by [`EmbedModel::embed`] and [`EmbeddingProvider::embed_texts`] so
/// both entry points batch identically. Callers hold the ONNX lock **inside**
/// `spawn_blocking`, never across an `.await` (see the `## Concurrency` note above).
fn embed_one_by_one(text: &Mutex<TextEmbedding>, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
    let mut guard = text
        .lock()
        .map_err(|_| anyhow!("text embed model lock poisoned"))?;
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        // One input per call: the quantized EmbeddingGemma model must not
        // batch (MODEL_REGISTRY §5). The hot path embeds a single query.
        let mut batch = guard
            .embed(vec![input.as_str()], None)
            .map_err(|e| anyhow!("text embedding failed: {e}"))?;
        out.push(
            batch
                .pop()
                .ok_or_else(|| anyhow!("text embedder returned no vector"))?,
        );
    }
    Ok(out)
}

impl EmbedModel for FastEmbedProvider {
    fn load(cache_dir: PathBuf) -> Result<Self> {
        Self::new(cache_dir)
    }

    fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        embed_one_by_one(&self.text, inputs)
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    fn dim(&self) -> usize {
        EMBED_DIM
    }

    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let text = Arc::clone(&self.text);
        let inputs = inputs.to_vec();
        let vectors = tokio::task::spawn_blocking(move || embed_one_by_one(&text, &inputs))
            .await
            .map_err(|e| anyhow!("embed_texts task failed: {e}"))??;

        Ok(vectors.into_iter().map(Embedding).collect())
    }

    fn text_model_name(&self) -> &str {
        TEXT_MODEL_NAME
    }
}

/// Idle-unloading holder for the text embedder (`03 §5`/`§6`, `03 §8`).
///
/// [`new`](Self::new) does not load the model: it allocates only the empty coordination state,
/// so the session loads on the first [`embed_texts`](EmbeddingProvider::embed_texts) — single
/// flight, so `N` concurrent callers cause exactly one load, and a caller cancelled mid-load
/// still leaves that one session published — or eagerly through [`load_now`](Self::load_now).
/// Once `idle_ttl` passes with no embedding, [`spawn_idle_unloader`](Self::spawn_idle_unloader)
/// releases the session and the next embed reloads it. `idle_ttl` of [`Duration::ZERO`] disables
/// unloading.
///
/// Releases are as careful as loads: a sweep (or an explicit [`unload`](Self::unload)) only
/// frees a session that no caller holds a clone of, so an embed that is still running — even one
/// that outlives the TTL — is never migrated onto a second ONNX session. It finishes on its own
/// handle and the next sweep gives the memory back; only the *next* call pays for a reload.
pub struct LazyEmbedder<M: EmbedModel = FastEmbedProvider> {
    cache_dir: PathBuf,
    idle_ttl: Duration,
    /// Resident model, if any. Read only long enough to clone the [`Arc`]; a caller that holds
    /// a clone is using the session, which every release path treats as "not evictable". The
    /// outer [`Arc`] lets a blocking load publish without a caller future to hand it to.
    model: Arc<RwLock<Option<Arc<M>>>>,
    /// Single-flight gate: whoever wins loads once, everyone else waits and then finds the
    /// model resident (`03 §5` — a burst of embeds must not start parallel ONNX sessions). The
    /// winner moves the guard *into the blocking load*, so a cancelled caller cannot release
    /// single flight while `M::load` is still allocating its ~1 GiB session.
    load_gate: Arc<TokioMutex<()>>,
    /// When the model was last loaded or used; the idle sweep compares against `idle_ttl`.
    /// Shared for the same reason as the slot: the load stamps it where the load completes.
    last_used: Arc<Mutex<Instant>>,
}

/// Recovers a guard from a poisoned lock. The slot and the idle clock hold plain data, so a
/// panic elsewhere cannot leave them inconsistent — failing an embed over it would be worse
/// than carrying on. (`FastEmbedProvider`'s ONNX lock still errors on poison: that state *is*
/// the model.)
fn recover<T>(poisoned: PoisonError<T>) -> T {
    poisoned.into_inner()
}

impl<M: EmbedModel> LazyEmbedder<M> {
    /// Creates the holder **without loading the model** — nothing is read from `cache_dir`
    /// until the first embed or [`load_now`](Self::load_now). Allocates only the shared
    /// coordination state (empty slot, load gate, idle clock). `idle_ttl` is how long a resident
    /// model may sit unused before [`spawn_idle_unloader`](Self::spawn_idle_unloader) frees it;
    /// [`Duration::ZERO`] means never unload.
    pub fn new(cache_dir: PathBuf, idle_ttl: Duration) -> Self {
        Self {
            cache_dir,
            idle_ttl,
            model: Arc::new(RwLock::new(None)),
            load_gate: Arc::new(TokioMutex::new(())),
            last_used: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Loads the model now so the first real embed doesn't pay the download/init cost
    /// (startup path, `03 §6`). Idempotent: a resident model is returned as-is.
    pub async fn load_now(&self) -> Result<()> {
        self.acquire().await.map(|_| ())
    }

    /// Frees the resident model, returning `true` when one was actually released and `false`
    /// when nothing was resident or a caller is still using the session (see
    /// `take_evictable`). The load gate is held across the teardown, so a reload cannot start a
    /// second ONNX session while the old one's ~1 GiB is still being freed.
    pub async fn unload(&self) -> bool {
        let released = self.evict(None, Instant::now()).await;
        if released {
            tracing::info!("fastembed provider unloaded");
        }
        released
    }

    /// Starts the idle sweep: every 30 s (see `IDLE_POLL_INTERVAL`) it releases the model once
    /// `idle_ttl` has elapsed since the last use. The tick re-checks residency, the clock, and
    /// whether any caller holds the session together under the load gate, so a use that started
    /// after the last tick is never evicted on a stale read. Does nothing when `idle_ttl` is zero
    /// (never unload). The task holds a [`Weak`](std::sync::Weak) reference, so it ends with the
    /// holder and needs no lifecycle plumbing into the kernel.
    pub fn spawn_idle_unloader(self: &Arc<Self>) {
        if self.idle_ttl.is_zero() {
            return;
        }
        let ttl = self.idle_ttl;
        tracing::info!(ttl_secs = ttl.as_secs(), "fastembed idle unload enabled");
        let holder = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(IDLE_POLL_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(embedder) = holder.upgrade() else {
                    tracing::info!("fastembed idle unloader stopped");
                    return;
                };
                embedder.evict_if_idle(ttl, Instant::now()).await;
            }
        });
    }

    /// The sweep's release: same coordination as [`unload`](Self::unload), gated on a full
    /// `ttl` of quiet before `now` (the caller supplies the clock, so the idle decision is a pure
    /// comparison and tests can drive it deterministically).
    async fn evict_if_idle(&self, ttl: Duration, now: Instant) -> bool {
        let released = self.evict(Some(ttl), now).await;
        if released {
            tracing::info!("fastembed provider unloaded after idle TTL");
        }
        released
    }

    /// Takes the resident model when it is safe to free — no caller holds a clone of it and,
    /// when `idle` is given, it has sat unused for at least that long as of `now` — then drops it
    /// off the async worker thread. Both the decision and the take happen while the load gate is
    /// held, so a load cannot interleave with a release (and vice versa).
    async fn evict(&self, idle: Option<Duration>, now: Instant) -> bool {
        let _gate = self.load_gate.lock().await;
        let Some(resident) = self.take_evictable(idle, now) else {
            return false;
        };
        // Session teardown frees the ONNX arenas (~1 GiB); keep it off the async worker
        // thread, same rule as `embed_texts`. The gate stays held so a reload cannot allocate a
        // second session beside the one still being freed.
        if let Err(error) = tokio::task::spawn_blocking(move || drop(resident)).await {
            tracing::warn!(error = %error, "embed model teardown task failed");
        }
        true
    }

    /// Takes the resident model when it is evictable: no caller holds a clone of it (a caller
    /// mid-embed holds one, so the session must survive), and — when `idle` is given — it has sat
    /// unused for at least that long before `now`. The checks run under the slot's write lock,
    /// which every clone acquisition holds as a read lock, so the decision and the take cannot be
    /// split by a concurrent [`acquire`](Self::acquire) into dropping a session that is about to
    /// be used.
    fn take_evictable(&self, idle: Option<Duration>, now: Instant) -> Option<Arc<M>> {
        let mut slot = self.slot_mut();
        if Arc::strong_count(slot.as_ref()?) != 1 {
            return None;
        }
        if let Some(ttl) = idle {
            let last_used = *self.last_used.lock().unwrap_or_else(recover);
            if !should_unload(true, now.saturating_duration_since(last_used), ttl) {
                return None;
            }
        }
        slot.take()
    }

    /// Whether a model is currently resident, without loading one. Test-only: release paths
    /// decide residency inside [`take_evictable`](Self::take_evictable) so their predicate can
    /// never go stale.
    #[cfg(test)]
    fn is_loaded(&self) -> bool {
        self.slot().is_some()
    }

    /// The resident model, if any. Never loads.
    fn slot(&self) -> Option<Arc<M>> {
        // The guard is held only long enough to clone the `Arc` — never across an `.await`.
        let slot = self.model.read().unwrap_or_else(recover);
        slot.clone()
    }

    /// Write access to the slot. Callers drop the guard before any `.await`.
    fn slot_mut(&self) -> std::sync::RwLockWriteGuard<'_, Option<Arc<M>>> {
        self.model.write().unwrap_or_else(recover)
    }

    /// Marks the model as just used, so the idle sweep restarts its TTL. Called when a use
    /// *begins* (so a long embed never looks idle while it runs) and again when it ends.
    fn stamp_used(&self) {
        *self.last_used.lock().unwrap_or_else(recover) = Instant::now();
    }

    /// The idle clock's last stamp. Test-only: the release decision takes its `now` from the
    /// caller, so tests read this stamp and advance a copy with `checked_add` instead of rewinding
    /// a real `Instant` (which can underflow on a freshly booted host).
    #[cfg(test)]
    fn last_used(&self) -> Instant {
        *self.last_used.lock().unwrap_or_else(recover)
    }

    /// Returns the resident model, loading it on first use. The load runs under the async
    /// gate, so concurrent callers serialize and only one `M::load` ever happens.
    ///
    /// The gate is *owned by the blocking load*, not borrowed from this future: the winner moves
    /// the guard, the slot, and the idle clock into the [`spawn_blocking`] closure, which loads,
    /// publishes, and stamps while it holds them. Dropping this future — a cancelled caller —
    /// therefore cannot hand single flight to the next embedder while `M::load` is still
    /// allocating; the caller stops waiting, the session still lands in the slot, and the next
    /// caller reuses it instead of starting a second ~1 GiB ONNX session. A failed load releases
    /// the gate the same way, so the next call retries.
    ///
    /// [`spawn_blocking`]: tokio::task::spawn_blocking
    async fn acquire(&self) -> Result<Arc<M>> {
        if let Some(model) = self.slot() {
            self.stamp_used();
            return Ok(model);
        }

        let gate = Arc::clone(&self.load_gate).lock_owned().await;
        if let Some(model) = self.slot() {
            self.stamp_used();
            return Ok(model);
        }

        let slot = Arc::clone(&self.model);
        let last_used = Arc::clone(&self.last_used);
        let cache_dir = self.cache_dir.clone();
        tokio::task::spawn_blocking(move || {
            let _gate = gate;
            let model = Arc::new(M::load(cache_dir)?);
            *slot.write().unwrap_or_else(recover) = Some(Arc::clone(&model));
            *last_used.lock().unwrap_or_else(recover) = Instant::now();
            Ok::<_, anyhow::Error>(model)
        })
        .await
        .map_err(|e| anyhow!("embed model load task failed: {e}"))?
    }
}

#[async_trait]
impl<M: EmbedModel> EmbeddingProvider for LazyEmbedder<M> {
    /// Metadata only — the store calls this on every upsert and must not pay a model load.
    fn dim(&self) -> usize {
        EMBED_DIM
    }

    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let model = self.acquire().await?;
        let inputs = inputs.to_vec();
        let worker = Arc::clone(&model);
        let joined = tokio::task::spawn_blocking(move || worker.embed(&inputs)).await;
        // Keep the caller's `Arc` until after the completion stamp: the worker drops its clone
        // inside the blocking task, so without this a sweep landing in the handoff could see the
        // slot as the only owner alongside a start-of-use clock older than the TTL and evict a
        // session that just finished.
        self.stamp_used();
        drop(model);
        let vectors = joined.map_err(|e| anyhow!("embed_texts task failed: {e}"))??;

        Ok(vectors.into_iter().map(Embedding).collect())
    }

    /// Metadata only — never loads the model.
    fn text_model_name(&self) -> &str {
        TEXT_MODEL_NAME
    }
}

/// Pure idle-unload predicate (extracted for testing, mirroring
/// `inference::supervisor::should_evict`): unload only when a model is resident and it has sat
/// idle for at least `ttl`. `ttl == 0` disables unloading entirely (`03 §8`).
fn should_unload(loaded: bool, elapsed: Duration, ttl: Duration) -> bool {
    loaded && !ttl.is_zero() && elapsed >= ttl
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::task::{Context, Waker};

    use super::*;

    /// Load/embed event log, keyed by the cache dir the test picked. `EmbedModel::load` is an
    /// associated function with no `self`, so the fake reports through this log; dirs are
    /// unique per test, so tests running concurrently never read each other's counts. `LOADED`
    /// records *attempts* — a failed or still-parked load counts too, so the single-flight
    /// assertions see every `M::load` the holder starts.
    static LOADED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    static EMBEDDED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// Rendezvous points that hold a fake embed open until the test has inspected the holder.
    /// The wait for `release` is bounded, so a failing assertion can never park a blocking thread
    /// (and the test binary) forever.
    struct EmbedStages {
        started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    /// `EmbedModel::load` cannot take test arguments, so a test arms the next load for its cache
    /// dir through this registry. The entry is consumed by the first load, so a later reload
    /// embeds normally and cannot wait on a rendezvous nobody is waiting at.
    static STAGES: Mutex<Vec<(PathBuf, Arc<EmbedStages>)>> = Mutex::new(Vec::new());

    /// Arms the next embed for `dir`: the returned receiver fires once the embed is running, and
    /// sending on the returned sender lets it finish.
    fn block_next_embed(
        dir: &Path,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        STAGES.lock().unwrap().push((
            dir.to_path_buf(),
            Arc::new(EmbedStages {
                started: Mutex::new(Some(started_tx)),
                release: Mutex::new(release_rx),
            }),
        ));
        (started_rx, release_tx)
    }

    /// The same rendezvous for `load`: the cold-cancellation regression must hold a load inside
    /// `M::load` while it drops the caller that started it. Bounded release, like [`EmbedStages`].
    struct LoadStages {
        started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    /// Arms the next `load` for its cache dir (consumed by that load, so a retry loads normally).
    static LOAD_STAGES: Mutex<Vec<(PathBuf, Arc<LoadStages>)>> = Mutex::new(Vec::new());

    /// Cache dirs whose next `load` must fail, so the load-error path (gate release) is testable.
    static LOAD_FAILURES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// Arms the next load for `dir`: the returned receiver fires once the load is running, and
    /// sending on the returned sender lets it finish.
    fn block_next_load(
        dir: &Path,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        LOAD_STAGES.lock().unwrap().push((
            dir.to_path_buf(),
            Arc::new(LoadStages {
                started: Mutex::new(Some(started_tx)),
                release: Mutex::new(release_rx),
            }),
        ));
        (started_rx, release_tx)
    }

    /// Arms the next load for `dir` to return an error instead of a model.
    fn fail_next_load(dir: &Path) {
        LOAD_FAILURES.lock().unwrap().push(dir.to_path_buf());
    }

    /// Releases a parked load when it goes out of scope, so an assertion that fires before the
    /// explicit release still unsticks the blocking thread instead of leaving the test binary to
    /// wait out the park's timeout.
    struct ReleaseOnDrop(std::sync::mpsc::Sender<()>);

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    /// Test double: no ONNX, no network, no files. `load` sleeps briefly so concurrent callers
    /// genuinely race for the single-flight gate, parks inside `load` when [`block_next_load`]
    /// armed it, and fails when [`fail_next_load`] armed it. `embed` returns one small vector per
    /// input — or, when [`block_next_embed`] armed it, parks inside the embed until released.
    struct FakeModel {
        dir: PathBuf,
        stages: Option<Arc<EmbedStages>>,
    }

    impl EmbedModel for FakeModel {
        fn load(cache_dir: PathBuf) -> Result<Self> {
            std::thread::sleep(Duration::from_millis(20));
            LOADED.lock().unwrap().push(cache_dir.clone());

            let fails = {
                let mut failures = LOAD_FAILURES.lock().unwrap();
                let index = failures.iter().position(|dir| dir == &cache_dir);
                index.map(|index| failures.remove(index)).is_some()
            };
            if fails {
                return Err(anyhow!("staged load failure"));
            }

            let index = {
                let staged = LOAD_STAGES.lock().unwrap();
                staged.iter().position(|(dir, _)| dir == &cache_dir)
            };
            if let Some(index) = index {
                let stages = LOAD_STAGES.lock().unwrap().remove(index).1;
                let started = stages.started.lock().unwrap().take();
                if let Some(started) = started {
                    let _ = started.send(());
                }
                let _ = stages
                    .release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(60));
            }

            let index = {
                let staged = STAGES.lock().unwrap();
                staged.iter().position(|(dir, _)| dir == &cache_dir)
            };
            let stages = index.map(|index| STAGES.lock().unwrap().remove(index).1);
            Ok(Self {
                dir: cache_dir,
                stages,
            })
        }

        fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
            if let Some(stages) = self.stages.as_ref() {
                let started = stages.started.lock().unwrap().take();
                if let Some(started) = started {
                    let _ = started.send(());
                }
                let _ = stages
                    .release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(60));
            }
            EMBEDDED.lock().unwrap().push(self.dir.clone());
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                out.push(vec![input.len() as f32, 1.0]);
            }
            Ok(out)
        }
    }

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ss-embed-{tag}-{}", std::process::id()))
    }

    fn count_events(events: &Mutex<Vec<PathBuf>>, dir: &Path) -> usize {
        let events = events.lock().unwrap();
        events.iter().filter(|path| path.as_path() == dir).count()
    }

    fn load_count(dir: &Path) -> usize {
        count_events(&LOADED, dir)
    }

    fn embed_count(dir: &Path) -> usize {
        count_events(&EMBEDDED, dir)
    }

    /// The vector dimension matches the `vec0` schema (`03 §4`) — pure, no model.
    #[test]
    fn embed_dim_is_768() {
        assert_eq!(EMBED_DIM, 768);
    }

    /// `dim`/`text_model_name` are metadata: the store reads them without wanting a session.
    #[tokio::test]
    async fn dim_and_name_do_not_load() {
        let dir = unique_dir("dim-name");
        let embedder = LazyEmbedder::<FakeModel>::new(dir.clone(), Duration::from_secs(60));

        assert_eq!(embedder.dim(), EMBED_DIM);
        assert_eq!(embedder.text_model_name(), TEXT_MODEL_NAME);

        assert_eq!(
            load_count(&dir),
            0,
            "metadata calls must not load the model"
        );
        assert!(!embedder.is_loaded());
    }

    /// A cold embedder hit by concurrent embeds loads the model exactly once (single flight).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_embeds_load_once() {
        let dir = unique_dir("concurrent");
        let embedder = Arc::new(LazyEmbedder::<FakeModel>::new(
            dir.clone(),
            Duration::from_secs(60),
        ));

        let mut tasks = Vec::new();
        for i in 0..8 {
            let embedder = Arc::clone(&embedder);
            tasks.push(tokio::spawn(async move {
                embedder.embed_texts(&[format!("input {i}")]).await
            }));
        }
        for (i, task) in tasks.into_iter().enumerate() {
            let out = task.await.expect("join").expect("embed");
            assert_eq!(out.len(), 1, "one input ⇒ one embedding");
            assert_eq!(out[0].0, vec![format!("input {i}").len() as f32, 1.0]);
        }

        assert_eq!(
            load_count(&dir),
            1,
            "concurrent callers must share one load"
        );
        assert_eq!(embed_count(&dir), 8);
        assert!(embedder.is_loaded());
    }

    /// A cancelled cold load must not release single flight. The blocking task owns the gate and
    /// publishes the session it built, so the next caller reuses it instead of starting a second
    /// ~1 GiB ONNX session beside the first one (`03 §5`, `03 §8`) — the shape that leaked memory
    /// and left the next search on a session nobody else could see.
    ///
    /// One poll of the waiting caller separates the shapes without any clock: the broken one has
    /// already submitted its extra blocking load by then, while the fixed one parks on the gate
    /// the in-flight load still owns.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cancelled_cold_load_publishes_one_session() {
        let dir = unique_dir("cold-cancel");
        let (started, release) = block_next_load(&dir);
        // Released on drop as well, so a failing assertion above still unsticks the parked load.
        let release = ReleaseOnDrop(release);
        let embedder = Arc::new(LazyEmbedder::<FakeModel>::new(
            dir.clone(),
            Duration::from_secs(60),
        ));

        let first = tokio::spawn({
            let embedder = Arc::clone(&embedder);
            async move { embedder.embed_texts(&["first".to_string()]).await }
        });
        started.await.expect("the first load reached M::load");
        assert_eq!(load_count(&dir), 1, "the first caller started the load");

        // Cancel the winner while `M::load` is parked: it never returns the session itself.
        first.abort();
        match first.await {
            Ok(_) => panic!("a cancelled caller must not return an embedding"),
            Err(error) => assert!(error.is_cancelled(), "expected cancellation, got {error}"),
        }

        let inputs = ["second".to_string()];
        let mut second = embedder.embed_texts(&inputs);
        let mut context = Context::from_waker(Waker::noop());
        assert!(
            second.as_mut().poll(&mut context).is_pending(),
            "the next caller waits for the in-flight load instead of starting another"
        );
        assert_eq!(
            load_count(&dir),
            1,
            "no second M::load while the first one runs"
        );

        // Let the parked load finish: it publishes the session, which the waiter then reuses.
        drop(release);
        let out = second.await.expect("the second caller embeds");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, vec!["second".len() as f32, 1.0]);
        assert_eq!(
            load_count(&dir),
            1,
            "the published session is reused, not loaded"
        );
        assert_eq!(embed_count(&dir), 1, "only the surviving caller embedded");
        assert!(embedder.is_loaded());
    }

    /// A failed `M::load` releases the gate together with the blocking task that took it, so the
    /// error reaches the caller and the next call retries a fresh load — no leaked guard to wait
    /// on, no half-loaded holder. The timeout only bounds a deadlock; it asserts nothing about
    /// timing.
    #[tokio::test]
    async fn failed_load_releases_the_gate_for_a_retry() {
        let dir = unique_dir("load-failure");
        fail_next_load(&dir);
        let embedder = LazyEmbedder::<FakeModel>::new(dir.clone(), Duration::from_secs(60));

        let error = embedder
            .load_now()
            .await
            .expect_err("the staged load fails");
        assert!(
            error.to_string().contains("staged load failure"),
            "the load error reaches the caller: {error:#}"
        );
        assert!(!embedder.is_loaded(), "a failed load publishes nothing");
        assert_eq!(load_count(&dir), 1);

        // A leaked gate would park this forever; the timeout only bounds that failure and is not
        // a timing assertion.
        let inputs = ["retry".to_string()];
        let retry = tokio::time::timeout(Duration::from_secs(10), embedder.embed_texts(&inputs));
        let out = retry
            .await
            .expect("the retry is not stuck behind a leaked gate")
            .expect("the retry loads a fresh session");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, vec!["retry".len() as f32, 1.0]);
        assert_eq!(load_count(&dir), 2, "the retry runs a fresh M::load");
        assert!(embedder.is_loaded());
    }

    /// After an unload the next embed reloads instead of failing.
    #[tokio::test]
    async fn unload_then_embed_reloads() {
        let dir = unique_dir("reload");
        let embedder = LazyEmbedder::<FakeModel>::new(dir.clone(), Duration::from_secs(60));

        embedder.load_now().await.expect("load");
        assert!(embedder.is_loaded());
        assert_eq!(load_count(&dir), 1);

        assert!(embedder.unload().await, "a resident model is released");
        assert!(!embedder.is_loaded());

        let out = embedder
            .embed_texts(&["hello".to_string()])
            .await
            .expect("embed");
        assert_eq!(out.len(), 1);
        assert_eq!(
            load_count(&dir),
            2,
            "an unloaded model loads again on demand"
        );
        assert!(embedder.is_loaded());
    }

    /// Unloading with nothing resident is a no-op, not an error or a spurious release.
    #[tokio::test]
    async fn unload_when_empty_is_noop() {
        let dir = unique_dir("empty");
        let embedder = LazyEmbedder::<FakeModel>::new(dir, Duration::ZERO);

        assert!(!embedder.unload().await);
        assert!(!embedder.is_loaded());
    }

    /// The idle clock is measured from the last use, so a session is released only after a full
    /// `ttl` of quiet — and a zero TTL opts out even when the clock is ancient. The test supplies
    /// the decision instant (`checked_add` on the stored stamp) instead of sleeping or rewinding a
    /// real `Instant`, so the boundary is exact and cannot underflow.
    #[tokio::test]
    async fn idle_ttl_bounds_eviction() {
        let dir = unique_dir("ttl");
        let ttl = Duration::from_secs(900);
        let embedder = LazyEmbedder::<FakeModel>::new(dir.clone(), ttl);
        embedder.load_now().await.expect("load");

        let loaded_at = embedder.last_used();
        assert!(
            !embedder.evict_if_idle(ttl, loaded_at).await,
            "a just-loaded session is not idle"
        );
        let idle_at = loaded_at
            .checked_add(ttl + Duration::from_millis(1))
            .expect("advance the decision clock");
        assert!(
            embedder.evict_if_idle(ttl, idle_at).await,
            "a full idle TTL releases the session"
        );
        assert!(!embedder.is_loaded());
        assert_eq!(load_count(&dir), 1);
        assert!(
            !embedder.evict_if_idle(ttl, idle_at).await,
            "nothing resident ⇒ nothing released"
        );

        let never = LazyEmbedder::<FakeModel>::new(unique_dir("ttl-zero"), Duration::ZERO);
        never.load_now().await.expect("load");
        let loaded_at = never.last_used();
        let ancient = loaded_at
            .checked_add(Duration::from_secs(365 * 24 * 60 * 60))
            .expect("advance the decision clock");
        assert!(
            !never.evict_if_idle(Duration::ZERO, ancient).await,
            "a zero TTL never unloads"
        );
        assert!(never.is_loaded());
    }

    /// A caller mid-embed holds its own [`Arc`], so neither the sweep nor an explicit unload may
    /// free the session under it: releasing and reloading would stack a second ONNX session
    /// beside the running one instead of giving memory back. The fake parks inside the real
    /// `embed_texts` path, so the boundary is deterministic — no sleeps.
    #[tokio::test]
    async fn release_never_strands_an_in_flight_embed() {
        let dir = unique_dir("in-flight");
        let (started, release) = block_next_embed(&dir);
        let ttl = Duration::from_secs(60);
        let embedder = Arc::new(LazyEmbedder::<FakeModel>::new(dir.clone(), ttl));
        embedder.load_now().await.expect("load");

        let embed = tokio::spawn({
            let embedder = Arc::clone(&embedder);
            async move { embedder.embed_texts(&["held".to_string()]).await }
        });
        // The embed is past `acquire` and parked inside `embed` from here on.
        started.await.expect("embed started");

        // The TTL is long gone as of this supplied instant, but the in-flight handle still pins
        // the session.
        let used_at = embedder.last_used();
        let stale_at = used_at
            .checked_add(ttl * 2)
            .expect("advance the decision clock");
        assert!(
            !embedder.evict_if_idle(ttl, stale_at).await,
            "an idle sweep must not free a session an embed is using"
        );
        assert!(
            !embedder.unload().await,
            "an explicit unload must not strand a running embed"
        );
        assert!(embedder.is_loaded());
        assert_eq!(
            load_count(&dir),
            1,
            "no reload while the session is in flight"
        );

        // Let the embed finish; the next idle sweep may release the session.
        release.send(()).expect("release the embed");
        let out = embed.await.expect("join").expect("embed");
        assert_eq!(out.len(), 1);
        assert_eq!(load_count(&dir), 1);

        let done_at = embedder.last_used();
        let stale_at = done_at
            .checked_add(ttl * 2)
            .expect("advance the decision clock");
        assert!(
            embedder.evict_if_idle(ttl, stale_at).await,
            "an idle session is released once the embed is done"
        );
        assert!(!embedder.is_loaded());
        assert_eq!(load_count(&dir), 1);

        // Reloading after that release is a fresh session, not a second one beside it.
        embedder
            .embed_texts(&["again".to_string()])
            .await
            .expect("reload");
        assert_eq!(load_count(&dir), 2);
    }

    /// A zero TTL means "never unload", however long the model sits idle (`03 §8`).
    #[test]
    fn zero_ttl_disables_unload() {
        let long_idle = Duration::from_secs(30 * 24 * 60 * 60);
        assert!(!should_unload(true, long_idle, Duration::ZERO));
        assert!(!should_unload(false, long_idle, Duration::from_secs(900)));
        assert!(!should_unload(
            true,
            Duration::from_secs(899),
            Duration::from_secs(900)
        ));
        assert!(should_unload(
            true,
            Duration::from_secs(900),
            Duration::from_secs(900)
        ));
    }

    /// An existing fastembed cache for the real-model tests, taken from
    /// `SCREENSSEARCH_EMBED_CACHE`, or a throwaway temp dir (which downloads) when unset. Only a
    /// dir this helper created is deleted afterwards, so a run pointed at a real cache never
    /// touches it.
    fn real_model_cache(tag: &str) -> (PathBuf, bool) {
        match std::env::var_os("SCREENSSEARCH_EMBED_CACHE") {
            Some(dir) => (PathBuf::from(dir), false),
            None => (
                std::env::temp_dir().join(format!("ssv2c-embed-{tag}-{}", std::process::id())),
                true,
            ),
        }
    }

    /// Loads the real EmbeddingGemma model through a [`LazyEmbedder`] and embeds text. With
    /// `SCREENSSEARCH_EMBED_CACHE` pointing at a populated fastembed cache the run is offline;
    /// otherwise it downloads hundreds of MB. Run locally:
    /// `SCREENSSEARCH_EMBED_CACHE="$APPDATA/app.screensearchv2c.desktop/models/fastembed" \
    ///  cargo test -p embeddings -- --ignored`.
    #[tokio::test]
    #[ignore = "loads the real EmbeddingGemma model; run locally with --ignored"]
    async fn lazy_holder_unloads_and_reloads_the_real_model() {
        let (cache, disposable) = real_model_cache("lazy");
        let embedder =
            LazyEmbedder::<FastEmbedProvider>::new(cache.clone(), Duration::from_secs(1));

        embedder.load_now().await.expect("load text model");
        assert!(embedder.is_loaded());

        let first = embedder
            .embed_texts(&["hello world".to_string()])
            .await
            .expect("embed");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].len(), EMBED_DIM);

        assert!(
            !embedder
                .evict_if_idle(Duration::from_secs(1), Instant::now())
                .await,
            "a just-used session is not idle"
        );
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(
            embedder
                .evict_if_idle(Duration::from_secs(1), Instant::now())
                .await,
            "an idle session is released"
        );
        assert!(!embedder.is_loaded());

        let second = embedder
            .embed_texts(&["hello world".to_string()])
            .await
            .expect("reload and embed");
        assert_eq!(
            first[0].0, second[0].0,
            "the reloaded session embeds identically"
        );

        if disposable {
            let _ = std::fs::remove_dir_all(&cache);
        }
    }

    /// Loads the real EmbeddingGemma model and embeds text: 768-dim output,
    /// identical input ⇒ identical vector, different input ⇒ different vector.
    /// Downloads the model (hundreds of MB) unless `SCREENSSEARCH_EMBED_CACHE` points at an
    /// existing cache — run locally:
    /// `cargo test -p embeddings -- --ignored`.
    #[tokio::test]
    #[ignore = "downloads the EmbeddingGemma model; run locally with --ignored"]
    async fn loads_and_embeds_text() {
        let (cache, disposable) = real_model_cache("provider");
        let provider = {
            let cache = cache.clone();
            tokio::task::spawn_blocking(move || FastEmbedProvider::new(cache))
                .await
                .unwrap()
                .expect("load text model")
        };

        assert_eq!(provider.dim(), 768);
        assert_eq!(provider.text_model_name(), TEXT_MODEL_NAME);

        let a = provider
            .embed_texts(&["hello world".to_string()])
            .await
            .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].len(), 768);

        let b = provider
            .embed_texts(&["hello world".to_string()])
            .await
            .unwrap();
        assert_eq!(a[0].0, b[0].0, "identical input ⇒ identical vector");

        let c = provider
            .embed_texts(&["a completely unrelated sentence".to_string()])
            .await
            .unwrap();
        assert_ne!(a[0].0, c[0].0, "different input ⇒ different vector");

        if disposable {
            let _ = std::fs::remove_dir_all(&cache);
        }
    }
}
