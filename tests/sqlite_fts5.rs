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
