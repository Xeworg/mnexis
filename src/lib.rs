//! Mnexis E2 — durable SQL core and CLI (in development).
//!
//! Module layout (PRD §3):
//! - `domain`: pure business types, no infrastructure dependencies.
//! - `storage`: SQLite adapter, migrations, `MemoryRepository` implementation.
//! - `retrieval`: FTS5 search module (T18 — currently the spike scaffold).
//! - `mcp`: MCP server over stdio.
//! - `corpus` / `retrieval` spike modules: retained until T18 retires them.
//!
//! The legacy `SpikeError` is removed; the public `open_sqlite` re-exports
//! the production one from `storage::sqlite` and returns `MnexisError`.

pub mod application;
pub mod cli;
pub mod config;
pub mod domain;
pub mod mcp;
pub mod retrieval;
pub mod storage;

pub use crate::domain::error::MnexisError;
pub use crate::storage::sqlite::open_sqlite;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir_in;

    #[test]
    fn open_sqlite_runs_migration_and_supports_fts5() {
        let directory = tempdir_in("target").unwrap();
        let path = directory.path().join("mnexis.db");
        {
            let connection = open_sqlite(&path).unwrap();
            connection
                .execute(
                    "INSERT INTO memories (uuid, topic_key, namespace, content, state, \
                     actor, channel, assertion_basis) \
                     VALUES ('uuid-1', '', 'ns', 'petgraph replaced by custom engine', \
                     'active', 'user', 'cli', 'user_stated')",
                    [],
                )
                .unwrap();
            let matches: i64 = connection
                .query_row(
                    "SELECT count(*) FROM memories_fts WHERE memories_fts MATCH 'petgraph'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(matches, 1);
        }
        let connection = open_sqlite(&path).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM memories WHERE uuid = 'uuid-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn independent_connections_serialize_writes_without_losing_committed_data() {
        let directory = tempdir_in("target").unwrap();
        let path = directory.path().join("shared.db");
        let first = open_sqlite(&path).unwrap();
        let second = open_sqlite(&path).unwrap();
        first
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS writes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
            )
            .unwrap();
        first
            .execute("INSERT INTO writes(body) VALUES ('from first')", [])
            .unwrap();
        second
            .execute("INSERT INTO writes(body) VALUES ('from second')", [])
            .unwrap();
        let count: i64 = first
            .query_row("SELECT count(*) FROM writes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
