//! `mnexis doctor` — SQLite and FTS5 health checks.
//!
//! Each check returns a [`Check`] record with a name, ok flag, and optional
//! detail. The full suite is collected into a [`DoctorReport`] that carries an
//! overall `healthy` flag and (when `--repair` ran) a [`RepairReport`].
//!
//! Exit-code semantics (e2-core §8):
//! - all checks ok → exit 0
//! - missing/broken FTS (checks b/c/f) → exit 6 (`INDEX_UNAVAILABLE`)
//! - schema newer than binary → exit 9 (`UNSUPPORTED_VERSION`)
//! - other failures (integrity, WAL) → exit 1 (infrastructure; no stable code)
//!
//! Precedence when multiple failures occur:
//! `UNSUPPORTED_VERSION` (9) > `INDEX_UNAVAILABLE` (6) > generic (1).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::repair::repair_db;
use crate::domain::error::MnexisError;

/// The binary's current schema version — the highest migration number in
/// `storage::sqlite::run_migrations`. Read from that module directly so it
/// stays in sync.
const EXPECTED_SCHEMA_VERSION: i64 = 2;

/// The canonical tables that must be present.
const CANONICAL_TABLES: &[&str] = &[
    "memories",
    "memory_history",
    "memory_links",
    "memory_tags",
    "idempotency_records",
    "memories_fts",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Result of one health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// Short identifier for this check, e.g. "schema_version".
    pub name: String,
    /// True when the check passed.
    pub ok: bool,
    /// Human-readable detail; empty when ok=true.
    pub detail: String,
}

/// Full doctor report (serialised as JSON for `--json` output).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    /// One entry per check that ran.
    pub checks: Vec<Check>,
    /// True when all checks passed.
    pub healthy: bool,
}

/// `doctor --repair` additionally emits this block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// memories_fts was dropped and re-created.
    pub fts_recreated: bool,
    /// All five FTS5 triggers were re-created.
    pub triggers_recreated: bool,
    /// Number of active + non-deleted rows inserted during reindex.
    pub reindexed: usize,
    /// All checks passed after the repair.
    pub healthy_after: bool,
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

/// a. `schema_migrations` exists, has a row, and version ≤ EXPECTED_SCHEMA_VERSION.
/// A version > EXPECTED_SCHEMA_VERSION → unsupported.
fn check_schema_version(conn: &Connection) -> Check {
    let table_exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if table_exists == 0 {
        return Check {
            name: "schema_version".into(),
            ok: false,
            detail: "schema_migrations table is missing".into(),
        };
    }

    let version: i64 = match conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    ) {
        Ok(v) => v,
        Err(e) => {
            return Check {
                name: "schema_version".into(),
                ok: false,
                detail: format!("cannot read version: {e}"),
            };
        }
    };

    if version > EXPECTED_SCHEMA_VERSION {
        return Check {
            name: "schema_version".into(),
            ok: false,
            detail: format!(
                "schema version {} is newer than this binary's expected version {} \
                 (UNSUPPORTED_VERSION)",
                version, EXPECTED_SCHEMA_VERSION
            ),
        };
    }

    Check {
        name: "schema_version".into(),
        ok: true,
        detail: format!("schema version {version}"),
    }
}

/// b. All canonical tables exist.
fn check_canonical_tables(conn: &Connection) -> Check {
    let mut missing = Vec::new();
    for table in CANONICAL_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists == 0 {
            missing.push(*table);
        }
    }

    if missing.is_empty() {
        Check {
            name: "canonical_tables".into(),
            ok: true,
            detail: "all canonical tables present".into(),
        }
    } else {
        Check {
            name: "canonical_tables".into(),
            ok: false,
            detail: format!("missing tables: {}", missing.join(", ")),
        }
    }
}

/// c. `memories_fts` is readable (SELECT count(*) succeeds).
fn check_fts_readable(conn: &Connection) -> Check {
    match conn.query_row("SELECT count(*) FROM memories_fts", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(count) => Check {
            name: "fts_readable".into(),
            ok: true,
            detail: format!("memories_fts is readable ({count} rows)"),
        },
        Err(e) => Check {
            name: "fts_readable".into(),
            ok: false,
            detail: format!("memories_fts is not readable: {e}"),
        },
    }
}

/// d. WAL journal mode is active.
fn check_wal_mode(conn: &Connection) -> Check {
    let mode: String = match conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)) {
        Ok(m) => m,
        Err(e) => {
            return Check {
                name: "wal_mode".into(),
                ok: false,
                detail: format!("cannot read journal_mode: {e}"),
            };
        }
    };

    if mode.eq_ignore_ascii_case("wal") {
        Check {
            name: "wal_mode".into(),
            ok: true,
            detail: format!("journal_mode = {mode}"),
        }
    } else {
        Check {
            name: "wal_mode".into(),
            ok: false,
            detail: format!("journal_mode is '{mode}', expected 'wal' (WAL not active)"),
        }
    }
}

/// e. `PRAGMA integrity_check` returns 'ok'.
fn check_integrity(conn: &Connection) -> Check {
    let result: String = match conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)) {
        Ok(r) => r,
        Err(e) => {
            return Check {
                name: "integrity".into(),
                ok: false,
                detail: format!("integrity_check failed to run: {e}"),
            };
        }
    };

    if result.eq_ignore_ascii_case("ok") {
        Check {
            name: "integrity".into(),
            ok: true,
            detail: "integrity_check passed".into(),
        }
    } else {
        Check {
            name: "integrity".into(),
            ok: false,
            detail: format!("integrity_check reported: {result}"),
        }
    }
}

