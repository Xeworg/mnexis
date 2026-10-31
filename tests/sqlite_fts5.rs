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

use mnexis_core::diagnostics::doctor::{
    apply_repair, dry_run_repair, failure_exit_code, plan_repair, run_checks,
};
use mnexis_core::domain::query::MatchMode;
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, StateFilter, search};
use mnexis_core::storage::sqlite::run_migrations;

/// A record with a title — the identity the priced answers carry — and a
/// kind it declares. The kind is declared because a record that declares
/// nothing is the gap `title_shape`'s sibling `type_shape` reports, and a
/// fixture is meant to be a store the doctor has nothing to say about.
fn insert_titled(conn: &Connection, uuid: &str, title: &str, topic_key: &str, content: &str) {
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES (?1, ?2, ?3, 'ns', 'knowledge', ?4, 'active', 'user', 'cli', 'user_stated')",
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
        &[],
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
        10,
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

    assert_eq!(schema_version(&conn), 10);
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
    assert_eq!(schema_version(&conn), 10);
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the rebuilt index survived the re-run"
    );

    // And a plain second run is still safe.
    run_migrations(&conn).expect("a second run must be a no-op");
    assert_eq!(schema_version(&conn), 10);
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

// ── the --check filter runs exactly one check (S2) ─────────────────────────

#[test]
fn check_ids_round_trip_through_their_codes() {
    for id in mnexis_core::diagnostics::doctor::ALL_CHECK_IDS {
        assert_eq!(
            mnexis_core::diagnostics::doctor::CheckId::from_code(id.as_str()),
            Some(id),
            "every check id parses from its wire form"
        );
    }
    assert_eq!(
        mnexis_core::diagnostics::doctor::CheckId::from_code("nonsense"),
        None,
        "an unknown code does not parse"
    );
}

#[test]
fn a_filtered_run_reports_only_the_requested_check() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_filter.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-filtro",
        "título para el filtro",
        "",
        "contenido para el filtro",
    );

    let report = mnexis_core::diagnostics::doctor::run_checks_for(
        &conn,
        &[mnexis_core::diagnostics::doctor::CheckId::FtsShape],
    );
    assert_eq!(report.checks.len(), 1, "only the requested check ran");
    assert_eq!(report.checks[0].check_id.as_str(), "fts_shape");
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy
    );
    assert!(
        report.summary.contains("1"),
        "the summary counts what ran: {}",
        report.summary
    );
    assert_eq!(
        failure_exit_code(&report),
        None,
        "healthy filtered run exits 0"
    );
}

#[test]
fn a_filtered_run_does_not_see_the_checks_it_did_not_run() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_filter_broken.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(&conn, "uuid-roto", "título roto", "", "contenido");
    conn.execute("DROP TABLE memories_fts", []).unwrap();

    // The FTS family is broken, but it did not run: the filtered report
    // only reflects wal_mode (S4 — the exit code follows what ran).
    let report = mnexis_core::diagnostics::doctor::run_checks_for(
        &conn,
        &[mnexis_core::diagnostics::doctor::CheckId::WalMode],
    );
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy
    );
    assert_eq!(failure_exit_code(&report), None);
}

// ── the envelope: every check has a stable machine contract (S1) ───────────

/// The exact ids the operator can filter with (`--check`) and pin tests on,
/// in the order `run_checks` emits them.
const ALL_CHECK_IDS: [&str; 9] = [
    "schema_version",
    "canonical_tables",
    "fts_readable",
    "wal_mode",
    "integrity",
    "fts_shape",
    "fts_invariant",
    "title_shape",
    "type_shape",
];

#[test]
fn a_healthy_store_reports_the_full_envelope() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_envelope.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-sobre",
        "el título de la memoria",
        "",
        "contenido que el índice lleva",
    );

    let report = run_checks(&conn);
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy
    );
    assert!(
        report.summary.contains("9"),
        "summary counts the checks: {}",
        report.summary
    );

    let ids: Vec<&str> = report.checks.iter().map(|c| c.check_id.as_str()).collect();
    assert_eq!(ids, ALL_CHECK_IDS, "the check ids are the contract");

    for c in &report.checks {
        assert!(c.ok, "{} must pass on a healthy store", c.check_id.as_str());
        assert_eq!(c.severity, mnexis_core::diagnostics::doctor::Severity::Ok);
        assert!(
            c.reason_code.is_none(),
            "a passing check carries no reason code"
        );
        assert!(!c.evidence.is_empty(), "evidence is always present: {c:?}");
        assert!(
            c.safe_next_step.is_none(),
            "a passing check proposes nothing"
        );
    }
}

