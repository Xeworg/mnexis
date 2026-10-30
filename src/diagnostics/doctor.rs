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
const EXPECTED_SCHEMA_VERSION: i64 = 8;

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
    TitleShape,
    TypeShape,
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
            CheckId::TitleShape => "title_shape",
            CheckId::TypeShape => "type_shape",
        }
    }

    /// Parse a `--check CODE` argument; `None` when the code is unknown.
    pub fn from_code(code: &str) -> Option<CheckId> {
        ALL_CHECK_IDS.iter().copied().find(|id| id.as_str() == code)
    }
}

/// Every check id, in the order a full run emits them.
pub const ALL_CHECK_IDS: [CheckId; 9] = [
    CheckId::SchemaVersion,
    CheckId::CanonicalTables,
    CheckId::FtsReadable,
    CheckId::WalMode,
    CheckId::Integrity,
    CheckId::FtsShape,
    CheckId::FtsInvariant,
    CheckId::TitleShape,
    CheckId::TypeShape,
];

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
    TitlesMissing,
    TitlesUnreadable,
    TypesUnclassified,
    TypesUnreadable,
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
            ReasonCode::TitlesMissing => "TITLES_MISSING",
            ReasonCode::TitlesUnreadable => "TITLES_UNREADABLE",
            ReasonCode::TypesUnclassified => "TYPES_UNCLASSIFIED",
            ReasonCode::TypesUnreadable => "TYPES_UNREADABLE",
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
            ReasonCode::TitlesMissing => {
                "run `mnexis doctor repair --plan` to see how the missing titles would be filled \
                 from each record's own text"
            }
            ReasonCode::TitlesUnreadable => {
                "restore the data directory from a backup; the records cannot be read"
            }
            ReasonCode::TypesUnclassified => {
                "give each record a kind with `mnexis update <uuid> --type <kind>` — a type is \
                 the writer's declaration, and the engine never invents one"
            }
            ReasonCode::TypesUnreadable => {
                "restore the data directory from a backup; the records cannot be read"
            }
        }
    }

    /// True when a failure with this code blocks correct service (search
    /// misses or refuses) — `status: failed`, not merely degraded. The WAL
    /// family is the exception: the store still serves correctly without it.
    /// The title family is the other one: a record without a title is read
    /// and answered, it is just not named — and a doctor that cannot read
    /// the titles is blind, not broken. The type family follows it: a
    /// record nobody classified is read and answered exactly like a typed
    /// one, and a doctor that cannot count them is blind, not broken.
    fn blocks_service(self) -> bool {
        !matches!(
            self,
            ReasonCode::WalUnreadable
                | ReasonCode::WalNotActive
                | ReasonCode::TitlesMissing
                | ReasonCode::TitlesUnreadable
                | ReasonCode::TypesUnclassified
                | ReasonCode::TypesUnreadable
        )
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

    /// A warning: the gap is real and it does not break the store.
    ///
    /// The store still serves correctly — a record without a title is
    /// read, it is just not named — so the run does not fail on it. The
    /// check says what it saw and what to do about it.
    fn warn(check_id: CheckId, reason: ReasonCode, evidence: String) -> Self {
        Check {
            check_id,
            ok: false,
            severity: Severity::Warn,
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
    /// One line: "all N checks passed", "N of M checks failed: ..." or
    /// "K of M checks warned: ...".
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

/// `doctor repair` emits this block. One type for all three modes — the
/// `mode` field says which ran.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RepairReport {
    /// plan | dry_run | apply.
    pub mode: RepairMode,
    /// The failing checks this repair fixes, and how.
    pub actions: Vec<PlannedAction>,
    /// The failing checks no repair can fix, with their safe next step.
    pub skipped: Vec<Check>,
    /// memories_fts was dropped and re-created.
    pub fts_recreated: bool,
    /// All five FTS5 triggers were re-created.
    pub triggers_recreated: bool,
    /// Number of active + non-deleted rows inserted during reindex.
    pub reindexed: usize,
    /// `PRAGMA journal_mode=WAL` was executed (apply only — never a dry-run).
    pub wal_reactivated: bool,
    /// How many records got the title their own text yields (dry-run:
    /// measured; apply: written).
    pub titles_derived: usize,
    /// The records left without a title: their text yields none, or the
    /// key their title would give is already another record's.
    pub titles_undeducible: Vec<String>,
    /// All checks passed after this command ran.
    pub healthy_after: bool,
}

/// What `doctor repair` does to one failing check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    /// Drop and re-create `memories_fts`, its triggers, and reindex every
    /// active row (fixes the whole FTS family and a missing
    /// `memories_fts` table).
    #[serde(rename = "rebuild_memories_fts")]
    RebuildFts,
    /// `PRAGMA journal_mode=WAL` (instant, no data is rewritten).
    ReactivateWal,
    /// Fill the missing titles from each record's own text (writes to the
    /// records; the fact does not change, only its missing identity does).
    #[serde(rename = "derive_titles")]
    DeriveTitles,
}

impl RepairAction {
    /// The wire/string form.
    pub fn as_str(self) -> &'static str {
        match self {
            RepairAction::RebuildFts => "rebuild_memories_fts",
            RepairAction::ReactivateWal => "reactivate_wal",
            RepairAction::DeriveTitles => "derive_titles",
        }
    }
}

