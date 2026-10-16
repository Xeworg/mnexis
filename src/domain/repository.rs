//! Storage port: the domain's interface to memory persistence.
//!
//! The `storage/` adapter (T16) implements this trait over SQLite. The
//! domain layer never touches `rusqlite` or the database directly.

use crate::domain::error::MnexisError;
use crate::domain::memory::Memory;

pub trait MemoryRepository {
    /// Insert a new memory. Returns the assigned rowid.
    /// Returns `MnexisError::TopicConflict` if `topic_key` + `namespace`
    /// already exists.
    fn save(&mut self, mem: &Memory) -> Result<i64, MnexisError>;

    /// Get a memory by its UUID. Returns `Ok(None)` if not found.
    fn get_by_uuid(&self, uuid: &str) -> Result<Option<Memory>, MnexisError>;

    /// Get a memory by its database rowid.
    fn get_by_rowid(&self, id: i64) -> Result<Option<Memory>, MnexisError>;

    /// List memories with optional namespace/state filter and pagination.
    /// `limit` is the maximum number of rows; `offset` skips the first N rows.
    fn list(
        &self,
        namespace: Option<&str>,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError>;

    /// Update a memory. The caller passes `expected_revision`; the storage
    /// layer checks it atomically and returns `MnexisError::RevisionConflict`
    /// if it doesn't match. The returned `Memory` has the new revision number.
    fn update(
        &mut self,
        mem: &Memory,
        expected_revision: i32,
    ) -> Result<Memory, MnexisError>;

    /// Soft-delete a memory by UUID.
    fn soft_delete(
        &mut self,
        uuid: &str,
        expected_revision: i32,
    ) -> Result<(), MnexisError>;

    /// Restore a soft-deleted memory.
    fn restore(&mut self, uuid: &str) -> Result<(), MnexisError>;

    /// Purge memories. `before` is an optional ISO-8601 cutoff; `all` purges
    /// all soft-deleted regardless of date. Returns the number of records purged.
    fn purge(&mut self, before: Option<&str>, all: bool) -> Result<usize, MnexisError>;

    /// Count memories matching the optional namespace/state filter.
    fn count(
        &self,
        namespace: Option<&str>,
        state: Option<&str>,
    ) -> Result<i64, MnexisError>;
}