#[test]
fn the_envelope_serializes_with_the_documented_field_names() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_envelope_json.db");
    let conn = open_sqlite(&path).unwrap();

    let value = serde_json::to_value(run_checks(&conn)).unwrap();
    assert_eq!(value["status"], "healthy");
    assert!(value["summary"].is_string());
    let checks = value["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 9);
    for c in checks {
        assert!(c["check_id"].is_string(), "snake_case check_id: {c}");
        assert!(c["ok"].is_boolean());
        assert!(c["severity"].is_string());
        assert!(c["reason_code"].is_null());
        assert!(c["evidence"].is_string());
        assert!(c["safe_next_step"].is_null());
    }
}

#[test]
fn a_stale_index_carries_its_reason_code_and_the_next_step() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_reason.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    regress_to_content_only_index(&conn);

    let report = run_checks(&conn);
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Failed
    );
    let shape = check(&report, "fts_shape");
    assert!(!shape.ok);
    assert_eq!(
        shape.severity,
        mnexis_core::diagnostics::doctor::Severity::Error
    );
    assert_eq!(
        shape.reason_code,
        Some(mnexis_core::diagnostics::doctor::ReasonCode::FtsStaleShape)
    );
    let step = shape.safe_next_step.as_deref().expect("a next step");
    assert!(
        step.contains("doctor repair"),
        "the next step names the repair command: {step}"
    );
    assert_eq!(
        failure_exit_code(&report),
        Some((6, "INDEX_UNAVAILABLE")),
        "the exit-code contract is unchanged (S4)"
    );
}

#[test]
fn wal_off_is_degraded_but_still_exit_1() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_wal.db");
    let conn = open_sqlite(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=DELETE").unwrap();

    let report = run_checks(&conn);
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Degraded,
        "the store still serves; it is not failed"
    );
    let wal = check(&report, "wal_mode");
    assert_eq!(
        wal.reason_code,
        Some(mnexis_core::diagnostics::doctor::ReasonCode::WalNotActive)
    );
    assert!(
        wal.safe_next_step
            .as_deref()
            .unwrap()
            .contains("doctor repair"),
        "wal is repairable: {}",
        wal.safe_next_step.as_deref().unwrap()
    );
    assert_eq!(
        failure_exit_code(&report),
        Some((1, "INFRASTRUCTURE_ERROR")),
        "a degraded store is still an infrastructure failure (S4)"
    );
}

// ── the FTS invariant reads its counts or fails (A6) ───────────────────────

#[test]
fn the_fts_invariant_fails_when_it_cannot_read_the_counts() {
    // Two unreadable counts are not an agreement. A store whose `memories`
    // and `memories_fts` cannot be read is the store the doctor exists for,
    // and a check that substitutes a sentinel for a failed read compares
    // two sentinels and passes — the one answer a doctor must never give
    // about a store it could not read. Dropping both tables is the only
    // deterministic way a test can make both reads fail: either one alone
    // still leaves the other readable.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_invariant_unreadable.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-ilegible",
        "título ilegible",
        "",
        "contenido ilegible",
    );
    conn.execute("DROP TABLE memories_fts", []).unwrap();
    conn.execute("DROP TABLE memories", []).unwrap();

    let report = run_checks(&conn);
    let invariant = check(&report, "fts_invariant");
    assert!(
        !invariant.ok,
        "a check whose counts were unreadable has not passed: {invariant:?}"
    );
    assert_eq!(
        invariant.reason_code,
        Some(mnexis_core::diagnostics::doctor::ReasonCode::FtsUnreadable),
        "the code says the counts were unreadable, not that the index is in sync: {invariant:?}"
    );
    assert!(
        invariant.evidence.contains("could not be read"),
        "the evidence says the counts could not be read: {}",
        invariant.evidence
    );
    assert!(
        !invariant.evidence.contains("(-1)"),
        "no sentinel stands in for a failed read: {}",
        invariant.evidence
    );
    assert_eq!(
        failure_exit_code(&report),
        Some((6, "INDEX_UNAVAILABLE")),
        "an unreadable index is still INDEX_UNAVAILABLE"
    );
}

