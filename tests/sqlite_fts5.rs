use std::{path::Path, process::Command};

use mnexis_core::open_sqlite;
use rusqlite::Connection;

const CHILD_DB_ENV: &str = "MNEXIS_FTS5_CHILD_DB";

fn match_count(connection: &Connection, term: &str) -> i64 {
    connection
        .query_row(
            "SELECT count(*) FROM mnexis_fts_probe WHERE mnexis_fts_probe MATCH ?1",
            [term],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn fts5_insert_update_delete_and_reopen_keep_the_index_consistent() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mnexis.db");
    {
        let connection = open_sqlite(&path).unwrap();
        connection
            .execute(
                "INSERT INTO mnexis_fts_probe(content) VALUES (?1)",
                ["petgraph chosen for the first prototype"],
            )
            .unwrap();
        assert_eq!(match_count(&connection, "petgraph"), 1);

        connection
            .execute(
                "UPDATE mnexis_fts_probe SET content = ?1 WHERE rowid = 1",
                ["custom graph engine replaces the old dependency"],
            )
            .unwrap();
        assert_eq!(match_count(&connection, "petgraph"), 0);
        assert_eq!(match_count(&connection, "custom"), 1);

        connection
            .execute("DELETE FROM mnexis_fts_probe WHERE rowid = 1", [])
            .unwrap();
        assert_eq!(match_count(&connection, "custom"), 0);
    }

    let reopened = open_sqlite(&path).unwrap();
    let rows: i64 = reopened
        .query_row("SELECT count(*) FROM mnexis_fts_probe", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn sqlite_transaction_failure_rolls_back_fts_and_connection_recovers() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("recovery.db");
    {
        let connection = open_sqlite(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE failure_guard (value INTEGER CHECK (value = 0)); BEGIN IMMEDIATE;",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mnexis_fts_probe(content) VALUES ('rolledback marker')",
                [],
            )
            .unwrap();
        assert!(
            connection
                .execute("INSERT INTO failure_guard(value) VALUES (1)", [])
                .is_err()
        );
        connection.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(match_count(&connection, "rolledback"), 0);

        connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
        connection
            .execute(
                "INSERT INTO mnexis_fts_probe(content) VALUES ('recovered marker')",
                [],
            )
            .unwrap();
        connection.execute_batch("COMMIT;").unwrap();
        assert_eq!(match_count(&connection, "recovered"), 1);
    }

    let reopened = open_sqlite(&path).unwrap();
    assert_eq!(match_count(&reopened, "rolledback"), 0);
    assert_eq!(match_count(&reopened, "recovered"), 1);
}

#[test]
fn child_process_can_read_committed_fts5_rows_while_parent_connection_is_open() {
    let Some(path) = std::env::var_os(CHILD_DB_ENV) else {
        return;
    };
    let connection = open_sqlite(Path::new(&path)).unwrap();
    assert_eq!(match_count(&connection, "shared"), 1);
    println!("child-read=1");
}

#[test]
fn separate_process_reader_observes_committed_fts5_row() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("shared.db");
    let connection = open_sqlite(&path).unwrap();
    connection
        .execute(
            "INSERT INTO mnexis_fts_probe(content) VALUES ('shared committed memory')",
            [],
        )
        .unwrap();
    connection.execute_batch("BEGIN;").unwrap();
    assert_eq!(match_count(&connection, "shared"), 1);

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_process_can_read_committed_fts5_rows_while_parent_connection_is_open",
            "--nocapture",
        ])
        .env(CHILD_DB_ENV, &path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("child-read=1"));
    connection.execute_batch("ROLLBACK;").unwrap();
}

// ── migration 006: the index knows the title (identity-visibility S2) ───────

use mnexis_core::diagnostics::doctor::{failure_exit_code, repair_and_rerun, run_checks};
use mnexis_core::domain::query::MatchMode;
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, StateFilter, search};
use mnexis_core::storage::sqlite::run_migrations;

fn insert_titled(conn: &Connection, uuid: &str, title: &str, topic_key: &str, content: &str) {
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES (?1, ?2, ?3, 'ns', 'fact', ?4, 'active', 'user', 'cli', 'user_stated')",
        rusqlite::params![uuid, title, topic_key, content],
    )
    .expect("insert_titled must succeed for the test fixture to be valid");
}

fn indexed(conn: &Connection, term: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH ?1",
        [term],
        |row| row.get(0),
    )
    .expect("the MATCH must run")
}

fn search_ids(conn: &Connection, query: &str) -> Vec<String> {
    search(
        conn,
        query,
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("the query must run")
    .into_iter()
    .map(|hit| hit.uuid)
    .collect()
}

fn schema_version(conn: &Connection) -> i64 {
    conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
        row.get(0)
    })
    .unwrap()
}

