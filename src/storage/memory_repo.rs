//! SQLite adapter for the `MemoryRepository` port from `domain::repository`.
//!
//! Maps between the `domain::Memory` value type and the production
//! `memories` table.

use rusqlite::{Connection, OptionalExtension, Row, ToSql, params};

use crate::domain::error::MnexisError;
use crate::domain::idempotency::IdempotencyRecord;
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink, MemoryState, MemoryTag};
use crate::domain::query::MatchMode;
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::NamespaceScope;
use crate::domain::session::{Session, UserPrompt};
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

    /// The one INSERT that creates a record.
    ///
    /// The single `save` and the batch import share it, so the column list,
    /// the keyword encoding and the refusal a duplicate topic key earns are
    /// decided once: an import writes a record exactly as a save does.
    fn insert_memory(conn: &Connection, mem: &Memory) -> Result<i64, MnexisError> {
        let state_str = mem.state.as_str();
        let keywords = Memory::keywords_column(&mem.keywords);
        let result = conn.execute(
            "INSERT INTO memories (
                uuid, title, topic_key, revision, namespace, memory_type, state, content,
                actor, channel, assertion_basis,
                valid_from, valid_until, deleted, deleted_at, purge_after,
                created_at, updated_at, keywords
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19
            )",
            params![
                mem.uuid,
                mem.title,
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
                keywords,
            ],
        );
        match result {
            Ok(_) => {
                let row_id: i64 = conn
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

    /// The one tag rewrite: replace the record's tags with `tags`.
    ///
    /// The create path, the update transaction and the standalone `set_tags`
    /// share it, so the delete-then-insert and the created-at default are the
    /// same write everywhere.
    fn write_tags(conn: &Connection, uuid: &str, tags: &[String]) -> Result<(), MnexisError> {
        conn.execute("DELETE FROM memory_tags WHERE memory_uuid = ?1", [uuid])
            .map_err(sql_to_domain)?;
        for tag in tags {
            conn.execute(
                "INSERT INTO memory_tags (memory_uuid, tag, created_at) \
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                params![uuid, tag],
            )
            .map_err(sql_to_domain)?;
        }
        Ok(())
    }

    /// The one idempotency INSERT.
    fn write_idempotency(conn: &Connection, record: &IdempotencyRecord) -> Result<(), MnexisError> {
        conn.execute(
            "INSERT INTO idempotency_records (id_key, namespace, request_hash, result_uuid, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.key.as_str(),
                record.namespace,
                record.request_hash,
                record.result_uuid,
                record.created_at,
            ],
        )
        .map_err(sql_to_domain)?;
        Ok(())
    }

    /// Read one record back by uuid, live or including the soft-deleted ones.
    fn select_memory(
        conn: &Connection,
        uuid: &str,
        include_deleted: bool,
    ) -> Result<Option<Memory>, MnexisError> {
        let live_only = if include_deleted {
            ""
        } else {
            " AND deleted = 0"
        };
        conn.query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM memories WHERE uuid = ?1{live_only}"),
            [uuid],
            |row| Self::row_to_memory(row).map_err(to_sql_err),
        )
        .optional()
        .map_err(sql_to_domain)
    }

    fn row_to_memory(row: &Row) -> Result<Memory, MnexisError> {
        let id: Option<i64> = row.get(0).map_err(sql_to_domain)?;
        let uuid: String = row.get(1).map_err(sql_to_domain)?;
        let title: String = row.get(2).map_err(sql_to_domain)?;
        let topic_key: String = row.get(3).map_err(sql_to_domain)?;
        let namespace: String = row.get(4).map_err(sql_to_domain)?;
        let memory_type: String = row.get(5).map_err(sql_to_domain)?;
        let state_str: String = row.get(6).map_err(sql_to_domain)?;
        let content: String = row.get(7).map_err(sql_to_domain)?;
        let state = MemoryState::from_db(&state_str)?;
        let revision: i32 = row.get(8).map_err(sql_to_domain)?;
        let deleted: i32 = row.get(9).map_err(sql_to_domain)?;
        let created_at: String = row.get(10).map_err(sql_to_domain)?;
        let updated_at: String = row.get(11).map_err(sql_to_domain)?;
        let deleted_at: Option<String> = row.get(12).map_err(sql_to_domain)?;
        let purge_after: Option<String> = row.get(13).map_err(sql_to_domain)?;
        let valid_from: Option<String> = row.get(14).map_err(sql_to_domain)?;
        let valid_until: Option<String> = row.get(15).map_err(sql_to_domain)?;
        let actor: String = row.get(16).map_err(sql_to_domain)?;
        let channel: String = row.get(17).map_err(sql_to_domain)?;
        let assertion_basis: String = row.get(18).map_err(sql_to_domain)?;
        let keywords_text: String = row.get(19).map_err(sql_to_domain)?;
        Ok(Memory {
            id,
            uuid,
            title,
            keywords: Memory::keywords_from_column(&keywords_text),
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
    ///
    /// `value` is the value filter: when it is set, the answer is the
    /// records that **carry the value**, and the filter applies before the
    /// limit, so the answer is a page of matches and not a short answer.
    #[allow(clippy::too_many_arguments)]
    fn list_filtered(
        &self,
        namespaces: &NamespaceScope,
        states: &[String],
        limit: usize,
        offset: usize,
        deleted: bool,
        newest_first: bool,
        memory_types: &[String],
        value: Option<&str>,
    ) -> Result<Vec<Memory>, MnexisError> {
        match value {
            None => self.list_page(
                namespaces,
                states,
                limit,
                offset,
                deleted,
                newest_first,
                memory_types,
                None,
                None,
            ),
            Some(value) => self.list_by_value(
                namespaces,
                states,
                limit,
                offset,
                deleted,
                memory_types,
                value,
            ),
        }
    }

    /// One page of rows, with the filters the SQL can decide.
    ///
    /// `like_value` narrows the rows the engine has to read: it is the
    /// **superset** predicate of the value filter (the text holds the value
    /// somewhere, not necessarily as a whole run), so it can only remove
    /// rows that cannot carry the value. The exact rule is decided after
    /// the row is read.
    ///
    /// `after_id` is the keyset that continues a scan: only rows after it are
    /// read, in `id` order. It is what the value filter pages with — an
    /// `OFFSET` would re-walk every row the previous pages had already read
    /// (post-audit-debts C3).
    #[allow(clippy::too_many_arguments)]
    fn list_page(
        &self,
        namespaces: &NamespaceScope,
        states: &[String],
        limit: usize,
        offset: usize,
        deleted: bool,
        newest_first: bool,
        memory_types: &[String],
        like_value: Option<&str>,
        after_id: Option<i64>,
    ) -> Result<Vec<Memory>, MnexisError> {
        // The superset predicate of the value filter, declared before the
        // parameters that borrow it (it outlives them).
        let like_pattern = like_value.map(|value| format!("%{value}%"));
        let Some((filter, mut param_refs)) = filter_sql(namespaces, states, deleted, memory_types)
        else {
            return Ok(Vec::new());
        };
        let mut sql = format!("SELECT {SELECT_COLUMNS}{filter}");
        let after_val: i64 = after_id.unwrap_or(0);
        if after_id.is_some() {
            // The keyset: the next page starts where the last one ended.
            sql.push_str(" AND id > ?");
            param_refs.push(&after_val);
        }
        if let Some(pattern) = like_pattern.as_ref() {
            // The value is digits, so it carries no wildcard of its own:
            // the pattern is a plain substring search over the four
            // indexed columns.
            sql.push_str(
                " AND (content LIKE ? OR title LIKE ? OR topic_key LIKE ? OR keywords LIKE ?)",
            );
            for _ in 0..4 {
                param_refs.push(pattern as &dyn ToSql);
            }
        }
        if after_id.is_some() {
            // A keyset scan reads forward, always: it is exhaustive, and its
            // caller counts matches, not rows.
            sql.push_str(" ORDER BY id LIMIT ?");
        } else {
            sql.push_str(if newest_first {
                " ORDER BY id DESC LIMIT ? OFFSET ?"
            } else {
                " ORDER BY id LIMIT ? OFFSET ?"
            });
        }
        let limit_val: i64 = limit as i64;
        let offset_val: i64 = offset as i64;
        param_refs.push(&limit_val);
        if after_id.is_none() {
            param_refs.push(&offset_val);
        }

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

    /// The records that **carry the value**, paged over the matches.
    ///
    /// The value is a run of three or more digits (see
    /// [`crate::domain::value`]): the SQL narrows the rows with the
    /// superset predicate and the rule itself is decided here, on the row
    /// that was read. The limit and the offset count **matches**, not the
    /// rows the engine scanned: a namespace of five thousand records with
    /// two that carry the value answers with those two.
    ///
    /// The scan pages with a **keyset** (`id > last`), never with an
    /// `OFFSET`: an offset makes the engine re-walk every row the previous
    /// pages had already read, which costs quadratic row visits on the hot
    /// path the extension calls (post-audit-debts C3).
    #[allow(clippy::too_many_arguments)]
    fn list_by_value(
        &self,
        namespaces: &NamespaceScope,
        states: &[String],
        limit: usize,
        offset: usize,
        deleted: bool,
        memory_types: &[String],
        value: &str,
    ) -> Result<Vec<Memory>, MnexisError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut matched: Vec<Memory> = Vec::new();
        // The matches the offset skips.
        let mut skipped = 0usize;
        // The last row the scan has read: the next chunk continues after it.
        let mut after: Option<i64> = None;
        loop {
            let rows = self.list_page(
                namespaces,
                states,
                VALUE_SCAN_CHUNK,
                0,
                deleted,
                false,
                memory_types,
                Some(value),
                after,
            )?;
            if rows.is_empty() {
                break;
            }
            let read = rows.len();
            // Every page read from the table carries its ids: without one the
            // scan cannot continue, and the pages it has are the answer.
            after = rows.last().and_then(|row| row.id);
            for row in rows {
                if !crate::domain::value::record_carries_value(
                    &row.content,
                    &row.title,
                    &row.topic_key,
                    &row.keywords,
                    value,
                ) {
                    continue;
                }
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                matched.push(row);
                if matched.len() == limit {
                    return Ok(matched);
                }
            }
            if read < VALUE_SCAN_CHUNK || after.is_none() {
                break;
            }
        }
        Ok(matched)
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
    states: &'a [String],
    deleted: bool,
    memory_types: &'a [String],
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
    // The lifecycle states the caller accepts: the set filters BEFORE the
    // limit, so the excluded rows can never crowd out the kept ones.
    if !states.is_empty() {
        sql.push_str(&format!(" AND state IN ({})", placeholders(states.len())));
        for state in states {
            param_refs.push(state as &dyn ToSql);
        }
    }
    // The kind of thing the fact is: an exact match, and it filters
    // BEFORE the limit, so a filtered answer is not a short answer.
    if !memory_types.is_empty() {
        sql.push_str(&format!(
            " AND memory_type IN ({})",
            placeholders(memory_types.len())
        ));
        for memory_type in memory_types {
            param_refs.push(memory_type as &dyn ToSql);
        }
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

const SELECT_COLUMNS: &str = "id, uuid, title, topic_key, namespace, memory_type, state, content, \
                                revision, deleted, created_at, updated_at, deleted_at, purge_after, \
                                valid_from, valid_until, actor, channel, assertion_basis, keywords";

/// How many rows the value filter reads at a time.
///
/// The filter is exhaustive: it keeps reading until the namespace ends or
/// the limit is filled with matches. The chunk is an internal detail — the
/// caller's limit counts matches, never rows — and it only decides how
/// often the engine goes back to the database.
const VALUE_SCAN_CHUNK: usize = 256;

impl MemoryRepository for SqliteMemoryRepository {
    fn save(&mut self, mem: &Memory) -> Result<i64, MnexisError> {
        Self::insert_memory(&self.conn, mem)
    }

    /// Create one record with everything that belongs to it — tags, the
    /// idempotency row, and the row the caller reads back — as one write.
    ///
    /// The read-back is inside the transaction on purpose: the caller gets the
    /// canonical row (assigned rowid, persisted timestamps) only when the row
    /// and its companions committed.
    fn save_with_tags_and_idempotency(
        &mut self,
        mem: &Memory,
        tags: &[String],
        idempotency: Option<&IdempotencyRecord>,
    ) -> Result<Memory, MnexisError> {
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;
        let result: Result<Memory, MnexisError> = (|| {
            Self::insert_memory(&tx, mem)?;
            if !tags.is_empty() {
                Self::write_tags(&tx, mem.uuid.as_str(), tags)?;
            }
            if let Some(record) = idempotency {
                Self::write_idempotency(&tx, record)?;
            }
            Self::select_memory(&tx, mem.uuid.as_str(), false)?
                .ok_or_else(|| MnexisError::not_found(format!("memory:{}", mem.uuid)))
        })();
        match result {
            Ok(stored) => {
                tx.commit().map_err(sql_to_domain)?;
                Ok(stored)
            }
            // Dropping the transaction rolls every write of this save back.
            Err(e) => Err(e),
        }
    }

    /// Insert a whole batch in one transaction: every record lands or none
    /// does. A duplicate topic key or uuid anywhere aborts the whole batch.
    fn save_batch(&mut self, memories: &[Memory]) -> Result<(), MnexisError> {
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;
        for mem in memories {
            Self::insert_memory(&tx, mem)?;
        }
        tx.commit().map_err(sql_to_domain)
    }

    fn get_by_uuid(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        Self::select_memory(&self.conn, uuid, false)
    }

    fn get_by_uuid_including_deleted(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        Self::select_memory(&self.conn, uuid, true)
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
        let states: Vec<String> = state.map(String::from).into_iter().collect();
        self.list_filtered(&scope, &states, limit, offset, false, false, &[], None)
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
        self.list_filtered(&scope, &[], limit, offset, true, false, &[], None)
    }

    fn list_in_typed(
        &self,
        namespaces: &NamespaceScope,
        state: Option<&str>,
        limit: usize,
        offset: usize,
        deleted: bool,
        memory_types: &[String],
        value: Option<&str>,
    ) -> Result<Vec<Memory>, MnexisError> {
        let states: Vec<String> = state.map(String::from).into_iter().collect();
        self.list_filtered(
            namespaces,
            &states,
            limit,
            offset,
            deleted,
            false,
            memory_types,
            value,
        )
    }

    fn list_recent(
        &self,
        namespaces: &NamespaceScope,
        limit: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        self.list_filtered(namespaces, &[], limit, 0, false, true, &[], None)
    }

    fn list_typed_state_recent(
        &self,
        namespaces: &NamespaceScope,
        memory_types: &[String],
        states: &[String],
        limit: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        self.list_filtered(
            namespaces,
            states,
            limit,
            0,
            false,
            true,
            memory_types,
            None,
        )
    }

    fn count_in(&self, namespaces: &NamespaceScope) -> Result<i64, MnexisError> {
        let Some((filter, param_refs)) = filter_sql(namespaces, &[], false, &[]) else {
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
        let keywords = Memory::keywords_column(&mem.keywords);
        let new_revision = expected_revision + 1;
        let affected = self
            .conn
            .execute(
                "UPDATE memories SET
                    content = ?1, state = ?2, topic_key = ?3, memory_type = ?4,
                    actor = ?5, channel = ?6, assertion_basis = ?7,
                    valid_from = ?8, valid_until = ?9,
                    revision = ?10, updated_at = ?11, keywords = ?12
                 WHERE id = ?13 AND revision = ?14",
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
                    keywords,
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
        // Which records are eligible: the predicate `memories` is deleted
        // with, repeated on the rows that point at it.
        let cutoff: Option<String> = before.map(str::to_string);
        let filter: &str = if all {
            "deleted = 1"
        } else if cutoff.is_some() {
            "deleted = 1 AND purge_after IS NOT NULL AND purge_after <= ?1"
        } else {
            return Err(MnexisError::invalid_argument(
                "before",
                "either 'before' (ISO-8601 cutoff) or 'all' must be specified",
            ));
        };
        let mut bound: Vec<&dyn ToSql> = Vec::new();
        // `--all` wins over a cutoff, exactly as it did before: the filter
        // it selects has no placeholder, so the cutoff is not bound.
        if !all {
            if let Some(cutoff) = cutoff.as_ref() {
                bound.push(cutoff);
            }
        }

        // The trail does not outlive the record it describes: the history
        // and link rows of the records being purged go first, in the same
        // transaction. `memory_history.memory_uuid` and the two
        // `memory_links` columns point at `memories(uuid)` without a
        // cascade, so without this step a soft-deleted record that has any
        // trail could never be purged (the foreign keys pinned it), and the
        // purge failed with BUSY. `memory_tags` already cascades.
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;
        tx.execute(
            &format!(
                "DELETE FROM memory_history WHERE memory_uuid IN \
                 (SELECT uuid FROM memories WHERE {filter})"
            ),
            bound.as_slice(),
        )
        .map_err(sql_to_domain)?;
        tx.execute(
            &format!(
                "DELETE FROM memory_links \
                 WHERE older_uuid IN (SELECT uuid FROM memories WHERE {filter}) \
                    OR newer_uuid IN (SELECT uuid FROM memories WHERE {filter})"
            ),
            bound.as_slice(),
        )
        .map_err(sql_to_domain)?;
        let purged = tx
            .execute(
                &format!("DELETE FROM memories WHERE {filter}"),
                bound.as_slice(),
            )
            .map_err(sql_to_domain)?;
        tx.commit().map_err(sql_to_domain)?;
        Ok(purged)
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

    fn links_between(&self, uuids: &[String]) -> Result<Vec<MemoryLink>, MnexisError> {
        if uuids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; uuids.len()].join(", ");
        let sql = format!(
            "SELECT id, older_uuid, newer_uuid, link_type, reason, created_at \
             FROM memory_links WHERE older_uuid IN ({placeholders}) \
                OR newer_uuid IN ({placeholders}) ORDER BY id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(sql_to_domain)?;
        let mut params: Vec<&dyn ToSql> = Vec::new();
        for uuid in uuids {
            params.push(uuid);
        }
        for uuid in uuids {
            params.push(uuid);
        }
        let rows = stmt
            .query_map(params.as_slice(), |row| {
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

        // The trail records the state the row had **before** the flip:
        // the revision the supersede replaces, its content, its pre-flip
        // state. Read inside the transaction, before the UPDATE, so the
        // row and the flip cannot disagree.
        let pre_flip = tx
            .query_row(
                "SELECT revision, memory_type, content, state, valid_from, valid_until, \
                        assertion_basis, actor \
                 FROM memories WHERE uuid = ?1",
                params![older_uuid],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_to_domain)?;
        let Some((
            revision,
            memory_type,
            content,
            state,
            valid_from,
            valid_until,
            assertion_basis,
            actor,
        )) = pre_flip
        else {
            return Err(MnexisError::not_found(format!("memory:{older_uuid}")));
        };

        // Older side: flip to superseded under the optimistic guard. Only
        // state / updated_at / revision change; the trail keeps what the
        // row was.
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
            "INSERT INTO memory_history (memory_uuid, revision, memory_type, content, state, \
                    valid_from, valid_until, assertion_basis, changed_by, change_reason, changed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                older_uuid,
                revision,
                memory_type,
                content,
                state,
                valid_from,
                valid_until,
                assertion_basis,
                actor,
                reason,
                now,
            ],
        )
        .map_err(sql_to_domain)?;

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
        Self::write_tags(&self.conn, uuid, tags)
    }

    /// Apply an update + history + tag rewrite + idempotency row as a single
    /// transaction. Used by `MemoryService::update` and by the re-save that
    /// lands on an existing record (`MemoryService::update_occupied`), so that
    /// a failure at any step rolls back the entire change
    /// (T23 `partial_failure_no_partial_writes`).
    fn update_with_history_and_tags(
        &mut self,
        updated: &Memory,
        expected_revision: i32,
        history: &MemoryHistory,
        tags: Option<&[String]>,
        idempotency: Option<&IdempotencyRecord>,
    ) -> Result<Memory, MnexisError> {
        let tx = self.conn.unchecked_transaction().map_err(sql_to_domain)?;
        let keywords = Memory::keywords_column(&updated.keywords);
        let result: Result<Memory, MnexisError> = (|| {
            let rows = tx
                .execute(
                    "UPDATE memories SET \
                       title = ?1, content = ?2, state = ?3, revision = revision + 1, \
                       updated_at = ?4, assertion_basis = ?5, \
                       topic_key = ?6, memory_type = ?7, \
                       valid_from = ?8, valid_until = ?9, \
                       actor = ?10, channel = ?11, keywords = ?12 \
                     WHERE uuid = ?13 AND revision = ?14",
                    params![
                        updated.title,
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
                        keywords,
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
            if let Some(tags) = tags
                && !tags.is_empty()
            {
                Self::write_tags(&tx, history.memory_uuid.as_str(), tags)?;
            }
            if let Some(record) = idempotency {
                Self::write_idempotency(&tx, record)?;
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
        Self::write_idempotency(&self.conn, r)
    }

    fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
        mode: MatchMode,
    ) -> Result<Vec<SearchHit>, MnexisError> {
        let scope = match namespace {
            Some(ns) => NamespaceScope::Only(vec![ns.to_string()]),
            None => NamespaceScope::All,
        };
        self.search_in(&scope, query, limit, mode)
    }

    fn search_in_typed(
        &self,
        namespaces: &NamespaceScope,
        query: &str,
        limit: usize,
        mode: MatchMode,
        memory_types: &[String],
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
            mode,
            ns_filter,
            crate::retrieval::StateFilter::default(),
            crate::retrieval::DeletedFilter::default(),
            None,
            memory_types,
            limit,
        )
    }

    // --- sessions and prompts ---

    fn start_session(&mut self, session: &Session) -> Result<Session, MnexisError> {
        // A session that already exists is the caller's session: the
        // engine never restarts it and never renames it.
        if let Some(existing) = self.get_session(&session.id)? {
            return Ok(existing);
        }
        self.conn
            .execute(
                "INSERT INTO sessions (id, namespace, directory, started_at, \
                 branch, branch_sha, branch_remote) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session.id,
                    session.namespace,
                    session.directory,
                    session.started_at,
                    session.branch,
                    session.branch_sha,
                    session.branch_remote
                ],
            )
            .map_err(|e| MnexisError::busy(format!("session start: {e}")))?;
        Ok(session.clone())
    }

    fn end_session(
        &mut self,
        id: &str,
        ended_at: &str,
        summary: Option<&str>,
    ) -> Result<Session, MnexisError> {
        let existing = self
            .get_session(id)?
            .ok_or_else(|| MnexisError::not_found(format!("session:{id}")))?;
        // A session that already ended keeps its end: only the summary can
        // still change.
        let ended = existing
            .ended_at
            .clone()
            .unwrap_or_else(|| ended_at.to_string());
        let new_summary = summary.map(|s| s.to_string()).or(existing.summary);
        self.conn
            .execute(
                "UPDATE sessions SET ended_at = ?1, summary = ?2 WHERE id = ?3",
                params![ended, new_summary, id],
            )
            .map_err(|e| MnexisError::busy(format!("session end: {e}")))?;
        Ok(Session {
            ended_at: Some(ended),
            summary: new_summary,
            ..existing
        })
    }

    fn get_session(&self, id: &str) -> Result<Option<Session>, MnexisError> {
        self.conn
            .query_row(
                "SELECT id, namespace, directory, started_at, ended_at, summary, \
                 branch, branch_sha, branch_remote \
                 FROM sessions WHERE id = ?1",
                [id],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        namespace: row.get(1)?,
                        directory: row.get(2)?,
                        started_at: row.get(3)?,
                        ended_at: row.get(4)?,
                        summary: row.get(5)?,
                        branch: row.get(6)?,
                        branch_sha: row.get(7)?,
                        branch_remote: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|e| MnexisError::busy(format!("session get: {e}")))
    }

    fn list_sessions(
        &self,
        namespaces: &NamespaceScope,
        limit: usize,
    ) -> Result<Vec<Session>, MnexisError> {
        let ns_filter = namespace_filter(namespaces);
        let (mut sql, owned) = session_query(&ns_filter);
        sql.push_str(" LIMIT ?");
        let limit_i64: i64 = limit as i64;
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| MnexisError::busy(format!("sessions list: {e}")))?;
        let mut all_params: Vec<&dyn ToSql> = owned.iter().map(|s| s as &dyn ToSql).collect();
        all_params.push(&limit_i64);
        let rows = stmt
            .query_map(&all_params[..], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    directory: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    summary: row.get(5)?,
                    branch: row.get(6)?,
                    branch_sha: row.get(7)?,
                    branch_remote: row.get(8)?,
                })
            })
            .map_err(|e| MnexisError::busy(format!("sessions list: {e}")))?;
        let sessions = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MnexisError::busy(format!("sessions list: {e}")))?;
        Ok(sessions)
    }

    fn previous_finished_session(&self, namespace: &str) -> Result<Option<Session>, MnexisError> {
        // Finished means `ended_at` is not null; the one that ended last is
        // the session before this one. The clock the engine writes has
        // second granularity, so two sessions can end in the same second:
        // `started_at`, then the registration order, break the tie, and the
        // answer stays deterministic.
        self.conn
            .query_row(
                "SELECT id, namespace, directory, started_at, ended_at, summary, \
                 branch, branch_sha, branch_remote FROM sessions \
                 WHERE namespace = ?1 AND ended_at IS NOT NULL \
                 ORDER BY ended_at DESC, started_at DESC, rowid DESC LIMIT 1",
                [namespace],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        namespace: row.get(1)?,
                        directory: row.get(2)?,
                        started_at: row.get(3)?,
                        ended_at: row.get(4)?,
                        summary: row.get(5)?,
                        branch: row.get(6)?,
                        branch_sha: row.get(7)?,
                        branch_remote: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|e| MnexisError::busy(format!("previous session: {e}")))
    }

    fn memories_in_window(
        &self,
        namespace: &str,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        // The window is inclusive on both ends: a memory written in the same
        // second the session started or ended was written inside it. The
        // timestamps are the same fixed-width RFC3339 UTC the engine writes,
        // so the string comparison orders them the way the clock does.
        let limit_val: i64 = limit as i64;
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM memories \
                 WHERE deleted = 0 AND namespace = ?1 \
                 AND created_at >= ?2 AND created_at <= ?3 \
                 ORDER BY created_at, id LIMIT ?4"
            ))
            .map_err(sql_to_domain)?;
        let rows = stmt
            .query_map(params![namespace, from, to, limit_val], |row| {
                Self::row_to_memory(row).map_err(to_sql_err)
            })
            .map_err(sql_to_domain)?;
        let mut memories = Vec::new();
        for row in rows {
            memories.push(row.map_err(sql_to_domain)?);
        }
        Ok(memories)
    }

    fn save_prompt(&mut self, prompt: &UserPrompt) -> Result<i64, MnexisError> {
        self.conn
            .execute(
                "INSERT INTO user_prompts (session_id, namespace, content, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    prompt.session_id,
                    prompt.namespace,
                    prompt.content,
                    prompt.created_at
                ],
            )
            .map_err(|e| MnexisError::busy(format!("prompt save: {e}")))?;
        Ok(self.conn.last_insert_rowid())
    }

    fn list_prompts(
        &self,
        namespaces: &NamespaceScope,
        session_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, MnexisError> {
        let ns_filter = namespace_filter(namespaces);
        let (mut sql, mut owned) = prompt_query(&ns_filter);
        let session_owned;
        if let Some(session) = session_id {
            sql.push_str(" AND session_id = ?");
            session_owned = session.to_string();
            owned.push(session_owned.clone());
        }
        sql.push_str(" ORDER BY id DESC LIMIT ?");
        let limit_i64: i64 = limit as i64;
        let mut all_params: Vec<&dyn ToSql> = owned.iter().map(|s| s as &dyn ToSql).collect();
        all_params.push(&limit_i64);
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| MnexisError::busy(format!("prompts list: {e}")))?;
        let rows = stmt
            .query_map(&all_params[..], |row| {
                Ok(UserPrompt {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    namespace: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| MnexisError::busy(format!("prompts list: {e}")))?;
        let prompts = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| MnexisError::busy(format!("prompts list: {e}")))?;
        Ok(prompts)
    }

    fn search_prompts(
        &self,
        namespaces: &NamespaceScope,
        query: &str,
        limit: usize,
        mode: MatchMode,
    ) -> Result<Vec<UserPrompt>, MnexisError> {
        let ns_filter = namespace_filter(namespaces);
        let hits = crate::retrieval::search_prompts(&self.conn, query, mode, ns_filter, limit)?;
        Ok(hits.iter().map(crate::retrieval::prompt_from_hit).collect())
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

/// The namespace filter for a scope, in the shape the retrieval layer
/// expects.
fn namespace_filter(namespaces: &NamespaceScope) -> crate::retrieval::NamespaceFilter {
    match namespaces {
        NamespaceScope::All => crate::retrieval::NamespaceFilter::Any,
        NamespaceScope::AllExcept(list) => {
            crate::retrieval::NamespaceFilter::AnyExcept(list.clone())
        }
        NamespaceScope::Only(list) => crate::retrieval::NamespaceFilter::Many(list.clone()),
    }
}

/// The session query for a namespace filter.
fn session_query(ns: &crate::retrieval::NamespaceFilter) -> (String, Vec<String>) {
    let mut sql = String::from(
        "SELECT id, namespace, directory, started_at, ended_at, summary, \
         branch, branch_sha, branch_remote FROM sessions WHERE 1=1",
    );
    let mut params: Vec<String> = Vec::new();
    match ns {
        crate::retrieval::NamespaceFilter::Any => {}
        crate::retrieval::NamespaceFilter::Namespace(one) => {
            sql.push_str(" AND namespace = ?");
            params.push(one.clone());
        }
        crate::retrieval::NamespaceFilter::AnyExcept(list) => {
            if !list.is_empty() {
                sql.push_str(" AND namespace NOT IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                params.extend(list.iter().cloned());
            }
        }
        crate::retrieval::NamespaceFilter::Many(list) => {
            if list.is_empty() {
                sql.push_str(" AND 0");
            } else {
                sql.push_str(" AND namespace IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                params.extend(list.iter().cloned());
            }
        }
    }
    sql.push_str(" ORDER BY started_at DESC");
    (sql, params)
}

/// The prompt query for a namespace filter.
fn prompt_query(ns: &crate::retrieval::NamespaceFilter) -> (String, Vec<String>) {
    let mut sql = String::from(
        "SELECT id, session_id, namespace, content, created_at FROM user_prompts WHERE 1=1",
    );
    let mut params: Vec<String> = Vec::new();
    match ns {
        crate::retrieval::NamespaceFilter::Any => {}
        crate::retrieval::NamespaceFilter::Namespace(one) => {
            sql.push_str(" AND namespace = ?");
            params.push(one.clone());
        }
        crate::retrieval::NamespaceFilter::AnyExcept(list) => {
            if !list.is_empty() {
                sql.push_str(" AND namespace NOT IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                params.extend(list.iter().cloned());
            }
        }
        crate::retrieval::NamespaceFilter::Many(list) => {
            if list.is_empty() {
                sql.push_str(" AND 0");
            } else {
                sql.push_str(" AND namespace IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                params.extend(list.iter().cloned());
            }
        }
    }
    (sql, params)
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
            .update_with_history_and_tags(&stale, 1, &history, None, None)
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

    /// A value filter that needs more than one chunk still pages exactly, and
    /// it pages by keyset: the chunk after the first continues from the last
    /// row it read, so no row is read twice (post-audit-debts C3).
    ///
    /// The fixture makes the superset predicate wide on purpose: most rows
    /// only *contain* the digits inside a longer run (and therefore do not
    /// carry the value), which is the shape that made the old `OFFSET` scan
    /// re-walk the table page after page.
    #[test]
    fn the_value_filter_pages_across_the_scan_chunk() {
        let dir = tempdir_in("target").unwrap();
        let conn = open_sqlite(&dir.path().join("value.db")).unwrap();
        let mut repo = SqliteMemoryRepository::new(conn);
        // 700 rows, more than two scan chunks of 256: every third row carries
        // the value as a whole run, the rest carry it inside a longer run.
        let mut expected: Vec<String> = Vec::new();
        let mut rows: Vec<Memory> = Vec::new();
        for i in 0..700 {
            let uuid = format!("uuid-{i}");
            let content = if i % 3 == 0 {
                format!("the port is 8081 in row {i}")
            } else {
                format!("the port is 80810 in row {i}")
            };
            let mut mem = Memory::new_proposed(
                uuid.clone(),
                format!("topic.{i}"),
                "proyecto-a".to_string(),
                "fact".to_string(),
                content,
                "user".to_string(),
                "cli".to_string(),
                "user_stated".to_string(),
                "2026-01-01T00:00:00Z".to_string(),
            );
            mem.state = MemoryState::Active;
            if i % 3 == 0 {
                expected.push(uuid);
            }
            rows.push(mem);
        }
        // One batch: the fixture is 700 rows and the test is about the scan,
        // not about how fast the engine can write them.
        repo.save_batch(&rows).unwrap();

        let scope = NamespaceScope::Only(vec!["proyecto-a".to_string()]);
        let uuids =
            |rows: Vec<Memory>| -> Vec<String> { rows.into_iter().map(|row| row.uuid).collect() };

        // Pages that cross the 256-row chunk boundary, and pages that start
        // after it: the offset counts matches, the scan still reads forward.
        for (offset, limit) in [(0usize, 5usize), (80, 5), (170, 5), (230, 10), (0, 300)] {
            let page = repo
                .list_in_typed(&scope, None, limit, offset, false, &[], Some("8081"))
                .expect("the value filter answers");
            let want: Vec<String> = expected.iter().skip(offset).take(limit).cloned().collect();
            assert_eq!(
                uuids(page),
                want,
                "offset {offset}, limit {limit}: the page is the matches, in row order"
            );
        }

        // A page past the last match answers nothing, not the first page.
        let empty = repo
            .list_in_typed(&scope, None, 5, expected.len(), false, &[], Some("8081"))
            .expect("the value filter answers");
        assert!(empty.is_empty(), "past the last match is an empty page");
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
        let hits = repo
            .search("content", Some("proyecto-a"), 10, MatchMode::Any)
            .unwrap();
        assert_eq!(
            hits.len(),
            2,
            "search must return both rows after a reindex"
        );
    }
}