#[test]
fn the_fts_invariant_still_sees_a_synced_store_and_a_real_mismatch() {
    // The two behaviours the sentinel must not change: a store whose index
    // carries every live record passes with both counts, and an index that
    // really disagrees fails as out of sync.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_invariant_sync.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-sincronizado",
        "título sincronizado",
        "",
        "contenido sincronizado",
    );

    let synced = run_checks(&conn);
    let invariant = check(&synced, "fts_invariant");
    assert!(invariant.ok, "a synced store passes: {invariant:?}");
    assert_eq!(
        invariant.evidence, "canonical active+non-deleted (1) == FTS5 rows (1)",
        "the pass evidence still carries the two counts it read"
    );

    conn.execute("DELETE FROM memories_fts", []).unwrap();
    let broken = run_checks(&conn);
    let invariant = check(&broken, "fts_invariant");
    assert!(
        !invariant.ok,
        "an index that lost its rows disagrees with the record: {invariant:?}"
    );
    assert_eq!(
        invariant.reason_code,
        Some(mnexis_core::diagnostics::doctor::ReasonCode::FtsOutOfSync),
        "a readable mismatch is out of sync, not unreadable: {invariant:?}"
    );
    assert!(
        invariant
            .evidence
            .contains("canonical active+non-deleted (1) != FTS5 rows (0)"),
        "the evidence reports both counts it read: {}",
        invariant.evidence
    );
    assert_eq!(
        failure_exit_code(&broken),
        Some((6, "INDEX_UNAVAILABLE")),
        "an out-of-sync index is INDEX_UNAVAILABLE"
    );
}

// ── doctor repair: plan, dry-run, apply (S3) ────────────────────────────────

#[test]
fn plan_on_a_healthy_store_has_nothing_to_do() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_plan_ok.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(&conn, "uuid-plan-sano", "título sano", "", "contenido sano");

    let (report, current) = plan_repair(&conn);
    assert_eq!(
        report.mode,
        mnexis_core::diagnostics::doctor::RepairMode::Plan
    );
    assert!(report.actions.is_empty(), "nothing to repair: {report:?}");
    assert!(report.skipped.is_empty());
    assert_eq!(report.reindexed, 0);
    assert!(current.is_healthy());
    assert!(report.healthy_after, "a healthy store stays healthy");
}

#[test]
fn plan_names_the_repairable_checks_and_what_it_cannot_fix() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_plan_roto.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(&conn, "uuid-plan-roto", "título roto", "", "contenido roto");
    conn.execute("DROP TABLE memories_fts", []).unwrap();

    let (report, _current) = plan_repair(&conn);
    let ids: Vec<&str> = report.actions.iter().map(|a| a.check_id.as_str()).collect();
    for expected in [
        "canonical_tables",
        "fts_readable",
        "fts_shape",
        "fts_invariant",
    ] {
        assert!(
            ids.contains(&expected),
            "the plan must name {expected}: {ids:?}"
        );
    }
    assert!(
        report
            .actions
            .iter()
            .all(|a| a.action.as_str() == "rebuild_memories_fts"),
        "every FTS-family failure is healed by the rebuild: {report:?}"
    );
    assert!(report.skipped.is_empty(), "nothing else is broken");
    // And the check's own next step must agree: a missing memories_fts is
    // repairable, so it must point at the repair, not at a backup.
    let canonical = check(&_current, "canonical_tables");
    assert!(
        canonical
            .safe_next_step
            .as_deref()
            .is_some_and(|s| s.contains("doctor repair")),
        "an FTS-only canonical failure points at the repair: {canonical:?}"
    );
    // The wire form of the action matches its as_str().
    let wire = serde_json::to_value(&report).unwrap();
    assert_eq!(
        wire["actions"][0]["action"], "rebuild_memories_fts",
        "the wire form and as_str agree: {wire}"
    );

    // Canonical damage the repair must refuse: a missing memories table is
    // not an index problem.
    conn.execute("DROP TABLE memory_tags", []).unwrap();
    let (refused, _) = plan_repair(&conn);
    assert!(
        refused
            .skipped
            .iter()
            .any(|c| c.check_id.as_str() == "canonical_tables"),
        "canonical damage lands in skipped, not in actions: {refused:?}"
    );
}

