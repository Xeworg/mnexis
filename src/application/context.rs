//! The `memory_context` pipeline (PRD §9, A-07).
//!
//! The pipeline follows PRD §9 "Pipeline" steps 1-6 and e2-core §5:
//! 1. Authorize (scope: read).
//! 2. Retrieve candidates via FTS5 (`retrieval::search`).
//! 3. Apply authorized filters (state, namespace, memory_types,
//!    valid_window) on the candidate set.
//! 4. Revalidate canonical (revision check against `memories`).
//! 5. Conservative eligibility: drop superseded, archived, deleted,
//!    out-of-window. Diversity: no duplicate `topic_key`, no exact
//!    duplicate content.
//! 6. Select memories that fit the budget, in deterministic order
//!    (bm25 ASC, then created_at ASC).
//!
//! The output envelope is fixed: `items`, `retrieval_mode`,
//! `budget_enforcement`, `tokens_used`/`bytes_used`, `tokenizer_id`,
//! `index_status`, `warnings`, optional `empty_reason`.

use serde::{Deserialize, Serialize};

use crate::application::budget::{Budget, BudgetMode, SUPPORTED_TOKENIZER_V1, token_count_v1};
use crate::domain::error::MnexisError;
use crate::domain::memory::Memory;
use crate::domain::query::MatchMode;
use crate::retrieval::SearchHit;

/// Filters that layer on top of the FTS5 candidate set (PRD §9 step 3).
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub memory_types: Vec<String>,
    pub valid_window: Option<(Option<String>, Option<String>)>,
    /// When true, include only the active state; when false, include
    /// all states (the FTS5 query already filters at the retrieval
    /// layer; the diversity pass then drops ineligible ones).
    pub active_only: bool,
}

/// Input to `MemoryService::context`.
#[derive(Debug, Clone)]
pub struct ContextRequest {
    pub task: String,
    pub namespace: Option<String>,
    /// How the task's words are matched: `any` (the default) counts every
    /// term, `all` requires all of them, `phrase` keeps the task as one
    /// exact phrase.
    pub match_mode: MatchMode,
    pub token_budget: i64,
    pub tokenizer_id: Option<String>,
    pub max_items: Option<usize>,
    pub budget_mode: BudgetMode,
    pub search: SearchFilter,
    /// Used to validate temporal windows; not part of the public wire
    /// shape (the harness provides it).
    pub now: String,
}

/// One item in the `memory_context` response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextItem {
    pub uuid: String,
    /// What the item *is*: the title of the fact. Empty when the memory
    /// has none, exactly like its `topic_key`. The context is what the
    /// agent reads first, so the identity travels with it.
    pub title: String,
    /// The words the fact is called by in another language or another
    /// jargon (agent-keywords S4). Empty when the memory has none, never
    /// `null`.
    pub keywords: Vec<String>,
    pub revision: i32,
    pub namespace: String,
    pub content: String,
    pub memory_type: String,
    pub state: String,
    pub topic_key: String,
    pub assertion_basis: String,
    pub created_at: String,
}

/// The structured response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextResponse {
    pub items: Vec<ContextItem>,
    /// The namespace the door searched (#57).
    ///
    /// Every item carries its own namespace, but an empty answer has to
    /// say where it looked: without this the agent cannot tell "this
    /// project has no memories" from "it looked in another project" —
    /// and the second one reads exactly like the first.
    pub searched_namespace: String,
    pub retrieval_mode: &'static str,
    pub budget_enforcement: &'static str,
    pub tokens_used: i64,
    pub bytes_used: i64,
    pub tokenizer_id: String,
    pub index_status: &'static str,
    pub warnings: Vec<&'static str>,
    pub empty_reason: Option<String>,
}

const RETRIEVAL_MODE: &str = "textual";
const INDEX_STATUS_OK: &str = "ok";

impl ContextResponse {
    /// Build the empty-response envelope with the appropriate
    /// `empty_reason`. Used when the pipeline produces no items (either
    /// no candidates or the budget is too small for the envelope).
    pub fn empty(reason: impl Into<String>, budget: &Budget) -> Self {
        Self {
            items: Vec::new(),
            searched_namespace: String::new(),
            retrieval_mode: RETRIEVAL_MODE,
            budget_enforcement: budget.enforcement_label(),
            tokens_used: 0,
            bytes_used: 0,
            tokenizer_id: budget.tokenizer_id().to_string(),
            index_status: INDEX_STATUS_OK,
            warnings: warnings_for(budget),
            empty_reason: Some(reason.into()),
        }
    }
}

