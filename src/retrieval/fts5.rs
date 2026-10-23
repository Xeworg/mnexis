//! FTS5 ranked search over the production `memories` table.
//!
//! See `odd/tasks/e2-core.md` §9 for the design. The defaults match PRD §9:
//! `state = ActiveOnly`, `deleted = NotDeletedOnly`. `ValidWindow` further
//! filters to memories whose validity interval contains the reference time.
//!
//! The index is `fts5(content, title, topic_key)` since identity-visibility
//! S2, so a `MATCH` searches the identity of a fact as well as its content —
//! the words of a title or of a topic key find the record whose content says
//! something else. Nothing else about the query layer changed: the compiled
//! expression, the three match modes and `bm25(memories_fts)` are the same,
//! and `bm25` is deliberately left **unweighted** so that this module and
//! `retrieval::explain` score a query identically.

use rusqlite::{Connection, ToSql};

use crate::domain::error::MnexisError;
use crate::domain::query::MatchMode;

/// Namespace filter applied to every search.
#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceFilter {
    /// Match memories in any namespace.
    Any,
    /// Match memories in exactly this namespace.
    Namespace(String),
    /// Match memories in any of these namespaces.
    Many(Vec<String>),
    /// Match memories in every namespace except these.
    AnyExcept(Vec<String>),
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
#[allow(clippy::too_many_arguments)]
pub fn search(
    conn: &Connection,
    query: &str,
    mode: MatchMode,
    namespace: NamespaceFilter,
    state: StateFilter,
    deleted: DeletedFilter,
    valid_window: Option<ValidWindow>,
    limit: usize,
) -> Result<Vec<SearchHit>, MnexisError> {
    // The caller's words become a safe expression before they reach the
    // index: a multi-word query is not a phrase, and a hyphen is not syntax.
    let expression = crate::domain::query::compile(query, mode)?;
    let mut sql = String::from(
        "SELECT m.uuid, bm25(memories_fts) AS rank, m.namespace, m.memory_type, m.state, \
         m.revision, m.created_at, m.valid_from, m.valid_until \
         FROM memories_fts \
         JOIN memories AS m ON memories_fts.rowid = m.id \
         WHERE memories_fts MATCH ?1",
    );
    let mut params: Vec<&dyn ToSql> = vec![&expression];

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
        NamespaceFilter::Many(namespaces) => {
            if namespaces.is_empty() {
                sql.push_str(" AND 0");
            } else {
                let placeholders = vec!["?"; namespaces.len()].join(", ");
                sql.push_str(&format!(" AND m.namespace IN ({placeholders})"));
                for ns in namespaces {
                    params.push(ns);
                }
            }
        }
        NamespaceFilter::AnyExcept(namespaces) => {
            if !namespaces.is_empty() {
                let placeholders = vec!["?"; namespaces.len()].join(", ");
                sql.push_str(&format!(" AND m.namespace NOT IN ({placeholders})"));
                for ns in namespaces {
                    params.push(ns);
                }
            }
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
        .map_err(|e| fts_unavailable_or_busy("fts5::search prepare", &e))?;
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
        .map_err(|e| fts_query_error("fts5::search query_map", &e))?;
    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fts_query_error("fts5::search collect", &e))?;
    Ok(hits)
}

/// Map a rusqlite error from the search path.
///
/// A query the engine cannot parse is the caller's problem, not the index's:
/// it has to surface as an invalid argument naming the query, never as
/// `BUSY` (which means "try later") and never as `INDEX_UNAVAILABLE` (which
/// sends the operator to `mnexis doctor --repair` for a problem the repair
/// cannot fix).
fn fts_query_error(stage: &str, e: &rusqlite::Error) -> MnexisError {
    let msg = e.to_string();
    if is_query_error(&msg) {
        MnexisError::invalid_argument("query", format!("{stage}: {e}"))
    } else {
        fts_unavailable_or_busy(stage, e)
    }
}

/// True when an FTS5 error means the **query** is wrong, not the index.
///
/// The compiler quotes every term, so a query the caller sent cannot reach
/// FTS5 as syntax — this is the defence in depth for the paths that do
/// (a future mode, a raw query, a SQLite that parses differently). The
/// decision is a pure function of the message so it can be pinned by a
/// test instead of by an input that no longer exists.
fn is_query_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no such column")
        || lower.contains("fts5: syntax error")
        || lower.contains("unterminated string")
        || lower.contains("unknown special query")
        || lower.contains("malformed match")
}

/// Map a rusqlite error from an FTS5 path. When the message indicates the
/// FTS index or triggers are missing/unavailable, surface
/// `IndexUnavailable` so the operator knows to run `mnexis doctor --repair`.
/// Anything else is treated as the BUSY path the previous design used.
fn fts_unavailable_or_busy(stage: &str, e: &rusqlite::Error) -> MnexisError {
    let msg = e.to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("no such table")
        || lower.contains("fts5")
        || lower.contains("no such column")
        || lower.contains("database disk image is malformed")
    {
        MnexisError::index_unavailable(format!("{stage}: {e}"))
    } else {
        MnexisError::busy(format!("{stage}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::is_query_error;

    #[test]
    fn a_query_the_engine_cannot_parse_is_a_query_error() {
        // The messages SQLite produces for the ways a query can be wrong.
        for message in [
            "no such column: eu",
            "no such column: api",
            "fts5: syntax error near \"@\"",
            "fts5: syntax error near \"\"",
            "unterminated string",
            "unknown special query: *",
            "malformed MATCH expression: []",
        ] {
            assert!(
                is_query_error(message),
                "a query error must be recognised: {message}"
            );
        }
    }

    #[test]
    fn a_broken_index_is_not_a_query_error() {
        // The messages SQLite produces for the ways the *index* is wrong:
        // those must keep pointing at the repair, not at the caller.
        for message in [
            "no such table: memories_fts",
            "no such table: main.memories",
            "database disk image is malformed",
            "database is locked",
        ] {
            assert!(
                !is_query_error(message),
                "an index error must not be blamed on the query: {message}"
            );
        }
    }
}
