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
    append_entries_owned, content_hash, native_path, path_string, safe_read_owned, write_owned,
    WriteFailure,
};
use crate::{now_ms, AppState};

/// Persisted memory setting holding the selected output folder (`03 §8c`).
const OUTPUT_FOLDER: &str = "memory.schedule.output_folder";
const MAX_LISTED_IDS: usize = 20;

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

/// `^[a-z0-9][a-z0-9_-]{0,63}$` — a filename-safe, lower-case slug.
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

/// ATX heading text without the closing hashes Markdown permits (`## Activity ##`),
/// mirroring the UI's selectable-surface parser (`ui/src/lib/promotion.ts`).
fn heading_title(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("## ")
        .or_else(|| line.strip_prefix("##\t"))?;
    let trimmed = rest.trim_end();
    let without_hashes = trimmed.trim_end_matches('#');
    if without_hashes.len() == trimmed.len() {
        return Some(trimmed);
    }
    let head = without_hashes.trim_end();
    if head.len() == without_hashes.len() {
        // No whitespace before the hashes: they are part of the title, not a close.
        return Some(trimmed);
    }
    Some(head)
}

/// The UI treats a line indented up to three spaces as a fence delimiter and
/// skips fenced content, so the backend does too.
fn fence_line(line: &str) -> bool {
    let indented = line.trim_start_matches(' ');
    line.len() - indented.len() <= 3 && (indented.starts_with("```") || indented.starts_with("~~~"))
}

/// Top-level dash bullets (`- ` at column 0) under `## <heading>`, verbatim.
fn section_bullets(markdown: &str, heading: &str) -> Vec<String> {
    let target = heading.to_lowercase();
    let mut current = false;
    let mut in_fence = false;
    let mut items = Vec::new();
    for line in markdown.lines() {
        let line = line.trim_end();
        if fence_line(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
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
        let mut items: Vec<String> = Vec::new();
        for item in selected {
            if !available.iter().any(|bullet| bullet == &item.text) {
                return Err(fail(
                    "INVALID_CONFIG",
                    &format!(
                        "A selected item is no longer a bullet under \"{}\" in the saved report.",
                        spec.source
                    ),
                ));
            }
            if !items.contains(&item.text) {
                items.push(item.text.clone());
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
    let (run, result) = load_run(store, &proposal.selection.run_id).await?;
    let date = entry_date(&proposal.entry_date)?;
    let source_line = provenance(&run, &result);
    let blocks = build_entries(&result, &proposal.selection, &date, &source_line)?;
    let folder = output_folder(store).await?;
    let note_path = proposal.note_path.clone().ok_or_else(|| {
        fail(
            "INVALID_CONFIG",
            "The promotion proposal has no target project note.",
        )
    })?;
    let note = Path::new(&note_path).to_owned();
    let projects = projects_dir(&folder);
    let inside_projects = note
        .parent()
        .is_some_and(|parent| path_string(parent) == path_string(&projects));
    if !inside_projects || note.extension().and_then(|value| value.to_str()) != Some("md") {
        // The selected folder or the note itself changed since the preview: the
        // user re-prepares against the current folder instead of writing anywhere.
        return Ok(PromotionReceipt {
            path: note_path,
            hash: None,
            conflict: true,
            error: Some(fail(
                "FILE_CONFLICT",
                "The project note changed since the preview; prepare the promotion again.",
            )),
            recovery_path: None,
        });
    }
    let entries: Vec<(String, String)> = blocks
        .into_iter()
        .map(|block| (block.target.to_string(), block.block))
        .collect();
    let expected = proposal.note_hash.clone();
    let target = note.clone();
    // Native filesystem I/O is not cancellable; a stopped writer cannot leave a
    // persisted intent behind because promotion keeps no delivery state.
    let write = tokio::task::spawn_blocking(move || {
        append_entries_owned(&target, &entries, expected.as_deref())
    })
    .await
    .map_err(|_| fail("WRITE_FAILED", "The file writer stopped unexpectedly."))?;
    match write {
        Ok(receipt) => {
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
        Err(error) if error.conflict => Ok(PromotionReceipt {
            path: note_path,
            hash: None,
            conflict: true,
            error: Some(fail("FILE_CONFLICT", &error.message)),
            recovery_path: error.recovery_path,
        }),
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
    let slug = slug.trim().to_owned();
    if !valid_slug(&slug) {
        return Err(fail(
            "INVALID_CONFIG",
            "Use lowercase letters, digits, dashes or underscores, starting with a letter or digit (max 64 characters).",
        ));
    }
    let store = db(&state)?;
    let folder = output_folder(store.as_ref()).await?;
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
}
