//! The `graph_normalize` job body: the optional LLM alias/normalization pass (`03 §7h.2`).
//!
//! This pass is additive and optional: it reads a bounded, most-recent-first batch of session
//! summaries plus the existing entity catalog, asks the attached answer model for canonical
//! names and aliases as strict JSON, validates the proposals through
//! [`graph::normalize::validate_proposals`] and persists them with
//! [`Store::graph_apply_normalization`]. It never rewrites a heuristic entity's
//! `normalized_key` and never deletes a row, so the heuristic graph stays complete and useful
//! with no model attached.
//!
//! No new setting enables it (0.4.5 D6, `03 §8`): the scheduler enqueues it opportunistically
//! when an answer provider is attached and the machine is idle. With no provider, no recent
//! summary or a malformed model answer, the job logs and completes as a no-op; only a
//! transport or storage failure is retryable.
//!
//! The model output contract is fixed here, because the kernel job is the only place a model
//! answer enters the graph:
//!
//! ```json
//! {"proposals":[{"kind":"app","canonical_name":"Name","aliases":["Alias"],
//!   "matched_entity_id":null,"confidence":0.8}]}
//! ```
//!
//! A proposal with `matched_entity_id` only adds aliases to that existing entity; a proposal
//! without one creates an `app` or `document` row (the only kinds a topic-shaped name may
//! become, `03 §7h.1`), and `validate_proposals` recomputes its dedup identity.

use std::sync::Arc;

use anyhow::Context;
use traits::{
    AnswerOpts, AnswerProvider, EntityCatalogRow, EntityKind, NormalizationOutcome,
    NormalizationProposal, RetrievedChunk, SessionSummaryInput, Store,
};

use crate::worker_pool::Outcome;

/// Sessions one normalization pass reads (kernel constant, never a setting: 0.4.5 D6).
pub(crate) const SESSION_LIMIT: u32 = 24;
/// Entity catalog rows one normalization pass reads (kernel constant, never a setting).
pub(crate) const CATALOG_LIMIT: u32 = 128;
/// Upper bound on the model answer's token budget for one pass.
const MAX_TOKENS: u32 = 1_024;

const SYSTEM_PROMPT: &str =
    "You normalize the ScreenSearch work graph. Reply with one JSON object and no prose.";

/// What one normalization pass did, or why it skipped.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NormalizeOutcome {
    Applied(NormalizationOutcome),
    Skipped(&'static str),
}

/// Runs one claimed `graph_normalize` job (`03 §7h.2`). The pass is optional: an absent
/// model, no summary input or a malformed model answer is a logged no-op, never an error.
pub(crate) async fn run(
    store: &Arc<dyn Store>,
    answer: Option<&Arc<dyn AnswerProvider>>,
) -> Outcome {
    let Some(answer) = answer else {
        tracing::info!("graph normalize skipped: no answer provider attached");
        return Outcome::Complete {
            changed_data: false,
        };
    };
    match normalize(store.as_ref(), answer.as_ref()).await {
        Ok(NormalizeOutcome::Applied(outcome)) => {
            tracing::info!(
                entities_created = outcome.entities_created,
                aliases_inserted = outcome.aliases_inserted,
                "graph normalize applied"
            );
            Outcome::Complete {
                changed_data: outcome.entities_created > 0 || outcome.aliases_inserted > 0,
            }
        }
        Ok(NormalizeOutcome::Skipped(reason)) => {
            tracing::info!(reason, "graph normalize skipped");
            Outcome::Complete {
                changed_data: false,
            }
        }
        Err(error) => Outcome::Retry(format!("graph normalize: {error}")),
    }
}

/// One normalization pass over the most recent session summaries (`03 §7h.2`).
pub(crate) async fn normalize(
    store: &dyn Store,
    answer: &dyn AnswerProvider,
) -> anyhow::Result<NormalizeOutcome> {
    let sessions = store
        .graph_sessions_with_summaries(SESSION_LIMIT)
        .await
        .context("reading sessions with summaries")?;
    if sessions.is_empty() {
        return Ok(NormalizeOutcome::Skipped("no recent session summaries"));
    }
    let catalog = store
        .graph_entity_catalog(CATALOG_LIMIT)
        .await
        .context("reading the entity catalog")?;
    let instruction = build_instruction(&catalog);
    let context = build_context(&sessions);
    let (reply, _citations) = answer
        .summarize(
            SYSTEM_PROMPT,
            &instruction,
            &context,
            AnswerOpts {
                thinking: false,
                max_tokens: MAX_TOKENS,
            },
        )
        .await
        .context("calling the answer model for graph normalization")?;
    let raw = match parse_reply(&reply) {
        Ok(proposals) => proposals,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "graph normalize ignored an invalid model answer"
            );
            return Ok(NormalizeOutcome::Skipped("malformed model answer"));
        }
    };
    if raw.is_empty() {
        return Ok(NormalizeOutcome::Skipped("the model proposed nothing"));
    }
    let proposals = match graph::normalize::validate_proposals(raw) {
        Ok(proposals) => proposals,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "graph normalize rejected an inadmissible model answer"
            );
            return Ok(NormalizeOutcome::Skipped("inadmissible model answer"));
        }
    };
    let outcome = store
        .graph_apply_normalization(&proposals)
        .await
        .context("persisting graph normalization proposals")?;
    Ok(NormalizeOutcome::Applied(outcome))
}

