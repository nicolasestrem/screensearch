//! Acceptance tests for the seeded fixture generator (`ssv2c-evalfixture gen`).
//!
//! Every fixture here is generated with `embeddings: false` into a `tempfile` directory, so
//! the tests never load or download a model and never touch a real profile. Generation runs
//! in plain `#[test]` functions because `gen::generate` owns its runtime; the read-back opens
//! a fresh `SqliteStore` on the finished file inside a separate runtime.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;

use evalfixture::gen::{generate, world_entities, GenManifest, GenOptions};
use evalfixture::{
    db_path, API_TOKEN_SETTING, DEFAULT_FRAMES_PER_WEEK, DEFAULT_SEED, DEFAULT_WEEKS,
};
use store::SqliteStore;
use traits::{
    EvidenceWindow, FrameContinuation, FrameEvidence, FrameFilters, FramePageRequest, Session,
    SessionFilter, FRAME_PAGE_MAX_LIMIT,
};

fn options(dir: &Path, seed: u64, weeks: u32, frames_per_week: usize) -> GenOptions {
    GenOptions {
        out_dir: dir.to_path_buf(),
        seed,
        weeks,
        frames_per_week,
        embeddings: false,
    }
}

fn default_options(dir: &Path, seed: u64) -> GenOptions {
    options(dir, seed, DEFAULT_WEEKS, DEFAULT_FRAMES_PER_WEEK)
}

/// What a fresh store reports about a finished fixture.
struct Readback {
    schema_version: i32,
    frames: Vec<FrameEvidence>,
    sessions: Vec<Session>,
    token: Option<String>,
}

