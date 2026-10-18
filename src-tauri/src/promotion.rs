//! Promotion into the single user-created project note (`03 §8e`).
//!
//! The note at `<selected folder>/ScreenSearch/Projects/<slug>.md` is created
//! empty by the user and is the only accumulation target. Promotion is the only
//! write path: `prepare_promotion` reads and validates a saved weekly synthesis
//! or daily report and previews dated entry blocks without appending note bytes;
//! it may finish metadata for a previously confirmed append. `confirm_promotion`
//! rebuilds those blocks and appends them in one atomic write. Unconfirmed
//! proposals are never persisted, and there is no HTTP/MCP surface.
//!
//! A confirmed append and its ownership-row update cannot be one atomic
//! operation across the filesystem and SQLite. `confirm_promotion` therefore
//! persists a bounded write intent (exact target, source/run identity and the
//! old/new hashes) in the existing settings KV before writing. If the append
//! lands but the ownership save or cleanup fails, the next `prepare`/`confirm`
//! reconciles the live bytes against that intent instead of mistaking the
//! app's own write for a user edit. The intent never contains selected text.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use store::SqliteStore;
use tauri::State;
use traits::*;

use crate::memory_files::{
    append_entries_owned, content_hash, heading_title, merged_content, native_path, path_string,
    safe_read_owned, write_owned, MarkdownFence, WriteFailure,
};
use crate::{now_ms, AppState};

/// Persisted memory setting holding the selected output folder (`03 §8c`).
const OUTPUT_FOLDER: &str = "memory.schedule.output_folder";

/// Internal KV prefix for one compact confirmed-write record per exact note:
/// `memory.promotion.pending.<sha256(normalized path)>`. `"null"` means no
/// pending intent; the keys are not settings/API surface and are never exposed
/// to the UI or HTTP reads.
const PROMOTION_INTENT: &str = "memory.promotion.pending";
/// Last committed receipt per note. Kept after the pending intent is cleared so
/// a lost success response can be answered without a second append.
const PROMOTION_RECEIPT: &str = "memory.promotion.receipt";

const MAX_LISTED_IDS: usize = 20;

/// Reserved DOS device names resolve to a device even with an extension
/// (`con.md` opens the console), so no project note may be named one. Slugs are
/// lower-case only, so the comparison is exact; `valid_slug` rejects them too.
/// Microsoft's documented list is CON, PRN, AUX, NUL, COM1–COM9 and LPT1–LPT9;
/// `COM0`/`LPT0` are not reserved file names, so they stay ordinary slugs.
const RESERVED_SLUGS: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Serializes every project-note mutation in this process: the create
/// scan-then-create window, and the confirm ownership-check/write/save sequence.
/// Without it two creates can both observe an empty Projects folder, or two
/// confirms can append and replace the ownership row from the same stale state.
/// A confirm moves its guard into the blocking writer and holds it until the
/// writer returns, so cancelling the async caller cannot release the lock while
/// the non-cancellable native write can still land.
/// This is a tokio lock (never held across a blocking mutex wait); the app is
/// single-instance, so a second process is outside this boundary.
static NOTE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Durable evidence of the one confirmed append whose ownership update has not
/// been proven. It deliberately carries no selected text, preview or proposal:
/// only the exact note path, source/run identity and the old/new SHA-256 values
/// needed to decide whether the app's own write landed.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct PromotionIntent {
    path: String,
    source_id: String,
    run_id: String,
    old_hash: String,
    new_hash: String,
    /// Digest of the exact selection plus its promotion date. Hashes and run
    /// identity alone cannot distinguish two different selections from the same
    /// snapshot; recovery success requires this to match too.
    #[serde(default)]
    fingerprint: String,
    /// False while the append may still need its ownership row; true after the
    /// ownership row is durable. The committed record is the compact last-receipt
    /// tombstone used to answer a lost-response retry without appending again.
    #[serde(default)]
    committed: bool,
}

/// Canonical digest of the user's exact selection and promotion date. This is
/// stored instead of the text, so a same-hash retry cannot be mistaken for a
/// different selection that happened to start from the same note bytes.
fn promotion_fingerprint(selection: &PromotionSelection, entry_date: &str) -> String {
    // Structured encoding preserves item boundaries even when text contains
    // newlines or delimiter characters. Only this digest is persisted.
    let canonical = serde_json::json!([selection, entry_date]).to_string();
    content_hash(canonical.as_bytes())
}

/// Per-target internal KV slot, keyed by the normalized note path. One compact
/// latest intent/receipt per note means switching output folders cannot
/// overwrite an unresolved append for the previous exact target.
fn record_key(prefix: &str, note_path: &str) -> String {
    let normalized = note_path.replace('\\', "/");
    format!("{prefix}.{}", content_hash(normalized.as_bytes()))
}

async fn read_record(
    store: &dyn Store,
    prefix: &str,
    note_path: &str,
) -> Result<Option<PromotionIntent>, ReportError> {
    let Some(raw) = store
        .get_setting(&record_key(prefix, note_path))
        .await
        .map_err(storage)?
    else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" {
        return Ok(None);
    }
    let intent: PromotionIntent = serde_json::from_str(raw).map_err(storage)?;
    // A collision or a hand-edited key must never apply another note's record.
    Ok((intent.path.as_str() == note_path).then_some(intent))
}

async fn read_intent(
    store: &dyn Store,
    note_path: &str,
) -> Result<Option<PromotionIntent>, ReportError> {
    read_record(store, PROMOTION_INTENT, note_path).await
}

async fn read_receipt(
    store: &dyn Store,
    note_path: &str,
) -> Result<Option<PromotionIntent>, ReportError> {
    read_record(store, PROMOTION_RECEIPT, note_path).await
}

async fn write_record(
    store: &dyn Store,
    prefix: &str,
    intent: &PromotionIntent,
) -> Result<(), ReportError> {
    let value = serde_json::to_string(intent).map_err(storage)?;
    store
        .set_setting(&record_key(prefix, &intent.path), &value)
        .await
        .map_err(storage)
}

async fn write_intent(store: &dyn Store, intent: &PromotionIntent) -> Result<(), ReportError> {
    write_record(store, PROMOTION_INTENT, intent).await
}

/// Promote a pending intent to the last committed receipt. Failure is harmless:
/// the ownership row is already durable, and the next reconcile repeats this.
async fn commit_intent(store: &dyn Store, intent: &PromotionIntent) {
    let committed = PromotionIntent {
        committed: true,
        ..intent.clone()
    };
    if write_record(store, PROMOTION_RECEIPT, &committed)
        .await
        .is_ok()
    {
        clear_intent(store, &intent.path).await;
    }
}

/// Clear a pre-write intent. Failure is harmless and idempotent: the next
/// reconcile sees the same old bytes and repeats the clear.
async fn clear_intent(store: &dyn Store, note_path: &str) {
    let _ = store
        .set_setting(&record_key(PROMOTION_INTENT, note_path), "null")
        .await;
}

/// What a pending intent proves about the live note.
enum Recovery {
    None,
    /// The confirmed append is installed and its ownership row is saved. The
    /// carried identity lets confirm distinguish its own retry from a different
    /// proposal that merely happened to run afterwards.
    Applied {
        old_hash: String,
        new_hash: String,
        run_id: String,
        fingerprint: String,
    },
    /// A confirmed append is associated with this note, but the live bytes are
    /// neither the pre-write nor the intended post-write hash. Never adopt them.
    Edited,
}

/// Resolve a pending confirmed-write intent against the live note, before any
/// ownership guard can call the app's own bytes a user edit.
///
/// Callers hold `NOTE_LOCK`, and a writer keeps that same guard inside its
/// blocking closure until the native write returns. Therefore a live hash that
/// still equals `old_hash` here cannot belong to an in-flight writer: it is a
/// pre-write interruption (safe to drop) rather than a race with an append that
/// might still complete. Store calls are individually durable and every step is
/// idempotent, so cancellation inside the async ownership/commit sequence is
/// repaired by the next reconcile.
async fn reconcile_intent(
    store: &dyn Store,
    note_path: &str,
    current_hash: Option<&str>,
) -> Result<Recovery, ReportError> {
    let pending = read_intent(store, note_path).await?;
    let record = match pending {
        Some(intent)
            if !intent.committed
                && current_hash == Some(intent.old_hash.as_str())
                && intent.old_hash != intent.new_hash =>
        {
            // No writer can still be running under this guard. Discard a
            // pre-write interruption, but retain the prior successful receipt:
            // the caller may be retrying that earlier, already-landed append.
            clear_intent(store, note_path).await;
            read_receipt(store, note_path).await?
        }
        Some(intent) => Some(intent),
        None => read_receipt(store, note_path).await?,
    };
    let Some(intent) = record else {
        return Ok(Recovery::None);
    };
    match current_hash {
        Some(current) if current == intent.new_hash.as_str() => {
            match store.get_report_file(note_path).await.map_err(storage)? {
                Some(owner) if owner.source_id != intent.source_id => {
                    // Never move an intent across the ownership source boundary.
                    return Ok(Recovery::Edited);
                }
                Some(owner)
                    if owner.hash == intent.new_hash
                        && owner.run_id == intent.run_id
                        && owner.source_id == intent.source_id => {}
                _ => {
                    store
                        .save_report_file(ReportFileOwnership {
                            path: note_path.to_string(),
                            source_id: intent.source_id.clone(),
                            run_id: intent.run_id.clone(),
                            hash: intent.new_hash.clone(),
                            updated_at: now_ms(),
                        })
                        .await
                        .map_err(storage)?;
                }
            }
            if !intent.committed {
                // The append landed but its ownership save did not finish. The
                // last committed receipt lets a lost-response retry succeed.
                commit_intent(store, &intent).await;
            }
            Ok(Recovery::Applied {
                old_hash: intent.old_hash,
                new_hash: intent.new_hash,
                run_id: intent.run_id,
                fingerprint: intent.fingerprint,
            })
        }
        Some(current) if !intent.committed && current == intent.old_hash.as_str() => {
            // The append never became visible. With the writer lock held, no
            // blocking append can still install these bytes, so the intent is
            // stale evidence and can be discarded without touching the note.
            clear_intent(store, note_path).await;
            Ok(Recovery::None)
        }
        // A committed receipt whose note was edited/reverted has no recovery
        // claim: the normal ownership guard owns that conflict. A pending intent
        // with unknown bytes is never adopted.
        _ if intent.committed => Ok(Recovery::None),
        _ => Ok(Recovery::Edited),
    }
}

