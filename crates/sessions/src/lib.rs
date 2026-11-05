#![forbid(unsafe_code)]

mod confidence;
mod exchange;
mod segmenter;
mod taxonomy;

use std::sync::OnceLock;

use traits::{ExtractedExchange, SessionContent, SessionSegmenter, ToolRecognition};

pub use exchange::extract_exchanges;
pub use segmenter::segment_concurrent;
pub use traits::{
    SegmentationParams, SegmenterFrame, SessionDraft, SessionHost, SessionKind,
    SESSION_MERGE_GAP_MS,
};

/// Read-side helper for the 0.4.5 work graph (`03 §7h.2`): the bundled taxonomy's
/// recognition for one frame context, as a plain value the pure `graph` crate can consume
/// without depending on this crate.
///
/// This is a read-only view of the *same* recognition the segmenter applies; it does not
/// change a boundary, a confidence, the taxonomy version or an output shape. It returns
/// `None` for an unrecognized context and also degrades to `None` if the bundled taxonomy
/// ever fails to parse (the shipped engine's startup validation is the loud failure path).
pub fn recognize_tool(
    app_hint: Option<&str>,
    window_title: Option<&str>,
) -> Option<ToolRecognition> {
    static TAXONOMY: OnceLock<Option<taxonomy::Taxonomy>> = OnceLock::new();
    let taxonomy = TAXONOMY
        .get_or_init(|| taxonomy::Taxonomy::seed().ok())
        .as_ref()?;
    taxonomy
        .recognize(app_hint, window_title)
        .map(|recognized| ToolRecognition {
            tool_id: recognized.id,
            kind: recognized.kind,
            host: recognized.host,
        })
}

/// Parsed, reusable production provider. The bundled taxonomy is validated once at startup.
#[derive(Debug, Clone)]
pub struct SessionEngine {
    taxonomy: taxonomy::Taxonomy,
}

impl SessionEngine {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            taxonomy: taxonomy::Taxonomy::seed()?,
        })
    }

    pub fn taxonomy_version(&self) -> u32 {
        self.taxonomy.version
    }
}

impl SessionSegmenter for SessionEngine {
    fn segment(&self, frames: &[SegmenterFrame], params: &SegmentationParams) -> Vec<SessionDraft> {
        segmenter::segment_with_taxonomy(frames, &self.taxonomy, params)
    }

    fn extract_exchanges(
        &self,
        tool_id: &str,
        contents: &[SessionContent],
    ) -> Vec<ExtractedExchange> {
        exchange::extract_exchanges(tool_id, contents)
    }
}

#[cfg(test)]
mod contract_tests {
    use crate::confidence::{anchored_confidence, focus_confidence};

    #[test]
    fn confidence_tiers_keep_anchorless_below_anchored() {
        let clean_anchor = anchored_confidence(600_000, 0);
        let absorbed_anchor = anchored_confidence(300_000, 300_000);
        let dense_focus = focus_confidence(Some("notepad"), 180.0, 90);
        let marginal_focus = focus_confidence(Some("notepad"), 90.0, 90);
        let bare_focus = focus_confidence(None, 180.0, 90);

        assert!(clean_anchor > absorbed_anchor);
        assert!(absorbed_anchor > dense_focus);
        assert!(dense_focus > marginal_focus);
        assert!(marginal_focus > bare_focus);
    }
}
