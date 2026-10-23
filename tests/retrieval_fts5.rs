//! RED tests for `mnexis_core::retrieval::fts5::search` (T18).
//!
//! Each test exercises ONE documented rule from `odd/tasks/e2-core.md` §9.
//! The T18 stubs return Err until the GREEN step lands; every test below
//! should fail with "called Result::unwrap() on an Err value" until then.
//!
//! Conventions:
//! - `tempdir_in("target")` keeps cargo-spawned processes inside the project
//!   filesystem (TMPDIR redirect applied at L72).
//! - Insertion is via direct SQL so the tests can pin arbitrary `state`,
//!   `deleted`, and validity windows without depending on the service layer.

use rusqlite::{Connection, params};

use mnexis_core::domain::query::MatchMode;
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, StateFilter, ValidWindow, search};
use mnexis_core::storage::sqlite::open_sqlite;

/// Insert one memory row directly via SQL (bypassing service/repo) so each
/// test can pin the exact `state`, `deleted`, and validity window it needs.
#[allow(clippy::too_many_arguments)]
fn insert_raw(
    conn: &Connection,
    uuid: &str,
    namespace: &str,
    topic_key: &str,
    memory_type: &str,
    state: &str,
    content: &str,
    deleted: i32,
    valid_from: Option<&str>,
    valid_until: Option<&str>,
) {
    conn.execute(
        "INSERT INTO memories (
            uuid, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis,
            deleted, valid_from, valid_until
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'user', 'cli', 'user_stated', ?7, ?8, ?9)",
        params![
            uuid,
            topic_key,
            namespace,
            memory_type,
            content,
            state,
            deleted,
            valid_from,
            valid_until
        ],
    )
    .expect("insert_raw: row insert must succeed for the test fixture to be valid");
}

fn db_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("mnexis.db")
}

// ── rule 1: default search returns active-only ────────────────────────────────

#[test]
fn search_default_excludes_non_active_states() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn, "uuid-a", "ns", "topic-a", "fact", "active", "Rust", 0, None, None,
    );
    insert_raw(
        &conn,
        "uuid-b",
        "ns",
        "topic-b",
        "fact",
        "superseded",
        "Rust",
        0,
        None,
        None,
    );
    insert_raw(
        &conn, "uuid-c", "ns", "topic-c", "fact", "proposed", "Rust", 0, None, None,
    );

    let hits = search(
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
        hits.len(),
        1,
        "only the active memory should appear by default (state filter default)"
    );
    assert_eq!(hits[0].uuid, "uuid-a");
}

// ── rule 2: default search excludes soft-deleted rows ─────────────────────────

#[test]
fn search_default_excludes_soft_deleted_rows() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn, "uuid-a", "ns", "topic-a", "fact", "active", "Rust", 0, None, None,
    );
    insert_raw(
        &conn, "uuid-b", "ns", "topic-b", "fact", "active", "Rust", 1, None, None,
    );

    let hits = search(
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
        hits.len(),
        1,
        "soft-deleted rows must not appear in default search"
    );
    assert_eq!(hits[0].uuid, "uuid-a");
}

// ── rule 3: namespace filter isolates results ────────────────────────────────

#[test]
fn search_namespace_filter_isolates_results() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-a",
        "proyecto-a",
        "topic-a",
        "fact",
        "active",
        "graph",
        0,
        None,
        None,
    );
    insert_raw(
        &conn,
        "uuid-b",
        "proyecto-b",
        "topic-b",
        "fact",
        "active",
        "graph",
        0,
        None,
        None,
    );

    let hits_a = search(
        &conn,
        "graph",
        MatchMode::Any,
        NamespaceFilter::Namespace("proyecto-a".to_string()),
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(hits_a.len(), 1);
    assert_eq!(hits_a[0].namespace, "proyecto-a");
    assert_eq!(hits_a[0].uuid, "uuid-a");
}

// ── rule 4: StateFilter::Any is a no-op against the active-only FTS5 index ─

