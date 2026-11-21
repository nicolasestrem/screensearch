//! `export` - the dev-only JSON export of a fixture database.
//!
//! `ssv2c-evalfixture export --db <path> --out <file.json>` reads a fixture produced by
//! `ssv2c-evalfixture gen` and writes **one** UTF-8 JSON file (pretty-printed, LF newlines,
//! trailing newline) that any app with a plain JSON parser can read: no SQLite, no vectors,
//! no images, no full captured text. The same database always produces a byte-identical
//! file - every collection is sorted and every project id is assigned in a stable order, so
//! there is no hash-map iteration order, current time or randomness in the output.
//!
//! The file has exactly this shape, in this field order:
//!
//! ```json
//! {
//!   "persona": { "id", "description", "language", "country", "timezone", "os" },
//!   "range": { "start", "end" },
//!   "projects": [ { "id", "name", "first_seen", "last_seen", "session_count" } ],
//!   "sessions": [ { "id", "start", "end", "kind", "project_id",
//!                   "apps", "titles", "words" } ],
//!   "days": [ { "date", "active_minutes", "session_count", "top_project_id" } ]
//! }
//! ```
//!
//! * `persona` is the identity [`gen`](crate::gen) wrote under the `evalfixture.persona`
//!   setting, or the documented legacy descriptor (`northwind-tech`) for the pinned default
//!   fixture, which predates personas; it also supplies the fixed UTC offset of every
//!   timestamp. `os` is always `windows`: the product captures only Windows desktops, so no
//!   persona models another platform.
//! * Timestamps are ISO 8601 with that offset and millisecond precision
//!   (`2026-09-07T08:12:33.500+02:00`); `days[].date` is the local `YYYY-MM-DD` date.
//!   `range.start` is the earliest session start and `range.end` the latest session end.
//! * `sessions` are sorted by `(start, id)`. `apps` holds the session's distinct `app_hint`
//!   values alphabetically, `titles` its first five distinct non-empty window titles in
//!   frame order, and `words` up to eight distinct one-or-two-token fragments of its
//!   content text in frame order (tokens containing a URL scheme, separator or drive colon
//!   are dropped). `kind` is the session kind (`focus`, `meeting`, `ai`, `other`).
//! * `projects` group sessions by the context token their frames vote with: a browser URL
//!   reduces to its site (the host), except on a code host where it is the repository root
//!   (host plus owner and repository, route words dropped, the walk stopped before an id-like
//!   segment), otherwise an absolute path found in the window title is reduced to its project
//!   root (a trailing file name and build-folder components such as `src` or `docs` are
//!   stripped). Each session's token is its most frequent frame token (ties go to the earliest
//!   frame); sessions sharing a token are one project. Ids are `p01`, `p02`, ... in
//!   `(first_seen, token)` order; names humanise the token's last component. A session whose
//!   frames vote for no token gets `project_id: null`.
//! * `days` are local calendar dates sorted ascending. `active_minutes` is the summed
//!   session duration on that date in whole minutes, `session_count` counts sessions that
//!   start on it, and `top_project_id` is the project with the most such sessions (ties go
//!   to the lexicographically smallest id, and no project means `null`).
//!
//! A database with sessions but no frames still exports (no projects, days from the
//! sessions' own timestamps); a database with no sessions is an error, never an empty file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};
use serde::Serialize;
use store::SqliteStore;
use traits::{
    EvidenceWindow, FrameContinuation, FrameEvidence, FrameFilters, FramePageRequest, Session,
    SessionFilter, FRAME_PAGE_MAX_LIMIT,
};

use crate::gen::personas::{metadata_for_setting, offset_of, PersonaMetadata, PERSONA_SETTING};

const USAGE: &str = "\
USAGE:
  ssv2c-evalfixture export --db <path> --out <file.json>

OPTIONS:
  --db <path>     Fixture database written by `ssv2c-evalfixture gen` (required).
  --out <file>    Destination JSON file (required). An existing file is overwritten; the same
                  database always produces a byte-identical file.