const EDITED_AFTER_CONFIRM: &str = "The project note changed after a confirmed promotion whose ownership update did not finish; nothing was appended. Reconcile the note before promoting again.";

/// A promotable section: the report heading it comes from and the project-note
/// heading it is appended under (`03 §8e`, fixed note sections).
struct SectionSpec {
    section: PromotionSection,
    source: &'static str,
    target: &'static str,
}

/// Canonical order: Decisions, OpenQuestions, NextSteps, Log.
const SECTIONS: [SectionSpec; 4] = [
    SectionSpec {
        section: PromotionSection::Decisions,
        source: "Explicitly evidenced decisions",
        target: "Decisions",
    },
    SectionSpec {
        section: PromotionSection::OpenQuestions,
        source: "Open questions",
        target: "Open questions",
    },
    SectionSpec {
        section: PromotionSection::NextSteps,
        source: "Next steps",
        target: "Next steps",
    },
    SectionSpec {
        section: PromotionSection::Log,
        source: "Activity",
        target: "Log",
    },
];

/// One rebuilt entry block plus the project-note section it belongs to.
struct EntryBlock {
    section: PromotionSection,
    target: &'static str,
    items: Vec<String>,
    block: String,
}

fn fail(code: &str, message: &str) -> ReportError {
    ReportError {
        code: code.into(),
        message: message.into(),
    }
}

fn storage(_: impl std::fmt::Display) -> ReportError {
    fail("STORE_ERROR", "Could not read or save memory report state.")
}

fn db(state: &AppState) -> Result<Arc<SqliteStore>, ReportError> {
    state
        .store
        .clone()
        .ok_or_else(|| fail("STORE_ERROR", "Database unavailable."))
}

fn projects_dir(output_folder: &Path) -> PathBuf {
    output_folder.join("ScreenSearch").join("Projects")
}

/// The selected folder is the daily/weekly output folder (`03 §8c`); project
/// notes never get a separate folder choice.
async fn output_folder(store: &dyn Store) -> Result<PathBuf, ReportError> {
    let raw = store.get_setting(OUTPUT_FOLDER).await.map_err(storage)?;
    let folder: Option<String> = match raw {
        Some(value) => serde_json::from_str(&value).map_err(storage)?,
        None => None,
    };
    let folder = folder
        .filter(|folder| !folder.trim().is_empty())
        .map(|folder| native_path(Path::new(&folder)))
        .ok_or_else(|| fail("INVALID_CONFIG", "Select an output folder first."))?;
    if !folder.is_absolute() {
        return Err(fail("INVALID_CONFIG", "Select an absolute output folder."));
    }
    Ok(folder)
}

/// Reject redirected entries (Windows reparse points) and non-files so a scan
/// only ever reports a real, top-level `.md` note.
fn is_redirected(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// One non-recursive scan of `ScreenSearch/Projects`: top-level `.md` regular
/// files only, sorted for deterministic messaging.
fn project_notes(output_folder: &Path) -> Result<Vec<PathBuf>, ReportError> {
    let dir = projects_dir(output_folder);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => {
            return Err(fail(
                "SOURCE_UNAVAILABLE",
                "The Projects folder cannot be read.",
            ))
        }
    };
    let mut notes = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|_| fail("SOURCE_UNAVAILABLE", "The Projects folder cannot be read."))?;
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() || is_redirected(&meta) {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        notes.push(path);
    }
    notes.sort();
    Ok(notes)
}

fn status_of(notes: &[PathBuf]) -> ProjectNoteStatus {
    ProjectNoteStatus {
        path: (notes.len() == 1).then(|| path_string(&notes[0])),
        exists: !notes.is_empty(),
        count: u32::try_from(notes.len()).unwrap_or(u32::MAX),
    }
}

/// Exactly one note is supported in this slice; zero and many are distinct,
/// explicit errors rather than a silent pick.
fn single_note(notes: &[PathBuf]) -> Result<&Path, ReportError> {
    match notes {
        [] => Err(fail("NOT_FOUND", "Create a project note first.")),
        [note] => Ok(note),
        many => {
            let paths: Vec<String> = many.iter().map(|path| path_string(path)).collect();
            Err(fail(
                "FILE_CONFLICT",
                &format!(
                    "More than one project note exists; keep exactly one: {}.",
                    paths.join(", ")
                ),
            ))
        }
    }
}

/// `^[a-z0-9][a-z0-9_-]{0,63}$` — a filename-safe, lower-case slug that is not a
/// reserved DOS device name.
fn valid_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes.len() <= 64
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-' || *byte == b'_'
        })
        && !RESERVED_SLUGS.contains(&slug)
}

/// The user-facing reason a slug is refused; `None` means it is safe to create.
fn slug_error(slug: &str) -> Option<&'static str> {
    if RESERVED_SLUGS.contains(&slug) {
        return Some("That name is reserved by Windows; choose another project note name.");
    }
    if !valid_slug(slug) {
        return Some("Use lowercase letters, digits, dashes or underscores, starting with a letter or digit (max 64 characters).");
    }
    None
}

/// Keep the potentially blocking native read off the async command thread.
async fn read_owned(path: &Path) -> Result<Option<Vec<u8>>, WriteFailure> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || safe_read_owned(&path))
        .await
        .map_err(|_| WriteFailure {
            message: "The file reader stopped unexpectedly.".into(),
            conflict: false,
            recovery_path: None,
        })?
}

fn read_error(error: WriteFailure) -> ReportError {
    fail(
        if error.conflict {
            "FILE_CONFLICT"
        } else {
            "SOURCE_UNAVAILABLE"
        },
        &error.message,
    )
}

/// The persisted ownership row is the project-note write guard (`03 §8c`/`§8e`):
/// after the first promotion, only the same source may append, and only while the
/// live bytes still match the hash of the app's latest write. An unowned note
/// (no row) is adoptable on its first promotion; an owned note is never silently
/// adopted after an edit or by another source.
fn owner_conflict(
    owner: &ReportFileOwnership,
    run: &ReportRun,
    current: Option<&str>,
) -> Option<ReportError> {
    if owner.source_id != run.source_id {
        return Some(fail(
            "FILE_CONFLICT",
            "Another source owns this project note; promotion cannot append to it.",
        ));
    }
    if current != Some(owner.hash.as_str()) {
        return Some(fail(
            "FILE_CONFLICT",
            "The project note was edited after the app's last promotion; promotion cannot append to it.",
        ));
    }
    None
}

/// A structured `FILE_CONFLICT` receipt: the resolved note is named, but no hash
/// is claimed and no ownership row is written because nothing was appended.
fn conflict_receipt(
    path: String,
    message: &str,
    recovery_path: Option<String>,
) -> PromotionReceipt {
    PromotionReceipt {
        path,
        hash: None,
        conflict: true,
        error: Some(fail("FILE_CONFLICT", message)),
        recovery_path,
    }
}

/// Top-level dash bullets (`- ` at column 0) under `## <heading>`, verbatim.
/// Fenced content is skipped with the same marker/length rules as the UI parser,
/// so a `## heading` or bullet inside a fence is never promoted.
fn section_bullets(markdown: &str, heading: &str) -> Vec<String> {
    let target = heading.to_lowercase();
    let mut current = false;
    let mut fence = MarkdownFence::default();
    let mut items = Vec::new();
    for line in markdown.lines() {
        let line = line.trim_end();
        if fence.contains_line(line) {
            continue;
        }
        if let Some(title) = heading_title(line) {
            current = title.trim().to_lowercase() == target;
            continue;
        }
        if line == "#" || line.starts_with("# ") {
            current = false;
            continue;
        }
        if !current {
            continue;
        }
        if let Some(text) = line.strip_prefix("- ") {
            if !text.is_empty() {
                items.push(text.to_string());
            }
        }
    }
    items
}

fn sorted_ids(ids: &[i64]) -> String {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return "—".into();
    }
    let mut text = ids
        .iter()
        .take(MAX_LISTED_IDS)
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    if ids.len() > MAX_LISTED_IDS {
        text.push_str(&format!(", … ({} total)", ids.len()));
    }
    text
}

fn ordered_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        return "—".into();
    }
    // Persisted order, no truncation: a week never consumes more than seven
    // dailies, and the frozen format caps only frames and sessions.
    ids.join(", ")
}

/// The provenance sentence naming the source run, its daily runs and the frame
/// and session evidence. The entry block renders it as one italic line.
fn provenance(run: &ReportRun, result: &SavedReport) -> String {
    let source = match run.kind {
        ReportRunKind::WeeklySynthesis => "weekly synthesis",
        ReportRunKind::Daily => "daily report",
    };
    let mut line = format!("Source: {source} run {}", run.run_id);
    if run.kind == ReportRunKind::WeeklySynthesis {
        line.push_str("; daily runs ");
        line.push_str(&ordered_ids(&result.daily_run_ids));
    }
    line.push_str("; frames ");
    line.push_str(&sorted_ids(&result.frame_ids));
    line.push_str("; sessions ");
    line.push_str(&sorted_ids(&result.session_ids));
    line.push('.');
    line
}

fn local_date(timezone: &str, at: i64) -> Result<String, ReportError> {
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| fail("INVALID_CONFIG", "The saved report time zone is invalid."))?;
    let instant = DateTime::<Utc>::from_timestamp_millis(at).ok_or_else(|| {
        fail(
            "INVALID_CONFIG",
            "The promotion date could not be resolved.",
        )
    })?;
    Ok(instant.with_timezone(&zone).format("%Y-%m-%d").to_string())
}

fn entry_date(value: &str) -> Result<String, ReportError> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| fail("INVALID_CONFIG", "The promotion date is invalid."))?;
    // Canonical `YYYY-MM-DD`, so a confirmed entry can never carry a heading the
    // preview did not show.
    Ok(parsed.format("%Y-%m-%d").to_string())
}