/// One planned repair: which check, why, and the action that fixes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlannedAction {
    pub check_id: CheckId,
    pub reason_code: ReasonCode,
    pub action: RepairAction,
}

/// The mode `doctor repair` ran in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairMode {
    /// Read-only: what would be repaired (the default).
    Plan,
    /// The repair runs inside a savepoint that is rolled back: the counts
    /// are real, nothing is written. The WAL pragma is listed, never run.
    DryRun,
    /// The repair for real.
    Apply,
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
        let mut check = Check::fail(
            CheckId::CanonicalTables,
            ReasonCode::CanonicalTablesMissing,
            format!("missing tables: {}", missing.join(", ")),
        );
        // Contextual next step: when the only missing table is the FTS
        // index, the rebuild recreates it — telling the operator to restore
        // a backup would be wrong. Anything else really is canonical damage.
        if missing.len() == 1 && missing[0] == "memories_fts" {
            check.safe_next_step =
                Some("run `mnexis doctor repair --apply` (recreates memories_fts)".into());
        }
        check
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

    let fts: i64 = conn
        .query_row("SELECT count(*) FROM memories_fts", [], |r| r.get(0))
        .unwrap_or(-1);

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

/// h. Every record that is not deleted carries a title.
///
/// The title is what the fact IS, and every priced answer names the fact
/// with it. The records that predate the identity work carry no title at
/// all, so their answers carry an identity of nothing: a uuid, a
/// namespace and a preview, with nothing naming the fact.
///
/// This is a warning, not a failure: the store serves correctly — a
/// record without a title is read, it is just not named — so the run
/// does not fail on it. The gap is the debt the identity work left
/// behind, and this check is the one that sees it.
fn check_title_shape(conn: &Connection) -> Check {
    let mut statement = match conn.prepare(
        "SELECT uuid FROM memories WHERE deleted = 0 AND trim(title) = '' \
         ORDER BY created_at, uuid",
    ) {
        Ok(statement) => statement,
        Err(error) => {
            return Check::fail(
                CheckId::TitleShape,
                ReasonCode::TitlesUnreadable,
                format!("cannot read the titles: {error}"),
            );
        }
    };
    let mut uuids: Vec<String> = Vec::new();
    match statement.query_map([], |row| row.get::<_, String>(0)) {
        Ok(rows) => {
            for row in rows.flatten() {
                uuids.push(row);
            }
        }
        Err(error) => {
            return Check::fail(
                CheckId::TitleShape,
                ReasonCode::TitlesUnreadable,
                format!("cannot read the titles: {error}"),
            );
        }
    }
    if uuids.is_empty() {
        return Check::pass(
            CheckId::TitleShape,
            "every record carries a title".to_string(),
        );
    }
    let noun = if uuids.len() == 1 {
        "record"
    } else {
        "records"
    };
    let shown = uuids.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    let more = uuids.len().saturating_sub(5);
    let evidence = if more > 0 {
        format!(
            "{} {noun} without a title: {shown} (+{more} more)",
            uuids.len()
        )
    } else {
        format!("{} {noun} without a title: {shown}", uuids.len())
    };
    Check::warn(CheckId::TitleShape, ReasonCode::TitlesMissing, evidence)
}