";

/// Distinct window titles reported per session.
const TITLE_LIMIT: usize = 5;
/// Distinct content fragments reported per session.
const WORD_LIMIT: usize = 8;
/// Path components that name a build folder rather than a project: the token walks up past
/// them so `.../repo/src/main.rs` and `.../repo` group together.
const BUILD_FOLDERS: [&str; 14] = [
    "src", "tests", "test", "docs", "doc", "content", "app", "apps", "internal", "cmd", "pkg",
    "packages", "design", "assets",
];
/// A title is `path - app - ...`: the path ends at the title's first ` - ` separator.
const TITLE_SEPARATOR: &str = " - ";

/// Parsed `export` arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportOptions {
    db: PathBuf,
    out: PathBuf,
}

impl ExportOptions {
    /// Parses the arguments after `export`. Unknown flags and missing or dangling values are
    /// errors naming the flag, followed by the usage text.
    fn parse(args: &[String]) -> Result<Self> {
        let mut db: Option<PathBuf> = None;
        let mut out: Option<PathBuf> = None;
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let mut value = |flag: &str| -> Result<String> {
                iter.next()
                    .cloned()
                    .ok_or_else(|| anyhow!("`{flag}` needs a value\n\n{USAGE}"))
            };
            match arg.as_str() {
                "--db" => db = Some(PathBuf::from(value("--db")?)),
                "--out" => out = Some(PathBuf::from(value("--out")?)),
                other => bail!("unknown flag `{other}`\n\n{USAGE}"),
            }
        }
        Ok(Self {
            db: db.ok_or_else(|| anyhow!("`--db <path>` is required\n\n{USAGE}"))?,
            out: out.ok_or_else(|| anyhow!("`--out <file>` is required\n\n{USAGE}"))?,
        })
    }
}

/// `export` entry point: parse, read the fixture, write the JSON file and print a summary.
///
/// Synchronous: it builds its own current-thread Tokio runtime for the async Store API, so
/// it must be called from synchronous code (a plain `#[test]`, `spawn_blocking`, the CLI),
/// never from inside another runtime.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let options = ExportOptions::parse(args)?;
    ensure!(
        options.db.is_file(),
        "`--db` is not an existing file: {}",
        slash(&options.db)
    );
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "export::run builds its own Tokio runtime; call it from synchronous code (for example \
         through tokio::task::spawn_blocking), not from inside a runtime"
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building the export runtime")?;
    let database = runtime.block_on(read_database(&options.db))?;

    let document = build_document(database)?;
    let mut json =
        serde_json::to_string_pretty(&document).context("serializing the fixture export")?;
    json.push('\n');
    std::fs::write(&options.out, json.as_bytes())
        .with_context(|| format!("writing {}", slash(&options.out)))?;
    println!(
        "exported {} bytes to {} (persona={}, projects={}, sessions={}, days={})",
        json.len(),
        slash(&options.out),
        document.persona.id,
        document.projects.len(),
        document.sessions.len(),
        document.days.len()
    );
    Ok(())
}

/// One read of the fixture database, before the document is assembled.
struct FixtureRead {
    persona: PersonaMetadata,
    sessions: Vec<Session>,
    aggregates: BTreeMap<i64, SessionAggregate>,
}

/// The exported document, in the documented field order.
#[derive(Serialize)]
struct ExportDocument {
    persona: PersonaMetadata,
    range: RangeExport,
    projects: Vec<ProjectExport>,
    sessions: Vec<SessionExport>,
    days: Vec<DayExport>,
}

#[derive(Serialize)]
struct RangeExport {
    start: String,
    end: String,
}

#[derive(Serialize)]
struct ProjectExport {
    id: String,
    name: String,
    first_seen: String,
    last_seen: String,
    session_count: u64,
}

#[derive(Serialize)]
struct SessionExport {
    id: i64,
    start: String,
    end: String,
    kind: String,
    project_id: Option<String>,
    apps: Vec<String>,
    titles: Vec<String>,
    words: Vec<String>,
}

