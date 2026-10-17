//! Integration tests for the application service end-to-end against SQLite.

use mnexis_core::application::{
    ListFilter, MemoryService, SaveRequest, ServiceConfig, UpdateRequest,
};
use mnexis_core::domain::error::MnexisError;
use mnexis_core::domain::idempotency::IdempotencyKey;
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::scope::{AccessScope, Operation};
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use mnexis_core::storage::sqlite::open_sqlite;

fn make_service() -> (MemoryService<SqliteMemoryRepository>, tempfile::TempDir) {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&dir.path().join("svc.db")).unwrap();
    let repo = SqliteMemoryRepository::new(conn);
    let scope = AccessScope::new(
        vec!["proyecto-a".into()],
        vec!["proyecto-a".into()],
        "proyecto-a".into(),
        false,
        vec![
            Operation::Save,
            Operation::Get,
            Operation::List,
            Operation::Update,
            Operation::SoftDelete,
            Operation::Restore,
            Operation::History,
            Operation::Supersede,
            Operation::Search,
            Operation::Reindex,
        ],
    );
    let svc = MemoryService::new(repo, scope, ServiceConfig::default());
    (svc, dir)
}

fn save_req(uuid: &str, content: &str) -> SaveRequest {
    SaveRequest {
        uuid: uuid.into(),
        topic_key: format!("topic.{uuid}"),
        namespace: "proyecto-a".into(),
        memory_type: "fact".into(),
        content: content.into(),
        actor: "user".into(),
        channel: "cli".into(),
        assertion_basis: "user_stated".into(),
        valid_from: None,
        valid_until: None,
        tags: vec!["tag1".into(), "tag2".into()],
        idempotency_key: None,
        state: MemoryState::Active,
        now: "2026-01-01T00:00:00Z".into(),
    }
}

#[test]
fn save_and_get_round_trip() {
    let (mut svc, _dir) = make_service();
    let mem = svc.save(save_req("uuid-1", "hello")).unwrap();
    assert!(mem.id.unwrap_or(0) > 0);
    let fetched = svc.get("uuid-1").unwrap().unwrap();
    assert_eq!(fetched.content, "hello");
    assert_eq!(fetched.state, MemoryState::Active);
    assert_eq!(fetched.revision, 1);
}

#[test]
fn save_validates_content_length() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-2", "");
    req.content = "x".repeat(ServiceConfig::default().max_content_bytes + 1);
    let err = svc.save(req).unwrap_err();
    assert!(matches!(err, MnexisError::InvalidArgument { .. }));
}

#[test]
fn save_validates_topic_key_format() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-3", "ok");
    req.topic_key = "Has Spaces".into();
    let err = svc.save(req).unwrap_err();
    assert!(matches!(err, MnexisError::InvalidArgument { .. }));
}

#[test]
fn save_validates_tags_count() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-4", "ok");
    req.tags = (0..33).map(|i| format!("tag{i}")).collect();
    let err = svc.save(req).unwrap_err();
    assert!(matches!(err, MnexisError::InvalidArgument { .. }));
}

#[test]
fn save_duplicate_topic_key_returns_topic_conflict() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-a", "first")).unwrap();
    let mut dup = save_req("uuid-b", "second");
    dup.topic_key = "topic.uuid-a".into(); // same as the first save
    let err = svc.save(dup).unwrap_err();
    assert!(matches!(err, MnexisError::TopicConflict { .. }));
}

#[test]
fn list_returns_all_in_namespace() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "a")).unwrap();
    svc.save(save_req("uuid-2", "b")).unwrap();
    let list = svc.list(ListFilter::default()).unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn update_increments_revision_and_writes_history() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "original")).unwrap();
    let req = UpdateRequest {
        uuid: "uuid-1".into(),
        expected_revision: 1,
        content: Some("updated".into()),
        state: None,
        tags: None,
        assertion_basis: None,
        reason: "fix typo".into(),
        now: "2026-01-02T00:00:00Z".into(),
    };
    let updated = svc.update(req).unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.content, "updated");
    let history = svc.history("uuid-1").unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].revision, 2);
}

