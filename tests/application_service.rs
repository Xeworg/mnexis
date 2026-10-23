//! Integration tests for the application service end-to-end against SQLite.

use mnexis_core::application::{
    ListFilter, MemoryService, SaveRequest, ServiceConfig, SupersedeRequest, UpdateRequest,
};
use mnexis_core::domain::error::MnexisError;
use mnexis_core::domain::idempotency::IdempotencyKey;
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::query::MatchMode;
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
fn save_topic_key_rejection_names_the_character_and_the_allowed_set() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-3b", "ok");
    req.topic_key = "snake design".into();
    let err = svc.save(req).unwrap_err();
    match err {
        MnexisError::InvalidArgument { field, message } => {
            assert_eq!(field, "topic_key");
            assert_eq!(
                message,
                "invalid character: ' ' (allowed: lowercase letters, digits, '.', '-' and '/')"
            );
        }
        other => panic!("expected InvalidArgument, got {other:?}"),
    }
    // The rejected save must not leave a record behind.
    assert!(svc.get("uuid-3b").unwrap().is_none());
}

#[test]
fn save_accepts_the_separator_an_agent_groups_facts_with() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-3c", "the fact");
    // The slash is how an agent writes a grouped key, and the other
    // engine's convention: it used to be refused.
    req.topic_key = "payments-api/server-config".into();
    let saved = svc.save(req).expect("a grouped topic key must be accepted");
    assert_eq!(saved.topic_key, "payments-api/server-config");
    let found = svc.get("uuid-3c").unwrap().expect("the memory is stored");
    assert_eq!(found.topic_key, "payments-api/server-config");
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
    assert_eq!(
        err,
        MnexisError::RevisionConflict {
            current_revision: 1
        },
        "the conflict carries the revision actually stored, not the expected one"
    );
    // The rejected write changed nothing.
    let stored = svc.get("uuid-1").unwrap().unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(stored.content, "original");
}

#[test]
fn links_read_returns_the_supersession_chain_for_both_sides() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "older")).unwrap();
    svc.save(save_req("uuid-2", "newer")).unwrap();
    svc.supersede(SupersedeRequest {
        older_uuid: "uuid-1".into(),
        newer_uuid: "uuid-2".into(),
        expected_old_revision: 1,
        expected_new_revision: 1,
        reason: "replaced".into(),
        now: "2026-01-02T00:00:00Z".into(),
    })
    .unwrap();

    let chain = svc.links("uuid-1").unwrap();
    assert_eq!(chain.len(), 1, "one supersession link: {chain:?}");
    assert!(chain[0].id.is_some(), "the persisted row id is exposed");
    assert_eq!(chain[0].older_uuid, "uuid-1");
    assert_eq!(chain[0].newer_uuid, "uuid-2");
    assert_eq!(chain[0].link_type, "supersedes");
    assert!(!chain[0].created_at.is_empty());
    // The newer side sees the same chain.
    assert_eq!(svc.links("uuid-2").unwrap(), chain);
    // A memory outside the chain has none.
    svc.save(save_req("uuid-3", "unrelated")).unwrap();
    assert!(svc.links("uuid-3").unwrap().is_empty());
    // A missing memory is a stable not-found, like every other read.
    assert!(matches!(
        svc.links("missing").unwrap_err(),
        MnexisError::NotFound { .. }
    ));
}

#[test]
fn tags_read_returns_persisted_tags_in_order() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "tagged")).unwrap();
    let tags: Vec<String> = svc
        .tags("uuid-1")
        .unwrap()
        .into_iter()
        .map(|t| t.tag)
        .collect();
    assert_eq!(tags, vec!["tag1", "tag2"]);
    assert!(matches!(
        svc.tags("missing").unwrap_err(),
        MnexisError::NotFound { .. }
    ));
}

