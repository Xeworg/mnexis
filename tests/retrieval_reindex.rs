//! RED tests for `mnexis_core::retrieval::reindex::reindex_all` (T18).
//!
//! Each test exercises ONE documented rule from `odd/tasks/e2-core.md` §9
//! ("Rebuild from canonical data"). The T18 stub returns Err until the
//! GREEN step lands.

use rusqlite::{Connection, params};

use mnexis_core::retrieval::reindex::reindex_all;
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, StateFilter, search};
use mnexis_core::storage::sqlite::open_sqlite;

fn insert_raw(
    conn: &Connection,
    uuid: &str,
    namespace: &str,
    state: &str,
    content: &str,
    deleted: i32,
) {
    conn.execute(
        "INSERT INTO memories (
            uuid, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis, deleted
         ) VALUES (?1, '', ?2, 'fact', ?3, ?4, 'user', 'cli', 'user_stated', ?5)",
        params![uuid, namespace, content, state, deleted],
    )
    .expect("insert_raw must succeed for the test fixture to be valid");
}

fn db_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("mnexis.db")
}

// ── rule 1: reindex rebuilds FTS5 from canonical data ─────────────────────────

#[test]
fn reindex_rebuilds_fts5_from_canonical_data() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(&conn, "uuid-a", "ns", "active", "Rust language", 0);
    insert_raw(&conn, "uuid-b", "ns", "active", "Python language", 0);

    // Manually corrupt the FTS index by deleting rows from `memories_fts`.
    conn.execute("DELETE FROM memories_fts", [])
        .expect("manual FTS corruption must succeed");

    let before = search(
        &conn,
        "Rust",
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(empty)");
    assert!(
        before.is_empty(),
        "FTS5 should be empty after manual corruption"
    );

    let n = reindex_all(&conn).expect("RED stub returns Err; GREEN replaces with Ok(n)");
    assert_eq!(
        n, 2,
        "reindex must report 2 active, non-deleted rows re-indexed"
    );

    let after = search(
        &conn,
        "Rust",
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(
        after.len(),
        1,
        "after reindex, search must find the row again"
    );
    assert_eq!(after[0].uuid, "uuid-a");
}

// ── rule 2: reindex handles a missing (dropped) FTS5 table ────────────────────

#[test]
fn reindex_handles_dropped_fts5_table() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(&conn, "uuid-a", "ns", "active", "Rust language", 0);

    // Drop the FTS5 table entirely. Reindex must recreate it.
    conn.execute("DROP TABLE memories_fts", [])
        .expect("DROP must succeed");

    let n = reindex_all(&conn).expect("RED stub returns Err; GREEN replaces with Ok(n)");
    assert_eq!(n, 1);

    let hits = search(
        &conn,
        "Rust",
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(
        hits.len(),
        1,
        "search must work after reindex recreates the FTS5 table"
    );
    assert_eq!(hits[0].uuid, "uuid-a");
}

// ── rule 3: reindex is idempotent ────────────────────────────────────────────

#[test]
fn reindex_is_idempotent() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    // Three rows; only one is active and not deleted.
    insert_raw(&conn, "uuid-a", "ns", "active", "Rust language", 0);
    insert_raw(&conn, "uuid-b", "ns", "superseded", "Rust language", 0);
    insert_raw(&conn, "uuid-c", "ns", "active", "Rust language", 1);

    let n1 = reindex_all(&conn).expect("RED stub returns Err; GREEN replaces with Ok(n)");
    assert_eq!(
        n1, 1,
        "only the one active + non-deleted row should be re-indexed"
    );

    let n2 = reindex_all(&conn).expect("RED stub returns Err; GREEN replaces with Ok(n)");
    assert_eq!(
        n2, 1,
        "running reindex twice must produce the same result on the same canonical state"
    );

    let hits = search(
        &conn,
        "Rust",
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].uuid, "uuid-a");
}
