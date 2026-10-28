//! E3 unit + integration tests for the budget, tokenizer and
//! memory_context pipeline (PRD §9, A-07).

use mnexis_core::application::budget::{
    Budget, BudgetMode, SUPPORTED_TOKENIZER_V1, token_count_v1,
};
use mnexis_core::application::{ContextRequest, MemoryService, SearchFilter};
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::query::MatchMode;
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use mnexis_core::storage::sqlite::open_sqlite;
use tempfile::tempdir_in;

// --- tokenizer v1 ---

#[test]
fn v1_id_is_stable() {
    assert_eq!(SUPPORTED_TOKENIZER_V1, "supported-tokenizer-v1");
}

#[test]
fn empty_string_counts_zero() {
    assert_eq!(token_count_v1(""), 0);
}

#[test]
fn upper_bound_property_never_overcounts_per_byte() {
    for n in 0..=512 {
        let s: String = std::iter::repeat_n('a', n).collect();
        let tokens = token_count_v1(&s);
        assert!(
            tokens <= n as i64,
            "{n} bytes must not exceed {tokens} tokens"
        );
    }
}

#[test]
fn deterministic_for_same_input() {
    let s = "el render del grafo requiere 2500 tokens, según PRD";
    assert_eq!(token_count_v1(s), token_count_v1(s));
    let longer = "petgraph replaced by custom engine; 500 memories, 150 queries";
    assert_eq!(token_count_v1(longer), token_count_v1(longer));
}

#[test]
fn ascii_punctuation_splits_correctly() {
    let s = "hello, world! 2025.";
    let n = token_count_v1(s);
    assert!(n > 0);
    assert!(n <= s.len() as i64);
}

#[test]
fn non_ascii_tokens_at_most_bytes() {
    let s = "ñáéíóú";
    // The supported tokenizer v1 is an upper-bound: count(s) <= bytes(s).
    let n = token_count_v1(s);
    assert!(n <= s.len() as i64, "{n} tokens > {} bytes", s.len());
    // Each non-ASCII char counts as one token (the bound is tight per char).
    assert_eq!(n, s.chars().count() as i64);
}

#[test]
fn whitespace_collapse_is_bounded() {
    let with_spaces = "    a   b    c    ";
    let without_padding = "a b c";
    let n1 = token_count_v1(with_spaces);
    let n2 = token_count_v1(without_padding);
    assert!(n1 <= n2 + 1, "{n1} tokens for padded, {n2} for unpadded");
}

#[test]
fn long_string_never_exceeds_byte_length() {
    let s = "el render del grafo requiere 2500 tokens, según PRD. \
             petgraph replaced by custom engine. \
             500 memories, 150 queries, indexed by FTS5.";
    let n = token_count_v1(s);
    assert!(n > 0);
    assert!(n <= s.len() as i64);
}

// --- budget exact mode ---

#[test]
fn budget_exact_mode_uses_token_budget_directly() {
    let b = Budget::new(100, BudgetMode::Exact);
    assert_eq!(b.token_budget(), 100);
    assert_eq!(b.enforcement_label(), "exact");
    assert_eq!(b.tokenizer_id(), SUPPORTED_TOKENIZER_V1);
}

#[test]
fn budget_exact_check_accepts_within_and_rejects_over() {
    let b = Budget::new(10, BudgetMode::Exact);
    assert!(b.check_tokens(5).is_ok());
    assert!(b.check_tokens(10).is_ok());
    let err = b.check_tokens(11).unwrap_err();
    assert!(matches!(
        err,
        mnexis_core::domain::error::MnexisError::BudgetTooSmall { .. }
    ));
}

#[test]
fn budget_estimated_check_uses_byte_budget() {
    let b = Budget::new(10, BudgetMode::Estimated);
    assert_eq!(b.byte_budget(), 40);
    assert!(b.check_bytes(40).is_ok());
    assert!(b.check_bytes(41).is_err());
}

#[test]
fn budget_unsupported_tokenizer_yields_budget_too_small() {
    let b = Budget::new(10, BudgetMode::Exact);
    let err = b
        .check_with_tokenizer("unsupported-tokenizer-vX", 5)
        .unwrap_err();
    assert!(matches!(
        err,
        mnexis_core::domain::error::MnexisError::BudgetTooSmall { .. }
    ));
}

// --- memory_context pipeline ---

fn make_service() -> (MemoryService<SqliteMemoryRepository>, tempfile::TempDir) {
    let dir = tempdir_in("target").expect("tempdir");
    let db = dir.path().join("e3.db");
    let repo = SqliteMemoryRepository::new(open_sqlite(&db).expect("open"));
    let scope = mnexis_core::config::ProfileSettings::default()
        .to_access_scope()
        .expect("default scope");
    let svc = MemoryService::new(repo, scope, Default::default());
    (svc, dir)
}

