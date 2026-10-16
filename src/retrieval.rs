//! Ranked FTS5 search path over a memories-like store.
//!
//! Schema
//! ------
//! - `memories` — canonical table (see `create_benchmark_store`)
//! - `memories_fts` — PLAIN `fts5(content)` table, not external-content.
//!   Kept in sync by AFTER INSERT/UPDATE/DELETE triggers on `memories`.
//!   Evidence: T13 proved plain `fts5(content)` tables maintain index
//!   across UPDATE/DELETE and reopen; no external-content table is needed.
//!
//! Search
//! ------
//! `search` uses `MATCH` with `ORDER BY rank` (bm25). The default filter
//! excludes `state != 'active'` and `deleted != 0`. A namespace filter can
//! further restrict results.  A query with no matching documents returns an
//! empty `Vec`, never an error.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result row from `search`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Canonical `rowid` of the matched memory.
    pub id: i64,
    /// bm25 rank — lower is better (more relevant).
    pub rank: f64,
}

/// Namespace filter applied to every search.
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceFilter {
    /// Match memories in any namespace.
    Any,
    /// Match memories in exactly this namespace.
    Namespace(String),
}

/// A memory record as inserted into the canonical table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Database-assigned rowid. `None` on insert.
    pub id: Option<i64>,
    pub uuid: String,
    pub namespace: String,
    pub topic_key: String,
    pub memory_type: String,
    pub content: String,
    pub state: String,
    pub revision: i32,
    pub deleted: i32,
    pub created_at: String,
    /// Optional validity window start (ISO-8601).
    pub valid_from: Option<String>,
    /// Optional validity window end (ISO-8601).
    pub valid_until: Option<String>,
    /// Optional provenance annotation.
    pub assertion_basis: Option<String>,
}

/// Open handle returned by `create_benchmark_store`.
pub struct BenchmarkStore {
    pub conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid        TEXT    NOT NULL,
    namespace   TEXT    NOT NULL,
    topic_key   TEXT    NOT NULL DEFAULT '',
    memory_type TEXT    NOT NULL DEFAULT 'fact',
    content     TEXT    NOT NULL,
    state       TEXT    NOT NULL
                CHECK (state IN ('proposed','active','disputed','superseded','archived')),
    revision    INTEGER NOT NULL DEFAULT 1,
    deleted     INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    valid_from  TEXT,
    valid_until TEXT,
    assertion_basis TEXT
);

-- PLAIN FTS5 table; NOT external-content.
-- T13 evidence backs plain tables: they maintain index across UPDATE/DELETE
-- and survive reopen without explicit rebuild.
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(content);
"#;

// AFTER INSERT — sync new row into FTS
const TRIGGER_INSERT: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_after_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;
"#;

// AFTER UPDATE — sync changed content into FTS (re-index if content changed,
// or remove if row became non-active/deleted)
const TRIGGER_UPDATE: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_after_update_content
AFTER UPDATE OF content ON memories BEGIN
    UPDATE memories_fts SET content = new.content WHERE rowid = new.id;
END;

CREATE TRIGGER IF NOT EXISTS memories_after_update_state_deleted
AFTER UPDATE OF state, deleted ON memories BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
END;
"#;

// AFTER DELETE — remove row from FTS
const TRIGGER_DELETE: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_after_delete AFTER DELETE ON memories BEGIN
    DELETE FROM memories_fts WHERE rowid = old.id;
END;
"#;

impl BenchmarkStore {
    /// Open (or create) a benchmark store at `path`.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(TRIGGER_INSERT)?;
        conn.execute_batch(TRIGGER_UPDATE)?;
        conn.execute_batch(TRIGGER_DELETE)?;
        Ok(Self { conn })
    }
}

/// Open (or create) the benchmark store and verify the FTS5 table exists.
pub fn create_benchmark_store(path: &Path) -> Result<BenchmarkStore, rusqlite::Error> {
    BenchmarkStore::open(path)
}

const INSERT_MEMORY_SQL: &str = r#"
    INSERT INTO memories (
        uuid, namespace, topic_key, memory_type, content,
        state, revision, deleted, created_at,
        valid_from, valid_until, assertion_basis
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
"#;

/// Insert one memory record into the canonical table inside a transaction.
/// Returns the assigned rowid.
///
/// The FTS trigger handles index maintenance; no manual FTS write is needed.
pub fn insert_memory(conn: &Connection, mem: &Memory) -> Result<i64, rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        INSERT_MEMORY_SQL,
        params![
            mem.uuid,
            mem.namespace,
            mem.topic_key,
            mem.memory_type,
            mem.content,
            mem.state,
            mem.revision,
            mem.deleted,
            mem.created_at,
            mem.valid_from,
            mem.valid_until,
            mem.assertion_basis,
        ],
    )?;
    let row_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |r| r.get(0))?;
    tx.commit()?;
    Ok(row_id)
}

