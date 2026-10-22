//! SQLite adapter for the `MemoryRepository` port from `domain::repository`.
//!
//! Maps between the `domain::Memory` value type and the production
//! `memories` table.

use rusqlite::{Connection, OptionalExtension, Row, ToSql, params};

use crate::domain::error::MnexisError;
use crate::domain::idempotency::IdempotencyRecord;
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink, MemoryState, MemoryTag};
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::NamespaceScope;
// SearchHit is defined in the spike's retrieval module for T17; T18 will
// define the production retrieval type in src/retrieval/fts5.rs.
use crate::retrieval::SearchHit;

/// SQLite-backed implementation of `MemoryRepository`.
pub struct SqliteMemoryRepository {
    conn: Connection,
}

impl SqliteMemoryRepository {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn row_to_memory(row: &Row) -> Result<Memory, MnexisError> {
        let id: Option<i64> = row.get(0).map_err(sql_to_domain)?;
        let uuid: String = row.get(1).map_err(sql_to_domain)?;
        let topic_key: String = row.get(2).map_err(sql_to_domain)?;
        let namespace: String = row.get(3).map_err(sql_to_domain)?;
        let memory_type: String = row.get(4).map_err(sql_to_domain)?;
        let state_str: String = row.get(5).map_err(sql_to_domain)?;
        let content: String = row.get(6).map_err(sql_to_domain)?;
        let state = MemoryState::from_db(&state_str)?;
        let revision: i32 = row.get(7).map_err(sql_to_domain)?;
        let deleted: i32 = row.get(8).map_err(sql_to_domain)?;
        let created_at: String = row.get(9).map_err(sql_to_domain)?;
        let updated_at: String = row.get(10).map_err(sql_to_domain)?;
        let deleted_at: Option<String> = row.get(11).map_err(sql_to_domain)?;
        let purge_after: Option<String> = row.get(12).map_err(sql_to_domain)?;
        let valid_from: Option<String> = row.get(13).map_err(sql_to_domain)?;
        let valid_until: Option<String> = row.get(14).map_err(sql_to_domain)?;
        let actor: String = row.get(15).map_err(sql_to_domain)?;
        let channel: String = row.get(16).map_err(sql_to_domain)?;
        let assertion_basis: String = row.get(17).map_err(sql_to_domain)?;
        Ok(Memory {
            id,
            uuid,
            topic_key,
            namespace,
            memory_type,
            content,
            state,
            revision,
            deleted: deleted != 0,
            created_at,
            updated_at,
            deleted_at,
            purge_after,
            valid_from,
            valid_until,
            actor,
            channel,
            assertion_basis,
        })
    }

    /// Shared implementation of [`MemoryRepository::list_in`],
    /// [`MemoryRepository::list_deleted`] and
    /// [`MemoryRepository::list_recent`]; `deleted` selects which side of the
    /// soft-delete flag the query returns and `newest_first` selects the row
    /// order (`id DESC` instead of `id`).
    fn list_filtered(
        &self,
        namespaces: &NamespaceScope,
        state: Option<&str>,
        limit: usize,
        offset: usize,
        deleted: bool,
        newest_first: bool,
    ) -> Result<Vec<Memory>, MnexisError> {
        let state = state.map(String::from);
        let Some((filter, mut param_refs)) = filter_sql(namespaces, state.as_ref(), deleted) else {
            return Ok(Vec::new());
        };
        let mut sql = format!("SELECT {SELECT_COLUMNS}{filter}");
        sql.push_str(if newest_first {
            " ORDER BY id DESC LIMIT ? OFFSET ?"
        } else {
            " ORDER BY id LIMIT ? OFFSET ?"
        });
        let limit_val: i64 = limit as i64;
        let offset_val: i64 = offset as i64;
        param_refs.push(&limit_val);
        param_refs.push(&offset_val);

        let mut stmt = self.conn.prepare(&sql).map_err(sql_to_domain)?;
        let rows = stmt
            .query_map(&param_refs[..], |row| {
                Self::row_to_memory(row).map_err(to_sql_err)
            })
            .map_err(sql_to_domain)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(sql_to_domain)?);
        }
        Ok(result)
    }
}

