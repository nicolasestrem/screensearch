//! The heuristic extraction rule table (`03 §7h.2`, `docs/kickoffs/P5.md`).
//!
//! Deterministic, model-free, and complete: this module reads only the stored frame
//! context (`frames.window_title`, `frames.browser_url`, `frames.app_hint`), the filtered
//! content text, the session the frame falls into and the session tool recognition passed
//! in by the kernel (a read-only view of [`traits::ToolRecognition`]).
//!
//! Every hit carries `source='heuristic'` semantics and the confidence of the rule that
//! produced it ([`confidence`]), never invented or smoothed. Confidence `0.60` is
//! deliberately below the clustering threshold, so an unqualified `#123` exists as an
//! entity but can never seed a thread.
//!
//! The mechanism is fixed by `03 §7h.2`; the numeric values and the closed extension,
//! host, app-stem and title lists below are P5 decisions recorded in
//! `specs/05_BUILD_REVIEW.md`.
//!
//! One `03 §7h.2` sentence needs a written reading here: the dedup paragraph says to
//! "keep the host and path for `domain`", while the rule table defines a domain entity from
//! "scheme plus host" / "host only". A domain entity's identity is the host the rule names
//! (fragment, query and trailing slash dropped); the path-preserving clause applies to the
//! kinds whose identity is a path (`file`, `document`, `repo`). The P4 reference oracle also
//! normalizes a URL's domain to its host, so the other reading would make the metric
//! disagree with the rule table itself. Recorded as `06` #39.

use std::collections::BTreeMap;

use traits::{EntityKind, GraphFrameInput, GraphHit, SessionHost, ToolRecognition};

/// The rule table's own confidence values (`03 §7h.2`, recorded as a P5 decision in `05`).
pub mod confidence {
    /// Browser URL with a scheme: the host is a `domain` entity.
    pub const URL_DOMAIN_SCHEME: f64 = 0.95;
    /// Browser URL without a scheme: the host is a `domain` entity.
    pub const URL_DOMAIN_HOST_ONLY: f64 = 0.90;
    /// Browser URL whose path ends in a document extension: a `document` entity.
    pub const URL_DOCUMENT: f64 = 0.85;
    /// Browser URL whose path is `/pull/<n>`, `/issues/<n>` or `/merge_requests/<n>`: a `ref`.
    pub const URL_REF: f64 = 0.95;
    /// Any text containing a repository identity `<host>/<owner>/<repo>`: a `repo`.
    pub const TEXT_REPO: f64 = 0.95;
    /// Any text containing an absolute Windows path with a drive letter and an extension.
    pub const TEXT_FILE: f64 = 0.95;
    /// Window title containing a document name ending in a document extension.
    pub const TITLE_DOCUMENT: f64 = 0.90;
    /// IDE title's leading file segment (`file - workspace - editor`).
    pub const IDE_FILE: f64 = 0.90;
    /// IDE title's workspace segment.
    pub const IDE_WORKSPACE_REPO: f64 = 0.80;
    /// Terminal cwd or prompt carrying an absolute directory path.
    pub const TERMINAL_DIRECTORY_REPO: f64 = 0.80;
    /// Any text containing an `ABC-123` style ticket key.
    pub const TEXT_TICKET: f64 = 0.85;
    /// `#123` with an adjacent issue/PR/ticket word.
    pub const TEXT_HASH_QUALIFIED: f64 = 0.85;
    /// A bare `#123`. Below the 0.7 clustering threshold by design (`03 §7h.2`).
    pub const TEXT_HASH_BARE: f64 = 0.60;
}

/// Document extensions the `document` rule accepts. A closed list: an unknown extension is
/// not a document reference. Kept identical in meaning to the P4 reference extractor so the
/// entity metric can disagree about the engine, not about what a document name is.
pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "md", "txt", "xlsx", "xls", "pptx", "ppt", "csv", "json", "yaml", "yml",
    "toml",
];

/// Code hosts whose `<host>/<owner>/<repo>` token is a repository identity. Closed list,
/// no guessing: the extraction rule names a "repository identity", not any three-segment path.
pub const CODE_HOSTS: &[&str] = &["github.com", "gitlab.com", "bitbucket.org", "codeberg.org"];

/// Application stems (process hints, normalized to lowercase without `.exe`) that identify an
/// IDE window. `03 §7h.2` names the "IDE title" source; the recognition taxonomy does not
/// carry VS Code (it was dropped from the PR2 set), so the app hint is the shipped signal.
pub const IDE_APP_STEMS: &[&str] = &[
    "code",
    "code - insiders",
    "vscodium",
    "devenv",
    "idea64",
    "idea",
    "pycharm64",
    "pycharm",
    "webstorm64",
    "webstorm",
    "goland64",
    "goland",
    "clion64",
    "clion",
    "rider64",
    "rider",
    "phpstorm64",
    "phpstorm",
    "android studio",
    "zed",
    "sublime_text",
    "cursor",
    "notepad++",
];

