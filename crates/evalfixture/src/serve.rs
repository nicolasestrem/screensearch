//! # `serve` - the product's own local HTTP API over a generated fixture database
//!
//! `ssv2c-evalfixture serve --db <fixture>/screensearch.db --data <fixture> [--port <n>]
//! [--embed] [--no-answer]` starts the real `crates/api` server ([`api::ApiServer::start`])
//! in this process, wired to a real [`SqliteStore`] over the database `gen` produced, so
//! the eval harness measures the shipped retrieval and answer code - never a
//! re-implementation of it. This is the fixture stand-in for "start the app and enable the
//! API in Settings": it never touches the maintainer's profile, database or models.
//!
//! ## Surface honesty
//!
//! The host implements exactly what a fixture has and fails the rest explicitly:
//!
//! - `GET /v1/search`, `GET /v1/frames/{id}` (+`?image=1`) and `POST /v1/ask` run the same
//!   store paths the composition root uses (cited per method below). `POST /v1/ask` calls
//!   `kernel::ask::ask_context` itself — the one legacy retrieval `Kernel::ask_context` and
//!   the app's `ApiHost` run — so the fixture cannot measure a variant of it.
//! - Ask v2 turns are not served here: the host keeps `ApiHost::ask_turn`'s `Ok(None)`
//!   default, so the route falls back to the legacy surface above. Serving a turn would
//!   persist `ask_threads`/`ask_turns` rows (and enqueue `session_embed` jobs) into the
//!   fixture database, and this binary serves reads only; nothing is fabricated to avoid it.
//! - Marks, where-was-i and export have no fixture backing: those methods fail with
//!   explicit errors. `frame_page`, `capture_coverage`, `report_page` and `report_detail`
//!   keep the trait's `UnsupportedStore` defaults (the routes map that to 503). Nothing
//!   fabricates a success or an empty page.
//!
//! ## Retrieval arms
//!
//! Without `--embed` no embedder is attached, and the product's own degradation applies:
//! `SqliteStore::hybrid_search` runs its FTS arm only, because the vector arm needs a
//! query embedder. That FTS-only mode is a real product state (the app serves search
//! before fastembed has loaded), so measuring it is measuring shipped behavior. With
//! `--embed`, the real fastembed provider is loaded from `<data>/models/fastembed` and
//! attached via `SqliteStore::set_embedder`, so `/v1/search` exercises the vector arm
//! too. A provider error is fatal - never a silent FTS fallback that would measure the
//! wrong arm.
//!
//! ## Answer sidecar
//!
//! Without `--no-answer`, the answer sidecar is built and preloaded exactly like the
//! composition root's `init_inference` (`src-tauri/src/lib.rs`): the llama-server engine is
//! ensured, the supervisor constructed with the app's configuration, the configured
//! answer tier resolved, and `preload()` run, so `/v1/ask` streams real answers. With
//! `--no-answer`, no provider is attached and the route answers
//! `503 {"error":"unavailable","message":"answer model not loaded"}` - precisely the
//! app's behavior with no model loaded.
//!
//! ## Read surface
//!
//! `serve` adds no writes of its own: it opens the existing database, reads the bearer
//! token from the `settings` row [`API_TOKEN_SETTING`] that `gen` wrote (a seeded
//! synthetic value; it is never generated here and never printed), and serves reads. If
//! the database or the token is missing, the error names the fix: run
//! `ssv2c-evalfixture gen` first. The only writes that can ever happen are the product's
//! own settings machinery inside `kernel::settings::load_settings` (its one-shot
//! `sessions.gap_close_secs` default latch, the same thing the app does the first time it
//! opens a fresh database); serve creates no tables and migrates nothing.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use inference::{AnswerSidecar, ModelSupervisor, SupervisorConfig};
use store::SqliteStore;
use traits::{
    AnswerProvider, ExportFrameRow, FrameDetail, ResumeContext, RetrievedChunk, Settings, Store,
};

use api::{ApiHost, ApiServer, FrameImage, SharedToken};

use crate::{API_TOKEN_SETTING, DEFAULT_PORT};

