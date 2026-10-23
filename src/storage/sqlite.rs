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

/// Adds the persisted supersede `reason` to `memory_links` (supersede-contract
/// S2). Existing rows keep the empty default; there is no backfill.
const MIGRATION_003: &str = r#"
ALTER TABLE memory_links ADD COLUMN reason TEXT NOT NULL DEFAULT '';
"#;

/// Migration 004: the sessions and the user prompts.
///
/// The two things a repository cannot hold. A **session** is the unit of
/// work the agent runs in: the caller names it (the agent never invents
/// one), it belongs to a namespace, and it ends with a summary. A **user
/// prompt** is what the user said: stored, searchable, and attached to the
/// session it happened in when there is one.
///
/// The migration is additive: new tables, a new index and new triggers. No
/// existing table is touched, so a profile that already holds memories
/// keeps every one of them.
///
/// The prompt table carries no foreign key on the session: a prompt can be
/// captured before its session is registered, and a prompt that names a
/// session the engine never saw is a fact about the past, not an error.
const MIGRATION_004: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    namespace   TEXT NOT NULL,
    directory   TEXT NOT NULL DEFAULT '',
    started_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    ended_at    TEXT,
    summary     TEXT
);

CREATE TABLE IF NOT EXISTS user_prompts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT,
    namespace   TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_user_prompts_session
    ON user_prompts(session_id);

CREATE INDEX IF NOT EXISTS idx_user_prompts_namespace
    ON user_prompts(namespace);

CREATE INDEX IF NOT EXISTS idx_user_prompts_created
    ON user_prompts(created_at DESC);

CREATE VIRTUAL TABLE IF NOT EXISTS user_prompts_fts USING fts5(content);

CREATE TRIGGER IF NOT EXISTS user_prompts_after_insert
AFTER INSERT ON user_prompts
BEGIN
    INSERT INTO user_prompts_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS user_prompts_after_delete
AFTER DELETE ON user_prompts
BEGIN
    DELETE FROM user_prompts_fts WHERE rowid = old.id;
END;
"#;

/// Migration 005: a title for every memory.
///
/// The title is the short, stable label of a fact — the thing that stays put
/// when the value inside the content changes. It is what the topic key
/// derives from, so a fact keeps one record across its revisions.
///
/// The migration is additive: one column with an empty default, so every
/// existing memory reads back with no title and every existing query works.
const MIGRATION_005: &str = r#"
ALTER TABLE memories ADD COLUMN title TEXT NOT NULL DEFAULT '';
"#;

/// The columns the full-text index carries, in declaration order — which is
/// also the order `bm25` reads its weights in.
///
/// The index keeps the **identity** of a fact next to its content: a caller
/// remembers `puerto del servicio` when the content has moved on to
/// `port = 8081`, so the words of the title and of the topic key have to be
/// searchable exactly like the words of the content.
pub const MEMORIES_FTS_COLUMNS: [&str; 3] = ["content", "title", "topic_key"];

/// The one definition of `memories_fts`: the table and the five triggers that
/// keep it in step with `memories`.
///
/// Migration 006, [`rebuild_memories_fts`] and therefore
/// `retrieval::reindex::reindex_all` and `doctor --repair` all build the index
/// from this constant, so the migrated index and the rebuilt one cannot drift
/// apart. The batch is safe to apply to any state of the table and of the
/// triggers: the table is dropped and re-created, every trigger is dropped and
/// re-created, and the caller re-indexes the canonical rows afterwards.
///
/// Column order is the order a weighted `bm25(memories_fts, ...)` would read,
/// in case weights ever become necessary. Today bm25 is left unweighted on
/// purpose: `retrieval::fts5::search` and `retrieval::explain` both score with
/// the bare `bm25(memories_fts)` expression, and that is public behaviour.
const MEMORIES_FTS_SCHEMA: &str = r#"
DROP TABLE IF EXISTS memories_fts;
CREATE VIRTUAL TABLE memories_fts USING fts5(content, title, topic_key);

-- Insert trigger: index into FTS5 only when the new memory is active and not
-- deleted (maintains the invariant state='active' AND deleted=0 ⇔ row in FTS5).
DROP TRIGGER IF EXISTS memories_after_insert_content;
CREATE TRIGGER memories_after_insert_content
AFTER INSERT ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    INSERT INTO memories_fts(rowid, content, title, topic_key)
    VALUES (new.id, new.content, new.title, new.topic_key);
