//! Promotion into the single user-created project note (`03 §8e`).
//!
//! The note at `<selected folder>/ScreenSearch/Projects/<slug>.md` is created
//! empty by the user and is the only accumulation target. Promotion is the only
//! write path: `prepare_promotion` reads and validates a saved weekly synthesis
//! or daily report and previews dated entry blocks without writing anything;
//! `confirm_promotion` rebuilds those blocks and appends them in one atomic
//! write. There is no proposal persistence and no HTTP/MCP surface.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use store::SqliteStore;
use tauri::State;
use traits::*;

use crate::memory_files::{
    append_entries_owned, content_hash, heading_title, native_path, path_string, safe_read_owned,
    write_owned, MarkdownFence, WriteFailure,
};
use crate::{now_ms, AppState};

/// Persisted memory setting holding the selected output folder (`03 §8c`).
const OUTPUT_FOLDER: &str = "memory.schedule.output_folder";
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
/// This is a tokio lock (never held across a blocking mutex wait); the app is
/// single-instance, so a second process is outside this boundary.
static NOTE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

/// Read-and-validate only: no file, note or ownership row is touched.
async fn prepare(
    store: &dyn Store,
    selection: &PromotionSelection,
    at: i64,
) -> Result<PromotionProposal, ReportError> {
    let (run, result) = load_run(store, &selection.run_id).await?;
    let date = local_date(&run.timezone, at)?;
    let source_line = provenance(&run, &result);
    let blocks = build_entries(&result, selection, &date, &source_line)?;
    let folder = output_folder(store).await?;
    let notes = project_notes(&folder)?;
    let note = single_note(&notes)?;
    let note_path = path_string(note);
    let note_hash = read_owned(note)
        .await
        .map_err(read_error)?
        .map(|bytes| content_hash(&bytes));
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
    let (run, result) = load_run(store, &proposal.selection.run_id).await?;
    let date = entry_date(&proposal.entry_date)?;
    let source_line = provenance(&run, &result);
    let blocks = build_entries(&result, &proposal.selection, &date, &source_line)?;
    let folder = output_folder(store).await?;
    // Serialize the scan, ownership check, append and ownership-row save: two
    // confirms must not both append from the same stale bytes and then race the
    // row replacement.
    let _guard = NOTE_LOCK.lock().await;
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
    let target = Path::new(&note_path).to_owned();
    // Native filesystem I/O is not cancellable; a stopped writer cannot leave a
    // persisted intent behind because promotion keeps no delivery state. The
    // writer re-checks `expected` atomically under its own file handle.
    let write = tokio::task::spawn_blocking(move || {
        append_entries_owned(&target, &entries, Some(expected.as_str()))
    })
    .await
    .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match write {
        Ok(receipt) => {
            // The ownership row is the note's write guard, not an attribution log
            // or a garbage-collection key: it stores the source identity and the
            // SHA-256 of the bytes this app last wrote for this one note. Prepare
            // and confirm both require the same source and a matching live hash,
            // so an edit or another source's ownership is never silently adopted;
            // the row survives run pruning and is replaced on every promotion.
            store
                .save_report_file(ReportFileOwnership {
                    path: note_path.clone(),
                    source_id: run.source_id.clone(),
                    run_id: run.run_id.clone(),
                    hash: receipt.hash.clone(),
                    updated_at: now_ms(),
                })
                .await
                .map_err(storage)?;
            Ok(PromotionReceipt {
                path: note_path,
                hash: Some(receipt.hash),
                conflict: false,
                error: None,
                recovery_path: None,
            })
        }
        Err(error) if error.conflict => Ok(conflict_receipt(
            note_path,
            &error.message,
            error.recovery_path,
        )),
        Err(error) => Err(fail("WRITE_FAILED", &error.message)),
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
    create_note(store.as_ref(), &slug).await
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
    let dir = projects_dir(&folder);
    std::fs::create_dir_all(&dir)
        .map_err(|_| fail("WRITE_FAILED", "Could not create the Projects folder."))?;
    let path = dir.join(format!("{slug}.md"));
    let target = path.clone();
    let write = tokio::task::spawn_blocking(move || write_owned(&target, "", None))
        .await
        .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match write {
        Ok(_) => Ok(status_of(&project_notes(&folder)?)),
        Err(error) if error.conflict => Err(fail(
            "FILE_CONFLICT",
            "A project note already exists; the app never overwrites it.",
        )),
        Err(error) => Err(fail("WRITE_FAILED", &error.message)),
    }
}

/// Preview the exact blocks a confirmation would append. Writes nothing.
#[tauri::command]
pub async fn prepare_promotion(
    selection: PromotionSelection,
    state: State<'_, AppState>,
) -> Result<PromotionProposal, ReportError> {
    let store = db(&state)?;
    prepare(store.as_ref(), &selection, now_ms()).await
}

/// Append the previewed selection in one atomic multi-section write.
#[tauri::command]
pub async fn confirm_promotion(
    proposal: PromotionProposal,
    state: State<'_, AppState>,
) -> Result<PromotionReceipt, ReportError> {
    let store = db(&state)?;
    confirm(store.as_ref(), &proposal).await
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
}
