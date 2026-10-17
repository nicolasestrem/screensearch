//! Frame, OCR, and vision records — the always-on capture path's write targets
//! plus the assembled per-frame read used by the `get_frame` IPC command
//! (`03 §3/§4/§7`).

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use textfilter::{classify, reconcile, ClassifyInput, FilterConfig};
use traits::{
    AppSuppression, CaptureTrigger, ExportFrameRow, FrameDetail, FrameEnrichmentInput, NewFrame,
    OcrResult, Result, SuppressReason, TextFilterContext, TextRole, TextSource, TextSpan,
    VisionAnalysis,
};

use crate::schema::{FILTER_VERSION, UNFILTERED_FILTER_VERSION};
use crate::SqliteStore;

/// Derives `frame_text.primary_source` from the recognizer's engine tag: the UIA provider
/// reports `engine = "uia"`; every OCR engine (`"winrt"`) maps to `ocr`. A frame that fell
/// back from UIA to OCR thus records `'ocr'`, so `FrameDetail.text_source` always reflects
/// what actually produced the text. (UIA spans separately carry `TextSource::Uia`.)
fn primary_source_for(engine: &str) -> &'static str {
    match engine {
        "uia" => TextSource::Uia.as_db_str(),
        _ => TextSource::Ocr.as_db_str(),
    }
}

/// The `settings` key holding the active attention `filter_version` watermark, used by
/// [`SqliteStore::backfill_filter_version`]. Internal bookkeeping — deliberately not a
/// user-facing [`traits::Settings`] field.
const CATALOG_FILTER_VERSION_KEY: &str = "text.catalog_filter_version";

/// Frames re-cleaned per backfill transaction. Small enough to keep the write lock short
/// (a concurrent capture isn't blocked for long) and the rollback bounded; large enough
/// to amortize per-statement overhead. Internal tuning constant.
const BACKFILL_BATCH: usize = 64;

/// Loads the chrome catalog's prior `seen_count`s for the foreground `app_hint` in a
/// single query, returning the `signature -> seen_count` map the pure classifier reads
/// via [`textfilter::ChromeCatalog`] (`HashMap` already implements it). One bulk read
/// instead of one point-lookup per candidate line on the OCR hot path. Scoped to the
/// current app because every signature this frame can query is built with this
/// `app_hint` as its prefix (`app_hint IS ?1` matches the NULL app correctly, unlike
/// `=`). Reads see committed state (the per-frame bump happens after classify); a read
/// error → empty map (treat as all-unseen — never over-suppress on a transient
/// failure, the top risk).
fn load_chrome_catalog(
    conn: &rusqlite::Connection,
    app_hint: Option<&str>,
) -> rusqlite::Result<HashMap<String, u32>> {
    let mut catalog = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT signature, seen_count FROM chrome_text_catalog WHERE app_hint IS ?1")?;
    let mut rows = stmt.query(params![app_hint])?;
    while let Some(row) = rows.next()? {
        catalog.insert(
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?.max(0) as u32,
        );
    }
    Ok(catalog)
}