#[derive(Serialize)]
struct DayExport {
    date: String,
    active_minutes: i64,
    session_count: u64,
    top_project_id: Option<String>,
}

/// Per-session accumulator fed one frame at a time. Keeping only these small values means
/// the export never holds the fixture's content text in memory, however large it is.
#[derive(Default)]
struct SessionAggregate {
    apps: BTreeSet<String>,
    titles: Vec<String>,
    seen_titles: BTreeSet<String>,
    words: Vec<String>,
    seen_words: BTreeSet<String>,
    token_counts: BTreeMap<String, usize>,
    /// Tokens in first-seen frame order, which is the tie-break order for [`Self::token`].
    token_order: Vec<String>,
    last_frame_at: Option<i64>,
}

impl SessionAggregate {
    /// Folds one frame of this session into the accumulators.
    fn observe(&mut self, frame: &FrameEvidence) {
        self.last_frame_at = Some(match self.last_frame_at {
            Some(last) => last.max(frame.captured_at),
            None => frame.captured_at,
        });
        if let Some(app) = frame.app_hint.as_deref().filter(|app| !app.is_empty()) {
            self.apps.insert(app.to_string());
        }
        if self.titles.len() < TITLE_LIMIT {
            if let Some(title) = frame
                .window_title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
            {
                let title = title.to_string();
                if self.seen_titles.insert(title.clone()) {
                    self.titles.push(title);
                }
            }
        }
        if let Some(token) = frame_token(frame) {
            if !self.token_counts.contains_key(&token) {
                self.token_order.push(token.clone());
            }
            *self.token_counts.entry(token).or_insert(0) += 1;
        }
        if self.words.len() < WORD_LIMIT {
            if let Some(text) = frame.content_text.as_deref() {
                self.observe_text(text);
            }
        }
    }

    /// Up to [`WORD_LIMIT`] distinct short fragments, one per line: the first two tokens of
    /// the line that survive the URL/path filter, joined by a space. Fragments shorter than
    /// three characters and repeats are skipped; lines are taken in frame order.
    fn observe_text(&mut self, text: &str) {
        for line in text.lines() {
            if self.words.len() >= WORD_LIMIT {
                break;
            }
            let mut kept: Vec<&str> = Vec::new();
            for token in line.split_whitespace() {
                if token.contains("://")
                    || token.contains('\\')
                    || token.contains('/')
                    || token.contains(':')
                {
                    continue;
                }
                kept.push(token);
                if kept.len() == 2 {
                    break;
                }
            }
            let fragment = kept.join(" ");
            if fragment.chars().count() < 3 || !self.seen_words.insert(fragment.clone()) {
                continue;
            }
            self.words.push(fragment);
        }
    }

    /// The session's most frequent non-empty token, or `None` when no frame voted. Ties keep
    /// the earliest frame's token: `token_order` is first-seen order and `>` is strict.
    fn token(&self) -> Option<&str> {
        let mut best: Option<(&str, usize)> = None;
        for token in &self.token_order {
            let count = self.token_counts.get(token).copied().unwrap_or(0);
            if best.is_none_or(|(_, best_count)| count > best_count) {
                best = Some((token, count));
            }
        }
        best.map(|(token, _)| token)
    }
}

/// One project while its sessions are collected.
struct ProjectGroup {
    first_seen: i64,
    last_seen: i64,
    session_count: u64,
}

/// Accumulated per-day figures while sessions are visited in start order.
#[derive(Default)]
struct DayGroup {
    duration_ms: i64,
    session_count: u64,
    project_sessions: BTreeMap<String, u64>,
}

