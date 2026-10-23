//! What a save says about the records its title looks like.
//!
//! The engine ranks and proposes, the agent adjudicates: a candidate carries
//! its score and the words it shares with the query, so the agent judges the
//! proposal instead of trusting it. Nothing here says "these are the same
//! fact" — it says "these are the records your title looks like".
//!
//! The link the proposal names is the relation that already exists:
//! `supersede`, with the reason that says why. No new kind of relation, no
//! new storage: the fact's history stays in the supersede chain.

use std::fmt::Write as _;

use crate::domain::duplicates::DuplicateCandidate;
use crate::domain::memory::Memory;

/// The most candidates a shortlist holds. A shortlist is not a listing: past
/// this many, the answer stops helping the agent and starts being a dump of
/// the namespace. The CLI and the MCP tool read the same cap, so their
/// envelopes cannot disagree.
pub const MAX_DUPLICATE_LIMIT: usize = 50;

/// The limit a caller asked for, capped at [`MAX_DUPLICATE_LIMIT`].
pub fn clamp_duplicate_limit(limit: usize) -> usize {
    limit.min(MAX_DUPLICATE_LIMIT)
}

/// One candidate in the on-demand envelope: the fields the save proposal
/// carries, without its `link` command (the agent asking on demand has
/// saved nothing to link yet). Both front ends build their items with it,
/// so the CLI and the MCP tool cannot drift.
pub fn duplicate_item(candidate: &DuplicateCandidate) -> serde_json::Value {
    serde_json::json!({
        "uuid": candidate.uuid,
        "title": candidate.title,
        "topic_key": candidate.topic_key,
        "score": candidate.score,
        "shared": candidate.shared,
    })
}

/// Why a duplicate title is linked. The fact's history lives in the
/// supersede chain; this is the reason the chain carries.
pub const SAME_FACT_REASON: &str = "same fact, different title";

/// The command that links a candidate to the record just saved.
///
/// The candidate is the **older** record: it existed before. The record just
/// saved is the newer one, so it replaces the old one and the fact ends up
/// in one record with both revisions kept.
pub fn link_command(candidate_uuid: &str, saved_uuid: &str) -> String {
    format!(
        "mnexis supersede --older {candidate_uuid} --newer {saved_uuid} --reason \"{SAME_FACT_REASON}\""
    )
}

/// The proposal in the two shapes the surfaces show: the JSON the CLI and
/// the MCP tool return, and the text the human reads.
///
/// The JSON is an array — one object per candidate, empty when nothing looks
/// like the title — and the text is empty in that case, so a save without a
/// proposal reads exactly as it always did.
pub fn duplicates_payload(
    saved: &Memory,
    candidates: &[DuplicateCandidate],
) -> (serde_json::Value, String) {
    let json = serde_json::Value::Array(
        candidates
            .iter()
            .map(|candidate| {
                serde_json::json!({
                    "uuid": candidate.uuid,
                    "title": candidate.title,
                    "topic_key": candidate.topic_key,
                    "score": candidate.score,
                    "shared": candidate.shared,
                    "link": link_command(&candidate.uuid, &saved.uuid),
                })
            })
            .collect(),
    );

    let mut text = String::new();
    if !candidates.is_empty() {
        text.push_str("these records of the namespace look like this title:");
        for candidate in candidates {
            let _ = write!(
                text,
                "\n  - {} [{}] score {} shared: {}",
                candidate.title,
                candidate.topic_key,
                candidate.score,
                candidate.shared.join(", ")
            );
            let _ = write!(
                text,
                "\n    if it is the same fact: {}",
                link_command(&candidate.uuid, &saved.uuid)
            );
        }
    }
    (json, text)
}
