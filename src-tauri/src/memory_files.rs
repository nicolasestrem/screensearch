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

/// Daily and requested-range report paths (`03 §8e`). The weekly branch the spec
/// attributes to `note_relative_path` lives in [`weekly_note_relative_path`]: a
/// `Weekly/YYYY-Www.md` path is built there, never here.
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
    anyhow::ensure!(
        run.kind == ReportRunKind::Daily,
        "daily note requires a daily run"
    );
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
        kind: run.kind,
        // Weekly provenance fields are populated by the G3 weekly writer (`03 §8e`).
        daily_run_ids: Vec::new(),
        window_local_dates: Vec::new(),
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

/// `Weekly/2026-W42.md` from the run's ISO week (`03 §8e`). Weekly notes live in
/// their own filename space: a daily run never writes here and a weekly run never
/// writes a `Daily/` note.
pub fn weekly_note_relative_path(run: &ReportRun) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        run.kind == ReportRunKind::WeeklySynthesis,
        "weekly note requires a weekly synthesis run"
    );
    let label = kernel::week_label(&run.timezone, run.window_start)?;
    Ok(PathBuf::from("Weekly").join(format!("{label}.md")))
}

/// Render the weekly note from the persisted run only (`03 §8e`). The body is the
/// saved synthesis verbatim: `save_weekly_execution` already prepended the weekly
/// PARTIAL banner, so a second banner here would double-state the week's gaps.
pub fn weekly_note(run: &ReportRun) -> anyhow::Result<String> {
    weekly_note_relative_path(run)?;
    let r = run
        .result
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("report has no saved output"))?;
    let status = run
        .outcome
        .ok_or_else(|| anyhow::anyhow!("report has no outcome"))?;
    // G3 writes no note for a NO_DATA week (`03 §8e`); only saved evidence exports.
    anyhow::ensure!(
        matches!(status, ReportOutcome::Ok | ReportOutcome::Partial),
        "weekly report is not exportable"
    );
    let mut ids = r.frame_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    // `generate_weekly_synthesis` records `frames_summarized` as the distinct frame
    // count of `frame_ids` on both engine paths, so the daily check holds here too.
    anyhow::ensure!(
        ids.len() == r.frames_summarized as usize,
        "report evidence count mismatch"
    );
    let mut session_ids = r.session_ids.clone();
    session_ids.sort_unstable();
    session_ids.dedup();
    let mut daily_run_ids: Vec<String> = Vec::new();
    for id in &r.daily_run_ids {
        if !daily_run_ids.contains(id) {
            daily_run_ids.push(id.clone());
        }
    }
    let timestamp = |ms| {
        DateTime::<Utc>::from_timestamp_millis(ms)
            .map(|d| d.to_rfc3339())
            .ok_or_else(|| anyhow::anyhow!("invalid report timestamp"))
    };
    let metadata = Frontmatter {
        requested_range: run.requested_range.clone(),
        base_run_id: None,
        schema: 2,
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
        kind: run.kind,
        daily_run_ids,
        window_local_dates: kernel::window_local_dates(
            &run.timezone,
            run.window_start,
            run.window_end,
        )?,
    };
    let mut text = format!(
        "---\n{}\n---\n\n# {}\n\n",
        serialize_frontmatter(&metadata)?,
        kernel::week_label(&run.timezone, run.window_start)?
    );
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

