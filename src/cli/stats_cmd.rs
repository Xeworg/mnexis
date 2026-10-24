//! `mnexis stats` — store-level counts for the operator.
//!
//! One read-only answer over the whole data dir: what each namespace holds
//! (active, superseded, soft-deleted memories), the totals (memories,
//! supersede links, sessions, prompts), and the schema version the store is
//! at. It opens its own connection (the doctor pattern) and never goes
//! through the scope: this is the operator looking at the store, not an
//! agent reading memories.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::cli::output::CommandOutcome;
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;

/// What one namespace holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NamespaceStats {
    pub namespace: String,
    pub active: i64,
    pub superseded: i64,
    pub soft_deleted: i64,
}

/// Whole-store totals.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TotalsStats {
    pub memories: i64,
    pub active: i64,
    pub superseded: i64,
    pub soft_deleted: i64,
    pub supersede_links: i64,
    pub sessions: i64,
    pub prompts: i64,
}

/// The `mnexis stats` answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StatsReport {
    pub schema_version: i64,
    pub namespaces: Vec<NamespaceStats>,
    pub totals: TotalsStats,
}

fn count(conn: &Connection, sql: &str) -> Result<i64, MnexisError> {
    conn.query_row(sql, [], |r| r.get(0))
        .map_err(|e| MnexisError::busy(format!("stats: {e}")))
}

/// Collect the counts from the store.
pub fn collect(conn: &Connection) -> Result<StatsReport, MnexisError> {
    let schema_version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(|e| MnexisError::busy(format!("stats schema version: {e}")))?;

    let mut stmt = conn
        .prepare(
            "SELECT namespace, \
                    COALESCE(SUM(state = 'active' AND deleted = 0), 0), \
                    COALESCE(SUM(state = 'superseded' AND deleted = 0), 0), \
                    COALESCE(SUM(deleted = 1), 0) \
             FROM memories GROUP BY namespace ORDER BY namespace",
        )
        .map_err(|e| MnexisError::busy(format!("stats namespaces: {e}")))?;
    let namespaces = stmt
        .query_map([], |row| {
            Ok(NamespaceStats {
                namespace: row.get(0)?,
                active: row.get(1)?,
                superseded: row.get(2)?,
                soft_deleted: row.get(3)?,
            })
        })
        .map_err(|e| MnexisError::busy(format!("stats namespaces: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MnexisError::busy(format!("stats namespaces: {e}")))?;

    let mut totals = TotalsStats {
        supersede_links: count(conn, "SELECT count(*) FROM memory_links")?,
        sessions: count(conn, "SELECT count(*) FROM sessions")?,
        prompts: count(conn, "SELECT count(*) FROM user_prompts")?,
        ..TotalsStats::default()
    };
    for ns in &namespaces {
        totals.active += ns.active;
        totals.superseded += ns.superseded;
        totals.soft_deleted += ns.soft_deleted;
    }
    totals.memories = totals.active + totals.superseded + totals.soft_deleted;

    Ok(StatsReport {
        schema_version,
        namespaces,
        totals,
    })
}

/// Text rendering: a compact per-namespace table and a totals line.
pub fn render(report: &StatsReport) -> String {
    let mut lines = Vec::new();
    if report.namespaces.is_empty() {
        lines.push("no memories in this store".to_string());
    } else {
        lines.push(format!(
            "{:<24} {:>7} {:>11} {:>8}",
            "namespace", "active", "superseded", "deleted"
        ));
        for ns in &report.namespaces {
            lines.push(format!(
                "{:<24} {:>7} {:>11} {:>8}",
                ns.namespace, ns.active, ns.superseded, ns.soft_deleted
            ));
        }
    }
    lines.push(format!(
        "totals: {} memories ({} active, {} superseded, {} soft-deleted), \
         {} supersede links, {} sessions, {} prompts, schema {}",
        report.totals.memories,
        report.totals.active,
        report.totals.superseded,
        report.totals.soft_deleted,
        report.totals.supersede_links,
        report.totals.sessions,
        report.totals.prompts,
        report.schema_version,
    ));
    lines.join("\n")
}

/// `mnexis stats` — the whole store in one answer.
pub fn stats(profile: &Profile) -> Result<CommandOutcome, MnexisError> {
    let conn = crate::storage::sqlite::open_sqlite(&profile.db_path())?;
    let report = collect(&conn)?;
    let json = serde_json::to_value(&report)
        .map_err(|e| MnexisError::busy(format!("serialize stats report: {e}")))?;
    Ok(CommandOutcome::new(json, render(&report)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_renders_a_table_and_a_totals_line() {
        let report = StatsReport {
            schema_version: 7,
            namespaces: vec![NamespaceStats {
                namespace: "default".into(),
                active: 2,
                superseded: 1,
                soft_deleted: 1,
            }],
            totals: TotalsStats {
                memories: 4,
                active: 2,
                superseded: 1,
                soft_deleted: 1,
                supersede_links: 1,
                sessions: 1,
                prompts: 1,
            },
        };
        let text = render(&report);
        assert!(text.contains("default"), "{text}");
        assert!(text.contains("4 memories"), "{text}");
        assert!(text.contains("schema 7"), "{text}");
    }
}
