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
    AskCitations, AskSessionHit, AskThreadCandidate, ChunkSource, Embedding, EmbeddingProvider,
    EntityKind, EvidenceKind, GraphHit, InsightEvidence, InsightGenerator, InsightPayload, JobKind,
    NewAskTurn, NewFrame, NewInsight, NewJob, NewSession, OcrResult, RecurrencePayload,
    SearchQuery, SessionHost, SessionKind, ThreadDraft, ThreadMemberDraft, ThreadPlan, TimeRange,
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
async fn ask_entity_resolution_requires_whole_mentions_not_substrings() {
    let store = SqliteStore::open_in_memory().unwrap();
    let session = store
        .insert_session(ai_session(100, Some(200)))
        .await
        .unwrap();
    let frame = seed_frame(&store, 120, "workspace api notes").await;
    let api_id = seed_entity(
        &store,
        session,
        frame,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "api".to_string(),
            normalized_key: "repo:api".to_string(),
            confidence: 0.9,
            aliases: vec!["api gateway".to_string()],
        },
    )
    .await;

    // A substring hit is not a mention: `api` sits inside "capital", and resolving it would
    // scope retrieval (and persist that scope on the turn) to an unrelated workspace.
    assert!(
        store
            .ask_resolve_entities("What was the capital expenditure?")
            .await
            .unwrap()
            .is_empty(),
        "a short name must not resolve out of a longer word"
    );

    // A whole mention resolves, in any case…
    let direct = store
        .ask_resolve_entities("where are the API docs?")
        .await
        .unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].entity_id, api_id);
    assert_eq!(direct[0].matched, "api");

    // …and the same boundary rule applies to aliases.
    let via_alias = store
        .ask_resolve_entities("debug the API GATEWAY today")
        .await
        .unwrap();
    assert_eq!(via_alias.len(), 1);
    assert_eq!(via_alias[0].entity_id, api_id);
    assert_eq!(via_alias[0].matched, "api gateway");

    // Punctuation and string ends are boundaries too.
    assert_eq!(
        store.ask_resolve_entities("api?").await.unwrap().len(),
        1,
        "trailing punctuation is a boundary"
    );
    assert_eq!(
        store.ask_resolve_entities("api").await.unwrap().len(),
        1,
        "the question itself may be the mention"
    );
    assert!(
        store
            .ask_resolve_entities("rapid response")
            .await
            .unwrap()
            .is_empty(),
        "`api` inside `rapid` is not a mention"
    );
}

