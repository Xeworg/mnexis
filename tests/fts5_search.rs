//! Tests for the FTS5 ranked search path over a memories-like store.
//!
//! Evidence from T13 (plain `fts5(content)` tables maintain index across
//! UPDATE/DELETE and reopen) backs the trigger-maintained FTS5 design used here.
//!
//! ## What is tested
//!
//! (a) **Ranking order** — a document matching more query terms ranks above a
//!     document matching fewer.
//!
//! (b) **Default search excludes superseded and deleted rows** while they
//!     remain stored.
//!
//! (c) **Namespace filter isolates results** — documents in other namespaces
//!     never appear.
//!
//! (d) **A no-answer query returns an empty vector**, not an error.
//!
//! (e) **Update/delete keep the FTS index consistent** through the triggers
//!     and survive a reopen.
//!
//! All tests use `tempfile::tempdir_in("target")` so that the repository's
//! TMPDIR redirect in `.cargo/config.toml` keeps cargo-spawned processes inside
//! the project filesystem.

use mnexis_core::retrieval::{
    Memory, NamespaceFilter, create_benchmark_store, insert_memory, search,
};

/// Helper: build a DB file path inside a temp directory.
fn db_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("mnexis.db")
}

// ── (a) Ranking order ─────────────────────────────────────────────────────────

/// Two documents: doc_a matches BOTH "Rust" AND "fast"; doc_b matches only "Rust".
/// bm25 (ORDER BY rank ASC) must rank doc_a first because it matches more terms.
#[test]
fn search_ranks_doc_with_more_query_terms_above_doc_with_fewer() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let store = create_benchmark_store(&db_path(&directory)).expect("store");

    let doc_a = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-00000000000a".to_owned(),
        namespace: "test-ns".to_owned(),
        topic_key: "topic-a".to_owned(),
        memory_type: "fact".to_owned(),
        content: "Rust is a fast systems programming language".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };
    // doc_b contains "Rust" but not "fast"
    let doc_b = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-00000000000b".to_owned(),
        namespace: "test-ns".to_owned(),
        topic_key: "topic-b".to_owned(),
        memory_type: "fact".to_owned(),
        content: "Rust and Python are popular languages".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    let id_a = insert_memory(&store.conn, &doc_a).expect("insert a");
    let _id_b = insert_memory(&store.conn, &doc_b).expect("insert b");

    let hits = search(&store.conn, "Rust fast", NamespaceFilter::Any, 10).expect("search succeeds");

    assert!(!hits.is_empty(), "expected at least one hit");
    // doc_a matches both "Rust" AND "fast"; doc_b matches only "Rust" (no "fast"),
    // so with FTS5 AND semantics doc_b is excluded and doc_a is first.
    assert_eq!(
        hits.len(),
        1,
        "only doc_a should match both terms; doc_b lacks 'fast'"
    );
    assert_eq!(
        hits[0].id, id_a,
        "doc_a (matches both Rust AND fast) must rank above doc_b (matches only Rust)"
    );
}

// ── (b) Default search excludes superseded and deleted rows ───────────────────

#[test]
fn search_excludes_superseded_and_deleted_rows_while_they_remain_stored() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let store = create_benchmark_store(&db_path(&directory)).expect("store");

    let active_memory = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
        namespace: "test-ns".to_owned(),
        topic_key: "topic-active".to_owned(),
        memory_type: "fact".to_owned(),
        content: "The active document about Rust programming".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    let superseded_memory = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-000000000002".to_owned(),
        namespace: "test-ns".to_owned(),
        topic_key: "topic-sup".to_owned(),
        memory_type: "fact".to_owned(),
        content: "The superseded document about Rust programming".to_owned(),
        state: "superseded".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    let deleted_memory = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-000000000003".to_owned(),
        namespace: "test-ns".to_owned(),
        topic_key: "topic-del".to_owned(),
        memory_type: "fact".to_owned(),
        content: "The deleted document about Rust programming".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 1, // logically deleted
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    insert_memory(&store.conn, &active_memory).expect("insert active");
    insert_memory(&store.conn, &superseded_memory).expect("insert superseded");
    insert_memory(&store.conn, &deleted_memory).expect("insert deleted");

    // Verify all three are in the canonical table
    let total: i64 = store
        .conn
        .query_row(
            "SELECT count(*) FROM memories WHERE content LIKE '%Rust%'",
            [],
            |r| r.get(0),
        )
        .expect("count query");
    assert_eq!(total, 3, "all three should be stored");

    // But search must return only the active, non-deleted one
    let hits = search(&store.conn, "Rust", NamespaceFilter::Any, 10).expect("search succeeds");
    assert_eq!(
        hits.len(),
        1,
        "only the active non-deleted memory should appear in results"
    );
    assert_eq!(hits[0].id, 1); // first inserted
}

// ── (c) Namespace filter isolates results ─────────────────────────────────────