#[test]
fn plan_on_a_newer_schema_says_what_it_cannot_fix() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_plan_schema.db");
    let conn = open_sqlite(&path).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO schema_migrations (version) VALUES (99)",
        [],
    )
    .unwrap();

    let (report, current) = plan_repair(&conn);
    assert!(
        report.actions.is_empty(),
        "no repair can fix a newer schema"
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|c| c.check_id.as_str() == "schema_version"),
        "the newer schema lands in skipped: {report:?}"
    );
    assert!(!current.is_healthy());
}

#[test]
fn dry_run_does_the_work_and_rolls_it_back() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_dry.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(
        &conn,
        "uuid-dry",
        "título del dry-run",
        "",
        "contenido del dry-run",
    );
    conn.execute("DROP TABLE memories_fts", []).unwrap();

    let (report, after) = dry_run_repair(&conn).expect("dry run must execute");
    assert_eq!(
        report.mode,
        mnexis_core::diagnostics::doctor::RepairMode::DryRun
    );
    assert_eq!(
        report.reindexed, 1,
        "the work really ran inside the savepoint"
    );
    assert!(
        !after.is_healthy(),
        "and the rollback left the store as broken as before"
    );
    let still = run_checks(&conn);
    assert!(!still.is_healthy(), "no writes survived the dry-run");
    assert_eq!(
        failure_exit_code(&still),
        Some((6, "INDEX_UNAVAILABLE")),
        "a dry-run must not heal the store (S3)"
    );
}

#[test]
fn dry_run_does_not_touch_the_wal_pragma() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_dry_wal.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(&conn, "uuid-wal", "título wal", "", "contenido wal");
    conn.execute_batch("PRAGMA journal_mode=DELETE").unwrap();

    let (report, after) = dry_run_repair(&conn).expect("dry run must execute");
    // The pragma is not transactional: a dry-run lists it, never runs it.
    assert!(!report.wal_reactivated, "the dry run must not set WAL");
    let wal = check(&after, "wal_mode");
    assert!(!wal.ok, "journal_mode is still not wal after the dry run");

    let (applied, healed) = apply_repair(&conn).expect("apply must execute");
    assert!(applied.wal_reactivated);
    assert!(healed.is_healthy(), "apply re-activates WAL: {healed:?}");
}

#[test]
fn apply_on_a_healthy_store_is_a_no_op() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_apply_ok.db");
    let conn = open_sqlite(&path).unwrap();
    insert_titled(&conn, "uuid-ok", "título ok", "", "contenido ok");

    let (report, after) = apply_repair(&conn).expect("apply must execute");
    assert!(
        !report.fts_recreated,
        "nothing was broken, nothing was rebuilt"
    );
    assert_eq!(report.reindexed, 0);
    assert!(after.is_healthy());
}

// ── the doctor tells a stale index from a good one (S2) ─────────────────────

fn check<'a>(
    report: &'a mnexis_core::diagnostics::doctor::DoctorReport,
    name: &str,
) -> &'a mnexis_core::diagnostics::doctor::Check {
    report
        .checks
        .iter()
        .find(|c| c.check_id.as_str() == name)
        .unwrap_or_else(|| {
            panic!(
                "the doctor must report a '{name}' check: {:?}",
                report.checks
            )
        })
}

