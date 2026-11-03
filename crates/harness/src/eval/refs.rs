//! The narrow reference entity extractor used as the independent ground-truth oracle for the
//! entity metric (`docs/kickoffs/P4.md`, `03 §7h.1`/`§7h.2`).
//!
//! **Independence is the point.** This is *not* `crates/graph`, it does not import it, and it
//! shares no code with it: the harness oracle must be able to disagree with the engine. It only
//! recognises forms that are unambiguous **by construction**:
//!
//! | kind (03 §7h.1) | form |
//! |---|---|
//! | `file` | an absolute Windows path with a drive letter (`C:\dir\file.ext`) |
//! | `domain` | a full URL with scheme and host (`https://host/path`) -> the host |
//! | `repo` | `<host>/<owner>/<repo>` from a URL or a bare token on a known code host |
//! | `document` | a bare file name ending in a document extension (`plan.docx`) |
//! | `ref` | `#123`, `ABC-123` |
//!
//! `app` is a valid `03 §7h.1` kind but is **not** extracted: nothing in captured text names an
//! application unambiguously (`app_hint` is a separate, already-lossy column). Saying so beats
//! guessing. SSH remotes (`git@host:owner/repo.git`) are also left out, because the extractor
//! refuses to invent normalization rules the spec does not state.

use serde::{Deserialize, Serialize};

/// The five `03 §7h.1` kinds this extractor can recognise (see the module docs for `app`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Repo,
    File,
    Domain,
    Document,
    Ref,
}

impl RefKind {
    /// The `03 §7h.1` / `entities.kind` token.
    pub fn as_str(self) -> &'static str {
        match self {
            RefKind::Repo => "repo",
            RefKind::File => "file",
            RefKind::Domain => "domain",
            RefKind::Document => "document",
            RefKind::Ref => "ref",
        }
    }
}

/// One reference entity found in captured text. `text` is what the question quotes; `normalized`
/// is the `03 §7h.2` dedup identity (lowercase, trimmed, path/URL-normalized).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Reference {
    pub kind: RefKind,
    pub text: String,
    pub normalized: String,
}

impl Reference {
    fn new(kind: RefKind, text: impl Into<String>, normalized: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            normalized: normalized.into(),
        }
    }
}

/// Document extensions the `document` rule accepts. A closed list keeps the oracle honest: an
/// unknown extension is simply not a reference.
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "md", "txt", "xlsx", "xls", "pptx", "ppt", "csv", "json", "yaml", "yml",
    "toml",
];

/// Code hosts whose `/<owner>/<repo>` path is a repository identity. Closed list, no guessing.
const CODE_HOSTS: &[&str] = &["github.com", "gitlab.com", "bitbucket.org", "codeberg.org"];

/// Extract every reference in one text blob (a window title, a URL, a content body). Duplicates
/// within the blob are collapsed.
pub fn extract_references(text: &str) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    for raw in text.split_whitespace() {
        let token = clean(raw);
        if token.is_empty() {
            continue;
        }
        out.extend(classify(token));
    }
    out.sort();
    out.dedup();
    out
}

/// Strip bracketing and sentence punctuation, so `("report.pdf").` and `see report.pdf.` both
/// yield `report.pdf`. A token that is only punctuation becomes empty and is skipped.
fn clean(raw: &str) -> &str {
    raw.trim_matches(|c: char| {
        "([{<\"'`".contains(c) || ")]}>\"'`,;:!?.".contains(c) || c.is_whitespace()
    })
}

fn classify(token: &str) -> Vec<Reference> {
    if token.is_empty() {
        return Vec::new();
    }
    if let Some(url) = from_url(token) {
        return url;
    }
    if is_windows_path(token) {
        return vec![Reference::new(
            RefKind::File,
            token.to_string(),
            normalize_path(token),
        )];
    }
    if let Some(repo) = bare_repo(token) {
        return vec![repo];
    }
    if let Some(doc) = document(token) {
        return vec![doc];
    }
    if let Some(r) = issue_ref(token) {
        return vec![r];
    }
    Vec::new()
}

/// `scheme://host[/path...]` -> a `domain` reference for the host, plus a `repo` reference when
/// the host is a code host and the path carries `/<owner>/<repo>`.
fn from_url(token: &str) -> Option<Vec<Reference>> {
    let rest = token
        .strip_prefix("https://")
        .or_else(|| token.strip_prefix("http://"))?;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // Drop user-info, then any port. A bracketed IPv6 host is not a reference we classify.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.starts_with('[') {
        return None;
    }
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return None;
    }

    let mut out = vec![Reference::new(RefKind::Domain, host.clone(), host.clone())];
    if CODE_HOSTS.contains(&host.as_str()) {
        let segments: Vec<&str> = path
            .trim_start_matches('/')
            .split('/')
            .take(2)
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() == 2 {
            let repo = segments[1].trim_end_matches(".git");
            if !repo.is_empty() {
                let text = format!("{}/{}/{}", host, segments[0], repo);
                out.push(Reference::new(
                    RefKind::Repo,
                    text.clone(),
                    text.to_lowercase(),
                ));
            }
        }
    }
    Some(out)
}