#[tokio::test]
async fn ask_entity_resolution_is_unicode_case_insensitive() {
    let store = SqliteStore::open_in_memory().unwrap();
    let session = store
        .insert_session(ai_session(100, Some(200)))
        .await
        .unwrap();
    let frame = seed_frame(&store, 120, "Équipe planning").await;
    let equipe = seed_entity(
        &store,
        session,
        frame,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "Équipe".to_string(),
            normalized_key: "repo:equipe".to_string(),
            confidence: 0.9,
            aliases: vec!["équipe interne".to_string()],
        },
    )
    .await;

    // SQLite's `lower()` folds ASCII only, so a SQL prefilter would drop these before the
    // boundary check ever ran. Both sides are lowered in Rust instead.
    for question in ["équipe planning", "ÉQUIPE planning", "the Équipe repo"] {
        let matches = store.ask_resolve_entities(question).await.unwrap();
        assert_eq!(
            matches.len(),
            1,
            "`{question}` should resolve the accented canonical name"
        );
        assert_eq!(matches[0].entity_id, equipe);
    }

    // The alias form is folded the same way.
    let via_alias = store
        .ask_resolve_entities("revue ÉQUIPE INTERNE")
        .await
        .unwrap();
    assert_eq!(via_alias.len(), 1);
    assert_eq!(via_alias[0].entity_id, equipe);

    // And the boundary rule still applies to non-ASCII names.
    assert!(
        store
            .ask_resolve_entities("prééquipe")
            .await
            .unwrap()
            .is_empty(),
        "`équipe` inside `prééquipe` is not a mention"
    );
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
            scope_thread_id: None,
        })
        .await
        .unwrap();
    let second = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "and the day before?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
            scope_thread_id: None,
        })
        .await
        .unwrap();
    assert_eq!((first.ordinal, second.ordinal), (1, 2));
    assert_eq!(first.answer, None, "the answer stays NULL while streaming");

    let cited = traits::AskCitations {
        sessions: vec![7],
        frames: vec![11],
    };
    assert!(store
        .finish_ask_turn(first.id, "answer one", &cited)
        .await
        .unwrap());
    assert!(!store
        .finish_ask_turn(999_999, "ghost", &cited)
        .await
        .unwrap());

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
            scope_thread_id: None,
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
            scope_thread_id: None,
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
            scope_thread_id: None,
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
async fn ask_title_summary_changes_invalidate_the_stale_session_vector() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-vectors-stale.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");

    let session = store
        .insert_session(ai_session(1_000, Some(2_000)))
        .await
        .unwrap();
    store
        .set_session_title_summary(session, "Deploy runbook", "kubernetes rollout", "test")
        .await
        .unwrap();

    let conn = Connection::open(&db_path).expect("second connection");
    let vector_rows = |session_id: i64| -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM session_vectors WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )
        .unwrap()
    };

    let embedding = unit_vector(1.0, 0.0);
    assert!(store
        .upsert_session_vector(session, "Deploy runbook\nkubernetes rollout", &embedding)
        .await
        .unwrap());
    assert_eq!(vector_rows(session), 1);
    assert!(
        store.session_embed_candidates(10).await.unwrap().is_empty(),
        "a session that already has a vector is not a backfill candidate"
    );

    // Re-writing the same text must not cause re-embed churn.
    store
        .set_session_title_summary(session, "Deploy runbook", "kubernetes rollout", "test")
        .await
        .unwrap();
    assert_eq!(
        vector_rows(session),
        1,
        "an unchanged text keeps its vector"
    );

    // A regenerated summary must invalidate it, or the session is ranked by stale text forever
    // (the backfill only selects sessions with no vector row).
    store
        .set_session_title_summary(session, "Deploy runbook", "argocd sync failed", "test")
        .await
        .unwrap();
    assert_eq!(
        vector_rows(session),
        0,
        "a changed text drops the vector in the same transaction as the write"
    );
    let candidates = store.session_embed_candidates(10).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].session_id, session);
    assert_eq!(
        candidates[0].text, "Deploy runbook\nargocd sync failed",
        "the rebuild uses the new text"
    );

    // Clearing invalidates too, and then there is nothing to embed at all.
    assert!(store
        .upsert_session_vector(session, "Deploy runbook\nargocd sync failed", &embedding)
        .await
        .unwrap());
    assert_eq!(vector_rows(session), 1);
    store.clear_session_title_summary(session).await.unwrap();
    assert_eq!(
        vector_rows(session),
        0,
        "a cleared summary must not keep contributing stale vector evidence"
    );
    assert!(
        store.session_embed_candidates(10).await.unwrap().is_empty(),
        "both NULL is never embedded"
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

// ---- P9 conversations (`03 §7i.4`) -----------------------------------------------------

/// `list_ask_threads` reports each conversation's turn count, its latest turn and its title,
/// ordered newest-first (`last_turn_at` when it has turns, else `created_at`); `finish_ask_turn`
/// writes the answer and the citations in one row, and `list_ask_turns` reads that exact pair
/// back together with the scope thread the turn ran under.
#[tokio::test]
async fn ask_threads_list_titles_and_citations_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-threads.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");
    // A second connection pins the row timestamps: `created_at` has one-second granularity, so
    // without this every thread would tie and the order would only prove the id tiebreak.
    let raw = Connection::open(&db_path).expect("second connection");

    let older = store.create_ask_thread().await.expect("create thread");
    let newer_but_quiet = store.create_ask_thread().await.expect("create thread");
    let unused = store.create_ask_thread().await.expect("create thread");

    let scoped = store
        .append_ask_turn(NewAskTurn {
            thread_id: older,
            question: "what did I ship?".to_string(),
            window: Some(TimeRange {
                start: 1_000,
                end: 2_000,
            }),
            resolved_entities: vec![11],
            scope_thread_id: Some(42),
        })
        .await
        .expect("append the first turn");
    let finished = store
        .append_ask_turn(NewAskTurn {
            thread_id: older,
            question: "and the day before?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
            scope_thread_id: None,
        })
        .await
        .expect("append the follow-up");
    let quieter = store
        .append_ask_turn(NewAskTurn {
            thread_id: newer_but_quiet,
            question: "where are the invoices?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
            scope_thread_id: None,
        })
        .await
        .expect("append the other thread's turn");

    // Pinned so the order below is derived from these values and nothing else.
    let pinned = [
        (older, 1_000_i64),
        (newer_but_quiet, 2_000),
        (unused, 3_000),
    ];
    for (thread_id, created_at) in pinned {
        raw.execute(
            "UPDATE ask_threads SET created_at = ?2 WHERE id = ?1",
            params![thread_id, created_at],
        )
        .expect("pin the conversation's created_at");
    }
    let turn_times = [
        (scoped.id, 4_000_i64),
        (finished.id, 5_000),
        (quieter.id, 2_500),
    ];
    for (turn_id, created_at) in turn_times {
        raw.execute(
            "UPDATE ask_turns SET created_at = ?2 WHERE id = ?1",
            params![turn_id, created_at],
        )
        .expect("pin the turn's created_at");
    }

    let listed = store
        .list_ask_threads(10)
        .await
        .expect("list conversations");
    assert_eq!(
        listed.iter().map(|summary| summary.id).collect::<Vec<_>>(),
        vec![older, newer_but_quiet, unused],
        "conversations with turns lead in turn-recency order, and the unused one falls to the \
         end even though it was opened after the second conversation's turn"
    );
    let by_id = |id: i64| {
        listed
            .iter()
            .find(|summary| summary.id == id)
            .expect("the conversation is listed")
    };
    assert_eq!(by_id(older).turn_count, 2);
    assert_eq!(by_id(older).last_turn_at, Some(5_000));
    assert_eq!(by_id(older).created_at, 1_000);
    assert_eq!(by_id(newer_but_quiet).turn_count, 1);
    assert_eq!(by_id(newer_but_quiet).last_turn_at, Some(2_500));
    assert_eq!(
        (by_id(unused).turn_count, by_id(unused).last_turn_at),
        (0, None),
        "a conversation with no turns reports neither"
    );
    assert_eq!(by_id(older).title, None, "nobody named it yet");

    let limited = store.list_ask_threads(2).await.expect("list two");
    assert_eq!(
        limited.iter().map(|summary| summary.id).collect::<Vec<_>>(),
        vec![older, newer_but_quiet],
        "the limit cuts the tail of the same order"
    );

    // Naming a conversation is a write the list reflects; an unknown id changes nothing.
    assert!(store
        .set_ask_thread_title(older, "Deploys")
        .await
        .expect("set the title"));
    assert!(!store
        .set_ask_thread_title(999_999, "ghost")
        .await
        .expect("set a missing title"));
    let listed = store
        .list_ask_threads(10)
        .await
        .expect("list conversations");
    let titled = listed
        .iter()
        .find(|summary| summary.id == older)
        .expect("the conversation is listed")
        .title
        .as_deref();
    assert_eq!(titled, Some("Deploys"));
    assert_eq!(
        listed
            .iter()
            .filter(|summary| summary.id != older)
            .map(|summary| summary.title.clone())
            .collect::<Vec<_>>(),
        vec![None, None],
        "naming one conversation does not name another"
    );

    // The answer and the citations land in one row, and the reload sees exactly that pair.
    let citations = AskCitations {
        sessions: vec![5, 7],
        frames: vec![31],
    };
    assert!(store
        .finish_ask_turn(finished.id, "It landed.", &citations)
        .await
        .expect("finish the turn"));

    let turns = store.list_ask_turns(older).await.expect("read the turns");
    let by_turn = |id: i64| {
        turns
            .iter()
            .find(|turn| turn.id == id)
            .expect("the turn is listed")
    };
    let reloaded = by_turn(finished.id);
    assert_eq!(reloaded.answer.as_deref(), Some("It landed."));
    assert_eq!(
        reloaded.citations,
        Some(citations.clone()),
        "`list_ask_turns` returns the citations `finish_ask_turn` wrote"
    );
    assert_eq!(reloaded.scope_thread_id, None);
    let streaming = by_turn(scoped.id);
    assert_eq!(
        (streaming.answer.clone(), streaming.citations.clone()),
        (None, None),
        "an unfinished turn carries neither an answer nor citations"
    );
    assert_eq!(
        streaming.scope_thread_id,
        Some(42),
        "the scope thread the turn ran under is persisted, with no thread row required"
    );
    assert_eq!(
        streaming.window,
        Some(TimeRange {
            start: 1_000,
            end: 2_000
        })
    );

    // `last_ask_turn` is the same row, so a follow-up inherits the persisted scope too.
    let last = store
        .last_ask_turn(older)
        .await
        .expect("read the last turn")
        .expect("the conversation has turns");
    assert_eq!(last, reloaded.clone());
}

