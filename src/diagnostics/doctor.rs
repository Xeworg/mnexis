//! `mnexis doctor` — SQLite and FTS5 health checks.
//!
//! Each check returns a [`Check`] record carrying a stable [`CheckId`], a
//! [`Severity`], a machine [`ReasonCode`] when it failed, the `evidence`
//! string, and the `safe_next_step` an operator can run. The full suite is
//! collected into a [`DoctorReport`] with a top-level [`Status`] and a
//! one-line `summary` (when `doctor repair` ran, additionally a
//! [`RepairReport`]).
//!
//! Exit-code semantics (e2-core §8) are a contract and predate the
//! envelope — they do not change with it:
//! - all checks ok → exit 0
//! - missing/broken/stale FTS (canonical_tables/fts_readable/fts_shape/
//!   fts_invariant) → exit 6 (`INDEX_UNAVAILABLE`)
//! - schema newer than binary → exit 9 (`UNSUPPORTED_VERSION`)
//! - other failures (integrity, WAL) → exit 1 (infrastructure)
//!
//! Precedence when multiple failures occur:
//! `UNSUPPORTED_VERSION` (9) > `INDEX_UNAVAILABLE` (6) > generic (1).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::domain::error::MnexisError;
use crate::storage::sqlite::{MEMORIES_FTS_COLUMNS, missing_fts_columns, rebuild_memories_fts};

/// The binary's current schema version — the highest migration number in
/// `storage::sqlite::run_migrations`. Read from that module directly so it
/// stays in sync.
const EXPECTED_SCHEMA_VERSION: i64 = 7;

/// The canonical tables that must be present.
const CANONICAL_TABLES: &[&str] = &[
    "memories",
    "memory_history",
    "memory_links",
    "memory_tags",
    "idempotency_records",
    "memories_fts",
    "sessions",
    "user_prompts",
    "user_prompts_fts",
];

// ---------------------------------------------------------------------------
// Data types — the stable machine contract
// ---------------------------------------------------------------------------

/// Stable identifier of one health check. This is the `check_id` every
/// consumer pins on and the CODE the `--check` filter accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    SchemaVersion,
    CanonicalTables,
    FtsReadable,
    WalMode,
    Integrity,
    FtsShape,
    FtsInvariant,
}

impl CheckId {
    /// The wire/string form (matches the serde serialization).
    pub fn as_str(self) -> &'static str {
        match self {
            CheckId::SchemaVersion => "schema_version",
            CheckId::CanonicalTables => "canonical_tables",
            CheckId::FtsReadable => "fts_readable",
            CheckId::WalMode => "wal_mode",
            CheckId::Integrity => "integrity",
            CheckId::FtsShape => "fts_shape",
            CheckId::FtsInvariant => "fts_invariant",
        }
    }
}

/// Severity of one check result. `warn` is part of the contract for checks
/// that do not fail the run; every failure today is `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ok,
    Warn,
    Error,
}

/// Top-level status of the store, derived from the failing checks:
/// - `healthy` — every check passed.
/// - `degraded` — failures that do not block correct service (WAL family).
/// - `failed` — the store cannot serve correctly (schema, canonical tables,
///   FTS family, integrity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Healthy,
    Degraded,
    Failed,
}

/// Machine-readable failure code, one per distinct failure mode. These are
/// the codes an operator or a script matches on — they never change wording
/// once published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    SchemaMigrationsMissing,
    SchemaUnreadable,
    SchemaNewerThanBinary,
    CanonicalTablesMissing,
    FtsMissing,
    FtsUnreadable,
    WalUnreadable,
    WalNotActive,
    IntegrityUnreadable,
    IntegrityFailed,
    FtsShapeUnreadable,
    FtsStaleShape,
    FtsOutOfSync,
}