/// The wire form of the reason code the `type_shape` check emitted.
fn type_shape_code(report: &mnexis_core::diagnostics::doctor::DoctorReport) -> String {
    serde_json::to_value(check(report, "type_shape").reason_code)
        .expect("a reason code serialises")
        .as_str()
        .unwrap_or_default()
        .to_string()
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
        shape.evidence.contains("title") && shape.evidence.contains("topic_key"),
        "the evidence names the columns the index is missing: {}",
        shape.evidence
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

    let (repair, after) = apply_repair(&conn).expect("the repair must run");
    assert_eq!(repair.reindexed, 1, "the repair rebuilds the active row");
    assert!(
        repair.fts_recreated,
        "the FTS rebuild really ran: {repair:?}"
    );
    assert!(
        after.is_healthy(),
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

    assert_eq!(schema_version(&conn), 10);
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
    assert_eq!(schema_version(&conn), 10);
    assert_eq!(
        indexed(&conn, "puerto"),
        1,
        "the rebuilt index survived the re-run"
    );

    // And a plain second run is still safe.
    run_migrations(&conn).expect("a second run must be a no-op");
    assert_eq!(schema_version(&conn), 10);
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
        shape.evidence.contains("keywords"),
        "the evidence names the column the index is missing: {}",
        shape.evidence
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

// ── the type axis: how much of the store is unclassified (types A4) ────────

/// A record that declares the kind of thing it is.
fn insert_typed(conn: &Connection, uuid: &str, title: &str, content: &str, memory_type: &str) {
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES (?1, ?2, '', 'ns', ?3, ?4, 'active', 'user', 'cli', 'user_stated')",
        rusqlite::params![uuid, title, memory_type, content],
    )
    .expect("insert_typed must succeed for the test fixture to be valid");
}

/// A record that declares nothing: what a save lands on when it names no
/// type — the `fact` bucket the `type_shape` check counts.
fn insert_untyped(conn: &Connection, uuid: &str, title: &str, content: &str) {
    insert_typed(conn, uuid, title, content, "fact");
}

/// The store the byte share is measured on: three live records, two of
/// them unclassified, in 11 + 10 + 21 = 42 content bytes, 21 of them
/// unclassified — half the store.
fn unclassified_store(path: &Path) -> Connection {
    let conn = open_sqlite(path).unwrap();
    insert_untyped(
        &conn,
        "uuid-fact-titulado",
        "puerto del servicio",
        "port = 8081",
    );
    insert_untyped(&conn, "uuid-fact-sin-titulo", "", "staging-eu");
    insert_typed(
        &conn,
        "uuid-con-tipo",
        "nunca loggear el payload",
        "never log the payload",
        "knowledge",
    );
    conn
}

#[test]
fn the_type_shape_names_every_unclassified_record_with_its_size() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape.db");
    let conn = unclassified_store(&path);

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");

    // C1 — the count and the byte share of the live content.
    assert!(!shape.ok, "two records declare no type: {shape:?}");
    let evidence = &shape.evidence;
    assert!(
        evidence.contains("2 of 3 live records"),
        "the evidence counts the unclassified records against the live ones: {evidence}"
    );
    assert!(
        evidence.contains("21 of 42 content bytes"),
        "the evidence counts the unclassified bytes against the store's: {evidence}"
    );
    assert!(
        evidence.contains("50%"),
        "the evidence carries the share: {evidence}"
    );

    // C1 — every unclassified record is named with its identity and size.
    assert!(
        evidence.contains("uuid-fact-titulado"),
        "the titled record is named: {evidence}"
    );
    assert!(
        evidence.contains("puerto del servicio") && evidence.contains("11 B"),
        "the titled record carries its title and its 11 bytes: {evidence}"
    );
    assert!(
        evidence.contains("uuid-fact-sin-titulo"),
        "a record without a title is still named, by its uuid: {evidence}"
    );
    assert!(
        evidence.contains("(no title)") && evidence.contains("10 B"),
        "the absence of a title is said, and its 10 bytes are counted: {evidence}"
    );
    // The record that declared its kind is not named: the check counts one
    // bucket, not the store.
    assert!(
        !evidence.contains("uuid-con-tipo"),
        "a classified record is not in the worklist: {evidence}"
    );
    assert!(
        shape.safe_next_step.is_some(),
        "the check says what an operator can do about it: {shape:?}"
    );
}

#[test]
fn the_type_shape_counts_the_aggregate_the_store_reports() {
    // The counts and the byte share come from one aggregate over the live
    // rows. This store is the mixed one the other type tests use, plus the
    // two rows an aggregate can silently get wrong: a soft-deleted `fact`
    // (in neither the numerator nor the denominator) and a live record with
    // no content at all (in the denominator, 0 bytes).
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_aggregate.db");
    let conn = unclassified_store(&path);
    insert_untyped(&conn, "uuid-fact-vacio", "hecho vacío", "");
    insert_untyped(&conn, "uuid-fact-borrado", "hecho borrado", "0123456789");
    conn.execute(
        "UPDATE memories SET deleted = 1 WHERE uuid = 'uuid-fact-borrado'",
        [],
    )
    .unwrap();

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    let evidence = &shape.evidence;

    // 4 live records — 3 `fact` (11 + 10 + 0 bytes) and 1 declared — over
    // 42 + 0 content bytes, 21 of them unselectable: the soft-deleted
    // record's 10 bytes are in neither side of the share.
    assert!(
        evidence.contains("3 of 4 live records"),
        "the soft-deleted record is not counted among the live ones: {evidence}"
    );
    assert!(
        evidence.contains("21 of 42 content bytes") && evidence.contains("50%"),
        "the share counts the live content only: {evidence}"
    );
    assert!(
        evidence.contains("uuid-fact-vacio") && evidence.contains("0 B"),
        "a live record with no content is still named with its 0 bytes: {evidence}"
    );
    assert!(
        !evidence.contains("uuid-fact-borrado"),
        "a soft-deleted record is not in the worklist: {evidence}"
    );
    assert!(
        !evidence.contains("more)"),
        "a bucket under the cap carries no truncation marker: {evidence}"
    );
}

