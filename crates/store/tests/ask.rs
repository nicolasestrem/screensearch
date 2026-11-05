//! Ask v2 store coverage (`03 §7i`): entity mentions, the stage-1/2/3 retrieval queries, the
//! conversation rows, the `session_vectors` lifecycle, and the `session_embed` producer's
//! queue gate.
//!
//! Every expectation is derived from the fixture the test builds (ids come from the inserts,
//! texts and windows from the same constants the test seeds), never from a stored literal.

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::{params, Connection};
use store::{SqliteStore, EMBEDDING_DIM};
use traits::{
    AskSessionHit, AskThreadCandidate, ChunkSource, Embedding, EmbeddingProvider, EntityKind,
    GraphHit, JobKind, NewAskTurn, NewFrame, NewJob, NewSession, OcrResult, SearchQuery,
    SessionHost, SessionKind, ThreadDraft, ThreadMemberDraft, ThreadPlan, TimeRange,
};

/// Deterministic query embedding: no model download or inference belongs in these tests.
struct FixedEmbedder(Embedding);

#[async_trait::async_trait]
impl EmbeddingProvider for FixedEmbedder {
    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }

    async fn embed_texts(&self, inputs: &[String]) -> traits::Result<Vec<Embedding>> {
        Ok(vec![self.0.clone(); inputs.len()])
    }
}

/// A 768-d vector with the two leading components set, so cosine distance between fixtures is
/// exact (`vec0` fixes the dimension and the metric in the schema).
fn unit_vector(first: f32, second: f32) -> Embedding {
    let mut vector = vec![0.0_f32; EMBEDDING_DIM];
    vector[0] = first;
    vector[1] = second;
    Embedding(vector)
}

fn frame(at: i64) -> NewFrame {
    NewFrame {
        captured_at: at,
        monitor_index: 0,
        width: 1920,
        height: 1080,
        image_path: format!("frames/{at}.webp"),
        content_hash: format!("hash-{at}"),
        app_hint: Some("WindowsTerminal".to_string()),
        window_title: Some("codex".to_string()),
        browser_url: None,
        capture_trigger: None,
    }
}

fn ai_session(started_at: i64, ended_at: Option<i64>) -> NewSession {
    NewSession {
        started_at,
        ended_at,
        kind: SessionKind::Ai,
        tool: Some("codex".to_string()),
        host: Some(SessionHost::Terminal),
        context_key: "ai:codex".to_string(),
        confidence: 0.9,
        frozen: false,
    }
}

fn focus_session(started_at: i64, ended_at: Option<i64>) -> NewSession {
    NewSession {
        started_at,
        ended_at,
        kind: SessionKind::Focus,
        // The baseline CHECK only allows a tool on an `ai` session.
        tool: None,
        host: None,
        context_key: "focus".to_string(),
        confidence: 0.5,
        frozen: false,
    }
}

async fn seed_frame(store: &SqliteStore, at: i64, text: &str) -> i64 {
    let frame_id = store.insert_frame(frame(at)).await.unwrap();
    store
        .insert_ocr(
            frame_id,
            OcrResult {
                text: text.to_string(),
                mean_confidence: 0.8,
                engine: "test".to_string(),
                spans: Vec::new(),
            },
        )
        .await
        .unwrap();
    frame_id
}

async fn seed_entity(store: &SqliteStore, session_id: i64, frame_id: i64, hit: GraphHit) -> i64 {
    store
        .graph_apply_hits(
            Some(session_id),
            frame_id,
            1_000,
            std::slice::from_ref(&hit),
        )
        .await
        .unwrap();
    let catalog = store.graph_entity_catalog(50).await.unwrap();
    catalog
        .iter()
        .find(|entity| entity.kind == hit.kind && entity.normalized_key == hit.normalized_key)
        .expect("the seeded entity is in the catalog")
        .id
}

/// Two threads with fixed closed intervals, used by both stage-1 tests.
struct ThreadFixture {
    thread_a: i64,
    thread_b: i64,
    session_a: i64,
    session_b: i64,
    session_c: i64,
}