/// A run is promotable only with a saved result whose outcome is OK or PARTIAL.
async fn load_run(
    store: &dyn Store,
    run_id: &str,
) -> Result<(ReportRun, SavedReport), ReportError> {
    let run = store
        .get_report_run(run_id)
        .await
        .map_err(storage)?
        .ok_or_else(|| fail("NOT_FOUND", "Report run not found."))?;
    let promotable = matches!(
        run.outcome,
        Some(ReportOutcome::Ok | ReportOutcome::Partial)
    );
    let result = run.result.clone().filter(|_| promotable).ok_or_else(|| {
        fail(
            "SOURCE_UNAVAILABLE",
            "Only a saved report with OK or PARTIAL output can be promoted.",
        )
    })?;
    Ok((run, result))
}

/// Validate every selected item against the saved markdown and rebuild the
/// canonical entry blocks. A selection that no longer matches is an error;
/// nothing is ever silently dropped.
fn build_entries(
    result: &SavedReport,
    selection: &PromotionSelection,
    entry_date: &str,
    provenance: &str,
) -> Result<Vec<EntryBlock>, ReportError> {
    let mut blocks = Vec::new();
    for spec in SECTIONS {
        let selected: Vec<&PromotionItem> = selection
            .items
            .iter()
            .filter(|item| item.section == spec.section)
            .collect();
        if selected.is_empty() {
            continue;
        }
        let available = section_bullets(&result.markdown, spec.source);
        // Count the persisted occurrences: an identical bullet may legitimately
        // repeat in a report, but the selection may not invent extra copies.
        let mut remaining: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for bullet in &available {
            *remaining.entry(bullet.as_str()).or_default() += 1;
        }
        let mut items: Vec<String> = Vec::new();
        for item in selected {
            match remaining.get_mut(item.text.as_str()) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    // The selection is item-level: each persisted occurrence can be
                    // selected once, and the preview keeps the user's multiplicity.
                    items.push(item.text.clone());
                }
                Some(_) => {
                    return Err(fail(
                        "INVALID_CONFIG",
                        &format!(
                            "The selection repeats an item more times than the saved report contains under \"{}\".",
                            spec.source
                        ),
                    ))
                }
                None => {
                    return Err(fail(
                        "INVALID_CONFIG",
                        &format!(
                            "A selected item is no longer a bullet under \"{}\" in the saved report.",
                            spec.source
                        ),
                    ))
                }
            }
        }
        let mut block = format!("### {entry_date}\n\n");
        for item in &items {
            block.push_str("- ");
            block.push_str(item);
            block.push('\n');
        }
        block.push('\n');
        block.push('*');
        block.push_str(provenance);
        block.push('*');
        blocks.push(EntryBlock {
            section: spec.section,
            target: spec.target,
            items,
            block,
        });
    }
    if blocks.is_empty() {
        return Err(fail(
            "INVALID_CONFIG",
            "Select at least one item to promote.",
        ));
    }
    Ok(blocks)
}

/// Never appends note bytes. It may settle metadata for a previously confirmed
/// append (finish its ownership row / last-receipt tombstone, or discard stale
/// pre-write evidence) before either the ownership guard or `load_run` runs, so
/// recovery still works after the source run was pruned.
async fn prepare(
    store: &dyn Store,
    selection: &PromotionSelection,
    at: i64,
) -> Result<PromotionProposal, ReportError> {
    let folder = output_folder(store).await?;
    let _guard = NOTE_LOCK.lock().await;
    let notes = project_notes(&folder)?;
    let note = single_note(&notes)?;
    let note_path = path_string(note);
    let note_hash = read_owned(note)
        .await
        .map_err(read_error)?
        .map(|bytes| content_hash(&bytes));
    // A confirmed append may have landed before its ownership row was saved.
    // Reconcile that app-owned write from the intent first; otherwise the old
    // row would permanently reject the retry as if the user had edited the note.
    if matches!(
        reconcile_intent(store, &note_path, note_hash.as_deref()).await?,
        Recovery::Edited
    ) {
        return Err(fail("FILE_CONFLICT", EDITED_AFTER_CONFIRM));
    }
    let (run, result) = load_run(store, &selection.run_id).await?;
    let date = local_date(&run.timezone, at)?;
    let source_line = provenance(&run, &result);
    let blocks = build_entries(&result, selection, &date, &source_line)?;
    // Fail closed before previewing when the note is already owned: the row's
    // source identity and hash must still describe the live bytes. An unowned
    // note remains adoptable on its first promotion.
    if let Some(owner) = store.get_report_file(&note_path).await.map_err(storage)? {
        if let Some(error) = owner_conflict(&owner, &run, note_hash.as_deref()) {
            return Err(error);
        }
    }
    let entries: Vec<PromotionEntryPreview> = blocks
        .iter()
        .map(|block| PromotionEntryPreview {
            section: block.section,
            heading_date: date.clone(),
            items: block.items.clone(),
            provenance: source_line.clone(),
        })
        .collect();
    Ok(PromotionProposal {
        selection: selection.clone(),
        run_id: run.run_id.clone(),
        note_path: Some(note_path),
        note_exists: note_hash.is_some(),
        note_hash,
        entry_date: date,
        entries,
    })
}

/// Rebuild the blocks from the proposal's own selection and date, then append
/// them in one atomic write. A conflict preserves the user's bytes untouched.
///
/// The proposal is client-supplied, so the target is never trusted: the folder is
/// re-scanned, the one supported note is resolved again (zero/many stay explicit
/// errors) and the proposal's path must name exactly that note. The selection-time
/// hash is mandatory — a proposal without one cannot prove the note is the file
/// the preview described, and confirm never creates a note of its own.
async fn confirm(
    store: &dyn Store,
    proposal: &PromotionProposal,
) -> Result<PromotionReceipt, ReportError> {
    if proposal.run_id != proposal.selection.run_id {
        return Err(fail(
            "INVALID_CONFIG",
            "The promotion proposal does not match its selection.",
        ));
    }
    let Some(selection_hash) = proposal.note_hash.as_deref() else {
        return Err(fail(
            "INVALID_CONFIG",
            "The promotion proposal has no selection-time note hash; prepare the promotion again.",
        ));
    };
    let date = entry_date(&proposal.entry_date)?;
    let fingerprint = promotion_fingerprint(&proposal.selection, &date);
    let folder = output_folder(store).await?;
    // Serialize the scan, ownership check, append and ownership-row save: two
    // confirms must not both append from the same stale bytes and then race the
    // row replacement. The guard is moved into the native writer below, so a
    // dropped caller cannot release it while the append can still complete.
    let guard = NOTE_LOCK.lock().await;
    let notes = project_notes(&folder)?;
    let note_path = path_string(single_note(&notes)?);
    if proposal.note_path.as_deref() != Some(note_path.as_str()) {
        // The folder or the note changed since the preview (an honest delete race
        // lands here as a zero-note error above): the user re-prepares against the
        // current note instead of this call inventing or writing anywhere else.
        return Ok(conflict_receipt(
            note_path,
            "The project note changed since the preview; prepare the promotion again.",
            None,
        ));
    }
    let current = match read_owned(Path::new(&note_path)).await {
        Ok(current) => current,
        Err(error) if error.conflict => {
            return Ok(conflict_receipt(
                note_path,
                &error.message,
                error.recovery_path,
            ))
        }
        Err(error) => return Err(read_error(error)),
    };
    let current_hash = current.as_ref().map(|bytes| content_hash(bytes));
    // Recover a confirmed append before loading its source run: ownership and the
    // last-receipt tombstone must still settle after the run has been pruned.
    match reconcile_intent(store, &note_path, current_hash.as_deref()).await? {
        Recovery::Edited => {
            return Ok(conflict_receipt(note_path, EDITED_AFTER_CONFIRM, None));
        }
        Recovery::Applied {
            old_hash,
            new_hash,
            run_id,
            fingerprint: intent_fingerprint,
            ..
        } if old_hash.as_str() == selection_hash
            && intent_fingerprint == fingerprint
            && run_id == proposal.selection.run_id =>
        {
            // This is a retry (including a lost-response retry) of the exact
            // confirmed append that already landed. Report its durable result
            // instead of appending the selection twice.
            return Ok(PromotionReceipt {
                path: note_path,
                hash: Some(new_hash),
                conflict: false,
                error: None,
                recovery_path: None,
            });
        }
        Recovery::None | Recovery::Applied { .. } => {}
    }
    let (run, result) = load_run(store, &proposal.selection.run_id).await?;
    let source_line = provenance(&run, &result);
    let blocks = build_entries(&result, &proposal.selection, &date, &source_line)?;
    // The persisted row, not the client-supplied hash, decides whether this note
    // is still appendable. A forged proposal cannot adopt an edited note by
    // presenting the edited bytes' hash: the row's stored hash must match too.
    if let Some(owner) = store.get_report_file(&note_path).await.map_err(storage)? {
        if let Some(error) = owner_conflict(&owner, &run, current_hash.as_deref()) {
            return Ok(conflict_receipt(note_path, &error.message, None));
        }
    }
    let Some(expected) = current_hash.filter(|hash| selection_hash == hash.as_str()) else {
        return Ok(conflict_receipt(
            note_path,
            "The project note changed since the preview; prepare the promotion again.",
            None,
        ));
    };
    let entries: Vec<(String, String)> = blocks
        .into_iter()
        .map(|block| (block.target.to_string(), block.block))
        .collect();
    let Some(current_bytes) = current.as_deref() else {
        return Ok(conflict_receipt(
            note_path,
            "The project note disappeared before it could be written; prepare the promotion again.",
            None,
        ));
    };
    let existing = match std::str::from_utf8(current_bytes) {
        Ok(existing) => existing,
        Err(_) => {
            return Ok(conflict_receipt(
                note_path,
                "The project note is not valid UTF-8; it was not changed.",
                None,
            ))
        }
    };
    // Compute the exact bytes the writer will install so the intent can name the
    // post-write hash before the append becomes visible. This stores no text.
    let intended_hash = content_hash(merged_content(existing, &entries).as_bytes());
    let intent = PromotionIntent {
        path: note_path.clone(),
        source_id: run.source_id.clone(),
        run_id: run.run_id.clone(),
        old_hash: expected.clone(),
        new_hash: intended_hash,
        fingerprint,
        committed: false,
    };
    if write_intent(store, &intent).await.is_err() {
        // Fail before the filesystem has any side effect.
        return Err(fail(
            "STORE_ERROR",
            "The promotion could not be recorded before writing; nothing was written.",
        ));
    }
    let target = Path::new(&note_path).to_owned();
    // Native filesystem I/O is not cancellable. The blocking closure returns the
    // NOTE_LOCK guard with its result, so if this caller is dropped the detached
    // writer keeps the lock until it finishes; only then can the next
    // prepare/confirm reconcile the intent against settled bytes.
    let (_guard, write) = tokio::task::spawn_blocking(move || {
        let write = append_entries_owned(&target, &entries, Some(expected.as_str()));
        (guard, write)
    })
    .await
    .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match write {
        Ok(receipt) => {
            if receipt.hash != intent.new_hash {
                // The writer's own installed hash is authoritative; keep the
                // intent aligned with reality if they ever differ.
                let corrected = PromotionIntent {
                    new_hash: receipt.hash.clone(),
                    ..intent.clone()
                };
                let _ = write_intent(store, &corrected).await;
            }
            // The ownership row is the note's write guard, not an attribution log
            // or a garbage-collection key: it stores the source identity and the
            // SHA-256 of the bytes this app last wrote for this one note. Prepare
            // and confirm both require the same source and a matching live hash,
            // so an edit or another source's ownership is never silently adopted;
            // the row survives run pruning and is replaced on every promotion.
            if store
                .save_report_file(ReportFileOwnership {
                    path: note_path.clone(),
                    source_id: run.source_id.clone(),
                    run_id: run.run_id.clone(),
                    hash: receipt.hash.clone(),
                    updated_at: now_ms(),
                })
                .await
                .is_err()
            {
                // The append is durable; the intent makes the retry finish this
                // row instead of treating the app's write as a user edit.
                return Err(fail(
                    "STORE_ERROR",
                    "The promotion was appended, but its ownership record could not be saved. Retry the promotion to finish recording it.",
                ));
            }
            // Keep the compact last committed receipt. A lost success response
            // can then answer the same-old-hash/same-fingerprint retry without
            // appending again; a genuinely new explicit promotion has a new
            // snapshot hash and proceeds normally.
            commit_intent(store, &intent).await;
            Ok(PromotionReceipt {
                path: note_path,
                hash: Some(receipt.hash),
                conflict: false,
                error: None,
                recovery_path: None,
            })
        }
        // `_guard` still holds NOTE_LOCK while the conflict is settled.
        Err(error) if error.conflict => settle_conflict(store, note_path, &intent, error).await,
        Err(error) => Err(fail("WRITE_FAILED", &error.message)),
    }
}