/// Application stems that identify a terminal window.
pub const TERMINAL_APP_STEMS: &[&str] = &[
    "windowsterminal",
    "wt",
    "powershell",
    "pwsh",
    "cmd",
    "conhost",
    "mintty",
    "alacritty",
    "wezterm",
    "konsole",
    "gnome-terminal",
    "kitty",
    "hyper",
    "wsl",
    "bash",
    "zsh",
    "fish",
];

/// IDE editor names a three-segment `<file> - <workspace> - <editor>` title ends with. The
/// title shape is the spec's own (`03 §7h.2` table); this closed list is how a window title
/// is recognized as that shape when the app hint does not identify the IDE.
pub const IDE_EDITOR_TITLE_SUFFIXES: &[&str] = &[
    "visual studio code",
    "visual studio",
    "intellij idea",
    "webstorm",
    "pycharm",
    "phpstorm",
    "goland",
    "clion",
    "rider",
    "android studio",
    "zed",
    "sublime text",
    "cursor",
    "notepad++",
    "xcode",
];

/// Shell names a terminal title ends with (`<cwd> - <shell>`), the fixture and the common
/// Windows shells. Used only to classify the window as a terminal when the app hint is
/// missing or unknown.
pub const TERMINAL_TITLE_SUFFIXES: &[&str] = &[
    "powershell",
    "windows powershell",
    "command prompt",
    "cmd",
    "bash",
    "zsh",
    "fish",
    "pwsh",
    "wsl",
];

/// Words that turn a bare `#123` into a qualified issue/PR reference (`03 §7h.2` table).
pub const ISSUE_WORDS: &[&str] = &[
    "issue", "issues", "pr", "prs", "pull", "ticket", "tickets", "bug", "bugs", "fix", "fixes",
    "fixed", "close", "closes", "closed", "ref", "refs", "resolve", "resolves", "resolved",
    "merge", "mr", "patch",
];

/// Path segments that make a URL path an issue/PR reference (`03 §7h.2` table).
const URL_REF_SEGMENTS: &[&str] = &["pull", "issues", "merge_requests"];

/// One frame's extraction input (`03 §7h.2`).
#[derive(Debug, Clone, Copy)]
pub struct ExtractionInput<'a> {
    pub frame: &'a GraphFrameInput,
    /// The bundled taxonomy's recognition for this frame context, when the kernel resolved
    /// one. It is input data, never a model call.
    pub recognition: Option<&'a ToolRecognition>,
}