/// Put the profile back in the pre-006 state the migration has to heal: the
/// ledger says 5 and the index is the content-only table of migration 001.
fn regress_to_content_only_index(conn: &Connection) {
    conn.execute("DROP TABLE memories_fts", [])
        .expect("the pre-006 index is dropped");
    conn.execute("CREATE VIRTUAL TABLE memories_fts USING fts5(content)", [])
        .expect("the pre-006 content-only index is recreated");
    conn.execute(
        "INSERT INTO memories_fts(rowid, content) \
         SELECT id, content FROM memories WHERE state = 'active' AND deleted = 0",
        [],
    )
    .expect("the pre-006 index is populated with content only");
    conn.execute("DELETE FROM schema_migrations WHERE version >= 6", [])
        .expect("the ledger goes back to 5");
}

#[test]
fn migration_006_indexes_the_title_and_the_topic_key() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig006.db");
    let conn = open_sqlite(&path).unwrap();

    assert_eq!(
        schema_version(&conn),
        7,
        "the ledger must reach the end of the migrations"
    );

    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "a fresh profile indexes the title of an inserted memory"
    );
    assert_eq!(
        indexed(&conn, "servicio"),
        1,
        "the whole title is searchable, not only its first word"
    );

    insert_titled(&conn, "uuid-key", "", "puerto-del-servicio", "port = 8081");
    assert_eq!(
        indexed(&conn, "puerto"),
        2,
        "the topic key is indexed as well as the title"
    );
}

#[test]
fn migration_006_rebuilds_an_index_built_without_the_title() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig006_old.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    insert_titled(&conn, "uuid-key", "", "puerto-del-servicio", "port = 8081");

    regress_to_content_only_index(&conn);
    assert_eq!(
        indexed(&conn, "puerto"),
        0,
        "the pre-006 index only knows the content, which says 'port'"
    );

    run_migrations(&conn).expect("the migration must heal the index");

    assert_eq!(schema_version(&conn), 7);
    assert_eq!(
        indexed(&conn, "puerto"),
        2,
        "the migration rebuilds every active row with its title and topic key"
    );
    let mut ids = search_ids(&conn, "puerto");
    ids.sort();
    assert_eq!(
        ids,
        vec!["uuid-key".to_string(), "uuid-puerto".to_string()],
        "both records are findable again through the search layer"
    );
}

#[test]
fn migration_006_is_idempotent_and_probes_before_it_applies() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig006_again.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );

    // A half-applied migration: the schema is already the new one, the ledger
    // lost its row. The probe decides, so the re-run must be a no-op and must
    // never fail on the index it already rebuilt.
    conn.execute("DELETE FROM schema_migrations WHERE version >= 6", [])
        .unwrap();
    run_migrations(&conn).expect("the re-run must not fail on the index it already built");
    assert_eq!(schema_version(&conn), 7);
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the rebuilt index survived the re-run"
    );

    // And a plain second run is still safe.
    run_migrations(&conn).expect("a second run must be a no-op");
    assert_eq!(schema_version(&conn), 7);
    assert_eq!(indexed(&conn, "puerto"), 1);
}

#[test]
fn migration_006_recreates_the_triggers_with_the_new_columns() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig006_triggers.db");
    let conn = open_sqlite(&path).unwrap();

    // The insert trigger, the into-active pair, the update trigger and the
    // delete trigger all have to carry the new columns.
    insert_titled(&conn, "uuid-t", "puerto del servicio", "", "port = 8081");
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "insert trigger covers the title"
    );

    conn.execute(
        "UPDATE memories SET state = 'superseded' WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(indexed(&conn, "puerto"), 0, "out-of-active drops the row");

    conn.execute(
        "UPDATE memories SET state = 'active' WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "into-active re-indexes the title"
    );

    conn.execute(
        "UPDATE memories SET title = 'base de datos', topic_key = 'base-de-datos' \
         WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(indexed(&conn, "puerto"), 0, "the old title left the index");
    assert_eq!(indexed(&conn, "datos"), 1, "the new title is indexed");

    conn.execute("DELETE FROM memories WHERE uuid = 'uuid-t'", [])
        .unwrap();
    assert_eq!(indexed(&conn, "datos"), 0, "the delete trigger still fires");
}

// ── the doctor tells a stale index from a good one (S2) ─────────────────────

fn check<'a>(
    report: &'a mnexis_core::diagnostics::doctor::DoctorReport,
    name: &str,
) -> &'a mnexis_core::diagnostics::doctor::Check {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| {
            panic!(
                "the doctor must report a '{name}' check: {:?}",
                report.checks
            )
        })
}