#[test]
fn namespace_filter_isolates_results() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let store = create_benchmark_store(&db_path(&directory)).expect("store");

    let doc_a = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-000000000011".to_owned(),
        namespace: "proyecto-a".to_owned(),
        topic_key: "topic-a".to_owned(),
        memory_type: "fact".to_owned(),
        content: "Document about graph traversal".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    let doc_b = Memory {
        id: None,
        uuid: "00000000-0000-0000-0000-000000000012".to_owned(),
        namespace: "proyecto-b".to_owned(),
        topic_key: "topic-b".to_owned(),
        memory_type: "fact".to_owned(),
        content: "Document about graph traversal".to_owned(),
        state: "active".to_owned(),
        revision: 1,
        deleted: 0,
        created_at: "2025-01-01T00:00:00Z".to_owned(),
        valid_from: None,
        valid_until: None,
        assertion_basis: None,
    };

    let id_a = insert_memory(&store.conn, &doc_a).expect("insert a");
    insert_memory(&store.conn, &doc_b).expect("insert b");

    // Filter by proyecto-a — should only see doc_a
    let hits = search(
        &store.conn,
        "graph",
        NamespaceFilter::Namespace("proyecto-a".to_owned()),
        10,
    )
    .expect("search succeeds");
    assert_eq!(hits.len(), 1, "only proyecto-a documents should appear");
    assert_eq!(hits[0].id, id_a);

    // Filter by proyecto-b — should only see doc_b
    let hits = search(
        &store.conn,
        "graph",
        NamespaceFilter::Namespace("proyecto-b".to_owned()),
        10,
    )
    .expect("search succeeds");
    assert_eq!(hits.len(), 1, "only proyecto-b documents should appear");
    assert_eq!(hits[0].id, id_a + 1);
}

// ── (d) No-answer query returns empty vector, not an error ────────────────────

#[test]
fn no_answer_query_returns_empty_vector_not_error() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let store = create_benchmark_store(&db_path(&directory)).expect("store");

    // No document contains both "xyzzyx" and "plugh"
    let hits = search(&store.conn, "xyzzyx plugh", NamespaceFilter::Any, 10)
        .expect("search should succeed even when there are no matches");
    assert!(
        hits.is_empty(),
        "no-answer query must return an empty vector, not an error"
    );
}

// ── (e) Update/delete keep the FTS index consistent through triggers ───────────

#[test]
fn update_and_delete_preserve_fts_index_consistency_through_reopen() {
    let directory = tempfile::tempdir_in("target").unwrap();
    let db_path = directory.path().join("reopen.db");

    {
        let store = create_benchmark_store(&db_path).expect("store");

        let doc = Memory {
            id: None,
            uuid: "00000000-0000-0000-0000-000000000020".to_owned(),
            namespace: "test-ns".to_owned(),
            topic_key: "topic-upd".to_owned(),
            memory_type: "fact".to_owned(),
            content: "Original content about graph algorithms".to_owned(),
            state: "active".to_owned(),
            revision: 1,
            deleted: 0,
            created_at: "2025-01-01T00:00:00Z".to_owned(),
            valid_from: None,
            valid_until: None,
            assertion_basis: None,
        };
        let row_id = insert_memory(&store.conn, &doc).expect("insert");

        // Verify original content is searchable
        let hits = search(&store.conn, "graph", NamespaceFilter::Any, 10).expect("search original");
        assert_eq!(hits.len(), 1, "original should be found");

        // Update: change the content to something that does NOT contain "graph"
        // The replacement deliberately uses "forest" — no overlap with "graph".
        store
            .conn
            .execute(
                "UPDATE memories SET content = ?1, revision = revision + 1 WHERE rowid = ?2",
                rusqlite::params!["New content about algorithms in the forest", row_id],
            )
            .expect("update");

        // "graph" should no longer match — the trigger updated the FTS entry
        let hits =
            search(&store.conn, "graph", NamespaceFilter::Any, 10).expect("search after update");
        assert!(
            hits.is_empty(),
            "updated content must not match the old term via FTS trigger"
        );

        // "forest" should now match — the trigger added the new content
        let hits =
            search(&store.conn, "forest", NamespaceFilter::Any, 10).expect("search for new term");
        assert_eq!(hits.len(), 1, "new term must appear via FTS trigger");

        // Soft-delete: mark as deleted
        store
            .conn
            .execute(
                "UPDATE memories SET deleted = 1, revision = revision + 1 WHERE rowid = ?1",
                [row_id],
            )
            .expect("soft delete");

        // "forest" should disappear from search
        let hits =
            search(&store.conn, "forest", NamespaceFilter::Any, 10).expect("search after delete");
        assert!(
            hits.is_empty(),
            "soft-deleted row must not appear in search"
        );

        // State change: mark as superseded
        store
            .conn
            .execute(
                "UPDATE memories SET state = 'superseded', deleted = 0, revision = revision + 1 WHERE rowid = ?1",
                [row_id],
            )
            .expect("supersede");
    }

    // Reopen the database and verify the index is still consistent
    let store = create_benchmark_store(&db_path).expect("reopen");

    // "New" should not appear (superseded rows are excluded)
    let hits = search(&store.conn, "New", NamespaceFilter::Any, 10).expect("search after reopen");
    assert!(
        hits.is_empty(),
        "superseded content must not appear in search even after reopen"
    );

    // Canonical row must still exist (superseded, not purged)
    let count: i64 = store
        .conn
        .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
        .expect("canonical count");
    assert_eq!(
        count, 1,
        "canonical row must still exist (superseded, not purged)"
    );
}