/// ATX heading text without the closing hashes Markdown permits (`## Activity ##`),
/// shared by the source parser (`promotion.rs`) and the destination splice so both
/// read the same headings; the UI parser (`ui/src/lib/promotion.ts`) mirrors it.
/// Callers compare the returned title case-insensitively.
pub(crate) fn heading_title(line: &str) -> Option<&str> {
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

/// A Markdown fenced-code region. A fence opens on a run of at least three
/// backticks or tildes indented at most three spaces; it closes only on the same
/// character, a run at least as long, with nothing but whitespace after it. A
/// delimiter-looking line inside the region is content, never a close, so a
/// `## ` heading inside a fence is never a section boundary (`03 §8e`).
///
/// The UI parser mirrors these rules; keep the two in step.
#[derive(Default)]
pub(crate) struct MarkdownFence {
    open: Option<(char, usize)>,
}

impl MarkdownFence {
    /// True for a delimiter line and for every line inside the fenced region,
    /// false for ordinary lines outside it. The opener is a delimiter too.
    pub(crate) fn contains_line(&mut self, line: &str) -> bool {
        let Some((character, length, rest)) = fence_delimiter(line) else {
            return self.open.is_some();
        };
        match self.open {
            None => {
                // A backtick fence's info string cannot contain a backtick
                // (CommonMark), so such a line is an inline code span, not an opener.
                if character == '`' && rest.contains('`') {
                    return false;
                }
                self.open = Some((character, length));
                true
            }
            Some((opened, opened_length)) => {
                if character == opened && length >= opened_length && rest.trim().is_empty() {
                    self.open = None;
                }
                true
            }
        }
    }

    fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

/// `(fence character, run length, text after the run)` for a line that could
/// open or close a fenced block; `None` for every other line.
fn fence_delimiter(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let character = rest.chars().next()?;
    if character != '`' && character != '~' {
        return None;
    }
    let length = rest.chars().take_while(|c| *c == character).count();
    if length < 3 {
        return None;
    }
    Some((character, length, &rest[length..]))
}

fn is_blank_line(line: &str) -> bool {
    line.trim().is_empty()
}

/// Append each entry block to the end of its `## <section>` section, before the
/// next `## ` heading or EOF; missing sections are created at EOF in the given
/// order. Existing lines are copied byte-for-byte — the splice only inserts.
///
/// Section headings are matched like the source parser: closing hashes and case
/// are ignored, and a `## ` line inside a fenced block is content, never a
/// boundary. When the user repeated a heading, the first matching section wins;
/// the duplicate and its contents are left exactly as written.
///
/// An unclosed fence at EOF turns every following line into code. Appending at
/// EOF there would write the entry inside the user's code block, so the last
/// safe boundary is the fence opener: blocks and any missing sections are
/// inserted before it and the user's bytes stay untouched.
pub fn merged_content(existing: &str, entries: &[(String, String)]) -> String {
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = existing.split_inclusive('\n').map(str::to_owned).collect();
    // (lower-case heading, line index that ends the section) — the boundary is the
    // next heading line outside a fence, or EOF.
    let mut sections: Vec<(String, usize)> = Vec::new();
    let mut fence = MarkdownFence::default();
    let mut open_fence_start = None;
    for (index, line) in lines.iter().enumerate() {
        // A UTF-8 BOM may precede the first line. It is part of the file and is
        // preserved byte-for-byte on output, but heading and fence detection read
        // through a single leading BOM so a `## Decisions` behind it is a real
        // section boundary, not an invisible line that forces a duplicate heading
        // at EOF (`03 §8e`).
        let detected = if index == 0 {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line.as_str()
        };
        let was_open = fence.is_open();
        if fence.contains_line(detected) {
            if !was_open && fence.is_open() {
                open_fence_start = Some(index);
            } else if was_open && !fence.is_open() {
                open_fence_start = None;
            }
            continue;
        }
        if let Some(heading) = heading_title(detected) {
            if let Some(previous) = sections.last_mut() {
                previous.1 = index;
            }
            sections.push((heading.trim().to_lowercase(), lines.len()));
        }
    }
    let safe_end = open_fence_start.unwrap_or(lines.len());
    let mut insertions: Vec<(usize, Vec<String>)> = Vec::new();
    let mut missing: Vec<(String, Vec<&str>)> = Vec::new();
    for (heading, block) in entries {
        let block = block.trim_end_matches('\n');
        if block.is_empty() {
            continue;
        }
        let key = heading.trim().to_lowercase();
        if let Some((_, boundary)) = sections.iter().find(|(name, _)| name.as_str() == key) {
            let boundary = (*boundary).min(safe_end);
            match insertions.iter_mut().find(|(at, _)| *at == boundary) {
                Some((_, blocks)) => blocks.push(block.to_string()),
                None => insertions.push((boundary, vec![block.to_string()])),
            }
        } else {
            match missing.iter_mut().find(|(name, _)| name == heading) {
                Some((_, blocks)) => blocks.push(block),
                None => missing.push((heading.clone(), vec![block])),
            }
        }
    }
    if safe_end < lines.len() {
        // Missing sections cannot go at EOF either — that is inside the unclosed
        // fence. Render them as normal section blocks and put them at the opener,
        // after any entry blocks already destined for the same boundary.
        let blocks: Vec<String> = missing
            .iter()
            .map(|(heading, section_blocks)| {
                let mut section = format!("## {heading}");
                for block in section_blocks {
                    section.push_str("\n\n");
                    section.push_str(block);
                }
                section
            })
            .collect();
        if !blocks.is_empty() {
            match insertions.iter_mut().find(|(at, _)| *at == safe_end) {
                Some((_, existing_blocks)) => existing_blocks.extend(blocks),
                None => insertions.push((safe_end, blocks)),
            }
        }
        missing.clear();
    }
    // Descending so inserting at a later boundary never shifts an earlier one.
    insertions.sort_by_key(|(boundary, _)| std::cmp::Reverse(*boundary));
    for (boundary, blocks) in insertions {
        insert_blocks(&mut lines, boundary, &blocks, newline);
    }
    append_missing_sections(&mut lines, &missing, newline);
    lines.concat()
}

fn insert_blocks(lines: &mut Vec<String>, boundary: usize, blocks: &[String], newline: &str) {
    let at_eof = boundary >= lines.len();
    let mut chunk: Vec<String> = Vec::new();
    match boundary.checked_sub(1).and_then(|index| lines.get(index)) {
        Some(line) if is_blank_line(line) => {}
        Some(line) if line.ends_with('\n') => chunk.push(newline.to_string()),
        // The final line has no terminator: terminate it and open the blank line in
        // one insertion so the block still starts on its own line.
        Some(_) => chunk.push(format!("{newline}{newline}")),
        None => {}
    }
    for (index, block) in blocks.iter().enumerate() {
        for line in block.lines() {
            chunk.push(format!("{line}{newline}"));
        }
        if index + 1 < blocks.len() || !at_eof {
            chunk.push(newline.to_string());
        }
    }
    lines.splice(boundary..boundary, chunk);
}

fn append_missing_sections(
    lines: &mut Vec<String>,
    missing: &[(String, Vec<&str>)],
    newline: &str,
) {
    for (heading, blocks) in missing {
        if let Some(last) = lines.last() {
            if !last.ends_with('\n') {
                lines.push(newline.to_string());
                lines.push(newline.to_string());
            } else if !is_blank_line(last) {
                lines.push(newline.to_string());
            }
        }
        lines.push(format!("## {heading}{newline}"));
        lines.push(newline.to_string());
        for (index, block) in blocks.iter().enumerate() {
            for line in block.lines() {
                lines.push(format!("{line}{newline}"));
            }
            if index + 1 < blocks.len() {
                lines.push(newline.to_string());
            }
        }
    }
}

/// One atomic append for every touched section (`03 §8e`). The selection-time hash
/// is the adoption rule: a note that appeared, disappeared or changed since the
/// user prepared the promotion is a conflict and is never overwritten.
pub fn append_entries_owned(
    path: &Path,
    entries: &[(String, String)],
    selection_hash: Option<&str>,
) -> Result<WriteReceipt, WriteFailure> {
    let current = safe_read_owned(path)?;
    let current_hash = current.as_deref().map(content_hash);
    match (current_hash.as_deref(), selection_hash) {
        (None, Some(_)) => {
            return Err(failure(
                "note was removed since the promotion was prepared",
                true,
            ))
        }
        (Some(found), Some(expected)) if found != expected => {
            return Err(failure(
                "note changed since the promotion was prepared",
                true,
            ))
        }
        (Some(_), None) => {
            return Err(failure(
                "note appeared since the promotion was prepared",
                true,
            ))
        }
        // The project note is user-created (`03 §8e`): the app never invents it, so
        // an absent file with no selection is a conflict, never a create.
        (None, None) => {
            return Err(failure(
                "the project note is missing; create it before promoting",
                true,
            ))
        }
        _ => {}
    }
    let existing = match current {
        Some(bytes) => {
            String::from_utf8(bytes).map_err(|_| failure("note is not valid UTF-8", true))?
        }
        None => String::new(),
    };
    let merged = merged_content(&existing, entries);
    write_owned(path, &merged, current_hash.as_deref())
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

    /// Schema-1 daily frontmatter: the pre-F3 writer's exact field set and order
    /// (`03 §8e`).
    fn daily_frontmatter() -> Frontmatter {
        Frontmatter {
            requested_range: None,
            base_run_id: None,
            schema: 1,
            run_id: "3f2b6b1e-8a4c-4c3e-9b6a-1f2e3d4c5b6a".into(),
            source_id: "7c9d0e1f-2a3b-4c5d-8e9f-0a1b2c3d4e5f".into(),
            window_start: "2026-10-14T22:00:00+00:00".into(),
            window_end: "2026-10-15T22:00:00+00:00".into(),
            timezone: "Europe/Paris".into(),
            status: ReportOutcome::Ok,
            model: Some("ministral-3-3b-reasoning".into()),
            frames_sampled: 12,
            frames_summarized: 2,
            periods_total: 24,
            periods_covered: 24,
            truncated: false,
            frame_ids: vec![41, 42],
            session_ids: vec![7],
            kind: ReportRunKind::Daily,
            daily_run_ids: vec![],
            window_local_dates: vec![],
        }
    }

    #[test]
    fn schema1_daily_frontmatter_is_locked_byte_for_byte() {
        // The pre-F3 daily writer emitted exactly this block (`03 §8e`, field order
        // frozen): the v2 keys are absent and every other byte is unchanged.
        let expected = r#"{
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
}"#;
        let json = serialize_frontmatter(&daily_frontmatter()).unwrap();
        assert_eq!(json, expected);
        assert!(json.contains("\"schema\": 1"));
        for key in ["kind", "daily_run_ids", "window_local_dates"] {
            assert!(!json.contains(key), "schema-1 note leaked {key}: {json}");
        }
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

    fn stamp(value: &str) -> i64 {
        DateTime::parse_from_rfc3339(value)
            .expect("fixture timestamp")
            .timestamp_millis()
    }

    /// A saved weekly synthesis over ISO week 2026-W42 (Europe/Paris), with duplicate
    /// frame and daily-run IDs so provenance dedup is exercised.
    fn weekly_note_run(outcome: ReportOutcome, markdown: &str) -> ReportRun {
        ReportRun {
            requested_range: None,
            run_id: "9b1f3d4e-5a6c-4f70-8a9b-0c1d2e3f4a5b".into(),
            source_id: "6c7d8e9f-0a1b-4c2d-8e3f-4a5b6c7d8e9f".into(),
            kind: ReportRunKind::WeeklySynthesis,
            schedule_id: Some("weekly-schedule-fixture".into()),
            revision_of: None,
            window_start: stamp("2026-10-11T22:00:00Z"),
            window_end: stamp("2026-10-18T22:00:00Z"),
            timezone: "Europe/Paris".into(),
            local_date: "2026-10-18".into(),
            phase: traits::ReportRunPhase::Finished,
            outcome: Some(outcome),
            error: None,
            result: Some(traits::SavedReport {
                markdown: markdown.into(),
                model: Some("ministral-3-3b-reasoning".into()),
                frame_ids: vec![103, 101, 102, 101],
                session_ids: vec![9, 7, 8],
                cited_frame_ids: vec![101, 102],
                frames_sampled: 40,
                // `generate_weekly_synthesis` records the distinct frame count here.
                frames_summarized: 3,
                periods_total: 7,
                periods_covered: 7,
                passes: 2,
                truncated: false,
                daily_run_ids: vec![
                    "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".into(),
                    "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".into(),
                    "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e".into(),
                ],
            }),
            config_snapshot: "{}".into(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn weekly_note_contains_provenance() {
        let markdown = "**Partial coverage:** the week is missing complete saved daily coverage for 2026-10-15.\n\n## Activity\n\n- shipped the weekly writer [frame 101]";
        let run = weekly_note_run(ReportOutcome::Partial, markdown);
        assert_eq!(
            path_string(&weekly_note_relative_path(&run).unwrap()),
            "Weekly/2026-W42.md"
        );
        let text = weekly_note(&run).unwrap();
        let metadata = parse_frontmatter(&text).unwrap();
        assert_eq!(metadata.schema, 2);
        assert_eq!(metadata.kind, ReportRunKind::WeeklySynthesis);
        assert_eq!(metadata.status, ReportOutcome::Partial);
        assert_eq!(metadata.window_start, "2026-10-11T22:00:00+00:00");
        assert_eq!(metadata.window_end, "2026-10-18T22:00:00+00:00");
        assert_eq!(metadata.timezone, "Europe/Paris");
        assert!(metadata.requested_range.is_none());
        assert!(metadata.base_run_id.is_none());
        assert_eq!(
            metadata.window_local_dates,
            vec![
                "2026-10-12",
                "2026-10-13",
                "2026-10-14",
                "2026-10-15",
                "2026-10-16",
                "2026-10-17",
                "2026-10-18",
            ]
        );
        // Persisted order, duplicate dropped.
        assert_eq!(
            metadata.daily_run_ids,
            vec![
                "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
                "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e",
            ]
        );
        assert_eq!(metadata.frame_ids, vec![101, 102, 103]);
        assert_eq!(metadata.session_ids, vec![7, 8, 9]);
        assert_eq!(metadata.frames_summarized, 3);
        // Body: `# <week label>` then the saved synthesis verbatim, one banner only,
        // and the same evidence footnote the daily note writes.
        assert!(text.contains(&format!("# 2026-W42\n\n{markdown}")));
        assert_eq!(text.matches("**Partial coverage:**").count(), 1);
        assert!(!text.contains("> PARTIAL"));
        assert_eq!(
            text.matches("Evidence IDs identify captured sources at generation time")
                .count(),
            1
        );
        assert!(text.ends_with(
            "\n\nEvidence IDs identify captured sources at generation time; later retention may remove them.\n"
        ));
        // Kind separation: a daily run never renders a weekly note, and a NO_DATA
        // week writes no note at all (`03 §8e`).
        let mut daily = run.clone();
        daily.kind = ReportRunKind::Daily;
        assert!(weekly_note_relative_path(&daily).is_err());
        assert!(weekly_note(&daily).is_err());
        let mut no_data = run.clone();
        no_data.outcome = Some(ReportOutcome::NoData);
        assert!(weekly_note(&no_data).is_err());
    }

    #[test]
    fn daily_note_rejects_a_weekly_synthesis_run() {
        // Kind guard mirrors `weekly_note`: a daily write never renders a weekly run
        // into `Daily/<date>.md` (`03 §8e`).
        let run = weekly_note_run(
            ReportOutcome::Ok,
            "## Activity\n\n- weekly body [frame 101]",
        );
        let error = daily_note(&run, None).unwrap_err();
        assert!(
            error.to_string().contains("daily"),
            "unexpected error: {error}"
        );
    }

    fn assert_lines_preserved(original: &str, merged: &str) {
        let mut from = 0;
        for line in original.split_inclusive('\n') {
            let offset = merged[from..]
                .find(line)
                .unwrap_or_else(|| panic!("existing line was rewritten: {line:?}"));
            from += offset + line.len();
        }
    }

    #[test]
    fn merged_content_splices_into_user_sections_safely() {
        let existing = "# Project\n\n## decisions ##\n\n- user decision, keep\n\n```text\n## Log\n\n- fenced fake, never a section\n```\n\n- trailing user line\n";
        let entries = vec![
            (
                "Decisions".to_string(),
                "### 2026-10-18\n\n- promote decision".to_string(),
            ),
            (
                "Log".to_string(),
                "### 2026-10-18\n\n- promote log".to_string(),
            ),
        ];
        let merged = merged_content(existing, &entries);
        // Case-insensitive match with closing hashes: the entry extends the user's
        // own section instead of appending a second `## Decisions` at EOF.
        assert_eq!(merged.matches("## decisions ##").count(), 1);
        assert!(!merged.contains("## Decisions"));
        let decisions_headings = merged
            .split_inclusive('\n')
            .filter(|line| {
                heading_title(line)
                    .is_some_and(|title| title.trim().eq_ignore_ascii_case("decisions"))
            })
            .count();
        assert_eq!(decisions_headings, 1);
        // The fenced `## Log` is content, not a boundary: the decision block lands
        // after the fence closes, and only the missing Log section is created at EOF.
        let fence_close = merged
            .find("```\n\n- trailing user line")
            .expect("the fence and the user's trailing line survive");
        let decision_block = merged.find("- promote decision").unwrap();
        assert!(
            decision_block > fence_close,
            "the entry must not be spliced into the fence: {merged}"
        );
        let log_heading = merged.rfind("## Log").unwrap();
        let log_block = merged.find("- promote log").unwrap();
        assert!(
            log_heading < log_block,
            "the Log entry needs its own section"
        );
        assert!(merged[..log_heading].contains("- trailing user line"));
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn markdown_fence_tracks_character_length_and_whitespace_close() {
        let mut fence = MarkdownFence::default();
        assert!(fence.contains_line("```rust"));
        assert!(fence.is_open());
        assert!(fence.contains_line("## fake heading"));
        assert!(fence.contains_line("``` not a close"));
        assert!(fence.contains_line("``"));
        assert!(fence.contains_line("~~~"));
        assert!(fence.contains_line("````"));
        assert!(!fence.is_open());
        assert!(!fence.contains_line("## real heading"));

        let mut tilde = MarkdownFence::default();
        assert!(tilde.contains_line("~~~"));
        assert!(tilde.contains_line("```"));
        assert!(tilde.contains_line("## Log"));
        assert!(tilde.contains_line("~~~ not a close"));
        assert!(tilde.contains_line("~~"));
        assert!(tilde.is_open());
        assert!(tilde.contains_line("~~~~"));
        assert!(!tilde.contains_line("## Log"));
    }

    #[test]
    fn markdown_fence_requires_a_long_enough_whitespace_close() {
        let mut fence = MarkdownFence::default();
        assert!(fence.contains_line("````markdown"));
        assert!(fence.contains_line("```"));
        assert!(fence.contains_line("## Next steps"));
        assert!(fence.is_open());
        assert!(fence.contains_line("````"));
        assert!(!fence.is_open());
    }

    #[test]
    fn merged_content_ignores_fake_headings_in_mixed_fences() {
        let existing = "# Project\n\n## Log\n\n- real log\n\n~~~markdown\n## Decisions\n\n- fenced decision\n\n```\n## Next steps\n\n- fenced next\n\n~~\n\n~~~ not a close\n\n- still fenced\n~~~\n\n## Next steps\n\n- real next\n";
        let entries = vec![
            (
                "Log".to_string(),
                "### 2026-10-18\n\n- promote log".to_string(),
            ),
            (
                "Decisions".to_string(),
                "### 2026-10-18\n\n- promote decision".to_string(),
            ),
            (
                "Next steps".to_string(),
                "### 2026-10-18\n\n- promote next".to_string(),
            ),
        ];
        let merged = merged_content(existing, &entries);
        let fence_close = "- still fenced\n~~~\n";
        let after_fence = merged.find(fence_close).expect("tilde fence closes") + fence_close.len();
        let log_block = merged.find("- promote log").unwrap();
        assert!(
            log_block > after_fence,
            "Log must close only at the real heading after the fence: {merged}"
        );
        let real_next = merged.rfind("## Next steps").unwrap();
        assert!(log_block < real_next);
        assert!(merged.find("- promote next").unwrap() > real_next);
        // The fenced `## Decisions` is content: the real section is created at EOF
        // instead of splicing into the fence.
        assert!(merged.find("- promote decision").unwrap() > real_next);
        assert_eq!(merged.matches("## Decisions").count(), 2);
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_handles_four_backtick_fences() {
        let existing = "## Decisions\n\n- real decision\n\n````markdown\n```\n## Log\n\n- fenced fake\n```\n````\n\n## Log\n\n- real log\n";
        let entries = vec![
            (
                "Decisions".to_string(),
                "### 2026-10-18\n\n- promote decision".to_string(),
            ),
            (
                "Log".to_string(),
                "### 2026-10-18\n\n- promote log".to_string(),
            ),
        ];
        let merged = merged_content(existing, &entries);
        let fence_close = merged.find("```\n````\n").expect("four-backtick close");
        let decision_block = merged.find("- promote decision").unwrap();
        assert!(
            decision_block > fence_close,
            "a three-backtick line must not close a four-backtick fence: {merged}"
        );
        let real_log = merged.rfind("## Log").unwrap();
        assert!(decision_block < real_log);
        assert!(merged.find("- promote log").unwrap() > real_log);
        assert!(merged[..real_log].contains("## Log\n\n- fenced fake"));
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_uses_the_first_duplicate_section() {
        let existing = "## Decisions\n\n- first decision\n\n## decisions ###\n\n- duplicate decision\n\n## Log\n\n- log\n";
        let entries = vec![(
            "Decisions".to_string(),
            "### 2026-10-18\n\n- promote decision".to_string(),
        )];
        let merged = merged_content(existing, &entries);
        let first = merged.find("## Decisions").unwrap();
        let duplicate = merged.find("## decisions ###").unwrap();
        let block = merged.find("- promote decision").unwrap();
        assert!(
            first < block && block < duplicate,
            "the entry belongs to the first matching section: {merged}"
        );
        assert!(merged[duplicate..].contains("- duplicate decision"));
        assert_eq!(merged.matches("## decisions ###").count(), 1);
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_does_not_match_renamed_headings() {
        let existing = "## Decisions archive\n\n- user archive\n";
        let entries = vec![(
            "Decisions".to_string(),
            "### 2026-10-18\n\n- promote decision".to_string(),
        )];
        let merged = merged_content(existing, &entries);
        assert!(
            merged.find("- promote decision").unwrap()
                > merged.find("## Decisions archive").unwrap()
        );
        assert!(merged.contains("\n## Decisions\n"));
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_preserves_crlf_and_a_missing_final_newline() {
        let existing = "## Decisions\r\n\r\n- user decision\r\n\r\n## Log\r\n\r\n- user log";
        let entries = vec![(
            "Log".to_string(),
            "### 2026-10-18\n\n- promote log".to_string(),
        )];
        let merged = merged_content(existing, &entries);
        assert_eq!(merged.matches('\n').count(), merged.matches("\r\n").count());
        assert!(merged.contains("- user log\r\n\r\n### 2026-10-18\r\n\r\n- promote log\r\n"));
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_does_not_insert_inside_an_unclosed_fence() {
        let existing = "## Decisions\n\n- user decision\n\n```text\n## Log\n\n- fenced fake\n";
        let entries = vec![
            (
                "Decisions".to_string(),
                "### 2026-10-18\n\n- promote decision".to_string(),
            ),
            (
                "Log".to_string(),
                "### 2026-10-18\n\n- promote log".to_string(),
            ),
        ];
        let merged = merged_content(existing, &entries);
        let fence = merged.find("```text").unwrap();
        assert!(
            merged.find("- promote decision").unwrap() < fence,
            "the entry must not be written inside the open fence: {merged}"
        );
        assert!(merged.find("## Log").unwrap() < fence);
        assert!(merged.find("- promote log").unwrap() < fence);
        assert!(merged.ends_with("```text\n## Log\n\n- fenced fake\n"));
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn merged_content_reads_through_a_leading_utf8_bom() {
        // A note saved by another editor may open with a UTF-8 BOM. It is preserved
        // byte-for-byte, yet detection reads through it so the file's own
        // `## Decisions` is extended in place instead of a duplicate at EOF (`03 §8e`).
        let existing =
            "\u{feff}# Project\n\n## Decisions\n\n- user decision\n\n## Log\n\n- user log\n";
        let entries = vec![(
            "Decisions".to_string(),
            "### 2026-10-18\n\n- promote decision".to_string(),
        )];
        let merged = merged_content(existing, &entries);
        assert_eq!(
            merged.matches("## Decisions").count(),
            1,
            "a BOM must not hide the existing heading: {merged}"
        );
        let heading = merged.find("## Decisions").unwrap();
        let block = merged.find("- promote decision").unwrap();
        let log = merged.find("## Log").unwrap();
        assert!(
            heading < block && block < log,
            "the entry belongs inside the existing section: {merged}"
        );
        assert!(
            merged.starts_with('\u{feff}'),
            "BOM must survive: {merged:?}"
        );
        assert_eq!(&merged.as_bytes()[..3], &[0xEF, 0xBB, 0xBF]);
        assert_lines_preserved(existing, &merged);
    }

    #[test]
    fn promotion_appends_without_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ScreenSearch/Projects/project.md");
        let existing = "# Project\n\n## Decisions\n\n- already decided\n\n## Log\n\n### 2026-10-01\n\n- already logged\n";
        let original = write_owned(&path, existing, None).unwrap();
        let source = "*Source: weekly synthesis run weekly-fixture; daily runs daily-fixture; frames 101, 102; sessions 7, 8.*";
        let entries = vec![
            (
                "Decisions".to_string(),
                format!("### 2026-10-18\n\n- promote decision\n\n{source}"),
            ),
            (
                "Next steps".to_string(),
                format!("### 2026-10-18\n\n- promote next\n\n{source}"),
            ),
            (
                "Log".to_string(),
                format!("### 2026-10-18\n\n- promote log\n\n{source}"),
            ),
        ];
        let receipt = append_entries_owned(&path, &entries, Some(&original.hash)).unwrap();
        let merged = fs::read_to_string(&path).unwrap();
        let expected = format!(
            "# Project\n\n## Decisions\n\n- already decided\n\n### 2026-10-18\n\n- promote decision\n\n{source}\n\n## Log\n\n### 2026-10-01\n\n- already logged\n\n### 2026-10-18\n\n- promote log\n\n{source}\n\n## Next steps\n\n### 2026-10-18\n\n- promote next\n\n{source}\n"
        );
        assert_eq!(merged, expected);
        assert_lines_preserved(existing, &merged);
        // Multi-section promotion is one atomic write: no temporary/recovery files.
        assert!(fs::read_dir(path.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".screensearch-")));
        // Re-running with the hash the last write returned appends again; it never
        // rewrites or dedupes existing lines.
        let again = append_entries_owned(
            &path,
            &[(
                "Decisions".to_string(),
                format!("### 2026-10-19\n\n- promote follow-up\n\n{source}"),
            )],
            Some(&receipt.hash),
        )
        .unwrap();
        let appended = fs::read_to_string(&path).unwrap();
        assert_ne!(again.hash, receipt.hash);
        assert_eq!(appended.matches("### 2026-10-19").count(), 1);
        assert!(appended.contains("- promote decision"));
        assert_lines_preserved(&merged, &appended);
        // A stale selection hash is a conflict; the human edit is preserved.
        let edited = format!("{appended}user edit\n");
        fs::write(&path, &edited).unwrap();
        let conflict = append_entries_owned(&path, &entries, Some(&again.hash)).unwrap_err();
        assert!(conflict.conflict);
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        // A note that appeared after a selection that saw none is a conflict too.
        assert!(
            append_entries_owned(&path, &entries, None)
                .unwrap_err()
                .conflict
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), edited);
        // A note removed after selection is a conflict and is never recreated.
        let absent = dir.path().join("ScreenSearch/Projects/removed.md");
        let removed = append_entries_owned(&absent, &entries, Some(&receipt.hash)).unwrap_err();
        assert!(removed.conflict);
        assert!(!absent.exists());
    }

    #[test]
    fn promotion_conflicts_when_the_note_was_never_created() {
        // The project note is user-created (`03 §8e`): promoting into a path that was
        // never created is a conflict that writes nothing — no file, no parent dirs.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ScreenSearch/Projects/project.md");
        let entries = vec![(
            "Decisions".to_string(),
            "### 2026-10-18\n\n- promote decision".to_string(),
        )];
        let conflict = append_entries_owned(&path, &entries, None).unwrap_err();
        assert!(conflict.conflict);
        assert_eq!(
            conflict.message,
            "the project note is missing; create it before promoting",
        );
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
        assert!(!dir.path().join("ScreenSearch").exists());
    }
}