async fn seed_threads(store: &SqliteStore) -> ThreadFixture {
    let session_a = store
        .insert_session(ai_session(100, Some(200)))
        .await
        .unwrap();
    let session_b = store
        .insert_session(ai_session(300, Some(400)))
        .await
        .unwrap();
    let session_c = store
        .insert_session(ai_session(500, Some(600)))
        .await
        .unwrap();
    store
        .graph_apply_thread_plan(&ThreadPlan {
            threads: vec![
                ThreadDraft {
                    cluster_key: "cluster-a".to_string(),
                    title: "Cluster A".to_string(),
                    first_seen_at: 100,
                    last_seen_at: 200,
                    sessions: vec![ThreadMemberDraft {
                        session_id: session_a,
                        confidence: 0.9,
                    }],
                },
                ThreadDraft {
                    cluster_key: "cluster-b".to_string(),
                    title: "Cluster B".to_string(),
                    first_seen_at: 300,
                    last_seen_at: 600,
                    // Seeded out of order to prove the member list is returned ascending.
                    sessions: vec![
                        ThreadMemberDraft {
                            session_id: session_c,
                            confidence: 0.8,
                        },
                        ThreadMemberDraft {
                            session_id: session_b,
                            confidence: 0.9,
                        },
                    ],
                },
            ],
            ..ThreadPlan::default()
        })
        .await
        .unwrap();
    let threads = store.graph_stored_threads().await.unwrap();
    let thread_a = threads
        .iter()
        .find(|thread| thread.cluster_key == "cluster-a")
        .expect("cluster-a persisted")
        .id;
    let thread_b = threads
        .iter()
        .find(|thread| thread.cluster_key == "cluster-b")
        .expect("cluster-b persisted")
        .id;
    ThreadFixture {
        thread_a,
        thread_b,
        session_a,
        session_b,
        session_c,
    }
}