impl ReasonCode {
    /// The wire/string form (matches the serde serialization).
    pub fn as_str(self) -> &'static str {
        match self {
            ReasonCode::SchemaMigrationsMissing => "SCHEMA_MIGRATIONS_MISSING",
            ReasonCode::SchemaUnreadable => "SCHEMA_UNREADABLE",
            ReasonCode::SchemaNewerThanBinary => "SCHEMA_NEWER_THAN_BINARY",
            ReasonCode::CanonicalTablesMissing => "CANONICAL_TABLES_MISSING",
            ReasonCode::FtsMissing => "FTS_MISSING",
            ReasonCode::FtsUnreadable => "FTS_UNREADABLE",
            ReasonCode::WalUnreadable => "WAL_UNREADABLE",
            ReasonCode::WalNotActive => "WAL_NOT_ACTIVE",
            ReasonCode::IntegrityUnreadable => "INTEGRITY_UNREADABLE",
            ReasonCode::IntegrityFailed => "INTEGRITY_FAILED",
            ReasonCode::FtsShapeUnreadable => "FTS_SHAPE_UNREADABLE",
            ReasonCode::FtsStaleShape => "FTS_STALE_SHAPE",
            ReasonCode::FtsOutOfSync => "FTS_OUT_OF_SYNC",
        }
    }

    /// The one safe, specific action an operator can take next. Static per
    /// code so scripts can rely on it.
    pub fn safe_next_step(self) -> &'static str {
        match self {
            ReasonCode::SchemaMigrationsMissing | ReasonCode::SchemaUnreadable => {
                "point --data-dir at an initialized store, or run `mnexis init` in this one"
            }
            ReasonCode::SchemaNewerThanBinary => {
                "upgrade mnexis: this binary cannot open a store with a newer schema"
            }
            ReasonCode::CanonicalTablesMissing => {
                "restore the data directory from a backup; canonical tables are not recreatable"
            }
            ReasonCode::FtsMissing
            | ReasonCode::FtsUnreadable
            | ReasonCode::FtsShapeUnreadable
            | ReasonCode::FtsStaleShape
            | ReasonCode::FtsOutOfSync => {
                "run `mnexis doctor repair --apply` (rebuilds the index from canonical data)"
            }
            ReasonCode::WalUnreadable | ReasonCode::WalNotActive => {
                "run `mnexis doctor repair --apply` (re-activates WAL journaling)"
            }
            ReasonCode::IntegrityUnreadable | ReasonCode::IntegrityFailed => {
                "restore from an export or backup; a failed integrity check is not repairable in place"
            }
        }
    }

    /// True when a failure with this code blocks correct service (search
    /// misses or refuses) — `status: failed`, not merely degraded. The WAL
    /// family is the exception: the store still serves correctly without it.
    fn blocks_service(self) -> bool {
        !matches!(self, ReasonCode::WalUnreadable | ReasonCode::WalNotActive)
    }
}

/// Result of one health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Check {
    /// The stable check identity, e.g. "schema_version".
    pub check_id: CheckId,
    /// True when the check passed.
    pub ok: bool,
    /// ok | warn | error.
    pub severity: Severity,
    /// Machine failure code; `null` when the check passed.
    pub reason_code: Option<ReasonCode>,
    /// What the check observed; non-empty always.
    pub evidence: String,
    /// The safe next step; `null` when the check passed.
    pub safe_next_step: Option<String>,
}

impl Check {
    /// A passing check: no code, no next step, severity ok.
    fn pass(check_id: CheckId, evidence: String) -> Self {
        Check {
            check_id,
            ok: true,
            severity: Severity::Ok,
            reason_code: None,
            evidence,
            safe_next_step: None,
        }
    }

    /// A failing check: severity error today (no warn-level checks exist
    /// yet), the code, the evidence, and the code's safe next step.
    fn fail(check_id: CheckId, reason: ReasonCode, evidence: String) -> Self {
        Check {
            check_id,
            ok: false,
            severity: Severity::Error,
            reason_code: Some(reason),
            evidence,
            safe_next_step: Some(reason.safe_next_step().to_string()),
        }
    }
}

/// Full doctor report (serialised as JSON for `--json` output).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DoctorReport {
    /// healthy | degraded | failed.
    pub status: Status,
    /// One line: "all 7 checks passed" or "N of 7 checks failed: ...".
    pub summary: String,
    /// One entry per check that ran.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// True when every check passed.
    pub fn is_healthy(&self) -> bool {
        self.status == Status::Healthy
    }
}

