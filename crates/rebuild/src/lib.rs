//! # `ssv2c-rebuild` - dev-only v13 → v14 database rebuild (0.4.5 D3)
//!
//! The app ships **no migration chain** (0.4.5 D2): a database whose
//! `schema_version` is not 14 is refused, and this tool is the documented way to
//! carry a `v13` history into a fresh v14 file (`specs/03 §4`).
//!
//! Guarantees this crate implements:
//!
//! - The **source is opened `SQLITE_OPEN_READ_ONLY`** and is never written: no
//!   mutating pragma, no `VACUUM`, no WAL checkpoint, no rename. Its bytes and
//!   modification time are unchanged by a run (asserted by the crate's test fence).
//! - The **destination must not exist**; it is created from
//!   [`store::create_baseline`], the single source of truth for the v14 DDL.
//! - Rows are copied **generically**: for every carry table the source and
//!   destination column names are intersected via `PRAGMA table_info`, and only
//!   that intersection is selected and inserted. There is no v13 DDL and no
//!   per-version branch here.
//! - FTS5 is repopulated from the copied `frame_text` rows (`…_fts(t) VALUES
//!   ('rebuild')`) and `vec0` receives the embedding rows keyed by the copied
//!   `embeddings` rows - shadow tables are never copied.
//! - Settings follow the settings-v2 contract ([`settings::SettingsPolicy`]):
//!   `UI` + `file-only` keys, the `memory.*` keys owned by `03 §8c/§8d`, the API
//!   read credential, and the **reserved** history identity and allocator
//!   high-water, which are copied verbatim and never recomputed.
//! - The result is verified, not assumed: `schema_version = 14`,
//!   `PRAGMA integrity_check` is `ok`, and `PRAGMA foreign_key_check` is empty.
//!
//! The tool never renames the source. `*.v13.bak` is a maintainer step after the
//! new file is confirmed to open (0.4.5 D3); no flag here performs it.

use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{bail, Context, Result};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension, Transaction};
use traits::{FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY};

pub mod settings;

pub use settings::SettingsPolicy;

/// The binary name (also the refusal message the app prints, `03 §4`).
pub const BINARY_NAME: &str = "ssv2c-rebuild";

/// The only source schema this tool is documented to read (`03 §4`, 0.4.5 D3).
pub const SOURCE_SCHEMA_VERSION: i32 = 13;

/// Tables copied as rows, parent-first so the copy is readable even with foreign
/// keys enforced (they are switched off for the copy and checked afterwards).
///
/// `jobs` and `chrome_text_catalog` are deliberately absent: the first is a
/// transient queue and the second is derived from spans (the kickoff names both).
/// Every other pre-existing content table that is neither transient nor a derived
/// index is carried, so a rebuild never silently deletes user or OCR/vision data
/// the kickoff's minimum list does not name: `text_spans` (per-frame OCR/UIA spans),
/// `vision_analysis` (expensive model output), `tags`/`frame_tags` (user tags) and
/// `report_deliveries` (pending delivery state for the carried runs).
pub const CARRY_TABLES: &[&str] = &[
    "sessions",
    "frames",
    "frame_text",
    "text_spans",
    "vision_analysis",
    "embeddings",
    "marks",
    "session_artifacts",
    "tags",
    "frame_tags",
    "report_runs",
    "report_deliveries",
    "report_files",
];

/// External-content FTS5 mirrors repopulated from the copied rows.
pub const FTS_REBUILD_TABLES: &[&str] = &["frame_text_fts", "frame_text_raw_fts"];

/// Registers the `sqlite-vec` (`vec0`) extension for every connection opened after
/// this call, exactly once per process.
///
/// This mirrors `store`'s registration (the store calls it before opening its own
/// connection). The rebuild needs it independently because it drives its source and
/// destination connections with plain `rusqlite`, and the test fence builds its
/// synthetic fixture the same way.
pub fn register_vec_extension() {
    static VEC_INIT: Once = Once::new();
    VEC_INIT.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the extension entry point with the C ABI
        // `sqlite3_auto_extension` expects; the transmute is SQLite's documented
        // registration pattern (same one `store::register_vec_extension` uses).
        type ExtInit = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), ExtInit>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// One rebuilt database.
#[derive(Debug, Clone)]
pub struct RebuildOptions {
    /// The v13 database to read. Opened read-only.
    pub source: PathBuf,
    /// The v14 database to create. Must not exist.
    pub dest: PathBuf,
    /// How many random frame ids to print from the rebuilt file (`--sample-frames`).
    pub sample_frames: Option<usize>,
}

