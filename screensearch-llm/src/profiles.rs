//! Inference Profiles for LLM Generation
//!
//! Defines preset configurations for different use cases:
//! - VisionAnalysis: For screen content extraction (more precise)
//! - RagAnswer: For conversational RAG responses (more creative)

use serde::{Deserialize, Serialize};

/// Inference profile type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceProfile {
    /// Screen content extraction - precise, structured output
    VisionAnalysis,
    /// Conversational RAG - natural, source-citing responses
    RagAnswer,
    /// Custom profile (use parameters directly from config)
    Custom,
}

impl Default for InferenceProfile {
    fn default() -> Self {
        Self::RagAnswer
    }
}

impl InferenceProfile {
    /// Get the display name for this profile
    pub fn display_name(&self) -> &str {
        match self {
            Self::VisionAnalysis => "Vision Analysis",
            Self::RagAnswer => "RAG Answer",
            Self::Custom => "Custom",
        }
    }

    /// Get the parameters for this profile
    pub fn parameters(&self) -> InferenceParameters {
        match self {
            Self::VisionAnalysis => InferenceParameters {
                temperature: 0.2,
                top_k: 40,
                top_p: 0.9,
                max_tokens: 512,
                repeat_penalty: 1.1,
            },
            Self::RagAnswer => InferenceParameters {
                temperature: 0.4,
                top_k: 50,
                top_p: 0.95,
                max_tokens: 1024,
                repeat_penalty: 1.15,
            },
            Self::Custom => InferenceParameters::default(),
        }
    }

    /// Get the system prompt for this profile
    pub fn system_prompt(&self) -> &str {
        match self {
            Self::VisionAnalysis => VISION_ANALYSIS_SYSTEM_PROMPT,
            Self::RagAnswer => RAG_ANSWER_SYSTEM_PROMPT,
            Self::Custom => "",
        }
    }
}

/// Inference parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceParameters {
    /// Temperature for generation (0.0 = deterministic, 2.0 = very creative)
    pub temperature: f32,
    /// Top-k sampling (number of top tokens to consider)
    pub top_k: usize,
    /// Top-p (nucleus) sampling threshold
    pub top_p: f32,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Repetition penalty (1.0 = no penalty)
    pub repeat_penalty: f32,
}

impl Default for InferenceParameters {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_k: 40,
            top_p: 0.95,
            max_tokens: 2048,
            repeat_penalty: 1.1,
        }
    }
}

/// System prompt for Vision Analysis profile
const VISION_ANALYSIS_SYSTEM_PROMPT: &str = r#"You are a screen content analyzer. Extract structured information from screen captures.

TASK:
Analyze the provided screen content and extract:
1. Primary application/window being used
2. Main content visible (documents, code, conversations, etc.)
3. Key text elements and data points
4. User actions that can be inferred

OUTPUT FORMAT (JSON):
{
  "application": "string - app name",
  "window_title": "string - window title",
  "content_type": "string - document/code/chat/browser/other",
  "key_elements": ["array of important text snippets"],
  "activity": "string - brief description of what user is doing",
  "topics": ["array of topics/keywords"]
}

Be precise and factual. Only include information visible in the content."#;

/// System prompt for RAG Answer profile
const RAG_ANSWER_SYSTEM_PROMPT: &str = r#"You are ScreenSearch Assistant, answering questions based on the user's screen activity history.

CONTEXT:
You have access to timestamped screen captures and OCR text from the user's computer. Use this context to answer questions about what the user was working on, what they saw, or to help them find information.

GUIDELINES:
1. CITE SOURCES: When referencing specific information, mention the approximate time or application where you found it.
2. BE HELPFUL: If the context doesn't contain the answer, say so clearly rather than guessing.
3. SUMMARIZE WELL: For broad questions, provide a coherent narrative rather than listing raw data.
4. RESPECT PRIVACY: Don't emphasize or highlight sensitive information (passwords, personal messages, financial data).

RESPONSE STYLE:
- Conversational but informative
- Use timestamps like "around 2:30 PM" or "earlier today"
- Reference applications: "In VS Code, you were working on..."
- Be concise but thorough"#;

/// Validate inference parameters
pub fn validate_parameters(params: &InferenceParameters) -> Result<(), String> {
    if params.temperature < 0.0 || params.temperature > 2.0 {
        return Err("Temperature must be between 0.0 and 2.0".to_string());
    }
    if params.top_p < 0.0 || params.top_p > 1.0 {
        return Err("Top-p must be between 0.0 and 1.0".to_string());
    }
    if params.max_tokens == 0 {
        return Err("Max tokens must be greater than 0".to_string());
    }
    if params.repeat_penalty < 1.0 {
        return Err("Repeat penalty must be at least 1.0".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_defaults() {
        assert_eq!(InferenceProfile::default(), InferenceProfile::RagAnswer);
    }

    #[test]
    fn test_vision_parameters() {
        let params = InferenceProfile::VisionAnalysis.parameters();
        assert_eq!(params.temperature, 0.2);
        assert_eq!(params.top_k, 40);
        assert_eq!(params.max_tokens, 512);
    }

    #[test]
    fn test_rag_parameters() {
        let params = InferenceProfile::RagAnswer.parameters();
        assert_eq!(params.temperature, 0.4);
        assert_eq!(params.top_k, 50);
        assert_eq!(params.max_tokens, 1024);
    }

    #[test]
    fn test_system_prompts_not_empty() {
        assert!(!InferenceProfile::VisionAnalysis.system_prompt().is_empty());
        assert!(!InferenceProfile::RagAnswer.system_prompt().is_empty());
    }

    #[test]
    fn test_validate_parameters() {
        let valid = InferenceParameters::default();
        assert!(validate_parameters(&valid).is_ok());

        let invalid_temp = InferenceParameters {
            temperature: 3.0,
            ..Default::default()
        };
        assert!(validate_parameters(&invalid_temp).is_err());

        let invalid_top_p = InferenceParameters {
            top_p: 1.5,
            ..Default::default()
        };
        assert!(validate_parameters(&invalid_top_p).is_err());
    }
}