#[test]
fn the_doctor_reports_an_index_built_without_the_title() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_shape.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );

    regress_to_content_only_index(&conn);

    // Same row count as canonical data: the count check cannot see the
    // difference, the shape check has to.
    let report = run_checks(&conn);
    assert!(
        check(&report, "fts_invariant").ok,
        "the row counts agree — that is exactly why a count check is not enough"
    );
    let shape = check(&report, "fts_shape");
    assert!(
        !shape.ok,
        "the doctor must fail an index built without the title"
    );
    assert!(
        shape.detail.contains("title") && shape.detail.contains("topic_key"),
        "the detail names the columns the index is missing: {}",
        shape.detail
    );
    assert_eq!(
        failure_exit_code(&report),
        Some((6, "INDEX_UNAVAILABLE")),
        "a stale index is INDEX_UNAVAILABLE, and the repair is the way out"
    );

    // A good index passes the same check.
    run_migrations(&conn).unwrap();
    let healed = run_checks(&conn);
    assert!(
        check(&healed, "fts_shape").ok,
        "the rebuilt index passes the shape check"
    );
}

#[test]
fn doctor_repair_heals_a_title_less_index_and_keeps_it_healed() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_repair_shape.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    regress_to_content_only_index(&conn);

    let (repair, after) = repair_and_rerun(&conn).expect("the repair must run");
    assert_eq!(repair.reindexed, 1, "the repair rebuilds the active row");
    assert!(
        after.healthy,
        "the profile is healthy after the repair: {after:?}"
    );
    assert!(
        check(&after, "fts_shape").ok,
        "the repaired index has the title"
    );
    assert_eq!(
        search_ids(&conn, "puerto"),
        vec!["uuid-puerto".to_string()],
        "the title is searchable after the repair"
    );

    // The repair must also leave the canonical triggers behind: a memory
    // written after it is indexed with its title and topic key.
    insert_titled(&conn, "uuid-nuevo", "puerto de reserva", "", "port = 8082");
    assert_eq!(
        indexed(&conn, "reserva"),
        1,
        "the trigger the repair wrote indexes the title of a new memory"
    );
    conn.execute(
        "UPDATE memories SET title = 'puerto alternativo' WHERE uuid = 'uuid-nuevo'",
        [],
    )
    .unwrap();
    assert_eq!(
        indexed(&conn, "alternativo"),
        1,
        "the trigger the repair wrote re-indexes a title change"
    );
}

// ── migration 007: the index knows the words (agent-keywords S3) ────────────

fn insert_with_words(conn: &Connection, uuid: &str, title: &str, content: &str, words: &str) {
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis, keywords
         ) VALUES (?1, ?2, '', 'ns', 'fact', ?3, 'active', 'user', 'cli', 'user_stated', ?4)",
        rusqlite::params![uuid, title, content, words],
    )
    .expect("insert_with_words must succeed for the test fixture to be valid");
}

/// Put the profile back in the pre-007 state the migration has to heal: the
/// ledger says 6 and the index is the three-column one of migration 006.
fn regress_to_titled_index(conn: &Connection) {
    conn.execute("DROP TABLE memories_fts", [])
        .expect("the pre-007 index is dropped");
    conn.execute(
        "CREATE VIRTUAL TABLE memories_fts USING fts5(content, title, topic_key)",
        [],
    )
    .expect("the pre-007 index is recreated");
    conn.execute(
        "INSERT INTO memories_fts(rowid, content, title, topic_key) \
         SELECT id, content, title, topic_key FROM memories \
         WHERE state = 'active' AND deleted = 0",
        [],
    )
    .expect("the pre-007 index is populated without the words");
    conn.execute("DELETE FROM schema_migrations WHERE version >= 7", [])
        .expect("the ledger goes back to 6");
}

#[test]
fn migration_007_gives_the_memories_the_keywords_column() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig007_column.db");
    let conn = open_sqlite(&path).unwrap();

    let (col_type, dflt, notnull): (String, String, i64) = conn
        .query_row(
            "SELECT type, dflt_value, [notnull] FROM pragma_table_info('memories') \
             WHERE name = 'keywords'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("memories.keywords exists after migration 007");
    assert_eq!(col_type.to_uppercase(), "TEXT");
    assert_eq!(dflt, "''", "an existing memory reads back no words");
    assert_eq!(notnull, 1, "the column is NOT NULL with an empty default");
}

#[test]
fn migration_007_indexes_the_words_of_a_fact() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig007.db");
    let conn = open_sqlite(&path).unwrap();

    // The index carries the four parts of a memory, in the documented order.
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM pragma_table_info('memories_fts')")
            .unwrap();
        let rows = stmt.query_map([], |row| row.get(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(
        columns,
        vec![
            "content".to_string(),
            "title".to_string(),
            "topic_key".to_string(),
            "keywords".to_string()
        ],
        "the index is fts5(content, title, topic_key, keywords)"
    );

    // A record titled in one vocabulary and saved with the words of another:
    // the words find it, because the content says `port`.
    insert_with_words(
        &conn,
        "uuid-puerto",
        "listen port",
        "port = 8081",
        "puerto, puerto de escucha",
    );
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the insert trigger indexes the words"
    );
    assert_eq!(
        indexed(&conn, "escucha"),
        1,
        "every word of the list is searchable, not only the first"
    );
    assert_eq!(
        indexed(&conn, "port"),
        1,
        "the content is still indexed next to the words"
    );
    assert_eq!(
        search_ids(&conn, "puerto"),
        vec!["uuid-puerto".to_string()],
        "the search layer finds the memory by its word"
    );
}

