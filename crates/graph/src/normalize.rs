//! Validation for the optional LLM normalization proposals (`03 §7h.2`, second paragraph).
//!
//! The pass itself is not here: the kernel job builds a prompt from recent session summaries
//! and the entity catalog, calls the attached answer model and parses typed proposals. This
//! module owns the *rules* that decide whether a model answer is admissible, so the kernel
//! job stays a thin I/O shell and the absence of a model leaves the heuristic graph complete.
//!
//! Admissibility (`03 §7h.1/.2`):
//! - the entity kinds stay the closed six; a *new* row may only be `app` or `document`
//!   (a topic-shaped name is stored as an alias-backed app/document entity, never as a
//!   heuristic kind), while aliases may be attached to any existing entity;
//! - a proposal never carries its own dedup identity: the key is recomputed with the same
//!   [`normalized_key`](crate::extraction::normalized_key) the heuristic pass uses;
//! - aliases are trimmed, unique per proposal (case-insensitive), at most
//!   [`MAX_ALIASES_PER_PROPOSAL`], and never merely the canonical name;
//! - confidence is a finite value in `(0, 1]`;
//! - the output is deterministic: sorted, with exact duplicates removed.

use anyhow::{bail, Context};

use traits::{EntityKind, NormalizationProposal};

use crate::extraction::normalized_key;

/// Upper bound on aliases one model proposal may carry, so a runaway answer cannot write an
/// unbounded alias set into a single transaction.
pub const MAX_ALIASES_PER_PROPOSAL: usize = 12;
/// Upper bound on a canonical name's length. A name longer than this is model noise, not an
/// entity the user would recognize; rejecting it is honest.
pub const MAX_CANONICAL_NAME_CHARS: usize = 256;

/// Validates and canonicalizes raw model proposals. Any malformed proposal fails the whole
/// batch: a partial write would hide the malformed answer, and the job logs the failure
/// instead (`03 §7h.2`: the pass is optional, a failure degrades to no normalization).
pub fn validate_proposals(
    raw: Vec<NormalizationProposal>,
) -> anyhow::Result<Vec<NormalizationProposal>> {
    let mut validated = Vec::with_capacity(raw.len());
    for (index, proposal) in raw.into_iter().enumerate() {
        validated.push(validate_one(proposal).with_context(|| format!("proposal {index}"))?);
    }
    validated.sort_by(|a, b| {
        (
            a.matched_entity_id.unwrap_or(i64::MAX),
            a.kind,
            a.normalized_key.as_str(),
        )
            .cmp(&(
                b.matched_entity_id.unwrap_or(i64::MAX),
                b.kind,
                b.normalized_key.as_str(),
            ))
    });
    validated.dedup_by(|a, b| {
        a.matched_entity_id == b.matched_entity_id
            && a.kind == b.kind
            && a.normalized_key == b.normalized_key
            && a.canonical_name.eq_ignore_ascii_case(&b.canonical_name)
    });
    Ok(validated)
}