/// Per-table result of a carry copy.
#[derive(Debug, Clone)]
pub struct TableCopy {
    /// Table name.
    pub table: String,
    /// Rows read from the source.
    pub source_rows: i64,
    /// Rows present in the destination after the copy.
    pub dest_rows: i64,
}

/// Everything a run reports, so the CLI and the test fence read the same facts.
#[derive(Debug)]
pub struct RebuildReport {
    /// `schema_version` found in the source (13).
    pub source_version: i32,
    /// `schema_version` written to the destination (14).
    pub dest_version: i32,
    /// Ordinary-table row counts of the source, shadows excluded.
    pub source_counts: Vec<(String, i64)>,
    /// Ordinary-table row counts of the destination, shadows excluded.
    pub dest_counts: Vec<(String, i64)>,
    /// The carry-set copy, table by table.
    pub carried: Vec<TableCopy>,
    /// `embeddings` rows whose `vec0` vector was inserted.
    pub embedding_vectors: i64,
    /// Settings rows carried into the destination.
    pub settings_copied: usize,
    /// Settings rows deliberately not carried (retired or outside the contract).
    pub settings_dropped: Vec<String>,
    /// `frames.image_path` values that do not resolve on disk. The rows and their
    /// paths are still copied - the tool reports, it never silently drops a path.
    pub missing_images: Vec<String>,
    /// `PRAGMA integrity_check` rows; exactly `["ok"]` on success.
    pub integrity_check: Vec<String>,
    /// `PRAGMA foreign_key_check` row count; zero on success.
    pub foreign_key_violations: i64,
    /// Random frame ids from the rebuilt file (`--sample-frames`).
    pub sample_frames: Vec<i64>,
    /// Non-fatal observations worth showing the operator (an allocator below
    /// `MAX(frames.id)`, embeddings without vectors, …).
    pub warnings: Vec<String>,
}

/// Rebinds `source` into a fresh v14 database at `dest`.
///
/// The destination is removed again (best effort) if any step fails, so a failed run
/// does not intentionally leave a half-written file behind.
pub fn rebuild(options: &RebuildOptions) -> Result<RebuildReport> {
    if !options.source.is_file() {
        bail!(
            "source database {} does not exist or is not a regular file",
            options.source.display()
        );
    }
    if options.dest.exists() {
        bail!(
            "destination {} already exists; the rebuild only writes a fresh file \
             (delete or rename it first)",
            options.dest.display()
        );
    }
    let parent = options
        .dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        bail!("destination directory {} does not exist", parent.display());
    }

    register_vec_extension();
    let source = open_read_only(&options.source)?;
    let source_version = schema_version_of(&source)
        .with_context(|| format!("read schema_version from {}", options.source.display()))?;
    if source_version != SOURCE_SCHEMA_VERSION {
        bail!(
            "source schema_version is {source_version}; {BINARY_NAME} reads a v{SOURCE_SCHEMA_VERSION} \
             database into the v{} baseline",
            store::LATEST_SCHEMA_VERSION
        );
    }
    let warnings = validate_reserved_metadata(&source)?;

    let source_counts = counts_on(&source)?;
    let outcome = create_and_copy(options, &source, source_version, source_counts, warnings);
    if outcome.is_err() {
        // `dest` did not exist before this run, so nothing of value is removed.
        let _ = std::fs::remove_file(&options.dest);
    }
    outcome
}