/// Runs the whole rule table for one frame and returns the deduplicated hits.
///
/// Deterministic: the same input always yields the same hits in the same order. Duplicate
/// hits for the same `(kind, normalized_key)` collapse, keeping the highest rule confidence
/// and merging their aliases; the first-seen canonical display form wins.
pub fn extract(input: &ExtractionInput<'_>) -> Vec<GraphHit> {
    let frame = input.frame;
    let mut hits: BTreeMap<(EntityKind, String), GraphHit> = BTreeMap::new();

    let push = |kind: EntityKind,
                canonical: &str,
                confidence: f64,
                aliases: Vec<String>,
                hits: &mut BTreeMap<(EntityKind, String), GraphHit>| {
        let canonical = canonical.trim();
        if canonical.is_empty() {
            return;
        }
        let key = normalized_key(kind, canonical);
        if key.is_empty() {
            return;
        }
        let aliases: Vec<String> = aliases
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case(canonical))
            .collect();
        match hits.get_mut(&(kind, key.clone())) {
            Some(existing) => {
                existing.confidence = existing.confidence.max(confidence);
                for alias in aliases {
                    if !existing
                        .aliases
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(&alias))
                    {
                        existing.aliases.push(alias);
                    }
                }
            }
            None => {
                hits.insert(
                    (kind, key.clone()),
                    GraphHit {
                        kind,
                        canonical_name: canonical.to_string(),
                        normalized_key: key,
                        confidence,
                        aliases,
                    },
                );
            }
        }
    };

    // 1. Browser URL rules (`03 §7h.2` table, first four rows). The host-only form is the
    // same URL shape without a scheme.
    if let Some(url) = frame.browser_url.as_deref() {
        if let Some((scheme, host, path)) = parse_url(url) {
            push(
                EntityKind::Domain,
                &host,
                if scheme {
                    confidence::URL_DOMAIN_SCHEME
                } else {
                    confidence::URL_DOMAIN_HOST_ONLY
                },
                Vec::new(),
                &mut hits,
            );
            if let Some(document) = path_document(path) {
                push(
                    EntityKind::Document,
                    document,
                    confidence::URL_DOCUMENT,
                    Vec::new(),
                    &mut hits,
                );
            }
            if let Some((number, matched_path)) = path_ref(path) {
                push(
                    EntityKind::Ref,
                    &format!("#{number}"),
                    confidence::URL_REF,
                    vec![matched_path],
                    &mut hits,
                );
            }
        }
    }

    // 2. "Any text" rules: repository identity, absolute Windows file path, ticket keys and
    // `#123` references (`03 §7h.2` table).
    for text in any_text_sources(frame) {
        for (index, raw) in text.split_whitespace().enumerate() {
            let token = clean(raw);
            if token.is_empty() {
                continue;
            }
            if let Some(repo) = token_repo(token) {
                push(
                    EntityKind::Repo,
                    &repo,
                    confidence::TEXT_REPO,
                    Vec::new(),
                    &mut hits,
                );
            }
            if is_windows_absolute_path(token) && path_has_extension(token) {
                push(
                    EntityKind::File,
                    token,
                    confidence::TEXT_FILE,
                    Vec::new(),
                    &mut hits,
                );
            }
            if let Some(ticket) = ticket_key(token) {
                push(
                    EntityKind::Ref,
                    ticket,
                    confidence::TEXT_TICKET,
                    Vec::new(),
                    &mut hits,
                );
            }
            if let Some(number) = hash_ref(token) {
                let qualified = adjacent_issue_word(text, index);
                push(
                    EntityKind::Ref,
                    &format!("#{number}"),
                    if qualified {
                        confidence::TEXT_HASH_QUALIFIED
                    } else {
                        confidence::TEXT_HASH_BARE
                    },
                    Vec::new(),
                    &mut hits,
                );
            }
        }
    }

    // 3. Window title document names (`03 §7h.2` table).
    if let Some(title) = frame.window_title.as_deref() {
        for raw in title.split_whitespace() {
            let token = clean(raw);
            if let Some(name) = bare_document_name(token) {
                push(
                    EntityKind::Document,
                    name,
                    confidence::TITLE_DOCUMENT,
                    Vec::new(),
                    &mut hits,
                );
            }
        }
    }

    // 4. IDE and terminal title shapes, gated on the window's host lane.
    let lane = host_lane(input);
    if lane == Some(Lane::Ide) {
        if let Some(title) = frame.window_title.as_deref() {
            let segments = title_segments(title);
            if segments.len() >= 2 {
                let leading = segments[0];
                if looks_like_file_segment(leading) {
                    push(
                        EntityKind::File,
                        leading,
                        confidence::IDE_FILE,
                        Vec::new(),
                        &mut hits,
                    );
                }
                if segments.len() >= 3 {
                    let workspace = segments[1];
                    if !workspace.is_empty() {
                        push(
                            EntityKind::Repo,
                            workspace,
                            confidence::IDE_WORKSPACE_REPO,
                            Vec::new(),
                            &mut hits,
                        );
                    }
                }
            }
        }
    }
    if lane == Some(Lane::Terminal) {
        // The cwd is in the title (`<cwd> - <shell>`) or a prompt line in the content text
        // (`cwd: <path>`, `PS <path>>`). A directory becomes a `repo` anchor; a path with an
        // extension already matched the file rule above.
        for text in terminal_context_sources(frame) {
            for raw in text.split_whitespace() {
                let token = clean(raw);
                if is_windows_absolute_path(token) && !path_has_extension(token) {
                    push(
                        EntityKind::Repo,
                        token,
                        confidence::TERMINAL_DIRECTORY_REPO,
                        Vec::new(),
                        &mut hits,
                    );
                }
            }
        }
    }

    hits.into_values().collect()
}

/// The `03 §7h.2` dedup identity, defined once for every kind: lowercase, trimmed,
/// path/URL-normalized.
///
/// - lowercase; backslashes to forward slashes;
/// - strip `http://`, `https://` and a leading `www.`;
/// - drop a URL fragment; for `domain` also drop the query string and a trailing slash;
/// - drop a trailing slash and collapse repeated `/` separators;
/// - keep the path for `file`, `document` and `repo` (the path is the identity);
/// - `ref` is the normalized token itself (`#123` stays `#123`, it has no URL fragment).
pub fn normalized_key(kind: EntityKind, raw: &str) -> String {
    let mut text = raw.trim().to_ascii_lowercase();
    if kind == EntityKind::Ref {
        text.retain(|c| !c.is_whitespace());
        return text;
    }
    text = strip_scheme_and_www(text);
    text = text.replace('\\', "/");
    if kind == EntityKind::Domain {
        if let Some(index) = text.find(['#', '?']) {
            text.truncate(index);
        }
    }
    while text.contains("//") {
        text = text.replace("//", "/");
    }
    while text.len() > 1 && text.ends_with('/') {
        text.pop();
    }
    text
}

