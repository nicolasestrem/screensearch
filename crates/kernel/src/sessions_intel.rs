//! Lazy in-app session title/summary generation (D3). API/MCP reads never call this module.

use std::sync::Arc;

use traits::{AnswerOpts, AnswerProvider, RetrievedChunk, Session, Store};

const MAX_CONTEXT_FRAMES: usize = 24;
const MIN_USEFUL_SUMMARY_CHARS: usize = 32;

/// Generates and validates a title + summary in at most two `summarize` calls, then
/// caches both with model provenance. A fully cached row performs no model call.
pub async fn generate_session_title_summary(
    store: Arc<dyn Store>,
    answer: Arc<dyn AnswerProvider>,
    session_id: i64,
) -> anyhow::Result<Session> {
    let mut current = store
        .get_session(session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session {session_id} not found"))?;
    let rejected_cached_summary = current
        .summary
        .as_deref()
        .filter(|summary| !is_useful_session_summary(summary));
    if let Some(summary) = rejected_cached_summary {
        tracing::warn!(
            session_id,
            summary_len = summary.chars().count(),
            "invalid cached session summary rejected; clearing cache before regeneration"
        );
        if !store.clear_session_title_summary(session_id).await? {
            anyhow::bail!(
                "session {session_id} disappeared before its invalid summary cache could be cleared"
            );
        }
        current.title = None;
        current.summary = None;
        current.summary_model = None;
    } else if current.title.is_some()
        && current.summary.is_some()
        && current.summary_model.is_some()
    {
        return Ok(current);
    }

    let frames = store.session_frames_meta(session_id).await?;
    let sampled_ids = evenly_sample_ids(&frames, MAX_CONTEXT_FRAMES);
    let contents = store.content_texts_for_frames(&sampled_ids).await?;
    if contents.is_empty() {
        anyhow::bail!("session {session_id} has no filtered content_text to summarize");
    }
    let context: Vec<RetrievedChunk> = contents
        .into_iter()
        .map(|content| RetrievedChunk {
            frame_id: content.frame_id,
            text: content.content_text,
            score: 1.0,
            captured_at: content.captured_at,
        })
        .collect();
    let mut attempt = 0;
    let (title, summary) = loop {
        let (generated, _) = answer
            .summarize(
                "Create a neutral recall label from the supplied ScreenSearch session frames. Do not invent facts.",
                "Return exactly two lines: `Title: <short title>` then `Summary: <concise factual summary>`.",
                &context,
                AnswerOpts {
                    thinking: false,
                    max_tokens: 384,
                },
            )
            .await?;
        let rejected_summary_len = match parse_generated(&generated) {
            Ok((title, summary)) if is_useful_session_summary(&summary) => {
                break (title, summary);
            }
            Ok((_, summary)) => summary.chars().count(),
            Err(_) => 0,
        };
        if attempt == 1 {
            // D8 favors omission over invented output: preserve the NULL cache rather than
            // expose model garbage through the only value-delivery surface. The caller must see
            // a terminal error, not a successful empty response that renders as indefinite
            // generation (usage review 2026-08-01 §7.4).
            tracing::warn!(
                session_id,
                summary_len = rejected_summary_len,
                "session summary generation rejected twice; leaving cache empty"
            );
            anyhow::bail!("session {session_id} summary was rejected twice; cache left empty");
        }
        attempt += 1;
    };
    let model = answer
        .answer_model_label()
        .await
        .ok_or_else(|| anyhow::anyhow!("answer provider omitted session summary provenance"))?;
    if !store
        .set_session_title_summary(session_id, &title, &summary, &model)
        .await?
    {
        anyhow::bail!("session {session_id} disappeared before its summary could be cached");
    }
    store
        .get_session(session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session {session_id} disappeared after summary cache"))
}

fn evenly_sample_ids(frames: &[traits::SegmenterFrame], limit: usize) -> Vec<i64> {
    if frames.len() <= limit {
        return frames.iter().map(|frame| frame.id).collect();
    }
    (0..limit)
        .map(|index| frames[index * (frames.len() - 1) / (limit - 1)].id)
        .collect()
}

fn parse_generated(value: &str) -> anyhow::Result<(String, String)> {
    let mut title = None;
    let mut summary = None;
    for line in value.lines() {
        if let Some(value) = line.trim().strip_prefix("Title:") {
            title = nonempty(value);
        } else if let Some(value) = line.trim().strip_prefix("Summary:") {
            summary = nonempty(value);
        }
    }
    match (title, summary) {
        (Some(title), Some(summary)) => Ok((title, summary)),
        _ => anyhow::bail!("answer model returned an invalid session title/summary shape"),
    }
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}
/// Accepts only non-blank summaries of at least 32 Unicode scalar values that are not
/// standalone speaker labels. This blocks the cached `"User"` failure observed in production
/// (usage review 2026-08-01 §7.4); per D8, omission is safer than persisting model garbage.
pub fn is_useful_session_summary(value: &str) -> bool {
    let value = value.trim();
    let speaker = value.trim_matches(|character: char| !character.is_alphanumeric());
    if [
        "User",
        "Assistant",
        "System",
        "Codex",
        "ChatGPT",
        "Human",
        "AI",
    ]
    .iter()
    .any(|role| speaker.eq_ignore_ascii_case(role))
    {
        return false;
    }

    value.chars().count() >= MIN_USEFUL_SUMMARY_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: i64) -> traits::SegmenterFrame {
        traits::SegmenterFrame {
            id,
            captured_at: id,
            app_hint: None,
            window_title: None,
            browser_url: None,
        }
    }

    #[test]
    fn even_sampling_includes_both_session_endpoints() {
        let frames: Vec<_> = (1..=100).map(frame).collect();
        let sampled = evenly_sample_ids(&frames, 24);

        assert_eq!(sampled.len(), 24);
        assert_eq!(sampled.first(), Some(&1));
        assert_eq!(sampled.last(), Some(&100));
    }

    #[test]
    fn degenerate_session_summaries_are_rejected() {
        for summary in ["User", "user.", " Assistant ", "", "   ", "Brief update."] {
            assert!(!is_useful_session_summary(summary), "{summary:?}");
        }
    }

    #[test]
    fn punctuated_speaker_label_over_minimum_length_is_rejected() {
        let summary = "!!!!!!!!!!!!!!!!!!!!!!!!!!!!User!!!!";

        assert!(summary.chars().count() >= MIN_USEFUL_SUMMARY_CHARS);
        assert!(!is_useful_session_summary(summary));
    }

    #[test]
    fn substantive_session_summaries_are_accepted() {
        let summaries = [
            "The implementation plan for Luminous Playground includes nine TDD-driven tasks, with one release blocker related to a legacy PHP file containing a tracked plaintext credential.",
            "The session investigated a failing desktop capture path, compared the relevant logs with stored frame metadata, and identified the missing window-title update.",
            "The user reviewed the release checklist, resolved the remaining API documentation questions, and prepared the verified changes for the next build.",
        ];

        for summary in summaries {
            assert!(is_useful_session_summary(summary), "{summary:?}");
        }
    }
}