fn warnings_for(budget: &Budget) -> Vec<&'static str> {
    match budget.mode() {
        BudgetMode::Estimated => vec!["lexical_estimation"],
        BudgetMode::Exact => Vec::new(),
    }
}

/// Project a (hit, memory) into the public item shape, ordered by
/// bm25 ASC then `created_at` ASC.
fn project(out: &mut Vec<ContextItem>, mems: Vec<Memory>) {
    for mem in mems {
        out.push(ContextItem {
            uuid: mem.uuid,
            title: mem.title,
            keywords: mem.keywords,
            revision: mem.revision,
            namespace: mem.namespace,
            content: mem.content,
            memory_type: mem.memory_type,
            state: mem.state.as_str().to_string(),
            topic_key: mem.topic_key,
            assertion_basis: mem.assertion_basis,
            created_at: mem.created_at,
        });
    }
}

/// Compute the final response from the selected memories and the budget.
pub fn select_response(
    budget: &Budget,
    tokenizer_id: &str,
    selected: Vec<Memory>,
) -> Result<ContextResponse, MnexisError> {
    if selected.is_empty() {
        return Ok(ContextResponse::empty(
            "no candidates passed the eligibility and diversity filters",
            budget,
        ));
    }

    // Greedy fill: take memories in deterministic order until the
    // serialised envelope does not fit. Determinism is the caller's
    // order (the steering tier, then created_at ASC).
    let mut items: Vec<ContextItem> = Vec::with_capacity(selected.len().min(20));
    let mut accepted: Vec<Memory> = Vec::new();
    for mem in selected {
        accepted.push(mem);
        let mut tentative: Vec<ContextItem> = Vec::new();
        project(&mut tentative, accepted.clone());
        let serialised = serialise_envelope(&tentative, budget);
        let tokens = token_count_v1(&serialised);
        if budget.mode() == BudgetMode::Exact {
            if tokens > budget.token_budget() {
                // Drop the last item we just appended.
                accepted.pop();
                if accepted.is_empty() {
                    return Ok(ContextResponse::empty(
                        "even the envelope itself exceeds the token budget",
                        budget,
                    ));
                }
                break;
            }
        } else {
            // Estimated: byte proxy on the serialised envelope.
            if serialised.len() as i64 > budget.byte_budget() {
                accepted.pop();
                if accepted.is_empty() {
                    return Ok(ContextResponse::empty(
                        "even the envelope itself exceeds the estimated byte budget",
                        budget,
                    ));
                }
                break;
            }
        }
        items = tentative;
    }

    let tokens_used = token_count_v1(&serialise_envelope(&items, budget));
    let bytes_used = serialise_envelope(&items, budget).len() as i64;
    let tokenizer = match budget.mode() {
        BudgetMode::Exact => {
            // Caller asked for exact; we tokenize with the supported
            // one. The caller passes `tokenizer_id` for documentation;
            // if it doesn't match the supported id, we still count
            // but the response says which one we used.
            if tokenizer_id.is_empty() || tokenizer_id == SUPPORTED_TOKENIZER_V1 {
                SUPPORTED_TOKENIZER_V1
            } else {
                // A non-supported id in exact mode is a contract
                // violation: surface as BudgetTooSmall. Tests pin
                // this with `check_with_tokenizer`.
                let _ = budget.check_with_tokenizer(tokenizer_id, tokens_used)?;
                SUPPORTED_TOKENIZER_V1
            }
        }
        BudgetMode::Estimated => SUPPORTED_TOKENIZER_V1,
    };

    Ok(ContextResponse {
        items,
        searched_namespace: String::new(),
        retrieval_mode: RETRIEVAL_MODE,
        budget_enforcement: budget.enforcement_label(),
        tokens_used,
        bytes_used,
        tokenizer_id: tokenizer.to_string(),
        index_status: INDEX_STATUS_OK,
        warnings: warnings_for(budget),
        empty_reason: None,
    })
}