/// i. The records that declare no type, and the share of the store they
/// carry.
///
/// `fact` is the type a save lands on when it declares nothing — the
/// bucket every write inherits when the caller names no kind. It is not a
/// failure: an unclassified record is read and answered exactly like a
/// typed one. The gap is that the type axis cannot select it: a filter by
/// kind never returns it, so the structure the type promises is invisible
/// on the part of the store that carries no type at all. The doctor is the
/// only surface that can answer "how much of the store is unclassified?",
/// because every other surface reads only what it is asked for.
///
/// Read-only, and deliberately without a repair: a type is the writer's
/// declaration. The engine validates one and keeps the one already there;
/// it never infers one, because a repaired type would store a guess as a
/// fact — and the store would then claim an organisation nobody made.
///
/// This is a warning, not a failure: nothing about the records changes, so
/// the run does not fail on it. The live records are the ones that are not
/// soft-deleted (the same liveness `title_shape` reads); their content
/// bytes are measured as bytes, not characters, so a text in any script is
/// priced as the store stores it.
fn check_type_shape(conn: &Connection) -> Check {
    // The whole store's live content, the denominator of the share.
    let totals: (i64, i64) = match conn.query_row(
        "SELECT count(*), COALESCE(SUM(length(CAST(content AS BLOB))), 0) \
         FROM memories WHERE deleted = 0",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ) {
        Ok(totals) => totals,
        Err(error) => {
            return Check::fail(
                CheckId::TypeShape,
                ReasonCode::TypesUnreadable,
                format!("cannot read the store: {error}"),
            );
        }
    };

    let mut statement = match conn.prepare(
        "SELECT uuid, title, length(CAST(content AS BLOB)) FROM memories \
         WHERE deleted = 0 AND memory_type = ?1 ORDER BY created_at, uuid",
    ) {
        Ok(statement) => statement,
        Err(error) => {
            return Check::fail(
                CheckId::TypeShape,
                ReasonCode::TypesUnreadable,
                format!("cannot read the types: {error}"),
            );
        }
    };
    let mut records: Vec<(String, String, i64)> = Vec::new();
    match statement.query_map([crate::domain::memory_type::DEFAULT_MEMORY_TYPE], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) {
        Ok(rows) => {
            for row in rows.flatten() {
                records.push(row);
            }
        }
        Err(error) => {
            return Check::fail(
                CheckId::TypeShape,
                ReasonCode::TypesUnreadable,
                format!("cannot read the types: {error}"),
            );
        }
    }

    if records.is_empty() {
        // An empty store passes because it holds nothing, and the evidence
        // says exactly that: a vacuous pass is not a clean bill of health.
        let evidence = if totals.0 == 0 {
            "the store holds no live records, so nothing is unclassified (0 content bytes)"
                .to_string()
        } else {
            format!(
                "none of the {} live {} is unclassified (0 of {} content bytes)",
                totals.0,
                plural(totals.0, "record", "records"),
                totals.1
            )
        };
        return Check::pass(CheckId::TypeShape, evidence);
    }

    let unclassified_bytes: i64 = records.iter().map(|(_, _, bytes)| bytes).sum();
    // Rounded to the nearest whole percent, in integers: no float ever
    // reaches the evidence string.
    let share = if totals.1 == 0 {
        0
    } else {
        (unclassified_bytes * 100 + totals.1 / 2) / totals.1
    };
    let named: Vec<String> = records
        .iter()
        .map(|(uuid, title, bytes)| {
            let title = title.trim();
            let name = if title.is_empty() {
                "(no title)".to_string()
            } else {
                format!("\"{title}\"")
            };
            format!("{uuid} {name} {bytes} B")
        })
        .collect();
    let evidence = format!(
        "{} of {} live {} unclassified (the default type '{}'), {} of {} content bytes \
         ({share}%): {}",
        records.len(),
        totals.0,
        plural(totals.0, "record is", "records are"),
        crate::domain::memory_type::DEFAULT_MEMORY_TYPE,
        unclassified_bytes,
        totals.1,
        named.join("; ")
    );
    Check::warn(CheckId::TypeShape, ReasonCode::TypesUnclassified, evidence)
}

/// The word that agrees with a count: `singular` or `plural`.
fn plural<'a>(count: i64, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
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
    run_checks_for(conn, &ALL_CHECK_IDS)
}

/// Run only the requested checks (the `--check CODE` filter). The report
/// and its exit code reflect exactly what ran — a filtered run cannot see
/// the failures of checks it did not run.
pub fn run_checks_for(conn: &Connection, ids: &[CheckId]) -> DoctorReport {
    let checks = ids.iter().map(|id| check_for_id(conn, *id)).collect();
    build_report(checks)
}