fn read_back(db: &Path) -> Readback {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("read-back runtime");
    runtime.block_on(async {
        let store = SqliteStore::open_path(db).expect("the fixture opens as a v14 store");
        let schema_version = store.schema_version().expect("schema_version");
        let window = EvidenceWindow {
            from: 0,
            to: i64::MAX,
        };
        let mut frames = Vec::new();
        let mut continuation: Option<FrameContinuation> = None;
        loop {
            let page = store
                .frame_page(FramePageRequest {
                    window,
                    filters: FrameFilters::default(),
                    limit: FRAME_PAGE_MAX_LIMIT,
                    continuation: continuation.take(),
                })
                .await
                .expect("frame page");
            frames.extend(page.items);
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
        let sessions = store
            .sessions_in_range(SessionFilter::default())
            .await
            .expect("sessions");
        let token = store
            .get_setting(API_TOKEN_SETTING)
            .await
            .expect("settings read");
        Readback {
            schema_version,
            frames,
            sessions,
            token,
        }
    })
}

/// The eval harness's token trimming (`crates/harness/src/eval/refs.rs` `clean`).
fn clean(raw: &str) -> &str {
    raw.trim_matches(|c: char| {
        "([{<\"'`".contains(c) || ")]}>\"'`,;:!?.".contains(c) || c.is_whitespace()
    })
}

fn day_of(captured_at: i64) -> i64 {
    captured_at.div_euclid(86_400_000)
}

#[test]
fn the_default_fixture_is_reproducible_and_seed_sensitive() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();

    let a = generate(&default_options(first.path(), DEFAULT_SEED)).unwrap();
    let b = generate(&default_options(second.path(), DEFAULT_SEED)).unwrap();
    let c = generate(&default_options(
        other.path(),
        DEFAULT_SEED ^ 0x9e37_79b9_7f4a_7c15,
    ))
    .unwrap();

    assert_eq!(a.digest, b.digest, "same seed, same digest");
    assert_eq!((a.frames, a.sessions), (b.frames, b.sessions));
    assert_eq!(
        a.frames,
        DEFAULT_WEEKS as u64 * DEFAULT_FRAMES_PER_WEEK as u64,
        "every week holds exactly frames_per_week frames"
    );
    assert_ne!(a.digest, c.digest, "another seed, another digest");
    assert_eq!(a.digest.len(), 64);
    assert!(a.digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn the_default_fixture_reads_back_with_every_entity_in_content_text() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = generate(&default_options(dir.path(), DEFAULT_SEED)).unwrap();
    assert_eq!(manifest.db, db_path(dir.path()));
    assert_eq!(manifest.embeddings, 0);

    let back = read_back(&manifest.db);
    assert_eq!(back.schema_version, 14);
    assert_eq!(back.frames.len() as u64, manifest.frames);
    assert_eq!(back.sessions.len() as u64, manifest.sessions);

    let token = back.token.expect("api.token is written");
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));

    // Session ownership: every frame belongs to a session, every session owns two or more.
    let mut per_session: HashMap<i64, usize> = HashMap::new();
    for frame in &back.frames {
        let session = frame
            .session_id
            .unwrap_or_else(|| panic!("frame {} has no session", frame.frame_id));
        *per_session.entry(session).or_insert(0) += 1;
    }
    for session in &back.sessions {
        let owned = per_session.get(&session.id).copied().unwrap_or(0);
        assert!(owned >= 2, "session {} owns {owned} frames", session.id);
    }

    // Six to nine sessions on every working day (the fixture's working hours never cross
    // midnight UTC, so the UTC date is the working day).
    let mut sessions_per_day: BTreeMap<i64, usize> = BTreeMap::new();
    for session in &back.sessions {
        *sessions_per_day
            .entry(day_of(session.started_at))
            .or_insert(0) += 1;
    }
    assert_eq!(sessions_per_day.len(), DEFAULT_WEEKS as usize * 5);
    for (day, count) in &sessions_per_day {
        assert!((6..=9).contains(count), "day {day} holds {count} sessions");
    }

    // Every catalog entity survived the attention filter into some content_text.
    let contents: Vec<(&FrameEvidence, &str)> = back
        .frames
        .iter()
        .map(|f| (f, f.content_text.as_deref().unwrap_or("")))
        .collect();
    let tokens: Vec<BTreeSet<&str>> = contents
        .iter()
        .map(|(_, text)| text.split_whitespace().map(clean).collect())
        .collect();
    let world = world_entities();
    assert!(world.repositories.len() >= 10, "{:?}", world.repositories);
    assert!(world.paths.len() >= 40, "{}", world.paths.len());
    assert!(world.domains.len() >= 25, "{}", world.domains.len());
    assert!(world.tickets.len() >= 30, "{}", world.tickets.len());
    assert!(world.documents.len() >= 30, "{}", world.documents.len());
    assert!(world.tickets.iter().any(|t| t.starts_with('#')));
    assert!(world.tickets.iter().any(|t| !t.starts_with('#')));

    let mut missing: Vec<String> = Vec::new();
    let mut days_by_entity: BTreeMap<String, BTreeSet<i64>> = BTreeMap::new();
    let mut check = |entity: &str, found: &dyn Fn(usize) -> bool| {
        let days: BTreeSet<i64> = (0..contents.len())
            .filter(|i| found(*i))
            .map(|i| day_of(contents[i].0.captured_at))
            .collect();
        if days.is_empty() {
            missing.push(entity.to_string());
        }
        days_by_entity.insert(entity.to_string(), days);
    };
    for repo in &world.repositories {
        check(repo, &|i| contents[i].1.contains(repo.as_str()));
    }
    for path in &world.paths {
        check(path, &|i| contents[i].1.contains(path.as_str()));
    }
    for domain in &world.domains {
        let url = format!("https://{domain}");
        check(domain, &|i| contents[i].1.contains(url.as_str()));
    }
    for ticket in &world.tickets {
        check(ticket, &|i| tokens[i].contains(ticket.as_str()));
    }
    for document in &world.documents {
        check(document, &|i| tokens[i].contains(document.as_str()));
    }
    assert!(
        missing.is_empty(),
        "never reached content_text: {missing:?}"
    );
    let recurring = days_by_entity.values().filter(|d| d.len() >= 3).count();
    assert!(
        recurring > 0,
        "no entity appears in frames of three or more distinct days"
    );

    // Seen-again material: distinctive long lines shared by two to five frames.
    let mut line_frames: HashMap<&str, BTreeSet<i64>> = HashMap::new();
    for (frame, text) in &contents {
        for line in text.lines() {
            if line.chars().count() >= 24 && !line.contains('"') {
                line_frames.entry(line).or_default().insert(frame.frame_id);
            }
        }
    }
    let shared = line_frames
        .values()
        .filter(|frames| (2..=5).contains(&frames.len()))
        .count();
    assert!(
        shared >= 60,
        "only {shared} long lines recur in 2..=5 frames"
    );
}

#[test]
fn a_tiny_fixture_is_quick_and_reports_no_embeddings() {
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let manifest: GenManifest = generate(&options(dir.path(), DEFAULT_SEED, 1, 8)).unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs() < 5, "tiny fixture took {elapsed:?}");
    assert_eq!(manifest.frames, 8);
    assert_eq!(manifest.embeddings, 0);
    assert!(manifest.sessions >= 1);

    let summary = manifest.summary();
    let keys: Vec<&str> = summary
        .lines()
        .map(|line| line.split_once('=').map_or(line, |(k, _)| k))
        .collect();
    assert_eq!(
        keys,
        [
            "seed",
            "weeks",
            "frames",
            "sessions",
            "embeddings",
            "digest",
            "db"
        ]
    );
    assert!(summary.contains("embeddings=0\n"), "{summary}");
    assert!(summary.contains("seed=0x5cee55ee41520004\n"), "{summary}");
    assert!(
        !summary.contains('\\'),
        "summary paths use forward slashes: {summary}"
    );

    let back = read_back(&manifest.db);
    assert_eq!(back.frames.len(), 8);
    assert!(back.frames.iter().all(|f| f.session_id.is_some()));
}

#[test]
fn an_existing_fixture_is_never_appended_to() {
    let dir = tempfile::tempdir().unwrap();
    generate(&options(dir.path(), DEFAULT_SEED, 1, 8)).unwrap();
    let error = generate(&options(dir.path(), DEFAULT_SEED, 1, 8)).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("already exists"), "{message}");
    assert!(message.contains("--out"), "{message}");
}