/// The prompt's instruction: the strict JSON contract plus the existing catalog. Keeping the
/// catalog in the instruction (not the context chunks) means a provider that overrides
/// `summarize`'s system prompt still receives the identity data it must not contradict.
fn build_instruction(catalog: &[EntityCatalogRow]) -> String {
    let mut instruction = String::from(
        "Return JSON shaped exactly like \
         {\"proposals\":[{\"kind\":\"app\",\"canonical_name\":\"Name\",\
         \"aliases\":[\"Alias\"],\"matched_entity_id\":null,\"confidence\":0.8}]}.\n\
         Rules:\n\
         - kind is one of repo, file, domain, document, ref, app. A new entity (matched_entity_id null) \
         may only be app or document; never propose people.\n\
         - matched_entity_id must be an existing entity id from the catalog below when the proposal \
         only adds aliases to it.\n\
         - canonical_name must be a display name that appears in the session summaries; aliases are \
         additional display forms and must not repeat the canonical name.\n\
         - confidence is between 0 and 1. Propose nothing you cannot ground in the summaries.\n",
    );
    if catalog.is_empty() {
        instruction.push_str("Entity catalog: empty.\n");
        return instruction;
    }
    instruction.push_str("Entity catalog:\n");
    for entity in catalog {
        let aliases = if entity.aliases.is_empty() {
            String::new()
        } else {
            format!(
                " aliases=[{}]",
                entity
                    .aliases
                    .iter()
                    .map(|(alias, _)| format!("\"{alias}\""))
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        };
        instruction.push_str(&format!(
            "- id={} kind={} canonical=\"{}\"{}\n",
            entity.id,
            entity.kind.as_str(),
            entity.canonical_name,
            aliases
        ));
    }
    instruction
}

/// Session summaries as grounding context. The context-carrier field is named `frame_id`,
/// but nothing here reaches a store column or a `jobs.frame_id`: it only lets the provider
/// ground on the summary lines, and the returned citations are deliberately ignored.
fn build_context(sessions: &[SessionSummaryInput]) -> Vec<RetrievedChunk> {
    sessions
        .iter()
        .map(|session| {
            let title = session.title.as_deref().unwrap_or("");
            let summary = session.summary.as_deref().unwrap_or("");
            let tool = session.tool.as_deref().unwrap_or("unknown");
            RetrievedChunk {
                frame_id: session.session_id,
                text: format!(
                    "session {} tool={tool} title: {title}\nsummary: {summary}",
                    session.session_id
                ),
                score: 1.0,
                captured_at: session.started_at,
                // Normalization grounds on summaries, not on session context: its prompt
                // bytes stay exactly what they were before Ask v2 (`03 §7i.2`).
                session: None,
            }
        })
        .collect()
}

/// Parses the model reply into raw proposals. The whole answer fails on any malformed
/// proposal: a partial write would hide the malformed answer, and the pass is optional.
fn parse_reply(reply: &str) -> anyhow::Result<Vec<NormalizationProposal>> {
    let json = extract_json_object(reply)
        .ok_or_else(|| anyhow::anyhow!("model reply contained no JSON object"))?;
    let parsed: RawReply =
        serde_json::from_str(json).context("parsing the graph normalization JSON")?;
    let mut proposals = Vec::with_capacity(parsed.proposals.len());
    for raw in parsed.proposals {
        let kind = EntityKind::from_token(&raw.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown entity kind {:?}", raw.kind))?;
        proposals.push(NormalizationProposal {
            kind,
            canonical_name: raw.canonical_name,
            // `validate_proposals` recomputes the dedup identity from the canonical name.
            normalized_key: String::new(),
            aliases: raw.aliases,
            matched_entity_id: raw.matched_entity_id,
            confidence: raw.confidence,
        });
    }
    Ok(proposals)
}

/// Extracts the first complete JSON object from a model reply, tolerating a code fence or a
/// stray prose prefix around it.
fn extract_json_object(reply: &str) -> Option<&str> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    (end > start).then(|| &reply[start..=end])
}

#[derive(Debug, serde::Deserialize)]
struct RawReply {
    #[serde(default)]
    proposals: Vec<RawProposal>,
}

#[derive(Debug, serde::Deserialize)]
struct RawProposal {
    kind: String,
    canonical_name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    matched_entity_id: Option<i64>,
    confidence: f64,
}

/// Test-only answer providers with no model behind them: one echoes a canned reply, one
/// fails the call. Shared with the scheduler tests, which only need a provider present.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use traits::{AnswerDelta, AnswerOpts, AnswerProvider, RetrievedChunk};

    pub(crate) struct FakeAnswerProvider {
        reply: Option<String>,
        pub(crate) calls: Arc<AtomicUsize>,
    }

    impl FakeAnswerProvider {
        pub(crate) fn replying(reply: &str) -> Self {
            Self {
                reply: Some(reply.to_string()),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        pub(crate) fn failing() -> Self {
            Self {
                reply: None,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        pub(crate) fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl AnswerProvider for FakeAnswerProvider {
        async fn answer(
            &self,
            _query: &str,
            _context: &[RetrievedChunk],
            _opts: AnswerOpts,
            tx: tokio::sync::mpsc::Sender<AnswerDelta>,
        ) -> anyhow::Result<()> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let Some(reply) = &self.reply else {
                anyhow::bail!("fake answer provider failure");
            };
            if tx
                .send(AnswerDelta::Token {
                    text: reply.clone(),
                })
                .await
                .is_err()
            {
                return Ok(());
            }
            let _ = tx.send(AnswerDelta::done()).await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use store::SqliteStore;
    use traits::{AnswerProvider, NewFrame, NewSession, OcrResult, SessionKind, Store};

    use super::test_support::FakeAnswerProvider;
    use super::*;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    /// Inserts one session and one frame inside it, extracts the frame, and returns the
    /// session id. The frame's browser URL is the single fixture entity every test starts
    /// from, so expectations can read its id from the catalog instead of hardcoding one.
    async fn seed_session_with_entity(store: &Arc<dyn Store>) -> i64 {
        let session_id = store
            .insert_session(NewSession {
                started_at: 1_000,
                ended_at: Some(9_000),
                kind: SessionKind::Focus,
                tool: None,
                host: None,
                context_key: "ctx:graph-normalize".to_string(),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .expect("insert session");
        let frame_id = store
            .insert_frame(NewFrame {
                captured_at: 5_000,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: "frames/graph-normalize.webp".to_string(),
                content_hash: "hash-graph-normalize".to_string(),
                app_hint: Some("firefox".to_string()),
                window_title: Some("Work graph".to_string()),
                browser_url: Some("https://github.com/acme/widgets".to_string()),
                capture_trigger: None,
            })
            .await
            .expect("insert frame");
        store
            .insert_ocr(
                frame_id,
                OcrResult {
                    text: "work graph notes".to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .expect("insert ocr");
        // Linking is ownership-only (P5 review ruling), so the fixture assigns the frame to
        // its session before extracting the shared entity and its link.
        assert_eq!(
            store
                .assign_frames_session(&[frame_id], Some(session_id))
                .await
                .expect("assign frame"),
            1
        );
        crate::graph_extract::extract_frame(store.as_ref(), frame_id)
            .await
            .expect("extract frame");
        store
            .set_session_title_summary(
                session_id,
                "Work graph session",
                "The session reviewed the work graph normalization rules and their tests.",
                "fake-model",
            )
            .await
            .expect("cache session summary");
        session_id
    }

    async fn counts(store: &dyn Store) -> traits::GraphTableCounts {
        store.graph_table_counts().await.expect("counts")
    }

    #[tokio::test]
    async fn without_a_model_the_graph_is_unchanged() {
        let store = test_store().await;
        seed_session_with_entity(&store).await;
        let before = counts(store.as_ref()).await;
        assert!(matches!(
            run(&store, None).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(counts(store.as_ref()).await, before);
    }

    #[tokio::test]
    async fn no_summary_makes_no_model_call() {
        let store = test_store().await;
        let session_id = seed_session_with_entity(&store).await;
        // A title/summary pair is the pass's only input; clearing it must stop the pass
        // before any model call, which is what the provider's call counter proves.
        assert!(store
            .clear_session_title_summary(session_id)
            .await
            .expect("clear summary"));
        let before = counts(store.as_ref()).await;
        let fake = Arc::new(FakeAnswerProvider::replying(r#"{"proposals":[]}"#));
        let answer: Arc<dyn AnswerProvider> = fake.clone();
        assert!(matches!(
            run(&store, Some(&answer)).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(fake.calls(), 0, "no input means no model call");
        assert_eq!(counts(store.as_ref()).await, before);
    }

    #[tokio::test]
    async fn a_malformed_model_answer_leaves_the_graph_unchanged() {
        let store = test_store().await;
        seed_session_with_entity(&store).await;
        let before = counts(store.as_ref()).await;
        let fake = Arc::new(FakeAnswerProvider::replying("this is not JSON at all"));
        let answer: Arc<dyn AnswerProvider> = fake;
        assert!(matches!(
            run(&store, Some(&answer)).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(counts(store.as_ref()).await, before);
    }

    #[tokio::test]
    async fn a_transport_failure_is_retryable() {
        let store = test_store().await;
        seed_session_with_entity(&store).await;
        let answer: Arc<dyn AnswerProvider> = Arc::new(FakeAnswerProvider::failing());
        assert!(matches!(
            run(&store, Some(&answer)).await,
            Outcome::Retry(_)
        ));
    }

    #[tokio::test]
    async fn validated_proposals_are_persisted_additively() {
        let store = test_store().await;
        seed_session_with_entity(&store).await;
        let before = counts(store.as_ref()).await;
        let catalog = store
            .graph_entity_catalog(100)
            .await
            .expect("entity catalog");
        let domain = catalog
            .iter()
            .find(|entity| entity.canonical_name == "github.com")
            .expect("fixture frame produced the domain entity");
        let reply = format!(
            "{{\"proposals\":[\
             {{\"kind\":\"domain\",\"canonical_name\":\"github.com\",\
               \"aliases\":[\"GitHub Widgets\"],\"matched_entity_id\":{},\"confidence\":0.9}},\
             {{\"kind\":\"app\",\"canonical_name\":\"ScreenSearch\",\
               \"aliases\":[],\"matched_entity_id\":null,\"confidence\":0.85}}]}}",
            domain.id
        );
        let answer: Arc<dyn AnswerProvider> = Arc::new(FakeAnswerProvider::replying(&reply));
        assert!(matches!(
            run(&store, Some(&answer)).await,
            Outcome::Complete { changed_data: true }
        ));
        let after = counts(store.as_ref()).await;
        assert_eq!(
            after.entities,
            before.entities + 1,
            "exactly the proposed new app row is created"
        );
        assert!(
            after.entity_aliases > before.entity_aliases,
            "the proposed alias is inserted"
        );
        assert_eq!(
            after.session_entities, before.session_entities,
            "normalization never links or unlinks a session"
        );
        let catalog = store
            .graph_entity_catalog(100)
            .await
            .expect("entity catalog");
        let domain = catalog
            .iter()
            .find(|entity| entity.id == domain.id)
            .expect("domain entity survives");
        assert!(
            domain
                .aliases
                .iter()
                .any(|(alias, source)| alias == "GitHub Widgets"
                    && *source == traits::AliasSource::Llm),
            "the new alias carries the llm provenance"
        );
        assert!(
            catalog
                .iter()
                .any(|entity| entity.kind == EntityKind::App
                    && entity.canonical_name == "ScreenSearch"),
            "the topic-shaped name is stored as an app entity"
        );
    }

    #[tokio::test]
    async fn an_inadmissible_proposal_is_a_no_op() {
        let store = test_store().await;
        seed_session_with_entity(&store).await;
        let before = counts(store.as_ref()).await;
        // A new `repo` row is inadmissible: a topic-shaped name may only become app or
        // document, and `validate_proposals` rejects the whole batch.
        let reply = "{\"proposals\":[{\"kind\":\"repo\",\"canonical_name\":\"acme/widgets\",\
                     \"aliases\":[],\"matched_entity_id\":null,\"confidence\":0.9}]}";
        let answer: Arc<dyn AnswerProvider> = Arc::new(FakeAnswerProvider::replying(reply));
        assert!(matches!(
            run(&store, Some(&answer)).await,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(counts(store.as_ref()).await, before);
    }

    #[test]
    fn the_model_contract_is_parsed_from_a_fenced_answer() {
        let reply = "```json\n{\"proposals\":[{\"kind\":\"app\",\"canonical_name\":\"Name\",\
                     \"aliases\":[\"Alias\"],\"confidence\":0.5}]}\n```";
        let proposals = parse_reply(reply).expect("parse fenced reply");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].kind, EntityKind::App);
        assert_eq!(proposals[0].canonical_name, "Name");
        assert_eq!(proposals[0].aliases, vec!["Alias".to_string()]);
        assert_eq!(proposals[0].matched_entity_id, None);
        assert!((proposals[0].confidence - 0.5).abs() < f64::EPSILON);
    }
}
