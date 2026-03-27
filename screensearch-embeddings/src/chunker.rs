//! Text chunking utilities for embedding generation
//!
//! Splits long texts into optimal chunks for embedding while preserving
//! semantic meaning and respecting token limits.

/// Text chunker for splitting documents into embeddable chunks
#[derive(Debug, Clone)]
pub struct TextChunker {
    /// Maximum tokens per chunk
    max_tokens: usize,
    /// Overlap between chunks (in tokens)
    overlap: usize,
}

impl Default for TextChunker {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            overlap: 32,
        }
    }
}

impl TextChunker {
    /// Create a new text chunker with custom settings
    pub fn new(max_tokens: usize, overlap: usize) -> Self {
        Self { max_tokens, overlap }
    }

    /// Split text into chunks suitable for embedding
    ///
    /// Uses sentence-aware splitting to preserve semantic meaning.
    /// Each chunk will have approximately `max_tokens` tokens with
    /// `overlap` tokens of context from the previous chunk.
    pub fn chunk_text(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }

        // Simple sentence-based chunking
        // A more sophisticated approach would use the tokenizer
        let sentences: Vec<&str> = text
            .split(['.', '!', '?', '\n'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if sentences.is_empty() {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_word_count = 0;

        // Approximate: 1 word ≈ 1.3 tokens for English
        let max_words = (self.max_tokens as f32 / 1.3) as usize;

        for sentence in sentences {
            let sentence_words = sentence.split_whitespace().count();
            
            if current_word_count + sentence_words > max_words && !current_chunk.is_empty() {
                // Save current chunk and start new one
                chunks.push(current_chunk.trim().to_string());
                
                // Start new chunk with overlap (last few words of previous)
                let words: Vec<&str> = current_chunk.split_whitespace().collect();
                let overlap_words = (self.overlap as f32 / 1.3) as usize;
                if words.len() > overlap_words {
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
            current_chunk.push_str(sentence);
            current_word_count += sentence_words;
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        // If we ended up with no chunks (very short text), return the original
        if chunks.is_empty() {
            return vec![text.to_string()];
        }

        chunks
    }

    /// Estimate the number of tokens in a text (approximate)
    pub fn estimate_tokens(&self, text: &str) -> usize {
        // Rough estimation: ~1.3 tokens per word for English
        // For other languages this may vary
        let words = text.split_whitespace().count();
        ((words as f32) * 1.3) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_empty() {
        let chunker = TextChunker::default();
        assert!(chunker.chunk_text("").is_empty());
    }

    #[test]
    fn test_chunk_short_text() {
        let chunker = TextChunker::default();
        // "Hello, world!" gets split by '!' leaving "Hello, world"
        let chunks = chunker.chunk_text("Hello, world!");
        assert_eq!(chunks.len(), 1);
        // The chunk contains the text (punctuation may be stripped)
        assert!(chunks[0].contains("Hello"));
        assert!(chunks[0].contains("world"));
    }

    #[test]
    fn test_chunk_multiple_sentences() {
        let chunker = TextChunker::new(50, 10); // Small chunks for testing
        let text = "First sentence here. Second sentence follows. Third one comes next. Fourth is also present. Fifth sentence ends it.";
        let chunks = chunker.chunk_text(text);
        assert!(chunks.len() >= 1);
    }

    #[test]
    fn test_estimate_tokens() {
        let chunker = TextChunker::default();
        let tokens = chunker.estimate_tokens("Hello world this is a test");
        assert!(tokens > 0);
        assert!(tokens < 20);
    }
}
