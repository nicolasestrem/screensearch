//! Owned Markdown export. User edits are conflicts, never permission to overwrite.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use traits::{MemoryReportRange, ReportOutcome, ReportRun};

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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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
        serde_json::to_string_pretty(&metadata)?,
        report_label(run)
    );
    if status == ReportOutcome::Partial {
        text.push_str("> PARTIAL — coverage is incomplete; this is not an exhaustive account.\n\n");
    }
    text.push_str(&r.markdown);
    text.push_str("\n\nEvidence IDs identify captured sources at generation time; later retention may remove them.\n");
    Ok(text)
}

pub fn parse_frontmatter(text: &str) -> anyhow::Result<Frontmatter> {
    let body = text
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow::anyhow!("missing frontmatter"))?;
    let (json, _) = body
        .split_once("\n---\n")
        .ok_or_else(|| anyhow::anyhow!("incomplete frontmatter"))?;
    Ok(serde_json::from_str(json)?)
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
        };
        let text = format!(
            "---\n{}\n---\nbody",
            serde_json::to_string_pretty(&m).unwrap()
        );
        assert_eq!(parse_frontmatter(&text).unwrap(), m);
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