#[test]
fn the_type_shape_names_five_records_and_counts_the_rest() {
    // The `fact` bucket is where every write that declares nothing lands, so
    // a bucket larger than five rows is the normal store, not an edge case.
    // The evidence names a bounded worklist and counts the rest, exactly as
    // the sibling `title_shape` does.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_bounded.db");
    let conn = open_sqlite(&path).unwrap();
    for index in 0..20 {
        insert_untyped(
            &conn,
            &format!("uuid-fact-{index:02}"),
            &format!("hecho número {index:02}"),
            "port = 8081",
        );
    }

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    let evidence = &shape.evidence;

    assert!(
        evidence.contains("20 of 20 live records"),
        "the count is the whole bucket, never the sample: {evidence}"
    );
    let named = (0..20)
        .filter(|index| evidence.contains(&format!("uuid-fact-{index:02}")))
        .count();
    assert_eq!(named, 5, "the worklist is capped at five rows: {evidence}");
    assert!(
        evidence.contains("(+15 more)"),
        "the rows past the cap are counted, not listed: {evidence}"
    );
    assert!(
        !evidence.contains("uuid-fact-05"),
        "the sixth record is past the cap: {evidence}"
    );

    // The cap is the point: the report is one JSON field per failing check,
    // and an evidence string that grows with the store is what A7 measured.
    let json = serde_json::to_string(shape).expect("the check serialises");
    assert!(
        json.len() < 1024,
        "the report stays small with a 20-row bucket: {} bytes",
        json.len()
    );
    assert_eq!(
        shape.severity,
        mnexis_core::diagnostics::doctor::Severity::Warn,
        "a bounded worklist is still a warning: {shape:?}"
    );
    assert_eq!(
        failure_exit_code(&report),
        None,
        "and the run still exits 0"
    );
}

#[test]
fn the_type_shape_sees_the_junk_and_does_not_call_a_declared_fact_unclassified() {
    // A store the check has to tell apart: a declared kind, the generic
    // label written on purpose, and a value no filter can select — what a
    // store that predates the validation, or a hand edit, leaves behind.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_unknown.db");
    let conn = open_sqlite(&path).unwrap();
    let decision_content = "we deploy to staging-eu";
    let fact_content = "port = 8081";
    let junk_content = "esto es una nota";
    insert_typed(
        &conn,
        "uuid-decision",
        "deploy",
        decision_content,
        "decision",
    );
    insert_typed(&conn, "uuid-fact", "puerto", fact_content, "fact");
    insert_typed(&conn, "uuid-junk", "una nota", junk_content, "note");

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    let evidence = &shape.evidence;

    // The junk row is counted and named, with the value the store holds:
    // every other surface reads only what it is asked for, so the doctor is
    // the one that can say it is there at all.
    assert!(
        evidence.contains("unknown type"),
        "the evidence says the type is outside the valid set: {evidence}"
    );
    assert!(
        evidence.contains("1 record: uuid-junk") && evidence.contains("(stored 'note')"),
        "the junk row is counted and named with the value it stores: {evidence}"
    );
    assert!(
        !evidence.contains("uuid-decision"),
        "a declared kind is not in the worklist: {evidence}"
    );

    // And it is inside the share, not only the denominator: the bytes of a
    // row nothing can select are counted as unselectable, so the share the
    // check reports is not understated.
    let unselectable = (fact_content.len() + junk_content.len()) as i64;
    let total = (fact_content.len() + junk_content.len() + decision_content.len()) as i64;
    assert!(
        evidence.contains(&format!("{unselectable} of {total} content bytes")),
        "the junk row is in the numerator, not only in the denominator: {evidence}"
    );

    // The `fact` row is counted under the default label, and the evidence
    // states the documented limit instead of asserting a `fact` was never
    // classified: the store keeps the value and nothing else, so it cannot
    // tell an agent-declared `fact` from a defaulted one.
    assert!(
        evidence.contains("uuid-fact"),
        "the default-labelled row is named: {evidence}"
    );
    assert!(
        evidence.contains("declared or defaulted") && evidence.contains("cannot tell them apart"),
        "the evidence states the limit the store cannot get past: {evidence}"
    );
    assert!(
        !evidence.contains("unclassified"),
        "the check no longer claims a `fact` was never classified: {evidence}"
    );
    assert!(
        evidence.contains("the default label 'fact'") && evidence.contains("an unknown type"),
        "the evidence names both buckets, the default label and the unknown type: {evidence}"
    );

    // A warning, never a failure: a store with a junk type still serves
    // every read it is asked for.
    assert_eq!(
        shape.severity,
        mnexis_core::diagnostics::doctor::Severity::Warn,
        "the gap is a warning, never a failure: {shape:?}"
    );
    assert_eq!(
        serde_json::to_value(shape.reason_code).unwrap(),
        serde_json::json!("TYPES_UNKNOWN"),
        "both buckets are populated, so the code names the harder defect: {shape:?}"
    );
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy,
        "a warning does not move the store's status: {report:?}"
    );
    assert_eq!(
        failure_exit_code(&report),
        None,
        "a warning never fails the run"
    );
}

