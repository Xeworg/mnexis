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
    /// The fact's **whole** content.
    ///
    /// The door carries whole facts: the budget decides which facts fit, never
    /// how much of a fact is shown — a cut fact is a fact the agent has to
    /// fetch again, and fetching it costs a call.
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
    /// The session-state block: this project's state, with the content that
    /// fits its budget.
    ///
    /// One answer, not a fan-out: the live `project_state` and `decision`
    /// records of the namespace the door searched (newest first, whole content
    /// while the block's own budget has room for it), the index of the live
    /// records that did not fit, and the most recent session summary — the
    /// call-budget experiment measured the agent spending three extra rounds
    /// rebuilding this by hand, and the block bought a round back (S6: it
    /// ascends on the door, and only there).
    ///
    /// The block is outside the budget — the counted envelope is
    /// [`serialise_envelope`] (label, tokenizer, items) — so it never changes
    /// `items`, their order or `tokens_used`. It carries a budget of its own
    /// ([`crate::application::service::session_state::SESSION_STATE_TOKEN_BUDGET`]),
    /// which governs [`crate::application::SessionState::records`] alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_state: Option<crate::application::service::session_state::SessionState>,
    /// What the answer covered, and what it left out.
    ///
    /// The agent's cross-check round is a **coverage** problem, not a trust
    /// problem: the door already carried the facts and the agent re-read
    /// them anyway. An answer that says what it left out gives that round
    /// something to look for (board item 12, M4).
    pub coverage: Coverage,
}

/// How many candidates the answer may name as left out. The real number is
/// in [`Coverage::left_out`]; this keeps the declaration cheap.
pub const COVERAGE_ITEM_LIMIT: usize = 10;

/// What the answer covered, and what it left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Coverage {
    /// The candidates the door saw.
    pub candidates: usize,
    /// How many of them the answer carries.
    pub carried: usize,
    /// How many the budget left out. The list below is capped, so this is
    /// the real number.
    pub left_out: usize,
    /// The candidates the budget left out: identity, title, and the size that
    /// explains why they did not fit. Capped at [`COVERAGE_ITEM_LIMIT`].
    pub left_out_items: Vec<OmittedItem>,
}

/// A candidate the budget left out.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmittedItem {
    /// The memory's stable identifier, as `mnexis show` expects it.
    pub uuid: String,
    /// What the fact *is*. Empty when it has no title.
    pub title: String,
    /// The kind of thing the fact is.
    pub memory_type: String,
    /// The length of the fact's content in bytes: the number that explains
    /// why it did not fit the budget.
    pub bytes: usize,
}

impl OmittedItem {
    fn of(memory: &Memory) -> Self {
        Self {
            uuid: memory.uuid.clone(),
            title: memory.title.clone(),
            memory_type: memory.memory_type.clone(),
            bytes: memory.content.len(),
        }
    }
}

impl Coverage {
    /// The coverage of an answer: what it saw, what it carries, what it left
    /// out and by how much.
    fn of(candidates: usize, carried: usize, left_out: Vec<Memory>) -> Self {
        let count = left_out.len();
        let items = left_out
            .iter()
            .take(COVERAGE_ITEM_LIMIT)
            .map(OmittedItem::of)
            .collect();
        Self {
            candidates,
            carried,
            left_out: count,
            left_out_items: items,
        }
    }
}

const RETRIEVAL_MODE: &str = "textual";
const INDEX_STATUS_OK: &str = "ok";

impl ContextResponse {
    /// Build the empty-response envelope with the appropriate
    /// `empty_reason`. Used when the pipeline produces no items (either
    /// no candidates or the budget is too small for the envelope).
    pub fn empty(reason: impl Into<String>, budget: &Budget) -> Self {
        Self::empty_with(reason, budget, Coverage::default())
    }

