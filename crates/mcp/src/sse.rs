//! Server-Sent Events parsing for `POST /v1/ask`.
//!
//! Two pure pieces, both unit-testable without I/O: [`SseLineBuffer`] reassembles lines
//! across arbitrary byte-chunk boundaries, and [`AskAggregator`] folds the API's
//! `AnswerDelta` events into a final answer + citation list. The API emits one `data:` line
//! per delta with a `:` keep-alive comment roughly every 15 s (`docs/API.md`).

/// Accumulates bytes across `bytes_stream()` chunks and yields complete lines (without the
/// trailing newline). A trailing `\r` (defensive against CRLF) is stripped.
#[derive(Default)]
pub struct SseLineBuffer {
    buf: Vec<u8>,
}

impl SseLineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a chunk, returning every complete line it completed.
    ///
    /// Scans the buffer once, slicing each line in place (a trailing `\r` trimmed), then
    /// drains all consumed bytes in a single shift — no per-line temporary `Vec` or repeated
    /// front-drain. Splitting on the `\n` byte before the UTF-8 decode keeps every cut on a
    /// character boundary, so a multi-byte glyph straddling a chunk edge is never corrupted.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut lines = Vec::new();
        let mut start = 0;
        for pos in 0..self.buf.len() {
            if self.buf[pos] == b'\n' {
                let mut end = pos; // exclusive; drop the '\n'
                if end > start && self.buf[end - 1] == b'\r' {
                    end -= 1;
                }
                lines.push(String::from_utf8_lossy(&self.buf[start..end]).into_owned());
                start = pos + 1;
            }
        }
        if start > 0 {
            self.buf.drain(..start);
        }
        lines
    }

    /// Returns any final unterminated line (call once at stream end).
    pub fn finish(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let mut line = std::mem::take(&mut self.buf);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(String::from_utf8_lossy(&line).into_owned())
    }
}

/// The control-flow signal from feeding one SSE line to the aggregator.
#[derive(Debug, PartialEq, Eq)]
pub enum Flow {
    /// Keep reading.
    Continue,
    /// The stream signalled `done` — stop cleanly.
    Done,
    /// The stream signalled an `error` delta — stop with this message.
    Failed(String),
}

/// Folds `AnswerDelta` SSE events into an answer string + cited frame ids + cited session ids.
#[derive(Default)]
pub struct AskAggregator {
    pub answer: String,
    /// Source frame ids, in first-seen order, deduplicated.
    pub citations: Vec<i64>,
    /// Ask v2 (`03 §7i.3`): the distinct session ids carrying the frames in the assembled
    /// prompt, from the terminal `citations` event. Empty for a stream that never emits it.
    pub cited_sessions: Vec<i64>,
    pub tokens_seen: bool,
}