fn strip_scheme_and_www(mut text: String) -> String {
    if let Some(rest) = text.strip_prefix("https://") {
        text = rest.to_string();
    } else if let Some(rest) = text.strip_prefix("http://") {
        text = rest.to_string();
    }
    if let Some(rest) = text.strip_prefix("www.") {
        text = rest.to_string();
    }
    text
}

/// Splits a URL-ish token into `(had_scheme, host, path)`. `None` when there is no valid
/// host (an empty value, a non-HTTP scheme, an IPv6 literal, or a bare word).
fn parse_url(raw: &str) -> Option<(bool, String, &str)> {
    let trimmed = raw.trim();
    let (scheme, rest) = match trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.starts_with('[') {
        return None;
    }
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or(host_port);
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if !is_domain_name(&host) {
        return None;
    }
    Some((scheme, host, &rest[end..]))
}

fn is_domain_name(host: &str) -> bool {
    if host.len() < 3 || !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    if host
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '\\' || c == '@')
    {
        return false;
    }
    let label = host.rsplit('.').next().unwrap_or("");
    label.len() >= 2 && label.chars().all(|c| c.is_ascii_alphabetic())
}

/// The last path segment's bare file name when it ends in a document extension.
fn path_document(path: &str) -> Option<&str> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let name = path.rsplit('/').find(|segment| !segment.is_empty())?;
    if name.contains('.') && document_extension(name).is_some() {
        Some(name)
    } else {
        None
    }
}

/// The first `/pull/<n>`, `/issues/<n>` or `/merge_requests/<n>` in a URL path, as
/// `(number, matched path segment pair)`.
fn path_ref(path: &str) -> Option<(String, String)> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for pair in segments.windows(2) {
        let marker = pair[0].to_ascii_lowercase();
        let number = pair[1].trim();
        if URL_REF_SEGMENTS.contains(&marker.as_str())
            && !number.is_empty()
            && number.len() <= 6
            && number.bytes().all(|b| b.is_ascii_digit())
        {
            return Some((number.to_string(), format!("/{}/{}", pair[0], pair[1])));
        }
    }
    None
}

fn any_text_sources(frame: &GraphFrameInput) -> Vec<&str> {
    [
        frame.window_title.as_deref(),
        frame.browser_url.as_deref(),
        frame.content_text.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn terminal_context_sources(frame: &GraphFrameInput) -> Vec<&str> {
    [frame.window_title.as_deref(), frame.content_text.as_deref()]
        .into_iter()
        .flatten()
        .collect()
}

/// Trims the punctuation a reference token can carry at either end. Mirrors the P4 reference
/// extractor's rule; it is deliberately a separate implementation (independence is the point
/// of that oracle).
fn clean(raw: &str) -> &str {
    raw.trim_matches(|c: char| {
        "([{<\"'`".contains(c) || ")]}>\"'`,;:!?.".contains(c) || c.is_whitespace()
    })
}

fn strip_www(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// A token that is a repository identity: `<code-host>/<owner>/<repo>` with or without a
/// scheme, or a full URL on a code host.
fn token_repo(token: &str) -> Option<String> {
    if token.starts_with("https://") || token.starts_with("http://") {
        let (_, host, path) = parse_url(token)?;
        return code_host_repo(&host, path);
    }
    let mut parts = token.split('/');
    let host = strip_www(&parts.next()?.to_ascii_lowercase()).to_string();
    if !CODE_HOSTS.contains(&host.as_str()) {
        return None;
    }
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())?
        .trim_end_matches(".git");
    if repo.is_empty() {
        return None;
    }
    Some(format!("{host}/{owner}/{repo}"))
}

fn code_host_repo(host: &str, path: &str) -> Option<String> {
    if !CODE_HOSTS.contains(&strip_www(host)) {
        return None;
    }
    let clean_path = path.split(['?', '#']).next().unwrap_or(path);
    let segments: Vec<&str> = clean_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .take(2)
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let repo = segments[1].trim_end_matches(".git");
    if repo.is_empty() {
        return None;
    }
    Some(format!("{}/{}/{}", strip_www(host), segments[0], repo))
}

fn is_windows_absolute_path(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() >= 4
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn path_has_extension(token: &str) -> bool {
    let name = token.rsplit(['/', '\\']).next().unwrap_or(token);
    match name.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && !ext.is_empty(),
        None => false,
    }
}

/// A filename with a document extension and no path separators (`report.pdf`).
fn bare_document_name(token: &str) -> Option<&str> {
    if token.is_empty() || token.contains('/') || token.contains('\\') {
        return None;
    }
    document_extension(token).map(|_| token)
}

fn document_extension(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    let ext = ext.to_ascii_lowercase();
    DOCUMENT_EXTENSIONS
        .iter()
        .find(|known| **known == ext)
        .copied()
}

/// `ABC-123`: uppercase prefix of 2 to 10 letters followed by up to six digits.
fn ticket_key(token: &str) -> Option<&str> {
    let (prefix, digits) = token.split_once('-')?;
    let prefix_ok = (2..=10).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_uppercase())
        && prefix
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_uppercase());
    if prefix_ok && is_short_number(digits) {
        Some(token)
    } else {
        None
    }
}