/// A turn whose `citations` column does not decode is an error, never a silent "no citations":
/// a finished answer must not render as if it had no evidence (`03 §7i.3`).
#[tokio::test]
async fn ask_turn_with_undecodable_citations_is_an_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-citations.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");
    let raw = Connection::open(&db_path).expect("second connection");

    let thread = store.create_ask_thread().await.expect("create thread");
    let turn = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "what shipped?".to_string(),
            window: None,
            resolved_entities: Vec::new(),
            scope_thread_id: None,
        })
        .await
        .expect("append a turn");
    assert!(store
        .finish_ask_turn(
            turn.id,
            "the deploy",
            &AskCitations {
                sessions: vec![1],
                frames: vec![2],
            },
        )
        .await
        .expect("finish the turn"));

    raw.execute(
        "UPDATE ask_turns SET citations = ?2 WHERE id = ?1",
        params![turn.id, "{not json"],
    )
    .expect("corrupt the row");

    let error = store
        .list_ask_turns(thread)
        .await
        .expect_err("a malformed citations column must not decode");
    let message = format!("{error:#}").to_lowercase();
    assert!(
        message.contains("conversion error") && message.contains("text"),
        "the error must describe the undecodable column, got {error:#}"
    );
    assert!(
        store.last_ask_turn(thread).await.is_err(),
        "every turn read goes through the same decoder"
    );

    // A structurally valid but wrong-shaped value is refused too.
    raw.execute(
        "UPDATE ask_turns SET citations = ?2 WHERE id = ?1",
        params![turn.id, "[1,2]"],
    )
    .expect("write a wrong-shaped value");
    assert!(store.list_ask_turns(thread).await.is_err());
}