/// The app's `SIDECAR_HEALTH_TIMEOUT` (`src-tauri/src/lib.rs`): the max wait for the
/// sidecar's `/health` after a spawn. Kept as a literal here so the fixture serves with
/// the app's timing instead of inventing a dev-only one.
const SIDECAR_HEALTH_TIMEOUT: Duration = Duration::from_secs(180);

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture serve --db <path> --data <dir> [--port <n>] [--embed] [--no-answer]

OPTIONS:
  --db <path>    the fixture database written by `gen` (never created or migrated here)
  --data <dir>   the fixture directory: models/ and sidecar/ live and download here
  --port <n>     listening port (default 43210; 0 asks the OS for a free one)
  --embed        attach the real fastembed text embedder (the vector arm of /v1/search);
                 without it the product's FTS-only degradation applies
  --no-answer    skip the answer sidecar so /v1/ask answers 503 (the app's no-model
                 behavior)
";

/// The `serve` CLI surface. `db` must already exist (gen wrote it); `data` is the fixture
/// directory where the embedding cache and the llama-server engine/model live.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub db: PathBuf,
    pub data: PathBuf,
    pub port: u16,
    pub embed: bool,
    pub no_answer: bool,
}

impl ServeOptions {
    /// Parses the `serve` flags. `--db` and `--data` are required; the missing ones are
    /// named in the error instead of guessing a default location.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut db: Option<PathBuf> = None;
        let mut data: Option<PathBuf> = None;
        let mut port: Option<u16> = None;
        let mut embed = false;
        let mut no_answer = false;

        let mut index = 0;
        while let Some(arg) = args.get(index) {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            };
            match flag.as_str() {
                "--db" => db = Some(PathBuf::from(take_value(&flag, inline, args, &mut index)?)),
                "--data" => {
                    data = Some(PathBuf::from(take_value(&flag, inline, args, &mut index)?))
                }
                "--port" => {
                    let raw = take_value(&flag, inline, args, &mut index)?;
                    port = Some(
                        raw.parse::<u16>()
                            .with_context(|| format!("invalid --port `{raw}`"))?,
                    );
                }
                "--embed" => {
                    reject_value(&flag, inline)?;
                    embed = true;
                    index += 1;
                }
                "--no-answer" => {
                    reject_value(&flag, inline)?;
                    no_answer = true;
                    index += 1;
                }
                other => bail!("unknown flag `{other}` for serve\n\n{USAGE}"),
            }
        }

        let db = db.ok_or_else(|| {
            anyhow::anyhow!("--db is required (the fixture database written by gen)\n\n{USAGE}")
        })?;
        let data = data.ok_or_else(|| {
            anyhow::anyhow!(
                "--data is required (the fixture directory holding models/ and sidecar/)\n\n{USAGE}"
            )
        })?;
        Ok(Self {
            db,
            data,
            port: port.unwrap_or(DEFAULT_PORT),
            embed,
            no_answer,
        })
    }
}

/// A running fixture API server. Hold it while you serve; [`ServedFixture::stop`] shuts
/// the answer sidecar (when built) and the HTTP listener down, bounded, and releases the
/// port.
pub struct ServedFixture {
    server: ApiServer,
    /// Held so `stop` can make the same graceful app-exit call the composition root makes;
    /// dropping it after the server stops also ends the Job-Object no-orphan guarantee
    /// cleanly (the supervisor kills the sidecar before its handle goes away).
    supervisor: Option<Arc<ModelSupervisor>>,
}

impl ServedFixture {
    /// The actual bound loopback address. With `--port 0` this reports the OS-chosen
    /// port.
    pub fn local_addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    /// Clean stop: the answer sidecar is shut down first (the supervisor's app-exit path),
    /// then the API server drains its connections and releases the port.
    pub async fn stop(self) {
        if let Some(supervisor) = &self.supervisor {
            supervisor.shutdown().await;
        }
        self.server.stop().await;
    }
}