#[test]
fn search_state_filter_any_matches_default_when_index_is_active_only() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn, "uuid-a", "ns", "topic-a", "fact", "active", "Rust", 0, None, None,
    );
    insert_raw(
        &conn,
        "uuid-b",
        "ns",
        "topic-b",
        "fact",
        "superseded",
        "Rust",
        0,
        None,
        None,
    );

    // The canonical `memories_fts` index only contains active + non-deleted
    // rows (by the trigger invariant established in MIGRATION_001 and
    // MIGRATION_002). StateFilter::Any therefore CANNOT surface superseded
    // rows — they are not in FTS5 to begin with. The variant is preserved
    // on the API for future extensibility but is a no-op against the
    // current index invariant.
    let hits_default = search(
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
    let hits_any = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(hits_default.len(), 1);
    assert_eq!(
        hits_any.len(),
        1,
        "StateFilter::Any must not widen the FTS5-indexed set (index invariant)"
    );
    assert_eq!(hits_any[0].uuid, "uuid-a");
}

// ── rule 5: DeletedFilter::Any is a no-op against the not-deleted index ────

#[test]
fn search_deleted_filter_any_matches_default_when_index_is_not_deleted_only() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn, "uuid-a", "ns", "topic-a", "fact", "active", "Rust", 0, None, None,
    );
    insert_raw(
        &conn, "uuid-b", "ns", "topic-b", "fact", "active", "Rust", 1, None, None,
    );

    // Same invariant as rule 4: the canonical `memories_fts` index contains
    // only active + non-deleted rows. DeletedFilter::Any therefore CANNOT
    // surface soft-deleted rows; the variant is preserved for symmetry.
    let hits_default = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    let hits_any = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(hits_default.len(), 1);
    assert_eq!(
        hits_any.len(),
        1,
        "DeletedFilter::Any must not widen the FTS5-indexed set (index invariant)"
    );
    assert_eq!(hits_any[0].uuid, "uuid-a");
}

// ── rule 6: valid_window restricts by validity interval ───────────────────────

#[test]
fn search_valid_window_excludes_out_of_window_records() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    // In-window: 2026-01 to 2026-12.
    insert_raw(
        &conn,
        "uuid-a",
        "ns",
        "topic-a",
        "fact",
        "active",
        "Rust",
        0,
        Some("2026-01-01T00:00:00Z"),
        Some("2026-12-31T23:59:59Z"),
    );
    // Outside window: only valid during 2025.
    insert_raw(
        &conn,
        "uuid-b",
        "ns",
        "topic-b",
        "fact",
        "active",
        "Rust",
        0,
        Some("2025-01-01T00:00:00Z"),
        Some("2025-12-31T23:59:59Z"),
    );

    let window = ValidWindow {
        valid_from: Some("2026-06-01T00:00:00Z".to_string()),
        valid_until: Some("2026-06-30T23:59:59Z".to_string()),
    };

    let hits = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        Some(window),
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(
        hits.len(),
        1,
        "only the in-window record should appear at the reference time"
    );
    assert_eq!(hits[0].uuid, "uuid-a");
}

// ── rule 7: hit carries the full column set ──────────────────────────────────

#[test]
fn search_hit_carries_full_column_set() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-a",
        "ns",
        "topic-a",
        "decision",
        "active",
        "Rust language chosen",
        0,
        Some("2026-01-01T00:00:00Z"),
        Some("2026-12-31T23:59:59Z"),
    );

    let hits = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");

    assert_eq!(hits.len(), 1);
    let h = &hits[0];
    assert_eq!(h.uuid, "uuid-a");
    assert_eq!(h.namespace, "ns");
    assert_eq!(h.memory_type, "decision");
    assert_eq!(h.state, "active");
    assert_eq!(h.revision, 1);
    assert!(!h.created_at.is_empty(), "created_at must be populated");
    assert_eq!(h.valid_from.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert_eq!(h.valid_until.as_deref(), Some("2026-12-31T23:59:59Z"));
    assert!(
        h.rank < 0.0,
        "bm25 ranks must be negative (more negative = more relevant); got {}",
        h.rank
    );
}