/// `doctor repair` additionally emits this block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
        return Check::fail(
            CheckId::SchemaVersion,
            ReasonCode::SchemaMigrationsMissing,
            "schema_migrations table is missing".into(),
        );
    }

    let version: i64 = match conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    ) {
        Ok(v) => v,
        Err(e) => {
            return Check::fail(
                CheckId::SchemaVersion,
                ReasonCode::SchemaUnreadable,
                format!("cannot read version: {e}"),
            );
        }
    };

    if version > EXPECTED_SCHEMA_VERSION {
        return Check::fail(
            CheckId::SchemaVersion,
            ReasonCode::SchemaNewerThanBinary,
            format!(
                "schema version {} is newer than this binary's expected version {} \
                 (UNSUPPORTED_VERSION)",
                version, EXPECTED_SCHEMA_VERSION
            ),
        );
    }

    Check::pass(CheckId::SchemaVersion, format!("schema version {version}"))
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
        Check::pass(
            CheckId::CanonicalTables,
            "all canonical tables present".into(),
        )
    } else {
        Check::fail(
            CheckId::CanonicalTables,
            ReasonCode::CanonicalTablesMissing,
            format!("missing tables: {}", missing.join(", ")),
        )
    }
}

/// c. `memories_fts` exists (FTS_MISSING) and is readable (FTS_UNREADABLE).
fn check_fts_readable(conn: &Connection) -> Check {
    let table_exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if table_exists == 0 {
        return Check::fail(
            CheckId::FtsReadable,
            ReasonCode::FtsMissing,
            "memories_fts is missing (INDEX_UNAVAILABLE)".into(),
        );
    }

    match conn.query_row("SELECT count(*) FROM memories_fts", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(count) => Check::pass(
            CheckId::FtsReadable,
            format!("memories_fts is readable ({count} rows)"),
        ),
        Err(e) => Check::fail(
            CheckId::FtsReadable,
            ReasonCode::FtsUnreadable,
            format!("memories_fts is not readable: {e}"),
        ),
    }
}

/// d. WAL journal mode is active.
fn check_wal_mode(conn: &Connection) -> Check {
    let mode: String = match conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)) {
        Ok(m) => m,
        Err(e) => {
            return Check::fail(
                CheckId::WalMode,
                ReasonCode::WalUnreadable,
                format!("cannot read journal_mode: {e}"),
            );
        }
    };

    if mode.eq_ignore_ascii_case("wal") {
        Check::pass(CheckId::WalMode, format!("journal_mode = {mode}"))
    } else {
        Check::fail(
            CheckId::WalMode,
            ReasonCode::WalNotActive,
            format!("journal_mode is '{mode}', expected 'wal' (WAL not active)"),
        )
    }
}

/// e. `PRAGMA integrity_check` returns 'ok'.
fn check_integrity(conn: &Connection) -> Check {
    let result: String = match conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)) {
        Ok(r) => r,
        Err(e) => {
            return Check::fail(
                CheckId::Integrity,
                ReasonCode::IntegrityUnreadable,
                format!("integrity_check failed to run: {e}"),
            );
        }
    };

    if result.eq_ignore_ascii_case("ok") {
        Check::pass(CheckId::Integrity, "integrity_check passed".into())
    } else {
        Check::fail(
            CheckId::Integrity,
            ReasonCode::IntegrityFailed,
            format!("integrity_check reported: {result}"),
        )
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
        Check::pass(
            CheckId::FtsInvariant,
            format!("canonical active+non-deleted ({canonical}) == FTS5 rows ({fts})"),
        )
    } else {
        Check::fail(
            CheckId::FtsInvariant,
            ReasonCode::FtsOutOfSync,
            format!(
                "canonical active+non-deleted ({canonical}) != FTS5 rows ({fts}) — \
                 index is out of sync (INDEX_UNAVAILABLE)"
            ),
        )
    }
}

/// g. `memories_fts` carries the columns the current schema indexes.
///
/// The count check above cannot tell a stale index from a good one: a
/// content-only index built by schema 005 has exactly one row per active
/// memory and passes it. The **shape** is what distinguishes them — an
/// index built without the title cannot answer `SELECT title FROM memories_fts`,
/// and one built without the words cannot answer `SELECT keywords FROM
/// memories_fts`, so neither can find a memory by them either.
fn check_fts_shape(conn: &Connection) -> Check {
    let table_exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if table_exists == 0 {
        return Check::fail(
            CheckId::FtsShape,
            ReasonCode::FtsMissing,
            "memories_fts is missing (INDEX_UNAVAILABLE)".into(),
        );
    }

    match missing_fts_columns(conn) {
        Ok(missing) if missing.is_empty() => Check::pass(
            CheckId::FtsShape,
            format!("memories_fts indexes {}", MEMORIES_FTS_COLUMNS.join(", ")),
        ),
        Ok(missing) => Check::fail(
            CheckId::FtsShape,
            ReasonCode::FtsStaleShape,
            format!(
                "memories_fts was built without {} — the index predates the current \
                 schema (007), so a search cannot find a memory by every part of it \
                 (its content, its title, its topic key or its keywords); rebuild it \
                 with `mnexis reindex` or `mnexis doctor repair --apply` (INDEX_UNAVAILABLE)",
                missing.join(", ")
            ),
        ),
        Err(e) => Check::fail(
            CheckId::FtsShape,
            ReasonCode::FtsShapeUnreadable,
            format!("cannot read the shape of memories_fts: {e}"),
        ),
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
        check_fts_shape(conn),
        check_fts_invariant(conn),
    ];
    build_report(checks)
}