END;

DROP TRIGGER IF EXISTS memories_after_update_into_active;
CREATE TRIGGER memories_after_update_into_active
AFTER UPDATE OF state, deleted ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
    INSERT INTO memories_fts(rowid, content, title, topic_key)
    VALUES (new.id, new.content, new.title, new.topic_key);
END;

DROP TRIGGER IF EXISTS memories_after_update_out_of_active;
CREATE TRIGGER memories_after_update_out_of_active
AFTER UPDATE OF state, deleted ON memories
WHEN NOT (new.state = 'active' AND new.deleted = 0)
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
END;

-- The indexed text of a memory is three columns now, so a title or a topic key
-- change re-indexes the row exactly like a content change does. The body
-- updates an existing row instead of inserting one, so a memory that is not
-- alive stays out of the index.
DROP TRIGGER IF EXISTS memories_after_update_content;
CREATE TRIGGER memories_after_update_content
AFTER UPDATE OF content, title, topic_key ON memories
WHEN new.deleted = 0
BEGIN
    UPDATE memories_fts
    SET content = new.content, title = new.title, topic_key = new.topic_key
    WHERE rowid = new.id;
END;

DROP TRIGGER IF EXISTS memories_after_delete;
CREATE TRIGGER memories_after_delete
AFTER DELETE ON memories
BEGIN
    DELETE FROM memories_fts WHERE rowid = old.id;
END;
"#;

/// Rebuild `memories_fts` from canonical data: the one implementation behind
/// migration 006, `retrieval::reindex::reindex_all` and `doctor --repair`.
///
/// Drops the index, rebuilds the table and the triggers from
/// [`MEMORIES_FTS_SCHEMA`], then re-indexes every `state = 'active' AND
/// deleted = 0` row with its content, title and topic key. Returns the number
/// of rows indexed.
///
/// The rebuild is **atomic**: it runs in a savepoint, so a failure leaves the
/// index exactly as it was before, and a caller that already opened a
/// transaction (`doctor --repair`) gets one unit of work instead of a nested
/// transaction.
pub fn rebuild_memories_fts(conn: &Connection) -> Result<usize, MnexisError> {
    conn.execute_batch("SAVEPOINT rebuild_memories_fts")
        .map_err(|e| MnexisError::busy(format!("reindex savepoint: {e}")))?;
    match rebuild_memories_fts_inner(conn) {
        Ok(indexed) => {
            conn.execute_batch("RELEASE rebuild_memories_fts")
                .map_err(|e| MnexisError::busy(format!("reindex release savepoint: {e}")))?;
            Ok(indexed)
        }
        Err(error) => {
            // Report the original failure, not the rollback's.
            let _ = conn
                .execute_batch("ROLLBACK TO rebuild_memories_fts; RELEASE rebuild_memories_fts");
            Err(error)
        }
    }
}

fn rebuild_memories_fts_inner(conn: &Connection) -> Result<usize, MnexisError> {
    conn.execute_batch(MEMORIES_FTS_SCHEMA)
        .map_err(|e| MnexisError::busy(format!("rebuild memories_fts: {e}")))?;
    conn.execute(
        "INSERT INTO memories_fts(rowid, content, title, topic_key) \
         SELECT id, content, title, topic_key FROM memories \
         WHERE state = 'active' AND deleted = 0",
        [],
    )
    .map_err(|e| MnexisError::busy(format!("rebuild memories_fts rows: {e}")))
}

/// The columns of [`MEMORIES_FTS_COLUMNS`] that `memories_fts` does not
/// carry. An empty result means the index has the current shape; `content`
/// among the missing means the index does not exist at all.
///
/// The probe asks the table what it can answer instead of reading its `CREATE`
/// statement, so it agrees with what a search can actually do. Migration 006
/// uses it to decide whether the rebuild has already happened (a half-applied
/// migration leaves the schema and the ledger out of sync — the schema is what
/// decides), and the doctor uses it to tell a pre-006 index from a good one.
pub fn missing_fts_columns(conn: &Connection) -> Result<Vec<&'static str>, MnexisError> {
    let mut missing = Vec::new();
    for column in MEMORIES_FTS_COLUMNS {
        match conn.prepare(&format!("SELECT {column} FROM memories_fts LIMIT 0")) {
            Ok(_) => {}
            Err(e) => {
                let message = e.to_string().to_ascii_lowercase();
                if message.contains("no such column") || message.contains("no such table") {
                    missing.push(column);
                } else {
                    return Err(MnexisError::busy(format!(
                        "probe memories_fts for {column}: {e}"
                    )));
                }
            }
        }
    }
    Ok(missing)
}

