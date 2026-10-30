//! What a write costs the full-text index, and what the index says afterwards.
//!
//! `memories_fts` is maintained by five triggers on `memories`, and two of
//! them can answer the same `UPDATE`. Every FTS5 row write tokenises the row
//! again — an `UPDATE` of an FTS5 row is internally a delete plus an insert —
//! so a second trigger answering one statement is the same text paid for
//! twice. That cost is what these tests hold still, together with the rows
//! every write path leaves behind.
//!
//! # The observable
//!
//! **How many times one canonical `UPDATE` writes the content of the index
//! row.** Three ways to observe it were available, and two of them lie:
//!
//! - A trigger on `memories_fts` is the obvious wrapper, and SQLite refuses
//!   it: a trigger cannot be created on a virtual table.
//! - `total_changes` counts rows written, triggers included, but on an FTS5
//!   table one row write expands into a version-dependent number of shadow
//!   writes (`memories_fts_data`, `_docsize`, `_content`, …) that FTS5
//!   batches at commit. The absolute delta is a magic number that says
//!   nothing about how many times the row's text was tokenised.
//! - Rowid/rank churn is invisible: the rowid of an FTS5 row survives every
//!   write, so "was it written" cannot be read back from the index at all —
//!   a redundant rewrite leaves the same final index as the single write.
//!
//! So the count is taken where it is exact.
//! [`stand_in_for_the_index`] replaces `memories_fts` with a plain table of
//! the same name and the same four columns and counts, with triggers on that
//! table, the writes that tokenise a row again: an `INSERT` and an
//! `UPDATE OF content` are those writes, a `DELETE` is not. The triggers
//! under test live on `memories`, so which of them fires for a statement —
//! the question C4 is about — is the production one, untouched by the
//! stand-in. The same claim is then re-checked on the **real** FTS5 index in
//! [`a_live_update_costs_the_real_index_what_a_content_change_costs`], where
//! the price of one index write is measured in the same run instead of
//! assumed.

use mnexis_core::application::{MemoryService, SaveRequest, ServiceConfig};
use mnexis_core::domain::memory::{Memory, MemoryState};
use mnexis_core::domain::query::MatchMode;
use mnexis_core::domain::repository::MemoryRepository;
use mnexis_core::domain::scope::{AccessScope, Operation};
use mnexis_core::open_sqlite;
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use rusqlite::{Connection, OptionalExtension, params};

/// A scratch profile, kept inside `target/` for the reason `tests/
/// retrieval_fts5.rs` records: the development host's `/tmp` is quota-bound.
fn profile(name: &str) -> (Connection, tempfile::TempDir) {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join(format!("{name}.db"));
    (open_sqlite(&path).unwrap(), directory)
}

fn repository(name: &str) -> (SqliteMemoryRepository, tempfile::TempDir) {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join(format!("{name}.db"));
    (
        SqliteMemoryRepository::new(open_sqlite(&path).unwrap()),
        directory,
    )
}

/// A service over a scratch profile, bounded to the namespace `fact` saves in.
/// The save path — including the occupancy re-save — is the one under test.
fn service(name: &str) -> (MemoryService<SqliteMemoryRepository>, tempfile::TempDir) {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join(format!("{name}.db"));
    let repo = SqliteMemoryRepository::new(open_sqlite(&path).unwrap());
    let scope = AccessScope::new(
        vec!["pagos".to_string()],
        vec!["pagos".to_string()],
        "pagos".to_string(),
        false,
        vec![
            Operation::Save,
            Operation::Get,
            Operation::Search,
            Operation::Update,
        ],
    );
    (
        MemoryService::new(repo, scope, ServiceConfig::default()),
        directory,
    )
}

/// A save that names its own key, title and content: the shape that lands on
/// the record already holding the key.
fn save_request(uuid: &str, topic_key: &str, title: &str, content: &str) -> SaveRequest {
    SaveRequest {
        uuid: uuid.to_string(),
        title: title.to_string(),
        topic_key: topic_key.to_string(),
        namespace: "pagos".to_string(),
        memory_type: Some("fact".to_string()),
        content: content.to_string(),
        actor: "user".to_string(),
        channel: "cli".to_string(),
        assertion_basis: "user_stated".to_string(),
        valid_from: None,
        valid_until: None,
        tags: Vec::new(),
        keywords: Vec::new(),
        replace_keywords: false,
        idempotency_key: None,
        state: MemoryState::Active,
        now: "2026-01-01T00:00:00Z".to_string(),
    }
}