/// `#123`: up to six digits after the `#`.
fn hash_ref(token: &str) -> Option<&str> {
    let digits = token.strip_prefix('#')?;
    is_short_number(digits).then_some(digits)
}

fn is_short_number(value: &str) -> bool {
    !value.is_empty() && value.len() <= 6 && value.bytes().all(|b| b.is_ascii_digit())
}

/// Whether the `#123` at `index` has an issue/PR/ticket word immediately before or after it.
fn adjacent_issue_word(text: &str, index: usize) -> bool {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let is_issue_word = |raw: &str| {
        let token = clean(raw);
        ISSUE_WORDS
            .iter()
            .any(|word| token.eq_ignore_ascii_case(word))
    };
    index
        .checked_sub(1)
        .and_then(|i| tokens.get(i))
        .is_some_and(|raw| is_issue_word(raw))
        || tokens.get(index + 1).is_some_and(|raw| is_issue_word(raw))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Ide,
    Terminal,
}

/// Which window lane the frame belongs to, used to gate the IDE and terminal title rules.
///
/// Order: the persisted session host, then the taxonomy recognition, then the app hint, then
/// the title shape. A known non-IDE, non-terminal host (browser, desktop) answers `None`
/// rather than falling through, so a browser title can never be read as an IDE title.
fn host_lane(input: &ExtractionInput<'_>) -> Option<Lane> {
    let frame = input.frame;
    let host = frame
        .session
        .as_ref()
        .and_then(|session| session.host)
        .or_else(|| input.recognition.and_then(|recognition| recognition.host));
    if let Some(host) = host {
        return match host {
            SessionHost::Ide => Some(Lane::Ide),
            SessionHost::Terminal => Some(Lane::Terminal),
            SessionHost::Browser | SessionHost::Desktop => None,
        };
    }
    let stem = frame.app_hint.as_deref().map(normalize_stem);
    if let Some(stem) = stem.as_deref() {
        if IDE_APP_STEMS.contains(&stem) {
            return Some(Lane::Ide);
        }
        if TERMINAL_APP_STEMS.contains(&stem) {
            return Some(Lane::Terminal);
        }
        // A known app hint that is neither an IDE nor a terminal (a browser, a chat app, an
        // office app) closes the question: the title shape alone never reclassifies it.
        return None;
    }
    let title = frame.window_title.as_deref()?;
    let segments = title_segments(title);
    let last = segments.last()?;
    if IDE_EDITOR_TITLE_SUFFIXES
        .iter()
        .any(|editor| last.to_ascii_lowercase().contains(editor))
    {
        return Some(Lane::Ide);
    }
    if segments.len() <= 2
        && TERMINAL_TITLE_SUFFIXES
            .iter()
            .any(|shell| last.eq_ignore_ascii_case(shell))
    {
        return Some(Lane::Terminal);
    }
    None
}

fn normalize_stem(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    lower
        .strip_suffix(".exe")
        .map_or(lower.clone(), str::to_string)
}

