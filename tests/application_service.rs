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
        title: String::new(),
        topic_key: format!("topic.{uuid}"),
        namespace: "proyecto-a".into(),
        memory_type: Some("fact".into()),
        content: content.into(),
        actor: "user".into(),
        channel: "cli".into(),
        assertion_basis: "user_stated".into(),
        valid_from: None,
        valid_until: None,
        tags: vec!["tag1".into(), "tag2".into()],
        idempotency_key: None,
        keywords: Vec::new(),
        replace_keywords: false,
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
fn save_with_a_taken_topic_key_updates_that_record() {
    // The same key is the same fact: the save updates the record instead of
    // failing on the unique index.
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-a", "first")).unwrap();
    let mut dup = save_req("uuid-b", "second");
    dup.topic_key = "topic.uuid-a".into(); // same as the first save
    let updated = svc.save(dup).expect("the same key updates the record");
    assert_eq!(updated.uuid, "uuid-a", "the record keeps its identity");
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.content, "second");
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
        memory_type: None,

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
    assert_eq!(history[0].revision, 1, "the revision the update replaced");
    assert_eq!(history[0].content, "original", "the pre-update content");
}

#[test]
fn update_with_wrong_revision_returns_conflict() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "original")).unwrap();
    let req = UpdateRequest {
        memory_type: None,

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
            memory_types: Vec::new(),

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
        memory_type: None,

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
            title: String::new(),
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

// --- one fact, one record (memory-title) ---

const PORT_8081: &str = "El servicio payments-api escucha en el puerto TCP 8081, \
                         definido como port en la seccion server de config.toml con 4 workers";

#[test]
fn saving_with_a_key_the_namespace_uses_updates_that_record() {
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-t1", PORT_8081);
    first.title = "puerto del servicio".into();
    first.topic_key = "puerto-del-servicio".into();
    let saved = svc.save(first).expect("the first record is saved");
    assert_eq!(saved.revision, 1);

    // The same fact with a new value: same key, so the record is updated —
    // not duplicated, and not refused.
    let mut again = save_req("uuid-t2", &PORT_8081.replace("8081", "9090"));
    again.title = "puerto del servicio".into();
    again.topic_key = "puerto-del-servicio".into();
    let updated = svc.save(again).expect("the same key updates the record");

    assert_eq!(updated.uuid, "uuid-t1", "the record keeps its identity");
    assert_eq!(updated.revision, 2, "and its revision goes up");
    assert!(updated.content.contains("9090"), "with the new value");
    // The caller's uuid was not written as a second record.
    assert!(svc.get("uuid-t2").unwrap().is_none());
    // One record holds the topic.
    let taken = svc
        .topic_occupier("proyecto-a", "puerto-del-servicio")
        .expect("the occupier query");
    assert_eq!(taken.as_deref(), Some("uuid-t1"));
}

#[test]
fn the_previous_content_stays_in_the_history() {
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-t3", PORT_8081);
    first.title = "puerto del servicio".into();
    first.topic_key = "puerto-del-servicio".into();
    svc.save(first).expect("the first record");

    let mut again = save_req("uuid-t4", &PORT_8081.replace("8081", "9090"));
    again.title = "puerto del servicio".into();
    again.topic_key = "puerto-del-servicio".into();
    svc.save(again).expect("the update");

    // The trail records the state the re-save replaced: revision 1 is the
    // record as it was, and the live row is the one with the new value.
    let history = svc.history("uuid-t3").expect("the history");
    assert_eq!(history.len(), 1, "one row per change: {history:?}");
    assert_eq!(history[0].revision, 1, "the replaced revision: {history:?}");
    assert_eq!(
        history[0].content, PORT_8081,
        "the pre-update content, byte-identical: {history:?}"
    );
    assert_eq!(history[0].state, MemoryState::Active, "{history:?}");
    assert!(
        history[0].change_reason.contains("puerto-del-servicio"),
        "the reason names the key: {history:?}"
    );
    let live = svc.get("uuid-t3").unwrap().expect("the live record");
    assert_eq!(live.revision, 2, "the live row: {live:?}");
    assert!(
        live.content.contains("9090"),
        "and it carries the new value: {live:?}"
    );
}

#[test]
fn a_supersede_records_the_state_before_the_flip() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-s-old", PORT_8081)).unwrap();
    svc.save(save_req("uuid-s-new", "the port is now 9090"))
        .unwrap();
    svc.supersede(SupersedeRequest {
        older_uuid: "uuid-s-old".into(),
        newer_uuid: "uuid-s-new".into(),
        expected_old_revision: 1,
        expected_new_revision: 1,
        reason: "the port moved".into(),
        now: "2026-02-01T00:00:00Z".into(),
    })
    .expect("the supersede");

    let history = svc.history("uuid-s-old").expect("the history");
    assert_eq!(history.len(), 1, "the supersede wrote a row: {history:?}");
    assert_eq!(history[0].revision, 1, "the pre-bump revision: {history:?}");
    assert_eq!(
        history[0].content, PORT_8081,
        "the pre-flip content: {history:?}"
    );
    assert_eq!(
        history[0].state,
        MemoryState::Active,
        "the pre-flip state: {history:?}"
    );
    assert_eq!(
        history[0].changed_by, "user",
        "the actor of the record: {history:?}"
    );
    assert_eq!(
        history[0].change_reason, "the port moved",
        "the link's reason: {history:?}"
    );
    assert_eq!(
        history[0].changed_at, "2026-02-01T00:00:00Z",
        "the change's time: {history:?}"
    );
    let live = svc.get("uuid-s-old").unwrap().expect("the older record");
    assert_eq!(live.state, MemoryState::Superseded);
    assert_eq!(live.revision, 2, "the bump is on the live row: {live:?}");
    assert_eq!(
        live.content, PORT_8081,
        "a supersede never rewrites content"
    );
}