/// The schema version the store records.
fn schema_version(conn: &Connection) -> i64 {
    conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
        row.get(0)
    })
    .expect("the ledger must be readable")
}

/// An active fact that carries all four indexed columns.
fn fact(uuid: &str, content: &str) -> Memory {
    let mut memory = Memory::new_proposed(
        uuid,
        format!("topic.{uuid}"),
        "pagos",
        "fact",
        content,
        "user",
        "cli",
        "user_stated",
        "2026-01-01T00:00:00Z",
    );
    memory.state = MemoryState::Active;
    memory.title = "puerto del servicio".to_string();
    memory.keywords = vec!["puerto".to_string(), "listen-port".to_string()];
    memory
}

/// Save a fact and read it back: `save` answers the new row's rowid, and the
/// write-path tests need the record the row now is.
fn save(repo: &mut SqliteMemoryRepository, memory: &Memory) -> Memory {
    repo.save(memory).expect("the save must run");
    repo.get_by_uuid(&memory.uuid)
        .expect("the read-back must run")
        .expect("the saved record must read back")
}

/// The four columns of the index row of one record, or `None` when the record
/// has no row in the index at all.
fn indexed_row(conn: &Connection, uuid: &str) -> Option<(String, String, String, String)> {
    conn.query_row(
        "SELECT f.content, f.title, f.topic_key, f.keywords \
         FROM memories_fts AS f JOIN memories AS m ON m.id = f.rowid \
         WHERE m.uuid = ?1",
        [uuid],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .optional()
    .expect("reading the index row must run")
}

/// The index row of a record that must be in the index.
fn must_be_indexed(conn: &Connection, uuid: &str) -> (String, String, String, String) {
    indexed_row(conn, uuid).unwrap_or_else(|| panic!("{uuid} must have a row in memories_fts"))
}

/// The contract the triggers own, stated once: a record that is live
/// (`state = 'active'` and not deleted) has exactly one index row, and that
/// row carries the record's own four indexed columns; a record that is not
/// live has none. Every write path in this file ends with this check.
fn assert_index_matches_canonical(conn: &Connection, uuid: &str) {
    let (content, title, topic_key, keywords, state, deleted): (
        String,
        String,
        String,
        String,
        String,
        i64,
    ) = conn
        .query_row(
            "SELECT content, title, topic_key, keywords, state, deleted FROM memories \
             WHERE uuid = ?1",
            [uuid],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("the canonical row must be readable");
    let expected = if state == "active" && deleted == 0 {
        Some((content, title, topic_key, keywords))
    } else {
        None
    };
    assert_eq!(
        indexed_row(conn, uuid),
        expected,
        "the index row of {uuid} must be the record's own columns, and only \
         while the record is live"
    );
}

fn hits(conn: &Connection, term: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH ?1",
        [term],
        |row| row.get(0),
    )
    .expect("the MATCH must run")
}

/// Swap the FTS5 index for a plain table of the same name and columns, and
/// count the writes that tokenise the row again.
///
/// Both `INSERT` and `UPDATE OF content` on `memories_fts` send the row's text
/// through the tokenizer, so both are counted; `DELETE` only removes it and is
/// not. The production triggers keep writing to `memories_fts` unchanged.
fn stand_in_for_the_index(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE memories_fts;
         CREATE TABLE memories_fts (content TEXT, title TEXT, topic_key TEXT, keywords TEXT);
         CREATE TABLE index_content_writes (id INTEGER PRIMARY KEY AUTOINCREMENT);
         CREATE TRIGGER count_content_written_on_insert
         AFTER INSERT ON memories_fts
         BEGIN INSERT INTO index_content_writes (id) VALUES (NULL); END;
         CREATE TRIGGER count_content_written_on_update
         AFTER UPDATE OF content ON memories_fts
         BEGIN INSERT INTO index_content_writes (id) VALUES (NULL); END;",
    )
    .expect("the stand-in index must be installable");
}

/// How many times the content of the index row was written so far.
fn content_writes(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM index_content_writes", [], |row| {
        row.get(0)
    })
    .expect("the counter must be readable")
}

/// A fact of the size the audit measured — 8 KiB of words — so the row that is
/// written twice is a row whose text is worth tokenising.
fn fact_content(word: &str) -> String {
    format!("{word} del servicio de pagos. ").repeat(320)
}

