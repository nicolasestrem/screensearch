//! Text chunking utilities for embedding generation.
//!
//! Splits long OCR text into overlapping chunks that respect an approximate
//! token budget while preserving sentence boundaries. This replaces the Python
//! sidecar `/v1/chunk` endpoint with an equivalent in-process implementation.

/// Approximate English tokens per word, used to translate a token budget into a
/// word budget without pulling in a full tokenizer.
const TOKENS_PER_WORD: f32 = 1.3;

/// Text chunker for splitting documents into embeddable chunks.
#[derive(Debug, Clone)]
pub struct TextChunker {
    /// Maximum tokens per chunk.
    max_tokens: usize,
    /// Overlap between consecutive chunks (in tokens).
    overlap: usize,
}

impl Default for TextChunker {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            overlap: 64,
        }
    }
}

impl TextChunker {
    /// Create a new text chunker with custom settings.
    pub fn new(max_tokens: usize, overlap: usize) -> Self {
        Self {
            max_tokens,
            overlap,
        }
    }

    /// Split text into chunks suitable for embedding, using sentence-aware
    /// splitting to preserve semantic meaning. Each chunk holds approximately
    /// `max_tokens` tokens with `overlap` tokens of context carried over.
    pub fn chunk_text(&self, text: &str) -> Vec<String> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let sentences: Vec<&str> = text
            .split(['.', '!', '?', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if sentences.is_empty() {
            return vec![text.trim().to_string()];
        }

        let max_words = ((self.max_tokens as f32) / TOKENS_PER_WORD).max(1.0) as usize;
        let overlap_words = ((self.overlap as f32) / TOKENS_PER_WORD) as usize;

        // Hard-split any sentence longer than the per-chunk budget (e.g. OCR
        // output with no sentence punctuation) into word groups of `max_words`,
        // so no single piece can produce an oversized chunk that silently
        // truncates at embedding time.
        let mut units: Vec<String> = Vec::new();
        for sentence in sentences {
            let words: Vec<&str> = sentence.split_whitespace().collect();
            if words.len() > max_words {
                for group in words.chunks(max_words) {
                    units.push(group.join(" "));
                }
            } else {
                units.push(sentence.to_string());
            }
        }

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_word_count = 0;

        for sentence in units {
            let sentence_words = sentence.split_whitespace().count();

            if current_word_count + sentence_words > max_words && !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());

                // Seed the next chunk with the tail of the previous one (overlap).
                let words: Vec<&str> = current_chunk.split_whitespace().collect();
                if overlap_words > 0 && words.len() > overlap_words {
                    current_chunk = words[words.len() - overlap_words..].join(" ");
                    current_word_count = overlap_words;
                } else {
                    current_chunk = String::new();
                    current_word_count = 0;
                }
            }

            if !current_chunk.is_empty() {
                current_chunk.push_str(". ");
            }
            current_chunk.push_str(&sentence);
            current_word_count += sentence_words;
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        if chunks.is_empty() {
            return vec![text.trim().to_string()];
        }

        chunks
    }

    /// Estimate the number of tokens in a text (approximate).
    pub fn estimate_tokens(&self, text: &str) -> usize {
        ((text.split_whitespace().count() as f32) * TOKENS_PER_WORD) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_empty_returns_nothing() {
        assert!(TextChunker::default().chunk_text("").is_empty());
        assert!(TextChunker::default().chunk_text("   \n  ").is_empty());
    }

    #[test]
    fn chunk_short_text_is_single_chunk() {
        let chunks = TextChunker::default().chunk_text("Hello, world!");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("Hello"));
        assert!(chunks[0].contains("world"));
    }

    #[test]
    fn chunk_long_text_round_trips_all_sentences() {
        // Small budget (~7 words) forces multiple chunks; every sentence must survive.
        let chunker = TextChunker::new(10, 3);
        let text = "First sentence here. Second sentence follows. Third one comes next. \
                    Fourth is also present. Fifth sentence ends it.";
        let chunks = chunker.chunk_text(text);
        assert!(chunks.len() >= 2, "expected splitting, got {chunks:?}");
        let joined = chunks.join(" ");
        for needle in ["First", "Second", "Third", "Fourth", "Fifth"] {
            assert!(joined.contains(needle), "missing {needle} in {chunks:?}");
        }
    }

    #[test]
    fn punctuation_free_text_is_split_to_budget() {
        // 100 words, no sentence punctuation. With max_tokens=10 (~7 words),
        // every produced chunk must stay within the word budget.
        let chunker = TextChunker::new(10, 0);
        let max_words = ((10_f32) / TOKENS_PER_WORD).max(1.0) as usize;
        let text = (0..100)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunker.chunk_text(&text);
        assert!(chunks.len() > 1, "expected multiple chunks, got {chunks:?}");
        for chunk in &chunks {
            assert!(
                chunk.split_whitespace().count() <= max_words,
                "chunk exceeds {max_words} words: {chunk:?}"
            );
        }
    }

    #[test]
    fn estimate_tokens_is_positive_and_bounded() {
        let tokens = TextChunker::default().estimate_tokens("Hello world this is a test");
        assert!(tokens > 0 && tokens < 20);
    }
}
