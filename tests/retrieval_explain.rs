//! RED tests for `mnexis_core::retrieval::explain::explain` (T18).
//!
//! Each test exercises ONE documented rule from `odd/tasks/e2-core.md` §9.
//! The T18 stubs return Err until the GREEN step lands.

use rusqlite::{Connection, params};

use mnexis_core::domain::query::MatchMode;
use mnexis_core::retrieval::explain::explain;
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, StateFilter};
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

// ── rule 1: explain returns matched_terms + rank for every hit ───────────────

#[test]
fn explain_returns_hit_with_matched_terms_and_rank() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(&conn, "uuid-a", "ns", "active", "Rust language", 0);
    insert_raw(&conn, "uuid-b", "ns", "active", "Rust language", 0);

    let explained = explain(
        &conn,
        "Rust language",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(explained.len(), 2, "both rows contain both terms");
    for hit in &explained {
        assert!(
            !hit.matched_terms.is_empty(),
            "explanation must include the matched terms for each hit"
        );
        assert!(
            hit.matched_terms.iter().any(|t| t == "rust"),
            "matched_terms must list 'rust' (lower-cased FTS5 token)"
        );
        assert!(
            hit.matched_terms.iter().any(|t| t == "language"),
            "matched_terms must list 'language'"
        );
        assert!(
            hit.rank < 0.0,
            "bm25 rank must be negative (more negative = more relevant); got {}",
            hit.rank
        );
        assert!(!hit.uuid.is_empty());
    }
}

// ── rule 2: explain respects the same state/deleted filters as search ────────

#[test]
fn explain_default_excludes_superseded_and_deleted_rows() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(&conn, "uuid-a", "ns", "active", "Rust", 0);
    insert_raw(&conn, "uuid-b", "ns", "superseded", "Rust", 0);
    insert_raw(&conn, "uuid-c", "ns", "active", "Rust", 1);

    let explained = explain(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(
        explained.len(),
        1,
        "default explain must exclude superseded and soft-deleted rows"
    );
    assert_eq!(explained[0].uuid, "uuid-a");
}
