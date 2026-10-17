//! Dev-only packaged-verification fixture for `screensearch-mcp` (agent-memory P2.4).
//!
//! Starts the **real** `crates/api` server in-process over a **real** `SqliteStore` on a
//! disposable SQLite file, seeds one frame + one mark, prints a single JSON `ready` line on
//! stdout, then serves until killed or stdin closes. `scripts/verify-mcp-package.mjs` spawns
//! this and points a *packaged* `screensearch-mcp` binary at it, proving that an
//! authenticated HTTP read (`list_marks`) works on the native OS against real product code —
//! no synthetic HTTP responses anywhere.
//!
//! The `ready` line is a status line only: it deliberately omits the bearer token, which is a
//! live credential and must never be written to stdout or CI logs. The verifier proves the
//! token still works by its own HTTP sanity check (401 anonymous, 200 authenticated).
//!
//! Built from the `mcp` package's existing dev-dependencies only
//! (`cargo build --locked -p mcp --example api_fixture`); examples never enter the shipped
//! binary, so the wrapper's "no store, no app coupling" boundary is untouched.
//!
//! Environment (both required):
//! - `SCREENSEARCH_FIXTURE_DB`    path to the disposable SQLite file to create
//! - `SCREENSEARCH_FIXTURE_TOKEN` bearer token the fixture API will require

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use api::{ApiHost, ApiServer, FrameImage, SharedToken};
use async_trait::async_trait;
use serde_json::json;
use store::SqliteStore;
use traits::{
    AnswerProvider, ExportFrameRow, FrameDetail, NewFrame, OcrResult, ResumeContext,
    RetrievedChunk, Settings, Store,
};

/// The one note the fixture seeds; echoed on the `ready` line so the verifier asserts the
/// exact value it reads back through the packaged binary.
const FIXTURE_NOTE: &str = "packaged-binary-verify";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct FixtureHost {
    store: Arc<SqliteStore>,
    started_at_ms: i64,
}

#[async_trait]
impl ApiHost for FixtureHost {
    fn version(&self) -> String {
        format!("api-fixture {}", env!("CARGO_PKG_VERSION"))
    }

    fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    async fn is_capturing(&self) -> bool {
        false
    }

    fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }

    async fn settings(&self) -> Settings {
        Settings::default()
    }

    async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
        self.store.get_frame(frame_id).await
    }

    async fn frame_image(&self, _frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
        // The fixture stores no image files; report none rather than inventing bytes.
        Ok(None)
    }

    async fn ask_context(
        &self,
        _query: &str,
        _top_k: u32,
        _session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        // No answer model in the fixture (`answer_provider` is None, so `/v1/ask` returns
        // 503 before this is ever consulted); fail loudly if something still calls it.
        anyhow::bail!("api_fixture: ask context unavailable (no answer model)")
    }

    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        None
    }

    async fn add_mark(
        &self,
        frame_id: Option<i64>,
        capture_now: bool,
        note: Option<String>,
    ) -> anyhow::Result<i64> {
        match (frame_id, capture_now) {
            (Some(id), false) => self.store.insert_mark(id, now_ms(), note).await,
            // No capture worker in the fixture, so "mark now" is honestly unavailable.
            (None, true) => anyhow::bail!("api_fixture: capture is off"),
            _ => anyhow::bail!("exactly one of frame_id or capture_now"),
        }
    }

    async fn resolve_mark(&self, mark_id: i64) -> anyhow::Result<()> {
        self.store.resolve_mark(mark_id, now_ms()).await
    }

    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
        // No kernel/flow state in the fixture — "nothing to resume" is the honest answer.
        Ok(None)
    }

    async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>> {
        self.store
            .export_frames_page(after_id, from, to, limit)
            .await
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = std::env::var("SCREENSEARCH_FIXTURE_DB")
        .expect("SCREENSEARCH_FIXTURE_DB must be set (path to a disposable SQLite file)");
    let token = std::env::var("SCREENSEARCH_FIXTURE_TOKEN")
        .expect("SCREENSEARCH_FIXTURE_TOKEN must be set (bearer token the fixture API requires)");

    let store = Arc::new(SqliteStore::open_path(Path::new(&db_path))?);
    let seeded_at = now_ms();
    let frame_id = store
        .insert_frame(NewFrame {
            captured_at: seeded_at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: "fixture/frame.webp".to_string(),
            content_hash: format!("api-fixture-{seeded_at}"),
            app_hint: Some("api_fixture".to_string()),
            window_title: Some("packaged verification".to_string()),
            browser_url: None,
            capture_trigger: None,
        })
        .await?;
    store
        .insert_ocr(
            frame_id,
            OcrResult {
                text: "packaged verification fixture frame".to_string(),
                mean_confidence: 0.9,
                engine: "api_fixture".to_string(),
                spans: Vec::new(),
            },
        )
        .await?;
    let mark_id = store
        .insert_mark(frame_id, seeded_at, Some(FIXTURE_NOTE.to_string()))
        .await?;

    let host = Arc::new(FixtureHost {
        store,
        started_at_ms: seeded_at,
    });
    let shared: SharedToken = Arc::new(RwLock::new(token.clone()));
    // Port 0: an ephemeral loopback port, so parallel verifications never collide.
    let server = ApiServer::start(host, shared, 0).await?;
    let url = format!("http://127.0.0.1:{}", server.local_addr().port());

    // Single machine-readable line on stdout; everything else belongs to the parent. The
    // token is intentionally absent — a status line must never carry the live credential.
    println!(
        "{}",
        json!({
            "event": "ready",
            "url": url,
            "db": db_path,
            "frame_id": frame_id,
            "mark_id": mark_id,
            "note": FIXTURE_NOTE,
        })
    );
    std::io::stdout().flush()?;

    // Exit when the parent closes stdin (also fires when the parent dies and the pipe
    // breaks, so a killed verifier never leaves an orphaned fixture behind).
    tokio::task::spawn_blocking(|| {
        let mut sink = String::new();
        let _ = std::io::stdin().read_to_string(&mut sink);
        std::process::exit(0);
    });

    // Serve until then. `server` stays alive for the lifetime of main.
    std::future::pending::<()>().await;
    Ok(())
}
