//! The words of a fact travel on the export / import wire (S5).
//!
//! `mnexis export` writes one canonical record per line and `mnexis import`
//! writes those records back, so a vocabulary the save stored has to survive
//! the round trip: a restore that dropped it would lose words the agent
//! supplied, and no read afterwards could tell they were ever there.

use mnexis_core::application::io::{ConflictPolicy, ImportRecord};
use mnexis_core::application::{MemoryService, SaveRequest, ServiceConfig};
use mnexis_core::domain::error::MnexisError;
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::scope::{AccessScope, Operation};
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use mnexis_core::storage::sqlite::open_sqlite;

fn make_service() -> (MemoryService<SqliteMemoryRepository>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("system temp dir");
    let conn = open_sqlite(&dir.path().join("io.db")).unwrap();
    let repo = SqliteMemoryRepository::new(conn);
    let scope = AccessScope::new(
        vec!["proyecto-a".into()],
        vec!["proyecto-a".into()],
        "proyecto-a".into(),
        false,
        vec![
            Operation::Save,
            Operation::Get,
            Operation::Export,
            Operation::Import,
        ],
    );
    let svc = MemoryService::new(repo, scope, ServiceConfig::default());
    (svc, dir)
}

fn save_req(uuid: &str, topic_key: &str, words: &[&str]) -> SaveRequest {
    SaveRequest {
        uuid: uuid.into(),
        title: "the label of the fact".into(),
        topic_key: topic_key.into(),
        namespace: "proyecto-a".into(),
        memory_type: Some("fact".into()),
        content: "port = 8081".into(),
        actor: "user".into(),
        channel: "cli".into(),
        assertion_basis: "user_stated".into(),
        valid_from: None,
        valid_until: None,
        tags: Vec::new(),
        idempotency_key: None,
        keywords: words.iter().map(|w| w.to_string()).collect(),
        replace_keywords: false,
        state: MemoryState::Active,
        now: "2026-01-01T00:00:00Z".into(),
    }
}

/// The exact loop `mnexis export` runs: the namespaced snapshot, one
/// canonical record serialised per line.
fn export_body(svc: &MemoryService<SqliteMemoryRepository>) -> String {
    let memories = svc.export(false).expect("the export must run");
    let mut body = String::new();
    for memory in &memories {
        body.push_str(&serde_json::to_string(&ImportRecord::from(memory)).expect("record json"));
        body.push('\n');
    }
    body
}

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|w| w.to_string()).collect()
}

#[test]
fn export_carries_the_words_the_record_has() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req(
        "uuid-1",
        "topic.port",
        &["puerto", "port", "listen port"],
    ))
    .unwrap();

    let body = export_body(&svc);
    let line = body.lines().next().expect("one exported record");
    let record: serde_json::Value = serde_json::from_str(line).expect("the line is JSON");
    assert_eq!(
        record["keywords"],
        serde_json::json!(["puerto", "port", "listen port"]),
        "the exported record must carry the words the save stored: {line}"
    );
}

#[test]
fn an_export_import_round_trip_preserves_the_words() {
    let (mut source, _src_dir) = make_service();
    let saved = source
        .save(save_req(
            "uuid-1",
            "topic.port",
            &["puerto", "port", "listen port"],
        ))
        .unwrap();
    let body = export_body(&source);

    let (mut target, _dst_dir) = make_service();
    let summary = target
        .import(body.as_bytes(), ConflictPolicy::Reject, false)
        .expect("an export of one record must import");
    assert_eq!(
        summary.fresh, 1,
        "the record is new in the empty store: {summary:?}"
    );

    let restored = target.get("uuid-1").unwrap().expect("the record travels");
    assert_eq!(
        restored.keywords, saved.keywords,
        "the round trip keeps the words exactly"
    );
    assert_eq!(restored.title, saved.title, "the title travels too");
}

#[test]
fn an_export_written_before_the_words_existed_still_imports() {
    let (mut svc, _dir) = make_service();
    // The shape the engine wrote before `keywords` was on the wire: no such
    // field in the record at all.
    let old = "{\"uuid\":\"uuid-old\",\"title\":\"an old fact\",\"topic_key\":\"topic.old\",\
               \"namespace\":\"proyecto-a\",\"memory_type\":\"fact\",\"content\":\"an old record\",\
               \"state\":\"active\",\"revision\":1,\"deleted\":false,\
               \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
               \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
               \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}\n";

    let summary = svc
        .import(old.as_bytes(), ConflictPolicy::Reject, false)
        .expect("a file written before the field existed must still import");
    assert_eq!(summary.fresh, 1, "the old record is written: {summary:?}");

    let restored = svc.get("uuid-old").unwrap().expect("the record lands");
    assert_eq!(restored.content, "an old record");
    assert_eq!(
        restored.keywords,
        Vec::<String>::new(),
        "a record with no words field carries no words"
    );
}

#[test]
fn an_import_whose_words_differ_is_a_payload_difference() {
    let (mut svc, _dir) = make_service();
    svc.save(save_req("uuid-1", "topic.port", &["puerto"]))
        .unwrap();

    // Same uuid and same every other field, one word changed: the record is
    // not the local one, so `reject` must refuse it instead of counting an
    // "identical" no-op and dropping the difference.
    let local = svc.get("uuid-1").unwrap().unwrap();
    let mut record: serde_json::Value =
        serde_json::to_value(ImportRecord::from(&local)).expect("record json");
    record["keywords"] = serde_json::json!(["port"]);
    let body = format!("{}\n", serde_json::to_string(&record).unwrap());

    let error = svc
        .import(body.as_bytes(), ConflictPolicy::Reject, false)
        .expect_err("a record whose words differ is a uuid clash, not an identical record");
    assert!(
        matches!(error, MnexisError::RevisionConflict { .. }),
        "the clash keeps its established error code: {error:?}"
    );

    // Nothing was written, and the words on disk are the ones the save left.
    let kept = svc.get("uuid-1").unwrap().expect("the record is untouched");
    assert_eq!(kept.keywords, words(&["puerto"]));
}
