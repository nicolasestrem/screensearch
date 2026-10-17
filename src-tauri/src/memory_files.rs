//! Owned Markdown export. User edits are conflicts, never permission to overwrite.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use traits::{MemoryReportRange, ReportOutcome, ReportRun, ReportRunKind};

#[derive(Debug)]
pub struct WriteReceipt {
    pub hash: String,
}
#[derive(Debug)]
pub struct WriteFailure {
    pub message: String,
    pub conflict: bool,
    pub recovery_path: Option<String>,
}
impl std::fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for WriteFailure {}
fn failure(message: impl Into<String>, conflict: bool) -> WriteFailure {
    WriteFailure {
        message: message.into(),
        conflict,
        recovery_path: None,
    }
}
pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Windows accepts forward slashes; keep all persisted and displayed paths consistent.
pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Restore Windows verbatim/device syntax only at the native I/O boundary.
/// Ordinary drive and UNC paths keep their forward-slash spelling.
pub fn native_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if value.starts_with("//?/") || value.starts_with("//./") {
        PathBuf::from(value.replace('/', "\\"))
    } else {
        path.to_owned()
    }
}

/// Construct only after create_new succeeds: cleanup must never remove a file we
/// failed to create. The guard also covers validation failures after writing.
struct TemporaryNote(PathBuf);
impl Drop for TemporaryNote {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Schema-1 notes omit the v2 `kind`; it reads back as `daily` (`03 §8e`).
fn kind_is_daily(kind: &ReportRunKind) -> bool {
    *kind == ReportRunKind::Daily
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    #[serde(default)]
    pub requested_range: Option<MemoryReportRange>,
    #[serde(default)]
    pub base_run_id: Option<String>,
    pub schema: u32,
    pub run_id: String,
    pub source_id: String,
    pub window_start: String,
    pub window_end: String,
    pub timezone: String,
    pub status: ReportOutcome,
    pub model: Option<String>,
    pub frames_sampled: u32,
    pub frames_summarized: u32,
    pub periods_total: u32,
    pub periods_covered: u32,
    pub truncated: bool,
    pub frame_ids: Vec<i64>,
    pub session_ids: Vec<i64>,
    /// v2 (`03 §8e`): absent on schema-1 notes and read as `daily`.
    #[serde(default, skip_serializing_if = "kind_is_daily")]
    pub kind: ReportRunKind,
    /// v2 (`03 §8e`): daily runs a weekly synthesis consumed; empty on schema-1 notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub daily_run_ids: Vec<String>,
    /// v2 (`03 §8e`): local calendar dates covered by the window; empty on schema-1 notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub window_local_dates: Vec<String>,
}

impl Frontmatter {
    /// A note carrying any v2 field is written as schema 2 (`03 §8e`); schema-1 notes
    /// (existing daily notes) stay schema 1.
    fn carries_v2_fields(&self) -> bool {
        self.kind != ReportRunKind::Daily
            || !self.daily_run_ids.is_empty()
            || !self.window_local_dates.is_empty()
    }