// ── C4: one UPDATE, one index write ────────────────────────────────────────

#[test]
fn a_live_update_writes_the_fts_row_once() {
    let (conn, _guard) = profile("fts_write_once");
    stand_in_for_the_index(&conn);

    // The row enters the index through the production insert trigger.
    conn.execute(
        "INSERT INTO memories (uuid, title, topic_key, namespace, memory_type, content, \
         keywords, state, actor, channel, assertion_basis) \
         VALUES ('fact-1', 'puerto del servicio', 'topic.fact-1', 'pagos', 'fact', \
         'el puerto del servicio es 8081', 'puerto, listen-port', 'active', \
         'user', 'cli', 'user_stated')",
        [],
    )
    .unwrap();
    let baseline = content_writes(&conn);
    assert_eq!(baseline, 1, "the insert trigger indexes the row once");

    // The shape every writer sends: the new content plus the state the row
    // already has (`memory_repo.rs` sets `state` in both UPDATE statements).
    conn.execute(
        "UPDATE memories SET content = ?1, state = 'active' WHERE uuid = 'fact-1'",
        ["the port of the service is 8081"],
    )
    .unwrap();

    let writes = content_writes(&conn) - baseline;
    assert_eq!(
        writes, 1,
        "one UPDATE of a live fact must write the index row once; \
         2 means both the transition trigger and the content trigger answered \
         the same statement (C4)"
    );
    assert!(
        indexed_row(&conn, "fact-1").is_some(),
        "the row must still be in the index after the update"
    );
}

#[test]
fn a_live_update_costs_the_real_index_what_a_content_change_costs() {
    let (conn, _guard) = profile("fts_real_cost");
    conn.execute(
        "INSERT INTO memories (uuid, title, topic_key, namespace, memory_type, content, \
         keywords, state, actor, channel, assertion_basis) \
         VALUES ('fact-1', 'puerto del servicio', 'topic.fact-1', 'pagos', 'fact', ?1, \
         'puerto, listen-port', 'active', 'user', 'cli', 'user_stated')",
        [fact_content("puerto")],
    )
    .unwrap();

    // The price of one index write on this SQLite build is measured, not
    // assumed: a content-only UPDATE fires the content trigger once, so its
    // `total_changes` delta is the canonical row plus exactly one index write.
    // Every FTS5 shadow write that write costs is included on both sides, so
    // the comparison carries no magic number.
    let content_only = changes_from(&conn, |conn| {
        conn.execute(
            "UPDATE memories SET content = ?1 WHERE uuid = 'fact-1'",
            [fact_content("puertx")],
        )
    });
    let with_state = changes_from(&conn, |conn| {
        conn.execute(
            "UPDATE memories SET content = ?1, state = 'active' WHERE uuid = 'fact-1'",
            [fact_content("puertw")],
        )
    });

    assert_eq!(
        with_state, content_only,
        "sending the state the row already has must not cost the real FTS5 \
         index a single extra write (content-only: {content_only} changes, \
         content plus state: {with_state} changes)"
    );
}

/// The number of rows SQLite wrote while `statement` ran, triggers included.
fn changes_from(
    conn: &Connection,
    statement: impl Fn(&Connection) -> rusqlite::Result<usize>,
) -> u64 {
    let before = conn.total_changes();
    statement(conn).expect("the statement must run");
    conn.total_changes() - before
}

// ── every write path keeps the index row in step ───────────────────────────

#[test]
fn a_create_indexes_every_indexed_column_and_a_proposal_stays_out() {
    let (mut repo, _guard) = repository("fts_create");

    repo.save(&fact("fact-1", "el puerto del servicio es 8081"))
        .unwrap();
    let (content, title, topic_key, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, "el puerto del servicio es 8081");
    assert_eq!(title, "puerto del servicio");
    assert_eq!(topic_key, "topic.fact-1");
    assert_eq!(keywords, "puerto, listen-port");

    let mut proposal = fact("fact-2", "una propuesta que nadie acepto");
    proposal.state = MemoryState::Proposed;
    repo.save(&proposal).unwrap();
    assert_eq!(
        indexed_row(repo.connection(), "fact-2"),
        None,
        "a proposed fact is not searchable"
    );
    assert_index_matches_canonical(repo.connection(), "fact-1");
    assert_index_matches_canonical(repo.connection(), "fact-2");
}