#[test]
fn the_type_shape_emits_a_code_for_each_bucket() {
    // `Check` carries one reason code, so the buckets are told apart by the
    // code the check emits: the default label keeps the published
    // `TYPES_UNCLASSIFIED`, and a value outside the closed set emits the
    // additive `TYPES_UNKNOWN`. The two stores below are the two buckets,
    // one each, and together they are the set of codes a run can emit.
    let default_only_dir = tempfile::tempdir_in("target").unwrap();
    let default_only =
        open_sqlite(&default_only_dir.path().join("doctor_type_shape_default.db")).unwrap();
    insert_untyped(&default_only, "uuid-fact", "puerto", "port = 8081");
    let default_report = run_checks(&default_only);

    let unknown_only_dir = tempfile::tempdir_in("target").unwrap();
    let unknown_only =
        open_sqlite(&unknown_only_dir.path().join("doctor_type_shape_junk.db")).unwrap();
    insert_typed(
        &unknown_only,
        "uuid-junk",
        "una nota",
        "esto es una nota",
        "note",
    );
    let unknown_report = run_checks(&unknown_only);

    let mut codes = std::collections::BTreeSet::new();
    codes.insert(type_shape_code(&default_report));
    codes.insert(type_shape_code(&unknown_report));
    assert!(
        codes.contains("TYPES_UNCLASSIFIED") && codes.contains("TYPES_UNKNOWN"),
        "the check emits a code per bucket: {codes:?}"
    );

    // A junk type is not a failure either: the store still serves reads,
    // so the run stays healthy and exits 0.
    let junk = check(&unknown_report, "type_shape");
    assert!(
        junk.evidence.contains("uuid-junk") && junk.evidence.contains("(stored 'note')"),
        "the junk bucket is named even when it is the only one: {junk:?}"
    );
    assert_eq!(
        junk.severity,
        mnexis_core::diagnostics::doctor::Severity::Warn,
        "an unknown type is a warning: {junk:?}"
    );
    assert_eq!(
        unknown_report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy,
        "and the store is still healthy: {unknown_report:?}"
    );
    assert_eq!(
        failure_exit_code(&unknown_report),
        None,
        "a warning never fails the run"
    );
    assert_eq!(
        default_report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy,
        "the default label alone does not move the status either: {default_report:?}"
    );
}

#[test]
fn the_type_shape_says_so_when_every_record_declares_a_type() {
    // D1 — the other half of the anti-vacuity criterion: a store where
    // every record declares a type names none, and says so.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_all_typed.db");
    let conn = open_sqlite(&path).unwrap();
    insert_typed(&conn, "uuid-uno", "uno", "port = 8081", "knowledge");
    insert_typed(&conn, "uuid-dos", "dos", "staging-eu", "decision");

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    assert!(shape.ok, "nothing is unclassified: {shape:?}");
    assert!(shape.reason_code.is_none(), "a passing check has no code");
    assert!(
        shape.evidence.contains("2 live records"),
        "the pass says what it looked at: {}",
        shape.evidence
    );
    assert!(
        shape.evidence.contains("0"),
        "and that none of the bytes are unclassified: {}",
        shape.evidence
    );
}

