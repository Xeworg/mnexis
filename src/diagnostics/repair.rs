//! `doctor --repair` implementation.
//!
//! Inside one transaction, this module:
//! 1. Drops and recreates `memories_fts` with the exact DDL from
//!    `storage::sqlite::MIGRATION_001`.
//! 2. Recreates all five FTS5 maintenance triggers:
//!    - `memories_after_insert_content`
//!    - `memories_after_update_content`
//!    - `memories_after_update_into_active`
//!    - `memories_after_update_out_of_active`
//!    - `memories_after_delete`
//! 3. Calls `crate::retrieval::reindex::reindex_all` to rebuild the index
//!    from every `state = 'active' AND deleted = 0` row.
//!
//! Canonical tables are untouched. The repair is idempotent: running it on a
//! healthy database produces the same final state.

use rusqlite::Connection;

use crate::domain::error::MnexisError;
use crate::retrieval::reindex::reindex_all;

/// Recreate all five FTS5 maintenance triggers with their exact DDL from
/// the production schema (e2-core §2, as emitted by migrations 001 and 002).
/// Using `CREATE OR REPLACE TRIGGER` so any pre-existing (possibly buggy)
/// trigger body is always overwritten.
/// Recreate all five FTS5 maintenance triggers with the exact DDL from
/// the production schema (e2-core §2, as emitted by migrations 001 and 002).
/// Drops any pre-existing trigger first so a buggy body is always overwritten.
fn repair_triggers(conn: &Connection) -> Result<(), MnexisError> {
    // Migration 001 — trigger 1: AFTER INSERT.
    conn.execute("DROP TRIGGER IF EXISTS memories_after_insert_content", [])
        .map_err(|e| MnexisError::busy(format!("repair DROP insert trigger: {e}")))?;
    conn.execute(
        "CREATE TRIGGER memories_after_insert_content \
         AFTER INSERT ON memories \
         WHEN new.state = 'active' AND new.deleted = 0 \
         BEGIN \
             INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content); \
         END",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair CREATE insert trigger: {e}")))?;

    // Migration 001 — trigger 2: AFTER UPDATE state/deleted (into active).
    conn.execute(
        "DROP TRIGGER IF EXISTS memories_after_update_into_active",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair DROP into-active trigger: {e}")))?;
    conn.execute(
        "CREATE TRIGGER memories_after_update_into_active \
         AFTER UPDATE OF state, deleted ON memories \
         WHEN new.state = 'active' AND new.deleted = 0 \
         BEGIN \
             DELETE FROM memories_fts WHERE rowid = new.id; \
             INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content); \
         END",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair CREATE into-active trigger: {e}")))?;

    // Migration 001 — trigger 3: AFTER UPDATE state/deleted (out of active).
    conn.execute(
        "DROP TRIGGER IF EXISTS memories_after_update_out_of_active",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair DROP out-of-active trigger: {e}")))?;
    conn.execute(
        "CREATE TRIGGER memories_after_update_out_of_active \
         AFTER UPDATE OF state, deleted ON memories \
         WHEN NOT (new.state = 'active' AND new.deleted = 0) \
         BEGIN \
             DELETE FROM memories_fts WHERE rowid = new.id; \
         END",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair CREATE out-of-active trigger: {e}")))?;

    // Migration 002 — trigger 4: AFTER UPDATE OF content.
    conn.execute("DROP TRIGGER IF EXISTS memories_after_update_content", [])
        .map_err(|e| MnexisError::busy(format!("repair DROP update-content trigger: {e}")))?;
    conn.execute(
        "CREATE TRIGGER memories_after_update_content \
         AFTER UPDATE OF content ON memories \
         WHEN new.deleted = 0 \
         BEGIN \
             UPDATE memories_fts SET content = new.content WHERE rowid = new.id; \
         END",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair CREATE update-content trigger: {e}")))?;

    // Migration 002 — trigger 5: AFTER DELETE.
    conn.execute("DROP TRIGGER IF EXISTS memories_after_delete", [])
        .map_err(|e| MnexisError::busy(format!("repair DROP delete trigger: {e}")))?;
    conn.execute(
        "CREATE TRIGGER memories_after_delete \
         AFTER DELETE ON memories \
         BEGIN \
             DELETE FROM memories_fts WHERE rowid = old.id; \
         END",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("repair CREATE delete trigger: {e}")))?;

    Ok(())
}

/// Execute the full repair transaction: recreate FTS table, recreate
/// triggers, then bulk-reindex all active+non-deleted rows.
///
/// All three steps run inside a single SQLite transaction so that a failure
/// at any step aborts the entire repair without leaving partial state.
///
/// Returns the number of rows re-indexed (active + non-deleted count).
pub fn repair_db(conn: &Connection) -> Result<usize, MnexisError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| MnexisError::busy(format!("repair begin transaction: {e}")))?;

    // reindex_all drops and recreates the FTS table (standard FTS5 DDL).
    // We then recreate the triggers on the freshly-built table.
    let reindexed = reindex_all(&tx)?;
    repair_triggers(&tx)?;

    tx.commit()
        .map_err(|e| MnexisError::busy(format!("repair commit: {e}")))?;

    Ok(reindexed)
}