    /// The empty answer, with what it saw and what it left out: an answer
    /// that carries nothing still says why.
    pub fn empty_with(reason: impl Into<String>, budget: &Budget, coverage: Coverage) -> Self {
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
            session_state: None,
            coverage,
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

/// Compute the final response from the selected candidates and the budget.
///
/// The candidates arrive in the door's order — bm25 ASC (the most relevant
/// first), then `created_at` ASC — and the fill is greedy: the budget keeps
/// the most relevant facts that fit, and the ones it could not keep are
/// **declared**, not dropped in silence (S2).
pub fn select_response(
    budget: &Budget,
    tokenizer_id: &str,
    selected: Vec<(f64, Memory)>,
) -> Result<ContextResponse, MnexisError> {
    let candidates = selected.len();
    if selected.is_empty() {
        return Ok(ContextResponse::empty(
            "no candidates passed the eligibility and diversity filters",
            budget,
        ));
    }

    // Greedy fill: take the candidates in the door's order until the
    // serialised envelope does not fit.
    let mut items: Vec<ContextItem> = Vec::with_capacity(candidates.min(20));
    let mut accepted: Vec<Memory> = Vec::new();
    let mut left_out: Vec<Memory> = Vec::new();
    let mut exhausted = false;
    let mut empty_reason: Option<&'static str> = None;
    for (_, mem) in selected.iter() {
        if exhausted {
            // The budget is spent: this one and every one after it are
            // declared as left out.
            left_out.push(mem.clone());
            continue;
        }
        accepted.push(mem.clone());
        let mut tentative: Vec<ContextItem> = Vec::new();
        project(&mut tentative, accepted.clone());
        let serialised = serialise_envelope(&tentative, budget);
        let tokens = token_count_v1(&serialised);
        let over_budget = match budget.mode() {
            BudgetMode::Exact => tokens > budget.token_budget(),
            // Estimated: byte proxy on the serialised envelope.
            BudgetMode::Estimated => serialised.len() as i64 > budget.byte_budget(),
        };
        if over_budget {
            // Drop the last item we just appended.
            accepted.pop();
            left_out.push(mem.clone());
            exhausted = true;
            if accepted.is_empty() {
                empty_reason = Some(match budget.mode() {
                    BudgetMode::Exact => "even the envelope itself exceeds the token budget",
                    BudgetMode::Estimated => {
                        "even the envelope itself exceeds the estimated byte budget"
                    }
                });
            }
            continue;
        }
        items = tentative;
    }
    if let Some(reason) = empty_reason {
        // Nothing fits: the empty answer, and every candidate is declared
        // as left out — the answer says why it is empty.
        return Ok(ContextResponse::empty_with(
            reason,
            budget,
            Coverage::of(candidates, 0, left_out),
        ));
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
                budget.check_with_tokenizer(tokenizer_id, tokens_used)?;
                SUPPORTED_TOKENIZER_V1
            }
        }
        BudgetMode::Estimated => SUPPORTED_TOKENIZER_V1,
    };

    let carried = items.len();
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
        session_state: None,
        coverage: Coverage::of(candidates, carried, left_out),
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
    let mut eligible_pairs: Vec<(f64, Memory)> = Vec::new();
    for (hit, mem) in candidates {
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
        if let Some((vf, _vu)) = &filter.valid_window
            && let Some(vf) = vf
            && let Some(actual) = mem.valid_from.as_deref()
            && actual > vf.as_str()
        {
            continue;
        }
        let _ = now;
        // The rank travels with the memory: the door's order is bm25 ASC
        // (the most relevant first), then created_at ASC (PRD §9 step 6).
        eligible_pairs.push((hit.rank, mem));
    }
    let deduped = dedup(eligible_pairs);
    // Determinism: bm25 ASC, then created_at ASC.
    let mut selected = deduped;
    selected.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.created_at.cmp(&b.1.created_at))
    });
    select_response(budget, tokenizer_id, selected)
}

