//! Promotion wire types (`03 §8e` promotion).
//!
//! Promotion is the only write path into the single user-created project note:
//! the user selects individual bullet items from a saved report, previews the
//! dated entry blocks, and confirms. These types are the typed IPC contract
//! between the promotion commands and the UI; they carry no storage or path
//! logic. Proposals are never persisted — a rejected selection leaves no trace.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ReportError;

/// Target section of the project note (`03 §8e` promotion). Wire values are the
/// lowercase snake tokens the note's `## ` headings are derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum PromotionSection {
    Decisions,
    OpenQuestions,
    NextSteps,
    Log,
}

/// One selectable bullet: the section it maps to and its text verbatim from the
/// saved report (`03 §8e` promotion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PromotionItem {
    pub section: PromotionSection,
    pub text: String,
}

/// The user's selection: one source run and the items taken from it
/// (`03 §8e` promotion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PromotionSelection {
    pub run_id: String,
    pub items: Vec<PromotionItem>,
}

/// One `### <date>` entry block as it will be appended to a project-note section
/// (`03 §8e` promotion). `heading_date` is the promotion date; `provenance` is
/// the source footer line naming the run(s) and frame/session IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PromotionEntryPreview {
    pub section: PromotionSection,
    pub heading_date: String,
    pub items: Vec<String>,
    pub provenance: String,
}

/// Exactly what `confirm_promotion` will write, previewed before any write
/// (`03 §8e` promotion). `note_hash` is the selection-time content hash that the
/// confirmation re-checks: any divergence is a conflict, never an overwrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PromotionProposal {
    pub selection: PromotionSelection,
    /// Duplicate of `selection.run_id`, carried for display.
    pub run_id: String,
    /// Forward-slash display path of the target project note.
    pub note_path: Option<String>,
    pub note_exists: bool,
    /// Selection-time content hash; `None` = note absent at selection.
    pub note_hash: Option<String>,
    /// `YYYY-MM-DD` promotion date in the run's saved timezone.
    pub entry_date: String,
    /// Canonical order: Decisions, OpenQuestions, NextSteps, Log.
    pub entries: Vec<PromotionEntryPreview>,
}

/// Outcome of one atomic multi-section append (`03 §8e` promotion). `hash` is
/// set exactly when this promotion's bytes are installed at `path`. A conflict
/// without `hash` appended nothing. A conflict with `hash` is the concurrent
/// swap of `03 §8c`: the append is installed and the version the writer
/// displaced is preserved at `recovery_path`. User bytes are never deleted or
/// rewritten in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct PromotionReceipt {
    pub path: String,
    pub hash: Option<String>,
    pub conflict: bool,
    pub error: Option<ReportError>,
    pub recovery_path: Option<String>,
}

/// State of the single project note (`03 §8e` promotion): 0 notes → `exists`
/// false, >1 notes is the unsupported state surfaced by the commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ProjectNoteStatus {
    pub path: Option<String>,
    pub exists: bool,
    pub count: u32,
}