    /// Schema number stamped at serialization, derived from the carried fields so a
    /// note with v2 fields can never be written as schema 1.
    fn schema_number(&self) -> u32 {
        if self.carries_v2_fields() {
            2
        } else {
            1
        }
    }
}

pub fn report_label(run: &ReportRun) -> String {
    match &run.requested_range {
        Some(range) if range.start_date != range.end_date => {
            format!("{} to {}", range.start_date, range.end_date)
        }
        Some(range) => range.start_date.clone(),
        None => run.local_date.clone(),
    }
}

pub fn note_relative_path(
    local_date: &str,
    range: Option<&MemoryReportRange>,
    base_run_id: Option<&str>,
) -> anyhow::Result<PathBuf> {
    match range {
        Some(range) => {
            let start = kernel::memory_schedule::parse_date(&range.start_date)?;
            let end = kernel::memory_schedule::parse_date(&range.end_date)?;
            anyhow::ensure!(start <= end, "invalid requested date range");
            let base = base_run_id.ok_or_else(|| anyhow::anyhow!("missing base run identity"))?;
            anyhow::ensure!(
                uuid::Uuid::parse_str(base)?.to_string() == base,
                "invalid base run identity"
            );
            Ok(PathBuf::from("Reports").join(format!(
                "{}_to_{}_{}.md",
                range.start_date, range.end_date, base
            )))
        }
        None => {
            kernel::memory_schedule::parse_date(local_date)?;
            Ok(PathBuf::from("Daily").join(format!("{local_date}.md")))
        }
    }
}

pub fn daily_note(run: &ReportRun, base_run_id: Option<&str>) -> anyhow::Result<String> {
    note_relative_path(&run.local_date, run.requested_range.as_ref(), base_run_id)?;
    let r = run
        .result
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("report has no saved output"))?;
    let status = run
        .outcome
        .ok_or_else(|| anyhow::anyhow!("report has no outcome"))?;
    anyhow::ensure!(
        matches!(
            status,
            ReportOutcome::Ok | ReportOutcome::Partial | ReportOutcome::NoData
        ),
        "report is not exportable"
    );
    let mut ids = r.frame_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    anyhow::ensure!(
        ids.len() == r.frames_summarized as usize,
        "report evidence count mismatch"
    );
    let mut session_ids = r.session_ids.clone();
    session_ids.sort_unstable();
    session_ids.dedup();
    if status == ReportOutcome::NoData {
        anyhow::ensure!(
            ids.is_empty() && session_ids.is_empty() && r.model.is_none(),
            "no-data report contains model or evidence"
        );
    }
    let timestamp = |ms| {
        DateTime::<Utc>::from_timestamp_millis(ms)
            .map(|d| d.to_rfc3339())
            .ok_or_else(|| anyhow::anyhow!("invalid report timestamp"))
    };
    let metadata = Frontmatter {
        requested_range: run.requested_range.clone(),
        base_run_id: base_run_id.map(str::to_owned),
        // Stamped by `serialize_frontmatter` from the carried fields.
        schema: 1,
        run_id: run.run_id.clone(),
        source_id: run.source_id.clone(),
        window_start: timestamp(run.window_start)?,
        window_end: timestamp(run.window_end)?,
        timezone: run.timezone.clone(),
        status,
        model: r.model.clone(),
        frames_sampled: r.frames_sampled,
        frames_summarized: r.frames_summarized,
        periods_total: r.periods_total,
        periods_covered: r.periods_covered,
        truncated: r.truncated,
        frame_ids: ids,
        session_ids,
    };
    // JSON flow mappings are valid YAML 1.2. This preserves scalar escaping and array types.
    let mut text = format!(
        "---\n{}\n---\n\n# {}\n\n",
        serialize_frontmatter(&metadata)?,
        report_label(run)
    );
    if status == ReportOutcome::Partial {
        text.push_str("> PARTIAL — coverage is incomplete; this is not an exhaustive account.\n\n");
    }
    text.push_str(&r.markdown);
    text.push_str("\n\nEvidence IDs identify captured sources at generation time; later retention may remove them.\n");
    Ok(text)
}

/// Serialize the frontmatter JSON body written between a note's `---` fences.
/// The schema number is stamped from the carried fields (`03 §8e`): notes with v2
/// fields are written as schema 2, schema-1 notes stay schema 1.
pub fn serialize_frontmatter(metadata: &Frontmatter) -> anyhow::Result<String> {
    let mut stamped = metadata.clone();
    stamped.schema = metadata.schema_number();
    Ok(serde_json::to_string_pretty(&stamped)?)
}

pub fn parse_frontmatter(text: &str) -> anyhow::Result<Frontmatter> {
    let body = text
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow::anyhow!("missing frontmatter"))?;
    let (json, _) = body
        .split_once("\n---\n")
        .ok_or_else(|| anyhow::anyhow!("incomplete frontmatter"))?;
    let metadata: Frontmatter = serde_json::from_str(json)?;
    anyhow::ensure!(
        metadata.schema == 1 || metadata.schema == 2,
        "unsupported frontmatter schema {}",
        metadata.schema
    );
    Ok(metadata)
}