/// Splits `file - workspace - editor` style titles. The separator is the spec's own shape
/// (space hyphen space); an empty title has no segments.
fn title_segments(title: &str) -> Vec<&str> {
    title
        .split(" - ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn looks_like_file_segment(segment: &str) -> bool {
    if segment.contains('/') || segment.contains('\\') {
        return true;
    }
    path_has_extension(segment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clustering;
    use traits::{GraphSessionInput, SessionKind};

    fn frame(
        window_title: Option<&str>,
        browser_url: Option<&str>,
        app_hint: Option<&str>,
        content_text: Option<&str>,
    ) -> GraphFrameInput {
        GraphFrameInput {
            frame_id: 1,
            captured_at: 1_700_000_000_000,
            app_hint: app_hint.map(str::to_string),
            window_title: window_title.map(str::to_string),
            browser_url: browser_url.map(str::to_string),
            content_text: content_text.map(str::to_string),
            session: None,
        }
    }

    fn extract_plain(frame: &GraphFrameInput) -> Vec<GraphHit> {
        extract(&ExtractionInput {
            frame,
            recognition: None,
        })
    }

    fn hits_of_kind(frame: &GraphFrameInput, kind: EntityKind) -> Vec<GraphHit> {
        extract_plain(frame)
            .into_iter()
            .filter(|hit| hit.kind == kind)
            .collect()
    }

    #[test]
    fn browser_url_with_scheme_yields_a_domain_at_095() {
        let f = frame(
            None,
            Some("https://github.com/acme/widgets/pull/42?tab=files"),
            Some("chrome"),
            None,
        );
        let domains = hits_of_kind(&f, EntityKind::Domain);
        assert_eq!(domains.len(), 1, "{domains:?}");
        assert_eq!(domains[0].canonical_name, "github.com");
        assert_eq!(domains[0].confidence, confidence::URL_DOMAIN_SCHEME);
    }

    #[test]
    fn browser_url_host_only_yields_a_domain_at_090() {
        let f = frame(
            None,
            Some("www.example.com/docs/plan.pdf"),
            Some("chrome"),
            None,
        );
        let domains = hits_of_kind(&f, EntityKind::Domain);
        assert_eq!(domains.len(), 1, "{domains:?}");
        assert_eq!(domains[0].canonical_name, "www.example.com");
        assert_eq!(domains[0].confidence, confidence::URL_DOMAIN_HOST_ONLY);
    }

    #[test]
    fn browser_url_path_document_is_085() {
        let f = frame(
            None,
            Some("https://example.com/docs/plan.pdf"),
            Some("chrome"),
            None,
        );
        let documents = hits_of_kind(&f, EntityKind::Document);
        assert_eq!(documents.len(), 1, "{documents:?}");
        assert_eq!(documents[0].canonical_name, "plan.pdf");
        assert_eq!(documents[0].confidence, confidence::URL_DOCUMENT);
        assert_eq!(documents[0].normalized_key, "plan.pdf");
    }

    #[test]
    fn browser_url_issue_and_pr_paths_are_refs_at_095() {
        for (url, expected) in [
            ("https://github.com/acme/widgets/pull/42", "#42"),
            ("https://github.com/acme/widgets/issues/7", "#7"),
            (
                "https://gitlab.com/team/service/merge_requests/1234",
                "#1234",
            ),
        ] {
            let f = frame(None, Some(url), Some("chrome"), None);
            let refs = hits_of_kind(&f, EntityKind::Ref);
            assert!(
                refs.iter()
                    .any(|hit| hit.canonical_name == expected
                        && hit.confidence == confidence::URL_REF),
                "{url}: {refs:?}"
            );
        }
    }

    #[test]
    fn repository_identity_is_095_from_a_url_and_a_bare_token() {
        let from_url = frame(
            Some("PR - https://github.com/acme/widgets/pull/42"),
            None,
            Some("chrome"),
            None,
        );
        let repos = hits_of_kind(&from_url, EntityKind::Repo);
        assert_eq!(repos.len(), 1, "{repos:?}");
        assert_eq!(repos[0].canonical_name, "github.com/acme/widgets");
        assert_eq!(repos[0].confidence, confidence::TEXT_REPO);

        let bare = frame(
            Some("cd gitlab.com/team/service"),
            None,
            Some("terminal"),
            None,
        );
        let repos = hits_of_kind(&bare, EntityKind::Repo);
        assert!(
            repos
                .iter()
                .any(|hit| hit.canonical_name == "gitlab.com/team/service"
                    && hit.confidence == confidence::TEXT_REPO),
            "{repos:?}"
        );
    }

    #[test]
    fn absolute_windows_path_with_extension_is_a_file_at_095() {
        let f = frame(
            Some(r"C:\dev\ledgerline\src\main.rs - ledgerline - Visual Studio Code"),
            None,
            Some("Code"),
            None,
        );
        let files = hits_of_kind(&f, EntityKind::File);
        assert!(
            files.iter().any(|hit| {
                hit.canonical_name == r"C:\dev\ledgerline\src\main.rs"
                    && hit.confidence == confidence::TEXT_FILE
            }),
            "{files:?}"
        );
        assert_eq!(
            files[0].normalized_key,
            "c:/dev/ledgerline/src/main.rs".to_string()
        );
    }

    #[test]
    fn window_title_document_name_is_090() {
        let f = frame(Some("Q3-plan.docx - Word"), None, Some("WINWORD"), None);
        let documents = hits_of_kind(&f, EntityKind::Document);
        assert!(
            documents.iter().any(|hit| {
                hit.canonical_name == "Q3-plan.docx" && hit.confidence == confidence::TITLE_DOCUMENT
            }),
            "{documents:?}"
        );
    }

    #[test]
    fn ide_title_leading_file_and_workspace_segments() {
        let f = frame(
            Some("main.rs - ledgerline - Visual Studio Code"),
            None,
            Some("Code"),
            None,
        );
        let hits = extract_plain(&f);
        assert!(
            hits.iter().any(|hit| {
                hit.kind == EntityKind::File
                    && hit.canonical_name == "main.rs"
                    && hit.confidence == confidence::IDE_FILE
            }),
            "{hits:?}"
        );
        assert!(
            hits.iter().any(|hit| {
                hit.kind == EntityKind::Repo
                    && hit.canonical_name == "ledgerline"
                    && hit.confidence == confidence::IDE_WORKSPACE_REPO
            }),
            "{hits:?}"
        );
    }

    #[test]
    fn terminal_cwd_is_a_repo_anchor_at_080() {
        let f = frame(
            Some(r"C:\dev\ledgerline - PowerShell"),
            None,
            Some("WindowsTerminal"),
            None,
        );
        let repos = hits_of_kind(&f, EntityKind::Repo);
        assert!(
            repos.iter().any(|hit| {
                hit.canonical_name == r"C:\dev\ledgerline"
                    && hit.confidence == confidence::TERMINAL_DIRECTORY_REPO
            }),
            "{repos:?}"
        );
        assert_eq!(repos[0].normalized_key, "c:/dev/ledgerline");
    }

    #[test]
    fn terminal_recognition_input_gates_the_cwd_rule() {
        let mut f = frame(Some("C:\\dev\\ledgerline - Custom Shell"), None, None, None);
        f.session = Some(GraphSessionInput {
            session_id: 3,
            started_at: 0,
            ended_at: None,
            tool: Some("claude-code".to_string()),
            host: Some(SessionHost::Terminal),
        });
        let repos = hits_of_kind(&f, EntityKind::Repo);
        assert!(
            repos
                .iter()
                .any(|hit| hit.confidence == confidence::TERMINAL_DIRECTORY_REPO),
            "{repos:?}"
        );
    }

    #[test]
    fn ticket_keys_and_hash_refs_carry_the_table_confidences() {
        let f = frame(
            Some("Claude Code - ABC-123 ledgerline"),
            None,
            Some("WindowsTerminal"),
            Some("fixes #77 for ABC-123"),
        );
        let refs = hits_of_kind(&f, EntityKind::Ref);
        assert!(
            refs.iter().any(|hit| {
                hit.canonical_name == "ABC-123" && hit.confidence == confidence::TEXT_TICKET
            }),
            "{refs:?}"
        );
        assert!(
            refs.iter().any(|hit| {
                hit.canonical_name == "#77" && hit.confidence == confidence::TEXT_HASH_QUALIFIED
            }),
            "{refs:?}"
        );
    }

    #[test]
    fn confidence_table_is_pinned_to_the_p5_decision() {
        // The numbers are the P5 decision recorded in specs/05_BUILD_REVIEW.md. Pinned as
        // literals here so a changed constant fails a test instead of agreeing with itself.
        assert_eq!(confidence::URL_DOMAIN_SCHEME, 0.95);
        assert_eq!(confidence::URL_DOMAIN_HOST_ONLY, 0.90);
        assert_eq!(confidence::URL_DOCUMENT, 0.85);
        assert_eq!(confidence::URL_REF, 0.95);
        assert_eq!(confidence::TEXT_REPO, 0.95);
        assert_eq!(confidence::TEXT_FILE, 0.95);
        assert_eq!(confidence::TITLE_DOCUMENT, 0.90);
        assert_eq!(confidence::IDE_FILE, 0.90);
        assert_eq!(confidence::IDE_WORKSPACE_REPO, 0.80);
        assert_eq!(confidence::TERMINAL_DIRECTORY_REPO, 0.80);
        assert_eq!(confidence::TEXT_TICKET, 0.85);
        assert_eq!(confidence::TEXT_HASH_QUALIFIED, 0.85);
        assert_eq!(confidence::TEXT_HASH_BARE, 0.60);
        // The below-the-threshold property is asserted at a runtime value in
        // `bare_hash_ref_is_060_and_stays_below_the_clustering_threshold`.
    }

    #[test]
    fn bare_hash_ref_is_060_and_stays_below_the_clustering_threshold() {
        let f = frame(Some("check #42 later"), None, Some("notepad"), None);
        let refs = hits_of_kind(&f, EntityKind::Ref);
        assert_eq!(refs.len(), 1, "{refs:?}");
        assert_eq!(refs[0].canonical_name, "#42");
        assert_eq!(refs[0].confidence, confidence::TEXT_HASH_BARE);
        assert!(refs[0].confidence < clustering::SEED_CONFIDENCE_MIN);
    }

    #[test]
    fn url_path_ref_keeps_an_evidence_alias() {
        let f = frame(
            None,
            Some("https://github.com/acme/widgets/pull/42"),
            Some("chrome"),
            None,
        );
        let refs = hits_of_kind(&f, EntityKind::Ref);
        let hit = refs
            .iter()
            .find(|hit| hit.canonical_name == "#42")
            .expect("url path ref");
        assert_eq!(hit.aliases, vec!["/pull/42".to_string()]);
    }

    #[test]
    fn duplicate_hits_keep_the_highest_confidence_once() {
        let f = frame(
            Some(r"C:\dev\ledgerline\src\main.rs - ledgerline - Visual Studio Code"),
            None,
            Some("Code"),
            Some(r"edit C:\dev\ledgerline\src\main.rs again"),
        );
        let files: Vec<GraphHit> = hits_of_kind(&f, EntityKind::File)
            .into_iter()
            .filter(|hit| hit.normalized_key == "c:/dev/ledgerline/src/main.rs")
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].confidence, confidence::TEXT_FILE);
    }

    #[test]
    fn a_browser_title_is_never_read_as_an_ide_title() {
        let f = frame(
            Some("main.rs - ledgerline - Visual Studio Code"),
            Some("https://example.com/docs"),
            Some("chrome"),
            None,
        );
        let files = hits_of_kind(&f, EntityKind::File);
        assert!(files.is_empty(), "{files:?}");
    }

    #[test]
    fn normalized_key_follows_the_written_identity_rules() {
        assert_eq!(
            normalized_key(EntityKind::File, r" C:\Dev\Ledgerline\\src\main.rs "),
            "c:/dev/ledgerline/src/main.rs"
        );
        assert_eq!(
            normalized_key(
                EntityKind::Domain,
                "HTTPS://www.Example.COM:8443/Path?q=1#frag"
            ),
            "example.com:8443/path"
        );
        assert_eq!(
            normalized_key(EntityKind::Repo, "GitHub.com/Acme/Widgets.git"),
            "github.com/acme/widgets.git"
        );
        assert_eq!(normalized_key(EntityKind::Document, "Plan.PDF"), "plan.pdf");
        assert_eq!(normalized_key(EntityKind::Ref, " ABC-123 "), "abc-123");
        assert_eq!(normalized_key(EntityKind::Ref, "#123"), "#123");
    }

    #[test]
    fn extract_is_deterministic_and_sorted() {
        let f = frame(
            Some(r"C:\dev\ledgerline\src\main.rs - ledgerline - Visual Studio Code"),
            Some("https://github.com/acme/widgets/pull/42"),
            Some("Code"),
            Some("ticket ABC-123 and #9"),
        );
        let first = extract_plain(&f);
        let second = extract_plain(&f);
        assert_eq!(first, second);
        let mut sorted = first.clone();
        sorted.sort_by(|a, b| {
            (a.kind, a.normalized_key.as_str()).cmp(&(b.kind, b.normalized_key.as_str()))
        });
        assert_eq!(first, sorted);
    }

    #[test]
    fn an_empty_frame_extracts_nothing() {
        let f = frame(None, None, None, None);
        assert!(extract_plain(&f).is_empty());
    }

    #[test]
    fn recognition_input_gates_the_terminal_lane() {
        let mut f = frame(Some("C:\\dev\\ledgerline - Custom Shell"), None, None, None);
        // No app hint and a title shape the closed shell list does not know: no lane.
        assert!(extract_plain(&f)
            .iter()
            .all(|hit| hit.kind != EntityKind::Repo));
        let recognition = ToolRecognition {
            tool_id: "claude-code".to_string(),
            kind: SessionKind::Ai,
            host: Some(SessionHost::Terminal),
        };
        f.app_hint = Some("Custom".to_string());
        let with = extract(&ExtractionInput {
            frame: &f,
            recognition: Some(&recognition),
        });
        assert!(
            with.iter().any(|hit| {
                hit.kind == EntityKind::Repo
                    && hit.confidence == confidence::TERMINAL_DIRECTORY_REPO
            }),
            "{with:?}"
        );
    }
}