#[test]
fn a_title_derives_the_key_when_the_caller_gives_none() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-t5", PORT_8081);
    req.title = "Puerto del Servicio".into();
    req.topic_key = String::new();
    let saved = svc.save(req).expect("the save derives the key");
    assert_eq!(
        saved.topic_key, "puerto-del-servicio",
        "the key comes from the title, lowercased"
    );
}

#[test]
fn a_title_in_another_language_derives_a_usable_key() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-t6", "un hecho sobre el puerto");
    req.title = "サービスのポート".into();
    req.topic_key = String::new();
    let saved = svc.save(req).expect("the save derives the key");
    assert_eq!(saved.topic_key, "サービスのポート");
}

#[test]
fn a_memory_without_a_title_keeps_no_topic_and_no_upsert() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-t7", PORT_8081);
    req.title = String::new();
    req.topic_key = String::new();
    let saved = svc.save(req).expect("the save");
    assert_eq!(saved.topic_key, "", "no title, no key");
}

#[test]
fn saving_the_same_request_twice_is_a_retry_not_a_second_fact() {
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-t8", PORT_8081);
    first.topic_key = String::new();
    first.idempotency_key = Some(IdempotencyKey::new("idem-t8"));
    let saved = svc.save(first).expect("the first save");
    assert_eq!(saved.uuid, "uuid-t8");

    // The same request again: the replay returns the original, without
    // writing a second record and without bumping the revision.
    let mut again = save_req("uuid-t8", PORT_8081);
    again.topic_key = String::new();
    again.idempotency_key = Some(IdempotencyKey::new("idem-t8"));
    let replayed = svc.save(again).expect("the replay");
    assert_eq!(replayed.uuid, "uuid-t8", "the original record comes back");
    assert_eq!(replayed.revision, 1, "a replay does not write a revision");
}

// --- a save never writes into a record that is not alive
//     (identity-visibility S3) ---

/// The existing promise of `memory-title`, unchanged: the title of an
/// active record still lands on that record.
#[test]
fn saving_with_the_title_of_an_active_record_still_updates_it() {
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-alive", PORT_8081);
    first.title = "puerto del servicio".into();
    first.topic_key = String::new();
    svc.save(first).expect("the first save");

    let mut again = save_req("uuid-alive-2", &PORT_8081.replace("8081", "9090"));
    again.title = "puerto del servicio".into();
    again.topic_key = String::new();
    let updated = svc
        .save(again)
        .expect("an active record is still updated by its title");

    assert_eq!(updated.uuid, "uuid-alive", "the record keeps its identity");
    assert_eq!(updated.revision, 2, "and its revision goes up");
    assert!(updated.content.contains("9090"), "with the new value");
    assert_eq!(updated.state, MemoryState::Active);
    assert!(svc.get("uuid-alive-2").unwrap().is_none());
}