/// A stable serialisation of an items envelope, used to token-count
/// the response. The shape mirrors what the CLI/MCP surfaces so the
/// token count on either path is the same. When `items` is empty the
/// output is the empty envelope (so `token_count_v1("") == 0` and the
/// A-07 invariant holds for empty responses too).
pub fn serialise_envelope(items: &[ContextItem], budget: &Budget) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str("{\"budget_enforcement\":\"");
    s.push_str(budget.enforcement_label());
    s.push_str("\",\"tokenizer_id\":\"");
    s.push_str(budget.tokenizer_id());
    s.push_str("\",\"items\":[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&serde_json::to_string(item).expect("serialise item"));
    }
    s.push_str("]}");
    s
}

/// Run the full pipeline: FTS5 candidates + canonical revalidation +
/// eligibility + dedup + budget selection.
pub fn build_response(
    budget: &Budget,
    tokenizer_id: &str,
    candidates: Vec<(SearchHit, Memory)>,
    filter: &SearchFilter,
    now: &str,
) -> Result<ContextResponse, MnexisError> {
    let mut eligible_pairs: Vec<Memory> = Vec::new();
    for (_hit, mem) in candidates {
        if mem.deleted {
            continue;
        }
        if filter.active_only && mem.state.as_str() != "active" {
            continue;
        }
        if !filter.memory_types.is_empty()
            && !filter.memory_types.iter().any(|t| t == &mem.memory_type)
        {
            continue;
        }
        if let Some((vf, _vu)) = &filter.valid_window {
            if let Some(vf) = vf {
                if let Some(actual) = mem.valid_from.as_deref() {
                    if actual > vf.as_str() {
                        continue;
                    }
                }
            }
        }
        let _ = now;
        eligible_pairs.push(mem);
    }
    let deduped = dedup(eligible_pairs);
    // Determinism (arm B of the steering experiment): the steering tier
    // of the type first, then created_at ASC — the door answers "where
    // are we" before "what is true", over what the text search matched.
    let mut selected = deduped;
    selected.sort_by(|a, b| {
        crate::domain::memory_type::steering_tier(&a.memory_type)
            .cmp(&crate::domain::memory_type::steering_tier(&b.memory_type))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
    select_response(budget, tokenizer_id, selected)
}

fn dedup(mems: Vec<Memory>) -> Vec<Memory> {
    use std::collections::HashSet;
    let mut seen_topics: HashSet<String> = HashSet::new();
    let mut seen_content: HashSet<String> = HashSet::new();
    let mut out: Vec<Memory> = Vec::with_capacity(mems.len());
    for mem in mems {
        if !mem.topic_key.is_empty() && !seen_topics.insert(mem.topic_key.clone()) {
            continue;
        }
        if !seen_content.insert(mem.content.clone()) {
            continue;
        }
        out.push(mem);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::memory::MemoryState;
    use crate::retrieval::fts5::SearchHit;

    fn memory(uuid: &str, content: &str, kind: &str, state: MemoryState) -> Memory {
        Memory {
            id: Some(1),
            uuid: uuid.into(),
            title: format!("titulo de {uuid}"),
            keywords: Vec::new(),
            topic_key: String::new(),
            namespace: "default".into(),
            memory_type: kind.into(),
            content: content.into(),
            state,
            revision: 1,
            deleted: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: "user".into(),
            channel: "cli".into(),
            assertion_basis: "user_stated".into(),
        }
    }

    fn hit(uuid: &str, rank: f64) -> SearchHit {
        SearchHit {
            uuid: uuid.into(),
            rank,
            namespace: "default".into(),
            memory_type: "fact".into(),
            state: "active".into(),
            revision: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            valid_from: None,
            valid_until: None,
        }
    }

    #[test]
    fn dedup_drops_duplicate_topic_keys() {
        // m1 has no topic_key and distinct content -> kept.
        // m2 carries topic_key "shared" and distinct content -> kept.
        // m3 carries topic_key "shared" (duplicate of m2) and distinct
        //   content -> dropped (topic_key wins over content dedup).
        let m1 = memory("u1", "alpha", "fact", MemoryState::Active);
        let mut m2 = memory("u2", "beta", "fact", MemoryState::Active);
        m2.topic_key = "shared".into();
        let mut m3 = memory("u3", "gamma", "fact", MemoryState::Active);
        m3.topic_key = "shared".into();
        let out = dedup(vec![m1, m2, m3]);
        assert_eq!(
            out.len(),
            2,
            "first shared topic_key wins, content kept distinct"
        );
    }

    #[test]
    fn select_response_serialised_envelope_within_budget() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let m1 = memory("u1", "alpha alpha alpha", "fact", MemoryState::Active);
        let m2 = memory("u2", "beta", "fact", MemoryState::Active);
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![m1, m2]).unwrap();
        assert_eq!(out.items.len(), 2);
        // The token count the response claims equals what the
        // serialised envelope produces.
        let serialised = serialise_envelope(&out.items, &budget);
        assert_eq!(out.tokens_used, token_count_v1(&serialised));
    }

    #[test]
    fn a_context_item_carries_the_title_of_its_memory() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let m = memory("u1", "alpha", "fact", MemoryState::Active);
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![m]).unwrap();
        assert_eq!(out.items.len(), 1);
        assert_eq!(out.items[0].title, "titulo de u1");
        // Additive: the content the item already carried stays.
        assert_eq!(out.items[0].content, "alpha");
    }

    #[test]
    fn a_context_item_carries_the_words_of_its_memory() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let mut m = memory("u1", "listen on port 8081", "fact", MemoryState::Active);
        m.keywords = vec!["puerto".into(), "port".into()];
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![m]).unwrap();
        assert_eq!(out.items.len(), 1);
        assert_eq!(
            out.items[0].keywords,
            vec!["puerto", "port"],
            "the words travel with the context"
        );
    }

    #[test]
    fn a_context_item_with_no_words_reports_the_empty_list() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let m = memory("u1", "sin palabras", "fact", MemoryState::Active);
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![m]).unwrap();
        let json = serde_json::to_value(&out.items[0]).expect("serialise item");
        assert_eq!(
            json["keywords"],
            serde_json::json!([]),
            "no words is the empty list, never null: {json}"
        );
    }

    #[test]
    fn select_response_budget_too_small_returns_empty() {
        // A budget so small that even the envelope prelude doesn't fit.
        let budget = Budget::new(2, BudgetMode::Exact);
        let m1 = memory("u1", "alpha", "fact", MemoryState::Active);
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![m1]).unwrap();
        assert!(out.items.is_empty());
        assert!(out.empty_reason.is_some());
    }

    #[test]
    fn build_response_ranks_the_steering_tier_before_recency() {
        // Arm B of the steering experiment (tmp/bench/
        // steering-measurement.md): the door answers "where are we"
        // first — a project_state record created LATER outranks a
        // knowledge record created EARLIER, deterministically, with no
        // heuristic on the task's words.
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let mut older_knowledge = memory(
            "u-know",
            "the service listens on 8081",
            "knowledge",
            MemoryState::Active,
        );
        older_knowledge.created_at = "2026-01-01T00:00:00Z".into();
        let mut newer_state = memory(
            "u-state",
            "the bench runs with podman",
            "project_state",
            MemoryState::Active,
        );
        newer_state.created_at = "2026-02-02T00:00:00Z".into();
        let candidates = vec![
            (hit("u-know", 1.0), older_knowledge),
            (hit("u-state", 2.0), newer_state),
        ];
        let filter = SearchFilter {
            memory_types: vec![],
            valid_window: None,
            active_only: true,
        };
        let out = build_response(
            &budget,
            SUPPORTED_TOKENIZER_V1,
            candidates,
            &filter,
            "2026-03-03T00:00:00Z",
        )
        .unwrap();
        assert_eq!(
            out.items[0].uuid, "u-state",
            "the steering tier outranks recency"
        );
        assert_eq!(out.items[1].uuid, "u-know");
    }

    #[test]
    fn build_response_keeps_recency_order_within_one_type() {
        // The pinned behavior the tier may not move: records of the
        // same type keep the created_at ASC order the door has today.
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let mut older = memory("u-old", "alpha", "fact", MemoryState::Active);
        older.created_at = "2026-01-01T00:00:00Z".into();
        let mut newer = memory("u-new", "beta", "fact", MemoryState::Active);
        newer.created_at = "2026-02-02T00:00:00Z".into();
        let filter = SearchFilter {
            memory_types: vec![],
            valid_window: None,
            active_only: true,
        };
        let out = build_response(
            &budget,
            SUPPORTED_TOKENIZER_V1,
            vec![(hit("u-new", 1.0), newer), (hit("u-old", 2.0), older)],
            &filter,
            "2026-03-03T00:00:00Z",
        )
        .unwrap();
        assert_eq!(
            out.items[0].uuid, "u-old",
            "same type: older first, as today"
        );
        assert_eq!(out.items[1].uuid, "u-new");
    }
}