fn create_and_copy(
    options: &RebuildOptions,
    source: &Connection,
    source_version: i32,
    source_counts: Vec<(String, i64)>,
    mut warnings: Vec<String>,
) -> Result<RebuildReport> {
    let mut dest = Connection::open(&options.dest)
        .with_context(|| format!("create destination {}", options.dest.display()))?;
    store::create_baseline(&dest).context("create the v14 baseline at the destination")?;
    // Rows are copied parent-first anyway; disabling enforcement lets an unusual
    // source order succeed and the explicit `foreign_key_check` below is the real
    // verification, not the insert-time behaviour.
    dest.execute_batch("PRAGMA foreign_keys = OFF;")?;

    let mut carried = Vec::new();
    let (settings_copied, settings_dropped, embedding_vectors) = {
        let tx = dest.transaction()?;
        for table in CARRY_TABLES {
            // A carry table missing from either side means the rebuild would silently
            // drop carried history (a truncated v13 source, or a baseline gap). That is
            // a fidelity failure, not a table to skip (P3.md stop condition).
            if !table_exists(source, table)? {
                bail!(
                    "source database is missing the carry table `{table}`; refusing to \
                     rebuild a partial history"
                );
            }
            if !table_exists(&tx, table)? {
                bail!("destination baseline is missing the carry table `{table}`");
            }
            let source_rows = copy_table(source, &tx, table)?;
            carried.push(TableCopy {
                table: (*table).to_string(),
                source_rows,
                dest_rows: count_on(&tx, table)?,
            });
        }
        rebuild_fts(&tx)?;
        let (copied, dropped) = copy_settings(source, &tx)?;
        let vectors = copy_embedding_vectors(source, &tx, &mut warnings)?;
        tx.commit()?;
        (copied, dropped, vectors)
    };

    let dest_version = schema_version_of(&dest)?;
    if dest_version != store::LATEST_SCHEMA_VERSION {
        bail!(
            "destination schema_version is {dest_version}, expected {}",
            store::LATEST_SCHEMA_VERSION
        );
    }

    let integrity_check = integrity_check(&dest)?;
    if integrity_check != ["ok"] {
        bail!(
            "destination failed PRAGMA integrity_check: {}",
            integrity_check.join("; ")
        );
    }
    let foreign_key_violation_rows = foreign_key_violation_rows(&dest)?;
    if !foreign_key_violation_rows.is_empty() {
        bail!(
            "destination has {} PRAGMA foreign_key_check violation row(s): {}",
            foreign_key_violation_rows.len(),
            foreign_key_violation_rows.join("; ")
        );
    }
    let foreign_key_violations = foreign_key_violation_rows.len() as i64;
    verify_reserved_metadata(source, &dest)?;

    let embeddings: i64 = count_on(&dest, "embeddings")?;
    if embedding_vectors != embeddings {
        warnings.push(format!(
            "{embeddings} embedding row(s) but {embedding_vectors} vec0 vector(s) copied; \
             the vector index is incomplete in the rebuilt file"
        ));
    }

    let missing_images = missing_images(&options.source, &dest)?;
    let sample_frames = match options.sample_frames {
        Some(limit) => sample_frame_ids_on(&dest, limit)?,
        None => Vec::new(),
    };
    let dest_counts = counts_on(&dest)?;

    Ok(RebuildReport {
        source_version,
        dest_version,
        source_counts,
        dest_counts,
        carried,
        embedding_vectors,
        settings_copied,
        settings_dropped,
        missing_images,
        integrity_check,
        foreign_key_violations,
        sample_frames,
        warnings,
    })
}

/// Reads `PRAGMA table_info` in declaration order.
fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(table)))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// Copies every row of `table`, selecting only the columns both sides declare.
///
/// This is the whole version handling: v13 → v14 is a column intersection, never a
/// restated DDL or a per-version branch.
fn copy_table(source: &Connection, tx: &Transaction<'_>, table: &str) -> Result<i64> {
    let source_columns: std::collections::BTreeSet<String> =
        table_columns(source, table)?.into_iter().collect();
    let columns: Vec<String> = table_columns(tx, table)?
        .into_iter()
        .filter(|column| source_columns.contains(column))
        .collect();
    if columns.is_empty() {
        bail!("table `{table}` has no column shared by the source and the destination");
    }
    let column_list = columns
        .iter()
        .map(|column| quote_ident(column))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let select = format!("SELECT {column_list} FROM {}", quote_ident(table));
    let insert = format!(
        "INSERT INTO {} ({column_list}) VALUES ({placeholders})",
        quote_ident(table)
    );

    let mut stmt = source
        .prepare(&select)
        .with_context(|| format!("prepare source read of `{table}`"))?;
    let mut rows = stmt.query([])?;
    let mut insert = tx
        .prepare(&insert)
        .with_context(|| format!("prepare destination insert into `{table}`"))?;
    let mut copied = 0i64;
    while let Some(row) = rows.next()? {
        let values = (0..columns.len())
            .map(|index| row.get::<_, Value>(index))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        insert
            .execute(params_from_iter(values))
            .with_context(|| format!("insert row {copied} into `{table}`"))?;
        copied += 1;
    }
    Ok(copied)
}

