//! The prompts: what the user said, and how it is found again.
//!
//! The same contract as the memories' search: the caller's words are
//! compiled into a safe expression before they reach the index, so a
//! hyphen is part of a word and a multi-word query counts every word.

use rusqlite::{Connection, ToSql};

use crate::domain::error::MnexisError;
use crate::domain::query::MatchMode;
use crate::domain::session::UserPrompt;

use super::fts5::NamespaceFilter;

/// A prompt that matched a search.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptHit {
    /// The prompt's row id.
    pub id: i64,
    /// The session the prompt belongs to, when there is one.
    pub session_id: Option<String>,
    /// The namespace the prompt lives in.
    pub namespace: String,
    /// What the user said.
    pub content: String,
    /// When the prompt was captured.
    pub created_at: String,
    /// The rank the index gave the hit (bm25; more negative is better).
    pub rank: f64,
}

/// Ranked search over the prompts.
pub fn search_prompts(
    conn: &Connection,
    query: &str,
    mode: MatchMode,
    namespace: NamespaceFilter,
    limit: usize,
) -> Result<Vec<PromptHit>, MnexisError> {
    // The same compilation as the memories' search: a term can never be
    // read as FTS5 syntax.
    let expression = crate::domain::query::compile(query, mode)?;
    let mut sql = String::from(
        "SELECT p.id, p.session_id, p.namespace, p.content, p.created_at, \
         bm25(user_prompts_fts) AS rank \
         FROM user_prompts_fts \
         JOIN user_prompts AS p ON user_prompts_fts.rowid = p.id \
         WHERE user_prompts_fts MATCH ?1",
    );
    let mut params: Vec<&dyn ToSql> = vec![&expression];

    match &namespace {
        NamespaceFilter::Any => {}
        NamespaceFilter::Namespace(ns) => {
            sql.push_str(" AND p.namespace = ?");
            params.push(ns);
        }
        NamespaceFilter::AnyExcept(list) => {
            if !list.is_empty() {
                sql.push_str(" AND p.namespace NOT IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                for ns in list {
                    params.push(ns);
                }
            }
        }
        NamespaceFilter::Many(list) => {
            if list.is_empty() {
                sql.push_str(" AND 0");
            } else {
                sql.push_str(" AND p.namespace IN (");
                sql.push_str(&vec!["?"; list.len()].join(", "));
                sql.push(')');
                for ns in list {
                    params.push(ns);
                }
            }
        }
    }

    sql.push_str(" ORDER BY rank ASC LIMIT ?");
    let limit_i64: i64 = limit as i64;
    params.push(&limit_i64);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| MnexisError::busy(format!("prompts::search prepare: {e}")))?;
    let rows = stmt
        .query_map(&params[..], |row| {
            Ok(PromptHit {
                id: row.get(0)?,
                session_id: row.get(1)?,
                namespace: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                rank: row.get(5)?,
            })
        })
        .map_err(|e| MnexisError::busy(format!("prompts::search query_map: {e}")))?;
    let hits = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MnexisError::busy(format!("prompts::search collect: {e}")))?;
    Ok(hits)
}

/// Turn a search hit into the prompt it names.
pub fn prompt_from_hit(hit: &PromptHit) -> UserPrompt {
    UserPrompt {
        id: hit.id,
        session_id: hit.session_id.clone(),
        namespace: hit.namespace.clone(),
        content: hit.content.clone(),
        created_at: hit.created_at.clone(),
    }
}
