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
        &[],
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
        &[],
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

// ── the explanation follows the index (identity-visibility S2) ──────────────

/// The explanation runs the same MATCH as the search, so a record that only
/// carries the word in its title is explained too.
///
/// The matched-term list is derived from the row's content by `explain`; a
/// title-only match is therefore found and ranked but lists no term. That
/// list is produced outside this task's surfaces and is unchanged for content
/// queries — it is reported as a follow-up, not pinned as a defect here.
#[test]
fn explain_finds_a_record_by_the_words_of_its_title() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES ('uuid-t', 'puerto del servicio', '', 'ns', 'fact', 'port = 8081', \
         'active', 'user', 'cli', 'user_stated')",
        [],
    )
    .unwrap();

    let explained = explain(
        &conn,
        "puerto",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        &[],
        10,
    )
    .expect("the explanation must run");

    assert_eq!(explained.len(), 1, "the title is in the index");
    assert_eq!(explained[0].uuid, "uuid-t");
    assert!(
        explained[0].rank < 0.0,
        "bm25 stays negative; got {}",
        explained[0].rank
    );
}

// ── the explanation carries the same hit fields as the search ────────────────

/// Explain is the same query with more detail: a hit that says when the fact
/// is valid cannot say it in one surface and forget it in the other.
///
/// `retrieval::fts5::search` reads `m.valid_from` and `m.valid_until`; the
/// explanation read neither, so `search --explain` answered with less than the
/// search it explains (post-audit-debts S2).
#[test]
fn explain_carries_the_validity_window_of_each_hit() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    conn.execute(
        "INSERT INTO memories (
            uuid, topic_key, namespace, memory_type, content, state,
            valid_from, valid_until, actor, channel, assertion_basis
         ) VALUES ('uuid-w', '', 'ns', 'fact', 'ventana de validez', 'active',
         '2026-01-01T00:00:00Z', '2026-12-31T00:00:00Z', 'user', 'cli', 'user_stated')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO memories (
            uuid, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES ('uuid-n', '', 'ns', 'fact', 'ventana de validez', 'active',
         'user', 'cli', 'user_stated')",
        [],
    )
    .unwrap();

    let explained = explain(
        &conn,
        "ventana",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        &[],
        10,
    )
    .expect("the explanation must run");

    assert_eq!(explained.len(), 2, "both rows match: {explained:?}");
    let windowed = explained
        .iter()
        .find(|hit| hit.uuid == "uuid-w")
        .expect("the windowed row is explained");
    assert_eq!(
        windowed.valid_from.as_deref(),
        Some("2026-01-01T00:00:00Z"),
        "the explanation carries the start of the window: {windowed:?}"
    );
    assert_eq!(
        windowed.valid_until.as_deref(),
        Some("2026-12-31T00:00:00Z"),
        "and its end: {windowed:?}"
    );
    let open = explained
        .iter()
        .find(|hit| hit.uuid == "uuid-n")
        .expect("the unwindowed row is explained");
    assert_eq!(
        open.valid_from, None,
        "no window is no window, never a made-up one: {open:?}"
    );
    assert_eq!(open.valid_until, None, "{open:?}");
}