/// Reads the persona identity, every session and every frame of the fixture.
async fn read_database(db: &Path) -> Result<FixtureRead> {
    let store = SqliteStore::open_path(db).with_context(|| format!("opening {}", slash(db)))?;
    let setting = store
        .get_setting(PERSONA_SETTING)
        .await
        .with_context(|| format!("reading `{PERSONA_SETTING}` from {}", slash(db)))?;
    let persona = metadata_for_setting(setting.as_deref())?;
    let sessions = store
        .sessions_in_range(SessionFilter::default())
        .await
        .with_context(|| format!("reading the sessions of {}", slash(db)))?;
    ensure!(
        !sessions.is_empty(),
        "{} holds no sessions: run `ssv2c-evalfixture gen --out <dir>` first",
        slash(db)
    );
    let aggregates = read_frame_aggregates(&store).await?;
    Ok(FixtureRead {
        persona,
        sessions,
        aggregates,
    })
}

/// Streams every frame through [`SessionAggregate`]: `frame_page` yields one chronological
/// page at a time, and only the small per-session accumulators survive the page.
async fn read_frame_aggregates(store: &SqliteStore) -> Result<BTreeMap<i64, SessionAggregate>> {
    let window = EvidenceWindow {
        from: 0,
        to: i64::MAX,
    };
    let mut continuation: Option<FrameContinuation> = None;
    let mut aggregates: BTreeMap<i64, SessionAggregate> = BTreeMap::new();
    loop {
        let page = store
            .frame_page(FramePageRequest {
                window,
                filters: FrameFilters::default(),
                limit: FRAME_PAGE_MAX_LIMIT,
                continuation: continuation.take(),
            })
            .await
            .context("reading the fixture frames")?;
        for frame in &page.items {
            let Some(session_id) = frame.session_id else {
                continue;
            };
            aggregates.entry(session_id).or_default().observe(frame);
        }
        match page.next_position {
            Some(after) => {
                continuation = Some(FrameContinuation {
                    snapshot: page.snapshot,
                    after,
                })
            }
            None => break,
        }
    }
    Ok(aggregates)
}

/// Assembles the exported document from one read of the database.
fn build_document(read: FixtureRead) -> Result<ExportDocument> {
    let offset = offset_of(&read.persona.timezone)
        .with_context(|| format!("persona `{}`: invalid timezone", read.persona.id))?;
    let FixtureRead {
        persona,
        mut sessions,
        aggregates,
    } = read;
    sessions.sort_by_key(|session| (session.started_at, session.id));
    let (projects, project_of) = build_projects(&sessions, &aggregates, offset)?;
    let range = build_range(&sessions, &aggregates, offset)?;
    let session_exports = build_sessions(&sessions, &aggregates, &project_of, offset)?;
    let days = build_days(&sessions, &aggregates, &project_of, offset)?;
    Ok(ExportDocument {
        persona,
        range,
        projects,
        sessions: session_exports,
        days,
    })
}

/// Groups sessions into projects and returns them plus each session's project id.
///
/// Ids are assigned in `(first_seen, token)` order, so the returned list is already sorted
/// by `(first_seen, id)` as the format requires.
fn build_projects(
    sessions: &[Session],
    aggregates: &BTreeMap<i64, SessionAggregate>,
    offset: FixedOffset,
) -> Result<(Vec<ProjectExport>, BTreeMap<i64, String>)> {
    let mut groups: BTreeMap<String, ProjectGroup> = BTreeMap::new();
    for session in sessions {
        let Some(token) = aggregates
            .get(&session.id)
            .and_then(SessionAggregate::token)
        else {
            continue;
        };
        let end = session_end(session, aggregates.get(&session.id));
        let group = groups.entry(token.to_string()).or_insert(ProjectGroup {
            first_seen: session.started_at,
            last_seen: end,
            session_count: 0,
        });
        group.first_seen = group.first_seen.min(session.started_at);
        group.last_seen = group.last_seen.max(end);
        group.session_count += 1;
    }

    let mut seeds: Vec<(String, ProjectGroup)> = groups.into_iter().collect();
    seeds.sort_by(|(token_a, group_a), (token_b, group_b)| {
        (group_a.first_seen, token_a.as_str()).cmp(&(group_b.first_seen, token_b.as_str()))
    });

    let mut projects = Vec::with_capacity(seeds.len());
    let mut token_ids: BTreeMap<String, String> = BTreeMap::new();
    for (index, (token, group)) in seeds.into_iter().enumerate() {
        let id = format!("p{:02}", index + 1);
        projects.push(ProjectExport {
            id: id.clone(),
            name: humanise_token(&token),
            first_seen: timestamp(group.first_seen, offset)?,
            last_seen: timestamp(group.last_seen, offset)?,
            session_count: group.session_count,
        });
        token_ids.insert(token, id);
    }

    let mut project_of = BTreeMap::new();
    for session in sessions {
        if let Some(token) = aggregates
            .get(&session.id)
            .and_then(SessionAggregate::token)
        {
            if let Some(id) = token_ids.get(token) {
                project_of.insert(session.id, id.clone());
            }
        }
    }
    Ok((projects, project_of))
}