fn safe_parent(path: &Path, create: bool) -> Result<Option<Vec<fs::File>>, WriteFailure> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(failure(
            "output path must be absolute without parent traversal",
            false,
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| failure("output has no parent", false))?;
    let mut current = std::path::PathBuf::new();
    let mut guards = Vec::new();
    for component in parent.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) => {
                if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                    return Err(failure(
                        "output contains a redirected or non-directory component",
                        true,
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    return Ok(None);
                }
                fs::create_dir(&current)
                    .map_err(|_| failure("cannot create output directory", false))?;
            }
            Err(_) => return Err(failure("cannot inspect output directory", false)),
        }
        // Open the directory itself, not its reparse target, and deny deletion/
        // renaming until the write finishes. A scan alone leaves a parent-swap
        // race that could redirect the later temporary write or replacement.
        let guard = OpenOptions::new()
            // Attribute-only handles do not participate in Windows sharing
            // checks. LIST_DIRECTORY establishes read access, making the
            // omitted FILE_SHARE_DELETE actually block rename/delete opens.
            .access_mode(0x80 | 0x1) // FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY
            .share_mode(1 | 2) // read/write sharing, deliberately no FILE_SHARE_DELETE
            .custom_flags(0x02000000 | 0x00200000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
            .open(&current)
            .map_err(|_| failure("cannot protect output directory during writing", true))?;
        let meta = guard
            .metadata()
            .map_err(|_| failure("cannot inspect output directory handle", false))?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(failure(
                "output contains a redirected or non-directory component",
                true,
            ));
        }
        guards.push(guard);
    }
    Ok(Some(guards))
}

/// Read an existing generated path without creating directories or following a
/// reparse point. Callers still verify the returned bytes against their ownership hash.
pub fn safe_read_owned(path: &Path) -> Result<Option<Vec<u8>>, WriteFailure> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let native = native_path(path);
    let path = native.as_path();
    let Some(_parent_guards) = safe_parent(path, false)? else {
        return Ok(None);
    };
    let mut file = match OpenOptions::new()
        .read(true)
        .share_mode(1 | 4)
        .custom_flags(0x00200000)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(failure("cannot read generated file", false)),
    };
    let meta = file
        .metadata()
        .map_err(|_| failure("cannot inspect generated file", false))?;
    if !meta.is_file() || meta.file_attributes() & 0x400 != 0 {
        return Err(failure(
            "generated file is redirected or not a regular file",
            true,
        ));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes)
        .map_err(|_| failure("cannot read generated file", false))?;
    Ok(Some(bytes))
}