/// Copies the settings rows the v2 contract still owns; returns (copied, dropped keys).
fn copy_settings(source: &Connection, tx: &Transaction<'_>) -> Result<(usize, Vec<String>)> {
    let policy = SettingsPolicy::load();
    let mut stmt = source.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let mut rows = stmt.query([])?;
    let mut insert = tx.prepare(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )?;
    let mut copied = 0usize;
    let mut dropped = Vec::new();
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        if policy.carries(&key) {
            insert.execute(params![key, value])?;
            copied += 1;
        } else {
            dropped.push(key);
        }
    }
    Ok((copied, dropped))
}

/// Inserts the `vec0` rows belonging to the copied `embeddings` rows.
///
/// The vectors exist only in the source `vec0` table (the `embeddings` metadata has
/// no vector column), so they are read from there and inserted keyed by the rows we
/// just copied - never by copying the `vec0` shadow tables.
fn copy_embedding_vectors(
    source: &Connection,
    tx: &Transaction<'_>,
    warnings: &mut Vec<String>,
) -> Result<i64> {
    if !table_exists(source, "embedding_vectors")? || !table_exists(tx, "embedding_vectors")? {
        return Ok(0);
    }
    let mut stmt = source.prepare(
        "SELECT v.embedding_id, v.embedding
           FROM embedding_vectors v
           JOIN embeddings e ON e.id = v.embedding_id
          ORDER BY v.embedding_id",
    )?;
    let mut rows = stmt.query([])?;
    let mut insert =
        tx.prepare("INSERT INTO embedding_vectors(embedding_id, embedding) VALUES(?1, ?2)")?;
    let mut copied = 0i64;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let vector: Vec<u8> = row.get(1)?;
        insert.execute(params![id, vector])?;
        copied += 1;
    }
    let orphaned: i64 = source.query_row(
        "SELECT COUNT(*) FROM embeddings e
          WHERE NOT EXISTS (SELECT 1 FROM embedding_vectors v WHERE v.embedding_id = e.id)",
        [],
        |row| row.get(0),
    )?;
    if orphaned > 0 {
        warnings.push(format!(
            "{orphaned} source embedding row(s) have no vec0 vector; \
             the copy carries only the vectors the source had"
        ));
    }
    Ok(copied)
}

/// Repopulates the external-content FTS mirrors from the copied `frame_text` rows.
fn rebuild_fts(dest: &Connection) -> Result<()> {
    for table in FTS_REBUILD_TABLES {
        if !table_exists(dest, table)? {
            continue;
        }
        let quoted = quote_ident(table);
        dest.execute(
            &format!("INSERT INTO {quoted}({quoted}) VALUES('rebuild')"),
            [],
        )
        .with_context(|| format!("rebuild `{table}` from the copied rows"))?;
    }
    Ok(())
}

/// Reads a reserved settings value, returning `None` when the row is absent.
fn read_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .with_context(|| format!("read setting `{key}`"))
}

/// Refuses a source that cannot supply the reserved history identity/allocator.
///
/// Identity is carried, never invented and never recomputed, so a source missing
/// either key is a stop rather than a fresh UUID (`03 §7f.8`).
fn validate_reserved_metadata(source: &Connection) -> Result<Vec<String>> {
    let history_id = read_setting(source, HISTORY_ID_KEY)?.ok_or_else(|| {
        anyhow::anyhow!(
            "source has no `{HISTORY_ID_KEY}`; refusing to invent a history identity \
             (the reserved keys are copied verbatim, `03 §7f.8`)"
        )
    })?;
    if !traits::valid_history_id(&history_id) {
        bail!("source `{HISTORY_ID_KEY}` is not a valid history id: {history_id:?}");
    }
    let high_water = read_setting(source, FRAME_ID_HIGH_WATER_KEY)?.ok_or_else(|| {
        anyhow::anyhow!(
            "source has no `{FRAME_ID_HIGH_WATER_KEY}`; refusing to recompute the \
             allocator from MAX(frames.id) (`03 §7f.8`)"
        )
    })?;
    let allocator = parse_allocator(&high_water)?;
    let maximum: i64 = source.query_row("SELECT COALESCE(MAX(id),0) FROM frames", [], |row| {
        row.get(0)
    })?;
    let mut warnings = Vec::new();
    if allocator < maximum {
        warnings.push(format!(
            "source `{FRAME_ID_HIGH_WATER_KEY}` is {allocator}, below MAX(frames.id)={maximum}; \
             it is copied verbatim, never recomputed (`03 §7f.8`)"
        ));
    }
    Ok(warnings)
}

