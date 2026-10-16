//! Storage adapter: opens the SQLite database, applies the standard pragmas,
//! and runs pending migrations.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::domain::error::MnexisError;

/// Opens the canonical local database, applies the standard pragmas, and
/// runs pending migrations. Returns the configured connection.
///
/// The pragmas — carried from the T13/T14 evidence base (PRD §11) — are:
/// - `journal_mode = WAL` for concurrent readers with a single writer,
/// - `foreign_keys = ON` to enforce the `FOREIGN KEY` constraints,
/// - `busy_timeout = 2s` to wait briefly on `SQLITE_BUSY` before failing.
pub fn open_sqlite(path: &Path) -> Result<Connection, MnexisError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| MnexisError::busy(format!("open failed: {e}")))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| MnexisError::busy(format!("pragma journal_mode: {e}")))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| MnexisError::busy(format!("pragma foreign_keys: {e}")))?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|e| MnexisError::busy(format!("busy_timeout: {e}")))?;
    // Legacy probe table: kept for backward compat with the spike tests
    // (tests/sqlite_fts5.rs) that use it to verify FTS5 availability.
    // Safe to drop in a future migration once the spike tests are retired.
    conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS mnexis_fts_probe USING fts5(content);")
        .map_err(|e| MnexisError::busy(format!("create mnexis_fts_probe: {e}")))?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// Migration 001: the production schema baseline.
///
/// Creates all production tables (`memories`, `memory_history`, `memory_links`,
/// `memory_tags`, `idempotency_records`), the `memories_fts` FTS5 index, and
/// the two FTS5 maintenance triggers (`into_active` / `out_of_active`). Drops
/// the spike's legacy `mnexis_fts_probe` table. Every CREATE uses
/// `IF NOT EXISTS` so the migration is idempotent and the spike's
/// pre-existing `memories` table (same columns) is reused without recreation.
const MIGRATION_001: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid            TEXT    NOT NULL UNIQUE,
    topic_key       TEXT    NOT NULL DEFAULT '',
    revision        INTEGER NOT NULL DEFAULT 1,
    namespace       TEXT    NOT NULL,
    memory_type     TEXT    NOT NULL DEFAULT 'fact',
    state           TEXT    NOT NULL
                    CHECK (state IN ('proposed','active','disputed','superseded','archived')),
    content         TEXT    NOT NULL,
    actor           TEXT    NOT NULL,
    channel         TEXT    NOT NULL,
    assertion_basis TEXT    NOT NULL
                    CHECK (assertion_basis IN ('user_stated','observed','inferred','imported')),
    valid_from      TEXT,
    valid_until     TEXT,
    deleted         INTEGER NOT NULL DEFAULT 0,
    deleted_at      TEXT,
    purge_after     TEXT,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

CREATE TABLE IF NOT EXISTS memory_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    revision        INTEGER NOT NULL,
    memory_type     TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    state           TEXT    NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    assertion_basis TEXT    NOT NULL,
    changed_by      TEXT    NOT NULL,
    change_reason   TEXT    NOT NULL,
    changed_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid)
);

CREATE TABLE IF NOT EXISTS memory_links (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    older_uuid      TEXT    NOT NULL,
    newer_uuid      TEXT    NOT NULL,
    link_type       TEXT    NOT NULL DEFAULT 'supersedes'
                    CHECK (link_type = 'supersedes'),
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (older_uuid, newer_uuid, link_type),
    FOREIGN KEY (older_uuid) REFERENCES memories(uuid),
    FOREIGN KEY (newer_uuid) REFERENCES memories(uuid)
);

CREATE TABLE IF NOT EXISTS memory_tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    tag             TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (memory_uuid, tag),
    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS idempotency_records (
    id_key          TEXT    NOT NULL,
    namespace       TEXT    NOT NULL,
    request_hash    TEXT    NOT NULL,
    result_uuid     TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    PRIMARY KEY (id_key, namespace)
);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(content);

-- Insert trigger: index into FTS5 only when the new memory is active and not
-- deleted (maintains the invariant state='active' AND deleted=0 ⇔ row in FTS5).
CREATE TRIGGER IF NOT EXISTS memories_after_insert_content
AFTER INSERT ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;

-- Partial unique index: topic_key must be unique per namespace when set
-- (SQLite requires this as a separate CREATE UNIQUE INDEX, not inline in CREATE TABLE).
CREATE UNIQUE INDEX IF NOT EXISTS memories_topic_key_unique
ON memories(namespace, topic_key) WHERE topic_key != '';

CREATE TRIGGER IF NOT EXISTS memories_after_update_into_active
AFTER UPDATE OF state, deleted ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_after_update_out_of_active
AFTER UPDATE OF state, deleted ON memories
WHEN NOT (new.state = 'active' AND new.deleted = 0)
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
END;
"#;