/// The `WHERE` clause every list and count query shares, with its
/// parameters in placeholder order.
///
/// `None` means the scope is an empty `Only` list: no row satisfies that
/// filter, so the caller can answer without touching the database.
/// `deleted` selects which side of the soft-delete flag the query reads.
fn filter_sql<'a>(
    namespaces: &'a NamespaceScope,
    state: Option<&'a String>,
    deleted: bool,
) -> Option<(String, Vec<&'a dyn ToSql>)> {
    let mut sql = format!(
        " FROM memories WHERE deleted = {}",
        if deleted { 1 } else { 0 }
    );
    let mut param_refs: Vec<&'a dyn ToSql> = Vec::new();
    match namespaces {
        NamespaceScope::All => {}
        NamespaceScope::AllExcept(list) => {
            if !list.is_empty() {
                sql.push_str(&format!(
                    " AND namespace NOT IN ({})",
                    placeholders(list.len())
                ));
                for ns in list {
                    param_refs.push(ns as &dyn ToSql);
                }
            }
        }
        NamespaceScope::Only(list) => {
            if list.is_empty() {
                return None;
            }
            sql.push_str(&format!(" AND namespace IN ({})", placeholders(list.len())));
            for ns in list {
                param_refs.push(ns as &dyn ToSql);
            }
        }
    }
    if let Some(st) = state {
        sql.push_str(" AND state = ?");
        param_refs.push(st as &dyn ToSql);
    }
    Some((sql, param_refs))
}

/// `?,?,?` for an `IN` list of `n` namespaces.
fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

fn sql_to_domain(e: rusqlite::Error) -> MnexisError {
    MnexisError::busy(format!("sql: {e}"))
}

/// Bridge `MnexisError` to `rusqlite::Error` for use inside `query_row` /
/// `query_map` closures that must return `Result<_, rusqlite::Error>`.
fn to_sql_err(e: MnexisError) -> rusqlite::Error {
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(e);
    rusqlite::Error::ToSqlConversionFailure(boxed)
}

const SELECT_COLUMNS: &str = "id, uuid, topic_key, namespace, memory_type, state, content, \
                                revision, deleted, created_at, updated_at, deleted_at, purge_after, \
                                valid_from, valid_until, actor, channel, assertion_basis";