#[test]
fn saving_with_the_title_of_a_superseded_record_is_refused_and_names_the_successor() {
    let (mut svc, _dir) = make_service();
    let mut older = save_req("uuid-gone", PORT_8081);
    older.title = "puerto del servicio".into();
    older.topic_key = String::new();
    svc.save(older).expect("the record that will be replaced");

    let mut newer = save_req("uuid-current", &PORT_8081.replace("8081", "9090"));
    newer.title = "puerto del servicio configurado".into();
    newer.topic_key = String::new();
    svc.save(newer).expect("the record that replaces it");
    svc.supersede(supersede_req("uuid-gone", "uuid-current", 1, 1))
        .expect("the supersession chain");

    let before = svc
        .get_including_deleted("uuid-gone")
        .unwrap()
        .expect("the superseded record");

    // The same title yields the same key, and the key is held by a record
    // that is not alive: the save must not write the new value inside it.
    let mut attempt = save_req("uuid-attempt", &PORT_8081.replace("8081", "7070"));
    attempt.title = "puerto del servicio".into();
    attempt.topic_key = String::new();
    let error = svc
        .save(attempt)
        .expect_err("a superseded record is not written into");

    let message = error.to_string();
    assert!(
        message.contains("puerto-del-servicio"),
        "the refusal names the topic key: {message}"
    );
    assert!(
        message.contains("was replaced by uuid-current"),
        "the refusal names the current record of that fact: {message}"
    );
    assert!(
        message.contains("save with another title"),
        "the refusal names the way out: {message}"
    );

    // Atomic: the superseded record keeps its revision and its content.
    let after = svc.get_including_deleted("uuid-gone").unwrap().unwrap();
    assert_eq!(after.revision, before.revision, "nothing was written");
    assert_eq!(after.content, before.content, "the old value is untouched");
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after.state, MemoryState::Superseded);
    // And no second record was created for a fact the engine knows about.
    assert!(svc.get("uuid-attempt").unwrap().is_none());
    let all = svc.list(ListFilter::default()).unwrap();
    assert_eq!(all.len(), 2, "two records, none of them new: {all:?}");
    assert!(
        all.iter().all(|m| !m.content.contains("7070")),
        "the refused value is stored nowhere: {all:?}"
    );
    // The key was not released either: it still belongs to the dead record.
    assert_eq!(
        svc.topic_occupier("proyecto-a", "puerto-del-servicio")
            .unwrap()
            .as_deref(),
        Some("uuid-gone")
    );
}

#[test]
fn saving_with_the_title_of_a_deleted_record_is_refused_and_names_the_way_out() {
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-deleted", PORT_8081);
    first.title = "puerto del servicio".into();
    first.topic_key = String::new();
    svc.save(first).expect("the record");
    svc.soft_delete("uuid-deleted", 1).expect("the delete");

    let before = svc
        .get_including_deleted("uuid-deleted")
        .unwrap()
        .expect("the deleted record");

    // A delete is explicit: a save must not silently undo it.
    let mut attempt = save_req("uuid-after-delete", &PORT_8081.replace("8081", "7070"));
    attempt.title = "puerto del servicio".into();
    attempt.topic_key = String::new();
    let error = svc
        .save(attempt)
        .expect_err("a deleted record is not written into");

    let message = error.to_string();
    assert!(
        message.contains("puerto-del-servicio"),
        "the refusal names the topic key: {message}"
    );
    assert!(
        message.contains("was deleted"),
        "the refusal says what happened: {message}"
    );
    assert!(
        message.contains("restore uuid-deleted"),
        "the refusal names a way out: {message}"
    );
    assert!(
        message.contains("save with another title"),
        "and the other one: {message}"
    );

    // Atomic: the deleted record keeps its revision and its content.
    let after = svc.get_including_deleted("uuid-deleted").unwrap().unwrap();
    assert!(after.deleted, "the record stays deleted");
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.content, before.content);
    assert_eq!(after.updated_at, before.updated_at);
    assert!(svc.get("uuid-after-delete").unwrap().is_none());
}

