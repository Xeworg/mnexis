//! Per-result explanation of an FTS5 hit: rank, matched terms, and
//! (for excluded candidates) the reason a memory was filtered out.
//!
//! `explain` shares its WHERE clause with `fts5::search`; each returned
//! hit carries the subset of query tokens that matched the row's content.
//! This is the dev/doctor surface; production `search` returns the same
//! SQL rows with the rank baked in.

use rusqlite::{Connection, ToSql};

use crate::domain::error::MnexisError;

use super::fts5::{DeletedFilter, NamespaceFilter, StateFilter, ValidWindow};

/// Per-hit explanation. Carries the same fields as a `SearchHit` plus the
/// list of query tokens that the FTS5 parser matched against this row.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplainedHit {
    pub uuid: String,
    pub rank: f64,
    pub namespace: String,
    pub memory_type: String,
    pub state: String,
    pub revision: i32,
    pub created_at: String,
    /// Tokens from `query` that matched this hit (FTS5 normalisation:
    /// lowercased, simple tokenisation).
    pub matched_terms: Vec<String>,
}

/// Explain a query. Same SQL filter semantics as `fts5::search`; each
/// returned hit includes the subset of query tokens that appeared in the
/// row's content. A query with no matching documents returns an empty
/// `Vec`, not an error.
pub fn explain(
    conn: &Connection,
    query: &str,
    namespace: NamespaceFilter,
    state: StateFilter,
    deleted: DeletedFilter,
    valid_window: Option<ValidWindow>,
    limit: usize,
) -> Result<Vec<ExplainedHit>, MnexisError> {
    let mut sql = String::from(
        "SELECT m.uuid, bm25(memories_fts) AS rank, m.namespace, m.memory_type, m.state, \
         m.revision, m.created_at, m.content \
         FROM memories_fts \
         JOIN memories AS m ON memories_fts.rowid = m.id \
         WHERE memories_fts MATCH ?1",
    );
    let mut params: Vec<&dyn ToSql> = vec![&query];

    match state {
        StateFilter::ActiveOnly => sql.push_str(" AND m.state = 'active'"),
        StateFilter::Any => {}
    }
    match deleted {
        DeletedFilter::NotDeletedOnly => sql.push_str(" AND m.deleted = 0"),
        DeletedFilter::Any => {}
    }
    match &namespace {
        NamespaceFilter::Any => {}
        NamespaceFilter::Namespace(ns) => {
            sql.push_str(" AND m.namespace = ?");
            params.push(ns);
        }
    }
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

    // Pre-compute query tokens (FTS5 normalises by lower-casing). The
    // matched_terms for each hit are the subset of these tokens that
    // appear in the row's content (also lower-cased).
    let query_tokens: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| MnexisError::busy(format!("fts5::explain prepare: {e}")))?;
    let mut rows = stmt
        .query(&params[..])
        .map_err(|e| MnexisError::busy(format!("fts5::explain query: {e}")))?;

    let mut hits: Vec<ExplainedHit> = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| MnexisError::busy(format!("fts5::explain next: {e}")))?
    {
        let uuid: String = row
            .get(0)
            .map_err(|e| MnexisError::busy(format!("fts5::explain uuid: {e}")))?;
        let rank: f64 = row
            .get(1)
            .map_err(|e| MnexisError::busy(format!("fts5::explain rank: {e}")))?;
        let namespace_val: String = row
            .get(2)
            .map_err(|e| MnexisError::busy(format!("fts5::explain namespace: {e}")))?;
        let memory_type: String = row
            .get(3)
            .map_err(|e| MnexisError::busy(format!("fts5::explain memory_type: {e}")))?;
        let state_val: String = row
            .get(4)
            .map_err(|e| MnexisError::busy(format!("fts5::explain state: {e}")))?;
        let revision: i32 = row
            .get(5)
            .map_err(|e| MnexisError::busy(format!("fts5::explain revision: {e}")))?;
        let created_at: String = row
            .get(6)
            .map_err(|e| MnexisError::busy(format!("fts5::explain created_at: {e}")))?;
        let content: String = row
            .get(7)
            .map_err(|e| MnexisError::busy(format!("fts5::explain content: {e}")))?;

        let content_lower = content.to_lowercase();
        let matched_terms: Vec<String> = query_tokens
            .iter()
            .filter(|t| content_lower.split_whitespace().any(|w| w == t.as_str()))
            .cloned()
            .collect();

        hits.push(ExplainedHit {
            uuid,
            rank,
            namespace: namespace_val,
            memory_type,
            state: state_val,
            revision,
            created_at,
            matched_terms,
        });
    }
    Ok(hits)
}