#[test]
fn deleted_reads_expose_soft_deleted_records_for_restore() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "to delete")).unwrap();
    svc.save(save_req("uuid-2", "kept")).unwrap();
    svc.soft_delete("uuid-1", 1).unwrap();

    assert!(
        svc.get("uuid-1").unwrap().is_none(),
        "get stays deleted-blind"
    );
    let deleted = svc
        .get_including_deleted("uuid-1")
        .unwrap()
        .expect("the deleted row is visible to the deleted view");
    assert!(deleted.deleted);
    assert_eq!(deleted.revision, 2, "soft_delete bumped the revision");
    assert!(svc.get_including_deleted("missing").unwrap().is_none());

    let listed = svc.list_deleted(Some("proyecto-a"), 100, 0).unwrap();
    assert_eq!(listed.len(), 1, "only the deleted row: {listed:?}");
    assert_eq!(listed[0].uuid, "uuid-1");
    assert_eq!(
        svc.list_deleted(Some("proyecto-a"), 100, 0).unwrap().len(),
        1
    );
    assert!(
        matches!(
            svc.list_deleted(Some("other-ns"), 100, 0).unwrap_err(),
            MnexisError::NotFound { .. }
        ),
        "the deleted view keeps the same scope check as list (not-found disclosure)"
    );
    let live = svc
        .list(ListFilter {
            namespace: Some("proyecto-a".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(live.len(), 1, "list still hides the deleted row");

    // Tags and links stay readable while deleted: the deleted view renders
    // them before the user restores.
    assert_eq!(svc.tags("uuid-1").unwrap().len(), 2);
    assert!(svc.links("uuid-1").unwrap().is_empty());

    // Restore puts the row back in the live list.
    svc.restore("uuid-1").unwrap();
    assert!(svc.get("uuid-1").unwrap().is_some());
    assert!(
        svc.list_deleted(Some("proyecto-a"), 100, 0)
            .unwrap()
            .is_empty()
    );
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

// --- supersede contract v1 (odd/tasks/supersede-contract.md S1/S2) ---

fn supersede_req(older: &str, newer: &str, old_rev: i32, new_rev: i32) -> SupersedeRequest {
    SupersedeRequest {
        older_uuid: older.into(),
        newer_uuid: newer.into(),
        expected_old_revision: old_rev,
        expected_new_revision: new_rev,
        reason: "replaced by newer fact".into(),
        now: "2026-01-02T00:00:00Z".into(),
    }
}

fn seed_link(dir: &tempfile::TempDir, older: &str, newer: &str) {
    use mnexis_core::domain::memory::MemoryLink;
    use mnexis_core::domain::repository::MemoryRepository;
    let conn = open_sqlite(&dir.path().join("svc.db")).unwrap();
    let mut repo = SqliteMemoryRepository::new(conn);
    repo.save_link(&MemoryLink {
        id: None,
        older_uuid: older.into(),
        newer_uuid: newer.into(),
        link_type: "supersedes".into(),
        reason: String::new(),
        created_at: "2026-01-01T12:00:00Z".into(),
    })
    .unwrap();
}

#[test]
fn supersede_rejects_a_stale_expected_new_revision_and_writes_nothing() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("old-1", "older")).unwrap();
    svc.save(save_req("new-1", "newer")).unwrap();
    // A second writer moves the newer side to revision 2.
    svc.update(UpdateRequest {
        uuid: "new-1".into(),
        expected_revision: 1,
        content: Some("newer v2".into()),
        state: None,
        tags: None,
        assertion_basis: None,
        reason: "external edit".into(),
        now: "2026-01-02T00:00:00Z".into(),
    })
    .unwrap();

    let err = svc
        .supersede(supersede_req("old-1", "new-1", 1, 1))
        .unwrap_err();
    assert!(
        matches!(
            err,
            MnexisError::RevisionConflict {
                current_revision: 2
            }
        ),
        "stale newer revision must conflict with the disk revision: {err:?}"
    );
    // Nothing was written: the older side stays active at revision 1 and
    // no link exists.
    let older = svc.get("old-1").unwrap().unwrap();
    assert_eq!(older.state, MemoryState::Active);
    assert_eq!(older.revision, 1);
    assert!(svc.links("old-1").unwrap().is_empty());
}

#[test]
fn supersede_rejects_cross_namespace_pairs() {
    // A scope that may write two namespaces; supersede must still refuse
    // to link records across them (contract: same-namespace in MVP).
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&dir.path().join("svc.db")).unwrap();
    let repo = SqliteMemoryRepository::new(conn);
    let scope = AccessScope::new(
        vec!["ns-one".into(), "ns-two".into()],
        vec!["ns-one".into(), "ns-two".into()],
        "ns-one".into(),
        false,
        vec![
            Operation::Save,
            Operation::Get,
            Operation::List,
            Operation::Supersede,
        ],
    );
    let mut svc = MemoryService::new(repo, scope, ServiceConfig::default());

    let mut a = save_req("a-1", "in one");
    a.namespace = "ns-one".into();
    let mut b = save_req("b-1", "in two");
    b.namespace = "ns-two".into();
    svc.save(a).unwrap();
    svc.save(b).unwrap();

    let err = svc
        .supersede(supersede_req("a-1", "b-1", 1, 1))
        .unwrap_err();
    assert!(
        matches!(err, MnexisError::InvalidArgument { .. }),
        "cross-namespace supersede must be rejected: {err:?}"
    );
    let older = svc.get("a-1").unwrap().unwrap();
    assert_eq!(older.state, MemoryState::Active, "nothing written");
    assert!(svc.links("a-1").unwrap().is_empty());
}