// ── rule 8: a no-answer query returns an empty vec, not an error ─────────────

#[test]
fn search_empty_query_returns_empty_vec_not_error() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn, "uuid-a", "ns", "topic-a", "fact", "active", "Rust", 0, None, None,
    );

    let hits = search(
        &conn,
        "xyzzyx plugh",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(empty)");

    assert!(
        hits.is_empty(),
        "a query with no matches must return an empty vec, not error"
    );
}

// ── rule 9: UPDATE of content reindexes FTS5 (regression for migration 002) ─

#[test]
fn search_update_of_content_reindexes_after_update_trigger() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-a",
        "ns",
        "topic-a",
        "fact",
        "active",
        "graph algorithms",
        0,
        None,
        None,
    );

    // Sanity: 'graph' matches the inserted content.
    let hits_before = search(
        &conn,
        "graph",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(
        hits_before.len(),
        1,
        "the inserted row should match the original term"
    );

    // Update content (no state/deleted change) — the new content "forest"
    // must replace "graph" via the trigger on UPDATE OF content.
    conn.execute(
        "UPDATE memories SET content = ?1, revision = revision + 1 WHERE uuid = ?2",
        params!["forest algorithms", "uuid-a"],
    )
    .expect("UPDATE must succeed for the test fixture to be valid");

    let hits_for_graph = search(
        &conn,
        "graph",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert!(
        hits_for_graph.is_empty(),
        "after UPDATE of content, the old term 'graph' must disappear (regression: missing trigger)"
    );

    let hits_for_forest = search(
        &conn,
        "forest",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(
        hits_for_forest.len(),
        1,
        "the new content 'forest' must be in FTS5 after UPDATE"
    );
}

// ── rule 10: search survives a reopen ────────────────────────────────────────

#[test]
fn search_survives_reopen() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = db_path(&dir);
    {
        let conn = open_sqlite(&path).unwrap();
        insert_raw(
            &conn,
            "uuid-a",
            "ns",
            "topic-a",
            "fact",
            "active",
            "Rust language",
            0,
            None,
            None,
        );
        drop(conn);
    }
    let conn = open_sqlite(&path).unwrap();
    let hits = search(
        &conn,
        "Rust",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::Any,
        DeletedFilter::Any,
        None,
        10,
    )
    .expect("RED stub returns Err; GREEN replaces with Ok(hits)");
    assert_eq!(hits.len(), 1, "search must work after reopen");
    assert_eq!(hits[0].uuid, "uuid-a");
}

// ── the query contract: what the agent asks, the engine answers ──────────────
//
// These are the rules the benchmark could not see: a hyphen is part of a
// word, a multi-word query is not a phrase, and a query with FTS5 syntax in
// it is still just words.

/// A hyphenated query finds the memory instead of failing to parse.
#[test]
fn a_hyphenated_query_finds_the_memory() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-h",
        "ns",
        "topic-h",
        "fact",
        "active",
        "El deploy por defecto apunta a staging-eu",
        0,
        None,
        None,
    );
    let hits = search(
        &conn,
        "staging-eu",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("a hyphenated query must not fail");
    assert_eq!(hits.len(), 1, "the hyphenated query must find its memory");
    assert_eq!(hits[0].uuid, "uuid-h");
}

/// A multi-word query counts every word: it is not an implicit phrase.
#[test]
fn a_multi_word_query_is_not_a_phrase() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-m",
        "ns",
        "topic-m",
        "fact",
        "active",
        "El servicio escucha en el puerto 8081",
        0,
        None,
        None,
    );
    let hits = search(
        &conn,
        "puerto base de datos",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("a natural-language query must not fail");
    assert_eq!(
        hits.len(),
        1,
        "a natural-language query must find the memory that holds one of its words"
    );
}

