//! FTS5 maintenance: rebuild `memories_fts` from the canonical `memories`
//! table. Used by `MemoryService::reindex` (admin command) and by
//! `doctor --repair`.
//!
//! There is exactly one definition of the index — the table and its five
//! triggers — and it lives in `storage::sqlite`. The migration, this reindex
//! and `doctor --repair` all go through
//! [`crate::storage::sqlite::rebuild_memories_fts`], so a rebuilt index and a
//! migrated index are the same index by construction: four columns
//! (`content`, `title`, `topic_key`, `keywords`) and one row per
//! `state = 'active' AND deleted = 0` memory.
//!
//! Unlike the previous drop-and-recreate pass, the rebuild is atomic: it runs
//! in a savepoint, so a failure leaves the old index in place, and it joins a
//! caller's transaction instead of nesting a second one.

use rusqlite::Connection;

use crate::domain::error::MnexisError;

/// Drop `memories_fts`, recreate it and its triggers from the canonical
/// schema, then repopulate from every `state = 'active' AND deleted = 0` row.
/// Returns the number of rows re-indexed.
///
/// Idempotent: running on a healthy index produces the same final state.
/// Used by `MemoryService::reindex` and by `doctor --repair`.
pub fn reindex_all(conn: &Connection) -> Result<usize, MnexisError> {
    crate::storage::sqlite::rebuild_memories_fts(conn)
}