/// Opens the fixture database, wires the real API host and starts the product's own HTTP
/// server on loopback. The port (0 = OS-chosen) is bound before this returns.
pub async fn start(opts: &ServeOptions) -> Result<ServedFixture> {
    // `serve` is a read surface over a *generated* fixture: the database must already
    // exist, and a missing one names the fix instead of silently creating a fresh one
    // (`SqliteStore::open_path` would happily initialize an empty v14 file).
    if !opts.db.is_file() {
        bail!(
            "no fixture database at {} - run `ssv2c-evalfixture gen --out <dir>` first; \
             serve never creates or migrates a database",
            opts.db.display()
        );
    }
    let store = Arc::new(
        SqliteStore::open_path(&opts.db)
            .with_context(|| format!("opening the fixture database {}", opts.db.display()))?,
    );

    // The bearer token is part of the generated fixture; it is read from the settings
    // table, never generated here and never printed.
    let token = store
        .get_setting(API_TOKEN_SETTING)
        .await
        .with_context(|| format!("reading `{API_TOKEN_SETTING}` from {}", opts.db.display()))?;
    let Some(token) = token.filter(|token| !token.trim().is_empty()) else {
        bail!(
            "no `{API_TOKEN_SETTING}` setting in {} - it is written by \
             `ssv2c-evalfixture gen`; re-run gen (serve never generates a token)",
            opts.db.display()
        );
    };

    // `--embed`: attach the real text embedder so /v1/search runs its vector arm. The
    // load is blocking (fastembed downloads on first use), hence the blocking pool, like
    // the composition root. A failure is fatal, not a silent FTS-only fallback.
    if opts.embed {
        let cache_dir = opts.data.join("models").join("fastembed");
        let load_dir = cache_dir.clone();
        let provider =
            tokio::task::spawn_blocking(move || embeddings::FastEmbedProvider::new(load_dir))
                .await
                .context("the fastembed loader task failed")?
                .with_context(|| {
                    format!(
                        "loading the fastembed text model from {}",
                        cache_dir.display()
                    )
                })?;
        store.set_embedder(Arc::new(provider));
    }

    // The answer sidecar (skipped with `--no-answer`): built and preloaded exactly like
    // the composition root's init_inference, so /v1/ask measures the shipped answer path.
    let (supervisor, answer) = if opts.no_answer {
        (None, None)
    } else {
        let (supervisor, answer) = build_answer_sidecar(&opts.data, store.as_ref()).await?;
        (Some(supervisor), Some(answer))
    };

    let host = Arc::new(FixtureServeHost {
        store,
        data_dir: opts.data.clone(),
        answer,
        started_at_ms: now_ms(),
    });
    let shared: SharedToken = Arc::new(std::sync::RwLock::new(token));
    let server = ApiServer::start(host, shared, opts.port)
        .await
        .with_context(|| format!("binding the fixture API on 127.0.0.1:{}", opts.port))?;
    Ok(ServedFixture { server, supervisor })
}

/// Builds and preloads the answer lane exactly like the composition root's
/// `init_inference` (`src-tauri/src/lib.rs`): ensure the llama-server engine, construct
/// the supervisor with the app's configuration, resolve the configured answer tier, then
/// `preload()` so `/v1/ask` is instant. The fixture directory plays the app data dir.
async fn build_answer_sidecar(
    data: &Path,
    store: &SqliteStore,
) -> Result<(Arc<ModelSupervisor>, Arc<AnswerSidecar>)> {
    let sidecar_dir = data.join("sidecar");
    let models_root = data.join("models");
    std::fs::create_dir_all(&sidecar_dir)
        .with_context(|| format!("creating {}", sidecar_dir.display()))?;

    // Idempotent engine acquisition (skip-if-present) - the app's `ensure_binary`.
    let binary = inference::download::ensure_binary(&sidecar_dir)
        .await
        .context("resolving the llama-server engine")?;

    let mut reap_binaries = inference::download::installed_binary_candidates(&sidecar_dir);
    reap_binaries.push(binary.clone());
    // Probe once for the memory-tuning flags this engine accepts (a blocking `--help`
    // spawn, hence the blocking pool); a failed probe falls back to the conservative
    // caps, exactly like the app.
    let binary_for_probe = binary.clone();
    let caps = tokio::task::spawn_blocking(move || inference::probe_caps(&binary_for_probe))
        .await
        .unwrap_or_else(|_| inference::SidecarCaps::conservative());

    // `sidecar.idle_ttl_secs` / `sidecar.recycle_*` are file-only, resolved once at
    // supervisor construction (`03 §8`, 0.4.5 D6) - the app's exact path. The fixture
    // directory has no advanced.toml, so this resolves the documented defaults.
    let advanced = kernel::settings::load_advanced_config(data);
    let config = SupervisorConfig {
        binary,
        reap_binaries,
        pidfile: sidecar_dir.join("llama-server.pid"),
        sidecar_log: Some(sidecar_dir.join("llama-server.log")),
        idle_ttl: Duration::from_secs(u64::from(advanced.sidecar.idle_ttl_secs)),
        // The app's SIDECAR_HEALTH_TIMEOUT.
        health_timeout: SIDECAR_HEALTH_TIMEOUT,
        caps,
        recycle_ceiling_bytes: inference::resolve_recycle_ceiling(
            advanced.sidecar.recycle_enabled,
            advanced.sidecar.recycle_rss_mb,
        ),
    };
    // The supervisor owns the Job-Object no-orphan lifecycle and performs its own
    // startup reap of a stray sidecar.
    let supervisor = ModelSupervisor::new(config).context("initializing the model supervisor")?;

    let settings = kernel::settings::load_settings(store).await;
    let params = traits::SidecarParams::from(&advanced);
    let downloader = inference::ModelDownloader::new(models_root.clone());
    let answer = Arc::new(AnswerSidecar::new(
        supervisor.clone(),
        downloader,
        models_root,
        settings.models_answer_tier,
        params,
    ));
    // Eagerly load the configured tier (downloads on first use). Fatal on failure: a
    // serve that answers 503 later would hide the broken setup from the operator.
    answer
        .preload()
        .await
        .context("preloading the answer model")?;
    Ok((supervisor, answer))
}

