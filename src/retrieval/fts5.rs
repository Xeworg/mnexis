//! FTS5 ranked search over the production `memories` table.
//!
//! See `odd/tasks/e2-core.md` §9 for the design. The defaults match PRD §9:
//! `state = ActiveOnly`, `deleted = NotDeletedOnly`. `ValidWindow` further
//! filters to memories whose validity interval contains the reference time.

use rusqlite::{Connection, ToSql};

use crate::domain::error::MnexisError;

/// Namespace filter applied to every search.
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceFilter {
    /// Match memories in any namespace.
    Any,
    /// Match memories in exactly this namespace.
    Namespace(String),
}

/// State predicate. The PRD §9 default excludes proposed/disputed/superseded/archived.
#[derive(Debug, Clone, PartialEq)]
pub enum StateFilter {
    /// Match only `state = 'active'`.
    ActiveOnly,
    /// Match regardless of `state`.
    Any,
}

impl Default for StateFilter {
    fn default() -> Self {
        Self::ActiveOnly
    }
}

/// Deleted predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum DeletedFilter {
    /// Match only `deleted = 0`.
    NotDeletedOnly,
    /// Match regardless of `deleted`.
    Any,
}

impl Default for DeletedFilter {
    fn default() -> Self {
        Self::NotDeletedOnly
    }
}

/// Optional validity window `(valid_from, valid_until)` — both bounds
/// inclusive, with `None` meaning unbounded on that side. A memory is
/// eligible when its `valid_from <= reference` and `valid_until >= reference`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValidWindow {
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

/// Ranked FTS5 hit. `rank` is bm25 (more negative = more relevant). All
/// fields are returned by the production SQL so callers can sort, render,
/// and (via `explain`) drill in further.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub uuid: String,
    pub rank: f64,
    pub namespace: String,
    pub memory_type: String,
    pub state: String,
    pub revision: i32,
    pub created_at: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

/// Ranked FTS5 search. Returns hits ordered by bm25 rank ascending
/// (most relevant first). A query with no matching documents returns an
/// empty `Vec`, not an error.
pub fn search(
    conn: &Connection,
    query: &str,
    namespace: NamespaceFilter,
    state: StateFilter,
    deleted: DeletedFilter,
    valid_window: Option<ValidWindow>,
    limit: usize,
) -> Result<Vec<SearchHit>, MnexisError> {
    let mut sql = String::from(
        "SELECT m.uuid, bm25(memories_fts) AS rank, m.namespace, m.memory_type, m.state, \
         m.revision, m.created_at, m.valid_from, m.valid_until \
         FROM memories_fts \
         JOIN memories AS m ON memories_fts.rowid = m.id \
         WHERE memories_fts MATCH ?1",
    );
    let mut params: Vec<&dyn ToSql> = vec![&query];

    // State filter
    match state {
        StateFilter::ActiveOnly => sql.push_str(" AND m.state = 'active'"),
        StateFilter::Any => {}
    }

    // Deleted filter
    match deleted {
        DeletedFilter::NotDeletedOnly => sql.push_str(" AND m.deleted = 0"),
        DeletedFilter::Any => {}
    }

    // Namespace filter
    match &namespace {
        NamespaceFilter::Any => {}
        NamespaceFilter::Namespace(ns) => {
            sql.push_str(" AND m.namespace = ?");
            params.push(ns);
        }
    }

    // Validity window filter. When a bound is Some, only memories whose
    // validity interval contains that reference point are eligible.
    if let Some(window) = &valid_window {
        if let Some(vf) = &window.valid_from {
            sql.push_str(" AND (m.valid_from IS NULL OR m.valid_from <= ?)");
            params.push(vf);
        }
        if let Some(vu) = &window.valid_until {
            sql.push_str(" AND (m.valid_until IS NULL OR m.valid_until >= ?)");
            params.push(vu);
        }
    }

    sql.push_str(" ORDER BY rank ASC LIMIT ?");
    let limit_i64: i64 = limit as i64;
    params.push(&limit_i64);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| MnexisError::busy(format!("fts5::search prepare: {e}")))?;
    let rows = stmt
        .query_map(&params[..], |row| {
            Ok(SearchHit {
                uuid: row.get(0)?,
                rank: row.get(1)?,
                namespace: row.get(2)?,
                memory_type: row.get(3)?,
                state: row.get(4)?,
                revision: row.get(5)?,
                created_at: row.get(6)?,
                valid_from: row.get(7)?,
                valid_until: row.get(8)?,
            })
        })
        .map_err(|e| MnexisError::busy(format!("fts5::search query_map: {e}")))?;
    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MnexisError::busy(format!("fts5::search collect: {e}")))?;
    Ok(hits)
}