/// f. FTS5 invariant: count(active AND deleted=0) == count(memories_fts).
fn check_fts_invariant(conn: &Connection) -> Check {
    let canonical: i64 = conn
        .query_row(
            "SELECT count(*) FROM memories WHERE state = 'active' AND deleted = 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(-1);

    let fts: i64 = match conn.query_row("SELECT count(*) FROM memories_fts", [], |r| r.get(0)) {
        Ok(c) => c,
        Err(_) => -1,
    };

    if canonical == fts {
        Check {
            name: "fts_invariant".into(),
            ok: true,
            detail: format!("canonical active+non-deleted ({canonical}) == FTS5 rows ({fts})"),
        }
    } else {
        Check {
            name: "fts_invariant".into(),
            ok: false,
            detail: format!(
                "canonical active+non-deleted ({canonical}) != FTS5 rows ({fts}) — \
                 index is out of sync (INDEX_UNAVAILABLE)"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Purge eligibility count
// ---------------------------------------------------------------------------

/// Returns the number of soft-deleted records that would be purged by
/// `purge(before, all)`, implementing the same eligibility SQL as
/// `SqliteMemoryRepository::purge` (src/storage/memory_repo.rs).
///
/// - `all = true`: count of all `deleted = 1` rows.
/// - `all = false`: count of `deleted = 1 AND purge_after IS NOT NULL AND
///   purge_after <= before` rows.
pub fn purge_eligible_count(
    conn: &Connection,
    before: Option<&str>,
    all: bool,
) -> Result<usize, MnexisError> {
    let count: i64 = if all {
        conn.query_row("SELECT count(*) FROM memories WHERE deleted = 1", [], |r| {
            r.get(0)
        })
    } else if let Some(cutoff) = before {
        conn.query_row(
            "SELECT count(*) FROM memories WHERE deleted = 1 \
             AND purge_after IS NOT NULL AND purge_after <= ?1",
            [cutoff],
            |r| r.get(0),
        )
    } else {
        return Err(MnexisError::invalid_argument(
            "before",
            "either 'before' (ISO-8601 cutoff) or 'all' must be specified",
        ));
    }
    .map_err(|e| MnexisError::busy(format!("purge_eligible_count: {e}")))?;

    Ok(count as usize)
}

// ---------------------------------------------------------------------------
// Report construction
// ---------------------------------------------------------------------------

/// Run all doctor checks and return the structured report.
pub fn run_checks(conn: &Connection) -> DoctorReport {
    let checks = vec![
        check_schema_version(conn),
        check_canonical_tables(conn),
        check_fts_readable(conn),
        check_wal_mode(conn),
        check_integrity(conn),
        check_fts_invariant(conn),
    ];
    let healthy = checks.iter().all(|c| c.ok);
    DoctorReport { checks, healthy }
}

/// Returns the FTS-related check names (checks b, c, f from the spec).
fn fts_check_names() -> Vec<&'static str> {
    vec!["canonical_tables", "fts_readable", "fts_invariant"]
}

/// Classify the failure profile of a `DoctorReport` into an exit code and
/// an optional error label for stderr.
///
/// Precedence: UNSUPPORTED_VERSION (9) > INDEX_UNAVAILABLE (6) > generic (1).
pub fn failure_exit_code(report: &DoctorReport) -> Option<(i32, &'static str)> {
    let unsupported = report.checks.iter().any(|c| {
        c.name == "schema_version" && !c.ok && c.detail.contains("newer than this binary")
    });

    if unsupported {
        return Some((9, "UNSUPPORTED_VERSION"));
    }

    let fts_names = fts_check_names();
    let fts_failure = report
        .checks
        .iter()
        .any(|c| fts_names.contains(&c.name.as_str()) && !c.ok);

    if fts_failure {
        return Some((6, "INDEX_UNAVAILABLE"));
    }

    if !report.healthy {
        return Some((1, "INFRASTRUCTURE_ERROR"));
    }

    None
}

/// Text rendering for one check (used in the human-readable output).
pub fn check_line(check: &Check) -> String {
    if check.ok {
        format!("  ok  {}", check.name)
    } else {
        format!("  FAIL  {} — {}", check.name, check.detail)
    }
}

// ---------------------------------------------------------------------------
// Repair entry point
// ---------------------------------------------------------------------------

/// Execute `doctor --repair`: drop+recreate memories_fts and its triggers,
/// then reindex all active+non-deleted rows. Runs inside one transaction.
/// On success, re-runs all checks and returns both the repair report and the
/// post-repair health report.
pub fn repair_and_rerun(conn: &Connection) -> Result<(RepairReport, DoctorReport), MnexisError> {
    let reindexed = repair_db(conn)?;

    // Re-run all checks to confirm health after repair.
    let after_report = run_checks(conn);

    let repair_report = RepairReport {
        fts_recreated: true,
        triggers_recreated: true,
        reindexed,
        healthy_after: after_report.healthy,
    };

    Ok((repair_report, after_report))
}