/// Collects the integers of one JSON array, dropping non-integers and duplicates while
/// preserving the producer's order (the API already sorts sessions ascending).
fn dedup_ids(values: Option<&serde_json::Value>) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for id in values.and_then(|v| v.as_array()).into_iter().flatten() {
        if let Some(id) = id.as_i64() {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

impl AskAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one raw SSE line. Comment (`:`) and blank lines are ignored; `data:` lines are
    /// parsed as a tagged `AnswerDelta`. `thinking` deltas are discarded (a second model's
    /// chain-of-thought is prompt noise to the calling model); `data: {}` and unknown types
    /// are ignored (forward-compatible, so a newer API can add events without breaking a
    /// client that predates them).
    ///
    /// The Ask v2 terminal `citations` event (`03 §7i.3`) is aggregated, not ignored: its
    /// `frames` array *replaces* the frame citations gathered from the per-`citation`
    /// events of the same turn (the two describe the same set, and the terminal one is
    /// authoritative), and its `sessions` array becomes [`Self::cited_sessions`]. A stream
    /// that emits no `citations` event keeps the per-`citation` behaviour unchanged.
    pub fn feed(&mut self, line: &str) -> Flow {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with(':') {
            return Flow::Continue; // keep-alive comment or SSE record separator
        }
        let data = match line.strip_prefix("data:") {
            Some(rest) => rest.trim_start(),
            None => return Flow::Continue, // ignore `event:` / `id:` and any non-data field
        };
        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return Flow::Continue, // tolerate a malformed/empty payload
        };
        match value.get("type").and_then(|t| t.as_str()) {
            Some("token") => {
                if let Some(t) = value.get("text").and_then(|t| t.as_str()) {
                    self.answer.push_str(t);
                    self.tokens_seen = true;
                }
                Flow::Continue
            }
            Some("citation") => {
                if let Some(id) = value.get("frame_id").and_then(|f| f.as_i64()) {
                    if !self.citations.contains(&id) {
                        self.citations.push(id);
                    }
                }
                Flow::Continue
            }
            Some("citations") => {
                if let Some(frames) = value.get("frames") {
                    self.citations = dedup_ids(Some(frames));
                }
                if let Some(sessions) = value.get("sessions") {
                    self.cited_sessions = dedup_ids(Some(sessions));
                }
                Flow::Continue
            }
            Some("thinking") => Flow::Continue, // discarded — model-to-model noise
            Some("done") => Flow::Done,
            Some("error") => Flow::Failed(
                value
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("the answer stream reported an error")
                    .to_string(),
            ),
            _ => Flow::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_reassemble_across_every_byte_boundary() {
        let payload = b"data: {\"type\":\"token\",\"text\":\"hi\"}\ndata: {\"type\":\"done\"}\n";
        // Split the payload at each offset and confirm the reassembled line set is identical.
        let full = {
            let mut b = SseLineBuffer::new();
            b.push(payload)
        };
        for split in 1..payload.len() {
            let mut b = SseLineBuffer::new();
            let mut got = b.push(&payload[..split]);
            got.extend(b.push(&payload[split..]));
            assert_eq!(got, full, "mismatch splitting at {split}");
        }
    }

    #[test]
    fn crlf_is_stripped() {
        let mut b = SseLineBuffer::new();
        let lines = b.push(b"data: x\r\ndata: y\r\n");
        assert_eq!(lines, vec!["data: x".to_string(), "data: y".to_string()]);
    }

    #[test]
    fn keepalives_and_blank_lines_ignored() {
        let mut agg = AskAggregator::new();
        assert_eq!(agg.feed(": keep-alive"), Flow::Continue);
        assert_eq!(agg.feed(""), Flow::Continue);
        assert_eq!(agg.feed("event: message"), Flow::Continue);
        assert!(agg.answer.is_empty());
    }

    #[test]
    fn empty_data_object_ignored() {
        let mut agg = AskAggregator::new();
        assert_eq!(agg.feed("data: {}"), Flow::Continue);
        assert!(!agg.tokens_seen);
    }

    #[test]
    fn thinking_discarded_tokens_concatenated() {
        let mut agg = AskAggregator::new();
        agg.feed(r#"data: {"type":"thinking","text":"hmm"}"#);
        agg.feed(r#"data: {"type":"token","text":"The "}"#);
        agg.feed(r#"data: {"type":"token","text":"answer"}"#);
        assert_eq!(agg.answer, "The answer");
        assert!(agg.tokens_seen);
    }

    #[test]
    fn citations_deduped_in_arrival_order() {
        let mut agg = AskAggregator::new();
        agg.feed(r#"data: {"type":"citation","frame_id":41}"#);
        agg.feed(r#"data: {"type":"citation","frame_id":7}"#);
        agg.feed(r#"data: {"type":"citation","frame_id":41}"#);
        assert_eq!(agg.citations, vec![41, 7]);
        // No `citations` event arrived: the session list stays empty, never fabricated.
        assert!(agg.cited_sessions.is_empty());
    }

    #[test]
    fn citations_event_records_sessions_and_replaces_frames() {
        // The pre-0.4.5 per-frame events still arrive first; the terminal `citations` event
        // describes the same turn and is authoritative for both sets.
        let mut agg = AskAggregator::new();
        agg.feed(r#"data: {"type":"token","text":"The invoice "}"#);
        agg.feed(r#"data: {"type":"citation","frame_id":41}"#);
        agg.feed(r#"data: {"type":"citation","frame_id":7}"#);
        let event = serde_json::json!({
            "type": "citations",
            "sessions": [7, 9],
            "frames": [7, 41],
            "thread_id": 3,
            "turn_id": 1,
            "stage_counts": {"threads": 2, "sessions": 2, "frames": 2},
            "tokens_assembled": 812,
        });
        assert_eq!(
            agg.feed(&format!("data: {event}")),
            Flow::Continue,
            "citations is not terminal — `done` is"
        );
        assert_eq!(agg.cited_sessions, vec![7, 9]);
        assert_eq!(
            agg.citations,
            vec![7, 41],
            "the event's frames replace the per-citation list, in its own order"
        );
        assert_eq!(agg.answer, "The invoice ");
        assert_eq!(agg.feed(r#"data: {"type":"done"}"#), Flow::Done);
    }

    #[test]
    fn citations_event_tolerates_missing_arrays_and_duplicates() {
        let mut agg = AskAggregator::new();
        agg.feed(r#"data: {"type":"citation","frame_id":5}"#);
        // A malformed/partial event leaves what earlier events established untouched.
        agg.feed(r#"data: {"type":"citations","thread_id":1,"turn_id":1}"#);
        assert_eq!(agg.citations, vec![5]);
        assert!(agg.cited_sessions.is_empty());
        // Duplicates collapse; non-integer entries are dropped.
        agg.feed(r#"data: {"type":"citations","sessions":[4,4,"x",6],"frames":[5,5]}"#);
        assert_eq!(agg.cited_sessions, vec![4, 6]);
        assert_eq!(agg.citations, vec![5]);
    }

    #[test]
    fn unknown_event_types_are_ignored() {
        // A newer API may add events a client predating them does not know (the contract in
        // `03 §7i.3`: the stream is extended, never replaced). Such an event must neither
        // terminate the stream nor perturb the aggregated answer or citations.
        let mut agg = AskAggregator::new();
        agg.feed(r#"data: {"type":"token","text":"answer"}"#);
        agg.feed(r#"data: {"type":"citation","frame_id":12}"#);
        for unknown in [
            r#"data: {"type":"insight","id":4,"text":"later phases"}"#,
            r#"data: {"type":"session_citation","session_id":9}"#,
            r#"data: {"type":"progress","step":2}"#,
        ] {
            assert_eq!(agg.feed(unknown), Flow::Continue, "{unknown}");
        }
        assert_eq!(agg.answer, "answer");
        assert_eq!(agg.citations, vec![12]);
        assert!(agg.cited_sessions.is_empty());
        assert_eq!(agg.feed(r#"data: {"type":"done"}"#), Flow::Done);
    }

    #[test]
    fn done_and_error_terminate() {
        let mut agg = AskAggregator::new();
        assert_eq!(agg.feed(r#"data: {"type":"done"}"#), Flow::Done);
        assert_eq!(
            agg.feed(r#"data: {"type":"error","message":"boom"}"#),
            Flow::Failed("boom".to_string())
        );
    }
}