/// The fixture's [`ApiHost`]: a real store, real settings and the kernel's real legacy Ask
/// retrieval; no capture loop, no kernel instance, no marks. Modeled on the composition
/// root's `TauriApiHost` (`src-tauri/src/local_api.rs`) and the mcp example host
/// (`crates/mcp/examples/api_fixture.rs`).
struct FixtureServeHost {
    store: Arc<SqliteStore>,
    data_dir: PathBuf,
    /// The preloaded answer lane; `None` under `--no-answer` (the route answers 503).
    answer: Option<Arc<AnswerSidecar>>,
    started_at_ms: i64,
}

#[async_trait]
impl ApiHost for FixtureServeHost {
    fn version(&self) -> String {
        format!("ssv2c-evalfixture {}", env!("CARGO_PKG_VERSION"))
    }

    fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    /// Honest state, not a stub: nothing captures into a fixture while it is served.
    async fn is_capturing(&self) -> bool {
        false
    }

    fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }

    async fn settings(&self) -> Settings {
        // The same loader the composition root uses, so the API mirrors the UI defaults.
        kernel::settings::load_settings(self.store.as_ref()).await
    }

    async fn frame_detail(&self, frame_id: i64) -> Result<Option<FrameDetail>> {
        self.store.get_frame(frame_id).await
    }

    async fn frame_image(&self, frame_id: i64) -> Result<Option<FrameImage>> {
        // The composition root's path (`src-tauri/src/local_api.rs`): purged frames have
        // no image, the stored path is contained under the data dir, and a file missing
        // on disk is a clean `None` so the route answers a 404 rather than a 500. `gen`
        // mirrors the production capture-loop path format (kernel/src/capture_loop.rs:
        // `frames/day-<n>/<captured_at>-<monitor>.webp`) but stores no image bytes
        // anywhere, so this honestly resolves to `None` for every fixture frame: the
        // fixture measures text retrieval, not thumbnails.
        let Some(detail) = self.store.get_frame(frame_id).await? else {
            return Ok(None);
        };
        if detail.image_purged {
            return Ok(None);
        }
        let Some(path) = safe_frame_path(&self.data_dir, &detail.image_path) else {
            return Ok(None);
        };
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        // Current frames are WebP; pre-WebP ones are JPEG - the extension decides.
        let content_type = if detail.image_path.to_ascii_lowercase().ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        };
        Ok(Some(FrameImage {
            bytes,
            content_type,
        }))
    }

    async fn ask_context(
        &self,
        query: &str,
        top_k: u32,
        session_id: Option<i64>,
    ) -> Result<Vec<RetrievedChunk>> {
        // The kernel's own legacy retrieval (`kernel::ask::ask_context`, the one
        // `Kernel::ask_context` and `TauriApiHost::ask_context` run): content-text hybrid
        // search at the request's depth (chrome stays opt-in), session-scoped when asked,
        // then one bulk OCR hydrate with the hit snippet as the per-frame fallback. Calling
        // the shipped function — rather than reproducing it — is what makes the fixture's
        // `/v1/ask` measure the app's grounding instead of a variant of it.
        let store: Arc<dyn Store> = self.store.clone();
        kernel::ask::ask_context(&store, query, top_k, session_id).await
    }

    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.answer
            .clone()
            .map(|answer| answer as Arc<dyn AnswerProvider>)
    }

    async fn add_mark(
        &self,
        _frame_id: Option<i64>,
        _capture_now: bool,
        _note: Option<String>,
    ) -> Result<i64> {
        bail!(
            "evalfixture serve has no marks surface (no capture loop, no kernel): \
             marks are not available on the fixture API"
        )
    }

    async fn resolve_mark(&self, _mark_id: i64) -> Result<()> {
        bail!(
            "evalfixture serve has no marks surface (no capture loop, no kernel): \
             marks are not available on the fixture API"
        )
    }

    async fn where_was_i(&self) -> Result<Option<ResumeContext>> {
        // No kernel or flow state exists here, so Ok(None) would fabricate a
        // "nothing to resume" heuristic answer that was never computed; fail explicitly.
        bail!(
            "evalfixture serve has no resume heuristic (no kernel/flow state): \
             where-was-i is not available on the fixture API"
        )
    }

    async fn export_frames_page(
        &self,
        _after_id: i64,
        _from: Option<i64>,
        _to: Option<i64>,
        _limit: u32,
    ) -> Result<Vec<ExportFrameRow>> {
        bail!(
            "evalfixture serve is a read surface for /v1/search and /v1/ask evaluation: \
             export is not available on the fixture API"
        )
    }

    // frame_page, capture_coverage, report_page and report_detail deliberately keep the
    // trait's `UnsupportedStore` defaults (the history/report routes answer 503): the
    // fixture has no coverage or saved-report surface to forward to, and an empty page
    // would fabricate one.
}