/// Applies pending migrations. Creates the `schema_migrations` ledger if
/// absent. Applies migrations 001 through 006 if not already recorded. A
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

    if applied > 6 {
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

    if applied < 3 {
        conn.execute_batch(MIGRATION_003)
            .map_err(|e| MnexisError::busy(format!("migration 003: {e}")))?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (3)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 003: {e}")))?;
    }

    if applied < 4 {
        conn.execute_batch(MIGRATION_004)
            .map_err(|e| MnexisError::busy(format!("migration 004: {e}")))?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (4)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 004: {e}")))?;
    }

    if applied < 5 {
        // `ALTER TABLE ... ADD COLUMN` cannot be made conditional in SQL, and
        // a migration that failed halfway would leave the column without its
        // ledger row. The column is what decides, so a re-run is safe.
        let has_title: i64 = conn
            .prepare("SELECT count(*) FROM pragma_table_info('memories') WHERE name = 'title'")
            .and_then(|mut stmt| stmt.query_row([], |r| r.get(0)))
            .map_err(|e| MnexisError::busy(format!("migration 005 (probe): {e}")))?;
        if has_title == 0 {
            conn.execute_batch(MIGRATION_005)
                .map_err(|e| MnexisError::busy(format!("migration 005: {e}")))?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (5)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 005: {e}")))?;
    }

    if applied < 6 {
        // Migration 006: the index knows the title and the topic key.
        //
        // `memories_fts` was `fts5(content)`, so a record whose content says
        // `port = 8081` could not be found by the words of its title. An FTS5
        // table cannot change its shape in place, so the migration rebuilds it.
        //
        // Probe before applying (migration 005's lesson): a half-applied
        // migration leaves the schema and the ledger out of sync, and the
        // shape of the index — not the ledger — is what decides whether the
        // rebuild still has to happen. The rebuild is atomic, so a crash can
        // no longer leave a new-shape index without its rows.
        let missing = missing_fts_columns(conn)
            .map_err(|e| MnexisError::busy(format!("migration 006 (probe): {e}")))?;
        if !missing.is_empty() {
            rebuild_memories_fts(conn)
                .map_err(|e| MnexisError::busy(format!("migration 006: {e}")))?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (6)", [])
            .map_err(|e| MnexisError::busy(format!("record migration 006: {e}")))?;
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
        assert_eq!(version, 6, "migrations 001 through 006 should be recorded");

        // Migration 003 added the persisted supersede reason column.
        let (col_type, dflt, notnull): (String, String, i64) = conn
            .query_row(
                "SELECT type, dflt_value, [notnull] FROM pragma_table_info('memory_links') \
                 WHERE name = 'reason'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("memory_links.reason exists after migration 003");
        assert_eq!(col_type.to_uppercase(), "TEXT");
        assert_eq!(dflt, "''", "existing rows read back an empty reason");
        assert_eq!(notnull, 1, "the column is NOT NULL with an empty default");

        // Calling run_migrations again on the same DB must be a no-op.
        run_migrations(&conn).unwrap();
        let version_again: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version_again, 6, "005 must not double-record on a re-run");
    }

    #[test]
    fn migration_004_adds_the_sessions_and_the_user_prompts() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig004.db");
        let conn = open_sqlite(&path).unwrap();
        run_migrations(&conn).unwrap();

        for table in ["sessions", "user_prompts", "user_prompts_fts"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} exists after migration 004");
        }

        // A prompt is indexed as it is stored, and the index follows it.
        conn.execute(
            "INSERT INTO sessions (id, namespace) VALUES ('s1', 'payments')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO user_prompts (session_id, namespace, content) \
             VALUES ('s1', 'payments', 'el puerto es 8081')",
            [],
        )
        .unwrap();
        let indexed: i64 = conn
            .query_row(
                "SELECT count(*) FROM user_prompts_fts WHERE user_prompts_fts MATCH 'puerto'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1, "the prompt is in the index");

        conn.execute("DELETE FROM user_prompts", []).unwrap();
        let left: i64 = conn
            .query_row("SELECT count(*) FROM user_prompts_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "the index follows the deletion");
    }

    #[test]
    fn migration_004_keeps_the_memories_of_an_existing_profile() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig004_old.db");
        let conn = open_sqlite(&path).unwrap();
        // A profile at version 3, with a memory in it.
        run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO memories (uuid, topic_key, namespace, memory_type, state, content, \
             actor, channel, assertion_basis) \
             VALUES ('uuid-1', '', 'payments', 'fact', 'active', 'un hecho', \
             'user', 'cli', 'user_stated')",
            [],
        )
        .unwrap();
        // The ledger says 4: the next run applies only what is missing.
        conn.execute("DELETE FROM schema_migrations WHERE version >= 4", [])
            .unwrap();
        run_migrations(&conn).unwrap();
        let memories: i64 = conn
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(memories, 1, "the memory survived the migration");
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 6, "the migration was applied");
    }

    #[test]
    fn migration_005_adds_a_title_that_defaults_to_empty() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig005.db");
        let conn = open_sqlite(&path).unwrap();
        run_migrations(&conn).unwrap();

        let (col_type, dflt, notnull): (String, String, i64) = conn
            .query_row(
                "SELECT type, dflt_value, [notnull] FROM pragma_table_info('memories') \
                 WHERE name = 'title'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("memories.title exists after migration 005");
        assert_eq!(col_type.to_uppercase(), "TEXT");
        assert_eq!(dflt, "''", "existing rows read back no title");
        assert_eq!(notnull, 1, "the column is NOT NULL with an empty default");

        // A memory that already exists keeps working and reads an empty title.
        conn.execute(
            "INSERT INTO memories (uuid, topic_key, namespace, memory_type, state, content, \
             actor, channel, assertion_basis) \
             VALUES ('u1', '', 'ns', 'fact', 'active', 'un hecho', 'user', 'cli', 'user_stated')",
            [],
        )
        .unwrap();
        let title: String = conn
            .query_row("SELECT title FROM memories WHERE uuid = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "", "an old memory has no title, not a broken one");
    }

    #[test]
    fn migration_005_is_safe_to_replay() {
        // A migration that failed halfway leaves the column without its
        // ledger row. Re-running must not fail on a duplicate column.
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig005_again.db");
        let conn = open_sqlite(&path).unwrap();
        run_migrations(&conn).unwrap();
        conn.execute("DELETE FROM schema_migrations WHERE version = 5", [])
            .unwrap();
        run_migrations(&conn).expect("the re-run must not fail on the column it already added");
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 6, "the ledger records it");
    }

    #[test]
    fn migration_006_indexes_the_title_and_the_topic_key() {
        let dir = tempdir_in("target").unwrap();
        let path = dir.path().join("mig006.db");
        let conn = open_sqlite(&path).unwrap();

        // The index has the three columns, in the order bm25 reads them.
        let columns: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('memories_fts')")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get(0)).unwrap();
            rows.collect::<Result<_, _>>().unwrap()
        };
        for column in MEMORIES_FTS_COLUMNS {
            assert!(
                columns.iter().any(|c| c == column),
                "memories_fts must carry '{column}' after migration 006: {columns:?}"
            );
        }

        conn.execute(
            "INSERT INTO memories (uuid, title, topic_key, namespace, memory_type, content, \
             state, actor, channel, assertion_basis) \
             VALUES ('u1', 'puerto del servicio', 'puerto-del-servicio', 'ns', 'fact', \
             'port = 8081', 'active', 'user', 'cli', 'user_stated')",
            [],
        )
        .unwrap();
        // The trigger indexes all three columns, so the title and the topic
        // key are searchable, not only the content.
        for term in ["puerto", "servicio"] {
            let hits: i64 = conn
                .query_row(
                    "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH ?1",
                    [term],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(hits, 1, "'{term}' must be in the index after migration 006");
        }
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