#[test]
fn an_update_reindexes_the_content_the_title_and_the_keywords() {
    let (mut repo, _guard) = repository("fts_update");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));

    let mut updated = saved.clone();
    updated.content = "el puerto del servicio es 9090".to_string();
    updated.title = "puerto del servicio de pagos".to_string();
    updated.keywords = vec!["port".to_string(), "listening".to_string()];
    updated.updated_at = "2026-01-02T00:00:00Z".to_string();
    let updated = repo.update(&updated, saved.revision).unwrap();

    let (content, title, topic_key, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, updated.content);
    assert_eq!(title, updated.title);
    assert_eq!(topic_key, updated.topic_key.as_str());
    assert_eq!(keywords, "port, listening");
    assert_eq!(hits(repo.connection(), "8081"), 0, "the old value is gone");
    assert_eq!(hits(repo.connection(), "9090"), 1, "the new value is found");
    assert_eq!(
        hits(repo.connection(), "listening"),
        1,
        "the new word is found"
    );
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn an_update_with_history_reindexes_the_row() {
    let (mut repo, _guard) = repository("fts_update_history");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));

    let mut updated = saved.clone();
    updated.content = "el puerto del servicio es 9090".to_string();
    updated.keywords = vec!["port".to_string()];
    updated.updated_at = "2026-01-02T00:00:00Z".to_string();
    let history = mnexis_core::domain::memory::MemoryHistory {
        id: None,
        memory_uuid: saved.uuid.clone(),
        revision: saved.revision,
        memory_type: saved.memory_type.clone(),
        content: saved.content.clone(),
        state: saved.state,
        valid_from: saved.valid_from.clone(),
        valid_until: saved.valid_until.clone(),
        assertion_basis: saved.assertion_basis.clone(),
        changed_by: "user".to_string(),
        change_reason: "re-saved".to_string(),
        changed_at: "2026-01-02T00:00:00Z".to_string(),
    };
    repo.update_with_history_and_tags(&updated, saved.revision, &history, None, None)
        .unwrap();

    let (content, _, _, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, "el puerto del servicio es 9090");
    assert_eq!(keywords, "port");
    assert_eq!(hits(repo.connection(), "8081"), 0);
    assert_eq!(hits(repo.connection(), "9090"), 1);
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn an_update_with_history_carries_the_title() {
    let (mut repo, _guard) = repository("fts_update_history_title");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));

    // The writer receives a record whose title moved on. Nothing else about
    // the record changes except what a caller would send anyway.
    let mut renamed = saved.clone();
    renamed.title = "puerto del servicio de pagos".to_string();
    renamed.updated_at = "2026-01-02T00:00:00Z".to_string();
    let history = mnexis_core::domain::memory::MemoryHistory {
        id: None,
        memory_uuid: saved.uuid.clone(),
        revision: saved.revision,
        memory_type: saved.memory_type.clone(),
        content: saved.content.clone(),
        state: saved.state,
        valid_from: saved.valid_from.clone(),
        valid_until: saved.valid_until.clone(),
        assertion_basis: saved.assertion_basis.clone(),
        changed_by: "user".to_string(),
        change_reason: "retitled".to_string(),
        changed_at: "2026-01-02T00:00:00Z".to_string(),
    };
    let written = repo
        .update_with_history_and_tags(&renamed, saved.revision, &history, None, None)
        .expect("the write must run");

    assert_eq!(
        written.title, "puerto del servicio de pagos",
        "the title the caller sent must be the title on disk, not the one the \
         record already had"
    );
    assert_eq!(
        must_be_indexed(repo.connection(), "fact-1").1,
        "puerto del servicio de pagos",
        "the index carries the title the canonical row now holds"
    );
    assert_eq!(
        hits(repo.connection(), "pagos"),
        1,
        "a word only the new title has is searchable"
    );
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn a_resave_of_the_same_topic_key_keeps_the_title_it_was_given() {
    let (mut svc, _guard) = service("fts_resave_title");

    // A record that holds the key and has no title: a fact saved before the
    // engine named facts (migration 005 gives it the empty default).
    let first = svc
        .save(save_request(
            "uuid-1",
            "topic.fact-1",
            "",
            "el puerto del servicio es 8081",
        ))
        .expect("the first save must run");
    assert_eq!(first.title, "", "the fixture starts without a title");

    // The same key, saved again, now with the title of the fact. The save lands
    // on the record that holds the key, and it carries the title it was given
    // (the occupancy re-save writes through `update_with_history_and_tags`).
    let saved = svc
        .save(save_request(
            "uuid-2",
            "topic.fact-1",
            "escucha del puerto",
            "el puerto del servicio es 8081",
        ))
        .expect("the re-save must run");

    assert_eq!(
        saved.uuid, "uuid-1",
        "the re-save lands on the record that holds the key"
    );
    assert_eq!(
        saved.title, "escucha del puerto",
        "the re-save carries the title it was given"
    );
    assert_eq!(
        svc.get("uuid-1")
            .expect("the read must run")
            .expect("the record still reads back")
            .title,
        "escucha del puerto",
        "the title is on disk, not only in the answer"
    );
    let hits = svc
        .search("escucha", Some("pagos"), 10, MatchMode::Any)
        .expect("the search must run");
    assert_eq!(
        hits.len(),
        1,
        "a word only the title has is searchable: the re-save reached the index"
    );
}