fn dedup(candidates: Vec<(f64, Memory)>) -> Vec<(f64, Memory)> {
    use std::collections::HashSet;
    let mut seen_topics: HashSet<String> = HashSet::new();
    let mut seen_content: HashSet<String> = HashSet::new();
    let mut out: Vec<(f64, Memory)> = Vec::with_capacity(candidates.len());
    for (rank, mem) in candidates {
        if !mem.topic_key.is_empty() && !seen_topics.insert(mem.topic_key.clone()) {
            continue;
        }
        if !seen_content.insert(mem.content.clone()) {
            continue;
        }
        out.push((rank, mem));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::memory::MemoryState;

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
        let out = dedup(vec![(0.0, m1), (0.0, m2), (0.0, m3)]);
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
        let out =
            select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![(0.0, m1), (1.0, m2)]).unwrap();
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
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![(0.0, m)]).unwrap();
        assert_eq!(out.items.len(), 1);
        assert_eq!(out.items[0].title, "titulo de u1");
        assert_eq!(out.items[0].content, "alpha");
        let item = serde_json::to_value(&out.items[0]).expect("serialise item");
        assert!(
            item.get("preview").is_none(),
            "no surface of the item carries a preview: {item}"
        );
    }

    /// The door carries the whole fact: the budget decides which facts fit
    /// (whole items), never how much of a fact is shown.
    #[test]
    fn an_item_carries_the_whole_content_never_a_cut_fact() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let long = "alpha beta gamma delta ".repeat(20);
        let out = select_response(
            &budget,
            SUPPORTED_TOKENIZER_V1,
            vec![(0.0, memory("u1", &long, "fact", MemoryState::Active))],
        )
        .unwrap();
        assert_eq!(out.items.len(), 1);
        assert_eq!(
            out.items[0].content, long,
            "the whole content, never the first 160 characters"
        );
        let item = serde_json::to_value(&out.items[0]).expect("serialise item");
        assert!(
            item.get("preview").is_none(),
            "the item has no preview field: {item}"
        );
    }

    /// The door keeps the **most relevant** facts that fit the budget: bm25
    /// ASC, then created_at ASC (PRD §9 step 6). The implementation used to
    /// sort by `created_at` alone and drop the rank, so the budget filled
    /// with the oldest facts and the most relevant one could be left out.
    #[test]
    fn the_door_keeps_the_most_relevant_first() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let mut relevant = memory("u-relevant", "alpha", "fact", MemoryState::Active);
        relevant.created_at = "2026-01-01T00:00:00Z".into();
        let mut other = memory("u-other", "beta", "fact", MemoryState::Active);
        other.created_at = "2026-06-01T00:00:00Z".into();
        let hit = |uuid: &str, rank: f64| SearchHit {
            uuid: uuid.into(),
            rank,
            namespace: "default".into(),
            memory_type: "fact".into(),
            state: "active".into(),
            revision: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            valid_from: None,
            valid_until: None,
        };
        let out = build_response(
            &budget,
            SUPPORTED_TOKENIZER_V1,
            vec![
                (hit("u-relevant", -2.0), relevant),
                (hit("u-other", -1.0), other),
            ],
            &SearchFilter::default(),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(
            out.items[0].uuid, "u-relevant",
            "the most relevant fact comes first, even when it is the oldest"
        );
    }

    /// The answer declares what it did not include: the candidates the
    /// budget left out, with the size that explains why they did not fit.
    /// The agent's cross-check round needs something to look for.
    #[test]
    fn the_answer_declares_what_the_budget_left_out() {
        let m1 = memory("u1", "alpha alpha alpha", "fact", MemoryState::Active);
        let m2 = memory("u2", "beta beta beta", "fact", MemoryState::Active);
        let out = select_response(
            &Budget::new(120, BudgetMode::Exact),
            SUPPORTED_TOKENIZER_V1,
            vec![(0.0, m1), (1.0, m2)],
        )
        .unwrap();
        assert_eq!(out.coverage.candidates, 2, "the door saw two candidates");
        assert_eq!(
            out.coverage.carried,
            out.items.len(),
            "the answer says how many it carries"
        );
        assert_eq!(
            out.coverage.left_out,
            2 - out.coverage.carried,
            "the answer says how many it left out"
        );
        for entry in &out.coverage.left_out_items {
            assert!(!entry.uuid.is_empty(), "the entry names the record");
            assert!(
                entry.bytes > 0,
                "the entry says how big the fact is: {entry:?}"
            );
        }
        assert_eq!(
            out.coverage.left_out_items.len(),
            out.coverage.left_out.min(COVERAGE_ITEM_LIMIT),
            "the list is the count, up to the limit"
        );
    }

    #[test]
    fn a_context_item_carries_the_words_of_its_memory() {
        let budget = Budget::new(10_000, BudgetMode::Exact);
        let mut m = memory("u1", "listen on port 8081", "fact", MemoryState::Active);
        m.keywords = vec!["puerto".into(), "port".into()];
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![(0.0, m)]).unwrap();
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
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![(0.0, m)]).unwrap();
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
        let out = select_response(&budget, SUPPORTED_TOKENIZER_V1, vec![(0.0, m1)]).unwrap();
        assert!(out.items.is_empty());
        assert!(out.empty_reason.is_some());
    }
}