const APPENDED_WITH_RECOVERY: &str = "The entries were appended, but the note changed or could not be verified while they were being written. The version the write replaced is kept in the recovery copy.";

/// Settle a writer conflict while the caller still holds `NOTE_LOCK`.
///
/// A conflict flag alone does not mean nothing was written. On a concurrent
/// swap `write_owned` reports a conflict after `ReplaceFileW` installed the new
/// bytes, because the version it displaced is not the one this promotion was
/// prepared from (or cannot be verified); that version is preserved in the
/// recovery copy (`03 §8c`). Decide from the live bytes, exactly as the next
/// reconcile would: when they are this promotion's bytes, record ownership now
/// and report the append as written with its recovery copy, so the receipt,
/// the ownership row and the note agree. Anything else is a conflict that
/// appended nothing, and the intent stays for the next call to decide.
async fn settle_conflict(
    store: &dyn Store,
    note_path: String,
    intent: &PromotionIntent,
    error: WriteFailure,
) -> Result<PromotionReceipt, ReportError> {
    let live_hash = match read_owned(Path::new(&note_path)).await {
        Ok(Some(bytes)) => Some(content_hash(&bytes)),
        _ => None,
    };
    if live_hash.as_deref() != Some(intent.new_hash.as_str()) {
        return Ok(conflict_receipt(
            note_path,
            &error.message,
            error.recovery_path,
        ));
    }
    match reconcile_intent(store, &note_path, live_hash.as_deref()).await {
        Ok(Recovery::Applied {
            new_hash,
            fingerprint,
            ..
        }) if new_hash == intent.new_hash && fingerprint == intent.fingerprint => {
            Ok(PromotionReceipt {
                path: note_path,
                hash: Some(new_hash),
                conflict: true,
                error: Some(fail("FILE_CONFLICT", APPENDED_WITH_RECOVERY)),
                recovery_path: error.recovery_path,
            })
        }
        _ => {
            // The append is installed but could not be recorded; the pending
            // intent lets a retry finish it instead of calling it a user edit.
            let kept = error
                .recovery_path
                .as_deref()
                .map(|path| format!(" The version the write replaced is kept at {path}."))
                .unwrap_or_default();
            Err(fail(
                "STORE_ERROR",
                &format!(
                    "The promotion was appended, but its ownership record could not be saved.{kept} Retry the promotion to finish recording it."
                ),
            ))
        }
    }
}

/// State of the single project note; a count above 1 is surfaced as-is.
#[tauri::command]
pub async fn project_note_status(
    state: State<'_, AppState>,
) -> Result<ProjectNoteStatus, ReportError> {
    let store = db(&state)?;
    let folder = output_folder(store.as_ref()).await?;
    Ok(status_of(&project_notes(&folder)?))
}

/// Create the note default-empty. The app never invents, renames or deletes it,
/// so an existing note is refused rather than reused.
#[tauri::command]
pub async fn create_project_note(
    slug: String,
    state: State<'_, AppState>,
) -> Result<ProjectNoteStatus, ReportError> {
    let store = db(&state)?;
    owned_create(store, slug).await
}