/// Foreground-window context already on the `frames` row (`07` #12): `(app_hint,
/// window_title)`, both `None` if the row is missing.
fn frame_target_context(
    conn: &rusqlite::Connection,
    frame_id: i64,
) -> rusqlite::Result<(Option<String>, Option<String>)> {
    Ok(conn
        .query_row(
            "SELECT app_hint, window_title FROM frames WHERE id = ?1",
            params![frame_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((None, None)))
}

/// Reads a frame's `text_spans` in `span_index` (reading) order, reconstructing each
/// [`TextSpan`] with its stored role/geometry. Sync (takes a borrowed connection or
/// transaction) so it can run inside the filter-version backfill transaction; the async
/// [`SqliteStore::frame_spans`] wraps it. Unknown role/source/suppress tokens fall back
/// conservatively (never lose a span).
fn read_text_spans(conn: &rusqlite::Connection, frame_id: i64) -> rusqlite::Result<Vec<TextSpan>> {
    let mut stmt = conn.prepare(
        "SELECT text, normalized_text, source, role, x, y, w, h,
                line_index, is_searchable, suppress_reason
         FROM text_spans WHERE frame_id = ?1 ORDER BY span_index",
    )?;
    let spans = stmt
        .query_map(params![frame_id], |r| {
            let source: String = r.get(2)?;
            let role: String = r.get(3)?;
            let suppress: Option<String> = r.get(10)?;
            Ok(TextSpan {
                text: r.get(0)?,
                normalized_text: r.get(1)?,
                source: TextSource::from_db_str(&source).unwrap_or(TextSource::Ocr),
                role: TextRole::from_db_str(&role).unwrap_or(TextRole::Unknown),
                x: r.get::<_, f64>(4)? as f32,
                y: r.get::<_, f64>(5)? as f32,
                w: r.get::<_, f64>(6)? as f32,
                h: r.get::<_, f64>(7)? as f32,
                line_index: r.get::<_, i64>(8)? as u32,
                is_searchable: r.get::<_, i64>(9)? != 0,
                suppress_reason: suppress.as_deref().and_then(SuppressReason::from_db_str),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(spans)
}

/// Collapses per-word spans into one span **per line** (`line_index`), preserving each
/// line's first-appearance order. The DB-shrink lever for degrade-to-text retention
/// (`07` #73a): a purged frame's ~100 per-word rows become ~10-20 per-line rows while
/// keeping everything `FrameReconstruction` needs — the line's words joined in reading
/// order, positioned at the union of their boxes. A line reads at `Content` strength if it
/// holds any content word (else it keeps a recessive role) and is searchable if any word
/// was. Idempotent: already line-level spans pass through unchanged.
fn merge_spans_to_lines(spans: &[TextSpan]) -> Vec<TextSpan> {
    // Group by line_index, remembering the order each line first appears so the merged
    // output stays in reading order regardless of the input's ordering.
    let mut order: Vec<u32> = Vec::new();
    let mut groups: HashMap<u32, Vec<&TextSpan>> = HashMap::new();
    for s in spans {
        groups
            .entry(s.line_index)
            .or_insert_with(|| {
                order.push(s.line_index);
                Vec::new()
            })
            .push(s);
    }
    order
        .into_iter()
        .map(|li| {
            let line = &groups[&li];
            let min_x = line.iter().map(|s| s.x).fold(f32::INFINITY, f32::min);
            let min_y = line.iter().map(|s| s.y).fold(f32::INFINITY, f32::min);
            let max_right = line
                .iter()
                .map(|s| s.x + s.w)
                .fold(f32::NEG_INFINITY, f32::max);
            let max_bottom = line
                .iter()
                .map(|s| s.y + s.h)
                .fold(f32::NEG_INFINITY, f32::max);
            let text = line
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let is_searchable = line.iter().any(|s| s.is_searchable);
            // Content wins the line's tone (keeps it legible in the reconstruction); an
            // all-chrome/system line keeps its first span's recessive role.
            let role = line
                .iter()
                .find(|s| s.role == TextRole::Content)
                .map_or(line[0].role, |s| s.role);
            TextSpan {
                normalized_text: traits::normalize_text(&text),
                text,
                source: line[0].source,
                role,
                x: min_x,
                y: min_y,
                w: (max_right - min_x).max(0.0),
                h: (max_bottom - min_y).max(0.0),
                line_index: li,
                is_searchable,
                // A searchable (content-bearing) line has no suppress reason; an all-recessive
                // line carries its first span's, matching the metric's role accounting.
                suppress_reason: if is_searchable {
                    None
                } else {
                    line[0].suppress_reason
                },
            }
        })
        .collect()
}

/// Replaces a frame's `text_spans` wholesale (idempotent re-OCR / re-filter). Shared by
/// [`SqliteStore::insert_ocr`] (passthrough, roles `unknown`) and
/// [`SqliteStore::insert_ocr_filtered`] (classified roles). Carries `line_index` so PR3
/// can reconstruct lines exactly (`03 §3b`).
fn replace_text_spans(
    tx: &rusqlite::Transaction<'_>,
    frame_id: i64,
    spans: &[TextSpan],
) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM text_spans WHERE frame_id = ?1",
        params![frame_id],
    )?;
    let mut stmt = tx.prepare(
        "INSERT INTO text_spans
           (frame_id, span_index, text, normalized_text, source, role,
            x, y, w, h, line_index, is_searchable, suppress_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    for (i, span) in spans.iter().enumerate() {
        stmt.execute(params![
            frame_id,
            i as i64,
            span.text,
            span.normalized_text,
            span.source.as_db_str(),
            span.role.as_db_str(),
            // Bind as f64 (the REAL column type); f32→f64→f32 is lossless.
            span.x as f64,
            span.y as f64,
            span.w as f64,
            span.h as f64,
            span.line_index as i64,
            span.is_searchable as i32,
            span.suppress_reason.map(|r| r.as_db_str()),
        ])?;
    }
    Ok(())
}

impl SqliteStore {
    /// Inserts a captured frame, returning its new id (`03 §3`).
    pub async fn insert_frame(&self, f: NewFrame) -> Result<i64> {
        self.with_conn(move |conn| {
            // Acquire the database writer lock before reading metadata/MAX(id),
            // not just the per-handle mutex: independent handles share this DB.
            let tx = rusqlite::Transaction::new_unchecked(
                conn,
                rusqlite::TransactionBehavior::Immediate,
            )?;
            let id = crate::history::allocate_frame_id(&tx)?;
            tx.execute(
                "INSERT INTO frames
                   (captured_at, monitor_index, width, height, image_path, content_hash,
                    app_hint, window_title, browser_url, capture_trigger, id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    f.captured_at,
                    f.monitor_index,
                    f.width,
                    f.height,
                    f.image_path,
                    f.content_hash,
                    f.app_hint,
                    f.window_title,
                    f.browser_url,
                    f.capture_trigger.map(|t| t.as_db_str()),
                    id,
                ],
            )?;
            tx.commit()?;
            Ok(id)
        })
        .await
    }

    /// Stores (or replaces) the text signal for a frame (`03 §3b`/`§4`): the
    /// `frame_text` row (raw + content text, both FTS-mirrored by the schema
    /// triggers) and the per-word `text_spans`. Atomic — all writes share one
    /// transaction, mirroring [`Self::insert_vision`].
    ///
    /// **Interim populator (PR2, `07` #51):** PR3's classifier isn't wired yet, so
    /// `content_text` is a passthrough copy of `raw_text` (the column is `NOT NULL`),
    /// `filter_version` is the unfiltered marker, and `suppressed_count` is 0. The
    /// foreground-window context already on the `frames` row (gap #12) is copied into
    /// `target_window_title` / `target_app_hint`. Re-OCR replaces spans wholesale
    /// (delete-then-insert) so it is idempotent.
    pub async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> Result<()> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;

            // Foreground-window metadata (gap #12); the frame row was inserted just
            // before this call (`03 §5`). A missing row leaves both NULL.
            let (target_app_hint, target_window_title) = frame_target_context(conn, frame_id)?;

            // `?2` (raw_text) is bound once and reused for content_text (passthrough).
            tx.execute(
                "INSERT INTO frame_text
                   (frame_id, raw_text, content_text, primary_source, filter_version,
                    suppressed_count, target_window_title, target_app_hint)
                 VALUES (?1, ?2, ?2, ?3, ?4, 0, ?5, ?6)
                 ON CONFLICT(frame_id) DO UPDATE SET
                   raw_text = excluded.raw_text,
                   content_text = excluded.content_text,
                   primary_source = excluded.primary_source,
                   filter_version = excluded.filter_version,
                   suppressed_count = excluded.suppressed_count,
                   target_window_title = excluded.target_window_title,
                   target_app_hint = excluded.target_app_hint",
                params![
                    frame_id,
                    ocr.text,
                    primary_source_for(&ocr.engine),
                    UNFILTERED_FILTER_VERSION,
                    target_window_title,
                    target_app_hint,
                ],
            )?;

            // Replace spans wholesale (idempotent re-OCR); PR2 roles stay `unknown`.
            replace_text_spans(&tx, frame_id, &ocr.spans)?;

            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Inserts OCR **and** applies PR3's attention filter in one transaction
    /// (`03 §3b`, `docs/0.2.0.md` PR3). Classifies spans into roles via the pure
    /// [`textfilter`] crate, writes the **filtered** `content_text` directly (so the
    /// content FTS index is written once — no transient unfiltered window a concurrent
    /// search could match), stores the classified `text_spans`, and bumps the
    /// `chrome_text_catalog` for the signatures observed this frame. `raw_text` is
    /// always preserved; the foreground title/app come from the `frames` row (the rect
    /// and thresholds come from `ctx`). Because the embed worker reads `content_text`
    /// and is enqueued only after this commits, embeddings run over filtered text.
    pub async fn insert_ocr_filtered(
        &self,
        frame_id: i64,
        ocr: OcrResult,
        ctx: TextFilterContext,
    ) -> Result<()> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let (app_hint, window_title) = frame_target_context(conn, frame_id)?;

            let config = FilterConfig {
                chrome_suppress_min_seen: ctx.chrome_suppress_min_seen,
                chrome_protect_min_chars: ctx.chrome_protect_min_chars,
                chrome_region_buckets: ctx.chrome_region_buckets,
            };
            let catalog = load_chrome_catalog(conn, app_hint.as_deref())?;
            let input = ClassifyInput {
                spans: &ocr.spans,
                target_rect: ctx.target_rect,
                target_window_title: window_title.as_deref(),
                target_app_hint: app_hint.as_deref(),
            };
            let out = classify(&input, &catalog, &config);

            // Single filtered write — content FTS mirror is written exactly once.
            tx.execute(
                "INSERT INTO frame_text
                   (frame_id, raw_text, content_text, primary_source, filter_version,
                    suppressed_count, target_window_title, target_app_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(frame_id) DO UPDATE SET
                   raw_text = excluded.raw_text,
                   content_text = excluded.content_text,
                   primary_source = excluded.primary_source,
                   filter_version = excluded.filter_version,
                   suppressed_count = excluded.suppressed_count,
                   target_window_title = excluded.target_window_title,
                   target_app_hint = excluded.target_app_hint",
                params![
                    frame_id,
                    ocr.text,
                    out.content_text,
                    primary_source_for(&ocr.engine),
                    FILTER_VERSION,
                    out.suppressed_count as i64,
                    window_title,
                    app_hint,
                ],
            )?;

            replace_text_spans(&tx, frame_id, &out.spans)?;

            // Bump the catalog for each candidate signature observed this frame. The
            // current appearance reaches `seen_count`, and `suppressed` is recomputed
            // against the configured threshold for observability / stats.
            {
                let min_seen = ctx.chrome_suppress_min_seen.max(1) as i64;
                let mut stmt = tx.prepare(
                    "INSERT INTO chrome_text_catalog
                       (signature, app_hint, region_bucket, normalized_text,
                        seen_count, first_seen_at, last_seen_at, suppressed)
                     VALUES (?1, ?2, ?3, ?4, 1, unixepoch()*1000, unixepoch()*1000,
                             CASE WHEN 1 >= ?5 THEN 1 ELSE 0 END)
                     ON CONFLICT(signature) DO UPDATE SET
                       seen_count   = seen_count + 1,
                       last_seen_at = unixepoch()*1000,
                       suppressed   = CASE WHEN seen_count + 1 >= ?5 THEN 1 ELSE 0 END",
                )?;
                for obs in &out.observed {
                    stmt.execute(params![
                        obs.signature,
                        obs.app_hint,
                        obs.region_bucket,
                        obs.normalized_text,
                        min_seen,
                    ])?;
                }
            }

            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Per-app text-filter suppression metric over frames classified by
    /// `filter_version` (`03 §3b`). `rate = suppressed_spans / total_spans` grouped by
    /// the **target** (foreground) app. Filtering on `filter_version` excludes interim
    /// passthrough frames so they can't dilute the rate to a misleading 0%.
    pub async fn text_filter_stats(&self, filter_version: i32) -> Result<Vec<AppSuppression>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT ft.target_app_hint,
                        COUNT(*) AS total,
                        SUM(CASE WHEN ts.role IN ('chrome','system','background')
                                 THEN 1 ELSE 0 END) AS suppressed
                 FROM text_spans ts
                 JOIN frame_text ft ON ft.frame_id = ts.frame_id
                 WHERE ft.filter_version = ?1
                 GROUP BY ft.target_app_hint
                 ORDER BY suppressed DESC, total DESC",
            )?;
            let rows = stmt.query_map(params![filter_version], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (app, total_spans, suppressed_spans) = row?;
                let rate = if total_spans > 0 {
                    suppressed_spans as f32 / total_spans as f32
                } else {
                    0.0
                };
                out.push(AppSuppression {
                    app,
                    total_spans,
                    suppressed_spans,
                    rate,
                });
            }
            Ok(out)
        })
        .await
    }

    /// Backfills the active attention `filter_version` (`03 §3b`,
    /// `docs/AUDIT_0.2.0_PR3_2026-06-26.md`). When the stored watermark differs from
    /// `current`, re-cleans every frame whose `filter_version < current` against the
    /// now-warm `chrome_text_catalog` via [`textfilter::reconcile`]: positional roles
    /// decided at capture are preserved, but a short repeated edge label that was kept
    /// during the catalog's cold-start window (before its signature crossed
    /// `chrome_suppress_min_seen`) is retroactively demoted to chrome and dropped from
    /// `content_text` (the content FTS re-syncs via its trigger). For frames whose
    /// `content_text` actually changed, an `embed_text` job is enqueued (when `reembed`)
    /// so the vector arm re-embeds from clean text; the embeddings `content_hash` makes
    /// an unchanged frame a no-op. Monotonic and idempotent (only ever suppresses more):
    /// each frame's `filter_version` advances as it is processed and the watermark is
    /// recorded only after the whole pass, so an interrupted run safely resumes.
    ///
    /// Each batch ([`BACKFILL_BATCH`]) runs in its own [`Self::with_conn`] call, so the
    /// store's single connection is **released between batches** — the background startup
    /// backfill never holds the DB lock across the whole run, keeping search / settings /
    /// capture inserts responsive on a large upgraded DB. The chrome catalog is snapshotted
    /// once up front (one query per distinct `app_hint`) and reused for every batch instead
    /// of being re-read per frame; concurrent capture only warms catalogs for *new* frames
    /// (already at `current`), which this pass doesn't touch, so the snapshot is sound.
    /// Returns the number of frames whose `content_text` changed; a no-op returning `0`
    /// when the watermark already equals `current`. Run once at startup.
    pub async fn backfill_filter_version(
        &self,
        current: i32,
        chrome_suppress_min_seen: u32,
        chrome_protect_min_chars: u32,
        chrome_region_buckets: u32,
        reembed: bool,
    ) -> Result<u64> {
        // One short-lived lock: bail if already current, else list the sub-version frames
        // (oldest first) and snapshot every distinct app_hint's chrome catalog. `None` =>
        // watermark already current (nothing to do, no watermark write); `Some` => proceed
        // (and advance the watermark afterwards even if no frame ends up changing).
        type CatalogSnapshot = HashMap<Option<String>, HashMap<String, u32>>;
        let plan: Option<(Vec<i64>, CatalogSnapshot)> = self
            .with_conn(move |conn| {
                let stored: Option<i32> = conn
                    .query_row(
                        "SELECT value FROM settings WHERE key = ?1",
                        params![CATALOG_FILTER_VERSION_KEY],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                    .and_then(|v| v.parse().ok());
                if stored == Some(current) {
                    return Ok(None);
                }

                let frame_ids: Vec<i64> = {
                    let mut stmt = conn.prepare(
                        "SELECT frame_id FROM frame_text WHERE filter_version < ?1 ORDER BY frame_id",
                    )?;
                    let ids = stmt
                        .query_map(params![current], |r| r.get::<_, i64>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    ids
                };

                // Snapshot the catalog per distinct app_hint once (vs. one query per frame).
                let mut catalogs: CatalogSnapshot = HashMap::new();
                {
                    let mut stmt = conn.prepare(
                        "SELECT DISTINCT target_app_hint FROM frame_text WHERE filter_version < ?1",
                    )?;
                    let hints = stmt
                        .query_map(params![current], |r| r.get::<_, Option<String>>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    for hint in hints {
                        let cat = load_chrome_catalog(conn, hint.as_deref())?;
                        catalogs.insert(hint, cat);
                    }
                }
                Ok(Some((frame_ids, catalogs)))
            })
            .await?;

        let Some((frame_ids, catalogs)) = plan else {
            return Ok(0);
        };
        let catalogs = std::sync::Arc::new(catalogs);

        let mut changed_total: u64 = 0;
        for batch in frame_ids.chunks(BACKFILL_BATCH) {
            // Each batch takes the store connection on its own, then drops it — so other
            // DB work interleaves between batches instead of waiting out the whole pass.
            let batch: Vec<i64> = batch.to_vec();
            let catalogs = std::sync::Arc::clone(&catalogs);
            let changed = self
                .with_conn(move |conn| {
                    let config = FilterConfig {
                        chrome_suppress_min_seen,
                        chrome_protect_min_chars,
                        chrome_region_buckets,
                    };
                    let empty = HashMap::new();
                    let tx = conn.unchecked_transaction()?;
                    let mut changed = 0u64;
                    for &fid in &batch {
                        let (app_hint, old_content): (Option<String>, String) = tx.query_row(
                            "SELECT target_app_hint, content_text FROM frame_text WHERE frame_id = ?1",
                            params![fid],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )?;
                        let catalog = catalogs.get(&app_hint).unwrap_or(&empty);
                        let spans = read_text_spans(&tx, fid)?;
                        let out = reconcile(&spans, app_hint.as_deref(), catalog, &config);

                        if out.content_text != old_content {
                            tx.execute(
                                "UPDATE frame_text
                                 SET content_text = ?2, suppressed_count = ?3, filter_version = ?4
                                 WHERE frame_id = ?1",
                                params![fid, out.content_text, out.suppressed_count as i64, current],
                            )?;
                            replace_text_spans(&tx, fid, &out.spans)?;
                            // Invalidate the now-stale text embedding so the dropped chrome
                            // terms can't keep surfacing the frame through hybrid_search's
                            // vector arm (which fuses even when include_chrome=false) before
                            // the re-embed runs — or indefinitely if text embedding is off.
                            // The embeddings_ad trigger cascades to the vec0 shadow; the
                            // embed_text job (when reembed) rebuilds it from the new content.
                            tx.execute(
                                "DELETE FROM embeddings WHERE frame_id = ?1 AND source = 'ocr'",
                                params![fid],
                            )?;
                            changed += 1;
                            if reembed {
                                tx.execute(
                                    "INSERT INTO jobs (kind, frame_id, priority, max_attempts, not_before)
                                     VALUES ('embed_text', ?1, 0, 3, 0)",
                                    params![fid],
                                )?;
                            }
                        } else {
                            // No chrome to drop — just advance the version so it isn't rescanned.
                            tx.execute(
                                "UPDATE frame_text SET filter_version = ?2 WHERE frame_id = ?1",
                                params![fid, current],
                            )?;
                        }
                    }
                    tx.commit()?;
                    Ok(changed)
                })
                .await?;
            changed_total += changed;
        }

        // Advance the watermark after the whole pass (its own short lock).
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![CATALOG_FILTER_VERSION_KEY, current.to_string()],
            )?;
            Ok(())
        })
        .await?;
        Ok(changed_total)
    }

    /// Stores (or replaces) the deferred vision analysis for a frame (`03 §5`).
    pub async fn insert_vision(&self, frame_id: i64, v: VisionAnalysis) -> Result<()> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO vision_analysis
                   (frame_id, description, activity_type, app_hint, confidence, model)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(frame_id) DO UPDATE SET
                   description = excluded.description,
                   activity_type = excluded.activity_type,
                   app_hint = excluded.app_hint,
                   confidence = excluded.confidence,
                   model = excluded.model",
                params![
                    frame_id,
                    v.description,
                    v.activity_type,
                    v.app_hint,
                    v.confidence,
                    v.model,
                ],
            )?;
            // 03 §4: frames.activity_type is "filled by vision" — mirror the
            // classification onto the frame so the timeline can filter by activity
            // without joining vision_analysis.
            tx.execute(
                "UPDATE frames SET activity_type = ?2 WHERE id = ?1",
                params![frame_id, v.activity_type],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// The minimal inputs the embedding worker needs to enrich a frame — the stored
    /// JPEG's relative path and the **content text** (if any) — in one round-trip, or
    /// `None` if the frame no longer exists (`03 §5`). Lighter than [`Self::get_frame`]
    /// (no vision/tags), so the worker doesn't pay for context it won't embed.
    ///
    /// Embeddings run over `content_text` (`03 §3b`); in PR2 that equals `raw_text`
    /// (passthrough), so behavior is unchanged until PR3's filter lands. The field is
    /// still called `ocr_text` on [`FrameEnrichmentInput`] to keep the worker stable.
    pub async fn frame_enrichment_input(
        &self,
        frame_id: i64,
    ) -> Result<Option<FrameEnrichmentInput>> {
        self.with_conn(move |conn| {
            let row = conn
                .query_row(
                    "SELECT f.image_path, ft.content_text
                     FROM frames f LEFT JOIN frame_text ft ON ft.frame_id = f.id
                     WHERE f.id = ?1",
                    params![frame_id],
                    |r| {
                        Ok(FrameEnrichmentInput {
                            image_path: r.get(0)?,
                            ocr_text: r.get(1)?,
                        })
                    },
                )
                .optional()?;
            Ok(row)
        })
        .await
    }

    /// Frame ids with no `vision_analysis` row yet **and** no `vision_tag` job in a
    /// `pending`/`running`/`dead` state, oldest first, capped at `limit`, optionally
    /// within `[start, end)` capture time. Feeds the timer/idle vision batch and the
    /// `enqueue_vision` range target (`03 §5`). Excluding `pending`/`running` stops a
    /// slow batch from being re-enqueued on the next tick and the timer/idle lanes from
    /// double-queuing the same frames; excluding `dead` stops a poisoned frame (its job
    /// exhausted retries without ever writing a `vision_analysis` row) from being
    /// re-enqueued every tick forever. A `done` job does **not** exclude a frame (a job
    /// that finished without persisting analysis is eligible to retry); on-demand
    /// single-frame re-tagging bypasses this query, so a dead frame can still be forced.
    pub async fn untagged_frame_ids(
        &self,
        limit: u32,
        range: Option<(i64, i64)>,
    ) -> Result<Vec<i64>> {
        self.with_conn(move |conn| {
            // Build the query once, appending the optional time-window predicate and
            // binding every value as an anonymous positional `?` (in push order). The
            // NOT EXISTS guard skips frames whose `vision_tag` job is still in flight.
            let mut sql = String::from(
                "SELECT f.id FROM frames f
                 LEFT JOIN vision_analysis v ON v.frame_id = f.id
                 WHERE v.frame_id IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM jobs j
                       WHERE j.frame_id = f.id
                         AND j.kind = 'vision_tag'
                         AND j.state IN ('pending', 'running', 'dead')
                   )",
            );
            let mut args: Vec<i64> = Vec::new();
            if let Some((start, end)) = range {
                sql.push_str(" AND f.captured_at >= ? AND f.captured_at < ?");
                args.push(start);
                args.push(end);
            }
            sql.push_str(" ORDER BY f.captured_at ASC LIMIT ?");
            args.push(i64::from(limit));

            let mut stmt = conn.prepare(&sql)?;
            let ids = stmt
                .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(ids)
        })
        .await
    }

    /// Bulk-fetches **content text** for many frames in one `IN (…)` query (the `ask`
    /// grounding hydrate, `03 §7/§13.5`). Grounding uses content text (`03 §3b`); only
    /// frames with non-empty content are returned.
    pub async fn ocr_texts(&self, frame_ids: &[i64]) -> Result<HashMap<i64, String>> {
        if frame_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = frame_ids.to_vec();
        self.with_conn(move |conn| {
            let placeholders = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "SELECT frame_id, content_text FROM frame_text
                 WHERE frame_id IN ({placeholders}) AND content_text <> ''"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            let mut map = HashMap::new();
            for row in rows {
                let (id, text) = row?;
                map.insert(id, text);
            }
            Ok(map)
        })
        .await
    }

    /// Assembles the full per-frame detail (frame context + raw/content text +
    /// vision + tags), or `None` if the frame does not exist. Backs the `get_frame`
    /// command (`03 §7`). Raw text stays viewable here even though retrieval defaults
    /// to content text (`03 §3b`).
    pub async fn get_frame(&self, frame_id: i64) -> Result<Option<FrameDetail>> {
        self.with_conn(move |conn| {
            let base = conn
                .query_row(
                    "SELECT captured_at, monitor_index, width, height, image_path,
                            app_hint, window_title, browser_url, activity_type, capture_trigger,
                            image_purged
                     FROM frames WHERE id = ?1",
                    params![frame_id],
                    |r| {
                        Ok(FrameDetail {
                            frame_id,
                            captured_at: r.get(0)?,
                            monitor_index: r.get(1)?,
                            width: r.get(2)?,
                            height: r.get(3)?,
                            image_path: r.get(4)?,
                            image_purged: r.get::<_, i64>(10)? != 0,
                            app_hint: r.get(5)?,
                            window_title: r.get(6)?,
                            browser_url: r.get(7)?,
                            activity_type: r.get(8)?,
                            // NULL (legacy frame) or an unknown token → None.
                            capture_trigger: r
                                .get::<_, Option<String>>(9)?
                                .as_deref()
                                .and_then(CaptureTrigger::from_db_str),
                            raw_text: None,
                            content_text: None,
                            text_source: TextSource::Ocr,
                            suppressed_text_count: 0,
                            vision: None,
                            tags: Vec::new(),
                        })
                    },
                )
                .optional()?;

            let Some(mut detail) = base else {
                return Ok(None);
            };

            if let Some((raw, content, source, suppressed)) = conn
                .query_row(
                    "SELECT raw_text, content_text, primary_source, suppressed_count
                     FROM frame_text WHERE frame_id = ?1",
                    params![frame_id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()?
            {
                detail.raw_text = Some(raw);
                detail.content_text = Some(content);
                detail.text_source = TextSource::from_db_str(&source).unwrap_or(TextSource::Ocr);
                detail.suppressed_text_count = suppressed.max(0) as u32;
            }

            detail.vision = conn
                .query_row(
                    "SELECT description, activity_type, app_hint, confidence, model
                     FROM vision_analysis WHERE frame_id = ?1",
                    params![frame_id],
                    |r| {
                        Ok(VisionAnalysis {
                            description: r.get(0)?,
                            activity_type: r.get(1)?,
                            app_hint: r.get(2)?,
                            confidence: r.get(3)?,
                            model: r.get(4)?,
                        })
                    },
                )
                .optional()?;

            let mut stmt = conn.prepare(
                "SELECT t.name FROM frame_tags ft
                 JOIN tags t ON t.id = ft.tag_id
                 WHERE ft.frame_id = ?1 ORDER BY t.name",
            )?;
            detail.tags = stmt
                .query_map(params![frame_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(Some(detail))
        })
        .await
    }

    /// One keyset-cursor page of frames for the streaming JSON export (0.3.0 PR7;
    /// `03 §7c`, D12). Returns frames with `id > after_id`, ascending, LEFT JOINed to
    /// `frame_text` for the attention-filtered `content_text` (never `raw_text`, never
    /// image bytes), optionally bounded to the half-open window
    /// `from <= captured_at < to`, up to `limit` rows.
    ///
    /// Keyset (`id > after_id ORDER BY id ASC`) rather than OFFSET so the export is
    /// stable and flat-memory over an arbitrarily large history: the `api::export`
    /// stream calls this once per page, advancing `after_id` to the last returned id,
    /// so the single store connection is taken and released **per page** (like
    /// [`Self::purged_frame_ids`]) and never held across the whole response — capture
    /// and search keep their turn at the connection. `limit == 0` → empty.
    pub async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> Result<Vec<ExportFrameRow>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT f.id, f.captured_at, f.app_hint, f.window_title, f.browser_url,
                        f.activity_type, t.content_text
                 FROM frames f
                 LEFT JOIN frame_text t ON t.frame_id = f.id
                 WHERE f.id > ?1
                   AND (?2 IS NULL OR f.captured_at >= ?2)
                   AND (?3 IS NULL OR f.captured_at < ?3)
                 ORDER BY f.id ASC
                 LIMIT ?4",
            )?;
            let rows = stmt
                .query_map(params![after_id, from, to, i64::from(limit)], |r| {
                    Ok(ExportFrameRow {
                        frame_id: r.get(0)?,
                        captured_at: r.get(1)?,
                        app_hint: r.get(2)?,
                        window_title: r.get(3)?,
                        browser_url: r.get(4)?,
                        activity_type: r.get(5)?,
                        content_text: r.get(6)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// All text spans for a frame, ordered by `span_index` (`03 §3b`/`§4`). An
    /// inherent read that makes the `text_spans` write path observable — like
    /// [`Self::get_frame`] / `delete_frame` (`07` engineering note) — and the read
    /// PR3's classifier uses to recompute roles. Empty when the frame has no spans.
    pub async fn frame_spans(&self, frame_id: i64) -> Result<Vec<TextSpan>> {
        self.with_conn(move |conn| Ok(read_text_spans(conn, frame_id)?))
            .await
    }

    /// Collapses a frame's per-word `text_spans` into per-line spans, in place (`07` #73a).
    /// Called after a frame's screenshot is retention-purged (and by the one-time backfill)
    /// so degrade-to-text shrinks the **DB** too, not just disk: ~100 per-word rows become
    /// ~10-20 per-line rows. Non-destructive to search (FTS reads `frame_text.content_text`,
    /// the vector arm reads `embeddings` — both untouched) and to `FrameReconstruction`,
    /// which still gets each line's text at its on-screen box. Idempotent; a spanless frame
    /// is a no-op. One transaction so a reader never sees a half-merged frame.
    pub async fn merge_frame_spans_to_lines(&self, frame_id: i64) -> Result<()> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let spans = read_text_spans(&tx, frame_id)?;
            if spans.is_empty() {
                return Ok(());
            }
            let merged = merge_spans_to_lines(&spans);
            replace_text_spans(&tx, frame_id, &merged)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Atomically degrade a frame to text: in **one transaction**, collapse its per-word
    /// `text_spans` to per-line (`merge_spans_to_lines`) **and** set `image_purged = 1`. The
    /// caller deletes the on-disk image file first; this records the degrade in the DB.
    ///
    /// The atomicity is the point (`07` #73a, Codex review): the retention sweep only re-lists
    /// frames with `image_purged = 0`, so if the span merge and the purge flag were two separate
    /// writes and the merge failed *after* the flag was set, that frame would be excluded from
    /// every future sweep with its per-word rows stranded — defeating the DB-shrink guarantee for
    /// exactly the transient failures (SQLite busy/lock) the caller treats as recoverable. Merging
    /// and flag-setting in the same transaction means the frame is marked purged **iff** its spans
    /// were merged; on any failure nothing commits, `image_purged` stays `0`, and the next sweep
    /// retries the whole frame. Idempotent: re-degrading a frame whose spans are already per-line
    /// is a no-op merge plus an unchanged `image_purged = 1` UPDATE.
    pub async fn degrade_frame_to_text(&self, frame_id: i64) -> Result<()> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let spans = read_text_spans(&tx, frame_id)?;
            if !spans.is_empty() {
                let merged = merge_spans_to_lines(&spans);
                replace_text_spans(&tx, frame_id, &merged)?;
            }
            tx.execute(
                "UPDATE frames SET image_purged = 1 WHERE id = ?1",
                params![frame_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStore;

    #[test]
    fn primary_source_for_maps_engine_to_db_token() {
        assert_eq!(primary_source_for("uia"), "uia");
        assert_eq!(primary_source_for("winrt"), "ocr");
        assert_eq!(primary_source_for("anything-else"), "ocr");
    }

    fn new_frame() -> NewFrame {
        NewFrame {
            captured_at: 1,
            monitor_index: 0,
            width: 10,
            height: 10,
            image_path: "p.jpg".to_string(),
            content_hash: "h".to_string(),
            app_hint: None,
            window_title: None,
            browser_url: None,
            capture_trigger: Some(CaptureTrigger::Timer),
        }
    }

    fn ctx() -> TextFilterContext {
        TextFilterContext {
            target_rect: None,
            chrome_suppress_min_seen: 4,
            chrome_protect_min_chars: 48,
            chrome_region_buckets: 8,
        }
    }

    fn result(engine: &str) -> OcrResult {
        OcrResult {
            text: "hello world".to_string(),
            mean_confidence: -1.0,
            engine: engine.to_string(),
            spans: Vec::new(),
        }
    }

    /// Builds a per-word span on `line` at the given normalized `(x, y, w, h)` box/role.
    fn sp(
        text: &str,
        line: u32,
        bbox: (f32, f32, f32, f32),
        role: TextRole,
        searchable: bool,
    ) -> TextSpan {
        let (x, y, w, h) = bbox;
        TextSpan {
            normalized_text: traits::normalize_text(text),
            text: text.to_string(),
            source: TextSource::Ocr,
            role,
            x,
            y,
            w,
            h,
            line_index: line,
            is_searchable: searchable,
            suppress_reason: None,
        }
    }

    /// `merge_spans_to_lines` collapses the per-word spans of a purged frame into one span
    /// per line (the DB-shrink lever for degrade-to-text, `07` #73a) while keeping enough for
    /// `FrameReconstruction`: each line's words joined in order, at the union of their boxes.
    #[test]
    fn merge_spans_to_lines_collapses_words_and_unions_boxes() {
        let spans = vec![
            sp("hello", 0, (0.0, 0.0, 0.10, 0.05), TextRole::Content, true),
            sp("world", 0, (0.12, 0.0, 0.10, 0.05), TextRole::Content, true),
            sp("footer", 1, (0.0, 0.20, 0.10, 0.04), TextRole::Chrome, true),
        ];
        let merged = merge_spans_to_lines(&spans);
        assert_eq!(merged.len(), 2, "two lines → two spans");
        assert_eq!(merged[0].text, "hello world");
        assert_eq!(merged[0].line_index, 0);
        assert_eq!(merged[0].x, 0.0);
        assert_eq!(merged[0].y, 0.0);
        // union width = rightmost edge (0.12 + 0.10) − leftmost x (0.0)
        assert!(
            (merged[0].w - 0.22).abs() < 1e-6,
            "union width, got {}",
            merged[0].w
        );
        assert!((merged[0].h - 0.05).abs() < 1e-6);
        assert_eq!(merged[1].text, "footer");
        assert_eq!(merged[1].line_index, 1);
    }

    /// Merging is idempotent: applying it to already line-level spans returns them unchanged
    /// (so a re-run at purge time / in the backfill never degrades an already-merged frame).
    #[test]
    fn merge_spans_to_lines_is_idempotent() {
        let spans = vec![
            sp(
                "hello world",
                0,
                (0.0, 0.0, 0.22, 0.05),
                TextRole::Content,
                true,
            ),
            sp("footer", 1, (0.0, 0.20, 0.10, 0.04), TextRole::Chrome, true),
        ];
        let once = merge_spans_to_lines(&spans);
        let twice = merge_spans_to_lines(&once);
        assert_eq!(once.len(), twice.len());
        for (a, b) in once.iter().zip(&twice) {
            assert_eq!(a.text, b.text);
            assert_eq!(a.line_index, b.line_index);
            assert!((a.x - b.x).abs() < 1e-6 && (a.w - b.w).abs() < 1e-6);
        }
    }

    /// Empty in → empty out (a frame with no recognized text).
    #[test]
    fn merge_spans_to_lines_empty_is_empty() {
        assert!(merge_spans_to_lines(&[]).is_empty());
    }

    /// Role/searchable precedence: a line reads at Content strength if it holds any content
    /// word (so the reconstruction keeps it legible), and is searchable if any word was; an
    /// all-chrome line keeps a recessive role.
    #[test]
    fn merge_spans_to_lines_prefers_content_role() {
        let spans = vec![
            sp("Inbox", 0, (0.0, 0.0, 0.08, 0.04), TextRole::Chrome, false),
            sp(
                "quarterly",
                0,
                (0.10, 0.0, 0.12, 0.04),
                TextRole::Content,
                true,
            ),
            sp(
                "Settings",
                1,
                (0.0, 0.20, 0.08, 0.04),
                TextRole::Chrome,
                false,
            ),
        ];
        let merged = merge_spans_to_lines(&spans);
        assert_eq!(
            merged[0].role,
            TextRole::Content,
            "content word lifts the line to Content"
        );
        assert!(merged[0].is_searchable, "line is searchable if any word is");
        assert_eq!(
            merged[1].role,
            TextRole::Chrome,
            "all-chrome line stays recessive"
        );
        assert!(!merged[1].is_searchable);
    }

    /// `insert_ocr_filtered` must record `frame_text.primary_source` from the recognizer's
    /// engine, so `FrameDetail.text_source` tells the user whether UIA or OCR produced the
    /// text (a frame that fell back to OCR reports `ocr`).
    #[tokio::test]
    async fn filtered_insert_records_text_source_from_engine() {
        let store = SqliteStore::open_in_memory().expect("open store");

        let uia_id = store.insert_frame(new_frame()).await.expect("insert frame");
        store
            .insert_ocr_filtered(uia_id, result("uia"), ctx())
            .await
            .expect("insert uia text");
        let uia = store
            .get_frame(uia_id)
            .await
            .expect("get_frame")
            .expect("frame exists");
        assert_eq!(uia.text_source, TextSource::Uia);

        let ocr_id = store.insert_frame(new_frame()).await.expect("insert frame");
        store
            .insert_ocr_filtered(ocr_id, result("winrt"), ctx())
            .await
            .expect("insert ocr text");
        let ocr = store
            .get_frame(ocr_id)
            .await
            .expect("get_frame")
            .expect("frame exists");
        assert_eq!(ocr.text_source, TextSource::Ocr);
    }
}