/// `all` requires every word; `phrase` requires them in order.
#[test]
fn the_match_mode_decides_what_the_query_means() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-a",
        "ns",
        "topic-a",
        "fact",
        "active",
        "El servicio escucha en el puerto 8081",
        0,
        None,
        None,
    );
    let all = search(
        &conn,
        "puerto 8081",
        MatchMode::All,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("all-mode search must succeed");
    assert_eq!(all.len(), 1, "both words are present");

    let phrase = search(
        &conn,
        "8081 puerto",
        MatchMode::Phrase,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("phrase search must succeed");
    assert!(
        phrase.is_empty(),
        "the words are not in that order in the content, so the phrase must not match"
    );
}

/// FTS5 syntax inside the query is just words, never a parse error.
#[test]
fn fts5_syntax_inside_a_query_is_treated_as_words() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-c",
        "ns",
        "topic-c",
        "fact",
        "active",
        "El contacto de soporte es el oncall",
        0,
        None,
        None,
    );
    let hits = search(
        &conn,
        "contacto: oncall NEAR something",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("a query with FTS5 syntax must not fail the parse");
    assert_eq!(hits.len(), 1, "the query must find the memory");
}

/// A query with nothing to search for is the caller's mistake, not the
/// index's: it has to be an invalid argument, not a busy store.
#[test]
fn a_query_without_words_is_an_invalid_argument() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    let error = search(
        &conn,
        "   ",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect_err("an empty query must fail");
    assert_eq!(error.error_code_label(), "INVALID_ARGUMENT");
    assert!(
        !matches!(error, mnexis_core::domain::error::MnexisError::Busy { .. }),
        "an empty query must never be reported as a busy store"
    );
}

/// No text a caller can send reaches FTS5 as syntax, in any mode.
#[test]
fn no_query_text_can_reach_fts5_as_syntax() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_raw(
        &conn,
        "uuid-c",
        "ns",
        "topic-c",
        "fact",
        "active",
        "El contacto de soporte es el oncall",
        0,
        None,
        None,
    );
    // The queries an agent writes, and the ones a hostile caller writes.
    let queries = [
        "staging-eu",
        "v1.2",
        "user@example.com",
        "postgres://localhost:5432/payments",
        "a OR b",
        "NEAR(x, 3)",
        "content:puerto",
        "\"quoted phrase\"",
        "*",
        "AND",
        "\"\"",
        "say \"hi\" now",
        "-",
        "...",
    ];
    for q in queries {
        for mode in [MatchMode::Any, MatchMode::All, MatchMode::Phrase] {
            match search(
                &conn,
                q,
                mode,
                NamespaceFilter::Any,
                StateFilter::default(),
                DeletedFilter::default(),
                None,
                10,
            ) {
                Ok(_) => {}
                Err(e) => assert_eq!(
                    e.error_code_label(),
                    "INVALID_ARGUMENT",
                    "q={q:?} mode={mode:?} must never be BUSY or INDEX_UNAVAILABLE: {e}"
                ),
            }
        }
    }
    // And the hyphenated query still finds its memory.
    let hits = search(
        &conn,
        "staging-eu",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("the hyphenated query must run");
    assert_eq!(
        hits.len(),
        0,
        "no memory holds that word, but the query ran"
    );
}

// ── the index knows the title and the topic key (identity-visibility S2) ─────

/// Insert one active memory with a title, so a test can pin what the index is
/// supposed to know about a record besides its content.
fn insert_titled(conn: &Connection, uuid: &str, title: &str, topic_key: &str, content: &str) {
    conn.execute(
        "INSERT INTO memories (
            uuid, title, topic_key, namespace, memory_type, content, state,
            actor, channel, assertion_basis
         ) VALUES (?1, ?2, ?3, 'ns', 'fact', ?4, 'active', 'user', 'cli', 'user_stated')",
        params![uuid, title, topic_key, content],
    )
    .expect("insert_titled: row insert must succeed for the test fixture to be valid");
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