/// The exported time range: earliest session start through latest session end.
fn build_range(
    sessions: &[Session],
    aggregates: &BTreeMap<i64, SessionAggregate>,
    offset: FixedOffset,
) -> Result<RangeExport> {
    let start = sessions
        .first()
        .map(|session| session.started_at)
        .ok_or_else(|| anyhow!("no sessions to export"))?;
    let end = sessions
        .iter()
        .map(|session| session_end(session, aggregates.get(&session.id)))
        .max()
        .unwrap_or(start);
    Ok(RangeExport {
        start: timestamp(start, offset)?,
        end: timestamp(end, offset)?,
    })
}

/// The exported session rows, in `(start, id)` order.
fn build_sessions(
    sessions: &[Session],
    aggregates: &BTreeMap<i64, SessionAggregate>,
    project_of: &BTreeMap<i64, String>,
    offset: FixedOffset,
) -> Result<Vec<SessionExport>> {
    sessions
        .iter()
        .map(|session| {
            let aggregate = aggregates.get(&session.id);
            Ok(SessionExport {
                id: session.id,
                start: timestamp(session.started_at, offset)?,
                end: timestamp(session_end(session, aggregate), offset)?,
                kind: session.kind.as_str().to_string(),
                project_id: project_of.get(&session.id).cloned(),
                apps: aggregate
                    .map(|aggregate| aggregate.apps.iter().cloned().collect())
                    .unwrap_or_default(),
                titles: aggregate
                    .map(|aggregate| aggregate.titles.clone())
                    .unwrap_or_default(),
                words: aggregate
                    .map(|aggregate| aggregate.words.clone())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// The exported per-day rows, sorted by local date.
fn build_days(
    sessions: &[Session],
    aggregates: &BTreeMap<i64, SessionAggregate>,
    project_of: &BTreeMap<i64, String>,
    offset: FixedOffset,
) -> Result<Vec<DayExport>> {
    let mut days: BTreeMap<NaiveDate, DayGroup> = BTreeMap::new();
    for session in sessions {
        let date = local_date(session.started_at, offset)?;
        let day = days.entry(date).or_default();
        let duration = session_end(session, aggregates.get(&session.id)) - session.started_at;
        day.duration_ms += duration.max(0);
        day.session_count += 1;
        if let Some(project_id) = project_of.get(&session.id) {
            *day.project_sessions.entry(project_id.clone()).or_insert(0) += 1;
        }
    }
    days.into_iter()
        .map(|(date, day)| {
            Ok(DayExport {
                date: date.format("%Y-%m-%d").to_string(),
                active_minutes: day.duration_ms / 60_000,
                session_count: day.session_count,
                top_project_id: top_project(&day.project_sessions),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// The project with the most sessions on one day; `project_sessions` is ordered by id, so a
/// strict comparison keeps the lexicographically smallest id on ties.
fn top_project(project_sessions: &BTreeMap<String, u64>) -> Option<String> {
    let mut top: Option<(&String, u64)> = None;
    for (project, count) in project_sessions {
        if top.is_none_or(|(_, best)| *count > best) {
            top = Some((project, *count));
        }
    }
    top.map(|(project, _)| project.clone())
}

/// A session's end: its recorded `ended_at`, or the last frame time for an open session,
/// falling back to its start when it owns no frames.
fn session_end(session: &Session, aggregate: Option<&SessionAggregate>) -> i64 {
    session
        .ended_at
        .or_else(|| aggregate.and_then(|aggregate| aggregate.last_frame_at))
        .unwrap_or(session.started_at)
}

/// The context token a frame votes with: its browser URL, otherwise an absolute path in its
/// window title. Frames with neither cast no vote.
fn frame_token(frame: &FrameEvidence) -> Option<String> {
    if let Some(url) = frame
        .browser_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        return url_token(url);
    }
    frame.window_title.as_deref().and_then(title_path_token)
}

/// Code hosts whose URLs are grouped by repository rather than by site.
const CODE_HOSTS: [&str; 4] = ["bitbucket.org", "codeberg.org", "github.com", "gitlab.com"];

/// A scheme-less URL reduced to the context it belongs to: the **site** (its host), except on
/// a code host, where it is the repository root (host plus owner and repository, with route
/// words dropped and the walk stopped before an id-like segment). Examples:
/// `https://news.example/business/2026/story` -> `news.example`;
/// `https://acme.atlassian.net/browse/OPS-7` -> `acme.atlassian.net`;
/// `https://github.com/acme/repo/pulls/1?x=2` -> `github.com/acme/repo`.
fn url_token(url: &str) -> Option<String> {
    let rest = url.trim();
    let rest = rest
        .strip_prefix("https://")
        .or_else(|| rest.strip_prefix("http://"))
        .unwrap_or(rest);
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut segments = rest.split('/');
    let host = segments.next()?.trim();
    if host.is_empty() {
        return None;
    }
    let mut token = host.to_string();
    if !CODE_HOSTS.contains(&host) {
        return Some(token);
    }
    let mut kept = 0usize;
    for segment in segments.filter(|segment| !segment.is_empty()) {
        let lower = segment.to_ascii_lowercase();
        if ROUTE_WORDS.contains(&lower.as_str()) {
            continue;
        }
        if looks_like_id(segment) {
            break;
        }
        token.push('/');
        token.push_str(segment);
        kept += 1;
        if kept == 2 {
            break;
        }
    }
    Some(token)
}

/// Path segments that name an action or a route rather than a context.
const ROUTE_WORDS: [&str; 26] = [
    "admin",
    "blob",
    "browse",
    "commit",
    "commits",
    "d",
    "design",
    "document",
    "documents",
    "edit",
    "file",
    "files",
    "incident",
    "incidents",
    "issue",
    "issues",
    "merge_requests",
    "pull",
    "pull-requests",
    "pulls",
    "questions",
    "search",
    "tree",
    "view",
    "watch",
    "wiki",
];

/// Whether a URL or path segment is an identifier rather than a name: all digits, a tracker
/// key (`LED-142`, `OPS-7`), or an opaque token with digits (a UUID, a commit hash, an id).
fn looks_like_id(segment: &str) -> bool {
    if segment.is_empty() {
        return true;
    }
    if segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    let has_digit = segment.bytes().any(|byte| byte.is_ascii_digit());
    if !has_digit {
        return false;
    }
    let has_lower = segment.bytes().any(|byte| byte.is_ascii_lowercase());
    if !has_lower && segment.len() >= 4 {
        return true;
    }
    if segment.len() >= 12 {
        return true;
    }
    segment.len() >= 8 && segment.contains('-')
}

/// An absolute path inside a window title, reduced to its project root.
fn title_path_token(title: &str) -> Option<String> {
    let start = drive_path_start(title).or_else(|| unix_path_start(title))?;
    let tail = &title[start..];
    let end = tail.find(TITLE_SEPARATOR).unwrap_or(tail.len());
    path_token(tail[..end].trim_end())
}

/// The byte index where a Windows drive path (`C:\...` or `C:/...`) starts inside `title`.
/// A path is not detected inside a URL scheme (`https:`), where the drive colon follows an
/// alphabetic byte that is itself preceded by an alphabetic byte.
fn drive_path_start(title: &str) -> Option<usize> {
    let bytes = title.as_bytes();
    (0..bytes.len().saturating_sub(2)).find(|&index| {
        bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && matches!(bytes[index + 2], b'\\' | b'/')
            && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric())
    })
}

/// The byte index where a `/Users/`, `/home/` or `/mnt/` absolute path starts inside
/// `title`; the earliest match wins.
fn unix_path_start(title: &str) -> Option<usize> {
    ["/Users/", "/home/", "/mnt/"]
        .into_iter()
        .filter_map(|prefix| title.find(prefix))
        .min()
}

/// Normalises a title's path into a context token: separators become `/`, a trailing file
/// name (last component containing a `.`) is dropped, then build-folder components are
/// walked up. Returns `None` for a path with no meaningful component left.
fn path_token(path: &str) -> Option<String> {
    let normalised = path.replace('\\', "/");
    let rooted = normalised.starts_with('/');
    let mut components: Vec<&str> = normalised
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if components.last().is_some_and(|last| last.contains('.')) {
        components.pop();
    }
    while components.len() > 1 {
        let last = components.last()?.to_ascii_lowercase();
        if !BUILD_FOLDERS.contains(&last.as_str()) {
            break;
        }
        components.pop();
    }
    if components.is_empty() {
        return None;
    }
    let mut token = components.join("/");
    if rooted {
        token.insert(0, '/');
    }
    Some(token)
}

/// A project name from its token: `-`/`_` become spaces, the first letter is uppercased and
/// at most three words are kept. A last component shorter than three characters or all digits
/// keeps the previous component in front of it. A site-only token names its host: `www.` and
/// the top-level domain are dropped (`www.agroteruel.example` -> `Agroteruel`,
/// `intranet.acme.example` -> `Intranet acme`).
fn humanise_token(token: &str) -> String {
    let components: Vec<&str> = token
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    let Some(&last) = components.last() else {
        return token.to_string();
    };
    if components.len() == 1 && last.contains('.') {
        let host = last.strip_prefix("www.").unwrap_or(last);
        let mut labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
        if labels.len() > 1 {
            labels.pop();
        }
        return humanise_words(&labels.join(" "));
    }
    let mut parts = vec![last];
    let tiny_or_numeric = last.chars().count() < 3 || last.chars().all(|ch| ch.is_ascii_digit());
    if tiny_or_numeric {
        if let Some(previous) = components
            .len()
            .checked_sub(2)
            .map(|index| components[index])
        {
            parts.insert(0, previous);
        }
    }
    humanise_words(&parts.join(" "))
}

/// `-`/`_` become spaces, at most three words are kept and the first letter is uppercased.
fn humanise_words(raw: &str) -> String {
    let spaced = raw.replace(['-', '_'], " ");
    let words: Vec<&str> = spaced.split_whitespace().take(3).collect();
    let mut name = words.join(" ");
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        name = first.to_uppercase().collect::<String>() + chars.as_str();
    }
    name
}

/// The ISO 8601 form of a Unix-millisecond instant in `offset`, with millisecond precision
/// and an explicit offset (`2026-09-07T08:12:33.500+02:00`).
fn timestamp(at_ms: i64, offset: FixedOffset) -> Result<String> {
    Ok(instant(at_ms)?
        .with_timezone(&offset)
        .to_rfc3339_opts(SecondsFormat::Millis, false))
}

/// The calendar date of a Unix-millisecond instant in `offset`.
fn local_date(at_ms: i64, offset: FixedOffset) -> Result<NaiveDate> {
    Ok(instant(at_ms)?.with_timezone(&offset).date_naive())
}

/// The instant for a Unix-millisecond timestamp; out-of-range values are an error rather
/// than a silently changed export.
fn instant(at_ms: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(at_ms)
        .ok_or_else(|| anyhow!("timestamp {at_ms} ms is outside chrono's supported range"))
}

/// A path as documented, persisted and printed: forward slashes.
fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