/// Parses the persisted allocator exactly as the store does: non-empty ASCII digits.
fn parse_allocator(value: &str) -> Result<i64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("source `{FRAME_ID_HIGH_WATER_KEY}` is not a nonnegative decimal integer: {value:?}");
    }
    value
        .parse::<i64>()
        .with_context(|| format!("parse `{FRAME_ID_HIGH_WATER_KEY}` value {value:?}"))
}

/// Proves the reserved keys survived byte-identically (never recomputed).
fn verify_reserved_metadata(source: &Connection, dest: &Connection) -> Result<()> {
    for key in SettingsPolicy::reserved_keys() {
        let from_source = read_setting(source, key)?;
        let from_dest = read_setting(dest, key)?;
        if from_source != from_dest {
            bail!(
                "reserved setting `{key}` did not survive the rebuild \
                 (source {from_source:?}, destination {from_dest:?})"
            );
        }
    }
    Ok(())
}

/// Reports `frames.image_path` values missing from disk, keeping every row.
fn missing_images(source_path: &Path, dest: &Connection) -> Result<Vec<String>> {
    let columns: std::collections::BTreeSet<String> =
        table_columns(dest, "frames")?.into_iter().collect();
    let filter = if columns.contains("image_purged") {
        "image_purged = 0 AND "
    } else {
        ""
    };
    let mut stmt = dest.prepare(&format!(
        "SELECT image_path FROM frames
          WHERE {filter}image_path IS NOT NULL AND image_path <> ''
          ORDER BY id"
    ))?;
    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(paths
        .into_iter()
        .filter(|path| !resolve_image_path(source_path, path).exists())
        .collect())
}

fn resolve_image_path(source_path: &Path, image_path: &str) -> PathBuf {
    let image = Path::new(image_path);
    if image.is_absolute() {
        return image.to_path_buf();
    }
    let root = source_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    root.join(image)
}

/// Every ordinary table's row count, with FTS5/`vec0` shadow tables excluded.
pub fn table_counts(path: &Path) -> Result<Vec<(String, i64)>> {
    let conn = open_read_only(path)?;
    counts_on(&conn)
}

fn counts_on(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut virtual_tables = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND UPPER(sql) LIKE 'CREATE VIRTUAL TABLE%'",
        )?;
        for name in stmt.query_map([], |row| row.get::<_, String>(0))? {
            virtual_tables.push(name?);
        }
    }
    let mut names = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?;
        for name in stmt.query_map([], |row| row.get::<_, String>(0))? {
            names.push(name?);
        }
    }
    names.retain(|name| {
        !virtual_tables
            .iter()
            .any(|table| name == table || name.starts_with(&format!("{table}_")))
    });
    names.sort();
    let mut counts = Vec::with_capacity(names.len());
    for name in names {
        counts.push((name.clone(), count_on(conn, &name)?));
    }
    Ok(counts)
}

fn count_on(conn: &Connection, table: &str) -> Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {}", quote_ident(table)),
        [],
        |row| row.get(0),
    )
    .with_context(|| format!("count rows of `{table}`"))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn schema_version_of(conn: &Connection) -> Result<i32> {
    conn.query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .context("read schema_version")
}

fn integrity_check(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT integrity_check FROM pragma_integrity_check")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The exact `PRAGMA foreign_key_check` rows (table, rowid, parent table, fk index), so
/// a fidelity failure names what could not be preserved instead of only a count.
fn foreign_key_violation_rows(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT \"table\", rowid, parent, fkid FROM pragma_foreign_key_check")?;
    let rows = stmt
        .query_map([], |row| {
            let table: String = row.get(0)?;
            let rowid: Option<i64> = row.get(1)?;
            let parent: String = row.get(2)?;
            let fkid: i64 = row.get(3)?;
            Ok(format!(
                "table={table} rowid={rowid:?} parent={parent} fkid={fkid}"
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn sample_frame_ids_on(conn: &Connection, limit: usize) -> Result<Vec<i64>> {
    // `usize` exceeds `i64::MAX` only for absurd input on 64-bit; clamp it instead of
    // wrapping. SQLite treats a negative LIMIT as "no limit", so the clamp must stay
    // non-negative; the CLI parses a plain count and the report prints what it sampled.
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare("SELECT id FROM frames ORDER BY random() LIMIT ?1")?;
    let ids = stmt
        .query_map(params![limit], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {} read-only", path.display()))
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