/// Assemble the report from the checks that ran (all of them, or the ones a
/// `--check` filter left in place).
pub(crate) fn build_report(checks: Vec<Check>) -> DoctorReport {
    let status = if checks
        .iter()
        .any(|c| !c.ok && c.reason_code.is_some_and(|r| r.blocks_service()))
    {
        Status::Failed
    } else if checks.iter().all(|c| c.ok) {
        Status::Healthy
    } else {
        Status::Degraded
    };

    let failed: Vec<String> = checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| {
            format!(
                "{} ({})",
                c.check_id.as_str(),
                c.reason_code.map(ReasonCode::as_str).unwrap_or("FAILED")
            )
        })
        .collect();

    let summary = if failed.is_empty() {
        format!("all {} checks passed", checks.len())
    } else {
        format!(
            "{} of {} checks failed: {}",
            failed.len(),
            checks.len(),
            failed.join(", ")
        )
    };

    DoctorReport {
        status,
        summary,
        checks,
    }
}

/// The check ids whose failure makes the index unavailable (exit 6):
/// canonical_tables covers a dropped `memories_fts`, the other three are
/// the FTS family proper. Kept from the pre-envelope contract, unchanged.
fn index_unavailable_ids() -> [CheckId; 4] {
    [
        CheckId::CanonicalTables,
        CheckId::FtsReadable,
        CheckId::FtsShape,
        CheckId::FtsInvariant,
    ]
}

/// Classify the failure profile of a `DoctorReport` into an exit code and
/// an optional error label for stderr.
///
/// Precedence: UNSUPPORTED_VERSION (9) > INDEX_UNAVAILABLE (6) > generic (1).
pub fn failure_exit_code(report: &DoctorReport) -> Option<(i32, &'static str)> {
    let unsupported = report
        .checks
        .iter()
        .any(|c| c.reason_code == Some(ReasonCode::SchemaNewerThanBinary));

    if unsupported {
        return Some((9, "UNSUPPORTED_VERSION"));
    }

    let fts_failure = report
        .checks
        .iter()
        .any(|c| !c.ok && index_unavailable_ids().contains(&c.check_id));

    if fts_failure {
        return Some((6, "INDEX_UNAVAILABLE"));
    }

    if !report.is_healthy() {
        return Some((1, "INFRASTRUCTURE_ERROR"));
    }

    None
}

/// Text rendering for one check (used in the human-readable output).
pub fn check_line(check: &Check) -> String {
    if check.ok {
        format!("  ok  {}", check.check_id.as_str())
    } else {
        let code = check
            .reason_code
            .map(ReasonCode::as_str)
            .unwrap_or("FAILED");
        format!(
            "  FAIL  {} — {}: {}",
            check.check_id.as_str(),
            code,
            check.evidence
        )
    }
}

// ---------------------------------------------------------------------------
// Repair entry point
// ---------------------------------------------------------------------------

/// Execute `doctor repair --apply`: rebuild `memories_fts` and its triggers
/// from the canonical schema (`storage::sqlite::MEMORIES_FTS_SCHEMA`) and
/// reindex all active+non-deleted rows, content, title, topic key and
/// keywords alike.
///
/// The rebuild is one atomic unit of work (a savepoint), so a repair that
/// fails leaves the profile as it was. On success, re-runs all checks and
/// returns both the repair report and the post-repair health report.
pub fn repair_and_rerun(conn: &Connection) -> Result<(RepairReport, DoctorReport), MnexisError> {
    let reindexed = rebuild_memories_fts(conn)?;

    // Re-run all checks to confirm health after repair.
    let after_report = run_checks(conn);

    let repair_report = RepairReport {
        fts_recreated: true,
        triggers_recreated: true,
        reindexed,
        healthy_after: after_report.is_healthy(),
    };

    Ok((repair_report, after_report))
}