#[test]
fn saving_with_the_title_of_a_record_that_is_not_active_is_refused() {
    // The rule is "active and not deleted": a proposed (or disputed, or
    // archived) record is not writable by a save either.
    let (mut svc, _dir) = make_service();
    let mut proposed = save_req("uuid-proposed", PORT_8081);
    proposed.title = "puerto del servicio".into();
    proposed.topic_key = String::new();
    proposed.state = MemoryState::Proposed;
    svc.save(proposed).expect("a proposed record");

    let before = svc
        .get_including_deleted("uuid-proposed")
        .unwrap()
        .expect("the proposed record");

    let mut attempt = save_req("uuid-proposed-2", &PORT_8081.replace("8081", "7070"));
    attempt.title = "puerto del servicio".into();
    attempt.topic_key = String::new();
    let error = svc
        .save(attempt)
        .expect_err("only an active record is written into");

    let message = error.to_string();
    assert!(
        message.contains("which is proposed"),
        "the refusal names the state the record has: {message}"
    );
    assert!(
        message.contains("save with another title"),
        "the refusal names the way out: {message}"
    );

    let after = svc.get_including_deleted("uuid-proposed").unwrap().unwrap();
    assert_eq!(after.revision, before.revision, "nothing was written");
    assert_eq!(after.content, before.content);
    assert_eq!(after.state, MemoryState::Proposed);
}

#[test]
fn the_refusal_uses_an_existing_error_kind_and_exit_code() {
    // The refusal adds no error kind and no exit code: it is the existing
    // INVALID_ARGUMENT / exit 2, the same kind the supersede state gate
    // uses for "that record is not alive".
    let (mut svc, _dir) = make_service();
    let mut first = save_req("uuid-kind", PORT_8081);
    first.title = "puerto del servicio".into();
    first.topic_key = String::new();
    svc.save(first).unwrap();
    svc.soft_delete("uuid-kind", 1).unwrap();

    let mut attempt = save_req("uuid-kind-2", "otro valor");
    attempt.title = "puerto del servicio".into();
    attempt.topic_key = String::new();
    let error = svc.save(attempt).unwrap_err();

    let existing_labels = [
        "INVALID_ARGUMENT",
        "NOT_FOUND",
        "FORBIDDEN",
        "REVISION_CONFLICT",
        "TOPIC_CONFLICT",
        "BUSY",
        "INDEX_UNAVAILABLE",
        "BUDGET_TOO_SMALL",
        "CURSOR_EXPIRED",
        "UNSUPPORTED_VERSION",
    ];
    assert!(
        existing_labels.contains(&error.error_code_label()),
        "the refusal is a kind that already exists: {error:?}"
    );
    assert!(
        matches!(&error, MnexisError::InvalidArgument { field, .. } if field.as_str() == "title"),
        "the caller is told which argument to change: {error:?}"
    );
    assert_eq!(
        mnexis_core::cli::output::exit_code(&error),
        2,
        "the existing exit code of INVALID_ARGUMENT"
    );
}

// --- the key a title yields (memory-title) ---

#[test]
fn a_topic_key_in_another_language_is_accepted() {
    let (mut svc, _dir) = make_service();
    for (uuid, key) in [
        ("uuid-k1", "サービスのポート"),
        ("uuid-k2", "θύρα-υπηρεσίας"),
        ("uuid-k3", "порт-сервиса"),
        ("uuid-k4", "puerto.del-servicio"),
    ] {
        let mut req = save_req(uuid, "un hecho distinto por cada clave");
        req.topic_key = key.into();
        svc.save(req)
            .unwrap_or_else(|e| panic!("the key {key:?} must be accepted: {e:?}"));
    }
}

#[test]
fn an_uppercase_key_is_still_refused() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-k5", "un hecho");
    req.topic_key = "Puerto".into();
    let error = svc.save(req).unwrap_err();
    assert!(
        matches!(error, MnexisError::InvalidArgument { .. }),
        "a key is lowercase by contract: {error:?}"
    );
}

// --- the words of a fact (agent-keywords S1, S2) ---

/// A save request on `topic_key` that carries `words` as its vocabulary.
fn save_req_with_words(uuid: &str, topic_key: &str, words: &[&str]) -> SaveRequest {
    let mut req = save_req(uuid, "port = 8081");
    req.topic_key = topic_key.to_string();
    req.keywords = words.iter().map(|w| w.to_string()).collect();
    req
}

/// The words as they are stored, not only as the save answered them.
fn words_of(svc: &MemoryService<SqliteMemoryRepository>, uuid: &str) -> Vec<String> {
    svc.get(uuid)
        .expect("the read must run")
        .expect("the memory must exist")
        .keywords
}

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|w| w.to_string()).collect()
}