#[test]
fn supersede_rejects_a_self_link() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("solo-1", "only")).unwrap();

    let err = svc
        .supersede(supersede_req("solo-1", "solo-1", 1, 1))
        .unwrap_err();
    assert!(
        matches!(err, MnexisError::InvalidArgument { .. }),
        "a memory cannot supersede itself: {err:?}"
    );
    let mem = svc.get("solo-1").unwrap().unwrap();
    assert_eq!(mem.state, MemoryState::Active, "nothing written");
    assert!(svc.links("solo-1").unwrap().is_empty());
}

#[test]
fn supersede_rejects_a_link_that_would_close_a_cycle() {
    let (mut svc, dir) = make_service();
    svc.save(save_req("cyc-a", "a")).unwrap();
    svc.save(save_req("cyc-b", "b")).unwrap();
    svc.save(save_req("cyc-c", "c")).unwrap();
    // Raw links seeded outside the service (e.g. imported): b supersedes a,
    // c supersedes b. All three rows stay active, so the state gate alone
    // cannot catch the cycle the next link would close.
    seed_link(&dir, "cyc-a", "cyc-b");
    seed_link(&dir, "cyc-b", "cyc-c");

    // a->b->c exists; linking c under a (a supersedes c) closes the cycle.
    let err = svc
        .supersede(supersede_req("cyc-c", "cyc-a", 1, 1))
        .unwrap_err();
    assert!(
        matches!(err, MnexisError::InvalidArgument { .. }),
        "cycle-closing supersede must be rejected: {err:?}"
    );
    let older = svc.get("cyc-c").unwrap().unwrap();
    assert_eq!(older.state, MemoryState::Active, "nothing written");
    assert_eq!(svc.links("cyc-c").unwrap().len(), 1, "only the seeded link");
}

#[test]
fn supersede_persists_the_reason_on_the_link() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("r-old", "older")).unwrap();
    svc.save(save_req("r-new", "newer")).unwrap();

    svc.supersede(supersede_req("r-old", "r-new", 1, 1))
        .unwrap();
    let links = svc.links("r-old").unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].reason, "replaced by newer fact",
        "the request reason must be persisted on the link row"
    );
    let older = svc.get("r-old").unwrap().unwrap();
    assert_eq!(older.state, MemoryState::Superseded);
}

#[test]
fn supersede_rejects_when_the_older_side_is_not_active() {
    // Pre-existing gate, now pinned because supersede was rewritten around
    // it (T5 durable probe): a non-active older side must be rejected
    // before anything is written.
    let (mut svc, _dir) = make_service();
    let mut older = save_req("st-old", "older");
    older.state = MemoryState::Proposed;
    svc.save(older).unwrap();
    svc.save(save_req("st-new", "newer")).unwrap();

    let err = svc
        .supersede(supersede_req("st-old", "st-new", 1, 1))
        .unwrap_err();
    match &err {
        MnexisError::InvalidArgument { field, message } => {
            assert_eq!(field, "older");
            assert!(message.contains("must be active, is proposed"), "{message}");
        }
        other => panic!("expected InvalidArgument, got {other:?}"),
    }
    let mem = svc.get("st-old").unwrap().unwrap();
    assert_eq!(mem.state, MemoryState::Proposed, "nothing written");
    assert_eq!(mem.revision, 1);
    assert!(svc.links("st-old").unwrap().is_empty());
}