fn check_for_id(conn: &Connection, id: CheckId) -> Check {
    match id {
        CheckId::SchemaVersion => check_schema_version(conn),
        CheckId::CanonicalTables => check_canonical_tables(conn),
        CheckId::FtsReadable => check_fts_readable(conn),
        CheckId::WalMode => check_wal_mode(conn),
        CheckId::Integrity => check_integrity(conn),
        CheckId::FtsShape => check_fts_shape(conn),
        CheckId::FtsInvariant => check_fts_invariant(conn),
        CheckId::TitleShape => check_title_shape(conn),
        CheckId::TypeShape => check_type_shape(conn),
    }
}

/// Assemble the report from the checks that ran (all of them, or the ones a
/// `--check` filter left in place).
pub(crate) fn build_report(checks: Vec<Check>) -> DoctorReport {
    let status = if checks
        .iter()
        .any(|c| !c.ok && c.reason_code.is_some_and(|r| r.blocks_service()))
    {
        Status::Failed
    } else if checks
        .iter()
        .any(|c| !c.ok && c.severity == Severity::Error)
    {
        // An error that does not block service (the WAL family): the store
        // still serves, but something is wrong with it.
        Status::Degraded
    } else {
        // Everything passed, or the only gaps are warnings: the store
        // serves correctly either way.
        Status::Healthy
    };

    let failed: Vec<String> = checks
        .iter()
        .filter(|c| !c.ok && c.severity == Severity::Error)
        .map(|c| {
            format!(
                "{} ({})",
                c.check_id.as_str(),
                c.reason_code.map(ReasonCode::as_str).unwrap_or("FAILED")
            )
        })
        .collect();
    let warned: Vec<String> = checks
        .iter()
        .filter(|c| !c.ok && c.severity == Severity::Warn)
        .map(|c| {
            format!(
                "{} ({})",
                c.check_id.as_str(),
                c.reason_code.map(ReasonCode::as_str).unwrap_or("WARNED")
            )
        })
        .collect();

    let summary = match (failed.is_empty(), warned.is_empty()) {
        (true, true) => format!("all {} checks passed", checks.len()),
        (false, true) => format!(
            "{} of {} checks failed: {}",
            failed.len(),
            checks.len(),
            failed.join(", ")
        ),
        (true, false) => format!(
            "{} of {} checks warned: {}",
            warned.len(),
            checks.len(),
            warned.join(", ")
        ),
        (false, false) => format!(
            "{} of {} checks failed: {}; {} warned: {}",
            failed.len(),
            checks.len(),
            failed.join(", "),
            warned.len(),
            warned.join(", ")
        ),
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
    // A warning does not fail the run: only an error does. A store whose
    // records carry no title still serves every read correctly, so the
    // doctor says so and exits 0.
    if !report.checks.iter().any(|c| c.severity == Severity::Error) {
        return None;
    }

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
// Repair: plan / dry-run / apply
// ---------------------------------------------------------------------------

/// Which canonical tables are missing (shared by the check and the repair
/// plan, so they can never disagree).
fn missing_canonical_tables(conn: &Connection) -> Vec<&'static str> {
    CANONICAL_TABLES
        .iter()
        .filter(|table| {
            let exists: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            exists == 0
        })
        .copied()
        .collect()
}

/// The action that repairs one failing check, or `None` when nothing safe
/// can fix it. `canonical_tables` is the subtle one: a missing
/// `memories_fts` is an index problem the rebuild recreates — any other
/// missing table is canonical damage a repair must refuse to touch.
fn action_for(conn: &Connection, check: &Check) -> Option<RepairAction> {
    match check.check_id {
        CheckId::CanonicalTables => {
            let missing = missing_canonical_tables(conn);
            if missing.len() == 1 && missing[0] == "memories_fts" {
                Some(RepairAction::RebuildFts)
            } else {
                None
            }
        }
        CheckId::FtsReadable | CheckId::FtsShape | CheckId::FtsInvariant => {
            Some(RepairAction::RebuildFts)
        }
        CheckId::WalMode => Some(RepairAction::ReactivateWal),
        CheckId::TitleShape => Some(RepairAction::DeriveTitles),
        // Nothing repairs an unclassified record: a type is the writer's
        // declaration, and a repair that guessed one would store a guess
        // as a fact. The check names the records instead.
        CheckId::TypeShape => None,
        CheckId::SchemaVersion | CheckId::Integrity => None,
    }
}

/// Split the failing checks of a report into (repairable actions,
/// unfixable checks). Passing checks are ignored.
fn plan_actions(conn: &Connection, report: &DoctorReport) -> (Vec<PlannedAction>, Vec<Check>) {
    let mut actions = Vec::new();
    let mut skipped = Vec::new();
    for check in &report.checks {
        if check.ok {
            continue;
        }
        match action_for(conn, check) {
            Some(action) => actions.push(PlannedAction {
                check_id: check.check_id,
                reason_code: check
                    .reason_code
                    .expect("a failing check carries its reason code"),
                action,
            }),
            None => skipped.push(check.clone()),
        }
    }
    (actions, skipped)
}

/// Fill the missing titles from each record's own text.
///
/// The title is what the fact IS, and a record without one is read but
/// never named. The text the record already carries yields a title — its
/// first sentence — the same thing the agent writes by hand. A record
/// whose text yields nothing is left alone and reported: a title is never
/// invented, and a title that names nothing is worse than no title.
///
/// The topic key travels with the title, exactly as the save path derives
/// it: a record with a title and no key would be a second copy of a fact
/// the engine already knows. The key is written only when the record has
/// none and no other record in the namespace holds it — merging two facts
/// is the agent's decision, not the doctor's.
///
/// The fact does not change, so the revision does not move and the
/// history keeps what it says: only the missing identity is filled.
fn derive_missing_titles(conn: &Connection) -> Result<(usize, Vec<String>), MnexisError> {
    let mut statement = conn
        .prepare(
            "SELECT id, uuid, namespace, topic_key, content FROM memories \
             WHERE deleted = 0 AND trim(title) = '' ORDER BY created_at, uuid",
        )
        .map_err(|e| MnexisError::busy(format!("read the records without a title: {e}")))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| MnexisError::busy(format!("read the records without a title: {e}")))?;

    let mut derived = 0usize;
    let mut undeducible: Vec<String> = Vec::new();
    for row in rows {
        let (id, uuid, namespace, topic_key, content) =
            row.map_err(|e| MnexisError::busy(format!("read the records without a title: {e}")))?;
        let Some(title) = crate::domain::title::derive_title_from_content(&content) else {
            undeducible.push(uuid);
            continue;
        };
        let key = crate::domain::title::key_from_title(&title);
        if key.is_empty() {
            undeducible.push(uuid);
            continue;
        };
        // The key the record already holds wins; a key another record
        // holds is not written — the unique index would refuse it, and
        // merging two facts is the agent's decision.
        let new_key = if topic_key.is_empty() {
            let held: i64 = conn
                .query_row(
                    "SELECT count(*) FROM memories \
                     WHERE namespace = ?1 AND topic_key = ?2 AND id != ?3",
                    rusqlite::params![namespace, key, id],
                    |row| row.get(0),
                )
                .map_err(|e| MnexisError::busy(format!("read the topic keys: {e}")))?;
            if held > 0 {
                undeducible.push(uuid);
                continue;
            }
            key
        } else {
            topic_key
        };
        conn.execute(
            "UPDATE memories SET title = ?1, topic_key = ?2 WHERE id = ?3",
            rusqlite::params![title, new_key, id],
        )
        .map_err(|e| MnexisError::busy(format!("write the title: {e}")))?;
        derived += 1;
    }
    Ok((derived, undeducible))
}