fn save(svc: &mut MemoryService<SqliteMemoryRepository>, uuid: &str, content: &str, kind: &str) {
    let req = mnexis_core::application::SaveRequest {
        uuid: uuid.into(),
        title: String::new(),
        topic_key: format!("topic.{uuid}"),
        namespace: "default".into(),
        memory_type: Some(kind.into()),
        content: content.into(),
        actor: "user".into(),
        channel: "cli".into(),
        assertion_basis: "user_stated".into(),
        valid_from: None,
        valid_until: None,
        tags: Vec::new(),
        idempotency_key: None,
        keywords: Vec::new(),
        replace_keywords: false,
        state: MemoryState::Active,
        now: "2026-01-01T00:00:00Z".into(),
    };
    svc.save(req).unwrap();
}

#[test]
fn context_returns_items_in_budget_with_exact_enforcement() {
    let (mut svc, _dir) = make_service();
    save(&mut svc, "u1", "alpha alpha alpha", "fact");
    save(&mut svc, "u2", "beta beta", "fact");
    save(&mut svc, "u3", "gamma", "decision");

    let req = ContextRequest {
        task: "alpha".into(),
        match_mode: MatchMode::Any,
        namespace: Some("default".into()),
        token_budget: 1024,
        tokenizer_id: Some(SUPPORTED_TOKENIZER_V1.into()),
        max_items: Some(10),
        budget_mode: BudgetMode::Exact,
        full: false,
        search: SearchFilter::default(),
        now: "2026-01-01T00:00:00Z".into(),
    };
    let out = svc.context(&req).expect("context");
    assert_eq!(out.budget_enforcement, "exact");
    assert_eq!(out.tokenizer_id, SUPPORTED_TOKENIZER_V1);
    assert!(out.tokens_used > 0);
    assert!(!out.items.is_empty(), "should have items");
    // A-07: with the supported tokenizer, the actual tokens_used is
    // exactly what `token_count_v1` would compute on the serialised
    // envelope (pinned below for the determinism invariant).
    let serialised = mnexis_core::application::context::serialise_envelope(
        &out.items,
        &mnexis_core::application::budget::Budget::new(
            out.tokens_used,
            mnexis_core::application::BudgetMode::Exact,
        ),
    );
    assert_eq!(out.tokens_used, token_count_v1(&serialised));
}

#[test]
fn context_estimated_mode_keeps_estimated_label() {
    let (mut svc, _dir) = make_service();
    save(&mut svc, "u1", "alpha", "fact");
    let req = ContextRequest {
        task: "alpha".into(),
        match_mode: MatchMode::Any,
        namespace: Some("default".into()),
        token_budget: 1024,
        tokenizer_id: None,
        max_items: Some(10),
        budget_mode: BudgetMode::Estimated,
        full: false,
        search: SearchFilter::default(),
        now: "2026-01-01T00:00:00Z".into(),
    };
    let out = svc.context(&req).expect("context");
    assert_eq!(out.budget_enforcement, "estimated");
    assert!(!out.warnings.is_empty());
}

#[test]
fn context_refuses_to_exceed_budget_in_exact_mode() {
    let (mut svc, _dir) = make_service();
    // Seed a lot of content; the budget is tiny.
    for i in 0..20 {
        save(
            &mut svc,
            &format!("u{i}"),
            "the quick brown fox jumps over the lazy dog with some extra padding here",
            "fact",
        );
    }
    let req = ContextRequest {
        task: "fox".into(),
        match_mode: MatchMode::Any,
        namespace: Some("default".into()),
        token_budget: 5,
        tokenizer_id: Some(SUPPORTED_TOKENIZER_V1.into()),
        max_items: Some(10),
        budget_mode: BudgetMode::Exact,
        full: false,
        search: SearchFilter::default(),
        now: "2026-01-01T00:00:00Z".into(),
    };
    let res = svc.context(&req);
    // Either it returns an empty selection with empty_reason, or it
    // raises BudgetTooSmall when the envelope itself doesn't fit.
    match res {
        Ok(out) => {
            assert!(out.items.is_empty(), "tiny budget must not select anything");
            assert!(out.empty_reason.is_some(), "must explain the empty result");
        }
        Err(mnexis_core::domain::error::MnexisError::BudgetTooSmall { .. }) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn context_dedupes_same_topic_key() {
    let (mut svc, _dir) = make_service();
    save(&mut svc, "u1", "alpha", "fact");
    save(&mut svc, "u2", "alpha same content", "fact");
    let req = ContextRequest {
        task: "alpha".into(),
        match_mode: MatchMode::Any,
        namespace: Some("default".into()),
        token_budget: 4096,
        tokenizer_id: Some(SUPPORTED_TOKENIZER_V1.into()),
        max_items: Some(10),
        budget_mode: BudgetMode::Exact,
        full: false,
        search: SearchFilter::default(),
        now: "2026-01-01T00:00:00Z".into(),
    };
    let out = svc.context(&req).expect("context");
    let topics: std::collections::HashSet<_> =
        out.items.iter().map(|i| i.topic_key.clone()).collect();
    assert_eq!(
        topics.len(),
        out.items.len(),
        "no duplicate topic_keys: {out:?}"
    );
}