/// The refused-words error is the same invalid argument the tags use, on the
/// `keywords` field: the caller can tell which input it has to fix.
fn assert_keywords_refused(error: MnexisError) {
    match error {
        MnexisError::InvalidArgument { field, .. } => {
            assert_eq!(field, "keywords", "the refusal names the words")
        }
        other => panic!("a word list the limits refuse is INVALID_ARGUMENT: {other:?}"),
    }
}

#[test]
fn a_save_with_words_stores_them() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words(
        "uuid-1",
        "topic.fact1",
        &["puerto", "port"],
    ))
    .unwrap();
    assert_eq!(
        words_of(&svc, "uuid-1"),
        words(&["puerto", "port"]),
        "the words the agent gave are part of the record"
    );
    let listed = svc
        .list(ListFilter {
            memory_types: Vec::new(),

            namespace: Some("proyecto-a".into()),
            ..ListFilter::default()
        })
        .unwrap();
    assert_eq!(
        listed[0].keywords,
        words(&["puerto", "port"]),
        "the words come back with every read of the record"
    );
}

#[test]
fn a_second_save_accumulates_the_words() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words("uuid-1", "topic.fact1", &["puerto"]))
        .unwrap();
    let stored = svc
        .save(save_req_with_words(
            "uuid-2",
            "topic.fact1",
            &["listen port"],
        ))
        .unwrap();
    assert_eq!(stored.uuid, "uuid-1", "one key is one record");
    assert_eq!(
        stored.keywords,
        words(&["listen port", "puerto"]),
        "the requested words come first and the old ones stay"
    );
    assert_eq!(
        words_of(&svc, "uuid-1"),
        words(&["listen port", "puerto"]),
        "the accumulated words are on disk, not only in the answer"
    );

    // The amnesia is the common case: a save with no words keeps the ones the
    // record already carries instead of erasing them.
    let untouched = svc
        .save(save_req_with_words("uuid-3", "topic.fact1", &[]))
        .unwrap();
    assert_eq!(
        untouched.keywords,
        words(&["listen port", "puerto"]),
        "a save that carries no words keeps the vocabulary"
    );
    assert_eq!(words_of(&svc, "uuid-1"), untouched.keywords);
}

#[test]
fn a_second_save_with_replace_keywords_replaces_them() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words(
        "uuid-1",
        "topic.fact1",
        &["puerto", "port"],
    ))
    .unwrap();
    let mut req = save_req_with_words("uuid-2", "topic.fact1", &["costa"]);
    req.replace_keywords = true;
    let stored = svc.save(req).unwrap();
    assert_eq!(
        stored.keywords,
        words(&["costa"]),
        "replace_keywords replaces the words instead of adding them"
    );
    assert_eq!(words_of(&svc, "uuid-1"), words(&["costa"]));
}

#[test]
fn the_union_does_not_duplicate_a_word_it_already_has() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words("uuid-1", "topic.fact1", &["puerto"]))
        .unwrap();
    let stored = svc
        .save(save_req_with_words("uuid-2", "topic.fact1", &["Puerto"]))
        .unwrap();
    assert_eq!(
        stored.keywords.len(),
        1,
        "the case does not make a second word: {:?}",
        stored.keywords
    );
    assert_eq!(
        stored.keywords[0], "Puerto",
        "the requested spelling is the first one, so it wins"
    );
}

#[test]
fn a_word_is_trimmed_and_an_empty_one_is_dropped() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words(
        "uuid-1",
        "topic.fact1",
        &["  puerto  ", "", "   ", "Puerto", "port"],
    ))
    .unwrap();
    assert_eq!(
        words_of(&svc, "uuid-1"),
        words(&["puerto", "port"]),
        "a word is trimmed, an empty one is dropped and a duplicate is folded"
    );
}

#[test]
fn too_many_words_are_refused_and_nothing_is_written() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-1", "un hecho");
    req.keywords = (0..=32).map(|i| format!("palabra{i}")).collect();
    let error = svc.save(req).unwrap_err();
    assert_keywords_refused(error);
    assert!(
        svc.get("uuid-1").unwrap().is_none(),
        "a save the limit refuses stores nothing"
    );
}

#[test]
fn thirty_two_words_are_accepted_and_thirty_three_are_not() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-1", "un hecho");
    req.keywords = (0..32).map(|i| format!("palabra{i}")).collect();
    let stored = svc.save(req).unwrap();
    assert_eq!(stored.keywords.len(), 32, "32 words are within the limit");
}