fn validate_one(mut proposal: NormalizationProposal) -> anyhow::Result<NormalizationProposal> {
    let canonical = proposal.canonical_name.trim();
    if canonical.is_empty() {
        bail!("canonical name is empty");
    }
    if canonical.chars().count() > MAX_CANONICAL_NAME_CHARS {
        bail!("canonical name exceeds {MAX_CANONICAL_NAME_CHARS} characters");
    }
    if let Some(entity_id) = proposal.matched_entity_id {
        if entity_id <= 0 {
            bail!("matched_entity_id must be a positive entity id");
        }
        // An alias-only proposal is additive by definition; it may target any existing kind.
    } else if !matches!(proposal.kind, EntityKind::App | EntityKind::Document) {
        bail!(
            "a new entity may only be app or document (topic-shaped), got {}",
            proposal.kind.as_str()
        );
    }
    if !proposal.confidence.is_finite() || proposal.confidence <= 0.0 || proposal.confidence > 1.0 {
        bail!("confidence must be finite and in (0, 1]");
    }
    let key = normalized_key(proposal.kind, canonical);
    if key.is_empty() {
        bail!("canonical name normalizes to an empty key");
    }

    let canonical_lower = canonical.to_ascii_lowercase();
    let mut aliases: Vec<String> = Vec::new();
    for raw_alias in proposal.aliases {
        let alias = raw_alias.trim();
        if alias.is_empty() || alias.eq_ignore_ascii_case(canonical) {
            continue;
        }
        if alias.chars().count() > MAX_CANONICAL_NAME_CHARS {
            bail!("alias exceeds {MAX_CANONICAL_NAME_CHARS} characters");
        }
        if aliases
            .iter()
            .any(|known| known.eq_ignore_ascii_case(alias))
        {
            continue;
        }
        aliases.push(alias.to_string());
        if aliases.len() > MAX_ALIASES_PER_PROPOSAL {
            bail!("proposal carries more than {MAX_ALIASES_PER_PROPOSAL} aliases");
        }
    }
    if aliases
        .iter()
        .any(|alias| alias.to_ascii_lowercase() == canonical_lower)
    {
        aliases.retain(|alias| !alias.eq_ignore_ascii_case(canonical));
    }

    proposal.canonical_name = canonical.to_string();
    proposal.normalized_key = key;
    proposal.aliases = aliases;
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::NormalizationProposal;

    fn proposal(kind: EntityKind, name: &str) -> NormalizationProposal {
        NormalizationProposal {
            kind,
            canonical_name: name.to_string(),
            normalized_key: String::new(),
            aliases: Vec::new(),
            matched_entity_id: None,
            confidence: 0.9,
        }
    }

    #[test]
    fn recomputes_the_shared_dedup_identity() {
        let proposals =
            validate_proposals(vec![proposal(EntityKind::App, " ScreenSearch ")]).unwrap();
        assert_eq!(proposals[0].canonical_name, "ScreenSearch");
        assert_eq!(proposals[0].normalized_key, "screensearch");
    }

    #[test]
    fn rejects_a_new_heuristic_kind_but_allows_aliases_on_it() {
        assert!(validate_proposals(vec![proposal(EntityKind::Repo, "acme/widgets")]).is_err());

        let mut alias_only = proposal(EntityKind::Repo, "acme/widgets");
        alias_only.matched_entity_id = Some(4);
        alias_only.aliases = vec!["widgets".to_string()];
        let validated = validate_proposals(vec![alias_only]).unwrap();
        assert_eq!(validated[0].aliases, vec!["widgets".to_string()]);
    }

    #[test]
    fn drops_duplicate_and_canonical_aliases() {
        let mut p = proposal(EntityKind::Document, "ScreenSearch");
        p.aliases = vec![
            "ScreenSearch".to_string(),
            " screensearch ".to_string(),
            "Screen Search".to_string(),
            "Screen Search".to_string(),
        ];
        let validated = validate_proposals(vec![p]).unwrap();
        assert_eq!(validated[0].aliases, vec!["Screen Search".to_string()]);
    }

    #[test]
    fn rejects_non_finite_or_zero_confidence() {
        let mut p = proposal(EntityKind::App, "Name");
        p.confidence = 0.0;
        assert!(validate_proposals(vec![p]).is_err());
        let mut p = proposal(EntityKind::App, "Name");
        p.confidence = f64::NAN;
        assert!(validate_proposals(vec![p]).is_err());
        let mut p = proposal(EntityKind::App, "Name");
        p.confidence = 1.5;
        assert!(validate_proposals(vec![p]).is_err());
    }

    #[test]
    fn output_is_deterministic_and_sorted() {
        let raw = vec![
            proposal(EntityKind::App, "Zeta"),
            proposal(EntityKind::Document, "alpha"),
            proposal(EntityKind::App, "beta"),
        ];
        let first = validate_proposals(raw.clone()).unwrap();
        let second = validate_proposals(raw).unwrap();
        assert_eq!(first, second);
        let keys: Vec<&str> = first.iter().map(|p| p.normalized_key.as_str()).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn rejects_an_empty_canonical_name() {
        assert!(validate_proposals(vec![proposal(EntityKind::App, "   ")]).is_err());
    }
}