#[test]
fn a_transition_into_active_indexes_the_row_with_every_column() {
    let (mut repo, _guard) = repository("fts_into_active");
    let mut proposal = fact("fact-1", "el puerto del servicio es 8081");
    proposal.state = MemoryState::Proposed;
    let saved = save(&mut repo, &proposal);
    assert_eq!(indexed_row(repo.connection(), "fact-1"), None);

    // The statement every writer sends: the content it has plus the new state.
    let mut accepted = saved.clone();
    accepted.state = MemoryState::Active;
    accepted.title = "puerto del servicio".to_string();
    accepted.keywords = vec!["puerto".to_string()];
    accepted.updated_at = "2026-01-02T00:00:00Z".to_string();
    repo.update(&accepted, saved.revision).unwrap();

    let (content, title, topic_key, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, "el puerto del servicio es 8081");
    assert_eq!(title, "puerto del servicio");
    assert_eq!(topic_key, "topic.fact-1");
    assert_eq!(keywords, "puerto");
    assert_eq!(
        hits(repo.connection(), "puerto"),
        1,
        "the fact is searchable"
    );
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn a_transition_out_of_active_removes_the_row() {
    let (mut repo, _guard) = repository("fts_out_of_active");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));
    assert_eq!(hits(repo.connection(), "puerto"), 1);

    let mut disputed = saved.clone();
    disputed.state = MemoryState::Disputed;
    disputed.updated_at = "2026-01-02T00:00:00Z".to_string();
    repo.update(&disputed, saved.revision).unwrap();

    assert_eq!(
        indexed_row(repo.connection(), "fact-1"),
        None,
        "a disputed fact leaves the index"
    );
    assert_eq!(hits(repo.connection(), "puerto"), 0);
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn restore_puts_the_row_back_with_the_content_it_has() {
    let (mut repo, _guard) = repository("fts_restore");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));

    repo.soft_delete("fact-1", saved.revision).unwrap();
    assert_eq!(
        indexed_row(repo.connection(), "fact-1"),
        None,
        "a soft-deleted fact leaves the index"
    );
    assert_eq!(hits(repo.connection(), "puerto"), 0);

    repo.restore("fact-1").unwrap();
    let (content, title, topic_key, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, "el puerto del servicio es 8081");
    assert_eq!(title, "puerto del servicio");
    assert_eq!(topic_key, "topic.fact-1");
    assert_eq!(keywords, "puerto, listen-port");
    assert_eq!(
        hits(repo.connection(), "puerto"),
        1,
        "the fact is found again"
    );
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn supersede_takes_the_older_row_out_of_the_index() {
    let (mut repo, _guard) = repository("fts_supersede");
    let older = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));
    let newer = save(&mut repo, &fact("fact-2", "el puerto del servicio es 9090"));

    repo.supersede_atomic(
        "fact-1",
        "fact-2",
        older.revision,
        newer.revision,
        "the port moved",
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    assert_eq!(
        indexed_row(repo.connection(), "fact-1"),
        None,
        "the superseded fact leaves the index"
    );
    assert_eq!(
        must_be_indexed(repo.connection(), "fact-2").0,
        "el puerto del servicio es 9090",
        "the replacing fact keeps its row"
    );
    assert_eq!(hits(repo.connection(), "8081"), 0);
    assert_eq!(hits(repo.connection(), "9090"), 1);
    assert_index_matches_canonical(repo.connection(), "fact-1");
    assert_index_matches_canonical(repo.connection(), "fact-2");
}