/// Insert many memories inside a single transaction (bulk corpus loading).
///
/// Single-memory writes stay transactional through [`insert_memory`], which
/// matches the PRD's one-operation-one-transaction rule; this batch helper
/// exists only for benchmark corpus ingestion.
pub fn insert_memories(conn: &Connection, memories: &[Memory]) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(INSERT_MEMORY_SQL)?;
        for mem in memories {
            stmt.execute(params![
                mem.uuid,
                mem.namespace,
                mem.topic_key,
                mem.memory_type,
                mem.content,
                mem.state,
                mem.revision,
                mem.deleted,
                mem.created_at,
                mem.valid_from,
                mem.valid_until,
                mem.assertion_basis,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Ranked FTS5 search over active, non-deleted memories.
///
/// - `query` — FTS5 query text (space-separated terms).
/// - `namespace` — optional namespace restriction.
/// - `limit` — maximum number of results.
///
/// Returns hits ordered by bm25 rank (ascending, best first).
/// A query with no matching documents returns an empty `Vec`, not an error.
pub fn search(
    conn: &Connection,
    query: &str,
    namespace: NamespaceFilter,
    limit: usize,
) -> Result<Vec<SearchHit>, rusqlite::Error> {
    let base_sql = r#"
        SELECT m.id, bm25(memories_fts) AS rank
        FROM memories_fts
        JOIN memories AS m ON memories_fts.rowid = m.id
        WHERE memories_fts MATCH ?1
          AND m.state = 'active'
          AND m.deleted = 0
    "#;
    let order_sql = " ORDER BY rank ASC LIMIT ?2";

    let map_row = |row: &rusqlite::Row| {
        Ok(SearchHit {
            id: row.get(0)?,
            rank: row.get(1)?,
        })
    };

    let hits: Vec<SearchHit> = match &namespace {
        NamespaceFilter::Any => {
            let mut stmt = conn.prepare(&format!("{base_sql}{order_sql}"))?;
            stmt.query_map(params![query, limit as i64], map_row)?
                .collect::<Result<Vec<_>, _>>()?
        }
        NamespaceFilter::Namespace(ns) => {
            let sql = format!("{base_sql} AND m.namespace = ?3{order_sql}");
            let mut stmt = conn.prepare(&sql)?;
            stmt.query_map(params![query, limit as i64, ns.as_str()], map_row)?
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_tables_and_triggers() {
        let dir = tempfile::tempdir_in("target").unwrap();
        let store = create_benchmark_store(&dir.path().join("mnexis.db")).unwrap();

        let count: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memories'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let fts_count: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);

        // Verify the three expected triggers exist
        let expected_triggers = [
            "memories_after_insert",
            "memories_after_update_content",
            "memories_after_update_state_deleted",
            "memories_after_delete",
        ];
        for name in &expected_triggers {
            let exists: i64 = store
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                    [name],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "trigger {} should exist", name);
        }
    }

    #[test]
    fn insert_and_search_round_trip() {
        let dir = tempfile::tempdir_in("target").unwrap();
        let store = create_benchmark_store(&dir.path().join("mnexis.db")).unwrap();

        let mem = Memory {
            id: None,
            uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
            namespace: "test-ns".to_owned(),
            topic_key: "test-topic".to_owned(),
            memory_type: "fact".to_owned(),
            content: "Rust is a fast systems language".to_owned(),
            state: "active".to_owned(),
            revision: 1,
            deleted: 0,
            created_at: "2025-01-01T00:00:00Z".to_owned(),
            valid_from: None,
            valid_until: None,
            assertion_basis: None,
        };
        let row_id = insert_memory(&store.conn, &mem).unwrap();
        assert_eq!(row_id, 1i64);

        let hits = search(&store.conn, "Rust fast", NamespaceFilter::Any, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, row_id);
    }

    #[test]
    fn insert_memories_loads_a_batch_transactionally() {
        let dir = tempfile::tempdir_in("target").unwrap();
        let store = create_benchmark_store(&dir.path().join("batch.db")).unwrap();
        let corpus = crate::corpus::generate_corpus(50);
        insert_memories(&store.conn, &corpus).unwrap();

        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 50);

        let query = &crate::corpus::relevant_queries()[0];
        let hits = search(&store.conn, &query.text, NamespaceFilter::Any, 5).unwrap();
        assert!(
            hits.iter().any(|h| h.id == query.relevant[0]),
            "the first relevant query must find its active target after a batch load"
        );
    }
}