#[test]
fn migration_007_rebuilds_an_index_built_without_the_words() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig007_old.db");
    let conn = open_sqlite(&path).unwrap();
    insert_with_words(
        &conn,
        "uuid-puerto",
        "listen port",
        "port = 8081",
        "puerto, puerto de escucha",
    );

    regress_to_titled_index(&conn);
    assert_eq!(
        indexed(&conn, "puerto"),
        0,
        "the pre-007 index knows the content, the title and the key — not the words"
    );

    run_migrations(&conn).expect("the migration must heal the index");

    assert_eq!(schema_version(&conn), 7);
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the migration rebuilds every active row with its words"
    );
    assert_eq!(search_ids(&conn, "puerto"), vec!["uuid-puerto".to_string()]);
}

#[test]
fn migration_007_is_idempotent_and_probes_before_it_applies() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig007_again.db");
    let conn = open_sqlite(&path).unwrap();
    insert_with_words(&conn, "uuid-puerto", "", "port = 8081", "puerto");

    // A half-applied migration: the schema is already the new one, the ledger
    // lost its row. The probe decides, so the re-run is a no-op that neither
    // fails on the index it already rebuilt nor loses the words.
    conn.execute("DELETE FROM schema_migrations WHERE version = 7", [])
        .unwrap();
    run_migrations(&conn).expect("the re-run must not fail on the index it already built");
    assert_eq!(schema_version(&conn), 7);
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the rebuilt index survived the re-run"
    );

    // And a plain second run is still safe.
    run_migrations(&conn).expect("a second run must be a no-op");
    assert_eq!(schema_version(&conn), 7);
    assert_eq!(indexed(&conn, "puerto"), 1);
}

#[test]
fn migration_007_recreates_the_triggers_with_the_words() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("mig007_triggers.db");
    let conn = open_sqlite(&path).unwrap();
    insert_with_words(&conn, "uuid-t", "", "port = 8081", "puerto");

    // The update trigger follows a word that changes.
    conn.execute(
        "UPDATE memories SET keywords = 'costa' WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(indexed(&conn, "puerto"), 0, "the old word left the index");
    assert_eq!(indexed(&conn, "costa"), 1, "the new word is indexed");

    // A memory that dies leaves the index with its words; one that comes back
    // is re-indexed with them.
    conn.execute(
        "UPDATE memories SET state = 'superseded' WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(indexed(&conn, "costa"), 0, "out-of-active drops the row");
    conn.execute(
        "UPDATE memories SET state = 'active' WHERE uuid = 'uuid-t'",
        [],
    )
    .unwrap();
    assert_eq!(
        indexed(&conn, "costa"),
        1,
        "into-active re-indexes the words"
    );

    conn.execute("DELETE FROM memories WHERE uuid = 'uuid-t'", [])
        .unwrap();
    assert_eq!(indexed(&conn, "costa"), 0, "the delete trigger still fires");
}

#[test]
fn the_doctor_reports_an_index_built_without_the_words() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_words.db");
    let conn = open_sqlite(&path).unwrap();
    insert_with_words(&conn, "uuid-puerto", "listen port", "port = 8081", "puerto");

    regress_to_titled_index(&conn);

    // Same row count as canonical data: the count check cannot see the
    // difference, the shape check has to.
    let report = run_checks(&conn);
    assert!(
        check(&report, "fts_invariant").ok,
        "the row counts agree — that is exactly why a count check is not enough"
    );
    let shape = check(&report, "fts_shape");
    assert!(
        !shape.ok,
        "the doctor must fail an index built without the words"
    );
    assert!(
        shape.detail.contains("keywords"),
        "the detail names the column the index is missing: {}",
        shape.detail
    );
    assert_eq!(
        failure_exit_code(&report),
        Some((6, "INDEX_UNAVAILABLE")),
        "a stale index is INDEX_UNAVAILABLE, and the repair is the way out"
    );

    // A good index passes the same check, and the words are findable again.
    run_migrations(&conn).unwrap();
    let healed = run_checks(&conn);
    assert!(
        check(&healed, "fts_shape").ok,
        "the rebuilt index passes the shape check"
    );
    assert_eq!(search_ids(&conn, "puerto"), vec!["uuid-puerto".to_string()]);
}