#[test]
fn purge_takes_the_row_out_of_the_index() {
    let (mut repo, _guard) = repository("fts_purge");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));
    repo.soft_delete("fact-1", saved.revision).unwrap();

    let purged = repo.purge(None, true).unwrap();
    assert_eq!(purged, 1);
    assert_eq!(
        indexed_row(repo.connection(), "fact-1"),
        None,
        "a purged fact has no canonical row and no index row"
    );
    assert_eq!(hits(repo.connection(), "puerto"), 0);
}

#[test]
fn an_index_rebuilt_from_canonical_rows_carries_the_same_columns() {
    let (mut repo, _guard) = repository("fts_rebuild");
    repo.save(&fact("fact-1", "el puerto del servicio es 8081"))
        .unwrap();

    // The doctor's repair and `reindex` both end here; the row must read back
    // exactly as the canonical record, and a later write must keep it in step.
    let indexed = repo.reindex().unwrap();
    assert_eq!(indexed, 1);
    let (content, title, topic_key, keywords) = must_be_indexed(repo.connection(), "fact-1");
    assert_eq!(content, "el puerto del servicio es 8081");
    assert_eq!(title, "puerto del servicio");
    assert_eq!(topic_key, "topic.fact-1");
    assert_eq!(keywords, "puerto, listen-port");

    let saved = repo.get_by_uuid("fact-1").unwrap().unwrap();
    let mut updated = saved.clone();
    updated.content = "el puerto del servicio es 9090".to_string();
    updated.updated_at = "2026-01-02T00:00:00Z".to_string();
    repo.update(&updated, saved.revision).unwrap();
    assert_eq!(
        must_be_indexed(repo.connection(), "fact-1").0,
        "el puerto del servicio es 9090"
    );
    assert_eq!(hits(repo.connection(), "8081"), 0);
    assert_index_matches_canonical(repo.connection(), "fact-1");
}

#[test]
fn a_doctor_title_backfill_reaches_the_index() {
    let (conn, _guard) = profile("fts_title_backfill");
    // A record that predates titles: no title, no topic key, indexed by its
    // content alone. The doctor's backfill writes with `UPDATE ... SET title`.
    conn.execute(
        "INSERT INTO memories (uuid, namespace, content, state, actor, channel, assertion_basis) \
         VALUES ('fact-1', 'pagos', 'el puerto del servicio es 8081', 'active', 'user', 'cli', \
         'user_stated')",
        [],
    )
    .unwrap();
    assert_eq!(hits(&conn, "puerto"), 1);

    conn.execute(
        "UPDATE memories SET title = ?1, topic_key = ?2 WHERE id = (SELECT id FROM memories \
         WHERE uuid = 'fact-1')",
        params!["puerto de escucha", "puerto-de-escucha"],
    )
    .unwrap();

    let (content, title, topic_key, _) = must_be_indexed(&conn, "fact-1");
    assert_eq!(content, "el puerto del servicio es 8081");
    assert_eq!(title, "puerto de escucha");
    assert_eq!(topic_key, "puerto-de-escucha");
    assert_eq!(
        hits(&conn, "escucha"),
        1,
        "the word the backfill added is searchable: the title reaches the index"
    );
    assert_eq!(hits(&conn, "puerto"), 1, "the record is still one hit");
    assert_index_matches_canonical(&conn, "fact-1");
}

#[test]
fn a_rebuilt_index_still_answers_an_update_once() {
    let (mut repo, _guard) = repository("fts_rebuild_once");
    let saved = save(&mut repo, &fact("fact-1", "el puerto del servicio es 8081"));

    // `reindex` and the doctor's repair both rebuild the index *and* its
    // triggers; the rebuilt trigger set must be the single-writer one.
    assert_eq!(repo.reindex().unwrap(), 1);
    stand_in_for_the_index(repo.connection());
    // The stand-in starts holding what the rebuild left in the index.
    repo.connection()
        .execute(
            "INSERT INTO memories_fts (rowid, content, title, topic_key, keywords) \
             SELECT id, content, title, topic_key, keywords FROM memories",
            [],
        )
        .unwrap();
    let baseline = content_writes(repo.connection());
    assert_eq!(baseline, 1, "the rebuilt index holds the row");

    let mut updated = saved.clone();
    updated.content = "el puerto del servicio es 9090".to_string();
    updated.updated_at = "2026-01-02T00:00:00Z".to_string();
    repo.update(&updated, saved.revision).unwrap();

    assert_eq!(
        content_writes(repo.connection()) - baseline,
        1,
        "a rebuilt index must not resurrect the double write"
    );
}