/// `C:\...` or `C:/...` — an absolute Windows path with a drive letter.
fn is_windows_path(token: &str) -> bool {
    let b = token.as_bytes();
    b.len() >= 4 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// `<code-host>/<owner>/<repo>` written bare (a git remote echo, a terminal cwd, a chat message).
fn bare_repo(token: &str) -> Option<Reference> {
    let mut parts = token.split('/');
    let host = parts.next()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
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
    // Deeper segments (`.../pull/42`, `.../tree/main`) are still the same repository identity.
    let text = format!("{}/{}/{}", host, owner, repo);
    Some(Reference::new(
        RefKind::Repo,
        text.clone(),
        text.to_lowercase(),
    ))
}

/// A bare file name ending in a known document extension (`plan.docx`, `report.pdf`).
fn document(token: &str) -> Option<Reference> {
    if token.contains('/') || token.contains('\\') {
        return None;
    }
    let (stem, ext) = token.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    let ext = ext.to_ascii_lowercase();
    if !DOC_EXTS.contains(&ext.as_str()) {
        return None;
    }
    Some(Reference::new(
        RefKind::Document,
        token.to_string(),
        token.to_ascii_lowercase(),
    ))
}

/// `#123` (an issue/PR) or `ABC-123` (a ticket). Digits are bounded so a hash-like run is not
/// mistaken for an id.
fn issue_ref(token: &str) -> Option<Reference> {
    if let Some(digits) = token.strip_prefix('#') {
        if is_digits(digits, 6) {
            return Some(Reference::new(RefKind::Ref, token, token));
        }
    }
    let (prefix, digits) = token.split_once('-')?;
    let prefix_ok = (2..=10).contains(&prefix.len())
        && prefix.chars().all(|c| c.is_ascii_uppercase())
        && prefix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase());
    if prefix_ok && is_digits(digits, 6) {
        return Some(Reference::new(
            RefKind::Ref,
            token,
            token.to_ascii_uppercase(),
        ));
    }
    None
}

fn is_digits(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
}

/// `03 §7h.2` normalized_key for a path: lowercase, `\` folded to `/`, duplicate separators and
/// a trailing separator removed.
fn normalize_path(path: &str) -> String {
    let lowered = path.to_ascii_lowercase().replace('\\', "/");
    let mut out = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        if ch == '/' && out.ends_with('/') {
            continue;
        }
        out.push(ch);
    }
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(RefKind, String)> {
        extract_references(text)
            .into_iter()
            .map(|r| (r.kind, r.normalized))
            .collect()
    }

    #[test]
    fn windows_path_is_a_file_and_normalizes_separators() {
        let refs = extract_references(
            "** C:\\Users\\nicol\\Desktop\\MKV\\Captation_Video\\docs\\PLAN.md - VS Code",
        );
        assert_eq!(refs.len(), 1, "{refs:?}");
        assert_eq!(refs[0].kind, RefKind::File);
        assert_eq!(
            refs[0].normalized,
            "c:/users/nicol/desktop/mkv/captation_video/docs/plan.md"
        );
    }

    #[test]
    fn url_yields_host_and_repository_identity() {
        let refs = extract_references("PR - https://github.com/acme/widgets/pull/42?tab=files");
        let got: Vec<(RefKind, String)> = refs
            .iter()
            .map(|r| (r.kind, r.normalized.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                (RefKind::Repo, "github.com/acme/widgets".to_string()),
                (RefKind::Domain, "github.com".to_string()),
            ]
        );
    }

    #[test]
    fn www_and_ports_and_userinfo_are_normalized_away() {
        assert_eq!(
            kinds("https://user:pw@www.example.com:8443/a/b"),
            vec![(RefKind::Domain, "example.com".to_string())]
        );
    }

    #[test]
    fn bare_repo_and_document_and_refs() {
        assert_eq!(
            kinds("cd into gitlab.com/team/service and open notes.md, then fix #123 for ABC-77"),
            vec![
                (RefKind::Repo, "gitlab.com/team/service".to_string()),
                (RefKind::Document, "notes.md".to_string()),
                (RefKind::Ref, "#123".to_string()),
                (RefKind::Ref, "ABC-77".to_string()),
            ]
        );
    }

    #[test]
    fn sentence_punctuation_is_not_part_of_the_reference() {
        let refs = extract_references("(see \"report.pdf\").");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].text, "report.pdf");
        assert_eq!(refs[0].kind, RefKind::Document);
    }

    #[test]
    fn relative_paths_and_unknown_extensions_are_refused() {
        assert!(
            extract_references("src/main.rs and archive.zip and http://localhost/x").is_empty()
        );
        assert!(extract_references("relative\\path\\file.txt").is_empty());
    }

    #[test]
    fn duplicates_collapse_and_ordering_is_stable() {
        let a = extract_references("notes.md notes.md");
        assert_eq!(a.len(), 1);
        let b =
            extract_references("https://github.com/acme/widgets https://github.com/acme/widgets");
        assert_eq!(b.len(), 2, "one domain + one repo, each once: {b:?}");
    }
}