/// Atomic create/replace with displaced-content backup on Windows. Never discards a
/// mismatching backup: a concurrent path replacement remains recoverable and visible.
pub fn write_owned(
    path: &Path,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<WriteReceipt, WriteFailure> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACE_FILE_FLAGS,
    };
    let native = native_path(path);
    let path = native.as_path();
    let _parent_guards = safe_parent(path, true)?;
    let hash = content_hash(content.as_bytes());
    let mut existing = None;
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_file() || meta.file_attributes() & 0x400 != 0 {
                return Err(failure("output is redirected or not a regular file", true));
            }
            // Deny writes while inspecting/replacing; share-delete permits our atomic replacement.
            let mut file = OpenOptions::new()
                .read(true)
                .share_mode(1 | 4)
                .custom_flags(0x00200000) // OPEN_REPARSE_POINT: inspect the entry itself
                .open(path)
                .map_err(|_| failure("output is in use or unavailable", true))?;
            let opened = file
                .metadata()
                .map_err(|_| failure("cannot inspect opened note", false))?;
            if !opened.is_file() || opened.file_attributes() & 0x400 != 0 {
                return Err(failure(
                    "opened output is redirected or not a regular file",
                    true,
                ));
            }
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut bytes)
                .map_err(|_| failure("cannot inspect existing note", false))?;
            let old_hash = content_hash(&bytes);
            if expected_hash != Some(old_hash.as_str()) {
                return Err(failure("existing note was edited or is not owned", true));
            }
            if old_hash == hash {
                return Ok(WriteReceipt { hash });
            }
            existing = Some(file);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(failure("cannot inspect note", false)),
    }
    let parent = path.parent().expect("validated parent");
    let temp = parent.join(format!(".screensearch-{}.tmp", uuid::Uuid::new_v4()));
    let backup = parent.join(format!(
        ".screensearch-recovery-{}.md",
        uuid::Uuid::new_v4()
    ));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|_| failure("cannot create temporary note", false))?;
    let _temporary = TemporaryNote(temp.clone());
    let write_result = (|| {
        f.write_all(content.as_bytes())?;
        f.sync_all()
    })();
    drop(f);
    if write_result.is_err() {
        return Err(failure("cannot write temporary note", false));
    }
    let _verified_parent_guards = safe_parent(path, false)?;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let target_w = wide(path);
    let temp_w = wide(&temp);
    let backup_w = wide(&backup);
    // SAFETY: all strings are null-terminated UTF-16 and alive for each synchronous call.
    let moved = unsafe {
        if existing.is_some() {
            ReplaceFileW(
                PCWSTR(target_w.as_ptr()),
                PCWSTR(temp_w.as_ptr()),
                PCWSTR(backup_w.as_ptr()),
                REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
        } else {
            MoveFileExW(
                PCWSTR(temp_w.as_ptr()),
                PCWSTR(target_w.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    drop(existing);
    if moved.is_err() {
        return Err(WriteFailure{message:"atomic note replacement failed; inspect the destination and any retained recovery copy".into(),conflict:true,
        recovery_path:backup.exists().then(||path_string(&backup))});
    }
    // Verify the installed leaf before discarding the displaced copy: another
    // process can replace a leaf between name-based Win32 operations.
    let installed = safe_read_owned(path);
    if !matches!(installed, Ok(Some(ref bytes)) if content_hash(bytes) == hash) {
        return Err(WriteFailure {
            message: "note changed during installation; inspect the destination and retained recovery copy".into(),
            conflict: true,
            recovery_path: backup.exists().then(|| path_string(&backup)),
        });
    }
    if backup.exists() {
        let displaced = safe_read_owned(&backup)
            .ok()
            .flatten()
            .ok_or_else(|| WriteFailure {
                message: "cannot verify displaced note; recovery copy retained".into(),
                conflict: true,
                recovery_path: Some(path_string(&backup)),
            })?;
        if expected_hash != Some(content_hash(&displaced).as_str()) {
            return Err(WriteFailure {
                message: "note changed during replacement; original retained in recovery copy"
                    .into(),
                conflict: true,
                recovery_path: Some(path_string(&backup)),
            });
        }
        let _ = fs::remove_file(&backup);
    }
    Ok(WriteReceipt { hash })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forward_slash_paths_support_native_atomic_create_and_replace() {
        let dir = tempfile::tempdir().unwrap();
        let text = path_string(&dir.path().join("Daily/note.md"));
        assert!(!text.contains('\\'));
        let path = Path::new(&text);
        let original = write_owned(path, "original note", None).unwrap();
        write_owned(path, "updated note", Some(&original.hash)).unwrap();
        assert_eq!(safe_read_owned(path).unwrap().unwrap(), b"updated note");
    }

    #[test]
    fn displayed_verbatim_paths_preserve_native_atomic_operations() {
        let dir = tempfile::tempdir().unwrap();
        let original = fs::canonicalize(dir.path())
            .unwrap()
            .join("Daily")
            .join("note.md");
        let display = path_string(&original);
        assert!(display.starts_with("//?/"));
        assert!(!display.contains('\\'));
        assert_eq!(native_path(Path::new(&display)), original);
        let path = Path::new(&display);
        let receipt = write_owned(path, "original note", None).unwrap();
        write_owned(path, "updated note", Some(&receipt.hash)).unwrap();
        assert_eq!(safe_read_owned(path).unwrap().unwrap(), b"updated note");
    }

    #[test]
    fn safe_read_does_not_create_missing_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent/daily/note.md");
        assert!(safe_read_owned(&path).unwrap().is_none());
        assert!(!dir.path().join("absent").exists());
        assert!(safe_read_owned(Path::new("relative.md")).is_err());
        let present = dir.path().join("present.md");
        fs::write(&present, b"actual saved content").unwrap();
        assert_eq!(
            safe_read_owned(&present).unwrap().unwrap(),
            b"actual saved content"
        );
    }
    #[test]
    fn protected_parent_cannot_be_swapped_during_write() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("Daily");
        fs::create_dir(&parent).unwrap();
        let guards = safe_parent(&parent.join("note.md"), false)
            .unwrap()
            .unwrap();
        assert!(fs::rename(&parent, dir.path().join("displaced")).is_err());
        drop(guards);
        fs::rename(&parent, dir.path().join("displaced")).unwrap();
    }
    #[test]
    fn temporary_note_is_removed_on_early_failure() {
        let d = tempfile::tempdir().unwrap();
        let temporary = d.path().join("owned.tmp");
        let previous = d.path().join("note.md");
        fs::write(&previous, "previous valid note").unwrap();
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .unwrap();
            let _guard = TemporaryNote(temporary.clone());
            file.write_all(b"unfinished replacement").unwrap();
            drop(file);
            // A guard is retained across every fallible validation/replace step.
        }
        assert!(!temporary.exists());
        assert_eq!(fs::read_to_string(previous).unwrap(), "previous valid note");
    }
    #[test]
    fn edited_note_is_preserved_and_rerun_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("ScreenSearch/Daily/2026-10-15.md");
        let receipt = write_owned(&p, "first", None).unwrap();
        assert_eq!(
            write_owned(&p, "first", Some(&receipt.hash)).unwrap().hash,
            receipt.hash
        );
        let next = write_owned(&p, "second", Some(&receipt.hash)).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "second");
        fs::write(&p, "human edit").unwrap();
        assert!(
            write_owned(&p, "third", Some(&next.hash))
                .unwrap_err()
                .conflict
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "human edit");
    }
    #[test]
    fn unrelated_file_is_preserved() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("note.md");
        fs::write(&p, "human").unwrap();
        assert!(write_owned(&p, "generated", None).unwrap_err().conflict);
        assert_eq!(fs::read_to_string(p).unwrap(), "human");
    }
    #[test]
    fn frontmatter_roundtrips_with_unicode_and_quotes() {
        let m = Frontmatter {
            requested_range: None,
            base_run_id: None,
            schema: 1,
            run_id: "r".into(),
            source_id: "s".into(),
            window_start: "2026-01-01T00:00:00Z".into(),
            window_end: "2026-01-02T00:00:00Z".into(),
            timezone: "Europe/Paris".into(),
            status: ReportOutcome::Partial,
            model: Some("quoted \" modèle".into()),
            frames_sampled: 2,
            frames_summarized: 1,
            periods_total: 1,
            periods_covered: 1,
            truncated: true,
            frame_ids: vec![42],
            session_ids: vec![3],
            kind: ReportRunKind::Daily,
            daily_run_ids: vec![],
            window_local_dates: vec![],
        };
        let text = format!(
            "---\n{}\n---\nbody",
            serde_json::to_string_pretty(&m).unwrap()
        );
        assert_eq!(parse_frontmatter(&text).unwrap(), m);
    }

    /// Weekly-synthesis frontmatter fixture (`03 §8e`), Europe/Paris ISO week 2026-W42.
    fn weekly_frontmatter() -> Frontmatter {
        Frontmatter {
            requested_range: None,
            base_run_id: None,
            schema: 2,
            run_id: "9b1f3d4e-5a6c-4f70-8a9b-0c1d2e3f4a5b".into(),
            source_id: "6c7d8e9f-0a1b-4c2d-8e3f-4a5b6c7d8e9f".into(),
            window_start: "2026-10-11T22:00:00+00:00".into(),
            window_end: "2026-10-18T22:00:00+00:00".into(),
            timezone: "Europe/Paris".into(),
            status: ReportOutcome::Ok,
            model: Some("ministral-3-3b-reasoning".into()),
            frames_sampled: 40,
            frames_summarized: 4,
            periods_total: 7,
            periods_covered: 7,
            truncated: false,
            frame_ids: vec![101, 102, 103, 104],
            session_ids: vec![7, 8, 9],
            kind: ReportRunKind::WeeklySynthesis,
            daily_run_ids: vec![
                "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".into(),
                "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e".into(),
                "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f".into(),
                "3d4e5f6a-7b8c-4d9e-8f0a-2b3c4d5e6f7a".into(),
                "4e5f6a7b-8c9d-4e0f-9a1b-3c4d5e6f7a8b".into(),
                "5f6a7b8c-9d0e-4f1a-8b2c-4d5e6f7a8b9c".into(),
                "6a7b8c9d-0e1f-4a2b-9c3d-5e6f7a8b9c0d".into(),
            ],
            window_local_dates: vec![
                "2026-10-12".into(),
                "2026-10-13".into(),
                "2026-10-14".into(),
                "2026-10-15".into(),
                "2026-10-16".into(),
                "2026-10-17".into(),
                "2026-10-18".into(),
            ],
        }
    }

    #[test]
    fn frontmatter_v1_still_parses() {
        // Verbatim schema-1 frontmatter block as the current daily writer emits it (`03 §8c`).
        let text = r#"---
{
  "requested_range": null,
  "base_run_id": null,
  "schema": 1,
  "run_id": "3f2b6b1e-8a4c-4c3e-9b6a-1f2e3d4c5b6a",
  "source_id": "7c9d0e1f-2a3b-4c5d-8e9f-0a1b2c3d4e5f",
  "window_start": "2026-10-14T22:00:00+00:00",
  "window_end": "2026-10-15T22:00:00+00:00",
  "timezone": "Europe/Paris",
  "status": "OK",
  "model": "ministral-3-3b-reasoning",
  "frames_sampled": 12,
  "frames_summarized": 2,
  "periods_total": 24,
  "periods_covered": 24,
  "truncated": false,
  "frame_ids": [
    41,
    42
  ],
  "session_ids": [
    7
  ]
}
---

# 2026-10-15

body
"#;
        let metadata = parse_frontmatter(text).unwrap();
        assert_eq!(metadata.schema, 1);
        assert_eq!(metadata.kind, ReportRunKind::Daily);
        assert!(metadata.daily_run_ids.is_empty());
        assert!(metadata.window_local_dates.is_empty());
        assert_eq!(metadata.requested_range, None);
        assert_eq!(metadata.base_run_id, None);
        assert_eq!(metadata.run_id, "3f2b6b1e-8a4c-4c3e-9b6a-1f2e3d4c5b6a");
        assert_eq!(metadata.source_id, "7c9d0e1f-2a3b-4c5d-8e9f-0a1b2c3d4e5f");
        assert_eq!(metadata.window_start, "2026-10-14T22:00:00+00:00");
        assert_eq!(metadata.window_end, "2026-10-15T22:00:00+00:00");
        assert_eq!(metadata.timezone, "Europe/Paris");
        assert_eq!(metadata.status, ReportOutcome::Ok);
        assert_eq!(metadata.model.as_deref(), Some("ministral-3-3b-reasoning"));
        assert_eq!(metadata.frames_sampled, 12);
        assert_eq!(metadata.frames_summarized, 2);
        assert_eq!(metadata.periods_total, 24);
        assert_eq!(metadata.periods_covered, 24);
        assert!(!metadata.truncated);
        assert_eq!(metadata.frame_ids, vec![41, 42]);
        assert_eq!(metadata.session_ids, vec![7]);
    }

    #[test]
    fn frontmatter_v2_roundtrips_for_weekly_synthesis() {
        let m = weekly_frontmatter();
        let json = serialize_frontmatter(&m).unwrap();
        assert!(json.contains("\"schema\": 2"));
        assert!(json.contains("\"kind\": \"weekly_synthesis\""));
        let text = format!("---\n{json}\n---\nbody");
        assert_eq!(parse_frontmatter(&text).unwrap(), m);
        // The schema number is stamped from the carried fields: a note carrying v2
        // fields cannot be written as schema 1.
        let mut stale = m.clone();
        stale.schema = 1;
        assert!(serialize_frontmatter(&stale)
            .unwrap()
            .contains("\"schema\": 2"));
    }

    #[test]
    fn unknown_frontmatter_schema_is_rejected() {
        let json = serialize_frontmatter(&weekly_frontmatter()).unwrap();
        for unknown in ["0", "3", "99"] {
            let text = format!(
                "---\n{}\n---\nbody",
                json.replace("\"schema\": 2", &format!("\"schema\": {unknown}"))
            );
            assert!(parse_frontmatter(&text).is_err());
        }
    }

    #[test]
    fn range_filenames_validate_dates_and_base_identity() {
        let range = MemoryReportRange {
            start_date: "2026-10-01".into(),
            end_date: "2026-10-15".into(),
        };
        let base = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            path_string(&note_relative_path("2026-10-01", Some(&range), Some(&base)).unwrap()),
            format!("Reports/2026-10-01_to_2026-10-15_{base}.md")
        );
        assert!(note_relative_path("2026-10-01", Some(&range), Some("../bad")).is_err());
        assert!(note_relative_path("2026-10-01", Some(&range), None).is_err());
        assert!(note_relative_path("../bad", None, None).is_err());
    }
}