/// Validate and create the note default-empty. The app never invents, renames or
/// deletes it, so an existing note is refused rather than reused. Validation lives
/// here, in the shared core, so the command and every direct caller exercise the
/// same reserved-name and slug-charset rules.
///
/// The scan-then-create window is serialized in-process: without the lock two
/// callers can both observe an empty Projects folder and each create a note, which
/// is the one state promotion refuses. The loser fails with the "already exists"
/// conflict and creates nothing. Single-instance boundary: a second app process is
/// not covered; the OS-level `create_new` still prevents overwriting.
async fn create_note(store: &dyn Store, slug: &str) -> Result<ProjectNoteStatus, ReportError> {
    let slug = slug.trim();
    if let Some(message) = slug_error(slug) {
        return Err(fail("INVALID_CONFIG", message));
    }
    let folder = output_folder(store).await?;
    let _guard = NOTE_LOCK.lock().await;
    let notes = project_notes(&folder)?;
    if !notes.is_empty() {
        let detail = if notes.len() == 1 {
            format!(
                "A project note already exists at {}.",
                path_string(&notes[0])
            )
        } else {
            format!(
                "More than one project note already exists ({}); keep exactly one.",
                notes
                    .iter()
                    .map(|path| path_string(path))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return Err(fail("FILE_CONFLICT", &detail));
    }
    let path = projects_dir(&folder).join(format!("{slug}.md"));
    let target = path.clone();
    // Do not create the hierarchy eagerly: `write_owned` validates every existing
    // ancestor (including reparse points) before creating the next missing
    // directory, so a junction under ScreenSearch is rejected before any external
    // `Projects` directory can be created through it.
    let write = tokio::task::spawn_blocking(move || write_owned(&target, "", None))
        .await
        .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match write {
        Ok(_) => Ok(status_of(&project_notes(&folder)?)),
        Err(error) if error.conflict => Err(fail(
            "FILE_CONFLICT",
            "The project note could not be created safely; a note or redirected folder already occupies that path.",
        )),
        Err(error) => Err(fail("WRITE_FAILED", &error.message)),
    }
}

/// Run the whole prepare core in an owned task. A dropped command await (window
/// close, IPC cancellation) detaches the task instead of the work, so any
/// recovery metadata it is settling still completes.
async fn owned_prepare(
    store: Arc<SqliteStore>,
    selection: PromotionSelection,
    at: i64,
) -> Result<PromotionProposal, ReportError> {
    tokio::spawn(async move { prepare(store.as_ref(), &selection, at).await })
        .await
        .map_err(|_| fail("STORE_ERROR", "The promotion task stopped unexpectedly."))?
}

/// Same ownership rule for confirm: the detached task keeps the NOTE_LOCK guard
/// and completes the native append plus every durable metadata step.
fn spawn_confirm(
    store: Arc<SqliteStore>,
    proposal: PromotionProposal,
) -> tokio::task::JoinHandle<Result<PromotionReceipt, ReportError>> {
    tokio::spawn(async move { confirm(store.as_ref(), &proposal).await })
}

async fn owned_confirm(
    store: Arc<SqliteStore>,
    proposal: PromotionProposal,
) -> Result<PromotionReceipt, ReportError> {
    spawn_confirm(store, proposal)
        .await
        .map_err(|_| fail("WRITE_FAILED", "The promotion task stopped unexpectedly."))?
}

async fn owned_create(
    store: Arc<SqliteStore>,
    slug: String,
) -> Result<ProjectNoteStatus, ReportError> {
    tokio::spawn(async move { create_note(store.as_ref(), &slug).await })
        .await
        .map_err(|_| fail("WRITE_FAILED", "The note task stopped unexpectedly."))?
}

/// Preview the exact blocks a confirmation would append. Writes no note bytes.
#[tauri::command]
pub async fn prepare_promotion(
    selection: PromotionSelection,
    state: State<'_, AppState>,
) -> Result<PromotionProposal, ReportError> {
    let store = db(&state)?;
    owned_prepare(store, selection, now_ms()).await
}

/// Append the previewed selection in one atomic multi-section write.
#[tauri::command]
pub async fn confirm_promotion(
    proposal: PromotionProposal,
    state: State<'_, AppState>,
) -> Result<PromotionReceipt, ReportError> {
    let store = db(&state)?;
    owned_confirm(store, proposal).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_fixture(kind: ReportRunKind, markdown: &str) -> ReportRun {
        ReportRun {
            requested_range: None,
            run_id: "run-fixture".into(),
            source_id: "source-fixture".into(),
            kind,
            schedule_id: None,
            revision_of: None,
            window_start: 0,
            window_end: 604_800_000,
            timezone: "UTC".into(),
            local_date: "2026-10-12".into(),
            phase: ReportRunPhase::Finished,
            outcome: Some(ReportOutcome::Ok),
            error: None,
            result: Some(SavedReport {
                markdown: markdown.into(),
                model: Some("fixture".into()),
                // Unique and count-matched like every persisted run: the store's
                // provenance invariant rejects duplicates at create_report_run.
                frame_ids: vec![102, 101],
                session_ids: vec![8, 7],
                cited_frame_ids: vec![101],
                frames_sampled: 2,
                frames_summarized: 2,
                periods_total: 1,
                periods_covered: 1,
                passes: 1,
                truncated: false,
                daily_run_ids: vec!["0a1b".into(), "1b2c".into()],
            }),
            config_snapshot: "{}".into(),
            created_at: 1,
            updated_at: 1,
        }
    }

    async fn store_with_folder(folder: &Path) -> SqliteStore {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .set_setting(
                OUTPUT_FOLDER,
                &serde_json::to_string(&Some(path_string(folder))).unwrap(),
            )
            .await
            .unwrap();
        store
    }

    /// File-backed store so a test can install a real SQLite trigger from a
    /// second connection (and reopen it to model a process restart).
    async fn file_store_with_folder(db: &Path, folder: &Path) -> SqliteStore {
        let store = SqliteStore::open_path(db).unwrap();
        store
            .set_setting(
                OUTPUT_FOLDER,
                &serde_json::to_string(&Some(path_string(folder))).unwrap(),
            )
            .await
            .unwrap();
        store
    }

    fn sql(db: &Path, statement: &str) {
        rusqlite::Connection::open(db)
            .unwrap()
            .execute_batch(statement)
            .unwrap();
    }

    fn seed_note(folder: &Path) -> PathBuf {
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        note
    }

    async fn seed_selection(store: &dyn Store) -> PromotionSelection {
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        PromotionSelection {
            run_id: run.run_id,
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        }
    }

    /// The exact bytes the writer installs for a proposal, plus their hash.
    async fn merged_for(
        store: &dyn Store,
        selection: &PromotionSelection,
        proposal: &PromotionProposal,
    ) -> (String, String) {
        let (run, result) = load_run(store, &selection.run_id).await.unwrap();
        let blocks = build_entries(
            &result,
            selection,
            &proposal.entry_date,
            &provenance(&run, &result),
        )
        .unwrap();
        let entries: Vec<(String, String)> = blocks
            .into_iter()
            .map(|block| (block.target.to_string(), block.block))
            .collect();
        let path = Path::new(proposal.note_path.as_deref().unwrap());
        let existing = std::fs::read_to_string(path).unwrap();
        let merged = merged_content(&existing, &entries);
        let hash = content_hash(merged.as_bytes());
        (merged, hash)
    }

    #[tokio::test]
    async fn promotion_requires_explicit_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Activity\n\n- Shipped the capture pipeline\n\n## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n- Keep the export local\n\n## Open questions\n\n- Do we need a second note?\n\n## Next steps\n\n- Wire the promotion preview\n";
        let run = run_fixture(ReportRunKind::WeeklySynthesis, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![
                PromotionItem {
                    section: PromotionSection::Decisions,
                    text: "Adopt the weekly cadence".into(),
                },
                PromotionItem {
                    section: PromotionSection::Decisions,
                    text: "Keep the export local".into(),
                },
                PromotionItem {
                    section: PromotionSection::Log,
                    text: "Shipped the capture pipeline".into(),
                },
            ],
        };
        let at = 1_760_000_000_000;
        let proposal = prepare(&store, &selection, at).await.unwrap();
        assert_eq!(
            proposal.note_path.as_deref(),
            Some(path_string(&note).as_str())
        );
        assert!(proposal.note_exists);
        assert_eq!(
            proposal.note_hash.as_deref(),
            Some(content_hash(b"").as_str())
        );
        assert_eq!(proposal.entry_date, local_date("UTC", at).unwrap());
        assert_eq!(proposal.entries.len(), 2);
        assert_eq!(proposal.entries[0].section, PromotionSection::Decisions);
        assert_eq!(proposal.entries[1].section, PromotionSection::Log);
        assert!(proposal.entries[0]
            .provenance
            .starts_with("Source: weekly synthesis run run-fixture"));
        // Preparing is a read: the note bytes and the ownership table are untouched.
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        assert!(receipt.error.is_none());
        let written = std::fs::read_to_string(&note).unwrap();
        let expected = format!(
            "### {}\n\n- Adopt the weekly cadence\n- Keep the export local\n\n*Source: weekly synthesis run run-fixture; daily runs 0a1b, 1b2c; frames 101, 102; sessions 7, 8.*",
            proposal.entry_date
        );
        assert!(
            written.contains(&expected),
            "the entry block must match the frozen format: {written}"
        );
        assert_eq!(written.matches("*Source:").count(), 2);
        assert_eq!(
            receipt.path,
            path_string(&note),
            "the receipt names the written note"
        );
        assert_eq!(
            receipt.hash.as_deref(),
            Some(content_hash(written.as_bytes()).as_str())
        );
        let owned = store.list_report_files().await.unwrap();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].path, receipt.path);
        assert_eq!(owned[0].run_id, run.run_id);
        assert_eq!(owned[0].source_id, run.source_id);
        assert_eq!(Some(owned[0].hash.clone()), receipt.hash);
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            written,
            "a successful promotion rewrites nothing it appended"
        );
    }

    #[tokio::test]
    async fn promotion_to_edited_note_surfaces_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        // The user edits the note between select and confirm.
        let edited = b"## My own notes\n\n- hand written, never touched\n";
        std::fs::write(&note, edited).unwrap();
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(receipt.conflict, "an edited note must surface a conflict");
        assert_eq!(
            receipt.error.as_ref().map(|error| error.code.as_str()),
            Some("FILE_CONFLICT")
        );
        assert_eq!(
            std::fs::read(&note).unwrap(),
            edited.to_vec(),
            "the user's bytes are preserved byte-for-byte"
        );
        assert!(
            store.list_report_files().await.unwrap().is_empty(),
            "a conflict records no ownership"
        );
    }

    #[tokio::test]
    async fn create_project_note_rejects_reserved_slug_names() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        // `con.md` is the console device, not a file: every reserved DOS name is
        // refused by the shared create core, and nothing is created.
        for slug in RESERVED_SLUGS {
            let error = create_note(&store, slug).await.unwrap_err();
            assert_eq!(error.code, "INVALID_CONFIG", "{slug}: {}", error.message);
            assert!(
                error.message.contains("reserved"),
                "{slug} must fail for being reserved: {}",
                error.message
            );
            assert!(
                project_notes(&folder).unwrap().is_empty(),
                "{slug} must not create a note"
            );
        }
        // An upper-case device name never reaches the denylist: the slug charset
        // refuses it first, and nothing is created either way.
        assert_eq!(
            create_note(&store, "CON").await.unwrap_err().code,
            "INVALID_CONFIG"
        );
        assert!(project_notes(&folder).unwrap().is_empty());
        // Near-miss names are ordinary slugs: `console` and `com10` merely start
        // with a reserved prefix and must not be rejected by the denylist.
        assert!(valid_slug("console"));
        assert!(valid_slug("com10"));
        assert!(valid_slug("lpt10"));
        // `COM0`/`LPT0` are absent from Microsoft's reserved-name list, so the
        // denylist must not widen beyond COM1–COM9/LPT1–LPT9.
        assert!(valid_slug("com0"));
        assert!(valid_slug("lpt0"));
        // A normal slug still works, so the denylist is not a blanket refusal.
        assert_eq!(create_note(&store, "console").await.unwrap().count, 1);
    }

    #[tokio::test]
    async fn confirm_refuses_a_forged_or_stale_note_target() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let ghost = note_dir.join("ghost.md");
        // A forged proposal that drops the hash and names an absent note must not
        // reach the writer: its (None, None) case would create the file.
        let mut forged = proposal.clone();
        forged.note_path = Some(path_string(&ghost));
        forged.note_hash = None;
        let error = confirm(&store, &forged).await.unwrap_err();
        assert_eq!(error.code, "INVALID_CONFIG");
        assert!(
            !ghost.exists(),
            "confirm must never invent the note it writes to"
        );
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
        // A stale path with a plausible hash is the documented conflict receipt: no
        // write, the real note untouched, no ownership recorded.
        let mut stale = proposal.clone();
        stale.note_path = Some(path_string(&ghost));
        stale.note_hash = Some(content_hash(b"anything"));
        let receipt = confirm(&store, &stale).await.unwrap();
        assert!(receipt.conflict);
        assert_eq!(
            receipt.error.as_ref().map(|error| error.code.as_str()),
            Some("FILE_CONFLICT")
        );
        assert_eq!(
            receipt.path,
            path_string(&note),
            "the receipt names the note the app resolved"
        );
        assert!(!ghost.exists());
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
        // An honest delete race (zero notes now) is an explicit error, never a
        // create: the app does not put the note back.
        std::fs::remove_file(&note).unwrap();
        assert_eq!(
            confirm(&store, &proposal).await.unwrap_err().code,
            "NOT_FOUND"
        );
        assert!(!note.exists(), "confirm must not recreate the note");
    }

    #[tokio::test]
    async fn duplicate_selected_items_are_all_promoted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        // The report legitimately repeats a bullet; the selection is item-level, so
        // ticking both entries promotes both lines in selection order.
        let markdown = "## Explicitly evidenced decisions\n\n- Repeat the same bullet\n- Repeat the same bullet\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let item = PromotionItem {
            section: PromotionSection::Decisions,
            text: "Repeat the same bullet".into(),
        };
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![item.clone(), item],
        };
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert_eq!(
            proposal.entries[0].items,
            vec![
                "Repeat the same bullet".to_string(),
                "Repeat the same bullet".to_string()
            ],
            "the preview keeps the multiplicity the user selected"
        );
        confirm(&store, &proposal).await.unwrap();
        let written = std::fs::read_to_string(&note).unwrap();
        assert_eq!(
            written.matches("- Repeat the same bullet").count(),
            2,
            "both selected lines are written: {written}"
        );
    }

    #[tokio::test]
    async fn excess_duplicate_selected_items_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        // The report contains one occurrence; selecting the same text twice must
        // fail rather than fabricate a second persisted bullet.
        let markdown = "## Explicitly evidenced decisions\n\n- One persisted copy\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let item = PromotionItem {
            section: PromotionSection::Decisions,
            text: "One persisted copy".into(),
        };
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![item.clone(), item],
        };
        let error = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_CONFIG", "{}", error.message);
        assert!(
            error
                .message
                .contains("more times than the saved report contains"),
            "the rejection must name the multiplicity violation: {}",
            error.message
        );
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
    }

    #[test]
    fn section_bullets_ignores_fenced_headings_and_short_or_mismatched_closers() {
        // ```` opens a four-backtick fence; the three-backtick line inside is
        // content, not a close, so the heading and bullet stay fenced.
        let markdown = "## Activity\n\n- first\n\n````text\n## Explicitly evidenced decisions\n- fenced fake\n```\n- still fenced\n````\n\n- last\n\n## Explicitly evidenced decisions\n\n- real decision\n";
        assert_eq!(
            section_bullets(markdown, "Activity"),
            vec!["first".to_string(), "last".to_string()]
        );
        assert_eq!(
            section_bullets(markdown, "Explicitly evidenced decisions"),
            vec!["real decision".to_string()]
        );
        // A `~~~` fence is not closed by a ``` marker, regardless of length.
        let mismatched = "## Activity\n\n~~~\n## Explicitly evidenced decisions\n- fake\n```\n- still fenced\n~~~\n\n- last\n";
        assert_eq!(
            section_bullets(mismatched, "Activity"),
            vec!["last".to_string()],
            "the mismatched closer must not end the fence"
        );
        assert!(section_bullets(mismatched, "Explicitly evidenced decisions").is_empty());
    }

    #[tokio::test]
    async fn ownership_blocks_prepare_after_a_user_edit() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let at = 1_760_000_000_000;
        // No ownership row yet: the first promotion legitimately adopts the note.
        let first = prepare(&store, &selection, at).await.unwrap();
        assert_eq!(
            first.note_hash.as_deref(),
            Some(content_hash(b"").as_str()),
            "the unowned note's live bytes are the adoption hash"
        );
        let receipt = confirm(&store, &first).await.unwrap();
        assert!(!receipt.conflict);
        let row = store
            .get_report_file(&path_string(&note))
            .await
            .unwrap()
            .expect("the first promotion records ownership");
        assert_eq!(row.source_id, run.source_id);
        assert_eq!(Some(row.hash.clone()), receipt.hash);
        let written = std::fs::read(&note).unwrap();
        // A user edit before the next prepare: fail closed, leave bytes and row
        // exactly as they were, and never adopt the edited bytes.
        let edited = format!("{}\n- my own line\n", String::from_utf8_lossy(&written));
        std::fs::write(&note, edited.as_bytes()).unwrap();
        let error = prepare(&store, &selection, at).await.unwrap_err();
        assert_eq!(error.code, "FILE_CONFLICT", "{}", error.message);
        assert_eq!(std::fs::read(&note).unwrap(), edited.as_bytes().to_vec());
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap(),
            row,
            "a rejected prepare must not rewrite ownership"
        );
    }

    #[tokio::test]
    async fn ownership_blocks_confirm_after_an_edit_or_forged_hash() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let at = 1_760_000_000_000;
        confirm(&store, &prepare(&store, &selection, at).await.unwrap())
            .await
            .unwrap();
        let row = store
            .get_report_file(&path_string(&note))
            .await
            .unwrap()
            .unwrap();
        let untouched = std::fs::read(&note).unwrap();
        // The note is owned and unchanged: prepare is allowed.
        let proposal = prepare(&store, &selection, at).await.unwrap();
        let edited = format!("{}\n- my own line\n", String::from_utf8_lossy(&untouched));
        std::fs::write(&note, edited.as_bytes()).unwrap();
        // An edit between prepare and confirm conflicts even though the proposal's
        // selection-time hash is honest.
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(receipt.conflict);
        assert_eq!(
            receipt.error.as_ref().map(|error| error.code.as_str()),
            Some("FILE_CONFLICT")
        );
        assert_eq!(std::fs::read(&note).unwrap(), edited.as_bytes().to_vec());
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap(),
            row
        );
        // A forged proposal that presents the edited bytes' hash still loses: the
        // persisted row's hash must match the live bytes too.
        let mut forged = proposal.clone();
        forged.note_hash = Some(content_hash(edited.as_bytes()));
        let receipt = confirm(&store, &forged).await.unwrap();
        assert!(
            receipt.conflict,
            "a client-supplied matching hash cannot bypass the ownership row"
        );
        assert_eq!(
            receipt.error.as_ref().map(|error| error.code.as_str()),
            Some("FILE_CONFLICT")
        );
        assert_eq!(std::fs::read(&note).unwrap(), edited.as_bytes().to_vec());
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap(),
            row,
            "a forged confirm must not replace the ownership row"
        );
    }

    #[tokio::test]
    async fn ownership_requires_same_source_and_allows_a_new_run() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let first_run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(first_run.clone()).await.unwrap();
        let item = PromotionItem {
            section: PromotionSection::Decisions,
            text: "Adopt the weekly cadence".into(),
        };
        let first_selection = PromotionSelection {
            run_id: first_run.run_id.clone(),
            items: vec![item.clone()],
        };
        let at = 1_760_000_000_000;
        confirm(
            &store,
            &prepare(&store, &first_selection, at).await.unwrap(),
        )
        .await
        .unwrap();
        // A new run from the SAME source may append to the unchanged owned note;
        // source identity, not run identity, is the ownership boundary.
        let mut second_run = run_fixture(ReportRunKind::Daily, markdown);
        second_run.run_id = "run-second".into();
        store.create_report_run(second_run.clone()).await.unwrap();
        let second_selection = PromotionSelection {
            run_id: second_run.run_id.clone(),
            items: vec![item],
        };
        let proposal = prepare(&store, &second_selection, at).await.unwrap();
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        let row_after_second = store
            .get_report_file(&path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row_after_second.source_id, second_run.source_id);
        assert_eq!(row_after_second.run_id, second_run.run_id);
        assert_eq!(
            Some(row_after_second.hash.clone()),
            receipt.hash,
            "the row follows the latest successful run"
        );
        let bytes_after_second = std::fs::read(&note).unwrap();
        // A different source can neither prepare nor forge a confirm against the
        // note the first source owns.
        let mut foreign_run = run_fixture(ReportRunKind::Daily, markdown);
        foreign_run.run_id = "run-foreign".into();
        foreign_run.source_id = "source-foreign".into();
        store.create_report_run(foreign_run.clone()).await.unwrap();
        let foreign_selection = PromotionSelection {
            run_id: foreign_run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let error = prepare(&store, &foreign_selection, at).await.unwrap_err();
        assert_eq!(error.code, "FILE_CONFLICT", "{}", error.message);
        let mut forged = proposal.clone();
        forged.selection.run_id = foreign_run.run_id.clone();
        forged.run_id = foreign_run.run_id.clone();
        forged.note_hash = Some(row_after_second.hash.clone());
        let receipt = confirm(&store, &forged).await.unwrap();
        assert!(receipt.conflict, "another source must not take ownership");
        assert_eq!(std::fs::read(&note).unwrap(), bytes_after_second);
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap(),
            row_after_second,
            "the owner's row and bytes survive the foreign attempt"
        );
    }

    #[tokio::test]
    async fn promotion_refuses_multiple_project_notes() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let note_dir = folder.join("ScreenSearch").join("Projects");
        std::fs::create_dir_all(&note_dir).unwrap();
        let note = note_dir.join("launch.md");
        std::fs::write(&note, "").unwrap();
        let markdown = "## Explicitly evidenced decisions\n\n- Adopt the weekly cadence\n";
        let run = run_fixture(ReportRunKind::Daily, markdown);
        store.create_report_run(run.clone()).await.unwrap();
        let selection = PromotionSelection {
            run_id: run.run_id.clone(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Adopt the weekly cadence".into(),
            }],
        };
        let at = 1_760_000_000_000;
        let proposal = prepare(&store, &selection, at).await.unwrap();
        let second = note_dir.join("second.md");
        std::fs::write(&second, "").unwrap();
        // The unsupported two-note state is explicit at both prepare and confirm;
        // neither call writes a note or records ownership.
        assert_eq!(
            prepare(&store, &selection, at).await.unwrap_err().code,
            "FILE_CONFLICT"
        );
        assert_eq!(
            confirm(&store, &proposal).await.unwrap_err().code,
            "FILE_CONFLICT"
        );
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert_eq!(std::fs::read(&second).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn concurrent_create_project_note_yields_exactly_one_note() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let store = store_with_folder(&folder).await;
        let (first, second) =
            tokio::join!(create_note(&store, "alpha"), create_note(&store, "beta"));
        assert_eq!(
            usize::from(first.is_ok()) + usize::from(second.is_ok()),
            1,
            "two concurrent creates cannot both win: {first:?} / {second:?}"
        );
        let loser = first.as_ref().err().or(second.as_ref().err()).unwrap();
        assert_eq!(loser.code, "FILE_CONFLICT");
        let notes = project_notes(&folder).unwrap();
        assert_eq!(notes.len(), 1, "exactly one note survives the race");
        let winner = first.as_ref().ok().or(second.as_ref().ok()).unwrap();
        assert_eq!(
            winner.path.as_deref(),
            Some(path_string(&notes[0]).as_str()),
            "the winner's status names the one note"
        );
    }

    #[tokio::test]
    async fn intent_persist_failure_leaves_note_and_ownership_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        // Real SQLite fault: the pre-write intent cannot be inserted.
        sql(
            &db,
            r#"CREATE TRIGGER fail_intent_insert BEFORE INSERT ON settings
               WHEN NEW.key LIKE 'memory.promotion.pending.%'
               BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END;"#,
        );
        let error = confirm(&store, &proposal).await.unwrap_err();
        assert_eq!(error.code, "STORE_ERROR");
        assert_eq!(
            std::fs::read(&note).unwrap(),
            Vec::<u8>::new(),
            "a pre-write persistence failure must not touch the note"
        );
        assert!(store.list_report_files().await.unwrap().is_empty());
        assert!(read_intent(&store, &path_string(&note))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn landed_append_with_failed_ownership_is_finished_by_a_retry_without_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        sql(
            &db,
            r#"CREATE TRIGGER fail_owner_write BEFORE INSERT ON report_files
               BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;"#,
        );
        let error = confirm(&store, &proposal).await.unwrap_err();
        assert_eq!(error.code, "STORE_ERROR");
        let written = std::fs::read_to_string(&note).unwrap();
        assert_eq!(
            written.matches("Adopt the weekly cadence").count(),
            1,
            "the append itself landed once: {written}"
        );
        assert!(store.list_report_files().await.unwrap().is_empty());
        let pending = read_intent(&store, &path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pending.old_hash, proposal.note_hash.clone().unwrap());
        assert_eq!(pending.new_hash, content_hash(written.as_bytes()));
        assert_eq!(pending.run_id, proposal.run_id);
        // Restore the real store, then retry the exact confirmed proposal. The
        // intent proves the app's own bytes landed; recovery must finish the
        // ownership row instead of rejecting them or appending again.
        sql(&db, "DROP TRIGGER fail_owner_write;");
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        assert_eq!(receipt.hash.as_deref(), Some(pending.new_hash.as_str()));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), written);
        assert_eq!(
            written.matches("Adopt the weekly cadence").count(),
            1,
            "the retry must not append the same selection twice"
        );
        let owner = store
            .get_report_file(&path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(owner.hash, pending.new_hash);
        assert_eq!(owner.run_id, pending.run_id);
        let committed = read_receipt(&store, &path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert!(
            committed.committed,
            "success leaves a last-receipt tombstone"
        );
    }

    #[tokio::test]
    async fn owned_note_recovers_after_append_when_ownership_update_fails() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let path = path_string(&note);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let first = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert!(!confirm(&store, &first).await.unwrap().conflict);
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let old_owner = store.get_report_file(&path).await.unwrap().unwrap();
        sql(
            &db,
            r#"CREATE TRIGGER fail_owner_write BEFORE INSERT ON report_files
               BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;"#,
        );
        assert_eq!(
            confirm(&store, &proposal).await.unwrap_err().code,
            "STORE_ERROR"
        );
        let written = std::fs::read_to_string(&note).unwrap();
        assert_eq!(written.matches("Adopt the weekly cadence").count(), 2);
        assert_eq!(
            store.get_report_file(&path).await.unwrap().unwrap().hash,
            old_owner.hash
        );
        assert_ne!(old_owner.hash, content_hash(written.as_bytes()));
        sql(&db, "DROP TRIGGER fail_owner_write;");
        let retry = confirm(&store, &proposal).await.unwrap();
        assert!(!retry.conflict);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), written);
        let preview = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert_eq!(preview.note_hash, retry.hash);
        assert_eq!(
            store.get_report_file(&path).await.unwrap().unwrap().hash,
            content_hash(written.as_bytes())
        );
    }

    #[tokio::test]
    async fn detached_confirmation_finishes_its_file_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let note = seed_note(&folder);
        let store = Arc::new(store_with_folder(&folder).await);
        let selection = seed_selection(store.as_ref()).await;
        let proposal = prepare(store.as_ref(), &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let held = NOTE_LOCK.lock().await;
        // This is the same owned task the IPC command awaits. Dropping its
        // response handle must not cancel confirmed work, even before it locks.
        drop(spawn_confirm(Arc::clone(&store), proposal));
        drop(held);
        let receipt = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(receipt) = read_receipt(store.as_ref(), &path_string(&note))
                    .await
                    .unwrap()
                {
                    break receipt;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached confirmation must finish its committed receipt");
        let _settled = NOTE_LOCK.lock().await;
        assert!(receipt.committed);
        let written = std::fs::read_to_string(&note).unwrap();
        assert_eq!(written.matches("Adopt the weekly cadence").count(), 1);
        assert_eq!(content_hash(written.as_bytes()), receipt.new_hash);
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .hash,
            receipt.new_hash
        );
    }

    #[tokio::test]
    async fn success_cleanup_failure_is_idempotent_and_does_not_block_prepare() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let (merged, new_hash) = merged_for(&store, &selection, &proposal).await;
        // The intent insert must work; only the later cleanup UPDATE fails.
        sql(
            &db,
            r#"CREATE TRIGGER fail_intent_clear BEFORE UPDATE ON settings
               WHEN NEW.key LIKE 'memory.promotion.pending.%'
               BEGIN SELECT RAISE(ABORT, 'injected cleanup failure'); END;"#,
        );
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        assert_eq!(receipt.hash.as_deref(), Some(new_hash.as_str()));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), merged);
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .hash,
            new_hash
        );
        assert!(
            read_intent(&store, &path_string(&note))
                .await
                .unwrap()
                .is_some(),
            "the failed cleanup leaves the intent for the idempotent next pass"
        );
        // A later preview repeats the same no-op reconciliation and still works.
        let next = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert_eq!(next.note_hash.as_deref(), Some(new_hash.as_str()));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), merged);
    }

    #[tokio::test]
    async fn confirmed_write_interrupted_before_ownership_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let (merged, new_hash) = merged_for(&store, &selection, &proposal).await;
        write_intent(
            &store,
            &PromotionIntent {
                path: path_string(&note),
                source_id: "source-fixture".into(),
                run_id: proposal.run_id.clone(),
                old_hash: proposal.note_hash.clone().unwrap(),
                new_hash: new_hash.clone(),
                fingerprint: promotion_fingerprint(&selection, &proposal.entry_date),
                committed: false,
            },
        )
        .await
        .unwrap();
        std::fs::write(&note, &merged).unwrap();
        drop(store);
        // A fresh store models the restart: the intent and note are durable.
        let store = SqliteStore::open_path(&db).unwrap();
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        assert_eq!(receipt.hash.as_deref(), Some(new_hash.as_str()));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), merged);
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .hash,
            new_hash
        );
        assert!(
            read_receipt(&store, &path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .committed
        );
    }

    #[tokio::test]
    async fn stale_prewrite_intent_is_discarded_without_touching_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let first = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let old_hash = first.note_hash.clone().unwrap();
        write_intent(
            &store,
            &PromotionIntent {
                path: path_string(&note),
                source_id: "source-fixture".into(),
                run_id: first.run_id.clone(),
                old_hash: old_hash.clone(),
                new_hash: content_hash(b"never installed"),
                fingerprint: promotion_fingerprint(&selection, &first.entry_date),
                committed: false,
            },
        )
        .await
        .unwrap();
        drop(store);
        let store = SqliteStore::open_path(&db).unwrap();
        let again = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert_eq!(again.note_hash.as_deref(), Some(old_hash.as_str()));
        assert_eq!(std::fs::read(&note).unwrap(), Vec::<u8>::new());
        assert!(store.list_report_files().await.unwrap().is_empty());
        assert!(read_intent(&store, &path_string(&note))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn unrelated_edit_after_a_landed_append_still_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        sql(
            &db,
            r#"CREATE TRIGGER fail_owner_write BEFORE INSERT ON report_files
               BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;"#,
        );
        assert_eq!(
            confirm(&store, &proposal).await.unwrap_err().code,
            "STORE_ERROR"
        );
        sql(&db, "DROP TRIGGER fail_owner_write;");
        let written = std::fs::read_to_string(&note).unwrap();
        let edited = format!("{written}\n- my own line\n");
        std::fs::write(&note, &edited).unwrap();
        let error = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap_err();
        assert_eq!(error.code, "FILE_CONFLICT");
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(receipt.conflict, "a user edit must never be adopted");
        assert_eq!(std::fs::read_to_string(&note).unwrap(), edited);
        assert!(store.list_report_files().await.unwrap().is_empty());
        assert!(
            read_intent(&store, &path_string(&note))
                .await
                .unwrap()
                .is_some(),
            "the unresolved edit keeps the evidence instead of guessing"
        );
    }

    #[tokio::test]
    async fn prepare_waits_for_the_writer_guard_before_reconciling() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let note = seed_note(&folder);
        let store = Arc::new(store_with_folder(&folder).await);
        let selection = seed_selection(store.as_ref()).await;
        let proposal = prepare(store.as_ref(), &selection, 1_760_000_000_000)
            .await
            .unwrap();
        // Model a detached native writer that still owns NOTE_LOCK.
        let held = NOTE_LOCK.lock().await;
        let task_store = Arc::clone(&store);
        let task_selection = selection.clone();
        let mut task = tokio::spawn(async move {
            prepare(task_store.as_ref(), &task_selection, 1_760_000_000_000).await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut task)
                .await
                .is_err(),
            "prepare must wait while the non-cancellable writer can still land"
        );
        let (merged, new_hash) = merged_for(store.as_ref(), &selection, &proposal).await;
        write_intent(
            store.as_ref(),
            &PromotionIntent {
                path: path_string(&note),
                source_id: "source-fixture".into(),
                run_id: proposal.run_id.clone(),
                old_hash: proposal.note_hash.clone().unwrap(),
                new_hash: new_hash.clone(),
                fingerprint: promotion_fingerprint(&selection, &proposal.entry_date),
                committed: false,
            },
        )
        .await
        .unwrap();
        std::fs::write(&note, &merged).unwrap();
        drop(held);
        let prepared = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(prepared.note_hash.as_deref(), Some(new_hash.as_str()));
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .hash,
            new_hash
        );
        assert!(
            read_receipt(store.as_ref(), &path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .committed
        );
    }

    #[tokio::test]
    async fn lost_success_response_retry_returns_the_committed_receipt_without_appending() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = Arc::new(file_store_with_folder(&db, &folder).await);
        let selection = seed_selection(store.as_ref()).await;
        let proposal = prepare(store.as_ref(), &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let first = owned_confirm(Arc::clone(&store), proposal.clone())
            .await
            .unwrap();
        assert!(!first.conflict);
        let written = std::fs::read_to_string(&note).unwrap();
        // The caller lost the first response; the same proposal is retried.
        let retry = owned_confirm(Arc::clone(&store), proposal).await.unwrap();
        assert!(!retry.conflict);
        assert_eq!(retry.hash, first.hash);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), written);
        assert_eq!(written.matches("Adopt the weekly cadence").count(), 1);
    }

    #[tokio::test]
    async fn recovery_does_not_false_success_a_different_selection_or_date() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        sql(
            &db,
            r#"CREATE TRIGGER fail_owner_write BEFORE INSERT ON report_files
               BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;"#,
        );
        assert_eq!(
            confirm(&store, &proposal).await.unwrap_err().code,
            "STORE_ERROR"
        );
        sql(&db, "DROP TRIGGER fail_owner_write;");
        let written = std::fs::read_to_string(&note).unwrap();
        let mut different = proposal.clone();
        different.entry_date = "2026-10-13".into();
        let receipt = confirm(&store, &different).await.unwrap();
        assert!(
            receipt.conflict,
            "same old hash and run but a different date/selection must not inherit success"
        );
        assert_eq!(std::fs::read_to_string(&note).unwrap(), written);
        assert_eq!(written.matches("Adopt the weekly cadence").count(), 1);
    }

    #[tokio::test]
    async fn switching_output_folder_keeps_the_other_target_intent_recoverable() {
        let dir = tempfile::tempdir().unwrap();
        let folder_a = dir.path().join("vault-a");
        let folder_b = dir.path().join("vault-b");
        let db = dir.path().join("memory.sqlite");
        let note_a = seed_note(&folder_a);
        seed_note(&folder_b);
        let store = file_store_with_folder(&db, &folder_a).await;
        let selection = seed_selection(&store).await;
        let proposal_a = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let (merged_a, new_a) = merged_for(&store, &selection, &proposal_a).await;
        write_intent(
            &store,
            &PromotionIntent {
                path: path_string(&note_a),
                source_id: "source-fixture".into(),
                run_id: proposal_a.run_id.clone(),
                old_hash: proposal_a.note_hash.clone().unwrap(),
                new_hash: new_a.clone(),
                fingerprint: promotion_fingerprint(&selection, &proposal_a.entry_date),
                committed: false,
            },
        )
        .await
        .unwrap();
        std::fs::write(&note_a, &merged_a).unwrap();
        // Switch to another folder and confirm a new promotion there. It must
        // not overwrite the unresolved intent for the first exact target.
        store
            .set_setting(
                OUTPUT_FOLDER,
                &serde_json::to_string(&Some(path_string(&folder_b))).unwrap(),
            )
            .await
            .unwrap();
        let proposal_b = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        assert!(!confirm(&store, &proposal_b).await.unwrap().conflict);
        let pending_a = read_intent(&store, &path_string(&note_a))
            .await
            .unwrap()
            .unwrap();
        assert!(!pending_a.committed);
        // Return to the first folder: the original intent still recovers.
        store
            .set_setting(
                OUTPUT_FOLDER,
                &serde_json::to_string(&Some(path_string(&folder_a))).unwrap(),
            )
            .await
            .unwrap();
        let receipt = confirm(&store, &proposal_a).await.unwrap();
        assert!(!receipt.conflict);
        assert_eq!(receipt.hash.as_deref(), Some(new_a.as_str()));
        assert_eq!(std::fs::read_to_string(&note_a).unwrap(), merged_a);
        assert_eq!(
            store
                .get_report_file(&path_string(&note_a))
                .await
                .unwrap()
                .unwrap()
                .hash,
            new_a
        );
    }

    #[tokio::test]
    async fn confirmed_intent_recovers_before_a_pruned_source_run_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let db = dir.path().join("memory.sqlite");
        let note = seed_note(&folder);
        let store = file_store_with_folder(&db, &folder).await;
        let selection = PromotionSelection {
            run_id: "pruned-run".into(),
            items: vec![PromotionItem {
                section: PromotionSection::Decisions,
                text: "Already promoted".into(),
            }],
        };
        let proposal = PromotionProposal {
            selection: selection.clone(),
            run_id: selection.run_id.clone(),
            note_path: Some(path_string(&note)),
            note_exists: true,
            note_hash: Some(content_hash(b"")),
            entry_date: "2026-10-12".into(),
            entries: vec![],
        };
        let landed = "# Launch\n\n## Decisions\n\n### 2026-10-12\n\n- Already promoted\n";
        let landed_hash = content_hash(landed.as_bytes());
        write_intent(
            &store,
            &PromotionIntent {
                path: path_string(&note),
                source_id: "source-fixture".into(),
                run_id: selection.run_id.clone(),
                old_hash: content_hash(b""),
                new_hash: landed_hash.clone(),
                fingerprint: promotion_fingerprint(&selection, &proposal.entry_date),
                committed: false,
            },
        )
        .await
        .unwrap();
        std::fs::write(&note, landed).unwrap();
        // No report_runs row exists: only the durable intent can settle this.
        let receipt = confirm(&store, &proposal).await.unwrap();
        assert!(!receipt.conflict);
        assert_eq!(receipt.hash.as_deref(), Some(landed_hash.as_str()));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), landed);
        assert_eq!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .hash,
            landed_hash
        );
    }

    #[cfg(windows)]
    /// The settled state `write_owned` leaves on a concurrent swap: the note
    /// holds this promotion's bytes, the version the swap put there sits in the
    /// recovery copy, and the writer reports a conflict. Real files, real store.
    async fn swap_fixture(
        live_is_promotion: bool,
    ) -> (
        tempfile::TempDir,
        SqliteStore,
        PathBuf,
        PathBuf,
        PromotionIntent,
        String,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let note = seed_note(&folder);
        let store = store_with_folder(&folder).await;
        let selection = seed_selection(&store).await;
        let proposal = prepare(&store, &selection, 1_760_000_000_000)
            .await
            .unwrap();
        let (merged, new_hash) = merged_for(&store, &selection, &proposal).await;
        let intent = PromotionIntent {
            path: path_string(&note),
            source_id: "source-fixture".into(),
            run_id: proposal.run_id.clone(),
            old_hash: proposal.note_hash.clone().unwrap(),
            new_hash,
            fingerprint: promotion_fingerprint(&selection, &proposal.entry_date),
            committed: false,
        };
        write_intent(&store, &intent).await.unwrap();
        let recovery = note.with_file_name(".screensearch-recovery-swap.md");
        let edit = "edited in another program during the promotion\n".to_string();
        if live_is_promotion {
            // ReplaceFileW installed the promotion over the swapped-in edit.
            std::fs::write(&note, &merged).unwrap();
            std::fs::write(&recovery, &edit).unwrap();
        } else {
            // Another writer replaced the installed promotion again.
            std::fs::write(&note, &edit).unwrap();
            std::fs::write(&recovery, "").unwrap();
        }
        (dir, store, note, recovery, intent, edit)
    }

    #[tokio::test]
    async fn writer_conflict_after_install_reports_the_append_and_records_it() {
        let (_dir, store, note, recovery, intent, edit) = swap_fixture(true).await;
        let merged = std::fs::read_to_string(&note).unwrap();
        let receipt = settle_conflict(
            &store,
            path_string(&note),
            &intent,
            WriteFailure {
                message: "note changed during replacement; original retained in recovery copy"
                    .into(),
                conflict: true,
                recovery_path: Some(path_string(&recovery)),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            receipt.hash.as_deref(),
            Some(intent.new_hash.as_str()),
            "an installed append must never be reported as nothing written"
        );
        assert!(receipt.conflict);
        assert_eq!(receipt.error.as_ref().unwrap().code, "FILE_CONFLICT");
        assert_eq!(
            receipt.recovery_path.as_deref(),
            Some(path_string(&recovery).as_str())
        );
        // Ownership agrees with the disk now, not on a later silent reconcile.
        let owner = store
            .get_report_file(&path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(owner.hash, intent.new_hash);
        assert_eq!(owner.run_id, intent.run_id);
        assert!(read_intent(&store, &path_string(&note))
            .await
            .unwrap()
            .is_none());
        assert!(
            read_receipt(&store, &path_string(&note))
                .await
                .unwrap()
                .unwrap()
                .committed
        );
        // Nothing was deleted or rewritten: the edit survives in the copy.
        assert_eq!(std::fs::read_to_string(&note).unwrap(), merged);
        assert_eq!(std::fs::read_to_string(&recovery).unwrap(), edit);
    }

    #[tokio::test]
    async fn writer_conflict_without_the_append_installed_appends_nothing() {
        let (_dir, store, note, recovery, intent, edit) = swap_fixture(false).await;
        let receipt = settle_conflict(
            &store,
            path_string(&note),
            &intent,
            WriteFailure {
                message: "note changed during installation; inspect the destination and retained recovery copy".into(),
                conflict: true,
                recovery_path: Some(path_string(&recovery)),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            receipt.hash, None,
            "the live note does not hold this promotion"
        );
        assert!(receipt.conflict);
        assert_eq!(
            receipt.recovery_path.as_deref(),
            Some(path_string(&recovery).as_str())
        );
        assert!(
            store
                .get_report_file(&path_string(&note))
                .await
                .unwrap()
                .is_none(),
            "bytes the app did not write are never adopted"
        );
        let pending = read_intent(&store, &path_string(&note))
            .await
            .unwrap()
            .unwrap();
        assert!(
            !pending.committed,
            "the next call still decides from settled bytes"
        );
        assert_eq!(std::fs::read_to_string(&note).unwrap(), edit);
    }

    #[tokio::test]
    async fn create_project_note_rejects_a_junction_without_external_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("vault");
        let external = dir.path().join("outside");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(external.join("keep.txt"), "untouched").unwrap();
        let link = folder.join("ScreenSearch");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&external)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "the regression needs a real junction, not a skipped test"
        );
        let store = store_with_folder(&folder).await;
        let error = create_note(&store, "launch").await.unwrap_err();
        assert_eq!(error.code, "FILE_CONFLICT");
        assert!(
            !external.join("Projects").exists(),
            "the absent Projects directory must not be created through the junction"
        );
        assert_eq!(
            std::fs::read(external.join("keep.txt")).unwrap(),
            b"untouched"
        );
        assert!(project_notes(&folder).unwrap().is_empty());
    }
}