impl MemoryRepository for SqliteMemoryRepository {
    fn save(&mut self, mem: &Memory) -> Result<i64, MnexisError> {
        let state_str = mem.state.as_str();
        let result = self.conn.execute(
            "INSERT INTO memories (
                uuid, topic_key, revision, namespace, memory_type, state, content,
                actor, channel, assertion_basis,
                valid_from, valid_until, deleted, deleted_at, purge_after,
                created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15,
                ?16, ?17
            )",
            params![
                mem.uuid,
                mem.topic_key,
                mem.revision,
                mem.namespace,
                mem.memory_type,
                state_str,
                mem.content,
                mem.actor,
                mem.channel,
                mem.assertion_basis,
                mem.valid_from,
                mem.valid_until,
                mem.deleted as i32,
                mem.deleted_at,
                mem.purge_after,
                mem.created_at,
                mem.updated_at,
            ],
        );
        match result {
            Ok(_) => {
                let row_id: i64 = self
                    .conn
                    .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
                    .map_err(sql_to_domain)?;
                Ok(row_id)
            }
            Err(rusqlite::Error::SqliteFailure(ref code, _))
                if code.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(MnexisError::topic_conflict(
                    mem.topic_key.clone(),
                    mem.namespace.clone(),
                ))
            }
            Err(e) => Err(sql_to_domain(e)),
        }
    }

    fn get_by_uuid(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        self.conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM memories WHERE uuid = ?1 AND deleted = 0"),
                [uuid],
                |row| Self::row_to_memory(row).map_err(to_sql_err),
            )
            .optional()
            .map_err(sql_to_domain)
    }

    fn get_by_uuid_including_deleted(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        self.conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM memories WHERE uuid = ?1"),
                [uuid],
                |row| Self::row_to_memory(row).map_err(to_sql_err),
            )
            .optional()
            .map_err(sql_to_domain)
    }

    fn get_by_rowid(&self, id: i64) -> Result<Option<Memory>, MnexisError> {
        self.conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM memories WHERE id = ?1"),
                [id],
                |row| Self::row_to_memory(row).map_err(to_sql_err),
            )
            .optional()
            .map_err(sql_to_domain)
    }

    fn list(
        &self,
        namespace: Option<&str>,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        let scope = match namespace {
            Some(ns) => NamespaceScope::Only(vec![ns.to_string()]),
            None => NamespaceScope::All,
        };
        self.list_filtered(&scope, state, limit, offset, false, false)
    }

    fn list_deleted(
        &self,
        namespace: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        let scope = match namespace {
            Some(ns) => NamespaceScope::Only(vec![ns.to_string()]),
            None => NamespaceScope::All,
        };
        self.list_filtered(&scope, None, limit, offset, true, false)
    }

    fn list_in(
        &self,
        namespaces: &NamespaceScope,
        state: Option<&str>,
        limit: usize,
        offset: usize,
        deleted: bool,
    ) -> Result<Vec<Memory>, MnexisError> {
        self.list_filtered(namespaces, state, limit, offset, deleted, false)
    }

    fn list_recent(
        &self,
        namespaces: &NamespaceScope,
        limit: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        self.list_filtered(namespaces, None, limit, 0, false, true)
    }

    fn count_in(&self, namespaces: &NamespaceScope) -> Result<i64, MnexisError> {
        let Some((filter, param_refs)) = filter_sql(namespaces, None, false) else {
            return Ok(0);
        };
        let sql = format!("SELECT count(*){filter}");
        self.conn
            .query_row(&sql, &param_refs[..], |row| row.get(0))
            .map_err(sql_to_domain)
    }

    fn update(&mut self, mem: &Memory, expected_revision: i32) -> Result<Memory, MnexisError> {
        let id = mem
            .id
            .ok_or_else(|| MnexisError::invalid_argument("id", "memory has no rowid"))?;
        let state_str = mem.state.as_str();
        let new_revision = expected_revision + 1;
        let affected = self
            .conn
            .execute(
                "UPDATE memories SET
                    content = ?1, state = ?2, topic_key = ?3, memory_type = ?4,
                    actor = ?5, channel = ?6, assertion_basis = ?7,
                    valid_from = ?8, valid_until = ?9,
                    revision = ?10, updated_at = ?11
                 WHERE id = ?12 AND revision = ?13",
                params![
                    mem.content,
                    state_str,
                    mem.topic_key,
                    mem.memory_type,
                    mem.actor,
                    mem.channel,
                    mem.assertion_basis,
                    mem.valid_from,
                    mem.valid_until,
                    new_revision,
                    mem.updated_at,
                    id,
                    expected_revision,
                ],
            )
            .map_err(sql_to_domain)?;
        if affected == 0 {
            let current: Option<i64> = self
                .conn
                .query_row("SELECT revision FROM memories WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(sql_to_domain)?;
            return Err(MnexisError::revision_conflict(current.unwrap_or(0)));
        }
        self.get_by_rowid(id)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{id}")))
    }

    fn soft_delete(&mut self, uuid: &str, expected_revision: i32) -> Result<(), MnexisError> {
        let affected = self
            .conn
            .execute(
                "UPDATE memories SET deleted = 1, \
                    deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
                    revision = revision + 1 \
                 WHERE uuid = ?1 AND deleted = 0 AND revision = ?2",
                params![uuid, expected_revision],
            )
            .map_err(sql_to_domain)?;
        if affected == 0 {
            return Err(MnexisError::not_found(format!("memory:{uuid}")));
        }
        Ok(())
    }

    fn restore(&mut self, uuid: &str) -> Result<(), MnexisError> {
        let affected = self
            .conn
            .execute(
                "UPDATE memories SET deleted = 0, deleted_at = NULL, \
                    revision = revision + 1 \
                 WHERE uuid = ?1 AND deleted = 1",
                [uuid],
            )
            .map_err(sql_to_domain)?;
        if affected == 0 {
            return Err(MnexisError::not_found(format!(
                "memory:{uuid} (not deleted or missing)"
            )));
        }
        Ok(())
    }

    fn purge(&mut self, before: Option<&str>, all: bool) -> Result<usize, MnexisError> {
        if all {
            let n = self
                .conn
                .execute("DELETE FROM memories WHERE deleted = 1", [])
                .map_err(sql_to_domain)?;
            Ok(n)
        } else if let Some(cutoff) = before {
            let n = self
                .conn
                .execute(
                    "DELETE FROM memories WHERE deleted = 1 \
                     AND purge_after IS NOT NULL AND purge_after <= ?1",
                    [cutoff],
                )
                .map_err(sql_to_domain)?;
            Ok(n)
        } else {
            Err(MnexisError::invalid_argument(
                "before",
                "either 'before' (ISO-8601 cutoff) or 'all' must be specified",
            ))
        }
    }

    fn reindex(&mut self) -> Result<usize, MnexisError> {
        crate::retrieval::reindex::reindex_all(&self.conn)
    }

    fn count(&self, namespace: Option<&str>, state: Option<&str>) -> Result<i64, MnexisError> {
        let namespace = namespace.map(String::from);
        let state = state.map(String::from);
        let mut sql = String::from("SELECT count(*) FROM memories WHERE deleted = 0");
        let mut param_refs: Vec<&dyn ToSql> = Vec::new();
        if let Some(ns) = namespace.as_ref() {
            sql.push_str(" AND namespace = ?");
            param_refs.push(ns as &dyn ToSql);
        }
        if let Some(st) = state.as_ref() {
            sql.push_str(" AND state = ?");
            param_refs.push(st as &dyn ToSql);
        }
        let count: i64 = self
            .conn
            .query_row(&sql, &param_refs[..], |r| r.get(0))
            .map_err(sql_to_domain)?;
        Ok(count)
    }

    fn history(&self, uuid: &str) -> Result<Vec<MemoryHistory>, MnexisError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, memory_uuid, revision, memory_type, content, state, \
                        valid_from, valid_until, assertion_basis, changed_by, change_reason, changed_at \
                 FROM memory_history WHERE memory_uuid = ?1 ORDER BY revision DESC",
            )
            .map_err(sql_to_domain)?;
        let rows = stmt
            .query_map([uuid], |row| {
                let state_str: String = row.get(5)?;
                let state = MemoryState::from_db(&state_str).map_err(to_sql_err)?;
                Ok(MemoryHistory {
                    id: row.get(0)?,
                    memory_uuid: row.get(1)?,
                    revision: row.get(2)?,
                    memory_type: row.get(3)?,
                    content: row.get(4)?,
                    state,
                    valid_from: row.get(6)?,
                    valid_until: row.get(7)?,
                    assertion_basis: row.get(8)?,
                    changed_by: row.get(9)?,
                    change_reason: row.get(10)?,
                    changed_at: row.get(11)?,
                })
            })
            .map_err(sql_to_domain)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_to_domain)
    }

    fn save_history(&mut self, h: &MemoryHistory) -> Result<i64, MnexisError> {
        self.conn
            .execute(
                "INSERT INTO memory_history (memory_uuid, revision, memory_type, content, state, \
                        valid_from, valid_until, assertion_basis, changed_by, change_reason, changed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    h.memory_uuid,
                    h.revision,
                    h.memory_type,
                    h.content,
                    h.state.as_str(),
                    h.valid_from,
                    h.valid_until,
                    h.assertion_basis,
                    h.changed_by,
                    h.change_reason,
                    h.changed_at,
                ],
            )
            .map_err(sql_to_domain)?;
        self.conn
            .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
            .map_err(sql_to_domain)
    }

    fn save_link(&mut self, l: &MemoryLink) -> Result<i64, MnexisError> {
        self.conn
            .execute(
                "INSERT INTO memory_links (older_uuid, newer_uuid, link_type, reason, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    l.older_uuid,
                    l.newer_uuid,
                    l.link_type,
                    l.reason,
                    l.created_at
                ],
            )
            .map_err(sql_to_domain)?;
        self.conn
            .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
            .map_err(sql_to_domain)
    }

    fn links_for(&self, uuid: &str) -> Result<Vec<MemoryLink>, MnexisError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, older_uuid, newer_uuid, link_type, reason, created_at \
                 FROM memory_links WHERE older_uuid = ?1 OR newer_uuid = ?1 ORDER BY id",
            )
            .map_err(sql_to_domain)?;
        let rows = stmt
            .query_map([uuid], |row| {
                Ok(MemoryLink {
                    id: row.get(0)?,
                    older_uuid: row.get(1)?,
                    newer_uuid: row.get(2)?,
                    link_type: row.get(3)?,
                    reason: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(sql_to_domain)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_to_domain)
    }

    fn supersede_atomic(
        &mut self,
        older_uuid: &str,
        newer_uuid: &str,
        expected_old_revision: i32,
        expected_new_revision: i32,
        reason: &str,
        now: &str,
    ) -> Result<(), MnexisError> {
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;

        // Cycle rejection. Edges run older -> newer ("newer supersedes
        // older"). The new link older->newer closes a cycle iff `older` is
        // already reachable from `newer` over existing edges. The walk is
        // seeded with `newer` itself, so a self-link trips the same check.
        // UNION (not UNION ALL) deduplicates on cyclic imported data.
        let closes_cycle: bool = tx
            .query_row(
                "WITH RECURSIVE walk(x) AS ( \
                    VALUES (?1) \
                    UNION \
                    SELECT ml.newer_uuid FROM memory_links ml \
                    JOIN walk ON ml.older_uuid = walk.x \
                 ) SELECT EXISTS(SELECT 1 FROM walk WHERE x = ?2)",
                params![newer_uuid, older_uuid],
                |r| r.get(0),
            )
            .map_err(sql_to_domain)?;
        if closes_cycle {
            return Err(MnexisError::invalid_argument(
                "newer_uuid",
                format!(
                    "supersede would close a cycle: {older_uuid} is already \
                     superseded (transitively) by {newer_uuid}"
                ),
            ));
        }

        // Both-revision check, newer side: the row is not mutated by a
        // supersede, so this is a guard against concurrent writers.
        let newer_revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM memories WHERE uuid = ?1",
                params![newer_uuid],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_to_domain)?;
        match newer_revision {
            Some(revision) if revision == i64::from(expected_new_revision) => {}
            Some(revision) => return Err(MnexisError::revision_conflict(revision)),
            None => {
                return Err(MnexisError::not_found(format!("memory:{newer_uuid}")));
            }
        }

        // Older side: flip to superseded under the optimistic guard. Only
        // state / updated_at / revision change, mirroring what the service
        // used to write through `update` (no history row, by parity).
        let affected = tx
            .execute(
                "UPDATE memories SET state = 'superseded', revision = revision + 1, \
                 updated_at = ?1 WHERE uuid = ?2 AND revision = ?3 AND deleted = 0",
                params![now, older_uuid, expected_old_revision],
            )
            .map_err(sql_to_domain)?;
        if affected == 0 {
            let current: Option<i64> = tx
                .query_row(
                    "SELECT revision FROM memories WHERE uuid = ?1",
                    params![older_uuid],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_to_domain)?;
            return Err(MnexisError::revision_conflict(current.unwrap_or(0)));
        }

        tx.execute(
            "INSERT INTO memory_links (older_uuid, newer_uuid, link_type, reason, created_at) \
             VALUES (?1, ?2, 'supersedes', ?3, ?4)",
            params![older_uuid, newer_uuid, reason, now],
        )
        .map_err(sql_to_domain)?;

        tx.commit().map_err(sql_to_domain)
    }

    fn tags_for(&self, uuid: &str) -> Result<Vec<MemoryTag>, MnexisError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, memory_uuid, tag, created_at \
                 FROM memory_tags WHERE memory_uuid = ?1 ORDER BY tag",
            )
            .map_err(sql_to_domain)?;
        let rows = stmt
            .query_map([uuid], |row| {
                Ok(MemoryTag {
                    id: row.get(0)?,
                    memory_uuid: row.get(1)?,
                    tag: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(sql_to_domain)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_to_domain)
    }

    fn set_tags(&mut self, uuid: &str, tags: &[String]) -> Result<(), MnexisError> {
        self.conn
            .execute("DELETE FROM memory_tags WHERE memory_uuid = ?1", [uuid])
            .map_err(sql_to_domain)?;
        for tag in tags {
            self.conn
                .execute(
                    "INSERT INTO memory_tags (memory_uuid, tag, created_at) \
                     VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                    params![uuid, tag],
                )
                .map_err(sql_to_domain)?;
        }
        Ok(())
    }

    /// Atomic update + history + tag rewrite. Used by `MemoryService::update`
    /// so that a failure at any step rolls back the entire change
    /// (T23 `partial_failure_no_partial_writes`).
    fn update_with_history_and_tags(
        &mut self,
        updated: &Memory,
        expected_revision: i32,
        history: &MemoryHistory,
        tags: Option<&[String]>,
    ) -> Result<Memory, MnexisError> {
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;
        let result: Result<Memory, MnexisError> = (|| {
            let rows = tx
                .execute(
                    "UPDATE memories SET \
                       content = ?1, state = ?2, revision = revision + 1, \
                       updated_at = ?3, assertion_basis = ?4, \
                       topic_key = ?5, memory_type = ?6, \
                       valid_from = ?7, valid_until = ?8, \
                       actor = ?9, channel = ?10 \
                     WHERE uuid = ?11 AND revision = ?12",
                    params![
                        updated.content,
                        updated.state.as_str(),
                        updated.updated_at,
                        updated.assertion_basis,
                        updated.topic_key,
                        updated.memory_type,
                        updated.valid_from,
                        updated.valid_until,
                        updated.actor,
                        updated.channel,
                        updated.uuid,
                        expected_revision,
                    ],
                )
                .map_err(sql_to_domain)?;
            if rows == 0 {
                // Report the revision actually on disk, exactly like the
                // non-transactional `update` above: the caller's expected
                // revision is what failed the check, not the current one.
                let current: Option<i64> = tx
                    .query_row(
                        "SELECT revision FROM memories WHERE uuid = ?1",
                        [updated.uuid.as_str()],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(sql_to_domain)?;
                return Err(MnexisError::revision_conflict(current.unwrap_or(0)));
            }
            tx.execute(
                "INSERT INTO memory_history (memory_uuid, revision, memory_type, content, state, \
                        valid_from, valid_until, assertion_basis, changed_by, change_reason, changed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    history.memory_uuid,
                    history.revision,
                    history.memory_type,
                    history.content,
                    history.state.as_str(),
                    history.valid_from,
                    history.valid_until,
                    history.assertion_basis,
                    history.changed_by,
                    history.change_reason,
                    history.changed_at,
                ],
            )
            .map_err(sql_to_domain)?;
            if let Some(tags) = tags {
                if !tags.is_empty() {
                    tx.execute(
                        "DELETE FROM memory_tags WHERE memory_uuid = ?1",
                        [history.memory_uuid.as_str()],
                    )
                    .map_err(sql_to_domain)?;
                    for tag in tags {
                        tx.execute(
                            "INSERT INTO memory_tags (memory_uuid, tag, created_at) \
                             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                            params![history.memory_uuid, tag],
                        )
                        .map_err(sql_to_domain)?;
                    }
                }
            }
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT {SELECT_COLUMNS} FROM memories WHERE uuid = ?1"
                ))
                .map_err(sql_to_domain)?;
            stmt.query_row([updated.uuid.as_str()], |row| {
                Self::row_to_memory(row).map_err(to_sql_err)
            })
            .map_err(sql_to_domain)
        })();
        match result {
            Ok(memory) => {
                tx.commit().map_err(sql_to_domain)?;
                Ok(memory)
            }
            Err(e) => {
                // Drop tx without commit to roll back.
                Err(e)
            }
        }
    }

    fn get_idempotency(
        &self,
        key: &str,
        namespace: &str,
    ) -> Result<Option<IdempotencyRecord>, MnexisError> {
        self.conn
            .query_row(
                "SELECT id_key, namespace, request_hash, result_uuid, created_at \
                 FROM idempotency_records WHERE id_key = ?1 AND namespace = ?2",
                params![key, namespace],
                |row| {
                    Ok(IdempotencyRecord {
                        key: crate::domain::idempotency::IdempotencyKey(row.get(0)?),
                        namespace: row.get(1)?,
                        request_hash: row.get(2)?,
                        result_uuid: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(sql_to_domain)
    }

    fn save_idempotency(&mut self, r: &IdempotencyRecord) -> Result<(), MnexisError> {
        self.conn
            .execute(
                "INSERT INTO idempotency_records (id_key, namespace, request_hash, result_uuid, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    r.key.as_str(),
                    r.namespace,
                    r.request_hash,
                    r.result_uuid,
                    r.created_at,
                ],
            )
            .map_err(sql_to_domain)?;
        Ok(())
    }

    fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, MnexisError> {
        let scope = match namespace {
            Some(ns) => NamespaceScope::Only(vec![ns.to_string()]),
            None => NamespaceScope::All,
        };
        self.search_in(&scope, query, limit)
    }

    fn search_in(
        &self,
        namespaces: &NamespaceScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, MnexisError> {
        let ns_filter = match namespaces {
            NamespaceScope::All => crate::retrieval::NamespaceFilter::Any,
            NamespaceScope::AllExcept(list) => {
                crate::retrieval::NamespaceFilter::AnyExcept(list.clone())
            }
            NamespaceScope::Only(list) => crate::retrieval::NamespaceFilter::Many(list.clone()),
        };
        crate::retrieval::search(
            &self.conn,
            query,
            ns_filter,
            crate::retrieval::StateFilter::default(),
            crate::retrieval::DeletedFilter::default(),
            None,
            limit,
        )
    }

    /// Snapshot all memories in the given namespaces, optionally including
    /// soft-deleted ones. Used by `MemoryService::export`; the result is
    /// already filtered to the caller's authorised namespaces and the
    /// `deleted` policy. Returns a consistent row-by-row vec.
    fn list_for_export(
        &self,
        namespaces: &[String],
        include_deleted: bool,
    ) -> Result<Vec<Memory>, MnexisError> {
        if namespaces.is_empty() {
            return Ok(Vec::new());
        }
        // Build a single IN (?, ?, …) clause. The places binder is dynamic
        // because we don't know the count at compile time.
        let placeholders: Vec<&str> = (1..=namespaces.len()).map(|_| "?").collect();
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM memories WHERE namespace IN ({}) {} \
             ORDER BY id ASC",
            placeholders.join(","),
            if include_deleted {
                ""
            } else {
                "AND deleted = 0"
            }
        );
        let mut stmt = self.conn.prepare(&sql).map_err(sql_to_domain)?;
        let params: Vec<&dyn rusqlite::ToSql> = namespaces
            .iter()
            .map(|n| n as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Self::row_to_memory(row).map_err(to_sql_err)
            })
            .map_err(sql_to_domain)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql_to_domain)?);
        }
        Ok(out)
    }

    /// Find a memory that occupies the given `(namespace, topic_key)` pair
    /// (any state, any deletion status). Returns the uuid of the occupier,
    /// or None if the topic_key is free.
    fn topic_key_occupier(
        &self,
        namespace: &str,
        topic_key: &str,
    ) -> Result<Option<String>, MnexisError> {
        if topic_key.is_empty() {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT uuid FROM memories WHERE namespace = ?1 AND topic_key = ?2 LIMIT 1")
            .map_err(sql_to_domain)?;
        let mut rows = stmt
            .query(rusqlite::params![namespace, topic_key])
            .map_err(sql_to_domain)?;
        match rows.next().map_err(sql_to_domain)? {
            Some(row) => Ok(Some(row.get(0).map_err(sql_to_domain)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::open_sqlite;
    use tempfile::tempdir_in;

    fn make_repo(n: usize) -> (SqliteMemoryRepository, tempfile::TempDir) {
        let dir = tempdir_in("target").unwrap();
        let conn = open_sqlite(&dir.path().join("repo.db")).unwrap();
        let mut repo = SqliteMemoryRepository::new(conn);
        for i in 0..n {
            let m = Memory::new_proposed(
                format!("uuid-{i}"),
                if i == 0 {
                    "topic.shared".to_string()
                } else {
                    format!("topic.{i}")
                },
                "proyecto-a".to_string(),
                "fact".to_string(),
                format!("content body {i}"),
                "user".to_string(),
                "cli".to_string(),
                "user_stated".to_string(),
                "2026-01-01T00:00:00Z".to_string(),
            );
            repo.save(&m).unwrap();
        }
        (repo, dir)
    }

    #[test]
    fn save_and_get_by_uuid_round_trip() {
        let (repo, _dir) = make_repo(3);
        let m = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        assert_eq!(m.uuid, "uuid-0");
        assert_eq!(m.revision, 1);
        assert_eq!(m.content, "content body 0");
        assert_eq!(m.state, MemoryState::Proposed);
    }

    #[test]
    fn get_by_rowid_returns_inserted_memory() {
        let (repo, _dir) = make_repo(1);
        let m = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        let id = m.id.unwrap();
        let by_id = repo.get_by_rowid(id).unwrap().unwrap();
        assert_eq!(by_id.uuid, "uuid-0");
    }

    #[test]
    fn list_filters_by_namespace_and_state() {
        let (repo, _dir) = make_repo(3);
        let all = repo.list(None, None, 100, 0).unwrap();
        assert_eq!(all.len(), 3);
        let ns_a = repo.list(Some("proyecto-a"), None, 100, 0).unwrap();
        assert_eq!(ns_a.len(), 3);
        let empty = repo.list(Some("proyecto-b"), None, 100, 0).unwrap();
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn update_increments_revision_and_returns_new_memory() {
        let (mut repo, _dir) = make_repo(1);
        let m0 = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        let mut m1 = m0.clone();
        m1.content = "updated body".to_string();
        m1.updated_at = "2026-01-02T00:00:00Z".to_string();
        let m2 = repo.update(&m1, 1).unwrap();
        assert_eq!(m2.revision, 2);
        assert_eq!(m2.content, "updated body");
    }

    #[test]
    fn update_with_wrong_revision_returns_revision_conflict() {
        let (mut repo, _dir) = make_repo(1);
        let m0 = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        let mut m1 = m0.clone();
        m1.content = "updated".to_string();
        let err = repo.update(&m1, 99).unwrap_err();
        assert!(matches!(
            err,
            MnexisError::RevisionConflict {
                current_revision: 1
            }
        ));
    }

    #[test]
    fn update_with_history_reports_the_actual_current_revision() {
        let (mut repo, _dir) = make_repo(1);
        let m0 = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        // Move the row to revision 2 through the non-transactional path.
        let mut m1 = m0.clone();
        m1.content = "rev 2".to_string();
        m1.updated_at = "2026-01-02T00:00:00Z".to_string();
        let updated = repo.update(&m1, 1).unwrap();
        assert_eq!(updated.revision, 2);

        // A stale writer still expecting revision 1 hits the transactional
        // path. The conflict must name the revision actually on disk (2),
        // not the writer's expectation (1).
        let mut stale = updated.clone();
        stale.content = "stale write".to_string();
        let history = MemoryHistory {
            id: None,
            memory_uuid: "uuid-0".to_string(),
            revision: 2,
            memory_type: stale.memory_type.clone(),
            content: stale.content.clone(),
            state: stale.state,
            valid_from: None,
            valid_until: None,
            assertion_basis: stale.assertion_basis.clone(),
            changed_by: "user".to_string(),
            change_reason: "stale".to_string(),
            changed_at: "2026-01-03T00:00:00Z".to_string(),
        };
        let err = repo
            .update_with_history_and_tags(&stale, 1, &history, None)
            .unwrap_err();
        assert_eq!(
            err,
            MnexisError::RevisionConflict {
                current_revision: 2
            },
            "the conflict must carry the row's actual revision"
        );
    }

    #[test]
    fn soft_delete_and_restore_round_trip() {
        let (mut repo, _dir) = make_repo(1);
        repo.soft_delete("uuid-0", 1).unwrap();
        // After soft delete, get_by_uuid returns None (filters deleted=0).
        assert!(repo.get_by_uuid("uuid-0").unwrap().is_none());
        repo.restore("uuid-0").unwrap();
        // After restore, it's back.
        let m = repo.get_by_uuid("uuid-0").unwrap().unwrap();
        assert!(!m.deleted);
        assert_eq!(m.revision, 3); // 1 → soft_delete(2) → restore(3)
    }

    #[test]
    fn purge_removes_soft_deleted_records() {
        let (mut repo, _dir) = make_repo(2);
        repo.soft_delete("uuid-0", 1).unwrap();
        let purged = repo.purge(None, true).unwrap();
        assert_eq!(purged, 1);
        // uuid-0 is gone, uuid-1 remains.
        assert!(repo.get_by_uuid("uuid-0").unwrap().is_none());
        assert!(repo.get_by_uuid("uuid-1").unwrap().is_some());
    }

    #[test]
    fn save_duplicate_topic_key_returns_topic_conflict() {
        let (mut repo, _dir) = make_repo(0);
        let m1 = Memory::new_proposed(
            "uuid-a",
            "topic.dup",
            "proyecto-a",
            "fact",
            "first",
            "user",
            "cli",
            "user_stated",
            "2026-01-01T00:00:00Z",
        );
        repo.save(&m1).unwrap();
        let m2 = Memory::new_proposed(
            "uuid-b",
            "topic.dup",
            "proyecto-a",
            "fact",
            "second",
            "user",
            "cli",
            "user_stated",
            "2026-01-01T00:00:00Z",
        );
        let err = repo.save(&m2).unwrap_err();
        assert!(matches!(err, MnexisError::TopicConflict { .. }));
    }

    #[test]
    fn count_matches_list_length() {
        let (repo, _dir) = make_repo(5);
        let count = repo.count(None, None).unwrap();
        let list = repo.list(None, None, 100, 0).unwrap();
        assert_eq!(count as usize, list.len());
    }

    #[test]
    fn reindex_rebuilds_fts5_and_reports_active_non_deleted_count() {
        let dir = tempdir_in("target").unwrap();
        let conn = open_sqlite(&dir.path().join("reindex.db")).unwrap();
        let mut repo = SqliteMemoryRepository::new(conn);

        // Save two active memories (the trigger inserts them into FTS5
        // on save because state='active' AND deleted=0).
        for i in 0..2 {
            let m = Memory {
                state: MemoryState::Active,
                ..Memory::new_proposed(
                    format!("uuid-{i}"),
                    format!("topic.{i}"),
                    "proyecto-a".to_string(),
                    "fact".to_string(),
                    format!("content body {i}"),
                    "user".to_string(),
                    "cli".to_string(),
                    "user_stated".to_string(),
                    "2026-01-01T00:00:00Z".to_string(),
                )
            };
            repo.save(&m).unwrap();
        }

        let count = repo.reindex().unwrap();
        assert_eq!(
            count, 2,
            "two active + non-deleted memories must be re-indexed"
        );

        // After reindex, search must surface the active content via FTS5.
        let hits = repo.search("content", Some("proyecto-a"), 10).unwrap();
        assert_eq!(
            hits.len(),
            2,
            "search must return both rows after a reindex"
        );
    }
}