// ── migration 009: an existing store heals when it is opened ───────────────

/// The two `AFTER UPDATE` triggers exactly as a store at schema version 8
/// carries them: the transition trigger has no guard, so it answers a live
/// update together with the content trigger, and the row's text is paid for
/// twice. Written out here because the fixture has to be the shape that
/// existed, not the shape the code has now.
const PRE_009_TRIGGERS: &str = r#"
DROP TRIGGER IF EXISTS memories_after_update_into_active;
CREATE TRIGGER memories_after_update_into_active
AFTER UPDATE OF state, deleted ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
    INSERT INTO memories_fts(rowid, content, title, topic_key, keywords)
    VALUES (new.id, new.content, new.title, new.topic_key, new.keywords);
END;

DROP TRIGGER IF EXISTS memories_after_update_content;
CREATE TRIGGER memories_after_update_content
AFTER UPDATE OF content, title, topic_key, keywords ON memories
WHEN new.deleted = 0
BEGIN
    UPDATE memories_fts
    SET content = new.content, title = new.title, topic_key = new.topic_key,
        keywords = new.keywords
    WHERE rowid = new.id;
END;
"#;

#[test]
fn migration_009_gives_an_existing_store_the_single_write_trigger() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig009.db");

    // The store a version-8 binary left behind: everything migrated, the index
    // in its current shape and holding a live fact, and the two `AFTER UPDATE`
    // triggers without the guard.
    let store = open_sqlite(&path).expect("the store must open");
    store
        .execute(
            "INSERT INTO memories (uuid, title, topic_key, namespace, memory_type, content, \
             keywords, state, actor, channel, assertion_basis) \
             VALUES ('fact-1', 'puerto del servicio', 'topic.fact-1', 'pagos', 'fact', \
             'el puerto del servicio es 8081', 'puerto, listen-port', 'active', \
             'user', 'cli', 'user_stated')",
            [],
        )
        .expect("the fixture fact must be stored");
    store
        .execute_batch(PRE_009_TRIGGERS)
        .expect("the pre-009 triggers must be installable");
    store
        .execute("DELETE FROM schema_migrations WHERE version >= 9", [])
        .expect("the ledger must go back to version 8");
    assert_eq!(
        schema_version(&store),
        8,
        "the fixture is a store at schema version 8"
    );
    drop(store);

    // Opening it with this binary is the whole migration path.
    let conn = open_sqlite(&path).expect("the store must open again");
    assert_eq!(schema_version(&conn), 9, "the migration is recorded");
    assert_eq!(
        must_be_indexed(&conn, "fact-1"),
        (
            "el puerto del servicio es 8081".to_string(),
            "puerto del servicio".to_string(),
            "topic.fact-1".to_string(),
            "puerto, listen-port".to_string(),
        ),
        "the migration re-creates triggers only: the index keeps its rows"
    );

    stand_in_for_the_index(&conn);
    conn.execute(
        "INSERT INTO memories_fts (rowid, content, title, topic_key, keywords) \
         SELECT id, content, title, topic_key, keywords FROM memories WHERE uuid = 'fact-1'",
        [],
    )
    .expect("the stand-in must hold what the index held");
    let baseline = content_writes(&conn);
    assert_eq!(baseline, 1, "the store starts with one index row");

    // The shape every writer sends: the new content plus the state the row
    // already has.
    conn.execute(
        "UPDATE memories SET content = ?1, state = 'active' WHERE uuid = 'fact-1'",
        ["the port of the service is 8081"],
    )
    .expect("the update must run");

    assert_eq!(
        content_writes(&conn) - baseline,
        1,
        "an existing store must get the guard when it is opened: one UPDATE of \
         a live fact writes the index row once, and 2 means the pre-009 triggers \
         survived migration 009"
    );
    assert!(
        indexed_row(&conn, "fact-1").is_some(),
        "the row must still be in the index after the update"
    );
}