/// S2: a record whose content does not hold the word is found by the words of
/// its title. This is the measured failure: content `port = 8081`, title
/// `puerto del servicio`, and the caller searches `puerto`.
#[test]
fn a_memory_is_found_by_the_words_of_its_title() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_titled(
        &conn,
        "uuid-puerto",
        "puerto del servicio",
        "",
        "port = 8081",
    );

    assert_eq!(
        search_ids(&conn, "puerto"),
        vec!["uuid-puerto".to_string()],
        "the title must be in the index: searching 'puerto' finds the record \
         whose content says 'port = 8081'"
    );
    assert_eq!(
        search_ids(&conn, "servicio"),
        vec!["uuid-puerto".to_string()],
        "every word of the title is searchable, not only the first"
    );
}

/// S2: the topic key is the identity of the fact and is indexed too, so a
/// caller who knows the key finds the record without knowing its content.
#[test]
fn a_memory_is_found_by_the_words_of_its_topic_key() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_titled(&conn, "uuid-key", "", "puerto-del-servicio", "port = 8081");

    assert_eq!(
        search_ids(&conn, "puerto"),
        vec!["uuid-key".to_string()],
        "the topic key must be in the index"
    );
    assert_eq!(
        search_ids(&conn, "servicio"),
        vec!["uuid-key".to_string()],
        "a hyphenated topic key is searchable by its words"
    );
}

/// S2: the triggers cover the new columns. A title that changes must be
/// re-indexed, and the word it no longer holds must leave the index.
#[test]
fn a_title_change_is_reindexed() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_titled(
        &conn,
        "uuid-renamed",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    assert_eq!(
        search_ids(&conn, "puerto"),
        vec!["uuid-renamed".to_string()]
    );

    conn.execute(
        "UPDATE memories SET title = 'base de datos', topic_key = 'base-de-datos', \
         revision = revision + 1 WHERE uuid = 'uuid-renamed'",
        [],
    )
    .expect("the title change must succeed");

    assert!(
        search_ids(&conn, "puerto").is_empty(),
        "the old title must leave the index when the title changes"
    );
    assert_eq!(
        search_ids(&conn, "datos"),
        vec!["uuid-renamed".to_string()],
        "the new title must be in the index after UPDATE of title"
    );
}

/// S2 does not weaken the invariant the index already had: it holds the
/// active, non-deleted records only, whatever their title says.
#[test]
fn a_record_that_is_not_alive_is_not_found_by_its_title() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_titled(&conn, "uuid-dead", "puerto del servicio", "", "port = 8081");

    conn.execute(
        "UPDATE memories SET state = 'superseded' WHERE uuid = 'uuid-dead'",
        [],
    )
    .unwrap();

    assert!(
        search_ids(&conn, "puerto").is_empty(),
        "a superseded record must not be found by its title"
    );
}

/// S2 keeps the content path intact: a content word still finds the record
/// and still outranks a record that only carries the word in its title.
#[test]
fn a_content_match_still_ranks_above_a_title_only_match() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let conn = open_sqlite(&db_path(&dir)).unwrap();
    insert_titled(
        &conn,
        "uuid-title-only",
        "puerto del servicio",
        "",
        "port = 8081",
    );
    insert_titled(
        &conn,
        "uuid-content",
        "configuracion del servicio",
        "",
        "puerto puerto puerto: el puerto es 8081 y el puerto de reserva 8082",
    );

    let hits = search(
        &conn,
        "puerto",
        MatchMode::Any,
        NamespaceFilter::Any,
        StateFilter::default(),
        DeletedFilter::default(),
        None,
        10,
    )
    .expect("the query must run");
    assert_eq!(hits.len(), 2, "both records carry the word now");
    assert_eq!(
        hits[0].uuid, "uuid-content",
        "a record that repeats the word in its content must rank above one \
         that only carries it in its title"
    );
}
