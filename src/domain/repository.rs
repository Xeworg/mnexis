//! Storage port: the domain's interface to memory persistence.
//!
//! The `storage/` adapter (T16) implements this trait over SQLite. The
//! domain layer never touches `rusqlite` or the database directly.

use crate::domain::error::MnexisError;
use crate::domain::idempotency::IdempotencyRecord;
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink, MemoryTag};
// SearchHit is defined in the spike's retrieval module for T17; T18 will
// define the production retrieval type in src/retrieval/fts5.rs.
use crate::retrieval::SearchHit;

pub trait MemoryRepository {
    /// Insert a new memory. Returns the assigned rowid.
    /// Returns `MnexisError::TopicConflict` if `topic_key` + `namespace`
    /// already exists.
    fn save(&mut self, mem: &Memory) -> Result<i64, MnexisError>;

    /// Get a memory by its UUID. Returns `Ok(None)` if not found.
    fn get_by_uuid(&self, uuid: &str) -> Result<Option<Memory>, MnexisError>;

    /// Get a memory by its UUID including soft-deleted records. Used by
    /// the service's `soft_delete` and `restore` to authorize the caller
    /// against the memory's namespace before issuing the transition.
    fn get_by_uuid_including_deleted(&self, uuid: &str) -> Result<Option<Memory>, MnexisError>;

    /// Get a memory by its database rowid.
    fn get_by_rowid(&self, id: i64) -> Result<Option<Memory>, MnexisError>;

    /// List memories with optional namespace/state filter and pagination.
    fn list(
        &self,
        namespace: Option<&str>,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError>;

    /// List soft-deleted memories with the same namespace pagination as
    /// `list`. `list` intentionally hides deleted records; the TUI's
    /// deleted view and its restore flow need to browse them.
    fn list_deleted(
        &self,
        namespace: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError>;

    /// Update a memory with optimistic revision check.
    fn update(&mut self, mem: &Memory, expected_revision: i32) -> Result<Memory, MnexisError>;

    /// Soft-delete a memory by UUID.
    fn soft_delete(&mut self, uuid: &str, expected_revision: i32) -> Result<(), MnexisError>;

    /// Restore a soft-deleted memory.
    fn restore(&mut self, uuid: &str) -> Result<(), MnexisError>;

    /// Purge memories. `before` is an optional ISO-8601 cutoff; `all` purges
    /// all soft-deleted regardless of date.
    fn purge(&mut self, before: Option<&str>, all: bool) -> Result<usize, MnexisError>;

    /// Rebuild the FTS5 index from the canonical `memories` table. Returns
    /// the number of rows re-indexed. Used by `MemoryService::reindex`
    /// (admin command) and by `doctor --repair`.
    fn reindex(&mut self) -> Result<usize, MnexisError>;

    /// Count memories matching the optional namespace/state filter.
    fn count(&self, namespace: Option<&str>, state: Option<&str>) -> Result<i64, MnexisError>;

    /// Get the revision history for a memory, newest first.
    fn history(&self, uuid: &str) -> Result<Vec<MemoryHistory>, MnexisError>;

    /// Append a history record for a memory revision.
    fn save_history(&mut self, h: &MemoryHistory) -> Result<i64, MnexisError>;

    /// Persist a memory link (MVP: supersedes only).
    fn save_link(&mut self, l: &MemoryLink) -> Result<i64, MnexisError>;

    /// Get every link row that touches `uuid`, either as the older or the
    /// newer side of a supersession. Ordered by insertion (`id`).
    fn links_for(&self, uuid: &str) -> Result<Vec<MemoryLink>, MnexisError>;

    /// Atomically execute a supersede (contract v1: "Supersede is atomic,
    /// same-namespace in MVP, revision-checked on both records, and must
    /// reject cycles"). In one transaction: reject when the link would
    /// close a cycle in the supersession graph (the trivial self-link
    /// included), verify the newer row still holds
    /// `expected_new_revision` (`REVISION_CONFLICT` with the row's current
    /// revision otherwise), flip the older row to `superseded` at
    /// `expected_old_revision` (bumping its revision), and persist the
    /// link with its `reason`. Nothing commits unless every step passes;
    /// namespace, scope, and state gates stay in the service.
    fn supersede_atomic(
        &mut self,
        older_uuid: &str,
        newer_uuid: &str,
        expected_old_revision: i32,
        expected_new_revision: i32,
        reason: &str,
        now: &str,
    ) -> Result<(), MnexisError>;

    /// Get the tags attached to a memory.
    fn tags_for(&self, uuid: &str) -> Result<Vec<MemoryTag>, MnexisError>;

    /// Replace the tag set for a memory.
    fn set_tags(&mut self, uuid: &str, tags: &[String]) -> Result<(), MnexisError>;

    /// Look up an idempotency record by (key, namespace).
    fn get_idempotency(
        &self,
        key: &str,
        namespace: &str,
    ) -> Result<Option<IdempotencyRecord>, MnexisError>;

    /// Persist an idempotency record.
    fn save_idempotency(&mut self, r: &IdempotencyRecord) -> Result<(), MnexisError>;

    /// Find a memory that occupies the given `(namespace, topic_key)` pair
    /// (any state, any deletion status). Returns the uuid of the occupier,
    /// or None if the topic_key is free.
    fn topic_key_occupier(
        &self,
        namespace: &str,
        topic_key: &str,
    ) -> Result<Option<String>, MnexisError>;

    /// Snapshot all memories in the given namespaces, optionally including
    /// soft-deleted ones. Used by `MemoryService::export`; the result is
    /// already filtered to the caller's authorised namespaces and the
    /// `deleted` policy. Returns a consistent row-by-row vec.
    fn list_for_export(
        &self,
        namespaces: &[String],
        include_deleted: bool,
    ) -> Result<Vec<Memory>, MnexisError>;

    /// Apply an update + history + tag rewrite as a single transaction
    /// (T23 close test `partial_failure_no_partial_writes`). Returns the
    /// updated `Memory`. A failure inside the transaction rolls back all
    /// three writes.
    fn update_with_history_and_tags(
        &mut self,
        updated: &Memory,
        expected_revision: i32,
        history: &MemoryHistory,
        tags: Option<&[String]>,
    ) -> Result<Memory, MnexisError>;

    /// Full-text search over active, non-deleted memories. Delegates to
    /// the retrieval module (T18 will formalize the production search).
    fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, MnexisError>;
}