/// Deleting a conversation removes its turns and nothing else: the sessions, frames, graph rows
/// and insights a turn cited all survive (`03 §7i.4`).
#[tokio::test]
async fn deleting_a_conversation_keeps_history_graph_and_insights() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("ask-delete.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");
    let raw = Connection::open(&db_path).expect("second connection");
    let count = |table: &str| -> i64 {
        raw.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count rows")
    };

    // History the conversation will cite: a session, a frame, an entity link, two threads with
    // their members, and an evidenced insight.
    let threads = seed_threads(&store).await;
    let frame_id = seed_frame(&store, 1_500, "deploy notes").await;
    let entity_id = seed_entity(
        &store,
        threads.session_a,
        frame_id,
        GraphHit {
            kind: EntityKind::Repo,
            canonical_name: "screensearch".to_string(),
            normalized_key: "screensearch".to_string(),
            confidence: 0.9,
            aliases: vec!["ss".to_string()],
        },
    )
    .await;
    let insight_id = store
        .insert_insight(NewInsight {
            window_start: 100,
            window_end: 700,
            payload: InsightPayload::Recurrence(RecurrencePayload {
                entity_id,
                entity_kind: EntityKind::Repo.as_str().to_string(),
                entity_name: "screensearch".to_string(),
                first_session_id: threads.session_a,
                first_seen_at: 100,
                second_session_id: threads.session_b,
                second_seen_at: 300,
                gap_ms: 200,
                occurrence_count: 2,
            }),
            generator: InsightGenerator::Heuristic,
            evidence: vec![InsightEvidence {
                ordinal: 0,
                kind: EvidenceKind::Session,
                ref_id: threads.session_a,
            }],
        })
        .await
        .expect("insert an evidenced insight");

    let watched = [
        "sessions",
        "frames",
        "entities",
        "entity_aliases",
        "session_entities",
        "threads",
        "thread_sessions",
        "insights",
        "insight_evidence",
    ];
    let before = watched.map(|table| (table, count(table)));

    let thread = store.create_ask_thread().await.expect("create thread");
    let turn = store
        .append_ask_turn(NewAskTurn {
            thread_id: thread,
            question: "what did I ship?".to_string(),
            window: None,
            resolved_entities: vec![entity_id],
            // A work-graph thread id: persisted without an FK, so a later merge or re-cluster
            // cannot block a turn from being written.
            scope_thread_id: Some(threads.thread_a),
        })
        .await
        .expect("append a turn");
    assert!(store
        .finish_ask_turn(
            turn.id,
            "the deploy",
            &AskCitations {
                sessions: vec![threads.session_a],
                frames: vec![frame_id],
            },
        )
        .await
        .expect("finish the turn"));
    assert_eq!(count("ask_turns"), 1);

    assert!(store.delete_ask_thread(thread).await.expect("delete it"));
    assert_eq!(count("ask_threads"), 0);
    assert_eq!(
        count("ask_turns"),
        0,
        "the turns cascade with their conversation"
    );
    for (table, expected) in before {
        assert_eq!(
            count(table),
            expected,
            "`{table}` must be untouched by deleting a conversation"
        );
    }
    // The insight is still readable, not merely still counted.
    assert!(
        store
            .list_ask_turns(thread)
            .await
            .expect("read the deleted thread")
            .is_empty(),
        "the conversation is gone"
    );
    assert_eq!(
        store
            .graph_stored_threads()
            .await
            .expect("read the graph")
            .len(),
        2,
        "the work graph survives"
    );
    let insight_count: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM insights WHERE id = ?1",
            [insight_id],
            |row| row.get(0),
        )
        .expect("count the insight");
    assert_eq!(insight_count, 1, "the insight survives the conversation");
}