#[test]
fn update_with_wrong_revision_returns_conflict() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "original")).unwrap();
    let req = UpdateRequest {
        uuid: "uuid-1".into(),
        expected_revision: 99,
        content: Some("updated".into()),
        state: None,
        tags: None,
        assertion_basis: None,
        reason: "test".into(),
        now: "2026-01-02T00:00:00Z".into(),
    };
    let err = svc.update(req).unwrap_err();
    assert!(matches!(err, MnexisError::RevisionConflict { .. }));
}

#[test]
fn soft_delete_removes_from_get_results() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "data")).unwrap();
    svc.soft_delete("uuid-1", 1).unwrap();
    assert!(svc.get("uuid-1").unwrap().is_none());
}

#[test]
fn restore_brings_memory_back() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "data")).unwrap();
    svc.soft_delete("uuid-1", 1).unwrap();
    svc.restore("uuid-1").unwrap();
    let mem = svc.get("uuid-1").unwrap().unwrap();
    assert!(!mem.deleted);
    assert_eq!(mem.revision, 3); // 1, soft_delete(2), restore(3)
}

#[test]
fn save_with_idempotency_key_returns_same_id() {
    let (mut svc, _dir) = make_service();
    let idem = IdempotencyKey::new("idem-1");
    let mut req1 = save_req("uuid-1", "first");
    req1.idempotency_key = Some(idem.clone());
    let mut req2 = save_req("uuid-1", "first");
    req2.idempotency_key = Some(idem);
    let id1 = svc.save(req1).unwrap();
    let id2 = svc.save(req2).unwrap();
    assert_eq!(id1, id2);
}

#[test]
fn save_with_different_request_same_idempotency_key_returns_conflict() {
    let (mut svc, _dir) = make_service();
    let idem = IdempotencyKey::new("idem-1");
    let mut req1 = save_req("uuid-1", "first");
    req1.idempotency_key = Some(idem.clone());
    let _ = svc.save(req1).unwrap();
    // Second request: same idempotency key, different content, different topic_key
    // (so we hit the idempotency hash-mismatch branch, not the unique-constraint).
    let mut req2 = save_req("uuid-2", "second");
    req2.idempotency_key = Some(idem);
    let err = svc.save(req2).unwrap_err();
    assert!(matches!(err, MnexisError::TopicConflict { .. }));
}

#[test]
fn unauthorized_namespace_is_denied() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&dir.path().join("svc.db")).unwrap();
    let repo = SqliteMemoryRepository::new(conn);
    // Scope allows only "proyecto-b" (read+write), not "proyecto-a".
    let scope = AccessScope::new(
        vec!["proyecto-b".into()],
        vec!["proyecto-b".into()],
        "proyecto-b".into(),
        false,
        vec![Operation::Save, Operation::Get],
    );
    let mut svc = MemoryService::new(repo, scope, ServiceConfig::default());
    let err = svc.save(save_req("uuid-1", "x")).unwrap_err();
    // Default existence_disclosure: NotFound (avoids existence leak).
    assert!(matches!(err, MnexisError::NotFound { .. }));
}

#[test]
fn reindex_returns_count_of_active_non_deleted_memories() {
    let (mut svc, _dir) = make_service();
    // Two active memories — both must end up in FTS5 after reindex.
    svc.save(save_req("uuid-1", "first memory")).unwrap();
    svc.save(save_req("uuid-2", "second memory")).unwrap();

    let count = svc.reindex().expect("reindex must succeed");
    assert_eq!(
        count, 2,
        "both active memories must be reported as re-indexed"
    );
}

#[test]
fn reindex_is_denied_when_operation_not_in_scope() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&dir.path().join("svc.db")).unwrap();
    let repo = SqliteMemoryRepository::new(conn);
    // Scope omits `Operation::Reindex`.
    let scope = AccessScope::new(
        vec!["proyecto-a".into()],
        vec!["proyecto-a".into()],
        "proyecto-a".into(),
        false,
        vec![Operation::Save],
    );
    let mut svc = MemoryService::new(repo, scope, ServiceConfig::default());
    let err = svc.reindex().unwrap_err();
    assert!(
        matches!(err, MnexisError::Forbidden { .. }),
        "reindex without Operation::Reindex in scope must return Forbidden"
    );
}