/// Migration 002: completes the FTS5 maintenance triggers.
///
/// Migration 001 created the `into_active` / `out_of_active` triggers
/// (covering state and deleted transitions) and the `after_insert_content`
/// trigger, but left two paths uncovered:
///
/// - `UPDATE OF content` (a pure content change with no state/deleted
///   toggle) leaves the FTS row out of sync.
/// - `DELETE FROM memories` (rare in production, exercised by purge) leaves
///   the FTS row behind.
///
/// Both gaps are closed here so the canonical-table ↔ FTS5 invariant holds
/// under every operation the application service can perform.
const MIGRATION_002: &str = r#"
CREATE TRIGGER IF NOT EXISTS memories_after_update_content
AFTER UPDATE OF content ON memories
WHEN new.deleted = 0
BEGIN
    UPDATE memories_fts SET content = new.content WHERE rowid = new.id;
END;

CREATE TRIGGER IF NOT EXISTS memories_after_delete
AFTER DELETE ON memories
BEGIN
    DELETE FROM memories_fts WHERE rowid = old.id;
END;
"#;

/// Applies pending migrations. Creates the `schema_migrations` ledger if
/// absent. Applies migration 001 and 002 if not already recorded. A
/// recorded version higher than the binary's known migrations returns
/// `MnexisError::UnsupportedVersion` (no-downgrade rule).
pub fn run_migrations(conn: &Connection) -> Result<(), MnexisError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
            version INTEGER PRIMARY KEY, \
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))\
        );",
    )
    .map_err(|e| MnexisError::busy(format!("create schema_migrations: {e}")))?;

    let applied: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(|e| MnexisError::busy(format!("read schema_migrations: {e}")))?;

    if applied > 2 {
        return Err(MnexisError::unsupported_version(applied));
    }

    if applied < 1 {
        conn.execute_batch(MIGRATION_001)
            .map_err(|e| MnexisError::busy(format!("migration 001: {e}")))?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (1)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 001: {e}")))?;
    }

    if applied < 2 {
        conn.execute_batch(MIGRATION_002)
            .map_err(|e| MnexisError::busy(format!("migration 002: {e}")))?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (2)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 002: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir_in;

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig.db");
        let conn = open_sqlite(&path).unwrap();
        run_migrations(&conn).unwrap();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 2, "both migrations 001 and 002 should be recorded");

        // Calling run_migrations again on the same DB must be a no-op.
        run_migrations(&conn).unwrap();
        let version_again: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version_again, 2, "002 must not double-record on a re-run");
    }

    #[test]
    fn migration_002_adds_update_of_content_and_after_delete_triggers() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("trig002.db");
        let conn = open_sqlite(&path).unwrap();

        for name in &["memories_after_update_content", "memories_after_delete"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                    [name],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "trigger {name} should exist after migration 002");
        }

        // Regression: a content-only UPDATE (no state/deleted change) must
        // refresh the FTS row via the new trigger.
        conn.execute(
            "INSERT INTO memories (uuid, topic_key, namespace, content, state, actor, channel, assertion_basis) \
             VALUES ('uuid-1', '', 'ns', 'graph algorithms', 'active', 'user', 'cli', 'user_stated')",
            [],
        )
        .unwrap();
        let before: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'graph'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, 1);

        conn.execute(
            "UPDATE memories SET content = ?1, revision = revision + 1 WHERE uuid = 'uuid-1'",
            ["forest algorithms"],
        )
        .unwrap();

        let hits_graph: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'graph'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            hits_graph, 0,
            "after UPDATE of content, the old term must disappear from FTS5 (trigger 002)"
        );
        let hits_forest: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'forest'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            hits_forest, 1,
            "the new content must be in FTS5 after UPDATE OF content (trigger 002)"
        );

        // DELETE must also clear the FTS row.
        conn.execute("DELETE FROM memories WHERE uuid = 'uuid-1'", [])
            .unwrap();
        let hits_after_delete: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'forest'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            hits_after_delete, 0,
            "after DELETE, the FTS row must be removed (trigger 002)"
        );
    }

    #[test]
    fn production_schema_tables_exist() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("schema.db");
        let conn = open_sqlite(&path).unwrap();
        for table in &[
            "memories",
            "memory_history",
            "memory_links",
            "memory_tags",
            "idempotency_records",
            "schema_migrations",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {table} should exist after migration");
        }
    }

    #[test]
    fn fts5_triggers_maintain_active_invariant() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("trig.db");
        let conn = open_sqlite(&path).unwrap();
        // Insert an active memory — should be in FTS5.
        conn.execute(
            "INSERT INTO memories (uuid, topic_key, namespace, content, state, actor, channel, assertion_basis) \
             VALUES ('uuid-1', '', 'ns', 'hello world', 'active', 'user', 'cli', 'user_stated')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "active memory should be in FTS5");
        // Transition to superseded — out_of_active fires — removed from FTS5.
        conn.execute(
            "UPDATE memories SET state='superseded' WHERE uuid='uuid-1'",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "superseded memory should not be in FTS5");
        // Restore to active — into_active fires — re-indexed.
        conn.execute("UPDATE memories SET state='active' WHERE uuid='uuid-1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "restored-to-active memory should be re-indexed in FTS5"
        );
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("unsp.db");
        let conn = open_sqlite(&path).unwrap();
        // Manually set a higher version to simulate a newer-binary-opened-older-DB.
        conn.execute("INSERT INTO schema_migrations (version) VALUES (99)", [])
            .unwrap();
        let err = run_migrations(&conn).unwrap_err();
        assert!(matches!(err, MnexisError::UnsupportedVersion { .. }));
    }
}
