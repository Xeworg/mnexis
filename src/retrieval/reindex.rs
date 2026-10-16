//! FTS5 maintenance: rebuild `memories_fts` from the canonical `memories`
//! table. Used by `MemoryService::reindex` (admin command) and by
//! `doctor --repair`.
//!
//! See `odd/tasks/e2-core.md` §9 ("Rebuild from canonical data") for the
//! rebuild procedure: drop `memories_fts`, recreate it, then bulk-insert
//! every `state = 'active' AND deleted = 0` row. The FTS5 triggers defined
//! by migrations 001 and 002 reference the table by name and are preserved
//! across the DROP+CREATE — `doctor --repair` is therefore the canonical
//! way to heal a corrupted index.

use rusqlite::{Connection, params};

use crate::domain::error::MnexisError;

/// Drop `memories_fts`, recreate it, then repopulate from every
/// `state = 'active' AND deleted = 0` row. Returns the number of rows
/// re-indexed.
///
/// Idempotent: running on a healthy index produces the same final state.
/// Used by `MemoryService::reindex` and by `doctor --repair`.
pub fn reindex_all(conn: &Connection) -> Result<usize, MnexisError> {
    conn.execute("DROP TABLE IF EXISTS memories_fts", [])
        .map_err(|e| MnexisError::busy(format!("reindex drop memories_fts: {e}")))?;
    conn.execute("CREATE VIRTUAL TABLE memories_fts USING fts5(content)", [])
        .map_err(|e| MnexisError::busy(format!("reindex create memories_fts: {e}")))?;

    // Bulk re-insert every canonical active + non-deleted row. The
    // application service does NOT issue FTS DML on the write path; only
    // the triggers do. Here we bypass the triggers and write directly
    // because this is the rebuild path (after a DROP the triggers
    // already mirror what we want).
    let mut stmt = conn
        .prepare("SELECT id, content FROM memories WHERE state = 'active' AND deleted = 0")
        .map_err(|e| MnexisError::busy(format!("reindex prepare: {e}")))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| MnexisError::busy(format!("reindex query: {e}")))?;

    let mut count: usize = 0;
    while let Some(row) = rows
        .next()
        .map_err(|e| MnexisError::busy(format!("reindex next: {e}")))?
    {
        let id: i64 = row
            .get(0)
            .map_err(|e| MnexisError::busy(format!("reindex id: {e}")))?;
        let content: String = row
            .get(1)
            .map_err(|e| MnexisError::busy(format!("reindex content: {e}")))?;
        conn.execute(
            "INSERT INTO memories_fts(rowid, content) VALUES (?1, ?2)",
            params![id, content],
        )
        .map_err(|e| MnexisError::busy(format!("reindex insert id={id}: {e}")))?;
        count += 1;
    }
    Ok(count)
}