/// The composition root's frame-image containment rule (`src-tauri/src/lib.rs`,
/// `safe_frame_path`): only a relative path under `frames/` resolves, so a crafted
/// `image_path` in the database can never read a file outside the fixture directory.
fn safe_frame_path(data_dir: &Path, image_path: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel = Path::new(image_path);
    if rel.is_absolute() {
        return None;
    }
    let mut components = rel.components();
    match components.next()? {
        Component::Normal(first) if first == "frames" => {}
        _ => return None,
    }
    let mut safe = PathBuf::from("frames");
    for component in components {
        match component {
            Component::Normal(part) => safe.push(part),
            _ => return None,
        }
    }
    Some(data_dir.join(safe))
}

/// Parses the `serve` arguments, starts the server, prints the status lines (listening
/// URL, database path, embedding and answer state) and serves until Ctrl+C. The bearer
/// token is never printed: it lives in the fixture database for the harness to read.
pub async fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let opts = ServeOptions::parse(args)?;
    let fixture = start(&opts).await?;
    println!(
        "ssv2c-evalfixture: serving {} at http://{} (embeddings: {}, answer sidecar: {})",
        opts.db.display(),
        fixture.local_addr(),
        if opts.embed {
            "attached (vector arm on)"
        } else {
            "off (FTS-only, the product's own degradation)"
        },
        if opts.no_answer {
            "off (--no-answer, /v1/ask answers 503)"
        } else {
            "preloaded"
        },
    );
    println!(
        "harness: cargo run -p harness --bin harness -- eval --db {} --api http://{}",
        opts.db.display(),
        fixture.local_addr()
    );
    tokio::signal::ctrl_c()
        .await
        .context("waiting for Ctrl+C")?;
    fixture.stop().await;
    Ok(())
}

/// Wall-clock unix epoch ms (the kernel clock unit); the same one-liner the api fixture
/// example uses.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Reads a flag's value: either the `--flag=value` inline form or the next argument.
fn take_value(
    flag: &str,
    inline: Option<String>,
    args: &[String],
    index: &mut usize,
) -> Result<String> {
    if let Some(value) = inline {
        *index += 1;
        return Ok(value);
    }
    let Some(value) = args.get(*index + 1) else {
        bail!("missing value for {flag}\n\n{USAGE}");
    };
    *index += 2;
    Ok(value.clone())
}