/// `doctor repair --plan` (also the default): what would be repaired, what
/// cannot be fixed, and the current state. Reads only — writes nothing.
pub fn plan_repair(conn: &Connection) -> (RepairReport, DoctorReport) {
    let report = run_checks(conn);
    let (actions, skipped) = plan_actions(conn, &report);
    let repair = RepairReport {
        mode: RepairMode::Plan,
        actions,
        skipped,
        fts_recreated: false,
        triggers_recreated: false,
        reindexed: 0,
        wal_reactivated: false,
        titles_derived: 0,
        titles_undeducible: Vec::new(),
        healthy_after: report.is_healthy(),
    };
    (repair, report)
}

/// `doctor repair --apply`: perform the planned actions and re-run every
/// check. Only what is broken is touched — a healthy store is a no-op. The
/// FTS rebuild is one atomic unit of work (its own savepoint), so a repair
/// that fails leaves the profile as it was.
pub fn apply_repair(conn: &Connection) -> Result<(RepairReport, DoctorReport), MnexisError> {
    let before = run_checks(conn);
    let (actions, skipped) = plan_actions(conn, &before);
    let rebuild = actions.iter().any(|a| a.action == RepairAction::RebuildFts);
    let wal = actions
        .iter()
        .any(|a| a.action == RepairAction::ReactivateWal);
    let titles = actions
        .iter()
        .any(|a| a.action == RepairAction::DeriveTitles);

    let mut reindexed = 0;
    if rebuild {
        reindexed = rebuild_memories_fts(conn)?;
    }
    let mut wal_reactivated = false;
    if wal {
        conn.execute_batch("PRAGMA journal_mode=WAL")
            .map_err(|e| MnexisError::busy(format!("reactivate WAL: {e}")))?;
        wal_reactivated = true;
    }
    let mut titles_derived = 0;
    let mut titles_undeducible: Vec<String> = Vec::new();
    if titles {
        // The writes are one unit of work: a repair that fails leaves the
        // profile as it was.
        conn.execute_batch("SAVEPOINT doctor_repair_titles")
            .map_err(|e| MnexisError::busy(format!("title savepoint: {e}")))?;
        match derive_missing_titles(conn) {
            Ok((derived, undeducible)) => {
                titles_derived = derived;
                titles_undeducible = undeducible;
                conn.execute_batch("RELEASE doctor_repair_titles")
                    .map_err(|e| MnexisError::busy(format!("title savepoint: {e}")))?;
            }
            Err(error) => {
                let _ = conn.execute_batch(
                    "ROLLBACK TO doctor_repair_titles; RELEASE doctor_repair_titles",
                );
                return Err(error);
            }
        }
    }

    let after = run_checks(conn);
    let repair = RepairReport {
        mode: RepairMode::Apply,
        actions,
        skipped,
        fts_recreated: rebuild,
        triggers_recreated: rebuild,
        reindexed,
        wal_reactivated,
        titles_derived,
        titles_undeducible,
        healthy_after: after.is_healthy(),
    };
    Ok((repair, after))
}