// --- sessions and prompts ---

#[test]
fn a_session_starts_ends_and_keeps_its_summary() {
    let (mut svc, _dir) = make_service();
    let started = svc
        .start_session("01JX", None, "/work/project", "2026-01-01T00:00:00Z")
        .expect("the session starts");
    assert_eq!(started.id, "01JX");
    assert_eq!(started.ended_at, None);

    let ended = svc
        .end_session("01JX", Some("what the session did"), "2026-01-01T01:00:00Z")
        .expect("the session ends");
    assert_eq!(ended.ended_at.as_deref(), Some("2026-01-01T01:00:00Z"));
    assert_eq!(ended.summary.as_deref(), Some("what the session did"));

    // The session is still there, and it ended once.
    let again = svc
        .end_session("01JX", None, "2026-01-01T02:00:00Z")
        .expect("ending twice keeps the first end");
    assert_eq!(again.ended_at.as_deref(), Some("2026-01-01T01:00:00Z"));
    assert_eq!(again.summary.as_deref(), Some("what the session did"));
}

#[test]
fn starting_a_session_twice_keeps_the_first_one() {
    let (mut svc, _dir) = make_service();
    svc.start_session("01JX", None, "/work/project", "2026-01-01T00:00:00Z")
        .expect("the session starts");
    let again = svc
        .start_session("01JX", None, "/other", "2026-01-02T00:00:00Z")
        .expect("starting again is not an error");
    assert_eq!(again.started_at, "2026-01-01T00:00:00Z");
    assert_eq!(again.directory, "/work/project");
}

#[test]
fn ending_a_session_that_does_not_exist_is_not_found() {
    let (mut svc, _dir) = make_service();
    let error = svc
        .end_session("nope", None, "2026-01-01T00:00:00Z")
        .unwrap_err();
    assert!(matches!(error, MnexisError::NotFound { .. }));
}

#[test]
fn a_prompt_is_stored_and_found_again() {
    let (mut svc, _dir) = make_service();
    svc.start_session("01JX", None, "/work/project", "2026-01-01T00:00:00Z")
        .expect("the session starts");
    let prompt = svc
        .save_prompt(
            "el puerto del servicio es 8081",
            Some("01JX"),
            None,
            "2026-01-01T00:05:00Z",
        )
        .expect("the prompt is stored");
    assert!(prompt.id > 0);
    assert_eq!(prompt.session_id.as_deref(), Some("01JX"));

    let hits = svc
        .search_prompts("puerto 8081", 10, MatchMode::Any, None)
        .expect("the search runs");
    assert_eq!(hits.len(), 1, "the prompt is found: {hits:?}");
    assert_eq!(hits[0].content, "el puerto del servicio es 8081");
}

#[test]
fn a_hyphenated_prompt_query_still_finds_the_prompt() {
    let (mut svc, _dir) = make_service();
    svc.save_prompt(
        "el deploy por defecto es staging-eu",
        None,
        None,
        "2026-01-01T00:05:00Z",
    )
    .expect("the prompt is stored");
    let hits = svc
        .search_prompts("staging-eu", 10, MatchMode::Any, None)
        .expect("the search runs");
    assert_eq!(hits.len(), 1, "the hyphen is part of a word: {hits:?}");
}

#[test]
fn a_prompt_outside_the_write_allowlist_is_refused() {
    let (mut svc, _dir) = make_service();
    let error = svc
        .save_prompt("content", None, Some("otro"), "2026-01-01T00:05:00Z")
        .unwrap_err();
    // The refusal is the one a memory write would get: the profile decides
    // whether an unknown namespace is disclosed or hidden.
    let memory_error = svc
        .save(SaveRequest {
            uuid: "uuid-refused".into(),
            namespace: "otro".into(),
            content: "contenido".into(),
            ..save_req("uuid-refused", "ok")
        })
        .unwrap_err();
    assert_eq!(
        error.error_code_label(),
        memory_error.error_code_label(),
        "a prompt is refused the way a memory is: {error:?} vs {memory_error:?}"
    );
    assert!(
        matches!(
            error,
            MnexisError::Forbidden { .. } | MnexisError::NotFound { .. }
        ),
        "{error:?}"
    );
}