#[test]
fn the_type_shape_is_not_satisfied_by_an_empty_store() {
    // D1 — an empty store passes vacuously, and the evidence must say that
    // it saw nothing instead of claiming every record declares a type.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_empty.db");
    let conn = open_sqlite(&path).unwrap();

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    assert!(
        shape.ok,
        "an empty store has nothing unclassified: {shape:?}"
    );
    assert!(
        shape.evidence.contains("no live records"),
        "the pass says it saw nothing, not that every record is classified: {}",
        shape.evidence
    );
}

#[test]
fn the_type_shape_is_a_warning_the_store_still_serves() {
    // C2 — the run stays healthy and the exit code stays 0: a record
    // nobody classified is read and answered like any other.
    let directory = tempfile::tempdir_in("target").unwrap();
    let path = directory.path().join("doctor_type_shape_warn.db");
    let conn = unclassified_store(&path);

    let report = run_checks(&conn);
    let shape = check(&report, "type_shape");
    assert_eq!(
        shape.severity,
        mnexis_core::diagnostics::doctor::Severity::Warn,
        "the gap is a warning, never a failure: {shape:?}"
    );
    assert_eq!(
        serde_json::to_value(shape.reason_code).unwrap(),
        serde_json::json!("TYPES_UNCLASSIFIED"),
        "the machine code names the gap: {shape:?}"
    );
    assert_eq!(
        report.status,
        mnexis_core::diagnostics::doctor::Status::Healthy,
        "a warning does not move the store's status: {report:?}"
    );
    assert_eq!(
        failure_exit_code(&report),
        None,
        "a warning never fails the run"
    );
    assert!(
        report.summary.contains("type_shape"),
        "the summary names the warning: {}",
        report.summary
    );
    assert!(
        report.summary.contains("warned") && !report.summary.contains("failed"),
        "the summary counts a warning as a warning: {}",
        report.summary
    );

    // And no repair is planned for it: a type is the writer's declaration,
    // and a repair that guessed one would store a guess as a fact.
    let (repair, _) = plan_repair(&conn);
    assert!(
        repair
            .actions
            .iter()
            .all(|a| a.check_id.as_str() != "type_shape"),
        "nothing repairs an unclassified record: {repair:?}"
    );
    assert!(
        repair
            .skipped
            .iter()
            .any(|c| c.check_id.as_str() == "type_shape"),
        "the plan says out loud that it will not touch it: {repair:?}"
    );
    assert_eq!(
        repair.skipped.len(),
        1,
        "the only warning on this store is the unclassified one: {repair:?}"
    );
}

#[test]
fn an_unclassified_record_moves_no_other_check() {
    // C3, in the suite: two stores that differ only in the kind their
    // records declare. The eight checks that predate `type_shape` read
    // nothing about the type, so their entries must be byte-identical.
    let typed_dir = tempfile::tempdir_in("target").unwrap();
    let typed_path = typed_dir.path().join("doctor_type_shape_typed.db");
    let typed = open_sqlite(&typed_path).unwrap();
    insert_typed(&typed, "uuid-uno", "uno", "port = 8081", "knowledge");
    insert_typed(&typed, "uuid-dos", "dos", "staging-eu", "decision");

    let untyped_dir = tempfile::tempdir_in("target").unwrap();
    let untyped_path = untyped_dir.path().join("doctor_type_shape_untyped.db");
    let untyped = open_sqlite(&untyped_path).unwrap();
    insert_untyped(&untyped, "uuid-uno", "uno", "port = 8081");
    insert_untyped(&untyped, "uuid-dos", "dos", "staging-eu");

    let other_checks = |conn: &Connection| {
        let mut value = serde_json::to_value(run_checks(conn)).unwrap();
        let checks = value["checks"].as_array_mut().expect("checks array");
        checks.retain(|c| c["check_id"] != "type_shape");
        serde_json::to_string(checks).unwrap()
    };
    assert_eq!(
        other_checks(&typed),
        other_checks(&untyped),
        "the eight checks read nothing about the type"
    );
}