#[tokio::test]
async fn ask_entity_resolution_is_empty_without_a_graph_and_matches_seeded_aliases() {
    let store = SqliteStore::open_in_memory().unwrap();

    // No graph rows at all: the mention is simply absent, never an error (`03 §7i.1`).
    assert!(store
        .ask_resolve_entities("deploy the screensearch repo")
        .await
        .unwrap()
        .is_empty());

    let session_id = store
        .insert_session(ai_session(100, Some(200)))
        .await
        .unwrap();
    let frame_id = seed_frame(&store, 120, "work on the repo").await;
    let repo_id = seed_entity(
        &store,
        session_id,
        frame_id,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "screensearch".to_string(),
            normalized_key: "screensearch".to_string(),
            confidence: 0.9,
            aliases: vec!["screen search".to_string()],
        },
    )
    .await;
    let app_id = seed_entity(
        &store,
        session_id,
        frame_id,
        GraphHit {
            kind: EntityKind::App,
            canonical_name: "screen".to_string(),
            normalized_key: "app:screen".to_string(),
            confidence: 0.8,
            aliases: Vec::new(),
        },
    )
    .await;

    let matches = store
        .ask_resolve_entities("How did SCREEN SEARCH evolve inside screensearch?")
        .await
        .unwrap();
    // Both entities resolve; each appears once even though both its canonical name and its
    // alias are present in the question.
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches
            .iter()
            .map(|m| m.entity_id)
            .collect::<HashSet<_>>()
            .len(),
        2,
        "one row per entity"
    );
    // Longest match first: the repo's 13-char alias beats its own 12-char canonical name and
    // the app's 6-char name.
    assert_eq!(matches[0].entity_id, repo_id);
    assert_eq!(matches[0].matched, "screen search");
    assert_eq!(matches[1].entity_id, app_id);
    assert_eq!(matches[1].matched, "screen");

    // A question naming nothing returns nothing, even with a populated graph.
    assert!(store
        .ask_resolve_entities("no known entity is mentioned here")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn ask_candidate_threads_use_closed_window_overlap_and_entity_sessions() {
    let store = SqliteStore::open_in_memory().unwrap();
    let fixture = seed_threads(&store).await;

    // `[201, 350)`: thread A ends at 200 (outside), thread B starts at 300 (inside).
    let candidates = store
        .ask_candidate_threads(
            Some(TimeRange {
                start: 201,
                end: 350,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        candidates,
        vec![AskThreadCandidate {
            thread_id: fixture.thread_b,
            session_ids: vec![fixture.session_b, fixture.session_c],
        }],
        "members are ascending even though they were seeded out of order"
    );

    // Closed interval: a thread whose `last_seen_at` equals the window start still overlaps.
    let touching_start = store
        .ask_candidate_threads(
            Some(TimeRange {
                start: 200,
                end: 260,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        touching_start,
        vec![AskThreadCandidate {
            thread_id: fixture.thread_a,
            session_ids: vec![fixture.session_a],
        }]
    );

    // Closed interval: a thread whose `first_seen_at` equals the window end still overlaps
    // (thread A is out because it ends before this window starts).
    let touching_end = store
        .ask_candidate_threads(
            Some(TimeRange {
                start: 201,
                end: 300,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        touching_end,
        vec![AskThreadCandidate {
            thread_id: fixture.thread_b,
            session_ids: vec![fixture.session_b, fixture.session_c],
        }]
    );

    // An entity resolved on session A's frame pulls that thread in even when the window
    // misses it: the store joins the entity id to its sessions through `session_entities`.
    let frame_id = seed_frame(&store, 150, "work on the screensearch repo").await;
    let repo_id = seed_entity(
        &store,
        fixture.session_a,
        frame_id,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "screensearch".to_string(),
            normalized_key: "screensearch".to_string(),
            confidence: 0.9,
            aliases: Vec::new(),
        },
    )
    .await;
    let by_entity = store
        .ask_candidate_threads(
            Some(TimeRange {
                start: 201,
                end: 260,
            }),
            &[repo_id],
        )
        .await
        .unwrap();
    assert_eq!(
        by_entity,
        vec![AskThreadCandidate {
            thread_id: fixture.thread_a,
            session_ids: vec![fixture.session_a],
        }]
    );

    // An entity id the graph does not have narrows nothing and errors nothing (`§7i.1`: a
    // missing graph row falls back instead of blocking).
    let unknown_entity = store
        .ask_candidate_threads(
            Some(TimeRange {
                start: 201,
                end: 350,
            }),
            &[repo_id + 9_999],
        )
        .await
        .unwrap();
    assert_eq!(
        unknown_entity,
        vec![AskThreadCandidate {
            thread_id: fixture.thread_b,
            session_ids: vec![fixture.session_b, fixture.session_c],
        }]
    );

    // No window: the whole history, every thread, ascending by id.
    let all = store.ask_candidate_threads(None, &[]).await.unwrap();
    let mut expected = vec![fixture.thread_a, fixture.thread_b];
    expected.sort_unstable();
    assert_eq!(
        all.iter()
            .map(|candidate| candidate.thread_id)
            .collect::<Vec<_>>(),
        expected
    );
}

#[tokio::test]
async fn ask_candidate_sessions_narrow_threads_by_window_and_entity_scope() {
    let store = SqliteStore::open_in_memory().unwrap();
    let fixture = seed_threads(&store).await;
    let thread_b = vec![AskThreadCandidate {
        thread_id: fixture.thread_b,
        session_ids: vec![fixture.session_b, fixture.session_c],
    }];

    // Window covering only session B (300..400).
    let narrowed = store
        .ask_candidate_sessions(
            &thread_b,
            Some(TimeRange {
                start: 250,
                end: 450,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(narrowed, vec![fixture.session_b]);

    // The same window with an entity scope: the intersection is empty (the entity is linked to
    // a session the window excludes).
    let frame_c = seed_frame(&store, 550, "closed the screensearch issue").await;
    let repo_id = seed_entity(
        &store,
        fixture.session_c,
        frame_c,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "screensearch".to_string(),
            normalized_key: "screensearch".to_string(),
            confidence: 0.9,
            aliases: Vec::new(),
        },
    )
    .await;
    let empty = store
        .ask_candidate_sessions(
            &thread_b,
            Some(TimeRange {
                start: 250,
                end: 450,
            }),
            &[repo_id],
        )
        .await
        .unwrap();
    assert!(empty.is_empty());

    // A window covering both members, narrowed by the entity scope.
    let entity_narrowed = store
        .ask_candidate_sessions(
            &thread_b,
            Some(TimeRange {
                start: 250,
                end: 700,
            }),
            &[repo_id],
        )
        .await
        .unwrap();
    assert_eq!(entity_narrowed, vec![fixture.session_c]);

    // No thread candidates: the window's own sessions are the set (the documented fallback).
    let window_only = store
        .ask_candidate_sessions(
            &[],
            Some(TimeRange {
                start: 250,
                end: 700,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(window_only, vec![fixture.session_b, fixture.session_c]);

    // No thread candidates and no window: every session, ascending.
    let all = store.ask_candidate_sessions(&[], None, &[]).await.unwrap();
    assert_eq!(
        all,
        vec![fixture.session_a, fixture.session_b, fixture.session_c]
    );

    // An open session (`ended_at` NULL) overlaps a past window: the overlap rule treats it as
    // continuing to the present.
    let open = store.insert_session(ai_session(800, None)).await.unwrap();
    let with_open = store
        .ask_candidate_sessions(
            &[],
            Some(TimeRange {
                start: 750,
                end: 900,
            }),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(with_open, vec![open]);
}

#[tokio::test]
async fn ask_search_sessions_fuses_the_text_and_vector_arms() {
    let query = unit_vector(1.0, 0.0);
    let store = SqliteStore::open_in_memory()
        .unwrap()
        .with_embedder(Arc::new(FixedEmbedder(query.clone())));

    let both = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    let text_only = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    let vector_only = store
        .insert_session(ai_session(5_000, Some(6_000)))
        .await
        .unwrap();
    let neither = store
        .insert_session(ai_session(7_000, Some(8_000)))
        .await
        .unwrap();

    let both_text = "Deploy runbook\nkubernetes rollout";
    let text_only_text = "Deploy retrospective\nwhat went wrong";
    let vector_only_text = "Grocery list\nmilk and bread";
    let neither_text = "Quarterly taxes\nfiling notes";
    for (session_id, text) in [
        (both, both_text),
        (text_only, text_only_text),
        (vector_only, vector_only_text),
        (neither, neither_text),
    ] {
        let (title, summary) = text.split_once('\n').unwrap();
        store
            .set_session_title_summary(session_id, title, summary, "test")
            .await
            .unwrap();
    }

    // `both` is the exact query direction; `vector_only` is a 0.9-cosine neighbour. The other two
    // have no vector at all.
    assert!(store
        .upsert_session_vector(both, both_text, &query)
        .await
        .unwrap());
    let neighbour = unit_vector(0.9, 0.435_889_9);
    assert!(store
        .upsert_session_vector(vector_only, vector_only_text, &neighbour)
        .await
        .unwrap());

    let candidates = vec![both, text_only, vector_only, neither];
    let hits = store
        .ask_search_sessions("deploy", &candidates, 10)
        .await
        .unwrap();

    // Only the sessions an arm matched come back; a session with neither text nor vector is out.
    let hit_ids: Vec<i64> = hits.iter().map(|hit| hit.session_id).collect();
    assert_eq!(hit_ids.len(), 3);
    assert!(
        hit_ids.contains(&text_only),
        "the FTS-matching session is retrieved"
    );
    assert!(
        hit_ids.contains(&vector_only),
        "the vector-nearest session is retrieved"
    );
    assert!(
        !hit_ids.contains(&neither),
        "a session with no arm hit is excluded"
    );

    // Two arms are worth strictly more than one, whatever the per-arm order: the session matched
    // by both arms wins the fusion.
    assert_eq!(hit_ids[0], both);
    assert!(
        hits[0].score > hits[1].score,
        "the both-arms session outranks every single-arm session"
    );

    // `limit` truncates after fusion, best first.
    let limited: Vec<AskSessionHit> = store
        .ask_search_sessions("deploy", &candidates, 1)
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].session_id, both);

    // Nothing to search: no candidates means no hits, and no hits means no error.
    assert!(store
        .ask_search_sessions("deploy", &[], 10)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn ask_search_sessions_degrades_to_fts_without_an_embedder() {
    let store = SqliteStore::open_in_memory().unwrap();
    let matching = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    let unrelated = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(matching, "Deploy runbook", "kubernetes rollout", "test")
        .await
        .unwrap();
    store
        .set_session_title_summary(unrelated, "Grocery list", "milk and bread", "test")
        .await
        .unwrap();

    let hits = store
        .ask_search_sessions("deploy", &[matching, unrelated], 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, matching);
}

#[tokio::test]
async fn ask_search_frames_restricts_the_hybrid_arms_to_the_given_sessions() {
    let store = SqliteStore::open_in_memory().unwrap();
    let first = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    let second = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    let first_a = seed_frame(&store, 1_100, "deploy the release").await;
    let first_b = seed_frame(&store, 1_200, "deploy again").await;
    let second_a = seed_frame(&store, 3_100, "deploy in another session").await;
    store
        .assign_frames_session(&[first_a, first_b], Some(first))
        .await
        .unwrap();
    store
        .assign_frames_session(&[second_a], Some(second))
        .await
        .unwrap();
    let loose = seed_frame(&store, 5_000, "deploy without a session").await;

    let query = SearchQuery {
        text: "deploy".to_string(),
        limit: 10,
        time_range: None,
        include_chrome: false,
    };

    let scoped: HashSet<i64> = store
        .ask_search_frames(&query, &[first])
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.frame_id)
        .collect();
    assert_eq!(scoped, HashSet::from([first_a, first_b]));

    // The session predicate and the query window narrow together.
    let windowed: HashSet<i64> = store
        .ask_search_frames(
            &SearchQuery {
                time_range: Some(TimeRange {
                    start: 1_150,
                    end: 2_000,
                }),
                ..query.clone()
            },
            &[first],
        )
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.frame_id)
        .collect();
    assert_eq!(windowed, HashSet::from([first_b]));

    // No session winner: the fallback is the existing hybrid search over the query's window, so
    // every session's frames (and the sessionless one) are reachable.
    let fallback: HashSet<i64> = store
        .ask_search_frames(&query, &[])
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.frame_id)
        .collect();
    assert_eq!(fallback, HashSet::from([first_a, first_b, second_a, loose]));

    let sessions = store
        .ask_frame_sessions(&[first_a, first_b, second_a, loose])
        .await
        .unwrap();
    assert_eq!(sessions.get(&first_a), Some(&first));
    assert_eq!(sessions.get(&first_b), Some(&first));
    assert_eq!(sessions.get(&second_a), Some(&second));
    assert_eq!(
        sessions.get(&loose),
        None,
        "a frame outside any session is absent from the map"
    );
}

#[tokio::test]
async fn ask_search_frames_vector_arm_respects_the_session_set() {
    let query_direction = unit_vector(1.0, 0.0);
    let store = SqliteStore::open_in_memory()
        .unwrap()
        .with_embedder(Arc::new(FixedEmbedder(query_direction.clone())));
    let near = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    let far = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    let near_frame = store.insert_frame(frame(1_100)).await.unwrap();
    let far_frame = store.insert_frame(frame(3_100)).await.unwrap();
    store
        .assign_frames_session(&[near_frame], Some(near))
        .await
        .unwrap();
    store
        .assign_frames_session(&[far_frame], Some(far))
        .await
        .unwrap();
    // The nearest vector belongs to the session the query does *not* scope to, so only the Rust
    // post-filter (and the escalation that digs past the top ranks) can produce the scoped hit.
    store
        .upsert_text_embedding(
            near_frame,
            0,
            "nearest text",
            ChunkSource::Ocr,
            &query_direction,
            "test-model",
        )
        .await
        .unwrap();
    let neighbour = unit_vector(0.5, 0.866_025_4);
    store
        .upsert_text_embedding(
            far_frame,
            0,
            "neighbour text",
            ChunkSource::Ocr,
            &neighbour,
            "test-model",
        )
        .await
        .unwrap();

    // The text matches no frame, so the FTS arm is empty and only the vector arm can hit.
    let query = SearchQuery {
        text: "zzz".to_string(),
        limit: 10,
        time_range: None,
        include_chrome: false,
    };
    let scoped: Vec<i64> = store
        .ask_search_frames(&query, &[far])
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.frame_id)
        .collect();
    assert_eq!(scoped, vec![far_frame]);

    let unrestricted: HashSet<i64> = store
        .ask_search_frames(&query, &[])
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.frame_id)
        .collect();
    assert_eq!(unrestricted, HashSet::from([near_frame, far_frame]));
}

#[tokio::test]
async fn ask_session_labels_prefer_title_then_tool_then_kind() {
    let store = SqliteStore::open_in_memory().unwrap();
    let titled = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(titled, "  Deploy plan  ", "summary", "test")
        .await
        .unwrap();
    let tooled = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    let kinded = store
        .insert_session(focus_session(5_000, Some(6_000)))
        .await
        .unwrap();
    let long_titled = store
        .insert_session(ai_session(7_000, Some(8_000)))
        .await
        .unwrap();
    let long_title = "s".repeat(120);
    store
        .set_session_title_summary(long_titled, &long_title, "summary", "test")
        .await
        .unwrap();

    let labels = store
        .ask_session_labels(&[titled, tooled, kinded, long_titled, 99_999])
        .await
        .unwrap();
    // Derived from what was seeded: the trimmed title, the tool of an untitled ai session, the
    // kind of a session that carries neither.
    assert_eq!(labels.get(&titled).map(String::as_str), Some("Deploy plan"));
    assert_eq!(labels.get(&tooled).map(String::as_str), Some("codex"));
    assert_eq!(labels.get(&kinded).map(String::as_str), Some("focus"));
    // The documented prompt budget (60 chars) truncates a long title instead of carrying it.
    let capped: String = long_title.chars().take(60).collect();
    assert_eq!(labels.get(&long_titled), Some(&capped));
    assert_eq!(labels.get(&99_999), None);
    assert!(labels.values().all(|label| label.chars().count() <= 60));
}

#[tokio::test]
async fn ask_turn_ordinals_cascade_and_the_unique_guard_holds() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-turns.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");

    let thread = store.create_ask_thread().await.unwrap();
    assert!(store.ask_thread_exists(thread).await.unwrap());
    assert!(!store.ask_thread_exists(thread + 1).await.unwrap());

    let window = TimeRange {
        start: 1_000,
        end: 2_000,
    };
    let first = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "what did I do last week?".to_string(),
            window: Some(window),
            resolved_entities: vec![3, 5],
        })
        .await
        .unwrap();
    let second = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "and the day before?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!((first.ordinal, second.ordinal), (1, 2));
    assert_eq!(first.answer, None, "the answer stays NULL while streaming");

    assert!(store.finish_ask_turn(first.id, "answer one").await.unwrap());
    assert!(!store.finish_ask_turn(999_999, "ghost").await.unwrap());

    // `UNIQUE(thread_id, ordinal)` is the durable guard: the store always picks the next ordinal,
    // so a second writer is the only way to observe the constraint. Checked through a second
    // connection on the same file.
    let conn = Connection::open(&db_path).expect("second connection");
    let duplicate = conn.execute(
        "INSERT INTO ask_turns (thread_id, ordinal, question) VALUES (?1, ?2, ?3)",
        params![thread, second.ordinal, "duplicate ordinal"],
    );
    assert!(duplicate.is_err(), "a reused ordinal must be refused");
    // The window is stored as both columns, and the entities as a JSON array.
    let (window_start, window_end, entities): (i64, i64, String) = conn
        .query_row(
            "SELECT window_start, window_end, resolved_entities FROM ask_turns WHERE id = ?1",
            [first.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((window_start, window_end), (window.start, window.end));
    assert_eq!(entities, "[3,5]");

    let turns = store.list_ask_turns(thread).await.unwrap();
    assert_eq!(
        turns.iter().map(|turn| turn.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(turns[0].answer.as_deref(), Some("answer one"));

    assert!(store.delete_ask_thread(thread).await.unwrap());
    assert!(store.list_ask_turns(thread).await.unwrap().is_empty());
    assert!(!store.delete_ask_thread(thread).await.unwrap());
    let remaining: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ask_turns WHERE thread_id = ?1",
            [thread],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0, "the turns cascade with their conversation");
    // A turn for a conversation that no longer exists is refused, never resurrected.
    assert!(store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "orphan".to_string(),
            window: None,
            resolved_entities: Vec::new(),
        })
        .await
        .is_err());
}

#[tokio::test]
async fn ask_follow_up_reads_back_the_window_and_entity_ids() {
    let store = SqliteStore::open_in_memory().unwrap();
    let session_id = store
        .insert_session(ai_session(100, Some(200)))
        .await
        .unwrap();
    let frame_id = seed_frame(&store, 120, "work on the repo").await;
    let repo_id = seed_entity(
        &store,
        session_id,
        frame_id,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "screensearch".to_string(),
            normalized_key: "screensearch".to_string(),
            confidence: 0.9,
            aliases: Vec::new(),
        },
    )
    .await;
    let app_id = seed_entity(
        &store,
        session_id,
        frame_id,
        GraphHit {
            kind: EntityKind::App,
            canonical_name: "codex".to_string(),
            normalized_key: "app:codex".to_string(),
            confidence: 0.7,
            aliases: Vec::new(),
        },
    )
    .await;

    let thread = store.create_ask_thread().await.unwrap();
    let window = TimeRange {
        start: 10_000,
        end: 20_000,
    };
    let resolved = vec![repo_id, app_id];
    let first = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "what shipped last week?".to_string(),
            window: Some(window),
            resolved_entities: resolved.clone(),
        })
        .await
        .unwrap();
    assert_eq!(first.window, Some(window));

    // The follow-up's source: the previous turn's window and entities come back verbatim.
    let last = store
        .last_ask_turn(thread)
        .await
        .unwrap()
        .expect("a turn exists");
    assert_eq!(last.id, first.id);
    assert_eq!(last.window, Some(window));
    assert_eq!(last.resolved_entities, resolved);

    // A turn with no time intent stores neither column and reads back as no window.
    let bare = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "and then?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(bare.window, None);
    let last = store
        .last_ask_turn(thread)
        .await
        .unwrap()
        .expect("a turn exists");
    assert_eq!(last.id, bare.id);
    assert_eq!(last.window, None);
    assert!(last.resolved_entities.is_empty());

    // An empty conversation has no last turn.
    let empty = store.create_ask_thread().await.unwrap();
    assert_eq!(store.last_ask_turn(empty).await.unwrap(), None);
}

#[tokio::test]
async fn ask_session_vectors_row_is_deleted_with_its_session_and_kept_for_a_frozen_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-vectors.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");

    let live = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(live, "Deploy runbook", "kubernetes rollout", "test")
        .await
        .unwrap();
    let frozen_session = NewSession {
        frozen: true,
        ..ai_session(3_000, Some(4_000))
    };
    let frozen = store.insert_session(frozen_session).await.unwrap();
    store
        .set_session_title_summary(frozen, "Frozen review", "notes", "test")
        .await
        .unwrap();

    let embedding = unit_vector(1.0, 0.0);
    assert!(store
        .upsert_session_vector(live, "Deploy runbook\nkubernetes rollout", &embedding)
        .await
        .unwrap());
    assert!(store
        .upsert_session_vector(frozen, "Frozen review\nnotes", &embedding)
        .await
        .unwrap());

    let conn = Connection::open(&db_path).expect("second connection");
    let vector_rows = |session_id: i64| -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM session_vectors WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(vector_rows(live), 1);
    assert_eq!(vector_rows(frozen), 1);

    assert!(store.delete_unfrozen_session(live).await.unwrap());
    assert_eq!(
        vector_rows(live),
        0,
        "vec0 has no FK: the session delete removes the vector in the same transaction"
    );

    assert!(!store.delete_unfrozen_session(frozen).await.unwrap());
    assert_eq!(
        vector_rows(frozen),
        1,
        "a frozen session is not deleted, so its vector must survive"
    );
}

#[tokio::test]
async fn ask_session_embed_candidates_skip_empty_sessions_and_gate_the_upsert() {
    let store = SqliteStore::open_in_memory().unwrap();
    let embedding = unit_vector(1.0, 0.0);

    let titled = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(titled, "Deploy runbook", "kubernetes rollout", "test")
        .await
        .unwrap();
    let empty = store
        .insert_session(ai_session(3_000, Some(4_000)))
        .await
        .unwrap();
    let summary_only = store
        .insert_session(ai_session(5_000, Some(6_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(summary_only, "Quarterly review", "tax notes", "test")
        .await
        .unwrap();

    let candidates = store.session_embed_candidates(10).await.unwrap();
    let candidate_ids: Vec<i64> = candidates
        .iter()
        .map(|candidate| candidate.session_id)
        .collect();
    assert!(
        !candidate_ids.contains(&empty),
        "a session with both title and summary NULL is never a candidate"
    );
    assert_eq!(candidate_ids.len(), 2);
    assert_eq!(
        store.session_embed_pending().await.unwrap(),
        candidate_ids.len() as u64
    );

    // The candidate order is `updated_at DESC, id DESC`; derive it from the rows the fixture
    // just wrote instead of guessing the millisecond stamps.
    let mut expected = Vec::new();
    for session_id in [titled, summary_only] {
        let session = store.get_session(session_id).await.unwrap().unwrap();
        expected.push((session.updated_at, session.id));
    }
    expected.sort_by(|left, right| right.cmp(left));
    assert_eq!(
        candidate_ids,
        expected.iter().map(|(_, id)| *id).collect::<Vec<_>>()
    );

    let text_of = |session_id: i64| -> String {
        candidates
            .iter()
            .find(|candidate| candidate.session_id == session_id)
            .expect("a candidate for the seeded session")
            .text
            .clone()
    };
    assert_eq!(text_of(titled), "Deploy runbook\nkubernetes rollout");

    // An unchanged text writes, and a replay is idempotent.
    let titled_text = text_of(titled);
    assert!(store
        .upsert_session_vector(titled, &titled_text, &embedding)
        .await
        .unwrap());
    assert!(store
        .upsert_session_vector(titled, &titled_text, &embedding)
        .await
        .unwrap());
    assert_eq!(
        store.session_embed_pending().await.unwrap(),
        candidate_ids.len() as u64 - 1
    );
    assert!(!store
        .session_embed_candidates(10)
        .await
        .unwrap()
        .iter()
        .any(|candidate| candidate.session_id == titled));

    // A summary edited after the embedding invalidates the queued text: the stale write is
    // skipped, and the re-derived text is what lands.
    let edited_text = text_of(summary_only);
    store
        .set_session_title_summary(summary_only, "Changed title", "changed summary", "test")
        .await
        .unwrap();
    assert!(
        !store
            .upsert_session_vector(summary_only, &edited_text, &embedding)
            .await
            .unwrap(),
        "a text the session no longer produces must not be written"
    );
    let refreshed = store.session_embed_candidates(10).await.unwrap();
    let changed = refreshed
        .iter()
        .find(|candidate| candidate.session_id == summary_only)
        .expect("the edited session is pending again");
    assert_eq!(changed.text, "Changed title\nchanged summary");
    assert!(store
        .upsert_session_vector(summary_only, &changed.text, &embedding)
        .await
        .unwrap());
    assert_eq!(store.session_embed_pending().await.unwrap(), 0);

    // An empty concatenation is never embedded, and a wrong dimension is refused.
    assert!(!store
        .upsert_session_vector(empty, "", &embedding)
        .await
        .unwrap());
    assert!(store
        .upsert_session_vector(titled, "anything", &Embedding(vec![0.0; 8]))
        .await
        .is_err());
}

#[tokio::test]
async fn ask_pending_jobs_of_kind_counts_pending_and_running_session_embed_jobs() {
    let store = SqliteStore::open_in_memory().unwrap();
    let job = |kind: JobKind| NewJob {
        kind,
        frame_id: None,
        priority: 0,
        max_attempts: 3,
        not_before: 0,
    };
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::SessionEmbed)
            .await
            .unwrap(),
        0
    );

    store.enqueue_job(job(JobKind::SessionEmbed)).await.unwrap();
    store.enqueue_job(job(JobKind::VisionTag)).await.unwrap();
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::SessionEmbed)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::VisionTag)
            .await
            .unwrap(),
        1
    );

    // A claimed job is still in flight, so it still counts.
    let claimed = store
        .claim_jobs(&[JobKind::SessionEmbed], 1, 1_000_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::SessionEmbed)
            .await
            .unwrap(),
        1
    );

    // Completing it drains the count for that kind only.
    store.complete_job(claimed[0].id).await.unwrap();
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::SessionEmbed)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .pending_jobs_of_kind(JobKind::VisionTag)
            .await
            .unwrap(),
        1
    );
}