/// Boolean flags take no value.
fn reject_value(flag: &str, inline: Option<String>) -> Result<()> {
    if inline.is_some() {
        bail!("{flag} takes no value\n\n{USAGE}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{NewFrame, NewSession, OcrResult, SessionHost, SessionKind};

    /// The comparable shape of a chunk (`RetrievedChunk` has no `PartialEq`): identity,
    /// hydrated text, the score's exact bits, capture time and session context.
    fn chunks(chunks: &[RetrievedChunk]) -> Vec<(i64, String, u32, i64, bool)> {
        chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.frame_id,
                    chunk.text.clone(),
                    chunk.score.to_bits(),
                    chunk.captured_at,
                    chunk.session.is_some(),
                )
            })
            .collect()
    }

    /// The fixture host's `/v1/ask` grounding is the kernel's own legacy retrieval, not a
    /// copy of it: for one fixture — stored OCR text, two sessions — the host returns
    /// exactly the chunks `kernel::ask::ask_context` returns (same frames, same order, same
    /// hydrated text, same session scope), so the eval harness measures the shipped pipeline.
    #[tokio::test]
    async fn ask_context_delegates_to_the_kernel_retrieval() {
        async fn seed(store: &SqliteStore, at: i64, hash: &str, text: &str) -> i64 {
            let id = store
                .insert_frame(NewFrame {
                    captured_at: at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{hash}.webp"),
                    content_hash: hash.to_string(),
                    app_hint: Some("Editor".to_string()),
                    window_title: Some("planning".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            store
                .insert_ocr(
                    id,
                    OcrResult {
                        text: text.to_string(),
                        mean_confidence: 0.9,
                        engine: "test".to_string(),
                        spans: Vec::new(),
                    },
                )
                .await
                .unwrap();
            id
        }

        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let outside_text = "quarterly planning notes outside any session, long enough that a \
                            snippet would be a fragment of this stored row";
        let inside_text = "quarterly planning notes inside the session, likewise longer than a \
                           snippet fragment of the stored row";
        let outside = seed(store.as_ref(), 1_700_000_000_000, "outside", outside_text).await;
        let inside = seed(store.as_ref(), 1_700_000_060_000, "inside", inside_text).await;
        let session_id = store
            .insert_session(NewSession {
                started_at: 1_700_000_060_000,
                ended_at: Some(1_700_000_120_000),
                kind: SessionKind::Focus,
                tool: None,
                host: Some(SessionHost::Ide),
                context_key: "focus:editor".to_string(),
                confidence: 0.8,
                frozen: false,
            })
            .await
            .unwrap();
        store
            .assign_frames_session(&[inside], Some(session_id))
            .await
            .unwrap();

        let host = FixtureServeHost {
            store: store.clone(),
            data_dir: PathBuf::from("fixture"),
            answer: None,
            started_at_ms: 0,
        };
        let kernel_store: Arc<dyn Store> = store.clone();

        for scope in [None, Some(session_id)] {
            let host_chunks = host
                .ask_context("quarterly", 10, scope)
                .await
                .expect("fixture ask context");
            let kernel_chunks = kernel::ask::ask_context(&kernel_store, "quarterly", 10, scope)
                .await
                .expect("kernel ask context");

            assert!(
                !kernel_chunks.is_empty(),
                "the fixture query matches the stored frames"
            );
            assert_eq!(
                chunks(&host_chunks),
                chunks(&kernel_chunks),
                "the host forwards the kernel's chunks verbatim"
            );
            let mut retrieved: Vec<i64> = host_chunks.iter().map(|c| c.frame_id).collect();
            retrieved.sort_unstable();
            let mut expected = if scope.is_some() {
                vec![inside]
            } else {
                vec![outside, inside]
            };
            expected.sort_unstable();
            assert_eq!(retrieved, expected, "the session scope narrows retrieval");
            for chunk in &host_chunks {
                let stored = if chunk.frame_id == inside {
                    inside_text
                } else {
                    outside_text
                };
                assert_eq!(chunk.text, stored, "the stored OCR row, not the snippet");
                assert!(chunk.session.is_none());
            }
        }
    }
}