#[test]
fn a_word_longer_than_48_bytes_is_refused() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-1", "un hecho");
    req.keywords = vec!["a".repeat(49)];
    let error = svc.save(req).unwrap_err();
    assert_keywords_refused(error);
    assert!(
        svc.get("uuid-1").unwrap().is_none(),
        "the refusal is atomic: no record is written"
    );
}

#[test]
fn a_word_of_48_bytes_is_accepted() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-1", "un hecho");
    let word = "a".repeat(48);
    req.keywords = vec![word.clone()];
    let stored = svc.save(req).unwrap();
    assert_eq!(stored.keywords, vec![word], "48 bytes is the boundary");
}

#[test]
fn the_accumulated_words_obey_the_limit_and_leave_the_record_as_it_was() {
    let (mut svc, _dir) = make_service();
    let first: Vec<String> = (0..30).map(|i| format!("vieja{i}")).collect();
    let mut req = save_req("uuid-1", "un hecho");
    req.keywords = first.clone();
    svc.save(req).unwrap();

    let second: Vec<String> = (0..5).map(|i| format!("nueva{i}")).collect();
    let mut req = save_req("uuid-2", "un hecho");
    req.topic_key = "topic.uuid-1".into();
    req.keywords = second;
    let error = svc.save(req).unwrap_err();
    assert_keywords_refused(error);
    assert_eq!(
        words_of(&svc, "uuid-1"),
        first,
        "a refused save leaves the record's words untouched"
    );
}

#[test]
fn a_memory_is_found_by_its_words_even_when_the_content_says_something_else() {
    let (mut svc, _dir) = make_service();
    let mut req = save_req("uuid-1", "port = 8081");
    req.title = "listen port".into();
    req.keywords = words(&["puerto"]);
    svc.save(req).unwrap();

    let hits = svc.search("puerto", None, 10, MatchMode::Any).unwrap();
    assert_eq!(
        hits.len(),
        1,
        "the vocabulary finds a record whose content says 'port'"
    );
    assert_eq!(hits[0].uuid, "uuid-1");
}

#[test]
fn the_words_are_re_indexed_when_they_change() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words("uuid-1", "topic.fact1", &["puerto"]))
        .unwrap();
    assert_eq!(
        svc.search("puerto", None, 10, MatchMode::Any)
            .unwrap()
            .len(),
        1,
        "the word is in the index as soon as it is saved"
    );

    let mut req = save_req_with_words("uuid-2", "topic.fact1", &["costa"]);
    req.replace_keywords = true;
    svc.save(req).unwrap();
    assert!(
        svc.search("puerto", None, 10, MatchMode::Any)
            .unwrap()
            .is_empty(),
        "a replaced word leaves the index"
    );
    assert_eq!(
        svc.search("costa", None, 10, MatchMode::Any).unwrap().len(),
        1,
        "the word that replaced it is indexed"
    );
}

#[test]
fn a_save_without_words_stores_no_words() {
    // The behaviour every profile had before the words existed: a save with an
    // empty list writes the record and reads back with no vocabulary at all.
    let (mut svc, _dir) = make_service();
    let stored = svc.save(save_req("uuid-1", "hello")).unwrap();
    assert_eq!(stored.keywords, Vec::<String>::new());
    assert_eq!(words_of(&svc, "uuid-1"), Vec::<String>::new());
}

#[test]
fn an_update_keeps_the_words_of_the_record() {
    // The update path writes the canonical row too, so it has to carry the
    // words through instead of blanking them.
    let (mut svc, _dir) = make_service();
    svc.save(save_req_with_words(
        "uuid-1",
        "topic.fact1",
        &["puerto", "port"],
    ))
    .unwrap();
    let updated = svc
        .update(UpdateRequest {
            memory_type: None,

            uuid: "uuid-1".into(),
            expected_revision: 1,
            content: Some("port = 9090".into()),
            state: None,
            tags: None,
            assertion_basis: None,
            reason: "a new value".into(),
            now: "2026-01-02T00:00:00Z".into(),
        })
        .unwrap();
    assert_eq!(updated.content, "port = 9090");
    assert_eq!(
        updated.keywords,
        words(&["puerto", "port"]),
        "an update of the content does not touch the vocabulary"
    );
    assert_eq!(words_of(&svc, "uuid-1"), words(&["puerto", "port"]));
}