/// `doctor repair --dry-run`: the FTS rebuild really runs inside a
/// savepoint that is rolled back — the counts are measured, the store is
/// untouched. The WAL pragma cannot run inside a transaction, so it is
/// planned and never executed here; `--apply` runs it.
pub fn dry_run_repair(conn: &Connection) -> Result<(RepairReport, DoctorReport), MnexisError> {
    let before = run_checks(conn);
    let (actions, skipped) = plan_actions(conn, &before);
    let rebuild = actions.iter().any(|a| a.action == RepairAction::RebuildFts);
    let titles = actions
        .iter()
        .any(|a| a.action == RepairAction::DeriveTitles);

    let mut reindexed = 0;
    let mut titles_derived = 0;
    let mut titles_undeducible: Vec<String> = Vec::new();
    if rebuild || titles {
        conn.execute_batch("SAVEPOINT doctor_repair_dry_run")
            .map_err(|e| MnexisError::busy(format!("dry-run savepoint: {e}")))?;
        let mut failure: Option<MnexisError> = None;
        if rebuild {
            match rebuild_memories_fts(conn) {
                Ok(count) => reindexed = count,
                Err(error) => failure = Some(error),
            }
        }
        if failure.is_none() && titles {
            match derive_missing_titles(conn) {
                Ok((derived, undeducible)) => {
                    titles_derived = derived;
                    titles_undeducible = undeducible;
                }
                Err(error) => failure = Some(error),
            }
        }
        if let Some(error) = failure {
            // Undo whatever the attempt did, then report the failure.
            let _ = conn
                .execute_batch("ROLLBACK TO doctor_repair_dry_run; RELEASE doctor_repair_dry_run");
            return Err(error);
        }
        conn.execute_batch("ROLLBACK TO doctor_repair_dry_run; RELEASE doctor_repair_dry_run")
            .map_err(|e| MnexisError::busy(format!("dry-run rollback: {e}")))?;
    }

    // Post-rollback: the original state, by construction.
    let after = run_checks(conn);
    let repair = RepairReport {
        mode: RepairMode::DryRun,
        actions,
        skipped,
        fts_recreated: false,
        triggers_recreated: false,
        reindexed,
        wal_reactivated: false,
        titles_derived,
        titles_undeducible,
        healthy_after: after.is_healthy(),
    };
    Ok((repair, after))
}
